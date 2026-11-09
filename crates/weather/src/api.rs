//! The Open-Meteo client: forecast, air quality and city search.
//!
//! Open-Meteo needs no key. Its data is CC BY 4.0 and the free service is for
//! non-commercial use; the application names it on screen.
//!
//! Every call blocks. Run it on a worker.

use std::time::Duration;

use serde::Deserialize;

use crate::model::{AirQuality, Current, Day, Forecast, Hour, Minute, Place};
use crate::time::{parse_local, unix_now};

const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const AIR_URL: &str = "https://air-quality-api.open-meteo.com/v1/air-quality";
const GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
/// Who is asking, as the service asks to be told.
const AGENT: &str = "bingux-weather/0.1 (https://github.com/kierandrewett/bingux)";
const TIMEOUT: Duration = Duration::from_secs(12);

/// How many quarter hours of rain to ask for: six hours.
const MINUTELY_STEPS: u32 = 24;
/// Hours before the current one to ask for, to read a pressure trend.
const PAST_HOURS: u32 = 3;
const FORECAST_DAYS: u32 = 10;

/// Why a call failed.
#[derive(Debug)]
pub enum Error {
    /// The network, the name lookup or the server was not reachable in time.
    Offline(String),
    /// The server answered with an error status.
    Server(u16),
    /// The answer was not what the service documents.
    Unreadable(String),
}

