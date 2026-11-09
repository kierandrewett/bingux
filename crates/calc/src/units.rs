//! Units and what converts to what.
//!
//! Every unit is an exact fraction of a base unit of its category, so a
//! conversion is exact: 1 inch is 25.4 mm with no rounding. Temperatures and
//! fuel economy are not proportional and have their own rules.
//! Currencies take their rates from [`crate::rates`].

use crate::error::{CalcError, Result};
use crate::number::Num;
use crate::rates::Rates;

/// How a unit relates to the base unit of its category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Map {
    /// `base = value * factor`.
    Linear(&'static str),
    /// `base = value * scale + offset` (temperatures).
    Affine(&'static str, &'static str),
    /// `base = k / value` (litres per 100 km against kilometres per litre).
    Reciprocal(&'static str),
    /// A currency: euros per unit come from the exchange rates.
    Currency,
}

#[derive(Clone, Copy, Debug)]
pub struct Unit {
    /// A stable key, stored in the history and the settings.
    pub id: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    /// Other spellings, separated by `|`.
    pub aliases: &'static str,
    pub map: Map,
}

#[derive(Clone, Copy, Debug)]
pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
    pub units: &'static [Unit],
    /// The units a fresh converter starts on.
    pub from: &'static str,
    pub to: &'static str,
}

const fn lin(
    id: &'static str,
    symbol: &'static str,
    name: &'static str,
    aliases: &'static str,
    factor: &'static str,
) -> Unit {
    Unit {
        id,
        symbol,
        name,
        aliases,
        map: Map::Linear(factor),
    }
}

const LENGTH: &[Unit] = &[
    lin("m", "m", "Metre", "metre|metres|meter|meters", "1"),
    lin(
        "km",
        "km",
        "Kilometre",
        "kilometre|kilometres|kilometer|kilometers|kms",
        "1000",
    ),
    lin(
        "cm",
        "cm",
        "Centimetre",
        "centimetre|centimetres|centimeter|centimeters",
        "0.01",
    ),
    lin(
        "mm",
        "mm",
        "Millimetre",
        "millimetre|millimetres|millimeter|millimeters",
        "0.001",
    ),
    lin(
        "um",
        "\u{b5}m",
        "Micrometre",
        "um|micrometre|micrometres|micron|microns",
        "0.000001",
    ),
    lin(
        "nm",
        "nm",
        "Nanometre",
        "nanometre|nanometres|nanometer|nanometers",
        "0.000000001",
    ),
    lin("mi", "mi", "Mile", "mile|miles", "1609.344"),
    lin("yd", "yd", "Yard", "yard|yards", "0.9144"),
    lin("ft", "ft", "Foot", "foot|feet|'", "0.3048"),
    lin("in", "in", "Inch", "inch|inches|\"", "0.0254"),
    lin(
        "nmi",
        "nmi",
        "Nautical mile",
        "nautical mile|nautical miles",
        "1852",
    ),
    lin(
        "ly",
        "ly",
        "Light year",
        "light year|light years|lightyear|lightyears",
        "9460730472580800",
    ),
    lin(
        "au",
        "au",
        "Astronomical unit",
        "astronomical unit|astronomical units",
        "149597870700",
    ),
];

const MASS: &[Unit] = &[
    lin(
        "kg",
        "kg",
        "Kilogram",
        "kilogram|kilograms|kilo|kilos|kgs",
        "1",
    ),
    lin("g", "g", "Gram", "gram|grams|gramme|grammes", "0.001"),
    lin("mg", "mg", "Milligram", "milligram|milligrams", "0.000001"),
    lin(
        "ug",
        "\u{b5}g",
        "Microgram",
        "ug|microgram|micrograms|mcg",
        "0.000000001",
    ),
    lin(
        "t",
        "t",
        "Tonne",
        "tonne|tonnes|metric ton|metric tons",
        "1000",
    ),
    lin("lb", "lb", "Pound", "pound|pounds|lbs", "0.45359237"),
    lin("oz", "oz", "Ounce", "ounce|ounces", "0.028349523125"),
    lin("st", "st", "Stone", "stone|stones", "6.35029318"),
    lin(
        "ton-us",
        "ton",
        "US ton",
        "us ton|short ton|short tons",
        "907.18474",
    ),
    lin(
        "ton-uk",
        "long ton",
        "UK ton",
        "uk ton|long tons",
        "1016.0469088",
    ),
    lin("ct", "ct", "Carat", "carat|carats", "0.0002"),
];

