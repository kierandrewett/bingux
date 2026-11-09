//! What can go wrong in formatting, first; each case below guards one.
//!
//! Numbers
//! - Rounding that goes the wrong way on a tie, or through binary floats:
//!   `1.005`, `0.125`, `2.5`, `-2.5`, `0.1 + 0.2`.
//! - A negative that rounds to zero written as `-0`.
//! - A number that rounds up into a new digit (`999.5` to `1,000`) losing its
//!   grouping; a compact number that rounds up into the next unit.
//! - NaN, infinities, the largest integers and floats of 1e21 and 1e-7
//!   (floats written with an exponent in some formatters).
//! - Grouping, digits and separators of one language leaking into another
//!   through the formatter cache.
//! - A currency code that is not a code; a currency with no minor unit.
//!
//! Sizes
//! - `999_949` bytes rounding to `1000 kB` instead of `1 MB`; `u64::MAX`;
//!   binary `1023.9 KiB`; zero and one byte.
//!
//! Time
//! - A clock change: 00:59 UTC and 01:00 UTC in London are 00:59 and 02:00.
//! - A time read in another zone than the date: the day changes with the zone.
//! - A date that does not exist; a time past the supported years; negative
//!   Unix times; an offset out of range.
//! - "Yesterday" decided by the calendar day, not by 24 hours.
//! - Locales whose calendar is not Gregorian.
//!
//! Words
//! - Plural classes that English does not have (Russian `few` and `many`,
//!   Arabic `zero`, `two`), a decimal count, an order (1st, 2nd, 11th).
//! - Lists of none, one and two items; a locale with a different rule for
//!   the last separator.
//! - A time span that rounds up into the next unit (59.6 seconds); a span
//!   shorter than a second; an absurdly long span.

use std::time::Duration;

use super::*;
use crate::locale::Locale;

fn loc(tag: &str) -> Locale {
    Locale::parse(tag).unwrap()
}

fn number(value: impl Into<Quantity>) -> Number {
    Number::new(value).locale(&loc("en-GB"))
}

const NOW: i64 = 1_762_700_000; // 2025-11-09T14:53:20Z, a Sunday

fn now() -> Moment {
    Moment::from_unix(NOW)
}

fn date(tag: &str) -> Date {
    Date::new(now()).zone(Zone::Utc).locale(&loc(tag))
}

// --- numbers ---------------------------------------------------------------

#[test]
fn ties_round_away_from_zero_on_the_decimal_text() {
    assert_eq!(number(0.125).fraction_digits(0, 2).to_string(), "0.13");
    assert_eq!(number(1.005).max_fraction_digits(2).to_string(), "1.01");
    assert_eq!(number(2.5).max_fraction_digits(0).to_string(), "3");
    assert_eq!(number(-2.5).max_fraction_digits(0).to_string(), "-3");
    // 0.1 + 0.2 is 0.30000000000000004 in binary; the default is three digits.
    assert_eq!(number(0.1 + 0.2).to_string(), "0.3");
    assert_eq!(number(0.1f32).to_string(), "0.1");
}

#[test]
fn a_negative_that_rounds_to_zero_has_no_minus() {
    assert_eq!(number(-0.001).max_fraction_digits(0).to_string(), "0");
    assert_eq!(number(-0.0).to_string(), "0");
    assert_eq!(
        number(-0.001)
            .max_fraction_digits(0)
            .sign(Sign::Always)
            .to_string(),
        "+0"
    );
    assert_eq!(
        number(-0.001)
            .max_fraction_digits(0)
            .sign(Sign::ExceptZero)
            .to_string(),
        "0"
    );
}

#[test]
fn signs_can_be_forced_or_dropped() {
    assert_eq!(number(5).sign(Sign::Always).to_string(), "+5");
    assert_eq!(number(0).sign(Sign::Always).to_string(), "+0");
    assert_eq!(number(5).sign(Sign::ExceptZero).to_string(), "+5");
    assert_eq!(number(-5).sign(Sign::Never).to_string(), "5");
    assert_eq!(number(-5).to_string(), "-5");
}

#[test]
fn rounding_up_into_a_new_digit_keeps_the_grouping() {
    assert_eq!(number(999.5).max_fraction_digits(0).to_string(), "1,000");
    assert_eq!(
        number(999_999.6).max_fraction_digits(0).to_string(),
        "1,000,000"
    );
}

#[test]
fn compact_numbers_roll_into_the_next_unit() {
    let us = loc("en-US");
    let short = |n: i64| {
        Number::new(n)
            .compact(Compact::Short)
            .locale(&us)
            .to_string()
    };
    assert_eq!(short(999), "999");
    assert_eq!(short(1_234), "1.2K");
    assert_eq!(short(999_499), "999K");
    assert_eq!(short(999_500), "1M");
    assert_eq!(short(12_345_678), "12M");
    assert_eq!(
        Number::new(1_234_567)
            .compact(Compact::Long)
            .locale(&us)
            .to_string(),
        "1.2 million"
    );
}

