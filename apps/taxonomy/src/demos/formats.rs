//! Locale and formats: numbers, sizes, dates, relative times, lists and
//! counts, written for ten languages at once. Every demo fixes its instant
//! and its locales, so a screenshot reads the same on every machine.

use std::time::Duration;

use bingux_ui::format::{
    Bytes, Compact, Currency, CurrencyDisplay, Date, DatePart, HourCycle, List, Measure, Moment,
    Number, Percent, Plural, PluralKind, Quantity, Recent, Relative, Sign, Time, TimePrecision,
    TimeUnit, Timespan, Unit, Width, YearDisplay, Zone, plural_category,
};
use bingux_ui::locale::{Category, Locale};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::{rows, with};
use crate::model::Spec;

/// The locales every table shows: two English, the other European writing
/// systems, a CJK one and three that use other digits or calendars.
const TAGS: [&str; 10] = [
    "en-GB", "en-US", "de-DE", "fr-FR", "es-ES", "ru-RU", "ja-JP", "ar-EG", "hi-IN", "th-TH",
];

/// 2025-11-09 14:53:20 UTC, a Sunday.
fn now() -> Moment {
    Moment::from_unix(1_762_700_000)
}

fn locale(tag: &str) -> Locale {
    Locale::parse(tag).expect("a tag of the table")
}

/// One row per locale, the title its tag and the value what `format` writes.
fn table(format: impl Fn(&Locale) -> String) -> Element {
    rows(
        TAGS.iter()
            // A non-breaking hyphen: a long value must not wrap `th-TH` at its dash.
            .map(|tag| {
                property_row(tag.replace('-', "\u{2011}"), format(&locale(tag))).into_element()
            })
            .collect(),
    )
}

pub const NUMBERS: &[Spec] = &[
    Spec::forced("1234567.891", || {
        table(|l| Number::new(1_234_567.891).locale(l).format())
    }),
    Spec::forced("1234, then 12345 (Spanish groups from five digits)", || {
        table(|l| {
            format!(
                "{}   {}",
                Number::new(1234).locale(l),
                Number::new(12_345).locale(l)
            )
        })
    }),
    Spec::forced("Compact, short and long: 1234567", || {
        table(|l| {
            format!(
                "{}   {}",
                Number::new(1_234_567).compact(Compact::Short).locale(l),
                Number::new(1_234_567).compact(Compact::Long).locale(l)
            )
        })
    }),
    Spec::forced("Percent: 0.256", || {
        table(|l| {
            format!(
                "{}   {}",
                Percent::new(0.256).locale(l),
                Percent::new(0.256).max_fraction_digits(1).locale(l)
            )
        })
    }),
    Spec::forced("Rounding: ties go away from zero, no minus zero", || {
        let en = locale("en-GB");
        rows(vec![
            property_row(
                "0.125 to 2 digits",
                Number::new(0.125)
                    .max_fraction_digits(2)
                    .locale(&en)
                    .format(),
            )
            .into_element(),
            property_row(
                "2.5 to 0 digits",
                Number::new(2.5).max_fraction_digits(0).locale(&en).format(),
            )
            .into_element(),
            property_row(
                "-2.5 to 0 digits",
                Number::new(-2.5)
                    .max_fraction_digits(0)
                    .locale(&en)
                    .format(),
            )
            .into_element(),
            property_row(
                "-0.001 to 0 digits",
                Number::new(-0.001)
                    .max_fraction_digits(0)
                    .locale(&en)
                    .format(),
            )
            .into_element(),
            property_row("0.1 + 0.2", Number::new(0.1 + 0.2).locale(&en).format()).into_element(),
            property_row(
                "1234.5 to 3 significant digits",
                Number::new(1234.5)
                    .significant_digits(3)
                    .locale(&en)
                    .format(),
            )
            .into_element(),
            property_row(
                "5 with a sign",
                Number::new(5).sign(Sign::Always).locale(&en).format(),
            )
            .into_element(),
            property_row(
                "NaN and infinity",
                format!(
                    "{}   {}",
                    Number::new(f64::NAN).locale(&en),
                    Number::new(f64::INFINITY).locale(&en)
                ),
            )
            .into_element(),
        ])
    }),
    Spec::live("Type a number", || {
        let text = use_state(|| "1234567.891".to_string());
        let quantity = Quantity::parse(&text.read());
        let mut column = vec![with(
            "Number",
            text_field(text, |_: String| {}).placeholder("1234567.891"),
        )];
        column.push(table(|l| match &quantity {
            Some(q) => Number::new(q.clone()).locale(l).format(),
            None => "Not a number".to_string(),
        }));
        vstack(S3).children(column).into_element()
    }),
];