const VOLUME: &[Unit] = &[
    lin("l", "L", "Litre", "l|litre|litres|liter|liters", "1"),
    lin(
        "ml",
        "mL",
        "Millilitre",
        "ml|millilitre|millilitres|milliliter|milliliters|cc",
        "0.001",
    ),
    lin(
        "cl",
        "cL",
        "Centilitre",
        "cl|centilitre|centilitres|centiliter|centiliters",
        "0.01",
    ),
    lin(
        "m3",
        "m\u{b3}",
        "Cubic metre",
        "m3|cubic metre|cubic metres|cubic meter|cubic meters",
        "1000",
    ),
    lin(
        "cm3",
        "cm\u{b3}",
        "Cubic centimetre",
        "cm3|cubic centimetre|cubic centimetres",
        "0.001",
    ),
    lin(
        "gal-us",
        "gal",
        "US gallon",
        "gal|gallon|gallons|us gallon|us gallons",
        "3.785411784",
    ),
    lin("qt-us", "qt", "US quart", "qt|quart|quarts", "0.946352946"),
    lin("pt-us", "pt", "US pint", "pt|pint|pints", "0.473176473"),
    lin("cup-us", "cup", "US cup", "cup|cups", "0.2365882365"),
    lin(
        "floz-us",
        "fl oz",
        "US fluid ounce",
        "floz|fl. oz|fluid ounce|fluid ounces",
        "0.0295735295625",
    ),
    lin(
        "tbsp",
        "tbsp",
        "Tablespoon",
        "tablespoon|tablespoons",
        "0.01478676478125",
    ),
    lin(
        "tsp",
        "tsp",
        "Teaspoon",
        "teaspoon|teaspoons",
        "0.00492892159375",
    ),
    lin(
        "gal-uk",
        "UK gal",
        "UK gallon",
        "imperial gallon|imperial gallons|uk gallon",
        "4.54609",
    ),
    lin(
        "pt-uk",
        "UK pt",
        "UK pint",
        "imperial pint|imperial pints|uk pint",
        "0.56826125",
    ),
    lin(
        "floz-uk",
        "UK fl oz",
        "UK fluid ounce",
        "imperial fluid ounce|uk fl oz",
        "0.0284130625",
    ),
    lin(
        "in3",
        "in\u{b3}",
        "Cubic inch",
        "in3|cubic inch|cubic inches",
        "0.016387064",
    ),
    lin(
        "ft3",
        "ft\u{b3}",
        "Cubic foot",
        "ft3|cubic foot|cubic feet",
        "28.316846592",
    ),
];

const AREA: &[Unit] = &[
    lin(
        "m2",
        "m\u{b2}",
        "Square metre",
        "m2|square metre|square metres|square meter|square meters|sqm",
        "1",
    ),
    lin(
        "km2",
        "km\u{b2}",
        "Square kilometre",
        "km2|square kilometre|square kilometres",
        "1000000",
    ),
    lin(
        "cm2",
        "cm\u{b2}",
        "Square centimetre",
        "cm2|square centimetre|square centimetres",
        "0.0001",
    ),
    lin(
        "mm2",
        "mm\u{b2}",
        "Square millimetre",
        "mm2|square millimetre|square millimetres",
        "0.000001",
    ),
    lin("ha", "ha", "Hectare", "hectare|hectares", "10000"),
    lin("acre", "ac", "Acre", "ac|acre|acres", "4046.8564224"),
    lin(
        "ft2",
        "ft\u{b2}",
        "Square foot",
        "ft2|square foot|square feet|sqft",
        "0.09290304",
    ),
    lin(
        "in2",
        "in\u{b2}",
        "Square inch",
        "in2|square inch|square inches|sqin",
        "0.00064516",
    ),
    lin(
        "yd2",
        "yd\u{b2}",
        "Square yard",
        "yd2|square yard|square yards",
        "0.83612736",
    ),
    lin(
        "mi2",
        "mi\u{b2}",
        "Square mile",
        "mi2|square mile|square miles",
        "2589988.110336",
    ),
];

const SPEED: &[Unit] = &[
    lin(
        "mps",
        "m/s",
        "Metres per second",
        "m/s|mps|metres per second|meters per second",
        "1",
    ),
    lin(
        "kmh",
        "km/h",
        "Kilometres per hour",
        "kmh|kph|km/hr|kilometres per hour|kilometers per hour",
        "5/18",
    ),
    lin(
        "mph",
        "mph",
        "Miles per hour",
        "mi/h|miles per hour",
        "0.44704",
    ),
    lin("kn", "kn", "Knot", "kt|kts|knot|knots", "463/900"),
    lin(
        "fts",
        "ft/s",
        "Feet per second",
        "fps|feet per second",
        "0.3048",
    ),
    lin(
        "c-light",
        "c",
        "Speed of light",
        "speed of light",
        "299792458",
    ),
    lin("mach", "Mach", "Mach", "mach", "340.29"),
];

