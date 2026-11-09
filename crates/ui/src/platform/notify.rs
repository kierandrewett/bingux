//! Notifications: `org.freedesktop.Notifications`.
//!
//! The shell (or any notification daemon) owns the service and draws the
//! banner; an application only describes what it wants said.
//!
//! ```ignore
//! let sent = notify::send(
//!     &Notification::new("Download finished")
//!         .body("report.pdf is in Downloads.")
//!         .icon("folder-download")
//!         .desktop_entry("bingux-files"),
//! )?;
//! ```
//!
//! With no daemon the call fails at once with the bus's own reason and the
//! application carries on: a notification is never the only place a result
//! is shown. [`send`] blocks for at most [`bus::ANSWER_WITHIN`](super::bus), so
//! call it off the UI thread, or use [`send_then`].

use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::Value;

use super::bus::{self, BusError, Target};

const SERVICE: Target = Target {
    destination: "org.freedesktop.Notifications",
    path: "/org/freedesktop/Notifications",
    interface: "org.freedesktop.Notifications",
};

/// How urgent a notification is. A daemon may keep a critical one on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

/// How long a notification stays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Expire {
    /// The daemon decides.
    #[default]
    Default,
    /// Until it is dismissed.
    Never,
    /// This long.
    After(Duration),
}

/// What an application asks to say. Build it with [`Notification::new`] and
/// the setters, then [`send`] it.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    summary: String,
    body: String,
    app_name: String,
    icon: String,
    urgency: Urgency,
    expire: Expire,
    replaces: Option<Sent>,
    desktop_entry: Option<String>,
    category: Option<String>,
    transient: bool,
    actions: Vec<(String, String)>,
}

/// A notification the daemon accepted. Keep it to replace or close the
/// notification later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sent(pub u32);

impl Notification {
    /// A notification with a one-line `summary`.
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            body: String::new(),
            app_name: String::new(),
            icon: String::new(),
            urgency: Urgency::Normal,
            expire: Expire::Default,
            replaces: None,
            desktop_entry: None,
            category: None,
            transient: false,
            actions: Vec::new(),
        }
    }

    /// The second line. Plain text; a daemon may show a little markup.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// The name shown as the sender. Empty lets the daemon use the
    /// application's desktop entry.
    pub fn app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    /// An icon name (from the icon theme) or an absolute file path.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn expire(mut self, expire: Expire) -> Self {
        self.expire = expire;
        self
    }

    /// Replace an earlier notification in place instead of adding another:
    /// a progress that moves, a count that changes.
    pub fn replaces(mut self, earlier: Sent) -> Self {
        self.replaces = Some(earlier);
        self
    }

    /// The application's desktop file id without `.desktop`
    /// (`"bingux-files"`): the daemon takes the name and icon from it and
    /// groups the application's notifications.
    pub fn desktop_entry(mut self, id: impl Into<String>) -> Self {
        self.desktop_entry = Some(id.into());
        self
    }

    /// A category such as `"transfer.complete"` or `"email.arrived"`.
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// A transient notification is not kept in the daemon's history.
    pub fn transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    /// A button. `id` is what the daemon reports back; `label` is its text.
    /// The id `"default"` is the action of clicking the notification itself.
    pub fn action(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push((id.into(), label.into()));
        self
    }

    /// The call's arguments, as the specification orders them.
    fn wire(&self) -> Wire {
        let mut hints: HashMap<String, Value<'static>> = HashMap::new();
        hints.insert(
            "urgency".into(),
            Value::from(match self.urgency {
                Urgency::Low => 0_u8,
                Urgency::Normal => 1,
                Urgency::Critical => 2,
            }),
        );
        if let Some(id) = &self.desktop_entry {
            hints.insert("desktop-entry".into(), Value::from(id.clone()));
        }
        if let Some(category) = &self.category {
            hints.insert("category".into(), Value::from(category.clone()));
        }
        if self.transient {
            hints.insert("transient".into(), Value::from(true));
        }
        (
            self.app_name.clone(),
            self.replaces.map_or(0, |sent| sent.0),
            self.icon.clone(),
            self.summary.clone(),
            self.body.clone(),
            self.actions
                .iter()
                .flat_map(|(id, label)| [id.clone(), label.clone()])
                .collect(),
            hints,
            match self.expire {
                Expire::Default => -1,
                Expire::Never => 0,
                Expire::After(time) => i32::try_from(time.as_millis().max(1)).unwrap_or(i32::MAX),
            },
        )
    }
}

/// `Notify`'s arguments: application name, replaced id, icon, summary, body,
/// actions as id and label pairs, hints, timeout in milliseconds.
type Wire = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, Value<'static>>,
    i32,
);

/// Show `notification`. Blocks; see the module documentation.
pub fn send(notification: &Notification) -> Result<Sent, BusError> {
    crate::runtime::assert_off_ui_thread("platform::notify::send");
    bus::call::<Wire, u32>(bus::ANSWER_WITHIN, SERVICE, "Notify", notification.wire()).map(Sent)
}

/// Close a notification that is still showing.
pub fn close(sent: Sent) -> Result<(), BusError> {
    crate::runtime::assert_off_ui_thread("platform::notify::close");
    bus::call_unit(bus::ANSWER_WITHIN, SERVICE, "CloseNotification", (sent.0,))
}

