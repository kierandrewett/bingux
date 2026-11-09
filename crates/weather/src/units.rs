//! Units: which ones a person reads, chosen from their locale until they pick
//! their own, and how a value is written in them.
//!
//! The data is metric (see [`crate::model`]); only these functions convert.

use serde::{Deserialize, Serialize};

use crate::time::Clock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Temperature {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speed {
    #[default]
    KilometresPerHour,
    MilesPerHour,
    MetresPerSecond,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rain {
    #[default]
    Millimetres,
    Inches,
}

/// The units a person reads values in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Units {
    pub temperature: Temperature,
    pub speed: Speed,
    pub rain: Rain,
    #[serde(default)]
    pub clock: Clock,
}

/// Where Fahrenheit is the everyday scale.
const FAHRENHEIT: &[&str] = &["US", "BS", "BZ", "KY", "PW", "FM", "MH", "LR"];
/// Where distances and speeds are in miles, and rain in inches.
const INCHES: &[&str] = &["US", "LR", "MM"];
/// Where speeds are in miles per hour.
const MILES: &[&str] = &["US", "GB", "LR", "MM"];
/// Where the clock on the wall has twelve hours.
const TWELVE_HOUR: &[&str] = &["US", "CA", "AU", "NZ", "PH", "IN"];

impl Units {
    pub const METRIC: Units = Units {
        temperature: Temperature::Celsius,
        speed: Speed::KilometresPerHour,
        rain: Rain::Millimetres,
        clock: Clock::TwentyFour,
    };

    pub const IMPERIAL: Units = Units {
        temperature: Temperature::Fahrenheit,
        speed: Speed::MilesPerHour,
        rain: Rain::Inches,
        clock: Clock::Twelve,
    };

    /// The units of a two-letter region such as `GB`.
    pub fn of_region(region: &str) -> Units {
        let region = region.to_ascii_uppercase();
        let is = |list: &[&str]| list.contains(&region.as_str());
        Units {
            temperature: if is(FAHRENHEIT) {
                Temperature::Fahrenheit
            } else {
                Temperature::Celsius
            },
            speed: if is(MILES) {
                Speed::MilesPerHour
            } else {
                Speed::KilometresPerHour
            },
            rain: if is(INCHES) {
                Rain::Inches
            } else {
                Rain::Millimetres
            },
            clock: if is(TWELVE_HOUR) {
                Clock::Twelve
            } else {
                Clock::TwentyFour
            },
        }
    }

    /// The units of the locale the program runs in: `LC_ALL`, then
    /// `LC_MEASUREMENT`, then `LANG`. No locale, or `C`, is metric.
    pub fn auto() -> Units {
        let region = ["LC_ALL", "LC_MEASUREMENT", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty())
            .and_then(|locale| region_of(&locale));
        region.map_or(Units::METRIC, |region| Units::of_region(&region))
    }

    // ---- Numbers, converted ------------------------------------------------

    /// °C in the chosen scale.
    pub fn temperature_value(&self, celsius: f64) -> f64 {
        match self.temperature {
            Temperature::Celsius => celsius,
            Temperature::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }

    /// km/h in the chosen unit.
    pub fn speed_value(&self, kmh: f64) -> f64 {
        match self.speed {
            Speed::KilometresPerHour => kmh,
            Speed::MilesPerHour => kmh / 1.609_344,
            Speed::MetresPerSecond => kmh / 3.6,
        }
    }

    /// mm in the chosen unit.
    pub fn rain_value(&self, millimetres: f64) -> f64 {
        match self.rain {
            Rain::Millimetres => millimetres,
            Rain::Inches => millimetres / 25.4,
        }
    }

    // ---- Text --------------------------------------------------------------

    /// `21°`: no letter, as a temperature reads on a card.
    pub fn temperature(&self, celsius: f64) -> String {
        format!("{}°", self.temperature_value(celsius).round() as i64)
    }

    /// `21°C`.
    pub fn temperature_with_unit(&self, celsius: f64) -> String {
        format!("{}{}", self.temperature(celsius), self.temperature_letter())
    }

    pub fn temperature_letter(&self) -> &'static str {
        match self.temperature {
            Temperature::Celsius => "C",
            Temperature::Fahrenheit => "F",
        }
    }

    pub fn speed_unit(&self) -> &'static str {
        match self.speed {
            Speed::KilometresPerHour => "km/h",
            Speed::MilesPerHour => "mph",
            Speed::MetresPerSecond => "m/s",
        }
    }

    /// `12 km/h`.
    pub fn speed(&self, kmh: f64) -> String {
        let value = self.speed_value(kmh);
        match self.speed {
            Speed::MetresPerSecond => format!("{value:.1} {}", self.speed_unit()),
            _ => format!("{} {}", value.round() as i64, self.speed_unit()),
        }
    }

    pub fn rain_unit(&self) -> &'static str {
        match self.rain {
            Rain::Millimetres => "mm",
            Rain::Inches => "in",
        }
    }

    /// `3.7 mm`, `0.15 in`.
    pub fn rain(&self, millimetres: f64) -> String {
        let value = self.rain_value(millimetres);
        match self.rain {
            Rain::Millimetres if value >= 10.0 => format!("{} mm", value.round() as i64),
            Rain::Millimetres if value <= 0.0 => "0 mm".to_string(),
            Rain::Millimetres => format!("{value:.1} mm"),
            Rain::Inches if value <= 0.0 => "0 in".to_string(),
            Rain::Inches => format!("{value:.2} in"),
        }
    }

    /// `1013 hPa`, or `29.92 inHg` where rain is in inches.
    pub fn pressure(&self, hpa: f64) -> String {
        match self.rain {
            Rain::Millimetres => format!("{} hPa", hpa.round() as i64),
            Rain::Inches => format!("{:.2} inHg", hpa * 0.029_529_98),
        }
    }

    /// Visibility from metres: `10 km`, `800 m`, `6 mi`.
    pub fn distance(&self, metres: f64) -> String {
        match self.speed {
            Speed::MilesPerHour => {
                let miles = metres / 1_609.344;
                if miles >= 10.0 {
                    format!("{} mi", miles.round() as i64)
                } else {
                    format!("{} mi", trim(format!("{miles:.1}")))
                }
            }
            _ => {
                if metres < 1_000.0 {
                    format!("{} m", ((metres / 10.0).round() * 10.0) as i64)
                } else if metres >= 10_000.0 {
                    format!("{} km", (metres / 1_000.0).round() as i64)
                } else {
                    format!("{} km", trim(format!("{:.1}", metres / 1_000.0)))
                }
            }
        }
    }
}