const TIME: &[Unit] = &[
    lin("s", "s", "Second", "sec|secs|second|seconds", "1"),
    lin(
        "ms-t",
        "ms",
        "Millisecond",
        "msec|millisecond|milliseconds",
        "0.001",
    ),
    lin(
        "us-t",
        "\u{b5}s",
        "Microsecond",
        "us|microsecond|microseconds",
        "0.000001",
    ),
    lin(
        "ns",
        "ns",
        "Nanosecond",
        "nanosecond|nanoseconds",
        "0.000000001",
    ),
    lin("min", "min", "Minute", "mins|minute|minutes", "60"),
    lin("h", "h", "Hour", "hr|hrs|hour|hours", "3600"),
    lin("d", "d", "Day", "day|days", "86400"),
    lin("wk", "wk", "Week", "w|week|weeks", "604800"),
    lin("mo", "mo", "Month", "month|months", "2629746"),
    lin("yr", "yr", "Year", "y|year|years", "31556952"),
];

const DATA: &[Unit] = &[
    lin("B", "B", "Byte", "byte|bytes", "1"),
    lin("bit", "bit", "Bit", "bits", "1/8"),
    lin("kB", "kB", "Kilobyte", "kilobyte|kilobytes", "1000"),
    lin("MB", "MB", "Megabyte", "megabyte|megabytes", "1000000"),
    lin("GB", "GB", "Gigabyte", "gigabyte|gigabytes", "1000000000"),
    lin(
        "TB",
        "TB",
        "Terabyte",
        "terabyte|terabytes",
        "1000000000000",
    ),
    lin(
        "PB",
        "PB",
        "Petabyte",
        "petabyte|petabytes",
        "1000000000000000",
    ),
    lin("KiB", "KiB", "Kibibyte", "kibibyte|kibibytes", "1024"),
    lin("MiB", "MiB", "Mebibyte", "mebibyte|mebibytes", "1048576"),
    lin("GiB", "GiB", "Gibibyte", "gibibyte|gibibytes", "1073741824"),
    lin(
        "TiB",
        "TiB",
        "Tebibyte",
        "tebibyte|tebibytes",
        "1099511627776",
    ),
    lin(
        "PiB",
        "PiB",
        "Pebibyte",
        "pebibyte|pebibytes",
        "1125899906842624",
    ),
    lin("kbit", "kbit", "Kilobit", "kb|kilobit|kilobits", "125"),
    lin("Mbit", "Mbit", "Megabit", "Mb|megabit|megabits", "125000"),
    lin(
        "Gbit",
        "Gbit",
        "Gigabit",
        "Gb|gigabit|gigabits",
        "125000000",
    ),
];

const ENERGY: &[Unit] = &[
    lin("J", "J", "Joule", "joule|joules", "1"),
    lin("kJ", "kJ", "Kilojoule", "kilojoule|kilojoules", "1000"),
    lin("cal", "cal", "Calorie", "calorie|calories", "4.184"),
    lin(
        "kcal",
        "kcal",
        "Kilocalorie",
        "kilocalorie|kilocalories|Cal",
        "4184",
    ),
    lin("Wh", "Wh", "Watt hour", "watt hour|watt hours", "3600"),
    lin(
        "kWh",
        "kWh",
        "Kilowatt hour",
        "kilowatt hour|kilowatt hours",
        "3600000",
    ),
    lin(
        "eV",
        "eV",
        "Electronvolt",
        "electronvolt|electronvolts",
        "0.0000000000000000001602176634",
    ),
    lin(
        "BTU",
        "BTU",
        "British thermal unit",
        "btu|british thermal unit|british thermal units",
        "1055.05585262",
    ),
    lin(
        "ftlbf",
        "ft\u{b7}lbf",
        "Foot-pound",
        "ft lbf|foot pound|foot pounds|foot-pound",
        "1.3558179483314004",
    ),
    lin("therm", "thm", "Therm", "therm|therms", "105505585.262"),
];