#[test]
fn not_a_number_and_the_infinities_are_written_not_dropped() {
    assert_eq!(number(f64::NAN).to_string(), "NaN");
    assert_eq!(number(f64::INFINITY).to_string(), "∞");
    assert_eq!(number(f64::NEG_INFINITY).to_string(), "-∞");
    assert_eq!(number(f64::INFINITY).sign(Sign::Always).to_string(), "+∞");
    assert_eq!(
        Percent::new(f64::NAN).locale(&loc("en-GB")).to_string(),
        "NaN%"
    );
}

#[test]
fn the_largest_values_and_floats_that_would_have_an_exponent() {
    assert_eq!(number(u64::MAX).to_string(), "18,446,744,073,709,551,615");
    assert_eq!(
        number(i128::MAX).to_string(),
        "170,141,183,460,469,231,731,687,303,715,884,105,727"
    );
    assert_eq!(number(1e21).to_string(), "1,000,000,000,000,000,000,000");
    assert_eq!(number(1e-7).to_string(), "0");
    assert_eq!(number(1e-7).max_fraction_digits(8).to_string(), "0.0000001");
    assert!(
        number(f64::MAX)
            .to_string()
            .starts_with("179,769,313,486,231,57")
    );
}

#[test]
fn significant_digits_count_from_the_first_non_zero() {
    assert_eq!(number(1234.5).significant_digits(3).to_string(), "1,230");
    assert_eq!(number(0.012345).significant_digits(3).to_string(), "0.0123");
    assert_eq!(number(0).significant_digits(3).to_string(), "0");
    assert_eq!(number(99.99).significant_digits(2).to_string(), "100");
}

#[test]
fn fraction_digits_pad_to_the_least() {
    assert_eq!(number(0.5).fraction_digits(2, 2).to_string(), "0.50");
    assert_eq!(number(3).fraction_digits(1, 2).to_string(), "3.0");
    assert_eq!(number(3.14159).fraction_digits(1, 2).to_string(), "3.14");
}

#[test]
fn exact_decimals_keep_every_digit_and_text_that_is_not_one_is_refused() {
    let exact = Quantity::parse("12.50").unwrap();
    assert_eq!(number(exact).fraction_digits(2, 2).to_string(), "12.50");
    let long = Quantity::parse("0.12345678901234567890").unwrap();
    assert_eq!(
        number(long).max_fraction_digits(20).to_string(),
        "0.1234567890123456789"
    );
    for text in ["", "abc", "1,5", "--1", "1 000"] {
        assert!(Quantity::parse(text).is_none(), "{text:?}");
    }
}

#[test]
fn grouping_follows_the_language_and_can_be_overridden() {
    let digits = |tag: &str, n: i64| Number::new(n).locale(&loc(tag)).to_string();
    assert_eq!(digits("en-GB", 1_234_567), "1,234,567");
    assert_eq!(digits("de-DE", 1_234_567), "1.234.567");
    assert_eq!(digits("de-CH", 1_234_567), "1'234'567");
    // Spanish groups only from five digits.
    assert_eq!(digits("es-ES", 1234), "1234");
    assert_eq!(digits("es-ES", 12_345), "12.345");
    // Indian grouping: the last three, then twos.
    assert_eq!(digits("hi-IN", 1_234_567), "12,34,567");
    assert_eq!(
        number(1_234_567).grouping(Grouping::Never).to_string(),
        "1234567"
    );
    assert_eq!(
        Number::new(1234)
            .grouping(Grouping::Min2)
            .locale(&loc("en-GB"))
            .to_string(),
        "1234"
    );
}

#[test]
fn another_script_has_its_own_digits_and_marks() {
    let arabic = Number::new(1234.5).locale(&loc("ar-EG")).to_string();
    assert_eq!(arabic, "١٬٢٣٤٫٥");
    let persian = Number::new(12).locale(&loc("fa-IR")).to_string();
    assert_eq!(persian, "۱۲");
}

#[test]
fn the_formatter_cache_does_not_mix_locales() {
    let (de, en) = (loc("de-DE"), loc("en-GB"));
    for _ in 0..3 {
        assert_eq!(Number::new(1234.5).locale(&de).to_string(), "1.234,5");
        assert_eq!(Number::new(1234.5).locale(&en).to_string(), "1,234.5");
    }
}

#[test]
fn a_locale_the_data_does_not_have_falls_back_to_the_root_conventions() {
    let unknown = loc("xx-YY");
    assert_eq!(Number::new(1234.5).locale(&unknown).to_string(), "1,234.5");
    assert!(
        !Date::new(now())
            .zone(Zone::Utc)
            .locale(&unknown)
            .to_string()
            .is_empty()
    );
    assert!(!Relative::new(now()).locale(&unknown).to_string().is_empty());
    assert_eq!(
        List::and(["a", "b", "c"]).locale(&unknown).to_string(),
        "a, b, c"
    );
}

// --- percent and money -----------------------------------------------------

