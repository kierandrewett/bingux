//! Quick add: "Lunch with Sam at Nando's tomorrow 1pm" becomes an event.
//!
//! Parsed on this computer from a short list of phrases in English. The text
//! that no phrase takes becomes the title. Nothing is guessed that the text
//! does not say: with no date and no time there is no `start`, and the editor
//! opens on the day being looked at.
//!
//! # Failure modes
//!
//! | Input | Result | Test |
//! | ----- | ------ | ---- |
//! | Nothing, or only spaces | No title, no start | `nothing_typed_is_nothing` |
//! | "at" before a place and before a time | The place is the words after the first "at" that is not a time | `a_place_after_at`, `a_place_and_a_time_both_after_at` |
//! | 12am and 12pm; 24-hour times | Midnight, noon; the time as written | `midnight_noon_and_the_twenty_four_hour_clock` |
//! | A time already past today | Tomorrow | `a_time_with_no_date_is_today_or_tomorrow` |
//! | A date already past this year | Next year | `a_past_date_is_next_year` |
//! | Day first or month first numbers | By the setting | `numeric_dates_follow_the_order` |
//! | A date and no time | All day | `a_date_alone_is_all_day` |
//! | A range, a length | The end is set | `a_range_and_a_weekday`, `a_length` |
//! | Repeats ("every weekday", "every other week") | An RRULE | `repeats` |
//! | "next friday" said on a Friday | A week on | `next_friday_on_a_friday_is_a_week_on` |
//! | Text with no phrase in it | The whole text is the title | `plain_text_has_no_time` |
//! | A typographic apostrophe | Kept in the title | `a_typographic_apostrophe_stays_in_the_title` |

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

/// What a line of text says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quick {
    pub summary: String,
    pub location: String,
    /// The start. A date alone is an all-day event and has midnight here.
    pub start: Option<NaiveDateTime>,
    /// The end, when the text says one (`2-3pm`, `for 2 hours`).
    pub end: Option<NaiveDateTime>,
    pub all_day: bool,
    /// An `RRULE` value, when the text says it repeats.
    pub rule: Option<String>,
    /// A reminder, in minutes before the start.
    pub reminder: Option<i64>,
    /// The phrases that were taken, in the order they were found, so the
    /// field can show what it understood.
    pub phrases: Vec<String>,
}

struct Word {
    text: String,
    core: String,
    used: bool,
}

impl Word {
    fn new(text: &str) -> Self {
        let core = text
            .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')'))
            .to_lowercase();
        Self {
            text: text.to_string(),
            core,
            used: false,
        }
    }
}