const POWER: &[Unit] = &[
    lin("W", "W", "Watt", "watt|watts", "1"),
    lin("kW", "kW", "Kilowatt", "kilowatt|kilowatts", "1000"),
    lin("MW", "MW", "Megawatt", "megawatt|megawatts", "1000000"),
    lin(
        "hp",
        "hp",
        "Horsepower",
        "horsepower|bhp",
        "745.69987158227022",
    ),
    lin(
        "PS",
        "PS",
        "Metric horsepower",
        "ps|metric horsepower|cv",
        "735.49875",
    ),
    lin(
        "BTUh",
        "BTU/h",
        "BTU per hour",
        "btu/h|btu per hour",
        "0.29307107017",
    ),
];

const PRESSURE: &[Unit] = &[
    lin("Pa", "Pa", "Pascal", "pascal|pascals", "1"),
    lin("kPa", "kPa", "Kilopascal", "kilopascal|kilopascals", "1000"),
    lin(
        "MPa",
        "MPa",
        "Megapascal",
        "megapascal|megapascals",
        "1000000",
    ),
    lin(
        "hPa",
        "hPa",
        "Hectopascal",
        "hectopascal|hectopascals|mbar|millibar|millibars",
        "100",
    ),
    lin("bar", "bar", "Bar", "bars", "100000"),
    lin(
        "atm",
        "atm",
        "Atmosphere",
        "atmosphere|atmospheres",
        "101325",
    ),
    lin(
        "psi",
        "psi",
        "Pound per square inch",
        "pounds per square inch",
        "6894.757293168",
    ),
    lin(
        "mmHg",
        "mmHg",
        "Millimetre of mercury",
        "mm hg|mmhg",
        "133.322387415",
    ),
    lin("torr", "Torr", "Torr", "torr", "20265/152"),
    lin("inHg", "inHg", "Inch of mercury", "in hg|inhg", "3386.389"),
];

const ANGLE: &[Unit] = &[
    lin("deg", "\u{b0}", "Degree", "deg|degree|degrees", "1"),
    // 180 / pi, to 40 digits.
    lin(
        "rad",
        "rad",
        "Radian",
        "radian|radians",
        "57.29577951308232087679815481410517033240",
    ),
    lin(
        "grad",
        "grad",
        "Gradian",
        "gon|gradian|gradians|grads",
        "0.9",
    ),
    lin(
        "turn",
        "turn",
        "Turn",
        "rev|revolution|revolutions|turns",
        "360",
    ),
    lin(
        "arcmin",
        "\u{2032}",
        "Arcminute",
        "arcmin|arcminute|arcminutes",
        "1/60",
    ),
    lin(
        "arcsec",
        "\u{2033}",
        "Arcsecond",
        "arcsec|arcsecond|arcseconds",
        "1/3600",
    ),
];

const TEMPERATURE: &[Unit] = &[
    Unit {
        id: "C",
        symbol: "\u{b0}C",
        name: "Celsius",
        aliases: "c|celsius|centigrade|degc|deg c|degrees celsius",
        map: Map::Affine("1", "273.15"),
    },
    Unit {
        id: "F",
        symbol: "\u{b0}F",
        name: "Fahrenheit",
        aliases: "f|fahrenheit|degf|deg f|degrees fahrenheit",
        map: Map::Affine("5/9", "45967/180"),
    },
    Unit {
        id: "K",
        symbol: "K",
        name: "Kelvin",
        aliases: "k|kelvin|kelvins",
        map: Map::Affine("1", "0"),
    },
    Unit {
        id: "R",
        symbol: "\u{b0}R",
        name: "Rankine",
        aliases: "r|rankine|degr|deg r",
        map: Map::Affine("5/9", "0"),
    },
];

const FUEL: &[Unit] = &[
    lin("kml", "km/L", "Kilometres per litre", "km/l|kpl", "1"),
    // 1 mpg (US) = 1.609344 / 3.785411784 km/L.
    lin(
        "mpg-us",
        "mpg",
        "Miles per US gallon",
        "mpg|miles per gallon|mpg us",
        "1609344/3785411.784",
    ),
    lin(
        "mpg-uk",
        "UK mpg",
        "Miles per UK gallon",
        "mpg uk|miles per imperial gallon",
        "1609.344/4546.09",
    ),
    Unit {
        id: "l100",
        symbol: "L/100 km",
        name: "Litres per 100 km",
        aliases: "l/100km|l/100 km|litres per 100 km",
        map: Map::Reciprocal("100"),
    },
];

