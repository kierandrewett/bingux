//! The name of a place from its coordinates, with OpenStreetMap's Nominatim.
//!
//! One request per position found, at most: Nominatim's policy asks for no more
//! than one a second and an identifying agent, which [`super::http`] sends.

use std::time::Duration;

use serde_json::Value;

use super::http;

const TIMEOUT: Duration = Duration::from_secs(6);

/// A place's name as the nearest town, region and country.
#[derive(Clone, Debug, PartialEq)]
pub struct Named {
    pub name: String,
    pub region: Option<String>,
    pub country: Option<String>,
}

/// The town at `latitude`, `longitude`, in `language`. Blocks.
pub(super) fn lookup(latitude: f64, longitude: f64, language: &str) -> Option<Named> {
    let url = format!(
        "https://nominatim.openstreetmap.org/reverse?format=jsonv2&zoom=10&lat={latitude:.4}&lon={longitude:.4}&accept-language={language}"
    );
    parse(&http::get(&url, TIMEOUT).ok()?)
}

pub(super) fn parse(body: &str) -> Option<Named> {
    let value: Value = serde_json::from_str(body).ok()?;
    let address = value.get("address")?;
    let field = |key: &str| {
        address
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let name = [
        "city",
        "town",
        "village",
        "municipality",
        "suburb",
        "hamlet",
        "county",
        "state",
    ]
    .iter()
    .find_map(|key| field(key))?;
    Some(Named {
        region: field("state").filter(|region| *region != name),
        country: field("country"),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_town_is_named_with_its_region() {
        let named = parse(
            r#"{"address":{"town":"Hebden Bridge","county":"Calderdale","state":"England","country":"United Kingdom"}}"#,
        )
        .unwrap();
        assert_eq!(named.name, "Hebden Bridge");
        assert_eq!(named.region.as_deref(), Some("England"));
        assert!(parse(r#"{"error":"Unable to geocode"}"#).is_none());
    }
}