#[test]
fn percentages_scale_the_fraction_and_round_like_numbers() {
    let en = loc("en-GB");
    assert_eq!(Percent::new(0.256).locale(&en).to_string(), "26%");
    assert_eq!(
        Percent::new(0.256)
            .max_fraction_digits(1)
            .locale(&en)
            .to_string(),
        "25.6%"
    );
    assert_eq!(Percent::new(0.005).locale(&en).to_string(), "1%");
    assert_eq!(Percent::new(1).locale(&en).to_string(), "100%");
    assert_eq!(Percent::new(12.5).locale(&en).to_string(), "1,250%");
    assert_eq!(Percent::new(-0.05).locale(&en).to_string(), "-5%");
    assert_eq!(Percent::new(0).locale(&en).to_string(), "0%");
    assert_eq!(
        Percent::new(0.5)
            .fraction_digits(1, 1)
            .locale(&en)
            .to_string(),
        "50.0%"
    );
}

#[test]
fn percent_signs_follow_the_language() {
    // A space before the sign in French, none in English.
    let fr = Percent::new(0.256).locale(&loc("fr-FR")).to_string();
    assert!(
        fr.starts_with("26") && fr.ends_with('%') && fr.chars().nth(2).unwrap().is_whitespace(),
        "{fr:?}"
    );
}

#[test]
fn money_uses_the_currency_digits_and_symbols_of_the_locale() {
    let (gb, us, ja) = (loc("en-GB"), loc("en-US"), loc("ja-JP"));
    assert_eq!(
        Currency::new(1234.5, "GBP").locale(&gb).to_string(),
        "£1,234.50"
    );
    assert_eq!(Currency::new(12.5, "usd").locale(&us).to_string(), "$12.50");
    // The yen has no minor unit.
    assert_eq!(
        Currency::new(1234, "JPY").locale(&ja).to_string(),
        "￥1,234"
    );
    assert_eq!(Currency::new(1234, "JPY").locale(&us).to_string(), "¥1,234");
    assert_eq!(
        Currency::new(12.345, "USD").locale(&us).to_string(),
        "$12.35"
    );
    assert_eq!(
        Currency::new(-12.5, "USD")
            .accounting()
            .locale(&us)
            .to_string(),
        "($12.50)"
    );
    assert_eq!(
        Currency::new(-12.5, "USD").locale(&us).to_string(),
        "-$12.50"
    );
    assert_eq!(
        Currency::new(1234.5, "USD")
            .display(CurrencyDisplay::Name)
            .locale(&us)
            .to_string(),
        "1,234.50 US dollars"
    );
    assert_eq!(
        Currency::new(12.5, "EUR")
            .display(CurrencyDisplay::Code)
            .locale(&us)
            .to_string(),
        "EUR\u{a0}12.50"
    );
}

#[test]
fn a_euro_in_germany_follows_the_number_then_the_sign() {
    assert_eq!(
        Currency::new(1234.5, "EUR")
            .locale(&loc("de-DE"))
            .to_string(),
        "1.234,50\u{a0}€"
    );
}

#[test]
fn a_currency_that_is_not_a_code_is_written_after_the_number() {
    let us = loc("en-US");
    assert_eq!(Currency::new(12.5, "A").locale(&us).to_string(), "12.5 A");
    assert_eq!(Currency::new(12.5, "").locale(&us).to_string(), "12.5 ");
    assert_eq!(
        Currency::new(f64::NAN, "USD").locale(&us).to_string(),
        "NaN USD"
    );
}

// --- sizes -----------------------------------------------------------------

fn size(bytes: u64) -> String {
    Bytes::new(bytes).locale(&loc("en-GB")).to_string()
}

#[test]
fn sizes_pick_the_largest_unit_and_one_digit_under_ten() {
    assert_eq!(size(0), "0\u{a0}B");
    assert_eq!(size(1), "1\u{a0}B");
    assert_eq!(size(999), "999\u{a0}B");
    assert_eq!(size(1000), "1\u{a0}kB");
    assert_eq!(size(1500), "1.5\u{a0}kB");
    assert_eq!(size(19_000), "19\u{a0}kB");
    assert_eq!(size(2_400_000), "2.4\u{a0}MB");
    assert_eq!(size(9_999), "10\u{a0}kB");
    assert_eq!(size(340_000), "340\u{a0}kB");
}

#[test]
fn a_size_that_rounds_up_to_a_thousand_moves_to_the_next_unit() {
    assert_eq!(size(999_499), "999\u{a0}kB");
    assert_eq!(size(999_500), "1\u{a0}MB");
    assert_eq!(size(999_949), "1\u{a0}MB");
    assert_eq!(size(999_949_999), "1\u{a0}GB");
}

#[test]
fn the_largest_size_stops_at_petabytes() {
    assert_eq!(size(u64::MAX), "18,447\u{a0}PB");
}

#[test]
fn binary_sizes_count_in_1024() {
    let binary = |n: u64| Bytes::new(n).binary().locale(&loc("en-GB")).to_string();
    assert_eq!(binary(1023), "1023\u{a0}B");
    assert_eq!(binary(1024), "1\u{a0}KiB");
    assert_eq!(binary(1536), "1.5\u{a0}KiB");
    assert_eq!(binary(2_400_000), "2.3\u{a0}MiB");
    // 1023.999 KiB rounds to 1024 KiB: that is 1 MiB.
    assert_eq!(binary(1_048_575), "1\u{a0}MiB");
}