const WEEKDAYS: &[(&str, Weekday)] = &[
    ("monday", Weekday::Mon),
    ("mon", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("tue", Weekday::Tue),
    ("tues", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("wed", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("thu", Weekday::Thu),
    ("thur", Weekday::Thu),
    ("thurs", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("fri", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sat", Weekday::Sat),
    ("sunday", Weekday::Sun),
    ("sun", Weekday::Sun),
];

const MONTHS: &[(&str, u32)] = &[
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sept", 9),
    ("sep", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
    ("nov", 11),
    ("december", 12),
    ("dec", 12),
];

fn weekday_of(word: &str) -> Option<Weekday> {
    WEEKDAYS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, day)| *day)
}

fn month_of(word: &str) -> Option<u32> {
    MONTHS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, month)| *month)
}

fn weekday_code(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

/// Reads `text`. `now` is the time it is, on the clock the event is on;
/// `month_first` reads `3/4` as the fourth of March.
pub fn parse(text: &str, now: NaiveDateTime, month_first: bool) -> Quick {
    let mut words: Vec<Word> = text.split_whitespace().map(Word::new).collect();
    let mut quick = Quick::default();
    let today = now.date();

    let rule_weekday = take_repeat(&mut words, &mut quick);
    take_reminder(&mut words, &mut quick);
    let length = take_length(&mut words, &mut quick);
    let all_day = take_phrase(&mut words, &["all", "day"], &mut quick)
        || take_phrase(&mut words, &["all-day"], &mut quick);
    let mut times = take_times(&mut words, &mut quick);
    let mut date = take_date(&mut words, &mut quick, today, month_first);
    // "tonight" is a date and a time at once.
    if date.is_none() && take_phrase(&mut words, &["tonight"], &mut quick) {
        date = Some(today);
        if times.is_none() {
            times = Some((NaiveTime::from_hms_opt(19, 0, 0).unwrap_or_default(), None));
        }
    }
    // "every monday" with no date starts on the next one.
    if date.is_none() {
        if let Some(day) = rule_weekday {
            date = Some(next_weekday(today, day, false));
        }
    }

    // What is left: a place, then the title.
    quick.location = take_location(&mut words);
    quick.summary = words
        .iter()
        .filter(|word| !word.used)
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    quick.summary = trim_connectives(&quick.summary);

    quick.all_day = all_day;
    match (date, times) {
        (Some(date), Some((start, end))) if !all_day => {
            let start_at = date.and_time(start);
            quick.start = Some(start_at);
            quick.end = match (end, length) {
                (Some(end), _) => {
                    let mut end_at = date.and_time(end);
                    if end_at <= start_at {
                        end_at += Duration::days(1);
                    }
                    Some(end_at)
                }
                (None, Some(length)) => Some(start_at + length),
                (None, None) => None,
            };
        }
        (Some(date), _) => {
            quick.start = date.and_hms_opt(0, 0, 0);
            quick.all_day = true;
            quick.end = length
                .filter(|length| *length >= Duration::days(1))
                .and_then(|length| {
                    quick
                        .start
                        .map(|start| start + Duration::days(length.num_days()))
                });
        }
        (None, Some((start, end))) if !all_day => {
            let mut date = today;
            if date.and_time(start) < now {
                date += Duration::days(1);
            }
            let start_at = date.and_time(start);
            quick.start = Some(start_at);
            quick.end = match (end, length) {
                (Some(end), _) => {
                    let mut end_at = date.and_time(end);
                    if end_at <= start_at {
                        end_at += Duration::days(1);
                    }
                    Some(end_at)
                }
                (None, Some(length)) => Some(start_at + length),
                (None, None) => None,
            };
        }
        (None, _) if all_day => {
            quick.start = today.and_hms_opt(0, 0, 0);
        }
        _ => {}
    }
    quick
}

/// The first date on or after `from` that is `day`; `strictly` makes it after.
fn next_weekday(from: NaiveDate, day: Weekday, strictly: bool) -> NaiveDate {
    let mut date = if strictly {
        from + Duration::days(1)
    } else {
        from
    };
    while date.weekday() != day {
        date += Duration::days(1);
    }
    date
}

fn take_phrase(words: &mut [Word], phrase: &[&str], quick: &mut Quick) -> bool {
    if phrase.is_empty() || words.len() < phrase.len() {
        return false;
    }
    for start in 0..=words.len() - phrase.len() {
        let matches = phrase.iter().enumerate().all(|(offset, want)| {
            !words[start + offset].used && words[start + offset].core == *want
        });
        if matches {
            let shown: Vec<&str> = words[start..start + phrase.len()]
                .iter()
                .map(|w| w.text.as_str())
                .collect();
            quick.phrases.push(shown.join(" "));
            for word in &mut words[start..start + phrase.len()] {
                word.used = true;
            }
            return true;
        }
    }
    false
}

fn mark(words: &mut [Word], from: usize, count: usize, quick: &mut Quick) {
    let shown: Vec<&str> = words[from..from + count]
        .iter()
        .map(|w| w.text.as_str())
        .collect();
    quick.phrases.push(shown.join(" "));
    for word in &mut words[from..from + count] {
        word.used = true;
    }
}

/// "every day", "weekly", "every other week", "every 2 weeks", "every monday and
/// wednesday", "every weekday". Returns the weekday a weekly rule names, so a
/// date can default to it.
fn take_repeat(words: &mut [Word], quick: &mut Quick) -> Option<Weekday> {
    for index in 0..words.len() {
        if words[index].used {
            continue;
        }
        let core = words[index].core.clone();
        let single = match core.as_str() {
            "daily" => Some("FREQ=DAILY"),
            "weekly" => Some("FREQ=WEEKLY"),
            "fortnightly" | "biweekly" => Some("FREQ=WEEKLY;INTERVAL=2"),
            "monthly" => Some("FREQ=MONTHLY"),
            "yearly" | "annually" => Some("FREQ=YEARLY"),
            _ => None,
        };
        if let Some(rule) = single {
            mark(words, index, 1, quick);
            quick.rule = Some(rule.to_string());
            return None;
        }
        if core != "every" {
            continue;
        }
        let next = |offset: usize| {
            words
                .get(index + offset)
                .filter(|w| !w.used)
                .map(|w| w.core.clone())
        };
        let Some(first) = next(1) else { continue };
        let unit = |word: &str| match word {
            "day" | "days" => Some("DAILY"),
            "week" | "weeks" => Some("WEEKLY"),
            "month" | "months" => Some("MONTHLY"),
            "year" | "years" => Some("YEARLY"),
            _ => None,
        };
        // every day, every week
        if let Some(freq) = unit(&first) {
            mark(words, index, 2, quick);
            quick.rule = Some(format!("FREQ={freq}"));
            return None;
        }
        // every other week, every 2 weeks
        if first == "other" {
            if let Some(freq) = next(2).and_then(|w| unit(&w)) {
                mark(words, index, 3, quick);
                quick.rule = Some(format!("FREQ={freq};INTERVAL=2"));
                return None;
            }
        }
        if let Ok(count) = first.parse::<u32>() {
            if let Some(freq) = next(2).and_then(|w| unit(&w)) {
                mark(words, index, 3, quick);
                quick.rule = Some(if count <= 1 {
                    format!("FREQ={freq}")
                } else {
                    format!("FREQ={freq};INTERVAL={count}")
                });
                return None;
            }
        }
        if first == "weekday" || first == "weekdays" {
            mark(words, index, 2, quick);
            quick.rule = Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string());
            return None;
        }
        if first == "weekend" {
            mark(words, index, 2, quick);
            quick.rule = Some("FREQ=WEEKLY;BYDAY=SA,SU".to_string());
            return None;
        }
        // every monday, every monday and wednesday, every mon, wed
        let mut days = Vec::new();
        let mut taken = 1;
        while let Some(word) = words.get(index + taken).filter(|w| !w.used) {
            if let Some(day) = weekday_of(&word.core) {
                days.push(day);
                taken += 1;
                if let Some(joiner) = words.get(index + taken) {
                    if !joiner.used
                        && joiner.core == "and"
                        && words
                            .get(index + taken + 1)
                            .is_some_and(|w| weekday_of(&w.core).is_some())
                    {
                        taken += 1;
                    }
                }
            } else {
                break;
            }
        }
        if !days.is_empty() {
            mark(words, index, taken, quick);
            let list: Vec<&str> = days.iter().map(|day| weekday_code(*day)).collect();
            quick.rule = Some(format!("FREQ=WEEKLY;BYDAY={}", list.join(",")));
            return days.first().copied();
        }
    }
    None
}

/// "remind me 15 minutes before", "alert 1 hour before".
fn take_reminder(words: &mut [Word], quick: &mut Quick) {
    for index in 0..words.len() {
        if words[index].used
            || !matches!(words[index].core.as_str(), "remind" | "reminder" | "alert")
        {
            continue;
        }
        let mut at = index + 1;
        if words.get(at).is_some_and(|w| w.core == "me") {
            at += 1;
        }
        let Some(count) = words.get(at).and_then(|w| w.core.parse::<i64>().ok()) else {
            continue;
        };
        let Some(unit) = words.get(at + 1).map(|w| w.core.as_str()) else {
            continue;
        };
        let minutes = match unit {
            "min" | "mins" | "minute" | "minutes" => count,
            "h" | "hr" | "hrs" | "hour" | "hours" => count * 60,
            "day" | "days" => count * 1440,
            _ => continue,
        };
        let mut length = at + 2 - index;
        if words
            .get(at + 2)
            .is_some_and(|w| w.core == "before" || w.core == "early")
        {
            length += 1;
        }
        mark(words, index, length, quick);
        quick.reminder = Some(minutes);
        return;
    }
}

/// "for 2 hours", "for 30 min", "for 1h30", "for 3 days".
fn take_length(words: &mut [Word], quick: &mut Quick) -> Option<Duration> {
    for index in 0..words.len() {
        if words[index].used || words[index].core != "for" {
            continue;
        }
        let Some(next) = words.get(index + 1).filter(|w| !w.used) else {
            continue;
        };
        // "1h30" and "90min" in one word.
        if let Some(length) = glued_length(&next.core) {
            mark(words, index, 2, quick);
            return Some(length);
        }
        let first = next.core.clone();
        let (count, unit_at) = match first.as_str() {
            "an" | "a" => (1.0, index + 2),
            "half" => {
                if words.get(index + 2).is_some_and(|w| w.core == "an") {
                    (0.5, index + 3)
                } else {
                    (0.5, index + 2)
                }
            }
            other => match other.parse::<f64>() {
                Ok(count) => (count, index + 2),
                Err(_) => continue,
            },
        };
        let Some(unit) = words
            .get(unit_at)
            .filter(|w| !w.used)
            .map(|w| w.core.clone())
        else {
            continue;
        };
        let minutes = match unit.as_str() {
            "min" | "mins" | "minute" | "minutes" => count,
            "h" | "hr" | "hrs" | "hour" | "hours" => count * 60.0,
            "day" | "days" => count * 1440.0,
            "week" | "weeks" => count * 10080.0,
            _ => continue,
        };
        mark(words, index, unit_at + 1 - index, quick);
        return Some(Duration::minutes(minutes.round() as i64));
    }
    None
}

fn glued_length(word: &str) -> Option<Duration> {
    let (hours, rest) = word.split_once('h')?;
    let hours: i64 = hours.parse().ok()?;
    let rest = rest
        .trim_end_matches("ours")
        .trim_end_matches("rs")
        .trim_end_matches('m')
        .trim_end_matches("in");
    let minutes: i64 = if rest.is_empty() {
        0
    } else {
        rest.parse().ok()?
    };
    Some(Duration::minutes(hours * 60 + minutes))
}

/// A clock time in one word: `1pm`, `1:30pm`, `13:30`, `noon`, `midnight`.
/// `bare` allows a lone number (after "at"), read the way people mean it:
/// 1 to 6 is the afternoon, 7 to 11 the morning, 12 noon.
fn clock(word: &str, bare: bool) -> Option<NaiveTime> {
    match word {
        "noon" | "midday" => return NaiveTime::from_hms_opt(12, 0, 0),
        "midnight" => return NaiveTime::from_hms_opt(0, 0, 0),
        _ => {}
    }
    let (body, meridiem) =
        if let Some(body) = word.strip_suffix("pm").or_else(|| word.strip_suffix("p.m")) {
            (body, Some(true))
        } else if let Some(body) = word.strip_suffix("am").or_else(|| word.strip_suffix("a.m")) {
            (body, Some(false))
        } else {
            (word, None)
        };
    let (hours, minutes) = match body.split_once([':', '.']) {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok()?, 0),
    };
    if minutes > 59 {
        return None;
    }
    let colon = body.contains(':');
    match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&hours) {
                return None;
            }
            let hours = hours % 12 + if pm { 12 } else { 0 };
            NaiveTime::from_hms_opt(hours, minutes, 0)
        }
        None if colon && hours < 24 => NaiveTime::from_hms_opt(hours, minutes, 0),
        None if bare && (1..=12).contains(&hours) && !body.contains('.') => {
            let hours = match hours {
                1..=6 => hours + 12,
                12 => 12,
                other => other,
            };
            NaiveTime::from_hms_opt(hours, minutes, 0)
        }
        None => None,
    }
}