impl Error {
    /// Whether the person is probably offline, as opposed to the service being
    /// down or changed.
    pub fn is_offline(&self) -> bool {
        matches!(self, Error::Offline(_))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Offline(detail) => write!(f, "network unreachable: {detail}"),
            Error::Server(status) => write!(f, "the service answered {status}"),
            Error::Unreadable(detail) => write!(f, "unreadable answer: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

fn from_ureq(error: ureq::Error) -> Error {
    match error {
        ureq::Error::StatusCode(status) => Error::Server(status),
        other => Error::Offline(other.to_string()),
    }
}

/// GET `url`, as text.
fn get(url: &str) -> Result<String, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", AGENT)
        .call()
        .map_err(from_ureq)?;
    response.body_mut().read_to_string().map_err(from_ureq)
}

/// Percent-encodes `text` for a query value.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// ---- The raw shapes -------------------------------------------------------

#[derive(Deserialize)]
struct RawForecast {
    #[serde(default)]
    utc_offset_seconds: i32,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    elevation: Option<f64>,
    current: Option<RawCurrent>,
    hourly: Option<RawHourly>,
    daily: Option<RawDaily>,
    minutely_15: Option<RawMinutely>,
}

#[derive(Deserialize)]
struct RawCurrent {
    time: String,
    temperature_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    relative_humidity_2m: Option<f64>,
    precipitation: Option<f64>,
    weather_code: Option<f64>,
    is_day: Option<f64>,
    cloud_cover: Option<f64>,
    pressure_msl: Option<f64>,
    wind_speed_10m: Option<f64>,
    wind_direction_10m: Option<f64>,
    wind_gusts_10m: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RawHourly {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    apparent_temperature: Vec<Option<f64>>,
    #[serde(default)]
    relative_humidity_2m: Vec<Option<f64>>,
    #[serde(default)]
    dew_point_2m: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
    #[serde(default)]
    weather_code: Vec<Option<f64>>,
    #[serde(default)]
    pressure_msl: Vec<Option<f64>>,
    #[serde(default)]
    visibility: Vec<Option<f64>>,
    #[serde(default)]
    wind_speed_10m: Vec<Option<f64>>,
    #[serde(default)]
    wind_direction_10m: Vec<Option<f64>>,
    #[serde(default)]
    wind_gusts_10m: Vec<Option<f64>>,
    #[serde(default)]
    uv_index: Vec<Option<f64>>,
    #[serde(default)]
    is_day: Vec<Option<f64>>,
}

#[derive(Deserialize, Default)]
struct RawDaily {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    weather_code: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    sunrise: Vec<Option<String>>,
    #[serde(default)]
    sunset: Vec<Option<String>>,
    #[serde(default)]
    uv_index_max: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_sum: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    wind_speed_10m_max: Vec<Option<f64>>,
    #[serde(default)]
    wind_gusts_10m_max: Vec<Option<f64>>,
}

#[derive(Deserialize, Default)]
struct RawMinutely {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
}

#[derive(Deserialize)]
struct RawAir {
    current: Option<RawAirCurrent>,
}

#[derive(Deserialize)]
struct RawAirCurrent {
    us_aqi: Option<f64>,
    european_aqi: Option<f64>,
    pm2_5: Option<f64>,
    pm10: Option<f64>,
    ozone: Option<f64>,
    nitrogen_dioxide: Option<f64>,
    sulphur_dioxide: Option<f64>,
    carbon_monoxide: Option<f64>,
}

#[derive(Deserialize)]
struct RawSearch {
    #[serde(default)]
    results: Vec<RawPlace>,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
}

fn at<T: Copy>(values: &[Option<T>], index: usize) -> Option<T> {
    values.get(index).copied().flatten()
}

// ---- Parsing ---------------------------------------------------------------

/// Turns the body of a forecast call into a [`Forecast`] fetched at `fetched_at`.
pub fn parse_forecast(body: &str, fetched_at: i64) -> Result<Forecast, Error> {
    let raw: RawForecast =
        serde_json::from_str(body).map_err(|error| Error::Unreadable(error.to_string()))?;
    let current = raw
        .current
        .ok_or_else(|| Error::Unreadable("no current conditions".into()))?;
    let time = parse_local(&current.time)
        .ok_or_else(|| Error::Unreadable("bad time of the current conditions".into()))?;
    let current = Current {
        time,
        temperature: current.temperature_2m,
        apparent: current.apparent_temperature,
        humidity: current.relative_humidity_2m,
        precipitation: current.precipitation,
        code: current.weather_code.map(|code| code as u8),
        is_day: current.is_day.is_none_or(|flag| flag >= 0.5),
        cloud_cover: current.cloud_cover,
        pressure: current.pressure_msl,
        wind_speed: current.wind_speed_10m,
        wind_direction: current.wind_direction_10m,
        wind_gusts: current.wind_gusts_10m,
    };

    let hourly = raw.hourly.unwrap_or_default();
    let hours = hourly
        .time
        .iter()
        .enumerate()
        .filter_map(|(index, text)| {
            Some(Hour {
                time: parse_local(text)?,
                temperature: at(&hourly.temperature_2m, index),
                apparent: at(&hourly.apparent_temperature, index),
                humidity: at(&hourly.relative_humidity_2m, index),
                dew_point: at(&hourly.dew_point_2m, index),
                precip_chance: at(&hourly.precipitation_probability, index),
                precipitation: at(&hourly.precipitation, index),
                code: at(&hourly.weather_code, index).map(|code| code as u8),
                wind_speed: at(&hourly.wind_speed_10m, index),
                wind_direction: at(&hourly.wind_direction_10m, index),
                wind_gusts: at(&hourly.wind_gusts_10m, index),
                uv: at(&hourly.uv_index, index),
                visibility: at(&hourly.visibility, index),
                pressure: at(&hourly.pressure_msl, index),
                is_day: at(&hourly.is_day, index).is_none_or(|flag| flag >= 0.5),
            })
        })
        .collect();

    let daily = raw.daily.unwrap_or_default();
    let days = daily
        .time
        .iter()
        .enumerate()
        .filter_map(|(index, text)| {
            let local = |values: &[Option<String>]| {
                values
                    .get(index)
                    .and_then(|value| value.as_deref())
                    .and_then(parse_local)
            };
            Some(Day {
                time: parse_local(text)?,
                code: at(&daily.weather_code, index).map(|code| code as u8),
                high: at(&daily.temperature_2m_max, index),
                low: at(&daily.temperature_2m_min, index),
                sunrise: local(&daily.sunrise),
                sunset: local(&daily.sunset),
                uv_max: at(&daily.uv_index_max, index),
                precipitation: at(&daily.precipitation_sum, index),
                precip_chance: at(&daily.precipitation_probability_max, index),
                wind_max: at(&daily.wind_speed_10m_max, index),
                gusts_max: at(&daily.wind_gusts_10m_max, index),
            })
        })
        .collect();

    let minutely = raw.minutely_15.unwrap_or_default();
    let minutes = minutely
        .time
        .iter()
        .enumerate()
        .filter_map(|(index, text)| {
            Some(Minute {
                time: parse_local(text)?,
                precipitation: at(&minutely.precipitation, index),
            })
        })
        .collect();

    Ok(Forecast {
        fetched_at,
        utc_offset: raw.utc_offset_seconds,
        timezone: raw.timezone.unwrap_or_default(),
        elevation: raw.elevation,
        current,
        minutely: minutes,
        hourly: hours,
        daily: days,
        air: None,
    })
}

/// Turns the body of an air quality call into an [`AirQuality`].
pub fn parse_air(body: &str) -> Result<AirQuality, Error> {
    let raw: RawAir =
        serde_json::from_str(body).map_err(|error| Error::Unreadable(error.to_string()))?;
    let current = raw
        .current
        .ok_or_else(|| Error::Unreadable("no air quality".into()))?;
    Ok(AirQuality {
        us_aqi: current.us_aqi,
        european_aqi: current.european_aqi,
        pm2_5: current.pm2_5,
        pm10: current.pm10,
        ozone: current.ozone,
        nitrogen_dioxide: current.nitrogen_dioxide,
        sulphur_dioxide: current.sulphur_dioxide,
        carbon_monoxide: current.carbon_monoxide,
    })
}

/// Turns the body of a city search into places.
pub fn parse_search(body: &str) -> Result<Vec<Place>, Error> {
    let raw: RawSearch =
        serde_json::from_str(body).map_err(|error| Error::Unreadable(error.to_string()))?;
    Ok(raw
        .results
        .into_iter()
        .map(|place| {
            let mut detail = Vec::new();
            // A region that repeats the city (Singapore, Singapore) says nothing.
            if let Some(region) = place.admin1.filter(|region| *region != place.name) {
                detail.push(region);
            }
            if let Some(country) = place.country {
                detail.push(country);
            }
            Place {
                name: place.name,
                detail,
                latitude: place.latitude,
                longitude: place.longitude,
                timezone: place.timezone,
            }
        })
        .collect())
}

// ---- Calls -----------------------------------------------------------------

fn forecast_url(place: &Place) -> String {
    format!(
        "{FORECAST_URL}?latitude={:.4}&longitude={:.4}&timezone=auto&forecast_days={FORECAST_DAYS}\
         &past_hours={PAST_HOURS}&forecast_minutely_15={MINUTELY_STEPS}\
         &current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m\
         &hourly=temperature_2m,apparent_temperature,relative_humidity_2m,dew_point_2m,precipitation_probability,precipitation,weather_code,pressure_msl,visibility,wind_speed_10m,wind_direction_10m,wind_gusts_10m,uv_index,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_sum,precipitation_probability_max,wind_speed_10m_max,wind_gusts_10m_max\
         &minutely_15=precipitation",
        place.latitude, place.longitude,
    )
}

fn air_url(place: &Place) -> String {
    format!(
        "{AIR_URL}?latitude={:.4}&longitude={:.4}&timezone=auto\
         &current=us_aqi,european_aqi,pm2_5,pm10,ozone,nitrogen_dioxide,sulphur_dioxide,carbon_monoxide",
        place.latitude, place.longitude,
    )
}

/// The forecast and the air quality of `place`, fetched together. A missing
/// air quality (the service has none for the place) leaves `air` empty and is
/// not an error; a missing forecast is.
pub fn fetch(place: &Place) -> Result<Forecast, Error> {
    let (forecast, air) = std::thread::scope(|scope| {
        let air = scope.spawn(|| get(&air_url(place)).and_then(|body| parse_air(&body)));
        let forecast = get(&forecast_url(place)).and_then(|body| parse_forecast(&body, unix_now()));
        (forecast, air.join().ok().and_then(Result::ok))
    });
    let mut forecast = forecast?;
    forecast.air = air;
    Ok(forecast)
}

/// Cities that match `query`, most populous first, in `language` (`en`).
pub fn search(query: &str, language: &str) -> Result<Vec<Place>, Error> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let url = format!(
        "{GEOCODING_URL}?name={}&count=10&language={}&format=json",
        encode(query),
        encode(language),
    );
    parse_search(&get(&url)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORECAST: &str = r#"{
        "utc_offset_seconds": 3600, "timezone": "Europe/Paris", "elevation": 35.0,
        "current": {"time": "2026-11-09T14:45", "temperature_2m": 8.5, "apparent_temperature": 6.0,
            "relative_humidity_2m": 80, "precipitation": 0.0, "weather_code": 3, "is_day": 1,
            "cloud_cover": 100, "pressure_msl": 1011.2, "wind_speed_10m": 10.0,
            "wind_direction_10m": 220, "wind_gusts_10m": 22.0},
        "hourly": {"time": ["2026-11-09T14:00", "2026-11-09T15:00"],
            "temperature_2m": [8.0, null], "weather_code": [3, 61], "is_day": [1, 1],
            "uv_index": [0.5, 0.2]},
        "daily": {"time": ["2026-11-09"], "weather_code": [61],
            "temperature_2m_max": [10.0], "temperature_2m_min": [4.0],
            "sunrise": ["2026-11-09T07:30"], "sunset": ["2026-11-09T17:05"]},
        "minutely_15": {"time": ["2026-11-09T14:45"], "precipitation": [0.1]}
    }"#;

    #[test]
    fn forecast_parses_columns_into_rows() {
        let forecast = parse_forecast(FORECAST, 100).unwrap();
        assert_eq!(forecast.utc_offset, 3600);
        assert_eq!(forecast.current.temperature, Some(8.5));
        assert_eq!(forecast.current.code, Some(3));
        assert!(forecast.current.is_day);
        assert_eq!(forecast.hourly.len(), 2);
        assert_eq!(forecast.hourly[1].temperature, None);
        assert_eq!(forecast.hourly[1].code, Some(61));
        assert_eq!(forecast.hourly[1].humidity, None);
        let day = &forecast.daily[0];
        assert_eq!(day.high, Some(10.0));
        assert!(day.sunrise.unwrap() < day.sunset.unwrap());
        assert_eq!(forecast.minutely[0].precipitation, Some(0.1));
        assert!(forecast.air.is_none());
    }

    #[test]
    fn forecast_without_current_is_unreadable() {
        assert!(matches!(parse_forecast("{}", 0), Err(Error::Unreadable(_))));
        assert!(parse_forecast("not json", 0).is_err());
    }

    #[test]
    fn search_builds_places_and_drops_a_repeated_region() {
        let places = parse_search(
            r#"{"results": [
                {"name": "London", "latitude": 51.5, "longitude": -0.12, "country": "United Kingdom", "admin1": "England", "timezone": "Europe/London"},
                {"name": "Singapore", "latitude": 1.29, "longitude": 103.85, "country": "Singapore", "admin1": "Singapore"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(places[0].detail, ["England", "United Kingdom"]);
        assert_eq!(places[1].detail, ["Singapore"]);
        assert!(parse_search("{}").unwrap().is_empty());
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("São Paulo"), "S%C3%A3o%20Paulo");
        assert_eq!(encode("a&b"), "a%26b");
    }
}