#[test]
fn sizes_use_the_languages_digits_and_unit_names() {
    let de = Bytes::new(2_400_000).locale(&loc("de-DE")).to_string();
    assert_eq!(de, "2,4\u{a0}MB");
    let fr = Bytes::new(2_400_000).locale(&loc("fr-FR")).to_string();
    assert_eq!(fr, "2,4\u{a0}Mo");
    assert_eq!(
        Bytes::new(512).locale(&loc("fr-FR")).to_string(),
        "512\u{a0}o"
    );
    let ar = Bytes::new(19_000).locale(&loc("ar-EG")).to_string();
    assert!(ar.starts_with("١٩"), "{ar:?}");
}

#[test]
fn sizes_in_words_and_with_more_digits() {
    let en = loc("en-GB");
    assert_eq!(Bytes::new(0).words().locale(&en).to_string(), "0 bytes");
    assert_eq!(Bytes::new(1).words().locale(&en).to_string(), "1 byte");
    assert_eq!(Bytes::new(512).words().locale(&en).to_string(), "512 bytes");
    // Words are English only: another language keeps the symbol.
    assert_eq!(
        Bytes::new(1).words().locale(&loc("fr-FR")).to_string(),
        "1\u{a0}o"
    );
    assert_eq!(
        Bytes::new(2_345_678)
            .fraction_digits(2)
            .locale(&en)
            .to_string(),
        "2.35\u{a0}MB"
    );
}

// --- dates and times -------------------------------------------------------

#[test]
fn a_date_is_written_in_the_order_of_the_locale() {
    assert_eq!(date("en-GB").to_string(), "9 Nov 2025");
    assert_eq!(date("en-US").to_string(), "Nov 9, 2025");
    assert_eq!(date("de-DE").to_string(), "09.11.2025");
    assert_eq!(date("ja-JP").to_string(), "2025/11/09");
    assert_eq!(date("en-GB").short().to_string(), "09/11/2025");
    assert_eq!(date("en-US").short().to_string(), "11/9/25");
    assert_eq!(date("en-GB").long().to_string(), "9 November 2025");
    assert_eq!(
        date("en-GB").long().weekday().to_string(),
        "Sunday, 9 November 2025"
    );
    assert_eq!(date("en-GB").weekday().to_string(), "Sun, 9 Nov 2025");
}

#[test]
fn date_parts_can_be_asked_for_alone() {
    let part = |part| date("en-GB").part(part).to_string();
    assert_eq!(part(DatePart::MonthYear), "Nov 2025");
    assert_eq!(part(DatePart::Weekday), "Sun");
    assert_eq!(part(DatePart::Month), "Nov");
    assert_eq!(part(DatePart::DayMonth), "9 Nov");
    assert_eq!(part(DatePart::Year), "2025");
    assert_eq!(part(DatePart::Day), "9");
}

#[test]
fn the_year_can_be_left_off_in_the_current_year_only() {
    let this_year = date("en-GB").year(YearDisplay::IfNotCurrent).now(now());
    assert_eq!(this_year.to_string(), "9 Nov");
    let last_year = Date::new(now().plus_seconds(-86_400 * 365))
        .zone(Zone::Utc)
        .locale(&loc("en-GB"))
        .year(YearDisplay::IfNotCurrent)
        .now(now());
    assert_eq!(last_year.to_string(), "9 Nov 2024");
    assert_eq!(date("en-GB").year(YearDisplay::Never).to_string(), "9 Nov");
}

#[test]
fn the_clock_follows_the_locale_unless_told() {
    let at = |tag: &str, cycle| {
        Time::new(now())
            .zone(Zone::Utc)
            .locale(&loc(tag))
            .hour_cycle(cycle)
            .to_string()
    };
    assert_eq!(at("en-GB", HourCycle::Locale), "14:53");
    assert_eq!(at("en-US", HourCycle::Locale), "2:53\u{202f}PM");
    assert_eq!(at("en-US", HourCycle::Hours24), "14:53");
    assert!(
        at("en-GB", HourCycle::Hours12).starts_with("2:53"),
        "{}",
        at("en-GB", HourCycle::Hours12)
    );
    assert_eq!(
        Time::new(now())
            .zone(Zone::Utc)
            .seconds()
            .locale(&loc("en-GB"))
            .to_string(),
        "14:53:20"
    );
}

#[test]
fn a_time_of_day_with_no_date_and_a_date_and_time_together() {
    assert_eq!(
        Time::of_day(7, 5, 0).locale(&loc("en-GB")).to_string(),
        "07:05"
    );
    assert_eq!(Time::of_day(24, 0, 0).to_string(), "Invalid time");
    assert_eq!(
        date("en-GB").with_time(TimePrecision::Minutes).to_string(),
        "9 Nov 2025, 14:53"
    );
    assert_eq!(
        date("en-GB").with_time(TimePrecision::Seconds).to_string(),
        "9 Nov 2025, 14:53:20"
    );
    assert_eq!(
        date("en-GB")
            .long()
            .weekday()
            .with_time(TimePrecision::Minutes)
            .to_string(),
        "Sunday, 9 November 2025 at 14:53"
    );
}

