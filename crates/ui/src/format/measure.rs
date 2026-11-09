//! A quantity with its unit: "21 °C", "12 km/h", "1,013 hPa", "2 hr".

use std::fmt;

use super::Width;
use super::number::{Grouping, Number, Percent, Quantity, Sign, shifted};
use super::timespan::{TimeUnit, time_unit_text};
use crate::locale::{Category, Locale};

/// A unit of measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Celsius,
    Fahrenheit,
    Kelvin,
    Millimetre,
    Centimetre,
    Metre,
    Kilometre,
    Inch,
    Foot,
    Yard,
    Mile,
    Gram,
    Kilogram,
    Ounce,
    Pound,
    Millilitre,
    Litre,
    MetresPerSecond,
    KilometresPerHour,
    MilesPerHour,
    Knot,
    Hectopascal,
    Kilopascal,
    Millibar,
    InchOfMercury,
    Watt,
    Kilowatt,
    WattHour,
    KilowattHour,
    Hertz,
    Kilohertz,
    Megahertz,
    Gigahertz,
    /// An angle: `45°`.
    Degree,
    /// A share written as a percentage number: `45` is `45%`. For a
    /// fraction of a whole (`0.45`) use [`Percent`](super::Percent).
    Percent,
    /// A span of time, named in the language (`2 hr`, `2 hours`).
    Time(TimeUnit),
}

impl From<TimeUnit> for Unit {
    fn from(unit: TimeUnit) -> Unit {
        Unit::Time(unit)
    }
}

impl Unit {
    /// The symbol and whether it is written close to the number in
    /// `language`.
    fn symbol(self, language: &str) -> Option<(&'static str, bool)> {
        // `°C` and `°F` touch the number in English and in the languages
        // of East Asia, and stand a space away in most others.
        let tight_degrees = matches!(language, "en" | "ja" | "zh" | "ko" | "nl" | "he");
        Some(match self {
            Unit::Celsius => ("°C", tight_degrees),
            Unit::Fahrenheit => ("°F", tight_degrees),
            Unit::Kelvin => ("K", false),
            Unit::Millimetre => ("mm", false),
            Unit::Centimetre => ("cm", false),
            Unit::Metre => ("m", false),
            Unit::Kilometre => ("km", false),
            Unit::Inch => ("in", false),
            Unit::Foot => ("ft", false),
            Unit::Yard => ("yd", false),
            Unit::Mile => ("mi", false),
            Unit::Gram => ("g", false),
            Unit::Kilogram => ("kg", false),
            Unit::Ounce => ("oz", false),
            Unit::Pound => ("lb", false),
            Unit::Millilitre => ("mL", false),
            Unit::Litre => ("L", false),
            Unit::MetresPerSecond => ("m/s", false),
            Unit::KilometresPerHour => ("km/h", false),
            Unit::MilesPerHour => ("mph", false),
            Unit::Knot => ("kn", false),
            Unit::Hectopascal => ("hPa", false),
            Unit::Kilopascal => ("kPa", false),
            Unit::Millibar => ("mbar", false),
            Unit::InchOfMercury => ("inHg", false),
            Unit::Watt => ("W", false),
            Unit::Kilowatt => ("kW", false),
            Unit::WattHour => ("Wh", false),
            Unit::KilowattHour => ("kWh", false),
            Unit::Hertz => ("Hz", false),
            Unit::Kilohertz => ("kHz", false),
            Unit::Megahertz => ("MHz", false),
            Unit::Gigahertz => ("GHz", false),
            Unit::Degree => ("°", true),
            Unit::Percent | Unit::Time(_) => return None,
        })
    }
}

/// A number with its unit.
///
/// Time units and percentages are written by the language's own data (word,
/// plural, order and spacing). The other units use their international
/// symbol, which is what every language writes, set a no-break space from
/// the number, except degrees, which touch it in English.
///
/// ```
/// use bingux_ui::format::{Measure, Unit};
/// use bingux_ui::locale::Locale;
///
/// let en = Locale::parse("en-GB").unwrap();
/// let de = Locale::parse("de-DE").unwrap();
/// assert_eq!(Measure::new(21.5, Unit::Celsius).locale(&en).to_string(), "21.5°C");
/// assert_eq!(Measure::new(21.5, Unit::Celsius).locale(&de).to_string(), "21,5\u{a0}°C");
/// assert_eq!(Measure::new(12, Unit::KilometresPerHour).locale(&en).to_string(), "12\u{a0}km/h");
/// ```
#[derive(Clone, Debug)]
pub struct Measure {
    number: Number,
    unit: Unit,
    width: Width,
    locale: Option<Locale>,
}

impl Measure {
    pub fn new(value: impl Into<Quantity>, unit: impl Into<Unit>) -> Measure {
        Measure {
            number: Number::new(value),
            unit: unit.into(),
            width: Width::Short,
            locale: None,
        }
    }

    /// The fewest and most digits after the decimal point.
    pub fn fraction_digits(mut self, least: u8, most: u8) -> Measure {
        self.number = self.number.fraction_digits(least, most);
        self
    }

    /// At most `most` digits after the decimal point.
    pub fn max_fraction_digits(mut self, most: u8) -> Measure {
        self.number = self.number.max_fraction_digits(most);
        self
    }

    pub fn grouping(mut self, grouping: Grouping) -> Measure {
        self.number = self.number.grouping(grouping);
        self
    }

    pub fn sign(mut self, sign: Sign) -> Measure {
        self.number = self.number.sign(sign);
        self
    }

    /// `Long` writes a time unit in full (`2 hours`); other units keep
    /// their symbol at every width.
    pub fn width(mut self, width: Width) -> Measure {
        self.width = width;
        self
    }

    /// The locale to write it for, instead of the person's (`LC_NUMERIC`;
    /// `LC_MESSAGES` for the name of a time unit).
    pub fn locale(mut self, locale: &Locale) -> Measure {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        let numeric = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Numeric));
        let number = self.number.clone().locale(&numeric);
        match self.unit {
            Unit::Percent => match number.rounded() {
                Some(rounded) => {
                    // The percent formatter takes the number as written.
                    Percent::new(Quantity::from_decimal(shifted(rounded, -2)))
                        .max_fraction_digits(8)
                        .locale(&numeric)
                        .format()
                }
                None => format!("{number}%"),
            },
            Unit::Time(unit) => {
                let named = self
                    .locale
                    .clone()
                    .unwrap_or_else(|| Locale::for_category(Category::Messages));
                number
                    .rounded()
                    .and_then(|rounded| time_unit_text(&named, self.width, unit, &rounded))
                    .unwrap_or_else(|| number.format())
            }
            unit => {
                let (symbol, tight) = unit.symbol(numeric.language()).unwrap_or(("", true));
                let text = number.format();
                if tight {
                    format!("{text}{symbol}")
                } else {
                    format!("{text}\u{a0}{symbol}")
                }
            }
        }
    }
}

impl fmt::Display for Measure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
