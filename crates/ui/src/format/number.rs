//! Numbers, percentages and money.

use std::fmt;

use fixed_decimal::{Decimal, SignDisplay, SignedRoundingMode, UnsignedRoundingMode};
use icu::decimal::options::{
    CompactDecimalFormatterOptions, DecimalFormatterOptions, GroupingStrategy,
};
use icu::decimal::{CompactDecimalFormatter, DecimalFormatter};
use icu_experimental::dimension::currency::CurrencyType;
use icu_experimental::dimension::currency::formatter::CurrencyFormatter;
use icu_experimental::dimension::currency::options::{CurrencyFormatterOptions, CurrencyUsage};
use icu_experimental::dimension::percent::formatter::PercentFormatter;

use super::cache::cached;
use crate::locale::{Category, Locale};

/// Where a number's digits are grouped (`1,234,567`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// As the locale writes it: `1,234` in English, `1234` in Spanish
    /// (which groups only from five digits), `1.234` in German.
    #[default]
    Auto,
    /// Never: `1234567`.
    Never,
    /// Only from five digits: `1234` and `12,345`.
    Min2,
}

impl Grouping {
    fn strategy(self) -> GroupingStrategy {
        match self {
            Grouping::Auto => GroupingStrategy::Auto,
            Grouping::Never => GroupingStrategy::Never,
            Grouping::Min2 => GroupingStrategy::Min2,
        }
    }
}

/// When a number carries a sign.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sign {
    /// A minus on negative numbers. A negative number that rounds to zero
    /// has none (`-0.001` with no fraction digits is `0`).
    #[default]
    Minus,
    /// A plus on positive numbers and zero, a minus on negative ones.
    Always,
    /// As `Always`, but zero has no sign.
    ExceptZero,
    /// None: the magnitude only.
    Never,
}

impl Sign {
    fn display(self) -> SignDisplay {
        match self {
            Sign::Minus => SignDisplay::Negative,
            Sign::Always => SignDisplay::Always,
            Sign::ExceptZero => SignDisplay::ExceptZero,
            Sign::Never => SignDisplay::Never,
        }
    }
}

/// A short form for large numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compact {
    /// `1.2K`, `3.4M`.
    Short,
    /// `1.2 thousand`, `3.4 million`.
    Long,
}

/// What a number is made from. Floats are read by their shortest decimal
/// text (`0.1` is `0.1`, not `0.1000000000000000055`).
#[derive(Clone, Debug)]
enum Value {
    Decimal(Decimal),
    /// NaN or an infinity: the text of the number and whether it is
    /// negative.
    NonFinite {
        negative: bool,
        infinite: bool,
    },
}

impl Value {
    fn from_float(value: f64) -> Value {
        if !value.is_finite() {
            return Value::NonFinite {
                negative: value.is_sign_negative() && !value.is_nan(),
                infinite: value.is_infinite(),
            };
        }
        // Rust writes the shortest text that reads back as the same float,
        // never with an exponent.
        match Decimal::try_from_str(&value.to_string()) {
            Ok(decimal) => Value::Decimal(decimal),
            Err(_) => Value::NonFinite {
                negative: value.is_sign_negative(),
                infinite: true,
            },
        }
    }
}

/// A number as [`Number`] formats it: any integer or float, or a decimal
/// written as text.
#[derive(Clone, Debug)]
pub struct Quantity(Value);

macro_rules! quantity_from_int {
    ($($type:ty),*) => {$(
        impl From<$type> for Quantity {
            fn from(value: $type) -> Quantity {
                Quantity(Value::Decimal(Decimal::from(value)))
            }
        }
    )*};
}
quantity_from_int!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl From<f64> for Quantity {
    fn from(value: f64) -> Quantity {
        Quantity(Value::from_float(value))
    }
}

impl From<f32> for Quantity {
    fn from(value: f32) -> Quantity {
        // The float's own shortest text: 0.1f32 is 0.1, not 0.10000000149.
        Quantity(match value.is_finite() {
            true => Decimal::try_from_str(&value.to_string())
                .map(Value::Decimal)
                .unwrap_or(Value::NonFinite {
                    negative: value.is_sign_negative(),
                    infinite: true,
                }),
            false => Value::from_float(f64::from(value)),
        })
    }
}
/// `decimal` times ten to the power `places` (`shifted(250, -2)` is `2.50`).
pub(crate) fn shifted(mut decimal: Decimal, places: i16) -> Decimal {
    decimal.absolute.multiply_pow10(places);
    decimal.absolute.trim_start();
    decimal
}

