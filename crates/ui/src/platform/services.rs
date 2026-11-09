//! Which of the desktop services the platform calls use are there, for a page
//! that shows them or a feature that hides its button when they are not.
//!
//! Nothing needs to ask first: every platform call degrades by itself. This is
//! for presenting the state, not for guarding a call.

use zbus::blocking::Connection;

use super::bus;

/// What the session offers. A field is `false` for a service that is neither
/// running nor startable by the bus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Services {
    /// There is a session bus this process may use.
    pub bus: bool,
    /// `org.freedesktop.portal.Desktop`: Settings, OpenURI, Inhibit.
    pub portal: bool,
    /// `org.freedesktop.Notifications`.
    pub notifications: bool,
    /// `org.freedesktop.ScreenSaver`.
    pub screensaver: bool,
    /// `org.freedesktop.login1` on the system bus: suspend holds.
    pub logind: bool,
}

/// Whether `name` has an owner on the bus `connection` is on, or the bus can
/// start a service for it.
fn offered(connection: &Connection, names: &[&'static str]) -> zbus::Result<Vec<bool>> {
    let proxy = zbus::blocking::fdo::DBusProxy::new(connection)?;
    let activatable = proxy.list_activatable_names().unwrap_or_default();
    names
        .iter()
        .map(|name| -> zbus::Result<bool> {
            let bus_name = zbus::names::BusName::try_from(*name)?;
            Ok(proxy.name_has_owner(bus_name).unwrap_or(false)
                || activatable
                    .iter()
                    .any(|candidate| candidate.as_str() == *name))
        })
        .collect()
}

/// Ask the buses which of the services are there. Blocks for at most a few
/// seconds; call it off the UI thread.
pub fn services() -> Services {
    crate::runtime::assert_off_ui_thread("platform::services");
    if !bus::allowed() {
        return Services::default();
    }
    let mut found = Services::default();
    if let Ok(session) = bus::connect() {
        found.bus = true;
        let asked = bus::step(&session, bus::ANSWER_WITHIN, |connection| {
            offered(
                connection,
                &[
                    "org.freedesktop.portal.Desktop",
                    "org.freedesktop.Notifications",
                    "org.freedesktop.ScreenSaver",
                ],
            )
        });
        if let Ok(answers) = asked {
            found.portal = answers[0];
            found.notifications = answers[1];
            found.screensaver = answers[2];
        }
    }
    let system = bus::within(bus::ANSWER_WITHIN, || {
        let connection = Connection::system().ok()?;
        offered(&connection, &["org.freedesktop.login1"]).ok()
    });
    found.logind = system.flatten().is_some_and(|answers| answers[0]);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::BusError;

    #[test]
    fn without_a_bus_nothing_is_offered() {
        if std::env::var_os("BINGUX_PORTAL").is_none() {
            assert_eq!(services(), Services::default());
        }
    }

    #[test]
    fn a_bus_error_names_its_cause() {
        assert_eq!(
            BusError::NoBus.to_string(),
            "The desktop services are not available."
        );
    }
}