#[test]
fn the_zone_decides_the_day_and_the_hour() {
    // 14:53 UTC is 20:23 in India (UTC+5:30), and 23:53 in the time zone
    // of Tokyo (UTC+9) on the same day, 01:53 on the 10th in UTC+11.
    let at = |zone: Zone| {
        Time::new(now())
            .zone(zone)
            .locale(&loc("en-GB"))
            .to_string()
    };
    assert_eq!(at(Zone::Offset(19_800)), "20:23");
    assert_eq!(at(Zone::Offset(-18_000)), "09:53");
    let day = |zone: Zone| {
        Date::new(now())
            .zone(zone)
            .locale(&loc("en-GB"))
            .to_string()
    };
    assert_eq!(day(Zone::Offset(11 * 3600)), "10 Nov 2025");
    assert_eq!(day(Zone::Offset(-14 * 3600)), "9 Nov 2025");
    assert_eq!(day(Zone::Offset(-15 * 3600)), "8 Nov 2025");
}

#[test]
fn a_clock_change_skips_and_repeats_an_hour() {
    let london = Zone::named("Europe/London").unwrap();
    let at = |unix: i64| {
        Time::new(Moment::from_unix(unix))
            .zone(london.clone())
            .locale(&loc("en-GB"))
            .to_string()
    };
    // 2026-03-29: the clocks go from 01:00 to 02:00 at 01:00 UTC.
    assert_eq!(at(1_774_746_000 - 60), "00:59");
    assert_eq!(at(1_774_746_000), "02:00");
    assert_eq!(london.offset_at(Moment::from_unix(1_774_746_000 - 60)), 0);
    assert_eq!(london.offset_at(Moment::from_unix(1_774_746_000)), 3600);
    // The skipped 01:30 is moved forward by the gap: 02:30 summer time.
    let gap = Civil {
        year: 2026,
        month: 3,
        day: 29,
        hour: 1,
        minute: 30,
        second: 0,
        nanosecond: 0,
    };
    let moment = london.moment_of(gap).unwrap();
    assert_eq!(at(moment.unix_seconds()), "02:30");
    // 2026-10-25: the clocks go back at 01:00 UTC, so 01:30 happens twice;
    // the first reading is meant.
    let repeated = Civil {
        year: 2026,
        month: 10,
        day: 25,
        hour: 1,
        minute: 30,
        second: 0,
        nanosecond: 0,
    };
    let first = london.moment_of(repeated).unwrap();
    assert_eq!(first.unix_seconds(), 1_792_888_200);
}

#[test]
fn zones_that_do_not_exist_are_refused_and_offsets_out_of_range_read_as_utc() {
    assert!(Zone::named("Not/AZone").is_none());
    assert!(Zone::named("Europe/Paris").is_some());
    assert_eq!(
        Time::new(now())
            .zone(Zone::Offset(i32::MAX))
            .locale(&loc("en-GB"))
            .to_string(),
        "14:53"
    );
}

#[test]
fn a_date_that_does_not_exist_is_not_a_date() {
    let civil = |y, m, d| Date::civil(y, m, d).locale(&loc("en-GB")).to_string();
    assert_eq!(civil(2024, 2, 29), "29 Feb 2024");
    assert_eq!(civil(2025, 2, 29), "Invalid date");
    assert_eq!(civil(2025, 13, 1), "Invalid date");
    assert_eq!(civil(2025, 4, 31), "Invalid date");
    assert_eq!(civil(2025, 0, 1), "Invalid date");
    assert_eq!(civil(1900, 2, 29), "Invalid date");
    assert_eq!(civil(2000, 2, 29), "29 Feb 2000");
}

#[test]
fn times_before_the_epoch_and_beyond_the_years_are_written_not_wrapped() {
    let en = loc("en-GB");
    let at = |unix: i64| {
        Date::new(Moment::from_unix(unix))
            .zone(Zone::Utc)
            .locale(&en)
            .to_string()
    };
    assert_eq!(at(0), "1 Jan 1970");
    assert_eq!(at(-1), "31 Dec 1969");
    assert_eq!(at(-86_400 * 36_500), "25 Jan 1870");
    // Outside year 9999 the moment is clamped to the edge, not wrapped.
    assert!(at(i64::MAX).contains("9999"), "{}", at(i64::MAX));
    assert!(at(i64::MIN).contains("BC"), "{}", at(i64::MIN));
    assert_eq!(
        Moment::from_unix(i64::MAX).unix_seconds(),
        Moment::from_unix(i64::MAX - 1).unix_seconds()
    );
}

#[test]
fn calendars_other_than_gregorian_follow_the_locale() {
    assert_eq!(date("th-TH").to_string(), "9 พ.ย. 2568");
    assert_eq!(date("fa-IR").short().to_string(), "۱۴۰۴/۸/۱۸");
}

#[test]
fn year_boundaries_read_in_the_zone() {
    // 2025-12-31 23:59:59 UTC is already 2026 in UTC+1.
    let moment = Moment::from_unix(1_767_225_599);
    let en = loc("en-GB");
    assert_eq!(
        Date::new(moment).zone(Zone::Utc).locale(&en).to_string(),
        "31 Dec 2025"
    );
    assert_eq!(
        Date::new(moment)
            .zone(Zone::Offset(3600))
            .locale(&en)
            .to_string(),
        "1 Jan 2026"
    );
}