impl Quantity {
    pub(crate) fn from_decimal(decimal: Decimal) -> Quantity {
        Quantity(Value::Decimal(decimal))
    }

    /// The decimal the quantity holds; `None` for NaN and the infinities.
    pub(crate) fn decimal(&self) -> Option<&Decimal> {
        match &self.0 {
            Value::Decimal(decimal) => Some(decimal),
            Value::NonFinite { .. } => None,
        }
    }

    /// An exact decimal written as text: `"12.50"`, `"-0.003"`. Nothing is
    /// lost to a float, so money and measurements keep every digit. `None`
    /// when the text is not a plain decimal (no exponent, no grouping marks).
    pub fn parse(text: &str) -> Option<Quantity> {
        Decimal::try_from_str(text.trim())
            .ok()
            .map(|decimal| Quantity(Value::Decimal(decimal)))
    }
}

/// What every number format shares: how many digits, grouping and sign.
#[derive(Clone, Debug)]
struct Digits {
    min_fraction: u8,
    max_fraction: Option<u8>,
    significant: Option<u8>,
    grouping: Grouping,
    sign: Sign,
}

impl Default for Digits {
    fn default() -> Digits {
        Digits {
            min_fraction: 0,
            max_fraction: None,
            significant: None,
            grouping: Grouping::Auto,
            sign: Sign::Minus,
        }
    }
}

impl Digits {
    /// The decimal to write: rounded half away from zero (0.125 to two digits
    /// is 0.13, as people round), padded to the least fraction digits.
    fn prepare(&self, mut decimal: Decimal, default_max: u8) -> Decimal {
        let mode = SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand);
        if let Some(significant) = self.significant.filter(|digits| *digits > 0) {
            if !decimal.is_zero() {
                let position = decimal.nonzero_magnitude_start() - i16::from(significant) + 1;
                decimal.round_with_mode(position, mode);
            }
        } else {
            let max = self
                .max_fraction
                .unwrap_or(default_max)
                .max(self.min_fraction);
            decimal.round_with_mode(-i16::from(max), mode);
        }
        decimal.trim_end();
        // A negative number that rounded to nothing is zero, not minus zero.
        if decimal.is_zero() {
            decimal.sign = fixed_decimal::Sign::None;
        }
        if self.min_fraction > 0 {
            decimal.pad_end(-i16::from(self.min_fraction));
        }
        decimal.apply_sign_display(self.sign.display());
        decimal
    }
}

/// A number written for a locale.
///
/// ```
/// use bingux_ui::format::Number;
/// use bingux_ui::locale::Locale;
///
/// let de = Locale::parse("de-DE").unwrap();
/// assert_eq!(Number::new(1234567.891).locale(&de).to_string(), "1.234.567,891");
/// assert_eq!(Number::new(0.5).fraction_digits(2, 2).to_string(), "0.50");
/// ```
#[derive(Clone, Debug)]
pub struct Number {
    value: Quantity,
    digits: Digits,
    compact: Option<Compact>,
    locale: Option<Locale>,
}

impl Number {
    /// A number: an integer, a float or a [`Quantity`] from text.
    pub fn new(value: impl Into<Quantity>) -> Number {
        Number {
            value: value.into(),
            digits: Digits::default(),
            compact: None,
            locale: None,
        }
    }

    /// The fewest and most digits after the decimal point. A float and an
    /// exact decimal default to at most 3 and no least; an integer is whole.
    pub fn fraction_digits(mut self, least: u8, most: u8) -> Number {
        self.digits.min_fraction = least;
        self.digits.max_fraction = Some(most.max(least));
        self
    }

    /// At most `most` digits after the decimal point.
    pub fn max_fraction_digits(mut self, most: u8) -> Number {
        self.digits.max_fraction = Some(most);
        self
    }

    /// At most `digits` significant digits (`1234.5` to 3 is `1230`, `0.012345`
    /// to 3 is `0.0123`). Replaces the fraction digit limit.
    pub fn significant_digits(mut self, digits: u8) -> Number {
        self.digits.significant = Some(digits);
        self
    }

    pub fn grouping(mut self, grouping: Grouping) -> Number {
        self.digits.grouping = grouping;
        self
    }

    pub fn sign(mut self, sign: Sign) -> Number {
        self.digits.sign = sign;
        self
    }

    /// `1.2K`, `3.4M` or `1.2 thousand`, rounded to two significant digits
    /// (three digits and more are whole: `123K`).
    pub fn compact(mut self, compact: Compact) -> Number {
        self.compact = Some(compact);
        self
    }