/// A time or a range of times. Returns the start and an optional end.
fn take_times(words: &mut [Word], quick: &mut Quick) -> Option<(NaiveTime, Option<NaiveTime>)> {
    for index in 0..words.len() {
        if words[index].used {
            continue;
        }
        let core = words[index].core.clone();
        // from 2 to 3pm, from 2pm until 4pm
        if core == "from" {
            if let Some(first) = words
                .get(index + 1)
                .filter(|w| !w.used)
                .map(|w| w.core.clone())
            {
                if let Some(joiner) = words
                    .get(index + 2)
                    .filter(|w| !w.used)
                    .map(|w| w.core.clone())
                {
                    if matches!(joiner.as_str(), "to" | "until" | "till" | "-") {
                        if let Some(last) = words
                            .get(index + 3)
                            .filter(|w| !w.used)
                            .map(|w| w.core.clone())
                        {
                            if let Some((a, b)) = range_of(&first, &last) {
                                mark(words, index, 4, quick);
                                return Some((a, Some(b)));
                            }
                        }
                    }
                }
            }
        }
        // 2-3pm, 14:00-15:30
        if let Some((left, right)) = core.split_once(['-', '–']) {
            if let Some((a, b)) = range_of(left, right) {
                mark(words, index, 1, quick);
                return Some((a, Some(b)));
            }
        }
        // 2 to 3pm
        if let Some(joiner) = words
            .get(index + 1)
            .filter(|w| !w.used)
            .map(|w| w.core.clone())
        {
            if matches!(joiner.as_str(), "to" | "until" | "till") {
                if let Some(last) = words
                    .get(index + 2)
                    .filter(|w| !w.used)
                    .map(|w| w.core.clone())
                {
                    if let Some((a, b)) = range_of(&core, &last) {
                        if core.parse::<u32>().is_ok() || clock(&core, false).is_some() {
                            mark(words, index, 3, quick);
                            return Some((a, Some(b)));
                        }
                    }
                }
            }
        }
        // at 9, at 9:30, 9am, noon
        let after_at = index > 0 && words[index - 1].core == "at" && !words[index - 1].used;
        if let Some(time) = clock(&core, after_at) {
            if after_at {
                mark(words, index - 1, 2, quick);
            } else {
                mark(words, index, 1, quick);
            }
            return Some((time, None));
        }
    }
    None
}