/// The ISO codes and English names of the currencies the European Central
/// Bank publishes, and the euro.
const CURRENCIES: &[(&str, &str)] = &[
    ("EUR", "Euro"),
    ("USD", "US dollar"),
    ("GBP", "Pound sterling"),
    ("JPY", "Japanese yen"),
    ("CHF", "Swiss franc"),
    ("CAD", "Canadian dollar"),
    ("AUD", "Australian dollar"),
    ("NZD", "New Zealand dollar"),
    ("CNY", "Chinese yuan"),
    ("HKD", "Hong Kong dollar"),
    ("SGD", "Singapore dollar"),
    ("INR", "Indian rupee"),
    ("KRW", "South Korean won"),
    ("SEK", "Swedish krona"),
    ("NOK", "Norwegian krone"),
    ("DKK", "Danish krone"),
    ("ISK", "Icelandic krona"),
    ("PLN", "Polish zloty"),
    ("CZK", "Czech koruna"),
    ("HUF", "Hungarian forint"),
    ("RON", "Romanian leu"),
    ("TRY", "Turkish lira"),
    ("BRL", "Brazilian real"),
    ("MXN", "Mexican peso"),
    ("ZAR", "South African rand"),
    ("ILS", "Israeli shekel"),
    ("IDR", "Indonesian rupiah"),
    ("MYR", "Malaysian ringgit"),
    ("PHP", "Philippine peso"),
    ("THB", "Thai baht"),
];

/// The currency units, built once.
pub fn currencies() -> &'static [Unit] {
    use std::sync::OnceLock;
    static UNITS: OnceLock<Vec<Unit>> = OnceLock::new();
    UNITS.get_or_init(|| {
        CURRENCIES
            .iter()
            .map(|(code, name)| Unit {
                id: *code,
                symbol: *code,
                name: *name,
                aliases: "",
                map: Map::Currency,
            })
            .collect()
    })
}

/// Every category, in the order the picker lists them.
pub fn categories() -> Vec<Category> {
    vec![
        Category {
            id: "length",
            name: "Length",
            units: LENGTH,
            from: "km",
            to: "mi",
        },
        Category {
            id: "mass",
            name: "Weight and mass",
            units: MASS,
            from: "kg",
            to: "lb",
        },
        Category {
            id: "temperature",
            name: "Temperature",
            units: TEMPERATURE,
            from: "C",
            to: "F",
        },
        Category {
            id: "volume",
            name: "Volume",
            units: VOLUME,
            from: "l",
            to: "gal-us",
        },
        Category {
            id: "area",
            name: "Area",
            units: AREA,
            from: "m2",
            to: "ft2",
        },
        Category {
            id: "speed",
            name: "Speed",
            units: SPEED,
            from: "kmh",
            to: "mph",
        },
        Category {
            id: "time",
            name: "Time",
            units: TIME,
            from: "h",
            to: "min",
        },
        Category {
            id: "data",
            name: "Data",
            units: DATA,
            from: "GB",
            to: "MiB",
        },
        Category {
            id: "energy",
            name: "Energy",
            units: ENERGY,
            from: "kcal",
            to: "kJ",
        },
        Category {
            id: "power",
            name: "Power",
            units: POWER,
            from: "kW",
            to: "hp",
        },
        Category {
            id: "pressure",
            name: "Pressure",
            units: PRESSURE,
            from: "bar",
            to: "psi",
        },
        Category {
            id: "angle",
            name: "Angle",
            units: ANGLE,
            from: "deg",
            to: "rad",
        },
        Category {
            id: "fuel",
            name: "Fuel economy",
            units: FUEL,
            from: "l100",
            to: "mpg-us",
        },
        Category {
            id: "currency",
            name: "Currency",
            units: currencies(),
            from: "EUR",
            to: "USD",
        },
    ]
}

pub fn category(id: &str) -> Option<Category> {
    categories().into_iter().find(|c| c.id == id)
}

impl Category {
    pub fn unit(&self, id: &str) -> Option<&'static Unit> {
        self.units.iter().find(|u| u.id == id)
    }
}

/// An exact number from `"12.5"` or `"5/9"` (each side a decimal).
fn q(text: &str) -> Num {
    let decimal = |s: &str| -> Num {
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        Num::from_digits(false, &format!("{int}{frac}"), frac.len(), 0)
            .unwrap_or_else(|_| Num::zero())
    };
    let exact = match text.split_once('/') {
        Some((n, d)) => decimal(n).div(&decimal(d)).unwrap_or_else(|_| Num::zero()),
        None => decimal(text),
    };
    // A factor written to more than 20 digits is a measured or rounded
    // constant (a radian in degrees), not an exact one: it converts to an
    // approximation, which is shown to the display precision.
    let digits = text
        .chars()
        .filter(char::is_ascii_digit)
        .skip_while(|c| *c == '0')
        .count();
    if digits > 20 && !text.contains('/') {
        Num::approximate(exact.rat().clone(), None)
    } else {
        exact
    }
}

