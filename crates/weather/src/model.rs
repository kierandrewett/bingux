//! What a forecast holds, in metric units: °C, km/h, mm, hPa and metres.
//!
//! Every time is wall-clock seconds at the place (see [`crate::time`]). A
//! value the API did not give is `None`, never zero.

use serde::{Deserialize, Serialize};

use crate::time::{DAY, HOUR};

/// Reads the detail as parts, or as the one joined string of older saves.
fn detail_parts<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Joined(String),
        Parts(Vec<String>),
    }
    Ok(match Saved::deserialize(deserializer)? {
        Saved::Joined(text) if text.is_empty() => Vec::new(),
        Saved::Joined(text) => vec![text],
        Saved::Parts(parts) => parts,
    })
}

/// A place to ask about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    /// The region and country, for telling two cities of one name apart.
    /// Parts, not a joined line: the app joins them in its own language. A
    /// place saved before this was a single string still loads.
    #[serde(default, deserialize_with = "detail_parts")]
    pub detail: Vec<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub timezone: Option<String>,
}

impl Place {
    pub fn new(name: impl Into<String>, latitude: f64, longitude: f64) -> Self {
        Self {
            name: name.into(),
            detail: Vec::new(),
            latitude,
            longitude,
            timezone: None,
        }
    }

    /// The identity of a place: its coordinates to about a kilometre, so the
    /// same city found twice is one place.
    pub fn key(&self) -> String {
        format!("{:.2},{:.2}", self.latitude, self.longitude)
    }

    /// `51.51° N, 0.13° W`.
    pub fn coordinates(&self) -> String {
        let ns = if self.latitude >= 0.0 { 'N' } else { 'S' };
        let ew = if self.longitude >= 0.0 { 'E' } else { 'W' };
        format!(
            "{:.2}° {ns}, {:.2}° {ew}",
            self.latitude.abs(),
            self.longitude.abs()
        )
    }
}

/// The conditions now.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Current {
    pub time: i64,
    pub temperature: Option<f64>,
    pub apparent: Option<f64>,
    pub humidity: Option<f64>,
    pub precipitation: Option<f64>,
    pub code: Option<u8>,
    pub is_day: bool,
    pub cloud_cover: Option<f64>,
    pub pressure: Option<f64>,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<f64>,
    pub wind_gusts: Option<f64>,
}

/// One hour of forecast.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hour {
    pub time: i64,
    pub temperature: Option<f64>,
    pub apparent: Option<f64>,
    pub humidity: Option<f64>,
    pub dew_point: Option<f64>,
    /// Chance of precipitation, 0 to 100.
    pub precip_chance: Option<f64>,
    pub precipitation: Option<f64>,
    pub code: Option<u8>,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<f64>,
    pub wind_gusts: Option<f64>,
    pub uv: Option<f64>,
    /// Metres.
    pub visibility: Option<f64>,
    pub pressure: Option<f64>,
    pub is_day: bool,
}

/// One day of forecast. `time` is local midnight.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    pub time: i64,
    pub code: Option<u8>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    pub uv_max: Option<f64>,
    pub precipitation: Option<f64>,
    pub precip_chance: Option<f64>,
    pub wind_max: Option<f64>,
    pub gusts_max: Option<f64>,
}

/// Rain or snow in a quarter of an hour, for the next hours.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Minute {
    pub time: i64,
    pub precipitation: Option<f64>,
}

/// The air quality now.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AirQuality {
    pub us_aqi: Option<f64>,
    pub european_aqi: Option<f64>,
    pub pm2_5: Option<f64>,
    pub pm10: Option<f64>,
    pub ozone: Option<f64>,
    pub nitrogen_dioxide: Option<f64>,
    pub sulphur_dioxide: Option<f64>,
    pub carbon_monoxide: Option<f64>,
}

/// Everything known about one place, as fetched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    /// When it was fetched, in Unix seconds.
    pub fetched_at: i64,
    /// Seconds the place's clock is ahead of UTC.
    pub utc_offset: i32,
    pub timezone: String,
    pub elevation: Option<f64>,
    pub current: Current,
    pub minutely: Vec<Minute>,
    /// Starts three hours before the current hour, so a trend can be read.
    pub hourly: Vec<Hour>,
    pub daily: Vec<Day>,
    pub air: Option<AirQuality>,
}

impl Forecast {
    /// The wall-clock time at the place at Unix time `now`.
    pub fn local(&self, now: i64) -> i64 {
        now + i64::from(self.utc_offset)
    }

    /// Hours from the one that holds `local` on.
    pub fn hours_from(&self, local: i64) -> &[Hour] {
        let start = self
            .hourly
            .iter()
            .position(|hour| hour.time + HOUR > local)
            .unwrap_or(self.hourly.len());
        &self.hourly[start..]
    }

    /// The hour that holds `local`.
    pub fn hour_at(&self, local: i64) -> Option<&Hour> {
        self.hours_from(local)
            .first()
            .filter(|hour| hour.time <= local)
    }

    /// The hour `back` hours before the one that holds `local`.
    pub fn hour_before(&self, local: i64, back: usize) -> Option<&Hour> {
        let index = self
            .hourly
            .iter()
            .position(|hour| hour.time + HOUR > local)?;
        self.hourly.get(index.checked_sub(back)?)
    }

    /// The forecast day that holds `local`.
    pub fn day_at(&self, local: i64) -> Option<&Day> {
        self.daily
            .iter()
            .find(|day| local >= day.time && local < day.time + DAY)
    }

    /// Days from today on.
    pub fn days_from(&self, local: i64) -> &[Day] {
        let start = self
            .daily
            .iter()
            .position(|day| day.time + DAY > local)
            .unwrap_or(self.daily.len());
        &self.daily[start..]
    }

    /// The lowest low and highest high across `days`, for a range bar.
    pub fn range(days: &[Day]) -> Option<(f64, f64)> {
        let low = days.iter().filter_map(|day| day.low).reduce(f64::min)?;
        let high = days.iter().filter_map(|day| day.high).reduce(f64::max)?;
        Some((low, high))
    }

    /// The UV index now: the current hour's.
    pub fn uv_now(&self, local: i64) -> Option<f64> {
        self.hour_at(local).and_then(|hour| hour.uv)
    }

    /// Dew point, visibility and the rest that the current block does not give
    /// are read from the hour that holds `local`.
    pub fn hour_now(&self, local: i64) -> Option<&Hour> {
        self.hour_at(local)
    }
}