// --- relative times --------------------------------------------------------

fn ago(tag: &str, seconds: i64) -> String {
    Relative::new(now().plus_seconds(seconds))
        .now(now())
        .zone(Zone::Utc)
        .locale(&loc(tag))
        .to_string()
}

#[test]
fn relative_times_in_english() {
    assert_eq!(ago("en-GB", 0), "now");
    assert_eq!(ago("en-GB", -2), "now");
    assert_eq!(ago("en-GB", -30), "30 seconds ago");
    assert_eq!(ago("en-GB", -150), "2 minutes ago");
    assert_eq!(ago("en-GB", 120), "in 2 minutes");
    assert_eq!(ago("en-GB", -7200), "2 hours ago");
    assert_eq!(ago("en-GB", -86_400), "yesterday");
    assert_eq!(ago("en-GB", 86_400), "tomorrow");
    assert_eq!(ago("en-GB", -86_400 * 3), "3 days ago");
    assert_eq!(ago("en-GB", -86_400 * 14), "2 weeks ago");
    assert_eq!(ago("en-GB", -86_400 * 70), "2 months ago");
    assert_eq!(ago("en-GB", -86_400 * 800), "2 years ago");
}

#[test]
fn relative_times_can_be_short_narrow_and_always_numeric() {
    let two_minutes = now().plus_seconds(-150);
    let told = |width| {
        Relative::new(two_minutes)
            .now(now())
            .locale(&loc("en-US"))
            .width(width)
            .to_string()
    };
    assert_eq!(told(Width::Long), "2 minutes ago");
    assert_eq!(told(Width::Short), "2 min. ago");
    assert_eq!(told(Width::Narrow), "2m ago");
    let yesterday = Relative::new(now().plus_seconds(-86_400))
        .now(now())
        .zone(Zone::Utc)
        .numeric()
        .locale(&loc("en-GB"));
    assert_eq!(yesterday.to_string(), "1 day ago");
}

#[test]
fn relative_times_in_other_languages_have_their_own_plurals_and_words() {
    assert_eq!(ago("de-DE", -150), "vor 2 Minuten");
    assert_eq!(ago("de-DE", -86_400 * 2), "vorgestern");
    assert_eq!(ago("fr-FR", -86_400), "hier");
    assert_eq!(ago("es-ES", 86_400 * 2), "pasado mañana");
    assert_eq!(ago("ru-RU", -150), "2 минуты назад");
    assert_eq!(ago("ru-RU", -300), "5 минут назад");
    assert_eq!(ago("ja-JP", -150), "2 分前");
}

#[test]
fn a_list_cell_says_the_time_today_then_yesterday_then_the_date() {
    let recent = |seconds: i64| {
        Recent::new(now().plus_seconds(seconds))
            .now(now())
            .zone(Zone::Utc)
            .locale(&loc("en-GB"))
            .to_string()
    };
    assert_eq!(recent(-30), "Now");
    assert_eq!(recent(-3600), "13:53");
    assert_eq!(recent(3600), "15:53");
    assert_eq!(recent(-86_400), "Yesterday");
    assert_eq!(recent(86_400), "Tomorrow");
    assert_eq!(recent(-86_400 * 40), "30 Sept");
    assert_eq!(recent(-86_400 * 400), "5 Oct 2024");
    // 20 minutes ago across midnight is yesterday, not "20 minutes ago".
    let just_after_midnight = Moment::from_unix(1_762_646_400 + 600); // 00:10
    let then = just_after_midnight.plus_seconds(-1200);
    let told = Recent::new(then)
        .now(just_after_midnight)
        .zone(Zone::Utc)
        .locale(&loc("en-GB"));
    assert_eq!(told.to_string(), "Yesterday");
}

// --- lists and counts ------------------------------------------------------

#[test]
fn lists_of_none_one_two_and_many() {
    let en = loc("en-GB");
    let list = |items: &[&str]| List::and(items.iter().copied()).locale(&en).to_string();
    assert_eq!(list(&[]), "");
    assert_eq!(list(&["A"]), "A");
    assert_eq!(list(&["A", "B"]), "A and B");
    assert_eq!(list(&["A", "B", "C"]), "A, B and C");
    assert_eq!(
        List::and(["A", "B", "C"]).locale(&loc("en-US")).to_string(),
        "A, B, and C"
    );
    assert_eq!(
        List::or(["A", "B", "C"]).locale(&en).to_string(),
        "A, B or C"
    );
}

#[test]
fn lists_follow_the_conjunction_rules_of_the_language() {
    assert_eq!(
        List::and(["x", "Ibiza"]).locale(&loc("es-ES")).to_string(),
        "x e Ibiza"
    );
    assert_eq!(
        List::and(["A", "B", "C"]).locale(&loc("ja-JP")).to_string(),
        "A、B、C"
    );
    assert_eq!(
        List::and(["A", "B", "C"]).locale(&loc("de-DE")).to_string(),
        "A, B und C"
    );
    assert_eq!(
        List::and(["A", "B"])
            .width(Width::Narrow)
            .locale(&loc("en-GB"))
            .to_string(),
        "A, B"
    );
}