pub const MONEY: &[Spec] = &[
    Spec::forced("1234.50 pounds and 1234.50 euros", || {
        table(|l| {
            format!(
                "{}   {}",
                Currency::new(1234.5, "GBP").locale(l),
                Currency::new(1234.5, "EUR").locale(l)
            )
        })
    }),
    Spec::forced("1234 yen has no minor unit", || {
        table(|l| Currency::new(1234, "JPY").locale(l).format())
    }),
    Spec::forced("Symbol, narrow, code, name and accounting (en-US)", || {
        let en = locale("en-US");
        let money = |display| {
            Currency::new(-1234.5, "USD")
                .display(display)
                .locale(&en)
                .format()
        };
        rows(vec![
            property_row("Symbol", money(CurrencyDisplay::Symbol)).into_element(),
            property_row("Narrow", money(CurrencyDisplay::Narrow)).into_element(),
            property_row("Code", money(CurrencyDisplay::Code)).into_element(),
            property_row("Name", money(CurrencyDisplay::Name)).into_element(),
            property_row(
                "Accounting",
                Currency::new(-1234.5, "USD")
                    .accounting()
                    .locale(&en)
                    .format(),
            )
            .into_element(),
            property_row(
                "Not a currency",
                Currency::new(12.5, "A").locale(&en).format(),
            )
            .into_element(),
        ])
    }),
];

pub const SIZES: &[Spec] = &[
    Spec::forced("512 bytes, 19 000, 2 400 000", || {
        table(|l| {
            format!(
                "{}   {}   {}",
                Bytes::new(512).locale(l),
                Bytes::new(19_000).locale(l),
                Bytes::new(2_400_000).locale(l)
            )
        })
    }),
    Spec::forced(
        "Rolling over: 999 499, 999 500 and binary 1 048 575",
        || {
            let en = locale("en-GB");
            rows(vec![
                property_row("999 499 bytes", Bytes::new(999_499).locale(&en).format())
                    .into_element(),
                property_row("999 500 bytes", Bytes::new(999_500).locale(&en).format())
                    .into_element(),
                property_row(
                    "1 048 575 bytes, binary",
                    Bytes::new(1_048_575).binary().locale(&en).format(),
                )
                .into_element(),
                property_row(
                    "The largest size",
                    Bytes::new(u64::MAX).locale(&en).format(),
                )
                .into_element(),
                property_row(
                    "In words",
                    format!(
                        "{}   {}",
                        Bytes::new(1).words().locale(&en),
                        Bytes::new(512).words().locale(&en)
                    ),
                )
                .into_element(),
            ])
        },
    ),
    Spec::forced("Units: 21.5 °C, 12 km/h, 1013 hPa, 45 %", || {
        table(|l| {
            format!(
                "{}   {}   {}   {}",
                Measure::new(21.5, Unit::Celsius).locale(l),
                Measure::new(12, Unit::KilometresPerHour).locale(l),
                Measure::new(1013, Unit::Hectopascal).locale(l),
                Measure::new(45, Unit::Percent).locale(l)
            )
        })
    }),
    Spec::forced(
        "A span of 1 h 3 min 45 s: worded, long and as a clock",
        || {
            let span = Timespan::new(Duration::from_secs(3825));
            table(|l| {
                format!(
                    "{}   {}   {}",
                    span.clone().locale(l),
                    span.clone().width(Width::Long).locale(l),
                    span.clone().digital().locale(l)
                )
            })
        },
    ),
    Spec::forced("Time units named by the language: 1, 2 and 5 hours", || {
        table(|l| {
            [1, 2, 5]
                .map(|n| {
                    Measure::new(n, TimeUnit::Hour)
                        .width(Width::Long)
                        .locale(l)
                        .format()
                })
                .join("   ")
        })
    }),
];