impl Unit {
    /// `value` of this unit as a number of the category's base unit.
    pub fn to_base(&self, value: &Num, rates: Option<&Rates>) -> Result<Num> {
        match self.map {
            Map::Linear(factor) => value.mul(&q(factor)),
            Map::Affine(scale, offset) => value.mul(&q(scale))?.add(&q(offset)),
            Map::Reciprocal(k) => {
                if value.is_zero() {
                    return Err(CalcError::DivideByZero);
                }
                q(k).div(value)
            }
            Map::Currency => {
                let per_euro = rates
                    .and_then(|r| r.per_euro(self.id))
                    .ok_or_else(|| CalcError::NoRate(self.id.to_string()))?;
                value.div(&per_euro)
            }
        }
    }

    /// A number of the base unit, as this unit.
    pub fn from_base(&self, base: &Num, rates: Option<&Rates>) -> Result<Num> {
        match self.map {
            Map::Linear(factor) => base.div(&q(factor)),
            Map::Affine(scale, offset) => base.sub(&q(offset))?.div(&q(scale)),
            Map::Reciprocal(k) => {
                if base.is_zero() {
                    return Err(CalcError::DivideByZero);
                }
                q(k).div(base)
            }
            Map::Currency => {
                let per_euro = rates
                    .and_then(|r| r.per_euro(self.id))
                    .ok_or_else(|| CalcError::NoRate(self.id.to_string()))?;
                base.mul(&per_euro)
            }
        }
    }
}

/// `value` from one unit of `category` to another.
pub fn convert(
    category: &Category,
    from: &str,
    to: &str,
    value: &Num,
    rates: Option<&Rates>,
) -> Result<Num> {
    let (from, to) = match (category.unit(from), category.unit(to)) {
        (Some(from), Some(to)) => (from, to),
        _ => {
            return Err(CalcError::Unit(
                "That unit is not in this category. Pick a unit from the list.".into(),
            ));
        }
    };
    if from.id == to.id {
        return Ok(value.clone());
    }
    to.from_base(&from.to_base(value, rates)?, rates)
}

