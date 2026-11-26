//! A private session bus for tests that talk D-Bus for real.
//!
//! A test starts its own `dbus-daemon`, puts a fake service on it (a portal, a
//! notification daemon) and calls the platform code with a connection to it.
//! Nothing touches the person's session bus, and nothing is started for the
//! test: the daemon has no service directories, so a name nobody serves is an
//! error and not an installed program (`xdg-desktop-portal` and its backends
//! are what the stock session configuration would start on the first call to
//! `org.freedesktop.portal.Desktop`, and they would answer in the fake's
//! place). A machine without `dbus-daemon` skips these tests, saying so on
//! stderr.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

/// The daemon's whole configuration: a session bus on a socket in `{dir}`, any
/// client of the same user may talk to it and own any name, and there is no
/// `<servicedir>`, so no name can be started on demand. Not the stock
/// `session.conf`, which has `<standard_session_servicedirs/>`.
const CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir={dir}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A running `dbus-daemon`, stopped when dropped.
pub(crate) struct PrivateBus {
    child: Child,
    pub address: String,
}

impl PrivateBus {
    /// Start one, or `None` when there is no `dbus-daemon`.
    pub(crate) fn start() -> Option<Self> {
        let directory = std::env::temp_dir();
        let config = directory.join(format!(
            "bingux-test-bus-{}-{}.conf",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let escaped = directory
            .to_string_lossy()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        std::fs::write(&config, CONFIG.replace("{dir}", &escaped))
            .expect("the private bus's configuration, in the temporary directory");
        let spawned = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            let _ = std::fs::remove_file(&config);
            return None;
        };
        let mut address = String::new();
        BufReader::new(child.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        let address = address.trim().to_owned();
        if address.is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            // The daemon is installed and would not start: that is not a
            // machine without one, so the test must not be skipped for it.
            panic!(
                "dbus-daemon gave no address for the private bus; its configuration is {}",
                config.display()
            );
        }
        // The daemon has read it: nothing is left behind in the temporary directory.
        let _ = std::fs::remove_file(&config);
        Some(Self { child, address })
    }

    /// A plain client connection.
    pub(crate) fn client(&self) -> Connection {
        Builder::address(self.address.as_str())
            .expect("the daemon's address")
            .build()
            .expect("a connection to the private bus")
    }

    /// A connection that owns `name` and serves `interface` at `path`.
    pub(crate) fn serve<I: zbus::object_server::Interface>(
        &self,
        name: &str,
        path: &str,
        interface: I,
    ) -> Connection {
        Builder::address(self.address.as_str())
            .expect("the daemon's address")
            .name(name.to_owned())
            .expect("a bus name")
            .serve_at(path.to_owned(), interface)
            .expect("an object path")
            .build()
            .expect("a service on the private bus")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // A daemon that is killed leaves its socket behind.
        if let Some(socket) = self
            .address
            .strip_prefix("unix:path=")
            .and_then(|rest| rest.split(',').next())
        {
            let _ = std::fs::remove_file(socket);
        }
    }
}

/// Start a private bus or skip the test.
macro_rules! private_bus {
    () => {
        match $crate::platform::testing::PrivateBus::start() {
            Some(bus) => bus,
            None => {
                eprintln!("skipped: dbus-daemon is not installed");
                return;
            }
        }
    };
}
pub(crate) use private_bus;