pub const DATES: &[Spec] = &[
    Spec::forced("Short, medium and long date", || {
        table(|l| {
            let date = Date::new(now()).zone(Zone::Utc).locale(l);
            format!(
                "{}   {}   {}",
                date.clone().short(),
                date.clone().medium(),
                date.long()
            )
        })
    }),
    Spec::forced("Long with the weekday", || {
        table(|l| {
            Date::new(now())
                .zone(Zone::Utc)
                .long()
                .weekday()
                .locale(l)
                .format()
        })
    }),
    Spec::forced("Date and time, then the time with seconds", || {
        table(|l| {
            format!(
                "{}   {}",
                Date::new(now())
                    .zone(Zone::Utc)
                    .with_time(TimePrecision::Minutes)
                    .locale(l),
                Time::new(now()).zone(Zone::Utc).seconds().locale(l)
            )
        })
    }),
    Spec::forced(
        "Parts: month and year, weekday, month, day and month",
        || {
            table(|l| {
                [
                    DatePart::MonthYear,
                    DatePart::Weekday,
                    DatePart::Month,
                    DatePart::DayMonth,
                ]
                .map(|part| {
                    Date::new(now())
                        .zone(Zone::Utc)
                        .part(part)
                        .locale(l)
                        .format()
                })
                .join("   ")
            })
        },
    ),
    Spec::forced(
        "The clock: 12 hour, 24 hour, the locale's own (en-US)",
        || {
            let en = locale("en-US");
            let at = |cycle| {
                Time::new(now())
                    .zone(Zone::Utc)
                    .hour_cycle(cycle)
                    .locale(&en)
                    .format()
            };
            rows(vec![
                property_row("Locale", at(HourCycle::Locale)).into_element(),
                property_row("12 hour", at(HourCycle::Hours12)).into_element(),
                property_row("24 hour", at(HourCycle::Hours24)).into_element(),
            ])
        },
    ),
    Spec::forced("Zones: one instant, four wall clocks", || {
        let en = locale("en-GB");
        let at = |zone: Zone| {
            Date::new(now())
                .zone(zone)
                .with_time(TimePrecision::Minutes)
                .locale(&en)
                .format()
        };
        let named = |id: &str| Zone::named(id).expect("a zone of the database");
        rows(vec![
            property_row("UTC", at(Zone::Utc)).into_element(),
            property_row("Europe/London", at(named("Europe/London"))).into_element(),
            property_row("Asia/Kolkata", at(named("Asia/Kolkata"))).into_element(),
            property_row("Pacific/Auckland (the 10th)", at(named("Pacific/Auckland")))
                .into_element(),
            property_row(
                "Year left off in the current year",
                Date::new(now())
                    .zone(Zone::Utc)
                    .year(YearDisplay::IfNotCurrent)
                    .now(now())
                    .locale(&en)
                    .format(),
            )
            .into_element(),
        ])
    }),
];

pub const RELATIVE: &[Spec] = &[
    Spec::forced("2 minutes ago, in 2 hours, yesterday, 2 weeks ago", || {
        table(|l| {
            [-150, 7200, -86_400, -1_209_600]
                .map(|s| {
                    Relative::new(now().plus_seconds(s))
                        .now(now())
                        .zone(Zone::Utc)
                        .locale(l)
                        .format()
                })
                .join("   ")
        })
    }),
    Spec::forced("Short and narrow (en-US): 2 minutes ago", || {
        let en = locale("en-US");
        let told = |width| {
            Relative::new(now().plus_seconds(-150))
                .now(now())
                .width(width)
                .locale(&en)
                .format()
        };
        rows(vec![
            property_row("Long", told(Width::Long)).into_element(),
            property_row("Short", told(Width::Short)).into_element(),
            property_row("Narrow", told(Width::Narrow)).into_element(),
        ])
    }),
    Spec::forced(
        "A cell in a list (en-GB): the time, yesterday, the date",
        || {
            let en = locale("en-GB");
            let cell = |s: i64| {
                Recent::new(now().plus_seconds(s))
                    .now(now())
                    .zone(Zone::Utc)
                    .locale(&en)
                    .format()
            };
            rows(vec![
                property_row("30 seconds ago", cell(-30)).into_element(),
                property_row("An hour ago", cell(-3600)).into_element(),
                property_row("A day ago", cell(-86_400)).into_element(),
                property_row("40 days ago", cell(-86_400 * 40)).into_element(),
                property_row("400 days ago", cell(-86_400 * 400)).into_element(),
            ])
        },
    ),
];

