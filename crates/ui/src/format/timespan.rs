//! How long something lasts: "2 hr, 3 min", "1:03:45".

use std::fmt;
use std::time::Duration;

use fixed_decimal::Decimal;
use icu_experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu_experimental::dimension::units::options::{UnitsFormatterOptions, Width as IcuWidth};
use icu_experimental::measure::category::{CategorizedMeasureUnit, Duration as IcuDuration};

use super::Width;
use super::cache::cached;
use super::list::List;
use super::number::{Grouping, decimal_text};
use crate::locale::{Category, Locale};

/// A unit of time with names in every language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// `value` with the name of `unit`: `2 hr`, `1 minute`, `3 h`. `None` when
/// the data has no name for the unit in the language.
pub(crate) fn time_unit_text(
    locale: &Locale,
    width: Width,
    unit: TimeUnit,
    value: &Decimal,
) -> Option<String> {
    let key = format!("timeunit|{}|{width:?}|{unit:?}", locale.tag());
    let formatter = cached(&key, || {
        let prefs = locale.icu().into();
        let options = UnitsFormatterOptions::from(match width {
            Width::Long => IcuWidth::Long,
            Width::Short => IcuWidth::Short,
            Width::Narrow => IcuWidth::Narrow,
        });
        let make: fn() -> CategorizedMeasureUnit<IcuDuration> = match unit {
            TimeUnit::Millisecond => IcuDuration::millisecond,
            TimeUnit::Second => IcuDuration::second,
            TimeUnit::Minute => IcuDuration::minute,
            TimeUnit::Hour => IcuDuration::hour,
            TimeUnit::Day => IcuDuration::day,
            TimeUnit::Week => IcuDuration::week,
            TimeUnit::Month => IcuDuration::month,
            TimeUnit::Year => IcuDuration::year,
        };
        CategorizedFormatter::<IcuDuration>::try_new_core(prefs, make(), options)
            .or_else(|_| {
                CategorizedFormatter::<IcuDuration>::try_new_extended(prefs, make(), options)
            })
            .or_else(|_| {
                CategorizedFormatter::<IcuDuration>::try_new_outlier(prefs, make(), options)
            })
            .ok()
    })?;
    Some(formatter.format_fixed_decimal(value).to_string())
}

/// The length of a time span, as a person says it or as a player shows it.
///
/// ```
/// use std::time::Duration;
/// use bingux_ui::format::{Timespan, Width};
/// use bingux_ui::locale::Locale;
///
/// let en = Locale::parse("en-GB").unwrap();
/// let span = Timespan::new(Duration::from_secs(3825)).locale(&en);
/// assert_eq!(span.clone().to_string(), "1 hr, 3 min");
/// assert_eq!(span.clone().width(Width::Long).to_string(), "1 hour, 3 minutes");
/// assert_eq!(span.digital().to_string(), "1:03:45");
/// ```
#[derive(Clone, Debug)]
pub struct Timespan {
    span: Duration,
    width: Width,
    digital: bool,
    max_units: u8,
    locale: Option<Locale>,
}

impl Timespan {
    pub fn new(span: Duration) -> Timespan {
        Timespan {
            span,
            width: Width::Short,
            digital: false,
            max_units: 2,
            locale: None,
        }
    }

    /// `seconds` long; a negative or not-a-number length is none.
    pub fn from_secs_f64(seconds: f64) -> Timespan {
        Timespan::new(if seconds.is_finite() && seconds > 0.0 {
            Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX)
        } else {
            Duration::ZERO
        })
    }

    /// `Short` is `1 hr, 3 min`, `Long` `1 hour, 3 minutes`, `Narrow`
    /// `1h 3m`.
    pub fn width(mut self, width: Width) -> Timespan {
        self.width = width;
        self
    }

    /// As a clock: `3:45`, `1:03:45`. Hours show only when there is one; a
    /// span of a day or more counts hours past 24 (`25:00:00`).
    pub fn digital(mut self) -> Timespan {
        self.digital = true;
        self
    }

    /// The most units a worded span has (two by default: `1 hr, 3 min`, not
    /// `1 hr, 3 min, 45 sec`). A unit that is zero is left out.
    pub fn max_units(mut self, units: u8) -> Timespan {
        self.max_units = units.max(1);
        self
    }

    /// The locale to write it for, instead of the person's language
    /// (`LC_MESSAGES`).
    pub fn locale(mut self, locale: &Locale) -> Timespan {
        self.locale = Some(locale.clone());
        self
    }

    /// The text, to the nearest second.
    pub fn format(&self) -> String {
        super::isolate(self.compose())
    }

    /// The text without the isolate [`format`](Self::format) adds in a
    /// right-to-left interface: for a formatter that writes it into its own.
    pub(super) fn compose(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Messages));
        let total = self
            .span
            .as_secs()
            .saturating_add(u64::from(self.span.subsec_millis() >= 500));
        let (days, hours, minutes, seconds) = (
            total / 86_400,
            total % 86_400 / 3600,
            total % 3600 / 60,
            total % 60,
        );
        if self.digital {
            let digits = |value: u64, pad: bool| {
                let mut decimal = Decimal::from(value);
                if pad {
                    decimal.pad_start(2);
                }
                decimal_text(&locale, Grouping::Never, &decimal)
            };
            let all_hours = total / 3600;
            return if all_hours > 0 {
                format!(
                    "{}:{}:{}",
                    digits(all_hours, false),
                    digits(minutes, true),
                    digits(seconds, true)
                )
            } else {
                format!("{}:{}", digits(minutes, false), digits(seconds, true))
            };
        }
        let fields = [
            (TimeUnit::Day, days),
            (TimeUnit::Hour, hours),
            (TimeUnit::Minute, minutes),
            (TimeUnit::Second, seconds),
        ];
        let first = fields.iter().position(|(_, value)| *value > 0);
        let parts: Vec<String> = match first {
            Some(first) => fields[first..]
                .iter()
                .take(usize::from(self.max_units))
                .filter(|(_, value)| *value > 0)
                .map(|(unit, value)| {
                    time_unit_text(&locale, self.width, *unit, &Decimal::from(*value))
                        .unwrap_or_else(|| value.to_string())
                })
                .collect(),
            None => vec![
                time_unit_text(&locale, self.width, TimeUnit::Second, &Decimal::from(0u8))
                    .unwrap_or_else(|| "0".to_string()),
            ],
        };
        List::units(parts)
            .width(self.width)
            .locale(&locale)
            .format()
    }
}

impl fmt::Display for Timespan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