/// `2` and `3pm` is 14:00 to 15:00: the first takes the other's am or pm
/// when it has none and would otherwise end after the second.
fn range_of(first: &str, last: &str) -> Option<(NaiveTime, NaiveTime)> {
    let end = clock(last, true)?;
    let start = match clock(first, false) {
        Some(start) => start,
        None => {
            let hours: u32 = first.parse().ok()?;
            let candidate = clock(
                &format!(
                    "{hours}{}",
                    if last.ends_with("pm") || last.ends_with("p.m") {
                        "pm"
                    } else if last.ends_with("am") || last.ends_with("a.m") {
                        "am"
                    } else {
                        ""
                    }
                ),
                true,
            )?;
            candidate
        }
    };
    // 11-1pm: 11am to 1pm.
    let start = if start > end
        && start.format("%H").to_string().parse::<u32>().unwrap_or(0) >= 12
        && last.ends_with("pm")
    {
        start - Duration::hours(12)
    } else {
        start
    };
    Some((start, end))
}

/// A date: today, tomorrow, a weekday, next week, in 3 days, 14 march, 14/3.
fn take_date(
    words: &mut [Word],
    quick: &mut Quick,
    today: NaiveDate,
    month_first: bool,
) -> Option<NaiveDate> {
    for index in 0..words.len() {
        if words[index].used {
            continue;
        }
        let core = words[index].core.clone();
        let follows = |offset: usize| {
            words
                .get(index + offset)
                .filter(|w| !w.used)
                .map(|w| w.core.clone())
        };
        match core.as_str() {
            "today" => {
                mark(words, index, 1, quick);
                return Some(today);
            }
            "tomorrow" | "tmrw" | "tomo" => {
                mark(words, index, 1, quick);
                return Some(today + Duration::days(1));
            }
            "yesterday" => {
                mark(words, index, 1, quick);
                return Some(today - Duration::days(1));
            }
            _ => {}
        }
        // in 3 days, in 2 weeks, in a month
        if core == "in" {
            let count = match follows(1).as_deref() {
                Some("a" | "an" | "one") => Some(1),
                Some(other) => other.parse::<i64>().ok(),
                None => None,
            };
            if let (Some(count), Some(unit)) = (count, follows(2)) {
                let date = match unit.as_str() {
                    "day" | "days" => Some(today + Duration::days(count)),
                    "week" | "weeks" => Some(today + Duration::weeks(count)),
                    "month" | "months" => add_months(today, count as i32),
                    "year" | "years" => add_months(today, count as i32 * 12),
                    _ => None,
                };
                if let Some(date) = date {
                    mark(words, index, 3, quick);
                    return Some(date);
                }
            }
        }
        // next week, next month, next friday, this friday, on friday
        if matches!(core.as_str(), "next" | "this" | "on" | "coming") {
            if let Some(second) = follows(1) {
                if core == "next" && second == "week" {
                    mark(words, index, 2, quick);
                    return Some(next_weekday(today, Weekday::Mon, true));
                }
                if core == "next" && second == "month" {
                    mark(words, index, 2, quick);
                    return add_months(today.with_day(1)?, 1);
                }
                if let Some(day) = weekday_of(&second) {
                    mark(words, index, 2, quick);
                    let plain = next_weekday(
                        today,
                        day,
                        core == "on" || core == "next" || core == "coming",
                    );
                    if core == "next" {
                        let week_end = today
                            + Duration::days(6 - i64::from(today.weekday().num_days_from_monday()));
                        return Some(if plain <= week_end {
                            plain + Duration::days(7)
                        } else {
                            plain
                        });
                    }
                    return Some(plain);
                }
            }
            if core == "on" {
                // "on 14 march", "on the 14th": fall through to the date forms below.
                if let Some(date) = day_month(words, index + 1, quick, today, month_first, true) {
                    mark_front(words, index);
                    return Some(date);
                }
            }
            continue;
        }
        if let Some(day) = weekday_of(&core) {
            // A bare weekday, but not a short one that may be a word ("sun", "sat", "mon" are
            // still taken: a title that starts with one is unusual).
            mark(words, index, 1, quick);
            return Some(next_weekday(today, day, true));
        }
        if let Some(date) = day_month(words, index, quick, today, month_first, false) {
            return Some(date);
        }
    }
    None
}

