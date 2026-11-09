//! Keeping the screen on and the machine awake while something is going on: a
//! video playing, a presentation, a long copy.
//!
//! ```ignore
//! let hold = inhibit::inhibit("Media", "Playing a video", What::IDLE)?;
//! // ... playing ...
//! drop(hold); // the screen may blank again
//! ```
//!
//! The hold lasts as long as the [`Inhibitor`] lives. A process that crashes
//! loses it with its connection to the bus, so a hold is never left behind.
//!
//! Three services can take a hold, asked in this order:
//!
//! 1. the portal, `org.freedesktop.portal.Inhibit`, which covers idle and
//!    suspend and is what a sandboxed application has;
//! 2. `org.freedesktop.ScreenSaver`, which the shell serves and which covers
//!    idle only;
//! 3. logind, `org.freedesktop.login1.Manager.Inhibit` on the system bus, which
//!    covers idle and suspend and hands back a file descriptor that is the hold.
//!
//! The first that takes everything asked for ends the search. One that takes
//! part of it (ScreenSaver cannot stop a suspend) is kept and the rest is asked
//! of the next. [`Inhibitor::covers`] says what the hold really does, so a
//! caller that needs suspend blocked can tell when it is not. Each step has its
//! own time limit; with no bus the call fails at once.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedFd, OwnedObjectPath, Value};

use super::bus::{self, BusError, Target, step};

const STEP: Duration = Duration::from_millis(1200);

const PORTAL: Target = Target {
    destination: "org.freedesktop.portal.Desktop",
    path: "/org/freedesktop/portal/desktop",
    interface: "org.freedesktop.portal.Inhibit",
};
const REQUEST: &str = "org.freedesktop.portal.Request";
const SCREENSAVER: Target = Target {
    destination: "org.freedesktop.ScreenSaver",
    path: "/org/freedesktop/ScreenSaver",
    interface: "org.freedesktop.ScreenSaver",
};
/// Some implementations serve it at the root instead.
const SCREENSAVER_ROOT: Target = Target {
    destination: "org.freedesktop.ScreenSaver",
    path: "/ScreenSaver",
    interface: "org.freedesktop.ScreenSaver",
};
const LOGIND: Target = Target {
    destination: "org.freedesktop.login1",
    path: "/org/freedesktop/login1",
    interface: "org.freedesktop.login1.Manager",
};

/// What a hold keeps from happening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct What {
    /// The screen blanking or locking because nobody touched the machine.
    pub idle: bool,
    /// The machine going to sleep.
    pub suspend: bool,
}

impl What {
    pub const NOTHING: What = What {
        idle: false,
        suspend: false,
    };
    pub const IDLE: What = What {
        idle: true,
        suspend: false,
    };
    pub const SUSPEND: What = What {
        idle: false,
        suspend: true,
    };
    pub const IDLE_AND_SUSPEND: What = What {
        idle: true,
        suspend: true,
    };

    fn is_nothing(self) -> bool {
        self == Self::NOTHING
    }

    /// What is asked for here that `done` does not cover.
    fn minus(self, done: What) -> What {
        What {
            idle: self.idle && !done.idle,
            suspend: self.suspend && !done.suspend,
        }
    }

    /// The portal's flags: suspend is 4, idle is 8.
    fn portal_flags(self) -> u32 {
        (if self.suspend { 4 } else { 0 }) | (if self.idle { 8 } else { 0 })
    }

    /// logind's `what`: `sleep` and `idle`, colon separated.
    fn logind_what(self) -> String {
        match (self.suspend, self.idle) {
            (true, true) => "sleep:idle",
            (true, false) => "sleep",
            _ => "idle",
        }
        .to_owned()
    }
}

/// The service that took a hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Portal,
    ScreenSaver,
    Logind,
}

/// A hold that is released when it is dropped.
pub struct Inhibitor {
    /// The session bus the portal and ScreenSaver holds were taken on; none
    /// when logind alone gave the hold.
    connection: Option<Connection>,
    parts: Vec<Part>,
    covers: What,
}

enum Part {
    /// The portal's request object, closed to release.
    Portal(String),
    /// A ScreenSaver cookie and the target that gave it.
    ScreenSaver(u32, Target),
    /// The descriptor is the hold; closing it releases.
    Logind(#[allow(dead_code)] OwnedFd),
}

impl Inhibitor {
    /// What the hold keeps from happening.
    pub fn covers(&self) -> What {
        self.covers
    }

