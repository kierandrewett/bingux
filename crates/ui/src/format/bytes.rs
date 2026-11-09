//! File sizes: "512 B", "2.4 MB", "19 kB".

use std::fmt;

use fixed_decimal::{Decimal, SignedRoundingMode, UnsignedRoundingMode};

use super::number::{Grouping, Number, Quantity, shifted};
use super::plural::Plural;
use crate::locale::{Category, Locale};

/// The size of a kilo: 1000 (`kB`, `MB`, `GB`) as drive makers and Files
/// count, or 1024 (`KiB`, `MiB`, `GiB`) as memory is counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ByteBase {
    #[default]
    Decimal,
    Binary,
}

/// A size in bytes, in the largest unit that keeps it at 1 or more.
///
/// One decimal under 10, none from 10 up (`2.4 MB`, `19 kB`, `340 kB`);
/// a whole number has no `.0` (`2 MB`). The number is written for the locale
/// and sits a no-break space from its unit, so a size is never split across
/// two lines.
///
/// ```
/// use bingux_ui::format::{ByteBase, Bytes};
/// use bingux_ui::locale::Locale;
///
/// let en = Locale::parse("en-GB").unwrap();
/// let size = |n: u64| Bytes::new(n).locale(&en).to_string();
/// assert_eq!(size(512), "512\u{a0}B");
/// assert_eq!(size(2_400_000), "2.4\u{a0}MB");
/// assert_eq!(size(19_000), "19\u{a0}kB");
/// assert_eq!(Bytes::new(2_400_000).base(ByteBase::Binary).locale(&en).to_string(), "2.3\u{a0}MiB");
/// ```
#[derive(Clone, Debug)]
pub struct Bytes {
    bytes: u64,
    base: ByteBase,
    words: bool,
    fraction: Option<u8>,
    locale: Option<Locale>,
}

const MAX_UNIT: usize = 5;

impl Bytes {
    pub fn new(bytes: u64) -> Bytes {
        Bytes {
            bytes,
            base: ByteBase::Decimal,
            words: false,
            fraction: None,
            locale: None,
        }
    }

    pub fn base(mut self, base: ByteBase) -> Bytes {
        self.base = base;
        self
    }

    /// Binary units: `KiB`, `MiB`.
    pub fn binary(self) -> Bytes {
        self.base(ByteBase::Binary)
    }

    /// Under a kilo, the word instead of the symbol where the language has
    /// one: `512 bytes`, `1 byte`. English only; every other language keeps
    /// the symbol (`512 B`).
    pub fn words(mut self) -> Bytes {
        self.words = true;
        self
    }

    /// Up to `digits` digits after the point where there are any (a size
    /// under a kilo is whole), in place of one under 10 and none above.
    pub fn fraction_digits(mut self, digits: u8) -> Bytes {
        self.fraction = Some(digits);
        self
    }

    /// The locale to write it for, instead of the person's (`LC_NUMERIC`).
    pub fn locale(mut self, locale: &Locale) -> Bytes {
        self.locale = Some(locale.clone());
        self
    }

    /// The unit, the value in it rounded for display, and the fraction digits
    /// it was rounded to.
    fn scaled(&self) -> (usize, Decimal, u8) {
        let mode = SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand);
        let step: f64 = match self.base {
            ByteBase::Decimal => 1000.0,
            ByteBase::Binary => 1024.0,
        };
        let in_unit = |unit: usize| -> Decimal {
            match self.base {
                // Moving the point is exact.
                ByteBase::Decimal => shifted(Decimal::from(self.bytes), -3 * unit as i16),
                ByteBase::Binary => {
                    let value = self.bytes as f64 / step.powi(unit as i32);
                    Decimal::try_from_str(&value.to_string()).unwrap_or_default()
                }
            }
        };
        let mut unit = 0;
        while unit < MAX_UNIT && self.bytes as f64 >= step.powi(unit as i32 + 1) {
            unit += 1;
        }
        loop {
            let mut value = in_unit(unit);
            let digits: u8 = match (unit, self.fraction) {
                (0, _) => 0,
                (_, Some(digits)) => digits,
                // Below 10 a decimal helps.
                (_, None) if value.absolute.nonzero_magnitude_start() < 1 => 1,
                (_, None) => 0,
            };
            value.round_with_mode(-i16::from(digits), mode);
            // 999.95 kB rounds to 1000 kB: say 1 MB.
            let rounded: f64 = value.to_string().parse().unwrap_or(0.0);
            if rounded >= step && unit < MAX_UNIT {
                unit += 1;
                continue;
            }
            return (unit, value, digits);
        }
    }

    /// The text.
    pub fn format(&self) -> String {
        super::isolate(self.compose())
    }

    /// The text without the isolate [`format`](Self::format) adds in a
    /// right-to-left interface: for a formatter that writes it into its own.
    pub(super) fn compose(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Numeric));
        let (unit, value, digits) = self.scaled();
        if unit == 0 && self.words && locale.language() == "en" {
            return Plural::new(self.bytes, "{n} bytes")
                .one("{n} byte")
                .locale(&locale)
                .compose();
        }
        let number = Number::new(Quantity::from_decimal(value))
            .max_fraction_digits(digits)
            .grouping(Grouping::Min2)
            .locale(&locale)
            .format();
        format!(
            "{number}\u{a0}{}",
            symbol(locale.language(), self.base, unit)
        )
    }
}

/// `kB`, `MB` and `GB`, or what the language writes in their place: French
/// counts octets (`ko`, `Mo`).
fn symbol(language: &str, base: ByteBase, unit: usize) -> &'static str {
    const SI: [&str; 6] = ["B", "kB", "MB", "GB", "TB", "PB"];
    const IEC: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    const FRENCH_SI: [&str; 6] = ["o", "ko", "Mo", "Go", "To", "Po"];
    const FRENCH_IEC: [&str; 6] = ["o", "Kio", "Mio", "Gio", "Tio", "Pio"];
    let table = match (language, base) {
        ("fr", ByteBase::Decimal) => &FRENCH_SI,
        ("fr", ByteBase::Binary) => &FRENCH_IEC,
        (_, ByteBase::Decimal) => &SI,
        (_, ByteBase::Binary) => &IEC,
    };
    table[unit.min(MAX_UNIT)]
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