fn mark_front(words: &mut [Word], index: usize) {
    words[index].used = true;
}

/// "14 march", "march 14", "14th of march", "14/3", "2026-03-14", with an
/// optional year after. Marks the words it takes.
fn day_month(
    words: &mut [Word],
    index: usize,
    quick: &mut Quick,
    today: NaiveDate,
    month_first: bool,
    after_on: bool,
) -> Option<NaiveDate> {
    let word = |offset: usize| {
        words
            .get(index + offset)
            .filter(|w| !w.used)
            .map(|w| w.core.clone())
    };
    let first = word(0)?;
    let ordinal = |text: &str| -> Option<u32> {
        let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        let suffix = &text[digits.len()..];
        if !(suffix.is_empty() || matches!(suffix, "st" | "nd" | "rd" | "th")) {
            return None;
        }
        digits.parse().ok().filter(|day| (1..=31).contains(day))
    };
    let with_year = |month: u32, day: u32, year: Option<i32>| -> Option<NaiveDate> {
        match year {
            Some(year) => NaiveDate::from_ymd_opt(year, month, day),
            None => {
                let this = NaiveDate::from_ymd_opt(today.year(), month, day)?;
                if this < today {
                    NaiveDate::from_ymd_opt(today.year() + 1, month, day)
                } else {
                    Some(this)
                }
            }
        }
    };
    let year_at = |offset: usize| {
        word(offset)
            .and_then(|w| w.parse::<i32>().ok())
            .filter(|y| (1970..=2200).contains(y))
    };
    // 2026-03-14
    if first.len() == 10 && first.as_bytes().get(4) == Some(&b'-') {
        if let Ok(date) = NaiveDate::parse_from_str(&first, "%Y-%m-%d") {
            mark(words, index, 1, quick);
            return Some(date);
        }
    }
    // 14/3, 14/3/2026, 3/14
    if first.contains('/') {
        let parts: Vec<&str> = first.split('/').collect();
        if (2..=3).contains(&parts.len()) {
            let numbers: Option<Vec<i32>> = parts.iter().map(|p| p.parse().ok()).collect();
            if let Some(numbers) = numbers {
                let (day, month) = if month_first {
                    (numbers[1], numbers[0])
                } else {
                    (numbers[0], numbers[1])
                };
                let year = numbers
                    .get(2)
                    .map(|y| if *y < 100 { 2000 + *y } else { *y });
                if day > 0 && month > 0 {
                    if let Some(date) = with_year(month as u32, day as u32, year) {
                        mark(words, index, 1, quick);
                        return Some(date);
                    }
                }
            }
        }
    }
    // 14 march [2026], 14th of march
    if let Some(day) = ordinal(&first) {
        let mut at = 1;
        if word(at).as_deref() == Some("of") {
            at += 1;
        }
        if let Some(month) = word(at).as_deref().and_then(month_of) {
            let year = year_at(at + 1);
            if let Some(date) = with_year(month, day, year) {
                mark(words, index, at + 1 + usize::from(year.is_some()), quick);
                return Some(date);
            }
        }
        if after_on {
            // "on the 14th" has no month: the next 14th.
            let _ = after_on;
        }
    }
    // march 14 [2026]
    if let Some(month) = month_of(&first) {
        if let Some(day) = word(1).as_deref().and_then(ordinal) {
            let year = year_at(2);
            if let Some(date) = with_year(month, day, year) {
                mark(words, index, 2 + usize::from(year.is_some()), quick);
                return Some(date);
            }
        }
    }
    None
}

fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    let total = date.year() * 12 + date.month0() as i32 + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let mut day = date.day();
    loop {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
        day -= 1;
        if day == 0 {
            return None;
        }
    }
}

/// "at Nando's", "@ the office": what follows the last unused "at" (or an
/// "@"), up to the end.
fn take_location(words: &mut [Word]) -> String {
    let at = words.iter().rposition(|word| {
        !word.used
            && (word.core == "at"
                || word.core == "@"
                || word.text.starts_with('@') && word.text.len() > 1)
    });
    let Some(at) = at else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    if words[at].text.starts_with('@') && words[at].text.len() > 1 {
        parts.push(words[at].text[1..].to_string());
    }
    let mut end = at + 1;
    while end < words.len() && !words[end].used {
        parts.push(words[end].text.clone());
        end += 1;
    }
    if parts.is_empty() {
        return String::new();
    }
    for word in &mut words[at..end] {
        word.used = true;
    }
    parts.join(" ")
}

/// A title does not end on "on", "at", "for" and the like left behind by the
/// phrase that was taken.
fn trim_connectives(text: &str) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    while let Some(last) = words.last() {
        if matches!(
            last.to_lowercase().as_str(),
            "on" | "at"
                | "for"
                | "from"
                | "to"
                | "until"
                | "by"
                | "next"
                | "this"
                | "every"
                | "in"
                | "@"
                | "-"
                | "and"
        ) {
            words.pop();
        } else {
            break;
        }
    }
    while let Some(first) = words.first() {
        if matches!(first.to_lowercase().as_str(), "on" | "-" | "@") {
            words.remove(0);
        } else {
            break;
        }
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wednesday 12 March 2025, 10:00.
    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 3, 12)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    fn at(m: u32, d: u32, h: u32, min: u32) -> Option<NaiveDateTime> {
        NaiveDate::from_ymd_opt(2025, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
    }

    #[test]
    fn lunch_with_sam_tomorrow_at_one() {
        let q = parse("Lunch with Sam tomorrow 1pm", now(), false);
        assert_eq!(q.summary, "Lunch with Sam");
        assert_eq!(q.start, at(3, 13, 13, 0));
        assert!(!q.all_day);
    }

    #[test]
    fn a_place_after_at() {
        let q = parse("Lunch with Sam at Nando's tomorrow 1pm", now(), false);
        assert_eq!(q.summary, "Lunch with Sam");
        assert_eq!(q.location, "Nando's");
        assert_eq!(q.start, at(3, 13, 13, 0));
    }

    #[test]
    fn a_range_and_a_weekday() {
        let q = parse("Team meeting friday 2-3pm", now(), false);
        assert_eq!(q.summary, "Team meeting");
        assert_eq!(q.start, at(3, 14, 14, 0));
        assert_eq!(q.end, at(3, 14, 15, 0));
    }

    #[test]
    fn a_length() {
        let q = parse("Dentist on 24 march at 9:30 for 45 minutes", now(), false);
        assert_eq!(q.summary, "Dentist");
        assert_eq!(q.start, at(3, 24, 9, 30));
        assert_eq!(q.end, at(3, 24, 10, 15));
    }

    #[test]
    fn a_date_alone_is_all_day() {
        let q = parse("Mum's birthday 3 april", now(), false);
        assert!(q.all_day);
        assert_eq!(q.summary, "Mum's birthday");
        assert_eq!(q.start, at(4, 3, 0, 0));
    }

    #[test]
    fn a_past_date_is_next_year() {
        let q = parse("Anniversary 2 march", now(), false);
        assert_eq!(
            q.start,
            NaiveDate::from_ymd_opt(2026, 3, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
        );
    }

    #[test]
    fn repeats() {
        let q = parse("Stand-up every weekday 9:15am", now(), false);
        assert_eq!(q.rule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"));
        assert_eq!(q.summary, "Stand-up");
        let q = parse("Bins every other week", now(), false);
        assert_eq!(q.rule.as_deref(), Some("FREQ=WEEKLY;INTERVAL=2"));
        let q = parse("Piano every monday and thursday 5pm", now(), false);
        assert_eq!(q.rule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,TH"));
        assert_eq!(q.start, at(3, 17, 17, 0));
    }

    #[test]
    fn a_time_with_no_date_is_today_or_tomorrow() {
        assert_eq!(parse("Call Jo 3pm", now(), false).start, at(3, 12, 15, 0));
        assert_eq!(parse("Call Jo 9am", now(), false).start, at(3, 13, 9, 0));
    }

    #[test]
    fn next_and_in() {
        assert_eq!(
            parse("Review next friday", now(), false)
                .start
                .unwrap()
                .date(),
            NaiveDate::from_ymd_opt(2025, 3, 21).unwrap()
        );
        assert_eq!(
            parse("Review in 3 days 11am", now(), false).start,
            at(3, 15, 11, 0)
        );
        assert_eq!(
            parse("Review in 2 weeks", now(), false)
                .start
                .unwrap()
                .date(),
            NaiveDate::from_ymd_opt(2025, 3, 26).unwrap()
        );
    }

    #[test]
    fn numeric_dates_follow_the_order() {
        assert_eq!(parse("Party 3/4", now(), false).start, at(4, 3, 0, 0));
        assert_eq!(
            parse("Party 3/4", now(), true).start,
            at(3, 4, 0, 0).map(|d| d.with_year(2026).unwrap())
        );
    }

    #[test]
    fn a_reminder() {
        let q = parse("Flight tomorrow 6am remind me 2 hours before", now(), false);
        assert_eq!(q.reminder, Some(120));
        assert_eq!(q.summary, "Flight");
    }

    #[test]
    fn plain_text_has_no_time() {
        let q = parse("Buy milk", now(), false);
        assert_eq!(q.summary, "Buy milk");
        assert_eq!(q.start, None);
    }

    #[test]
    fn nothing_typed_is_nothing() {
        for text in ["", "   ", "\t"] {
            let q = parse(text, now(), false);
            assert_eq!(q.summary, "");
            assert_eq!(q.start, None);
            assert!(q.phrases.is_empty());
        }
    }

    #[test]
    fn a_place_and_a_time_both_after_at() {
        let q = parse("Meet Jo at the pub at 5pm", now(), false);
        assert_eq!(q.summary, "Meet Jo");
        assert_eq!(q.location, "the pub");
        assert_eq!(q.start, at(3, 12, 17, 0));
    }

    #[test]
    fn midnight_noon_and_the_twenty_four_hour_clock() {
        assert_eq!(
            parse("Lunch 12pm tomorrow", now(), false).start,
            at(3, 13, 12, 0)
        );
        assert_eq!(
            parse("Run 12am tomorrow", now(), false).start,
            at(3, 13, 0, 0)
        );
        assert_eq!(
            parse("Gym 17:30 tomorrow", now(), false).start,
            at(3, 13, 17, 30)
        );
    }

    #[test]
    fn next_friday_on_a_friday_is_a_week_on() {
        let friday = NaiveDate::from_ymd_opt(2025, 3, 14)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let q = parse("Review next friday", friday, false);
        assert_eq!(
            q.start.unwrap().date(),
            NaiveDate::from_ymd_opt(2025, 3, 21).unwrap()
        );
    }

    #[test]
    fn a_typographic_apostrophe_stays_in_the_title() {
        let q = parse("Mum\u{2019}s birthday 3 april", now(), false);
        assert_eq!(q.summary, "Mum\u{2019}s birthday");
        assert!(q.all_day);
    }

    #[test]
    fn noon_and_tonight() {
        assert_eq!(
            parse("Lunch tomorrow at noon", now(), false).start,
            at(3, 13, 12, 0)
        );
        assert_eq!(parse("Film tonight", now(), false).start, at(3, 12, 19, 0));
    }
}
