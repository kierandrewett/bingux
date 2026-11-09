//! Telling a window that its appearance may have changed.
//!
//! [`watch`] listens where a change can come from and calls `on_change` with no
//! detail: the owner reads [`super::current`] again. Nothing polls; every
//! listener sleeps in the kernel until something happens.
//!
//! * the portal's `SettingChanged` signal, when a portal is there;
//! * `gsettings monitor`, only when there is no portal to ask (the legacy
//!   Mutter session);
//! * the folder of `bingux.toml`, when it exists.
//!
//! Dropping the [`Watch`] stops all three: a window that closes leaves no
//! thread, child process or bus connection behind.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::OwnedValue;

use super::{config, portal};

/// Whether the real listeners may start. A test process and the headless
/// snapshot build never start any: a test that exits without running
/// destructors would leave a child and a bus connection behind.
/// `BINGUX_APPEARANCE_MONITOR=1` forces them on and `=0` off.
fn enabled() -> bool {
    match std::env::var("BINGUX_APPEARANCE_MONITOR").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => !(cfg!(test) || cfg!(feature = "testing")) && crate::platform::bus::allowed(),
    }
}

/// What a listener calls when something changed.
type Notify = Arc<dyn Fn() + Send + Sync>;

/// The running listeners. See the module documentation.
pub struct Watch(Arc<Inner>);

struct Inner {
    stop: Arc<AtomicBool>,
    /// The portal listener's own connection, closed on drop to end its wait.
    connection: Arc<Mutex<Option<Connection>>>,
    /// The `gsettings monitor` child.
    child: Arc<Mutex<Option<std::process::Child>>>,
    /// The watch on the config folder.
    _folder: Option<bingux_files::watch::Watcher>,
}

impl Clone for Watch {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let connection = self
            .connection
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        let child = self
            .child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if connection.is_some() || child.is_some() {
            // Closing and reaping wait on the other side, so not on the UI thread.
            std::thread::spawn(move || {
                if let Some(connection) = connection {
                    let _ = connection.close();
                }
                if let Some(mut child) = child {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            });
        }
    }
}

/// Start listening. `on_change` runs on a listener's thread, possibly often
/// for one change: it should only schedule a read.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) -> Watch {
    let stop = Arc::new(AtomicBool::new(false));
    let connection: Arc<Mutex<Option<Connection>>> = Arc::default();
    let child: Arc<Mutex<Option<std::process::Child>>> = Arc::default();
    let on_change: Notify = Arc::new(on_change);
    let mut folder = None;
    if enabled() {
        if let Some(directory) = config::watched_directory() {
            let notify = Arc::clone(&on_change);
            folder = bingux_files::watch::Watcher::new(&directory, move || notify()).ok();
        }
        let (stop, connection, child, on_change) = (
            Arc::clone(&stop),
            Arc::clone(&connection),
            Arc::clone(&child),
            Arc::clone(&on_change),
        );
        std::thread::Builder::new()
            .name("bingux-appearance".into())
            .spawn(move || {
                if !listen_portal(&stop, &connection, &on_change) {
                    monitor_gsettings(&stop, &child, &on_change);
                }
            })
            .ok();
    }
    Watch(Arc::new(Inner {
        stop,
        connection,
        child,
        _folder: folder,
    }))
}

/// Listen for the portal's `SettingChanged` until the connection is closed.
/// `false` when there is no portal to listen to, so the caller falls back.
fn listen_portal(stop: &AtomicBool, slot: &Mutex<Option<Connection>>, on_change: &Notify) -> bool {
    let Ok(connection) = Connection::session() else {
        return false;
    };
    if !portal_exists(&connection) {
        return false;
    }
    let Ok(rule) = match_rule() else {
        return false;
    };
    let Ok(messages) = MessageIterator::for_match_rule(rule, &connection, None) else {
        return false;
    };
    {
        let mut held = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        if stop.load(Ordering::SeqCst) {
            let _ = connection.close();
            return true;
        }
        *held = Some(connection);
    }
    for message in messages.map_while(Result::ok) {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok((namespace, _key, _value)) =
            message.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if namespace == portal::APPEARANCE || namespace == portal::INTERFACE {
            on_change();
        }
    }
    true
}

fn match_rule() -> zbus::Result<zbus::MatchRule<'static>> {
    Ok(zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(portal::SETTINGS.destination)?
        .interface("org.freedesktop.portal.Settings")?
        .member("SettingChanged")?
        .path(portal::SETTINGS.path)?
        .build())
}

/// Whether the portal frontend is running or can be started by the bus.
fn portal_exists(connection: &Connection) -> bool {
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(connection) else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from(portal::SETTINGS.destination) else {
        return false;
    };
    bus.name_has_owner(name).unwrap_or(false)
        || bus.list_activatable_names().is_ok_and(|names| {
            names
                .iter()
                .any(|activatable| activatable.as_str() == portal::SETTINGS.destination)
        })
}

/// Follow `gsettings monitor org.gnome.desktop.interface` until stopped. A
/// missing `gsettings` ends this at once.
fn monitor_gsettings(
    stop: &AtomicBool,
    slot: &Mutex<Option<std::process::Child>>,
    on_change: &Notify,
) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let mut command = Command::new("gsettings");
    command
        .args(["monitor", portal::INTERFACE])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // The child dies with the thread that spawned it: that thread lives as
    // long as the child.
    crate::runtime::die_with_parent(&mut command);
    let Ok(mut child) = command.spawn() else {
        return;
    };
    let stdout = child.stdout.take();
    {
        let mut held = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        // The guard may already be gone: then nobody will kill the child.
        if stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        *held = Some(child);
    }
    if let Some(stdout) = stdout {
        for _line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            on_change();
        }
    }
    let child = slot
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take();
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_process_starts_no_listener() {
        if std::env::var_os("BINGUX_APPEARANCE_MONITOR").is_none() {
            assert!(!enabled());
        }
        // Dropping a watch that listens to nothing is harmless.
        drop(watch(|| {}));
    }
}