    /// The services that took part of the hold, in the order they were asked.
    pub fn via(&self) -> Vec<Via> {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Portal(_) => Via::Portal,
                Part::ScreenSaver(..) => Via::ScreenSaver,
                Part::Logind(_) => Via::Logind,
            })
            .collect()
    }
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        let connection = self.connection.take();
        let parts = std::mem::take(&mut self.parts);
        // A hold that is only a descriptor is released by dropping it.
        let Some(connection) = connection else {
            return;
        };
        // Releasing is a call to the bus: not on the thread that dropped it.
        let release = move || {
            for part in parts {
                let _ = match part {
                    Part::Portal(path) => connection.call_method(
                        Some(PORTAL.destination),
                        path.as_str(),
                        Some(REQUEST),
                        "Close",
                        &(),
                    ),
                    Part::ScreenSaver(cookie, target) => connection.call_method(
                        Some(target.destination),
                        target.path,
                        Some(target.interface),
                        "UnInhibit",
                        &(cookie,),
                    ),
                    // Dropping the descriptor releases it.
                    Part::Logind(_) => continue,
                };
            }
        };
        if std::thread::Builder::new()
            .name("bingux-platform".into())
            .spawn(release)
            .is_err()
        {
            // No thread to release on: the connection closing releases them.
        }
    }
}

/// Take a hold for `application` doing `reason` (a short phrase the system may
/// show: "Playing a video"). Blocks; call it off the UI thread, or use
/// [`inhibit_then`].
pub fn inhibit(application: &str, reason: &str, what: What) -> Result<Inhibitor, BusError> {
    crate::runtime::assert_off_ui_thread("platform::inhibit::inhibit");
    if !bus::allowed() {
        return Err(BusError::NoBus);
    }
    // Without a session bus the portal and ScreenSaver cannot be asked, but
    // logind, on the system bus, still can.
    let session = bus::connect().ok();
    let system: SystemBus = Arc::new(Connection::system);
    acquire(session.as_ref(), &system, application, reason, what)
}

/// [`inhibit`] on the worker pool, calling `done` on the UI thread.
pub fn inhibit_then(
    application: String,
    reason: String,
    what: What,
    done: impl FnOnce(Result<Inhibitor, BusError>) + 'static,
) {
    crate::runtime::background_then(move || inhibit(&application, &reason, what), done);
}

/// How logind's system bus connection is made: the real one, or a test's.
pub(crate) type SystemBus = Arc<dyn Fn() -> zbus::Result<Connection> + Send + Sync>;