    /// The locale to write it for, instead of the person's
    /// (`LC_NUMERIC`).
    pub fn locale(mut self, locale: &Locale) -> Number {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Numeric));
        match &self.value.0 {
            Value::NonFinite { negative, infinite } => {
                non_finite(*negative, *infinite, self.digits.sign)
            }
            Value::Decimal(decimal) => {
                let decimal = self.digits.prepare(decimal.clone(), self.default_max());
                match self.compact {
                    Some(compact) => compact_text(&locale, compact, &decimal),
                    None => decimal_text(&locale, self.digits.grouping, &decimal),
                }
            }
        }
    }

    /// The decimal as it is written, rounded and signed; `None` for NaN and
    /// the infinities.
    pub(crate) fn rounded(&self) -> Option<Decimal> {
        self.value
            .decimal()
            .map(|decimal| self.digits.prepare(decimal.clone(), self.default_max()))
    }

    fn default_max(&self) -> u8 {
        // An integer has no fraction to show; a decimal and a float show up
        // to three digits, as most languages' number formatters do.
        match &self.value.0 {
            Value::Decimal(decimal) if decimal.nonzero_magnitude_end() >= 0 => 0,
            _ => 3,
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

fn non_finite(negative: bool, infinite: bool, sign: Sign) -> String {
    let body = if infinite { "∞" } else { "NaN" };
    let prefix = match (negative, sign) {
        (_, Sign::Never) => "",
        (true, _) => "-",
        (false, Sign::Always | Sign::ExceptZero) if infinite => "+",
        _ => "",
    };
    format!("{prefix}{body}")
}

pub(crate) fn decimal_text(locale: &Locale, grouping: Grouping, decimal: &Decimal) -> String {
    let key = format!("decimal|{}|{grouping:?}", locale.tag());
    let formatter = cached(&key, || {
        let mut options = DecimalFormatterOptions::default();
        options.grouping_strategy = Some(grouping.strategy());
        DecimalFormatter::try_new(locale.icu().into(), options).ok()
    });
    match formatter {
        Some(formatter) => formatter.format(decimal).to_string(),
        None => decimal.to_string(),
    }
}

fn compact_text(locale: &Locale, compact: Compact, decimal: &Decimal) -> String {
    let key = format!("compact|{}|{compact:?}", locale.tag());
    let formatter = cached(&key, || {
        let options = CompactDecimalFormatterOptions::default();
        match compact {
            Compact::Short => CompactDecimalFormatter::try_new_short(locale.icu().into(), options),
            Compact::Long => CompactDecimalFormatter::try_new_long(locale.icu().into(), options),
        }
        .ok()
    });
    match formatter {
        Some(formatter) => formatter.format_to_string(decimal),
        None => decimal.to_string(),
    }
}

/// A fraction written as a percentage: `0.256` is `26%`.
///
/// ```
/// use bingux_ui::format::Percent;
/// use bingux_ui::locale::Locale;
///
/// let fr = Locale::parse("fr-FR").unwrap();
/// assert_eq!(Percent::new(0.256).to_string(), "26%");
/// assert_eq!(Percent::new(0.256).max_fraction_digits(1).locale(&fr).to_string(), "25,6\u{202f}%");
/// ```
#[derive(Clone, Debug)]
pub struct Percent {
    value: Quantity,
    digits: Digits,
    locale: Option<Locale>,
}

impl Percent {
    /// `fraction` of a whole: `1.0` is 100%, `0.05` is 5%.
    pub fn new(fraction: impl Into<Quantity>) -> Percent {
        Percent {
            value: fraction.into(),
            digits: Digits::default(),
            locale: None,
        }
    }

    /// The fewest and most digits after the decimal point (none by default).
    pub fn fraction_digits(mut self, least: u8, most: u8) -> Percent {
        self.digits.min_fraction = least;
        self.digits.max_fraction = Some(most.max(least));
        self
    }

    /// At most `most` digits after the decimal point.
    pub fn max_fraction_digits(mut self, most: u8) -> Percent {
        self.digits.max_fraction = Some(most);
        self
    }

    pub fn sign(mut self, sign: Sign) -> Percent {
        self.digits.sign = sign;
        self
    }

    pub fn grouping(mut self, grouping: Grouping) -> Percent {
        self.digits.grouping = grouping;
        self
    }

    pub fn locale(mut self, locale: &Locale) -> Percent {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Numeric));
        let Value::Decimal(decimal) = &self.value.0 else {
            return format!("{}%", non_finite_of(&self.value.0, self.digits.sign));
        };
        let decimal = self.digits.prepare(shifted(decimal.clone(), 2), 0);
        percent_text(&locale, &decimal)
    }
}