/// Finds a unit by what a person types: its symbol, name or an alias. `hint`
/// limits the search to one category.
pub fn find_unit(text: &str, hint: Option<&Category>) -> Option<(Category, &'static Unit)> {
    let text = text.trim().trim_end_matches('.');
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let all = categories();
    let mut loose: Option<(Category, &'static Unit)> = None;
    for category in all.iter().filter(|c| hint.is_none_or(|h| h.id == c.id)) {
        for unit in category.units {
            if unit.symbol == text || unit.id == text || unit.aliases.split('|').any(|a| a == text)
            {
                return Some((*category, unit));
            }
            let matches = unit.symbol.to_lowercase() == lower
                || unit.id.to_lowercase() == lower
                || unit.name.to_lowercase() == lower
                || unit
                    .aliases
                    .split('|')
                    .any(|a| !a.is_empty() && a.to_lowercase() == lower);
            if matches && loose.is_none() {
                loose = Some((*category, unit));
            }
        }
    }
    loose
}

/// A conversion typed as `<expression> <unit> to <unit>`.
#[derive(Clone, Debug)]
pub struct Typed {
    pub expression: String,
    pub category: Category,
    pub from: &'static Unit,
    pub to: &'static Unit,
}

const SEPARATORS: [&str; 6] = [" to ", " in ", " as ", "->", "\u{2192}", " into "];

/// Reads `5 km to miles` or `72 degF in C`; `None` when the text is not a
/// conversion.
pub fn parse_typed(text: &str) -> Option<Typed> {
    let lower = text.to_lowercase();
    let (at, width) = SEPARATORS
        .iter()
        .filter_map(|sep| lower.rfind(sep).map(|at| (at, sep.len())))
        .max_by_key(|(at, _)| *at)?;
    // `to_lowercase` can change byte lengths; the separators are ASCII except
    // the arrow, so only text before a non-ASCII letter would shift.
    if !text.is_char_boundary(at) || !text.is_char_boundary(at + width) {
        return None;
    }
    let (left, right) = (text[..at].trim(), text[at + width..].trim());
    let (category, to) = find_unit(right, None)?;
    let boundaries: Vec<usize> = left
        .char_indices()
        .map(|(i, _)| i)
        .filter(|i| {
            *i > 0
                && left[..*i].chars().next_back().is_some_and(|c| {
                    c.is_ascii_digit() || c == ')' || c.is_whitespace() || c == '%' || c == '.'
                })
        })
        .collect();
    for start in boundaries {
        let suffix = left[start..].trim();
        if let Some((_, from)) = find_unit(suffix, Some(&category)) {
            let expression = left[..start].trim();
            if !expression.is_empty() {
                return Some(Typed {
                    expression: expression.to_string(),
                    category,
                    from,
                    to,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::plain;

    fn n(text: &str) -> Num {
        q(text)
    }

    fn conv(cat: &str, from: &str, to: &str, value: &str) -> String {
        let category = category(cat).unwrap();
        plain(&convert(&category, from, to, &n(value), None).unwrap(), 16)
    }

    #[test]
    fn lengths_are_exact() {
        assert_eq!(conv("length", "in", "mm", "1"), "25.4");
        assert_eq!(conv("length", "mi", "km", "1"), "1.609344");
        assert_eq!(conv("length", "km", "mi", "1.609344"), "1");
        assert_eq!(conv("length", "ft", "in", "1"), "12");
        assert_eq!(conv("length", "yd", "ft", "1"), "3");
        assert_eq!(conv("length", "km", "m", "0.001"), "1");
    }

    #[test]
    fn mass_volume_and_area() {
        assert_eq!(conv("mass", "lb", "oz", "1"), "16");
        assert_eq!(conv("mass", "kg", "g", "1.5"), "1500");
        assert_eq!(conv("mass", "st", "lb", "1"), "14");
        assert_eq!(conv("volume", "gal-us", "qt-us", "1"), "4");
        assert_eq!(conv("volume", "cup-us", "floz-us", "1"), "8");
        assert_eq!(conv("volume", "tbsp", "tsp", "1"), "3");
        assert_eq!(conv("area", "acre", "ft2", "1"), "43560");
        assert_eq!(conv("area", "ha", "m2", "1"), "10000");
    }

    #[test]
    fn temperatures_use_their_zero_points() {
        assert_eq!(conv("temperature", "C", "F", "100"), "212");
        assert_eq!(conv("temperature", "C", "F", "0"), "32");
        assert_eq!(conv("temperature", "F", "C", "-40"), "-40");
        assert_eq!(conv("temperature", "F", "C", "212"), "100");
        assert_eq!(conv("temperature", "C", "K", "0"), "273.15");
        assert_eq!(conv("temperature", "K", "C", "0"), "-273.15");
        assert_eq!(conv("temperature", "F", "K", "-459.67"), "0");
        assert_eq!(conv("temperature", "R", "F", "491.67"), "32");
    }

    #[test]
    fn speed_time_and_data() {
        assert_eq!(conv("speed", "kmh", "mps", "36"), "10");
        assert_eq!(conv("speed", "mph", "kmh", "1"), "1.609344");
        assert_eq!(conv("speed", "kn", "kmh", "1"), "1.852");
        assert_eq!(conv("time", "h", "min", "1.5"), "90");
        assert_eq!(conv("time", "d", "h", "1"), "24");
        assert_eq!(conv("data", "GiB", "MiB", "1"), "1024");
        assert_eq!(conv("data", "GB", "MB", "1"), "1000");
        assert_eq!(conv("data", "B", "bit", "1"), "8");
        assert_eq!(conv("data", "Mbit", "MB", "8"), "1");
    }

    #[test]
    fn energy_power_pressure_and_angle() {
        assert_eq!(conv("energy", "kcal", "kJ", "1"), "4.184");
        assert_eq!(conv("energy", "kWh", "Wh", "1"), "1000");
        assert_eq!(conv("power", "kW", "W", "1"), "1000");
        assert_eq!(conv("pressure", "bar", "kPa", "1"), "100");
        assert_eq!(conv("pressure", "atm", "Pa", "1"), "101325");
        assert_eq!(conv("angle", "turn", "deg", "1"), "360");
        assert_eq!(conv("angle", "deg", "grad", "90"), "100");
        assert_eq!(conv("angle", "arcmin", "deg", "60"), "1");
        assert_eq!(conv("angle", "rad", "deg", "1"), "57.29577951308232");
        assert_eq!(conv("angle", "deg", "rad", "180"), "3.141592653589793");
    }

    #[test]
    fn fuel_economy_is_reciprocal() {
        assert_eq!(conv("fuel", "l100", "kml", "10"), "10");
        assert_eq!(conv("fuel", "l100", "kml", "5"), "20");
        assert_eq!(conv("fuel", "kml", "l100", "20"), "5");
        assert_eq!(conv("fuel", "mpg-us", "l100", "30"), "7.840486111111111");
        let category = category("fuel").unwrap();
        assert_eq!(
            convert(&category, "l100", "kml", &n("0"), None),
            Err(CalcError::DivideByZero)
        );
    }

    #[test]
    fn currency_needs_rates() {
        let category = category("currency").unwrap();
        assert!(matches!(
            convert(&category, "USD", "GBP", &n("1"), None),
            Err(CalcError::NoRate(_))
        ));
    }

    #[test]
    fn currency_converts_through_the_euro() {
        let mut rates = Rates::empty();
        rates.set("USD", "1.25");
        rates.set("GBP", "0.8");
        let category = category("currency").unwrap();
        let usd_to_gbp = convert(&category, "USD", "GBP", &n("100"), Some(&rates)).unwrap();
        assert_eq!(plain(&usd_to_gbp, 16), "64");
        let eur_to_usd = convert(&category, "EUR", "USD", &n("8"), Some(&rates)).unwrap();
        assert_eq!(plain(&eur_to_usd, 16), "10");
        let usd_to_eur = convert(&category, "USD", "EUR", &n("10"), Some(&rates)).unwrap();
        assert_eq!(plain(&usd_to_eur, 16), "8");
        assert!(matches!(
            convert(&category, "USD", "JPY", &n("1"), Some(&rates)),
            Err(CalcError::NoRate(code)) if code == "JPY"
        ));
    }

    #[test]
    fn every_unit_id_is_unique_in_its_category_and_has_a_factor() {
        for category in categories() {
            let mut seen = std::collections::HashSet::new();
            for unit in category.units {
                assert!(seen.insert(unit.id), "{} twice in {}", unit.id, category.id);
                if let Map::Linear(f) = unit.map {
                    assert!(!q(f).is_zero(), "{} has no factor", unit.id);
                }
            }
            assert!(
                category.unit(category.from).is_some(),
                "{} from",
                category.id
            );
            assert!(category.unit(category.to).is_some(), "{} to", category.id);
        }
    }

    #[test]
    fn units_are_found_by_what_people_type() {
        let (c, u) = find_unit("km", None).unwrap();
        assert_eq!((c.id, u.id), ("length", "km"));
        assert_eq!(find_unit("Miles", None).unwrap().1.id, "mi");
        assert_eq!(find_unit("fahrenheit", None).unwrap().1.id, "F");
        assert_eq!(find_unit("\u{b0}C", None).unwrap().1.id, "C");
        assert_eq!(find_unit("usd", None).unwrap().1.id, "USD");
        assert_eq!(find_unit("MB", None).unwrap().1.id, "MB");
        assert_eq!(find_unit("Mb", None).unwrap().1.id, "Mbit");
        assert!(find_unit("blorp", None).is_none());
    }

    #[test]
    fn a_unit_can_belong_to_two_categories_and_the_hint_decides() {
        // `m` is the metre; `min` the minute; `ms` milliseconds or metres per second.
        let time = category("time").unwrap();
        assert_eq!(find_unit("ms", Some(&time)).unwrap().1.id, "ms-t");
        let speed = category("speed").unwrap();
        assert_eq!(find_unit("m/s", Some(&speed)).unwrap().1.id, "mps");
    }

    #[test]
    fn typed_conversions() {
        let t = parse_typed("5 km to miles").unwrap();
        assert_eq!(
            (t.expression.as_str(), t.from.id, t.to.id),
            ("5", "km", "mi")
        );
        let t = parse_typed("72 degF in C").unwrap();
        assert_eq!(
            (t.expression.as_str(), t.from.id, t.to.id),
            ("72", "F", "C")
        );
        let t = parse_typed("(2+3)*10 kg -> lb").unwrap();
        assert_eq!(
            (t.expression.as_str(), t.from.id, t.to.id),
            ("(2+3)*10", "kg", "lb")
        );
        let t = parse_typed("100 \u{b0}C to \u{b0}F").unwrap();
        assert_eq!((t.from.id, t.to.id), ("C", "F"));
        let t = parse_typed("10 usd to gbp").unwrap();
        assert_eq!((t.from.id, t.to.id), ("USD", "GBP"));
        let t = parse_typed("1.5 light years to km").unwrap();
        assert_eq!(t.from.id, "ly");
        assert!(parse_typed("5 km to blorp").is_none());
        assert!(parse_typed("5 to 6").is_none());
        assert!(parse_typed("5 + 3").is_none());
        // The units must be of one kind.
        assert!(parse_typed("5 kg to km").is_none());
    }
}
