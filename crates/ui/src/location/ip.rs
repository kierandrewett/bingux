//! A coarse position from the network address, with a keyless service.
//!
//! The service sees the address of this computer, which is why the callers
//! show that the position is approximate and where it came from. Services are
//! tried in turn; the first that answers wins.

use std::time::Duration;

use serde_json::Value;

use super::http::{self, Failure};
use super::{Fix, Source};

/// An IP position is as good as the city: about 25 km.
const ACCURACY: f64 = 25_000.0;

const SERVICES: [(&str, fn(&str) -> Option<Fix>); 3] = [
    ("https://ipwho.is/", parse_ipwho),
    ("https://get.geojs.io/v1/ip/geo.json", parse_geojs),
    ("https://ipapi.co/json/", parse_ipapi),
];

/// Asks each service in turn. `Err(true)` is "offline": every service failed to
/// connect. `Err(false)` is "unavailable": some answered, none gave a position.
pub(super) fn locate(timeout: Duration) -> Result<Fix, bool> {
    let mut reached = false;
    for (url, parse) in SERVICES {
        match http::get(url, timeout) {
            Ok(body) => {
                reached = true;
                if let Some(fix) = parse(&body) {
                    return Ok(fix);
                }
            }
            Err(Failure::Status) => reached = true,
            Err(Failure::Offline) => {}
        }
    }
    Err(!reached)
}

/// A number, or a number written as text (geojs writes its coordinates so).
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn build(
    value: &Value,
    name_key: &str,
    region_key: &str,
    country_key: &str,
    timezone: Option<String>,
) -> Option<Fix> {
    let latitude = number(value.get("latitude")?)?;
    let longitude = number(value.get("longitude")?)?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    // (0, 0) is what a service gives for an address it cannot place.
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }
    Some(Fix {
        latitude,
        longitude,
        accuracy: Some(ACCURACY),
        name: text(value, name_key),
        region: text(value, region_key),
        country: text(value, country_key),
        timezone,
        source: Source::Network,
    })
}

pub(super) fn parse_ipwho(body: &str) -> Option<Fix> {
    let value: Value = serde_json::from_str(body).ok()?;
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let timezone = value
        .get("timezone")
        .and_then(|zone| zone.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    build(&value, "city", "region", "country", timezone)
}

pub(super) fn parse_geojs(body: &str) -> Option<Fix> {
    let value: Value = serde_json::from_str(body).ok()?;
    let timezone = text(&value, "timezone");
    build(&value, "city", "region", "country", timezone)
}

pub(super) fn parse_ipapi(body: &str) -> Option<Fix> {
    let value: Value = serde_json::from_str(body).ok()?;
    if value.get("error").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let timezone = text(&value, "timezone");
    build(&value, "city", "region", "country_name", timezone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipwho_answer() {
        let fix = parse_ipwho(
            r#"{"success":true,"city":"Leeds","region":"England","country":"United Kingdom",
                "latitude":53.8,"longitude":-1.55,"timezone":{"id":"Europe/London"}}"#,
        )
        .unwrap();
        assert_eq!(fix.name.as_deref(), Some("Leeds"));
        assert_eq!(fix.timezone.as_deref(), Some("Europe/London"));
        assert_eq!(fix.source, Source::Network);
        assert!(parse_ipwho(r#"{"success":false,"message":"Too many requests"}"#).is_none());
    }

    #[test]
    fn geojs_writes_coordinates_as_text() {
        let fix = parse_geojs(
            r#"{"city":"Reston","region":"Virginia","country":"United States",
                "latitude":"38.9653","longitude":"-77.338","timezone":"America/New_York"}"#,
        )
        .unwrap();
        assert!((fix.latitude - 38.9653).abs() < 1e-9);
        assert_eq!(fix.country.as_deref(), Some("United States"));
    }

    #[test]
    fn ipapi_answer_and_unplaceable_addresses() {
        let fix = parse_ipapi(
            r#"{"city":"Paris","region":"Île-de-France","country_name":"France",
                "latitude":48.85,"longitude":2.35,"timezone":"Europe/Paris"}"#,
        )
        .unwrap();
        assert_eq!(fix.name.as_deref(), Some("Paris"));
        assert!(parse_ipapi(r#"{"error":true,"reason":"RateLimited"}"#).is_none());
        assert!(parse_geojs(r#"{"latitude":"0","longitude":"0"}"#).is_none());
        assert!(parse_geojs("not json").is_none());
        assert!(parse_geojs(r#"{"latitude":"95","longitude":"0"}"#).is_none());
    }
}
