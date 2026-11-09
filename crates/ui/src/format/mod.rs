//! Text for numbers, money, dates, times, sizes, lists and counts, written the
//! way the person's language and region write them.
//!
//! Every formatter is a small builder over one value. Without `.locale(..)`
//! it reads the person's locale from the environment (see
//! [`crate::locale`]); with it, it writes for that locale, which is what a
//! test or a preview does. Each one is [`Display`](std::fmt::Display), so it
//! goes straight into `format!` or a label.
//!
//! | Need                       | Use                                   | `en-GB`                      |
//! | -------------------------- | ------------------------------------- | ---------------------------- |
//! | A number                   | [`Number::new(1234.5)`](Number)       | `1,234.5`                    |
//! | A short number             | `Number::new(1_200_000).compact(..)`  | `1.2M`                       |
//! | A percentage               | [`Percent::new(0.256)`](Percent)      | `26%`                        |
//! | Money                      | [`Currency::new(12.5, "GBP")`](Currency) | `£12.50`                  |
//! | A file size                | [`Bytes::new(2_400_000)`](Bytes)      | `2.4 MB`                     |
//! | A quantity with its unit   | [`Measure::new(21, Unit::Celsius)`](Measure) | `21°C`                |
//! | A date                     | [`Date::new(moment)`](Date)           | `9 Nov 2025`                 |
//! | A date and time            | `Date::new(moment).with_time(..)`     | `9 Nov 2025, 14:53`          |
//! | A time of day              | [`Time::new(moment)`](Time)           | `14:53`                      |
//! | Time from now              | [`Relative::new(moment)`](Relative)   | `2 minutes ago`              |
//! | A cell in a list of files  | [`Recent::new(moment)`](Recent)       | `14:03`, `Yesterday`, `12 Oct` |
//! | How long                   | [`Timespan::new(duration)`](Timespan) | `1 hr, 3 min`, `1:03:45`     |
//! | A list of words            | [`List::and(items)`](List)            | `A, B and C`                 |
//! | A count with its noun      | [`Plural::new(n, "{n} files")`](Plural) | `1 file`, `2 files`        |
//!
//! ```
//! use bingux_ui::format::{Bytes, Number};
//! use bingux_ui::locale::Locale;
//!
//! let de = Locale::parse("de-DE").unwrap();
//! assert_eq!(Number::new(1234.5).locale(&de).to_string(), "1.234,5");
//! assert_eq!(Bytes::new(2_400_000).locale(&de).to_string(), "2,4\u{a0}MB");
//! ```
//!
//! # Where the data comes from
//!
//! Number, date, list, plural, relative time, currency and time unit data is
//! Unicode CLDR through ICU4X, compiled into the program (Unicode licence):
//! it needs no file on the host and covers every locale CLDR does. Time zone
//! arithmetic, including daylight saving, uses `jiff` with a bundled copy of
//! the IANA zone database.
//!
//! # Things to know
//!
//! - **Time.** A [`Moment`] is a point in time with no zone. A [`Zone`] reads
//!   it as a wall clock: [`Zone::Local`] (the system's, the default),
//!   [`Zone::Utc`], a fixed offset or a named zone from [`Zone::named`].
//!   Calendar dates with no time use [`Date::civil`], which has no zone to
//!   get wrong. The calendar (Gregorian, Buddhist, Persian, Hijri, ...) and
//!   12 or 24 hour clock follow the locale; [`HourCycle`] overrides the clock.
//! - **Rounding** is half away from zero on the decimal text (`1.005` to two
//!   digits is `1.01`), as people round, and never goes through binary
//!   floating point beyond the float's own shortest text. Money and exact
//!   amounts pass [`Quantity::parse`].
//! - **Names that are not in CLDR's data.** Units other than time units use
//!   their international symbol at every width; byte sizes use `kB`, `MB` (and
//!   `ko`, `Mo` in French). A unit missing from a language's data is never an
//!   error: the formatter falls back to the root locale's conventions.
//! - **Text that is translated** (a message with a count or a date inside)
//!   belongs in the app's catalogue, where each language writes its own
//!   plural forms. These formatters write the values that go into it.

mod bytes;
mod cache;
mod datetime;
mod list;
mod measure;
mod moment;
mod number;
mod plural;
mod relative;
mod timespan;

pub use bytes::{ByteBase, Bytes};
pub use datetime::{Date, DatePart, HourCycle, Length, Time, TimePrecision, YearDisplay};
pub use list::List;
pub use measure::{Measure, Unit};
pub use moment::{Civil, Moment, Zone};
pub use number::{Compact, Currency, CurrencyDisplay, Grouping, Number, Percent, Quantity, Sign};
pub use plural::{Plural, PluralCategory, PluralKind, plural_category};
pub use relative::{Recent, Relative};
pub use timespan::{TimeUnit, Timespan};

/// How much of a name or phrase is written: for a relative time ("2 minutes
/// ago", "2 min. ago", "2m ago"), a list or a time span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Width {
    /// In full.
    #[default]
    Long,
    /// Abbreviated.
    Short,
    /// As short as the language allows; may be ambiguous.
    Narrow,
}

#[cfg(test)]
mod tests;