fn category(tag: &str, count: i64) -> PluralCategory {
    plural_category(count, PluralKind::Cardinal, Some(&loc(tag)))
}

#[test]
fn plural_classes_that_english_does_not_have() {
    use PluralCategory::*;
    assert_eq!(
        [0, 1, 2, 5].map(|n| category("en-GB", n)),
        [Other, One, Other, Other]
    );
    // Russian: 1 and 21 are one, 2 to 4 few, 5 to 20 many.
    assert_eq!(
        [1, 2, 4, 5, 11, 12, 21, 22, 25, 101].map(|n| category("ru-RU", n)),
        [One, Few, Few, Many, Many, Many, One, Few, Many, One]
    );
    // Arabic has all six.
    assert_eq!(
        [0, 1, 2, 3, 11, 100].map(|n| category("ar-EG", n)),
        [Zero, One, Two, Few, Many, Other]
    );
    // Japanese has none.
    assert_eq!([0, 1, 2, 100].map(|n| category("ja-JP", n)), [Other; 4]);
}

#[test]
fn ordinals_have_their_own_classes() {
    use PluralCategory::*;
    let ordinal = |n: i64| plural_category(n, PluralKind::Ordinal, Some(&loc("en-GB")));
    assert_eq!(
        [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111].map(ordinal),
        [
            One, Two, Few, Other, Other, Other, Other, One, Two, Few, One, Other
        ]
    );
}

#[test]
fn a_decimal_count_is_read_with_its_digits() {
    use PluralCategory::*;
    let en = loc("en-GB");
    let of = |text: &str| {
        plural_category(
            Quantity::parse(text).unwrap(),
            PluralKind::Cardinal,
            Some(&en),
        )
    };
    assert_eq!(
        [of("1"), of("1.0"), of("1.5"), of("0")],
        [One, Other, Other, Other]
    );
    assert_eq!(
        plural_category(f64::NAN, PluralKind::Cardinal, Some(&en)),
        Other
    );
}

#[test]
fn plural_wording_picks_the_form_and_writes_the_count() {
    let en = loc("en-GB");
    let files = |n: i64| {
        Plural::new(n, "{n} files")
            .none("No files")
            .one("{n} file")
            .locale(&en)
            .to_string()
    };
    assert_eq!(files(0), "No files");
    assert_eq!(files(1), "1 file");
    assert_eq!(files(2), "2 files");
    assert_eq!(files(1200), "1,200 files");
    let ru = |n: i64| {
        Plural::new(n, "{n} файла")
            .one("{n} файл")
            .many("{n} файлов")
            .locale(&loc("ru-RU"))
            .to_string()
    };
    // Few is not given: it uses the `other` wording.
    assert_eq!(
        [1, 2, 5, 21].map(ru),
        ["1 файл", "2 файла", "5 файлов", "21 файл"]
    );
    let place = |n: i64| {
        Plural::new(n, "{n}th")
            .one("{n}st")
            .two("{n}nd")
            .few("{n}rd")
            .ordinal()
            .locale(&en)
            .to_string()
    };
    assert_eq!(
        [1, 2, 3, 4, 11, 21].map(place),
        ["1st", "2nd", "3rd", "4th", "11th", "21st"]
    );
    // A wording without {n} is returned as it is.
    assert_eq!(Plural::new(3, "Several").locale(&en).to_string(), "Several");
}

// --- time spans ------------------------------------------------------------

fn span(seconds: f64) -> Timespan {
    Timespan::from_secs_f64(seconds).locale(&loc("en-GB"))
}

#[test]
fn spans_are_worded_to_two_units() {
    assert_eq!(span(3825.0).to_string(), "1 hr, 3 mins");
    assert_eq!(
        span(3825.0).width(Width::Long).to_string(),
        "1 hour, 3 minutes"
    );
    assert_eq!(span(3825.0).width(Width::Narrow).to_string(), "1h 3m");
    assert_eq!(
        span(3825.0).max_units(3).to_string(),
        "1 hr, 3 mins, 45 secs"
    );
    assert_eq!(span(3600.0).to_string(), "1 hr");
    assert_eq!(span(3605.0).max_units(2).to_string(), "1 hr");
    assert!(
        span(90_061.0).to_string().starts_with("1 day, 1 hr"),
        "{}",
        span(90_061.0)
    );
}

#[test]
fn a_span_that_rounds_up_moves_to_the_next_unit() {
    assert_eq!(span(59.6).to_string(), "1 min");
    assert_eq!(span(59.4).width(Width::Long).to_string(), "59 seconds");
    assert_eq!(span(3599.6).width(Width::Long).to_string(), "1 hour");
}

#[test]
fn a_span_of_nothing_a_negative_one_and_a_huge_one() {
    assert_eq!(span(0.0).width(Width::Long).to_string(), "0 seconds");
    assert_eq!(span(0.4).width(Width::Long).to_string(), "0 seconds");
    assert_eq!(span(-5.0).width(Width::Long).to_string(), "0 seconds");
    assert_eq!(span(f64::NAN).width(Width::Long).to_string(), "0 seconds");
    assert!(
        !Timespan::new(Duration::MAX)
            .locale(&loc("en-GB"))
            .to_string()
            .is_empty()
    );
    assert!(
        !Timespan::new(Duration::MAX)
            .digital()
            .locale(&loc("en-GB"))
            .to_string()
            .is_empty()
    );
}