/// What the daemon can do: `"actions"`, `"body"`, `"body-markup"`,
/// `"persistence"`, and so on, as the specification names them.
pub fn capabilities() -> Result<Vec<String>, BusError> {
    crate::runtime::assert_off_ui_thread("platform::notify::capabilities");
    bus::call::<(), Vec<String>>(bus::ANSWER_WITHIN, SERVICE, "GetCapabilities", ())
}

/// [`send`] on the worker pool, calling `done` on the UI thread with the
/// result. For an event handler.
pub fn send_then(notification: Notification, done: impl FnOnce(Result<Sent, BusError>) + 'static) {
    crate::runtime::background_then(move || send(&notification), done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_call_is_laid_out_as_the_specification_orders_it() {
        let wire = Notification::new("Download finished")
            .body("report.pdf is in Downloads.")
            .app_name("Files")
            .icon("folder-download")
            .desktop_entry("bingux-files")
            .category("transfer.complete")
            .urgency(Urgency::Low)
            .transient(true)
            .action("default", "")
            .action("open", "Open Folder")
            .expire(Expire::After(Duration::from_secs(5)))
            .replaces(Sent(41))
            .wire();
        let (app, replaces, icon, summary, body, actions, hints, timeout) = wire;
        assert_eq!(app, "Files");
        assert_eq!(replaces, 41);
        assert_eq!(icon, "folder-download");
        assert_eq!(summary, "Download finished");
        assert_eq!(body, "report.pdf is in Downloads.");
        assert_eq!(actions, ["default", "", "open", "Open Folder"]);
        assert_eq!(timeout, 5000);
        assert_eq!(hints["urgency"], Value::from(0_u8));
        assert_eq!(
            hints["desktop-entry"],
            Value::from("bingux-files".to_owned())
        );
        assert_eq!(
            hints["category"],
            Value::from("transfer.complete".to_owned())
        );
        assert_eq!(hints["transient"], Value::from(true));
    }

    #[test]
    fn a_plain_notification_leaves_the_rest_to_the_daemon() {
        let (app, replaces, icon, _, body, actions, hints, timeout) =
            Notification::new("Saved").wire();
        assert_eq!(
            (app.as_str(), replaces, icon.as_str(), body.as_str()),
            ("", 0, "", "")
        );
        assert!(actions.is_empty());
        assert_eq!(timeout, -1, "the daemon decides how long");
        assert_eq!(hints.len(), 1, "only the urgency is stated");
        assert_eq!(hints["urgency"], Value::from(1_u8));
    }

    #[test]
    fn never_and_a_very_long_time_fit_the_wire() {
        let never = Notification::new("x").expire(Expire::Never).wire();
        assert_eq!(never.7, 0);
        let forever = Notification::new("x")
            .expire(Expire::After(Duration::from_secs(u64::MAX / 2)))
            .wire();
        assert_eq!(forever.7, i32::MAX);
    }

    #[test]
    fn without_a_bus_a_notification_fails_without_waiting() {
        if std::env::var_os("BINGUX_PORTAL").is_some() {
            return;
        }
        let started = std::time::Instant::now();
        assert_eq!(send(&Notification::new("x")), Err(BusError::NoBus));
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    /// A notification daemon that remembers what it was told.
    struct Daemon {
        seen: std::sync::Arc<std::sync::Mutex<Vec<(String, u32, String, Vec<String>, i32)>>>,
    }

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Daemon {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            app_name: String,
            replaces_id: u32,
            _app_icon: String,
            summary: String,
            _body: String,
            actions: Vec<String>,
            hints: HashMap<String, zbus::zvariant::OwnedValue>,
            expire_timeout: i32,
        ) -> u32 {
            let mut keys: Vec<String> = hints.keys().cloned().collect();
            keys.sort();
            let mut kept = actions;
            kept.extend(keys);
            self.seen
                .lock()
                .unwrap()
                .push((app_name, replaces_id, summary, kept, expire_timeout));
            7
        }

        fn get_capabilities(&self) -> Vec<String> {
            vec!["actions".into(), "body".into()]
        }
    }

    #[test]
    fn a_daemon_receives_what_was_asked_and_answers_with_an_id() {
        let bus = crate::platform::testing::private_bus!();
        let seen = std::sync::Arc::default();
        let _daemon = bus.serve(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Daemon {
                seen: std::sync::Arc::clone(&seen),
            },
        );
        let client = bus.client();
        let id: u32 = bus::call_on(
            &client,
            SERVICE,
            "Notify",
            &Notification::new("Saved")
                .app_name("Files")
                .desktop_entry("bingux-files")
                .action("open", "Open")
                .expire(Expire::Never)
                .wire(),
        )
        .expect("the daemon answers");
        assert_eq!(id, 7);
        let received = seen.lock().unwrap();
        assert_eq!(
            received[0],
            (
                "Files".to_owned(),
                0,
                "Saved".to_owned(),
                vec![
                    "open".into(),
                    "Open".into(),
                    "desktop-entry".into(),
                    "urgency".into()
                ],
                0
            )
        );
        let capabilities: Vec<String> =
            bus::call_on(&client, SERVICE, "GetCapabilities", &()).expect("the daemon answers");
        assert_eq!(capabilities, ["actions", "body"]);
    }
}
