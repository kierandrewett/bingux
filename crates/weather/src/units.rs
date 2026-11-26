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

    // ---- Readings ----------------------------------------------------------
    //
    // A reading is a number and a unit, already rounded the way a card shows
    // it. The app writes it in the language shown (number, unit symbol and
    // their spacing come from the locale); the crate writes no text.

    /// `21°`: no letter, as a temperature reads on a card.
    pub fn temperature_reading(&self, celsius: f64) -> Reading {
        Reading::whole(self.temperature_value(celsius), Unit::Degree)
    }

    /// `21°C`.
    pub fn temperature_with_unit_reading(&self, celsius: f64) -> Reading {
        let unit = match self.temperature {
            Temperature::Celsius => Unit::Celsius,
            Temperature::Fahrenheit => Unit::Fahrenheit,
        };
        Reading::whole(self.temperature_value(celsius), unit)
    }

    /// `12 km/h`; one decimal in metres per second.
    pub fn speed_reading(&self, kmh: f64) -> Reading {
        let value = self.speed_value(kmh);
        match self.speed {
            Speed::MetresPerSecond => Reading::new(value, Unit::MetresPerSecond, 1, 1),
            Speed::KilometresPerHour => Reading::whole(value, Unit::KilometresPerHour),
            Speed::MilesPerHour => Reading::whole(value, Unit::MilesPerHour),
        }
    }

    /// `3.7 mm`, `0.15 in`, `0 mm`.
    pub fn rain_reading(&self, millimetres: f64) -> Reading {
        let value = self.rain_value(millimetres);
        match self.rain {
            Rain::Millimetres if value >= 10.0 => Reading::whole(value, Unit::Millimetre),
            Rain::Millimetres if value <= 0.0 => Reading::whole(0.0, Unit::Millimetre),
            Rain::Millimetres => Reading::new(value, Unit::Millimetre, 1, 1),
            Rain::Inches if value <= 0.0 => Reading::whole(0.0, Unit::Inch),
            Rain::Inches => Reading::new(value, Unit::Inch, 2, 2),
        }
    }

    /// `1013 hPa`, or `29.92 inHg` where rain is in inches.
    pub fn pressure_reading(&self, hpa: f64) -> Reading {
        match self.rain {
            Rain::Millimetres => Reading::whole(hpa, Unit::Hectopascal),
            Rain::Inches => Reading::new(hpa * 0.029_529_98, Unit::InchOfMercury, 2, 2),
        }
    }

    /// Visibility from metres: `10 km`, `800 m`, `6 mi`.
    pub fn distance_reading(&self, metres: f64) -> Reading {
        match self.speed {
            Speed::MilesPerHour => {
                let miles = metres / 1_609.344;
                if miles >= 10.0 {
                    Reading::whole(miles, Unit::Mile)
                } else {
                    Reading::new(miles, Unit::Mile, 0, 1)
                }
            }
            _ => {
                if metres < 1_000.0 {
                    Reading::whole((metres / 10.0).round() * 10.0, Unit::Metre)
                } else if metres >= 10_000.0 {
                    Reading::whole(metres / 1_000.0, Unit::Kilometre)
                } else {
                    Reading::new(metres / 1_000.0, Unit::Kilometre, 0, 1)
                }
            }
        }
    }
}

/// The unit of a [`Reading`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// An angle's degree sign alone: a temperature with no scale letter.
    Degree,
    Celsius,
    Fahrenheit,
    KilometresPerHour,
    MilesPerHour,
    MetresPerSecond,
    Millimetre,
    Inch,
    Hectopascal,
    InchOfMercury,
    Metre,
    Kilometre,
    Mile,
}

/// A number in a unit, with the fraction digits a card shows (`least` to
/// `most`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub value: f64,
    pub unit: Unit,
    pub least: u8,
    pub most: u8,
}

impl Reading {
    fn new(value: f64, unit: Unit, least: u8, most: u8) -> Reading {
        Reading {
            value,
            unit,
            least,
            most,
        }
    }

    /// Rounded to a whole number.
    fn whole(value: f64, unit: Unit) -> Reading {
        Reading::new(value, unit, 0, 0)
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
        let reading = |value: f64, unit, least, most| Reading {
            value,
            unit,
            least,
            most,
        };
        assert_eq!(
            imperial.temperature_reading(0.0),
            reading(32.0, Unit::Degree, 0, 0)
        );
        assert_eq!(
            imperial.temperature_with_unit_reading(100.0),
            reading(212.0, Unit::Fahrenheit, 0, 0)
        );
        assert_eq!(
            Units::METRIC.speed_reading(12.4),
            reading(12.4, Unit::KilometresPerHour, 0, 0)
        );
        let ms = Units {
            speed: Speed::MetresPerSecond,
            ..Units::METRIC
        };
        assert_eq!(
            ms.speed_reading(36.0),
            reading(10.0, Unit::MetresPerSecond, 1, 1)
        );
        assert_eq!(
            Units::METRIC.rain_reading(3.74),
            reading(3.74, Unit::Millimetre, 1, 1)
        );
        assert_eq!(
            Units::METRIC.rain_reading(0.0),
            reading(0.0, Unit::Millimetre, 0, 0)
        );
        assert_eq!(imperial.rain_reading(25.4), reading(1.0, Unit::Inch, 2, 2));
        assert_eq!(
            Units::METRIC.pressure_reading(1013.2),
            reading(1013.2, Unit::Hectopascal, 0, 0)
        );
        assert_eq!(imperial.pressure_reading(1013.25).unit, Unit::InchOfMercury);
        assert_eq!(
            Units::METRIC.distance_reading(24_140.0).unit,
            Unit::Kilometre
        );
        assert_eq!(Units::METRIC.distance_reading(24_140.0).most, 0);
        assert_eq!(Units::METRIC.distance_reading(3_100.0).most, 1);
        assert_eq!(
            Units::METRIC.distance_reading(800.0),
            reading(800.0, Unit::Metre, 0, 0)
        );
        assert_eq!(imperial.distance_reading(9_656.0).unit, Unit::Mile);
    }
}