#[test]
fn digital_spans_pad_the_minutes_and_seconds_and_count_hours_past_a_day() {
    assert_eq!(span(3825.0).digital().to_string(), "1:03:45");
    assert_eq!(span(59.0).digital().to_string(), "0:59");
    assert_eq!(span(0.0).digital().to_string(), "0:00");
    assert_eq!(span(125.0).digital().to_string(), "2:05");
    assert_eq!(span(90_000.0).digital().to_string(), "25:00:00");
    assert_eq!(span(3599.6).digital().to_string(), "1:00:00");
    assert_eq!(
        Timespan::from_secs_f64(3825.0)
            .digital()
            .locale(&loc("ar-EG"))
            .to_string(),
        "١:٠٣:٤٥"
    );
}

#[test]
fn spans_in_other_languages() {
    let de = Timespan::from_secs_f64(3825.0)
        .width(Width::Long)
        .locale(&loc("de-DE"))
        .to_string();
    assert_eq!(de, "1 Stunde, 3 Minuten");
    let ru = Timespan::from_secs_f64(7380.0)
        .width(Width::Long)
        .locale(&loc("ru-RU"))
        .to_string();
    assert_eq!(ru, "2 часа 3 минуты");
}

// --- units -----------------------------------------------------------------

#[test]
fn units_are_spaced_as_the_language_spaces_them() {
    let measure =
        |tag: &str, value: f64, unit: Unit| Measure::new(value, unit).locale(&loc(tag)).to_string();
    assert_eq!(measure("en-GB", 21.5, Unit::Celsius), "21.5°C");
    assert_eq!(measure("de-DE", 21.5, Unit::Celsius), "21,5\u{a0}°C");
    assert_eq!(measure("fr-FR", 21.5, Unit::Celsius), "21,5\u{a0}°C");
    assert_eq!(
        measure("en-GB", 12.0, Unit::KilometresPerHour),
        "12\u{a0}km/h"
    );
    assert_eq!(
        measure("en-GB", 1013.25, Unit::Hectopascal),
        "1,013.25\u{a0}hPa"
    );
    assert_eq!(measure("en-GB", 45.0, Unit::Degree), "45°");
    assert_eq!(measure("en-GB", -3.0, Unit::Celsius), "-3°C");
}

#[test]
fn a_percentage_unit_and_a_time_unit_use_the_language_data() {
    assert_eq!(
        Measure::new(45, Unit::Percent)
            .locale(&loc("en-GB"))
            .to_string(),
        "45%"
    );
    assert_eq!(
        Measure::new(45.5, Unit::Percent)
            .locale(&loc("en-GB"))
            .to_string(),
        "45.5%"
    );
    assert_eq!(
        Measure::new(2, TimeUnit::Hour)
            .width(Width::Long)
            .locale(&loc("en-GB"))
            .to_string(),
        "2 hours"
    );
    assert_eq!(
        Measure::new(1, TimeUnit::Hour)
            .width(Width::Long)
            .locale(&loc("en-GB"))
            .to_string(),
        "1 hour"
    );
    assert_eq!(
        Measure::new(5, TimeUnit::Minute)
            .width(Width::Long)
            .locale(&loc("ru-RU"))
            .to_string(),
        "5 минут"
    );
    assert_eq!(
        Measure::new(2, TimeUnit::Minute)
            .width(Width::Long)
            .locale(&loc("ru-RU"))
            .to_string(),
        "2 минуты"
    );
}

// --- the person's own locale -------------------------------------------------

#[test]
fn the_default_locale_can_be_set_and_changes_the_revision() {
    let before = Locale::revision();
    Locale::set_current(Some(loc("de-DE")));
    assert!(Locale::revision() > before);
    assert_eq!(Number::new(1234.5).to_string(), "1.234,5");
    assert_eq!(Locale::preferred(), vec![loc("de-DE")]);
    Locale::set_current(None);
    assert!(Locale::revision() > before + 1);
}

/// A formatted value reads in one piece in a right-to-left sentence: it is
/// wrapped in a first-strong isolate there ("1.2 MB", not "MB 1.2"), and
/// left as it is in a left-to-right interface.
#[test]
fn values_are_isolated_in_a_right_to_left_interface() {
    use crate::controls::direction::Direction;
    let size = Bytes::new(1_204_331).locale(&loc("en-GB")).compose();
    assert_eq!(isolate_in(size.clone(), Direction::Ltr), size);
    let isolated = isolate_in(size.clone(), Direction::Rtl);
    assert_eq!(isolated, format!("\u{2068}{size}\u{2069}"));
    // Nothing else changes: the isolate marks are invisible and the value
    // inside keeps its text.
    assert_eq!(
        isolated.trim_matches(['\u{2068}', '\u{2069}']),
        "1.2\u{a0}MB"
    );
    assert_eq!(isolate_in(String::new(), Direction::Rtl), "");
}
