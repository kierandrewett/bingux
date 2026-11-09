//! The position from `org.freedesktop.portal.Location` (GeoClue behind it).
//!
//! The call sequence is the portal's: `CreateSession`, then `Start` (its
//! `Request` answers with a response code, 0 when the person allowed it), then
//! a `LocationUpdated` signal on the Location interface. Both signals are
//! subscribed to before the call that causes them, so neither can be missed.
//!
//! The whole exchange runs on a thread of its own and is waited for with a
//! deadline: a portal that never answers costs a parked thread, not a hung
//! window.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::{Fix, Source};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const LOCATION: &str = "org.freedesktop.portal.Location";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
/// The accuracy asked for: 2 is a city, which is all a forecast needs and
/// the least a person has to give away.
const ACCURACY_CITY: u32 = 2;

/// Why the portal gave no position.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// No portal, no Location interface or no answer in time.
    Unavailable,
    /// The person, or the portal's policy, said no.
    Denied,
}

/// Asks the portal for the position, waiting at most `timeout`. Blocks.
pub(super) fn locate(timeout: Duration) -> Result<Fix, Failure> {
    let (send, receive) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("bingux-location-portal".into())
        .spawn(move || {
            let _ = send.send(exchange());
        });
    if spawned.is_err() {
        return Err(Failure::Unavailable);
    }
    receive
        .recv_timeout(timeout)
        .unwrap_or(Err(Failure::Unavailable))
}

fn exchange() -> Result<Fix, Failure> {
    let unavailable = |_| Failure::Unavailable;
    let connection = Connection::session().map_err(unavailable)?;
    let location = Proxy::new(&connection, DESTINATION, PATH, LOCATION).map_err(unavailable)?;
    // The interface's version: a quick failure on a system with no portal.
    let _version: u32 = location.get_property("version").map_err(unavailable)?;

    let sender = connection
        .unique_name()
        .map(|name| name.as_str().trim_start_matches(':').replace('.', "_"))
        .ok_or(Failure::Unavailable)?;
    let pid = std::process::id();
    let request_token = format!("bingux_request_{pid}");
    let request_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{request_token}");

    // Subscribe before the calls that cause the signals.
    let updates = location
        .receive_signal("LocationUpdated")
        .map_err(unavailable)?;
    let request =
        Proxy::new(&connection, DESTINATION, request_path, REQUEST).map_err(unavailable)?;
    let responses = request.receive_signal("Response").map_err(unavailable)?;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert(
        "session_handle_token",
        Value::from(format!("bingux_session_{pid}")),
    );
    options.insert("accuracy", Value::from(ACCURACY_CITY));
    let session: OwnedObjectPath = location
        .call("CreateSession", &(options,))
        .map_err(unavailable)?;

    let mut start: HashMap<&str, Value<'_>> = HashMap::new();
    start.insert("handle_token", Value::from(request_token));
    let started: Result<OwnedObjectPath, _> = location.call("Start", &(&session, "", start));
    let outcome = started.map_err(unavailable).and_then(|_| {
        // The first answer: 0 is "allowed". Then the first position.
        let mut responses = responses;
        let response = responses.next().ok_or(Failure::Unavailable)?;
        let body = response.body();
        let (code, _results): (u32, HashMap<String, OwnedValue>) =
            body.deserialize().map_err(unavailable)?;
        if code != 0 {
            return Err(Failure::Denied);
        }
        let mut updates = updates;
        let update = updates.next().ok_or(Failure::Unavailable)?;
        let body = update.body();
        let (_, fields): (OwnedObjectPath, HashMap<String, OwnedValue>) =
            body.deserialize().map_err(unavailable)?;
        fix_from(&fields).ok_or(Failure::Unavailable)
    });

    // Close the session whatever happened.
    if let Ok(session) = Proxy::new(&connection, DESTINATION, session.as_str(), SESSION) {
        let _ = session.call_method("Close", &());
    }
    outcome
}

/// The position in a `LocationUpdated` dictionary.
fn fix_from(fields: &HashMap<String, OwnedValue>) -> Option<Fix> {
    let number = |key: &str| fields.get(key).and_then(|value| f64::try_from(value).ok());
    let latitude = number("Latitude")?;
    let longitude = number("Longitude")?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    let description = fields
        .get("Description")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    Some(Fix {
        latitude,
        longitude,
        // The portal uses a negative accuracy for "unknown".
        accuracy: number("Accuracy").filter(|metres| *metres >= 0.0),
        name: description,
        region: None,
        country: None,
        timezone: None,
        source: Source::Portal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary(entries: &[(&str, Value<'_>)]) -> HashMap<String, OwnedValue> {
        entries
            .iter()
            .map(|(key, value)| {
                (
                    (*key).to_string(),
                    OwnedValue::try_from(value.clone()).expect("a plain value"),
                )
            })
            .collect()
    }

    #[test]
    fn a_location_update_becomes_a_fix() {
        let fix = fix_from(&dictionary(&[
            ("Latitude", Value::from(53.8)),
            ("Longitude", Value::from(-1.55)),
            ("Accuracy", Value::from(1200.0)),
            ("Description", Value::from("Leeds")),
        ]))
        .unwrap();
        assert_eq!(fix.source, Source::Portal);
        assert_eq!(fix.accuracy, Some(1200.0));
        assert_eq!(fix.name.as_deref(), Some("Leeds"));
    }

    #[test]
    fn an_update_without_coordinates_is_nothing() {
        assert!(fix_from(&dictionary(&[("Latitude", Value::from(1.0))])).is_none());
        assert!(
            fix_from(&dictionary(&[
                ("Latitude", Value::from(91.0)),
                ("Longitude", Value::from(0.0)),
            ]))
            .is_none()
        );
    }
}