fn non_finite_of(value: &Value, sign: Sign) -> String {
    match value {
        Value::NonFinite { negative, infinite } => non_finite(*negative, *infinite, sign),
        Value::Decimal(decimal) => decimal.to_string(),
    }
}

fn percent_text(locale: &Locale, decimal: &Decimal) -> String {
    let key = format!("percent|{}", locale.tag());
    let formatter = cached(&key, || {
        PercentFormatter::try_new(locale.icu().into(), Default::default()).ok()
    });
    match formatter {
        Some(formatter) => formatter.format(decimal).to_string(),
        None => format!("{decimal}%"),
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// How a currency is named next to the amount.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurrencyDisplay {
    /// The locale's symbol: `$12.50`, `12,50 $US` (a dollar is told apart
    /// from the local one where that matters).
    #[default]
    Symbol,
    /// The shortest symbol: `$12.50` for every dollar.
    Narrow,
    /// The ISO code: `USD 12.50`.
    Code,
    /// The name: `12.50 US dollars`.
    Name,
}

/// An amount of money.
///
/// The number of fraction digits is the currency's own (two for pounds, none
/// for yen), unless set.
///
/// ```
/// use bingux_ui::format::Currency;
/// use bingux_ui::locale::Locale;
///
/// let gb = Locale::parse("en-GB").unwrap();
/// assert_eq!(Currency::new(1234.5, "GBP").locale(&gb).to_string(), "£1,234.50");
/// assert_eq!(Currency::new(1234, "JPY").locale(&gb).to_string(), "JP¥1,234");
/// ```
#[derive(Clone, Debug)]
pub struct Currency {
    amount: Quantity,
    code: String,
    display: CurrencyDisplay,
    accounting: bool,
    locale: Option<Locale>,
}

impl Currency {
    /// `amount` of the currency with ISO 4217 code `code` (`GBP`, `eur`).
    pub fn new(amount: impl Into<Quantity>, code: &str) -> Currency {
        Currency {
            amount: amount.into(),
            code: code.trim().to_ascii_uppercase(),
            display: CurrencyDisplay::Symbol,
            accounting: false,
            locale: None,
        }
    }

    pub fn display(mut self, display: CurrencyDisplay) -> Currency {
        self.display = display;
        self
    }

    /// Negative amounts in parentheses where the locale writes them so
    /// (`($12.50)`), as a ledger does.
    pub fn accounting(mut self) -> Currency {
        self.accounting = true;
        self
    }

    /// The locale to write it for, instead of the person's
    /// (`LC_MONETARY`).
    pub fn locale(mut self, locale: &Locale) -> Currency {
        self.locale = Some(locale.clone());
        self
    }

    /// The text. A code that names no currency is written after the number
    /// (`12.50 XYZ`).
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Monetary));
        let Value::Decimal(decimal) = &self.amount.0 else {
            return format!(
                "{} {}",
                non_finite_of(&self.amount.0, Sign::Minus),
                self.code
            );
        };
        let mut decimal = decimal.clone();
        decimal.apply_sign_display(SignDisplay::Negative);
        let fallback = || {
            format!(
                "{} {}",
                decimal_text(&locale, Grouping::Auto, &decimal),
                self.code
            )
        };
        let Ok(code) = CurrencyType::try_from_str(&self.code.to_ascii_lowercase()) else {
            return fallback();
        };
        let key = format!(
            "currency|{}|{}|{:?}|{}",
            locale.tag(),
            self.code,
            self.display,
            self.accounting
        );
        let usage = if self.accounting {
            CurrencyUsage::Accounting
        } else {
            CurrencyUsage::Standard
        };
        let options = CurrencyFormatterOptions::from(usage);
        let formatter = cached(&key, || {
            let prefs = locale.icu().into();
            match self.display {
                CurrencyDisplay::Symbol => CurrencyFormatter::try_new_symbol(prefs, code, options),
                CurrencyDisplay::Narrow => {
                    CurrencyFormatter::try_new_symbol_narrow(prefs, code, options)
                }
                CurrencyDisplay::Code => CurrencyFormatter::try_new_code(prefs, code, options),
                CurrencyDisplay::Name => CurrencyFormatter::try_new_name(prefs, code),
            }
            .ok()
        });
        match formatter {
            Some(formatter) => formatter.format_fixed_decimal(&decimal).to_string(),
            None => fallback(),
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