pub const COUNTS: &[Spec] = &[
    Spec::forced("A list of three: and, or", || {
        table(|l| {
            format!(
                "{}   {}",
                List::and(["A", "B", "C"]).locale(l),
                List::or(["A", "B", "C"]).locale(l)
            )
        })
    }),
    Spec::forced("Plural classes of 0, 1, 2, 3, 5 and 11", || {
        table(|l| {
            [0, 1, 2, 3, 5, 11]
                .map(|n| format!("{:?}", plural_category(n, PluralKind::Cardinal, Some(l))))
                .join(" ")
        })
    }),
    Spec::forced(
        "A count with its noun (en-GB), and in order (1st, 2nd, 3rd)",
        || {
            let en = locale("en-GB");
            let files = |n: i64| {
                Plural::new(n, "{n} files")
                    .none("No files")
                    .one("{n} file")
                    .locale(&en)
                    .format()
            };
            let place = |n: i64| {
                Plural::new(n, "{n}th")
                    .one("{n}st")
                    .two("{n}nd")
                    .few("{n}rd")
                    .ordinal()
                    .locale(&en)
                    .format()
            };
            rows(vec![
                property_row(
                    "0, 1 and 1200",
                    format!("{}   {}   {}", files(0), files(1), files(1200)),
                )
                .into_element(),
                property_row(
                    "1 to 4, 11, 21",
                    [1, 2, 3, 4, 11, 21].map(place).join("   "),
                )
                .into_element(),
            ])
        },
    ),
];

pub const LOCALE: &[Spec] = &[
    Spec::forced("What the environment says", || {
        let current = |category| Locale::for_category(category).tag();
        rows(vec![
            property_row(
                "Messages (LC_MESSAGES, LANGUAGE)",
                current(Category::Messages),
            )
            .into_element(),
            property_row("Numbers (LC_NUMERIC)", current(Category::Numeric)).into_element(),
            property_row("Dates and times (LC_TIME)", current(Category::Time)).into_element(),
            property_row("Money (LC_MONETARY)", current(Category::Monetary)).into_element(),
            property_row(
                "Preferred languages",
                Locale::preferred()
                    .iter()
                    .map(Locale::tag)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .into_element(),
        ])
    }),
    Spec::live("Pick a locale: one value in every formatter", || {
        let mut tag = use_state(|| "de-DE".to_string());
        let l = locale(&tag.read());
        let options: Vec<(String, String)> = TAGS
            .iter()
            .map(|tag| (tag.to_string(), tag.to_string()))
            .collect();
        vstack(S3)
            .child(with(
                "Locale",
                dropdown(options, tag.read().clone(), move |key: String| tag.set(key)),
            ))
            .child(rows(vec![
                property_row("Number", Number::new(1_234_567.891).locale(&l).format())
                    .into_element(),
                property_row("Percent", Percent::new(0.256).locale(&l).format()).into_element(),
                property_row("Money", Currency::new(1234.5, "EUR").locale(&l).format())
                    .into_element(),
                property_row("Size", Bytes::new(2_400_000).locale(&l).format()).into_element(),
                property_row(
                    "Date",
                    Date::new(now())
                        .zone(Zone::Utc)
                        .long()
                        .weekday()
                        .locale(&l)
                        .format(),
                )
                .into_element(),
                property_row("Time", Time::new(now()).zone(Zone::Utc).locale(&l).format())
                    .into_element(),
                property_row(
                    "Relative",
                    Relative::new(now().plus_seconds(-150))
                        .now(now())
                        .locale(&l)
                        .format(),
                )
                .into_element(),
                property_row("List", List::and(["A", "B", "C"]).locale(&l).format()).into_element(),
            ]))
            .into_element()
    }),
];