/// `6.0` is `6`.
fn trim(text: String) -> String {
    match text.strip_suffix(".0") {
        Some(whole) => whole.to_string(),
        None => text,
    }
}

/// `GB` of `en_GB.UTF-8`; none for `C`, `POSIX` or a locale with no region.
fn region_of(locale: &str) -> Option<String> {
    let base = locale.split(['.', '@']).next()?;
    let (_, region) = base.split_once('_')?;
    (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()))
        .then(|| region.to_ascii_uppercase())
}

/// The language of the locale, such as `en` or `de`, for the city search.
/// `en` when there is none.
pub fn language() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .and_then(|locale| {
            let code = locale.split(['_', '.', '@']).next()?.to_ascii_lowercase();
            (code.len() == 2 && code.chars().all(|c| c.is_ascii_lowercase())).then_some(code)
        })
        .unwrap_or_else(|| "en".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_choose_units() {
        assert_eq!(Units::of_region("US"), Units::IMPERIAL);
        let gb = Units::of_region("gb");
        assert_eq!(gb.temperature, Temperature::Celsius);
        assert_eq!(gb.speed, Speed::MilesPerHour);
        assert_eq!(gb.rain, Rain::Millimetres);
        assert_eq!(Units::of_region("DE"), Units::METRIC);
    }

    #[test]
    fn locales_name_regions() {
        assert_eq!(region_of("en_US.UTF-8").as_deref(), Some("US"));
        assert_eq!(region_of("de_DE@euro").as_deref(), Some("DE"));
        assert_eq!(region_of("C.UTF-8"), None);
        assert_eq!(region_of("POSIX"), None);
    }

    #[test]
    fn values_convert_and_read() {
        let imperial = Units::IMPERIAL;
        assert_eq!(imperial.temperature(0.0), "32°");
        assert_eq!(imperial.temperature_with_unit(100.0), "212°F");
        assert_eq!(Units::METRIC.temperature(-0.4), "0°");
        assert_eq!(Units::METRIC.speed(12.4), "12 km/h");
        assert_eq!(imperial.speed(16.0934), "10 mph");
        let ms = Units {
            speed: Speed::MetresPerSecond,
            ..Units::METRIC
        };
        assert_eq!(ms.speed(36.0), "10.0 m/s");
        assert_eq!(Units::METRIC.rain(3.74), "3.7 mm");
        assert_eq!(Units::METRIC.rain(0.0), "0 mm");
        assert_eq!(imperial.rain(25.4), "1.00 in");
        assert_eq!(Units::METRIC.pressure(1013.2), "1013 hPa");
        assert_eq!(imperial.pressure(1013.25), "29.92 inHg");
        assert_eq!(Units::METRIC.distance(24_140.0), "24 km");
        assert_eq!(Units::METRIC.distance(3_100.0), "3.1 km");
        assert_eq!(Units::METRIC.distance(800.0), "800 m");
        assert_eq!(imperial.distance(9_656.0), "6 mi");
    }
}
