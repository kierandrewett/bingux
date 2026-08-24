use anyhow::Result;
use serde::Deserialize;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const OPEN_METEO_FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A background-refreshed cache of current weather conditions for one configured location.
pub struct WeatherProvider {
    snapshot: Arc<Mutex<Option<WeatherSnapshot>>>,
}

impl WeatherProvider {
    /// Starts refreshing the configured location without putting network I/O on the query path.
    pub fn start(config: Option<crate::config::WeatherConfig>) -> Option<Self> {
        let config = config?;
        let snapshot = Arc::new(Mutex::new(None));
        let thread_snapshot = Arc::clone(&snapshot);

        thread::Builder::new()
            .name("bingux-weather-cache".to_owned())
            .spawn(move || refresh_loop(config, thread_snapshot))
            .ok()?;

        Some(Self { snapshot })
    }

    /// Returns the cached current conditions for a weather-focused query.
    pub fn query(&self, query: &str) -> Option<crate::protocol::ProviderResult> {
        if !is_weather_query(query) {
            return None;
        }

        let snapshot = self.snapshot.lock().ok()?.as_ref().copied()?;
        Some(crate::protocol::ProviderResult {
            result_id: "current".to_owned(),
            kind: crate::protocol::ResultKind::Weather,
            title: "Weather".to_owned(),
            subtitle: format!(
                "{}: {:.1} C (feels like {:.1} C), wind {:.1} km/h",
                weather_description(snapshot.weather_code),
                snapshot.temperature_celsius,
                snapshot.apparent_temperature_celsius,
                snapshot.wind_speed_kmh,
            ),
            icon: "weather-clear".to_owned(),
            score: 0.65,
        })
    }
}

fn refresh_loop(
    config: crate::config::WeatherConfig,
    snapshot: Arc<Mutex<Option<WeatherSnapshot>>>,
) {
    loop {
        if let Ok(updated_snapshot) = fetch_weather(&config) {
            if let Ok(mut cached_snapshot) = snapshot.lock() {
                *cached_snapshot = Some(updated_snapshot);
            }
        }

        thread::sleep(Duration::from_secs(config.refresh_seconds));
    }
}

fn fetch_weather(config: &crate::config::WeatherConfig) -> Result<WeatherSnapshot> {
    let url = format!(
        "{OPEN_METEO_FORECAST_URL}?latitude={}&longitude={}&current=temperature_2m%2Capparent_temperature%2Cweather_code%2Cwind_speed_10m",
        config.latitude, config.longitude,
    );
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build(),
    );
    let mut response = agent.get(&url).call()?;
    let response: OpenMeteoResponse = response.body_mut().read_json()?;
    let current = response.current;

    if !current.temperature_2m.is_finite()
        || !current.apparent_temperature.is_finite()
        || !current.wind_speed_10m.is_finite()
    {
        anyhow::bail!("Open-Meteo returned a non-finite weather value");
    }

    Ok(WeatherSnapshot {
        temperature_celsius: current.temperature_2m,
        apparent_temperature_celsius: current.apparent_temperature,
        weather_code: current.weather_code,
        wind_speed_kmh: current.wind_speed_10m,
    })
}

fn is_weather_query(query: &str) -> bool {
    let query = query.trim_start();
    has_query_keyword(query, "weather") || has_query_keyword(query, "forecast")
}

fn has_query_keyword(query: &str, keyword: &str) -> bool {
    let Some(prefix) = query.get(..keyword.len()) else {
        return false;
    };

    if !prefix.eq_ignore_ascii_case(keyword) {
        return false;
    }

    query[keyword.len()..]
        .chars()
        .next()
        .is_none_or(|character| !character.is_ascii_alphanumeric())
}

fn weather_description(code: u8) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Rime fog",
        51 => "Light drizzle",
        53 => "Moderate drizzle",
        55 => "Dense drizzle",
        56 => "Light freezing drizzle",
        57 => "Dense freezing drizzle",
        61 => "Slight rain",
        63 => "Moderate rain",
        65 => "Heavy rain",
        66 => "Light freezing rain",
        67 => "Heavy freezing rain",
        71 => "Slight snowfall",
        73 => "Moderate snowfall",
        75 => "Heavy snowfall",
        77 => "Snow grains",
        80 => "Slight rain showers",
        81 => "Moderate rain showers",
        82 => "Heavy rain showers",
        85 => "Light snow showers",
        86 => "Heavy snow showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm with light hail",
        99 => "Thunderstorm with heavy hail",
        _ => "Unknown conditions",
    }
}

#[derive(Clone, Copy)]
struct WeatherSnapshot {
    temperature_celsius: f64,
    apparent_temperature_celsius: f64,
    weather_code: u8,
    wind_speed_kmh: f64,
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
}

#[derive(Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    apparent_temperature: f64,
    weather_code: u8,
    wind_speed_10m: f64,
}

#[cfg(test)]
mod tests {
    use super::{is_weather_query, weather_description};

    #[test]
    fn matches_clear_weather_queries() {
        assert!(is_weather_query("weather"));
        assert!(is_weather_query("  FORECAST tomorrow"));
        assert!(is_weather_query("weather?"));
    }

    #[test]
    fn rejects_queries_without_a_weather_keyword() {
        assert!(!is_weather_query("weathering report"));
        assert!(!is_weather_query("show the forecast"));
        assert!(!is_weather_query("temperature"));
    }

    #[test]
    fn maps_wmo_codes_to_stable_text() {
        assert_eq!(weather_description(0), "Clear sky");
        assert_eq!(weather_description(63), "Moderate rain");
        assert_eq!(weather_description(99), "Thunderstorm with heavy hail");
        assert_eq!(weather_description(42), "Unknown conditions");
    }
}