/// The search of the module documentation, on given connections.
pub(crate) fn acquire(
    session: Option<&Connection>,
    system: &SystemBus,
    application: &str,
    reason: &str,
    what: What,
) -> Result<Inhibitor, BusError> {
    if what.is_nothing() {
        return Err(BusError::Failed("Nothing to keep from happening.".into()));
    }
    let mut parts = Vec::new();
    let mut covers = What::NOTHING;
    let mut last = BusError::NoBus;

    if let Some(connection) = session {
        let options: HashMap<String, Value<'static>> =
            HashMap::from([("reason".to_owned(), Value::from(reason.to_owned()))]);
        let portal = step(connection, STEP, move |connection| {
            let handle: OwnedObjectPath = bus::call_on(
                connection,
                PORTAL,
                "Inhibit",
                &(String::new(), what.portal_flags(), options),
            )?;
            Ok(handle.as_str().to_owned())
        });
        match portal {
            Ok(path) => {
                return Ok(Inhibitor {
                    connection: Some(connection.clone()),
                    parts: vec![Part::Portal(path)],
                    covers: what,
                });
            }
            Err(error) => last = error,
        }

        if what.idle {
            for target in [SCREENSAVER, SCREENSAVER_ROOT] {
                let (application, reason) = (application.to_owned(), reason.to_owned());
                let cookie = step(connection, STEP, move |connection| {
                    bus::call_on::<_, u32>(connection, target, "Inhibit", &(application, reason))
                });
                match cookie {
                    Ok(cookie) => {
                        parts.push(Part::ScreenSaver(cookie, target));
                        covers.idle = true;
                        break;
                    }
                    Err(error) => last = error,
                }
            }
        }
    }

    let left = what.minus(covers);
    if !left.is_nothing() {
        let (application, reason) = (application.to_owned(), reason.to_owned());
        let system = Arc::clone(system);
        let held = bus::within(STEP, move || -> Result<OwnedFd, BusError> {
            let failed = |error: zbus::Error| BusError::Failed(error.to_string());
            let system = system().map_err(failed)?;
            bus::call_on::<_, OwnedFd>(
                &system,
                LOGIND,
                "Inhibit",
                &(left.logind_what(), application, reason, "block".to_owned()),
            )
            .map_err(failed)
        })
        .unwrap_or(Err(BusError::TimedOut));
        match held {
            Ok(descriptor) => {
                parts.push(Part::Logind(descriptor));
                covers = What {
                    idle: covers.idle || left.idle,
                    suspend: covers.suspend || left.suspend,
                };
            }
            Err(error) => last = error,
        }
    }

    if parts.is_empty() {
        return Err(last);
    }
    Ok(Inhibitor {
        connection: session.cloned(),
        parts,
        covers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portal_names_idle_and_suspend_by_flag() {
        assert_eq!(What::IDLE.portal_flags(), 8);
        assert_eq!(What::SUSPEND.portal_flags(), 4);
        assert_eq!(What::IDLE_AND_SUSPEND.portal_flags(), 12);
    }

    #[test]
    fn logind_names_them_by_word() {
        assert_eq!(What::IDLE.logind_what(), "idle");
        assert_eq!(What::SUSPEND.logind_what(), "sleep");
        assert_eq!(What::IDLE_AND_SUSPEND.logind_what(), "sleep:idle");
    }

    #[test]
    fn what_a_service_could_not_take_is_left_for_the_next() {
        assert_eq!(What::IDLE_AND_SUSPEND.minus(What::IDLE), What::SUSPEND);
        assert_eq!(What::IDLE.minus(What::IDLE), What::NOTHING);
        assert_eq!(What::SUSPEND.minus(What::IDLE), What::SUSPEND);
    }

    #[test]
    fn without_a_bus_a_hold_fails_without_waiting() {
        if std::env::var_os("BINGUX_PORTAL").is_some() {
            return;
        }
        let started = std::time::Instant::now();
        assert_eq!(
            inhibit("Test", "Testing", What::IDLE).err(),
            Some(BusError::NoBus)
        );
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    use crate::platform::testing::private_bus;
    use std::sync::Mutex;

    type Log = Arc<Mutex<Vec<String>>>;

    /// `org.freedesktop.ScreenSaver`.
    struct Saver(Log);

    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl Saver {
        fn inhibit(&self, application: String, reason: String) -> u32 {
            self.0
                .lock()
                .unwrap()
                .push(format!("saver inhibit {application}: {reason}"));
            42
        }

        fn un_inhibit(&self, cookie: u32) {
            self.0
                .lock()
                .unwrap()
                .push(format!("saver uninhibit {cookie}"));
        }
    }

    /// logind's manager: the hold is a descriptor.
    struct Logind(Log);

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Logind {
        fn inhibit(&self, what: String, _who: String, _why: String, mode: String) -> OwnedFd {
            self.0.lock().unwrap().push(format!("logind {what} {mode}"));
            OwnedFd::from(std::os::fd::OwnedFd::from(
                std::fs::File::open("/dev/null").expect("/dev/null"),
            ))
        }
    }

    /// The portal's Inhibit interface, and the request it hands out.
    struct Portal(Log);

    const REQUEST_PATH: &str = "/org/freedesktop/portal/desktop/request/test/1";

    #[zbus::interface(name = "org.freedesktop.portal.Inhibit")]
    impl Portal {
        fn inhibit(
            &self,
            _window: String,
            flags: u32,
            options: HashMap<String, zbus::zvariant::OwnedValue>,
        ) -> OwnedObjectPath {
            let reason = options
                .get("reason")
                .and_then(|value| value.downcast_ref::<&str>().ok())
                .unwrap_or("")
                .to_owned();
            self.0
                .lock()
                .unwrap()
                .push(format!("portal inhibit {flags} {reason}"));
            OwnedObjectPath::try_from(REQUEST_PATH).unwrap()
        }
    }

    struct Request(Log);

    #[zbus::interface(name = "org.freedesktop.portal.Request")]
    impl Request {
        fn close(&self) {
            self.0.lock().unwrap().push("portal close".into());
        }
    }

    fn system_of(connection: &Connection) -> SystemBus {
        let connection = connection.clone();
        Arc::new(move || Ok(connection.clone()))
    }

    fn log_has(log: &Log, line: &str) -> bool {
        for _ in 0..100 {
            if log.lock().unwrap().iter().any(|seen| seen == line) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    #[test]
    fn the_portal_takes_the_whole_hold_and_releases_it_on_drop() {
        let bus = private_bus!();
        let log = Log::default();
        let portal = bus.serve(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            Portal(Arc::clone(&log)),
        );
        portal
            .object_server()
            .at(REQUEST_PATH, Request(Arc::clone(&log)))
            .unwrap();
        let client = bus.client();
        let hold = acquire(
            Some(&client),
            &system_of(&client),
            "Media",
            "Playing a video",
            What::IDLE_AND_SUSPEND,
        )
        .expect("the portal takes it");
        assert_eq!(hold.via(), [Via::Portal]);
        assert_eq!(hold.covers(), What::IDLE_AND_SUSPEND);
        assert!(log_has(&log, "portal inhibit 12 Playing a video"));
        drop(hold);
        assert!(
            log_has(&log, "portal close"),
            "dropping the hold closes the request"
        );
    }

    #[test]
    fn with_no_portal_the_screensaver_takes_idle_and_logind_takes_suspend() {
        let bus = private_bus!();
        let log = Log::default();
        let _saver = bus.serve(
            "org.freedesktop.ScreenSaver",
            "/org/freedesktop/ScreenSaver",
            Saver(Arc::clone(&log)),
        );
        let _logind = bus.serve(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            Logind(Arc::clone(&log)),
        );
        let client = bus.client();
        let hold = acquire(
            Some(&client),
            &system_of(&client),
            "Media",
            "Playing",
            What::IDLE_AND_SUSPEND,
        )
        .expect("two services take it between them");
        assert_eq!(hold.via(), [Via::ScreenSaver, Via::Logind]);
        assert_eq!(hold.covers(), What::IDLE_AND_SUSPEND);
        assert!(log_has(&log, "saver inhibit Media: Playing"));
        assert!(
            log_has(&log, "logind sleep block"),
            "only the suspend is left for logind: the screensaver took the idle"
        );
        drop(hold);
        assert!(log_has(&log, "saver uninhibit 42"));
    }

    #[test]
    fn a_hold_that_is_only_part_of_what_was_asked_says_so() {
        let bus = private_bus!();
        let log = Log::default();
        let _saver = bus.serve(
            "org.freedesktop.ScreenSaver",
            "/org/freedesktop/ScreenSaver",
            Saver(Arc::clone(&log)),
        );
        let client = bus.client();
        let hold = acquire(
            Some(&client),
            &system_of(&client),
            "Media",
            "Playing",
            What::IDLE_AND_SUSPEND,
        )
        .expect("the screensaver takes the idle");
        assert_eq!(hold.via(), [Via::ScreenSaver]);
        assert_eq!(hold.covers(), What::IDLE, "nothing could stop a suspend");
    }

    #[test]
    fn with_no_service_the_hold_fails_with_the_buss_reason() {
        let bus = private_bus!();
        let client = bus.client();
        let started = std::time::Instant::now();
        let failed = acquire(
            Some(&client),
            &system_of(&client),
            "Media",
            "Playing",
            What::IDLE,
        )
        .err()
        .expect("nothing is there");
        assert!(matches!(failed, BusError::Failed(_)), "{failed:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn logind_alone_still_holds_when_there_is_no_session_bus() {
        let bus = private_bus!();
        let log = Log::default();
        let _logind = bus.serve(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            Logind(Arc::clone(&log)),
        );
        let system = bus.client();
        let hold = acquire(None, &system_of(&system), "Media", "Playing", What::IDLE)
            .expect("logind takes it");
        assert_eq!(hold.via(), [Via::Logind]);
        assert!(log_has(&log, "logind idle block"));
    }
}
