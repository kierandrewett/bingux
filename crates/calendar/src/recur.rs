//! Repeating events: which occurrences fall in a window, and how an edit or
//! a delete applies to one occurrence, the ones after it, or all of them.
//!
//! A rule is expanded on the event's own wall clock, as RFC 5545 says: a
//! daily 09:00 event is at 09:00 on the day the clocks change, and an
//! occurrence that falls in the hour that does not exist is shifted by
//! [`Zone::instant`]. The expansion is the `rrule` crate's; this module adds
//! the parts the crate leaves to the caller: the start always being the first
//! occurrence, `UNTIL` read on the right clock, additions, exclusions and
//! changed occurrences.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc, Weekday};
use rrule::{RRuleSet, Tz as RTz};

use crate::event::{Event, Exception, Item, Key, Occurrence, Series};
use crate::ics;
use crate::zone::Zone;

/// The most keys one expansion produces. A daily event over five years is
/// 1,826; nobody draws more than that at once.
const LIMIT: u16 = 6000;

/// How many occurrences a rule is asked for when it is counted from its start.
const COUNT_LIMIT: u16 = 20_000;

/// What an edit or a delete applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// This occurrence only.
    This,
    /// This occurrence and every later one.
    Following,
    /// Every occurrence.
    All,
}

/// What to write to the store after an edit.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Save(Item),
    Delete(String),
}

// ---------------------------------------------------------------------------
// Expansion

/// The occurrences of `item` that touch `from..to` (not including `to`), on
/// the clock `view`. Both bounds are wall-clock times on `view`.
pub fn occurrences(
    item: &Item,
    from: NaiveDateTime,
    to: NaiveDateTime,
    view: Zone,
) -> Vec<Occurrence> {
    let mut found = Vec::new();
    let touches = |occurrence: &Occurrence| {
        if occurrence.end <= occurrence.start {
            occurrence.start >= from && occurrence.start < to
        } else {
            occurrence.end > from && occurrence.start < to
        }
    };
    let Some(series) = &item.series else {
        let occurrence = make(item, &item.event, None, false, view);
        if touches(&occurrence) {
            found.push(occurrence);
        }
        return found;
    };

    let duration = item.event.duration();
    // The window on the event's own clock, with a day of room each side and
    // the length of the event, so nothing that overlaps is missed.
    let (lo, hi) = {
        let (a, b) = if item.event.all_day {
            (from, to)
        } else {
            (
                view.convert(from, item.event.zone),
                view.convert(to, item.event.zone),
            )
        };
        (a - duration - Duration::days(2), b + Duration::days(2))
    };
    let keys = keys_between(item, series, lo, hi);
    for key in keys {
        if series.exclusions.contains(&key) {
            continue;
        }
        if item
            .exceptions
            .iter()
            .any(|exception| exception.key == key && !exception.and_future)
        {
            continue; // Replaced; added below.
        }
        // A "this and future" change stands for every later occurrence.
        let future = item
            .exceptions
            .iter()
            .filter(|exception| exception.and_future && exception.key <= key)
            .max_by_key(|exception| exception.key);
        let occurrence = match future {
            Some(exception) => {
                let mut event = exception.event.clone();
                let shift = key - exception.key;
                event.start += shift;
                event.end += shift;
                make(item, &event, Some(key), true, view)
            }
            None => {
                let mut event = item.event.clone();
                event.start = key;
                event.end = if event.all_day {
                    key + duration
                } else {
                    // The length is elapsed time: an hour across a clock
                    // change is still an hour.
                    let instant = item.event.zone.instant(key, Zone::Utc) + duration;
                    item.event.zone.wall(instant)
                };
                make(item, &event, Some(key), false, view)
            }
        };
        if touches(&occurrence) {
            found.push(occurrence);
        }
    }
    for exception in item
        .exceptions
        .iter()
        .filter(|exception| !exception.and_future)
    {
        if series.exclusions.contains(&exception.key) {
            continue;
        }
        let occurrence = make(item, &exception.event, Some(exception.key), true, view);
        if touches(&occurrence) {
            found.push(occurrence);
        }
    }
    found.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.end.cmp(&b.end)));
    found
}

fn make(item: &Item, event: &Event, key: Option<Key>, changed: bool, view: Zone) -> Occurrence {
    let (start, end) = if event.all_day {
        (event.start, event.end)
    } else {
        (
            view.wall(event.zone.instant(event.start, view)),
            view.wall(event.zone.instant(event.end, view)),
        )
    };
    Occurrence {
        uid: item.uid.clone(),
        calendar: item.calendar.clone(),
        key,
        recurring: item.series.is_some(),
        changed,
        summary: event.summary.clone(),
        location: event.location.clone(),
        all_day: event.all_day,
        start,
        end,
        zone: event.zone,
        origin_start: event.start,
        origin_end: event.end,
        status: event.status,
        availability: event.availability,
        has_reminder: !event.reminders.is_empty(),
    }
}

/// The keys of a series between two wall-clock times on the event's clock,
/// the start included, additions included, exclusions not yet removed.
fn keys_between(item: &Item, series: &Series, lo: NaiveDateTime, hi: NaiveDateTime) -> Vec<Key> {
    let start = item.event.start;
    let mut keys: Vec<Key> = match generate(&series.rule, item, lo, hi) {
        Ok(keys) => keys,
        // A rule that cannot be read leaves the event as a single one.
        Err(_) => Vec::new(),
    };
    if start >= lo && start <= hi && !keys.contains(&start) {
        keys.push(start);
    }
    for added in &series.additions {
        if *added >= lo && *added <= hi && !keys.contains(added) {
            keys.push(*added);
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// The rule's own dates, in the domain where every clock is read as UTC.
fn generate(
    rule: &str,
    item: &Item,
    lo: NaiveDateTime,
    hi: NaiveDateTime,
) -> Result<Vec<Key>, String> {
    let start = item.event.start;
    let prepared = prepare_rule(rule, &item.event, start_matches(rule, item)?)?;
    let text = format!(
        "DTSTART:{}Z\nRRULE:{prepared}",
        start.format("%Y%m%dT%H%M%S")
    );
    let set: RRuleSet = text.parse().map_err(|error| format!("{error}"))?;
    let at = |naive: NaiveDateTime| -> DateTime<RTz> { RTz::UTC.from_utc_datetime(&naive) };
    let result = set.after(at(lo)).before(at(hi)).all(LIMIT);
    Ok(result.dates.iter().map(|date| date.naive_local()).collect())
}

/// Whether the start is itself one of the rule's dates. RFC 5545 makes the
/// start the first occurrence whatever the rule says, and counts it in
/// `COUNT`; the expansion does neither when it is not one.
fn start_matches(rule: &str, item: &Item) -> Result<bool, String> {
    let start = item.event.start;
    let prepared = prepare_rule(rule, &item.event, true)?;
    let text = format!(
        "DTSTART:{}Z\nRRULE:{prepared}",
        start.format("%Y%m%dT%H%M%S")
    );
    let set: RRuleSet = text.parse().map_err(|error| format!("{error}"))?;
    let at = RTz::UTC.from_utc_datetime(&start);
    let result = set.after(at).before(at).all(2);
    Ok(!result.dates.is_empty())
}

/// The rule as the expansion reads it: `UNTIL` on the event's wall clock in
/// the form the expansion needs, and `COUNT` one lower when the start is not
/// one of the rule's dates.
fn prepare_rule(rule: &str, event: &Event, start_is_a_date: bool) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();
    for part in rule.split(';').filter(|part| !part.trim().is_empty()) {
        let Some((name, value)) = part.split_once('=') else {
            return Err(format!("not a rule part: {part}"));
        };
        let name = name.trim().to_ascii_uppercase();
        let value = value.trim();
        match name.as_str() {
            "UNTIL" => parts.push(format!("UNTIL={}", until_on_wall_clock(value, event)?)),
            "COUNT" if !start_is_a_date => {
                let count: u32 = value
                    .parse()
                    .map_err(|_| "COUNT is not a number".to_string())?;
                // The start takes one of the places. With none left the rule
                // makes nothing, and the start stands alone.
                if count <= 1 {
                    return Err("only the start remains".to_string());
                }
                parts.push(format!("COUNT={}", count - 1));
            }
            _ => parts.push(format!("{name}={value}")),
        }
    }
    Ok(parts.join(";"))
}

/// `UNTIL` as the wall clock of the event with a `Z`, whatever form it had.
fn until_on_wall_clock(value: &str, event: &Event) -> Result<String, String> {
    let bad = || format!("UNTIL is not a date: {value}");
    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").map_err(|_| bad())?;
        return Ok(format!("{}T235959Z", date.format("%Y%m%d")));
    }
    let (body, utc) = match value.strip_suffix(['Z', 'z']) {
        Some(body) => (body, true),
        None => (value, false),
    };
    let wall = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").map_err(|_| bad())?;
    let wall = if utc && !event.all_day {
        Zone::Utc.convert(wall, event.zone)
    } else {
        wall
    };
    let wall = if event.all_day {
        // Compared with midnights: the whole day counts.
        wall.date().and_hms_opt(23, 59, 59).unwrap_or(wall)
    } else {
        wall
    };
    Ok(format!("{}Z", wall.format("%Y%m%dT%H%M%S")))
}

/// All the keys a series has, up to `until` on the event's clock, for the
/// ones counting how many occurrences come before a split.
fn keys_before(item: &Item, series: &Series, key: Key) -> Vec<Key> {
    let lo = item.event.start - Duration::days(1);
    keys_between(item, series, lo, key - Duration::seconds(1))
        .into_iter()
        .filter(|candidate| *candidate < key)
        .collect()
}

// ---------------------------------------------------------------------------
// The event at an occurrence

/// The event as it is at occurrence `key`, in the event's own clock: the
/// original with its times moved to the occurrence, or the changed one.
pub fn event_at(item: &Item, key: Option<Key>) -> Event {
    let Some(key) = key else {
        return item.event.clone();
    };
    if let Some(exception) = item
        .exceptions
        .iter()
        .find(|exception| exception.key == key)
    {
        return exception.event.clone();
    }
    let future = item
        .exceptions
        .iter()
        .filter(|exception| exception.and_future && exception.key <= key)
        .max_by_key(|exception| exception.key);
    if let Some(exception) = future {
        let mut event = exception.event.clone();
        let shift = key - exception.key;
        event.start += shift;
        event.end += shift;
        return event;
    }
    let mut event = item.event.clone();
    let duration = item.event.duration();
    event.start = key;
    event.end = if event.all_day {
        key + duration
    } else {
        item.event
            .zone
            .wall(item.event.zone.instant(key, Zone::Utc) + duration)
    };
    event
}

// ---------------------------------------------------------------------------
// Editing

/// Applies an edited `event` to occurrence `key` of `item` (the item's start
/// when it does not repeat). `rule` is the rule the edit sets: `None` leaves
/// it as it is, `Some(None)` makes the event stop repeating, `Some(Some(r))`
/// sets `r`. `new_uid` names a series that a `Following` edit splits off.
pub fn edit(
    item: &Item,
    key: Option<Key>,
    scope: Scope,
    event: Event,
    rule: Option<Option<String>>,
    new_uid: &str,
) -> Vec<Change> {
    let Some(series) = item.series.clone() else {
        // Not repeating: the edit is the event; it may start repeating.
        let mut next = item.clone();
        next.event = event;
        if let Some(Some(rule)) = rule {
            next.series = Some(Series {
                rule,
                ..Series::default()
            });
        }
        next.sequence += 1;
        next.modified = Some(Utc::now());
        return vec![Change::Save(next)];
    };
    let key = key.unwrap_or(item.event.start);
    let scope = if scope == Scope::Following && key <= item.event.start {
        Scope::All
    } else {
        scope
    };
    match scope {
        Scope::This => {
            let mut next = item.clone();
            let mut event = event;
            event.extra = event_at(item, Some(key)).extra;
            next.exceptions.retain(|exception| exception.key != key);
            if let Some(series) = &mut next.series {
                series.exclusions.retain(|excluded| *excluded != key);
            }
            // A series stops repeating for this occurrence alone: nothing
            // else about it changes.
            next.exceptions.push(Exception {
                key,
                event,
                and_future: false,
            });
            next.exceptions.sort_by_key(|exception| exception.key);
            next.sequence += 1;
            next.modified = Some(Utc::now());
            vec![Change::Save(next)]
        }
        Scope::All => {
            let mut next = item.clone();
            let original = event_at(item, Some(key));
            let shift_days = (event.start.date() - original.start.date()).num_days();
            let moved = shift_days != 0
                || event.start.time() != original.start.time()
                || event.all_day != original.all_day
                || event.zone != original.zone
                || event.duration() != original.duration();
            // The master moves by the days the edit moved the occurrence, and
            // takes its time of day and length.
            let new_start =
                (item.event.start.date() + Duration::days(shift_days)).and_time(event.start.time());
            let mut master = with_start(&event, new_start);
            master.extra = item.event.extra.clone();
            next.event = master;
            match rule {
                Some(Some(rule)) => {
                    next.series = Some(Series { rule, ..series });
                    next.exceptions.clear();
                    if let Some(series) = &mut next.series {
                        series.exclusions.clear();
                        series.additions.clear();
                    }
                }
                Some(None) => {
                    next.series = None;
                    next.exceptions.clear();
                }
                None => {
                    if moved {
                        if let Some(series) = &mut next.series {
                            series.rule = shift_weekdays(&series.rule, shift_days);
                            series.exclusions.clear();
                            series.additions.clear();
                        }
                        next.exceptions.clear();
                    }
                }
            }
            next.sequence += 1;
            next.modified = Some(Utc::now());
            vec![Change::Save(next)]
        }
        Scope::Following => {
            let mut head = item.clone();
            let mut tail = item.clone();
            let before = keys_before(item, &series, key);
            let rule_text = series.rule.clone();
            // The head ends before this occurrence.
            if let Some(head_series) = &mut head.series {
                head_series.rule = end_rule_before(&rule_text, &item.event, key, before.len());
                head_series.exclusions.retain(|excluded| *excluded < key);
                head_series.additions.retain(|added| *added < key);
            }
            head.exceptions.retain(|exception| exception.key < key);
            head.sequence += 1;
            head.modified = Some(Utc::now());
            // The tail is a new series that starts at the edited occurrence.
            let same_clock_shift = event.start - key;
            tail.uid = new_uid.to_string();
            tail.etag = None;
            tail.created = Some(Utc::now());
            tail.modified = Some(Utc::now());
            tail.sequence = 0;
            let mut tail_event = event;
            tail_event.extra = item.event.extra.clone();
            tail.event = tail_event;
            let unchanged_rule = rule.is_none();
            tail.series = match rule {
                Some(Some(rule)) => Some(Series {
                    rule,
                    ..Series::default()
                }),
                Some(None) => None,
                None => {
                    let mut tail_series = series.clone();
                    tail_series.rule = count_after(&rule_text, before.len());
                    if same_clock_shift != Duration::zero() {
                        tail_series.rule =
                            shift_weekdays(&tail_series.rule, same_clock_shift.num_days());
                        tail_series.exclusions.clear();
                        tail_series.additions.clear();
                    } else {
                        tail_series.exclusions.retain(|excluded| *excluded >= key);
                        tail_series.additions.retain(|added| *added >= key);
                    }
                    Some(tail_series)
                }
            };
            tail.exceptions = if unchanged_rule && same_clock_shift == Duration::zero() {
                item.exceptions
                    .iter()
                    .filter(|exception| exception.key >= key)
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            vec![Change::Save(head), Change::Save(tail)]
        }
    }
}

/// `event` with its start at `start`, keeping its length.
fn with_start(event: &Event, start: NaiveDateTime) -> Event {
    let length = event.duration();
    let mut moved = event.clone();
    moved.start = start;
    moved.end = if event.all_day {
        start + length
    } else {
        event
            .zone
            .wall(event.zone.instant(start, Zone::Utc) + length)
    };
    moved
}

/// Removes occurrence `key` of `item`, or the ones after it, or all.
pub fn remove(item: &Item, key: Option<Key>, scope: Scope) -> Vec<Change> {
    let Some(series) = &item.series else {
        return vec![Change::Delete(item.uid.clone())];
    };
    let key = key.unwrap_or(item.event.start);
    let scope = if scope == Scope::Following && key <= item.event.start {
        Scope::All
    } else {
        scope
    };
    match scope {
        Scope::All => vec![Change::Delete(item.uid.clone())],
        Scope::This => {
            let mut next = item.clone();
            next.exceptions.retain(|exception| exception.key != key);
            if let Some(series) = &mut next.series {
                if !series.exclusions.contains(&key) {
                    series.exclusions.push(key);
                    series.exclusions.sort();
                }
            }
            next.sequence += 1;
            next.modified = Some(Utc::now());
            vec![Change::Save(next)]
        }
        Scope::Following => {
            let mut next = item.clone();
            let before = keys_before(item, series, key);
            if let Some(next_series) = &mut next.series {
                next_series.rule = end_rule_before(&series.rule, &item.event, key, before.len());
                next_series.exclusions.retain(|excluded| *excluded < key);
                next_series.additions.retain(|added| *added < key);
            }
            next.exceptions.retain(|exception| exception.key < key);
            next.sequence += 1;
            next.modified = Some(Utc::now());
            vec![Change::Save(next)]
        }
    }
}

/// The rule ending before occurrence `key`: `UNTIL` just before it, or when
/// the rule counts, `COUNT` set to how many come before.
fn end_rule_before(rule: &str, event: &Event, key: Key, before: usize) -> String {
    let counted = rule
        .split(';')
        .any(|part| part.trim().to_ascii_uppercase().starts_with("COUNT="));
    let mut parts: Vec<String> = rule
        .split(';')
        .filter(|part| {
            let upper = part.trim().to_ascii_uppercase();
            !upper.starts_with("UNTIL=") && !upper.starts_with("COUNT=") && !part.trim().is_empty()
        })
        .map(str::to_string)
        .collect();
    if counted {
        parts.push(format!("COUNT={}", before.max(1)));
    } else {
        parts.push(format!("UNTIL={}", until_before(event, key)));
    }
    parts.join(";")
}

/// The `UNTIL` value that excludes `key` and all after it.
fn until_before(event: &Event, key: Key) -> String {
    if event.all_day {
        return (key.date() - Duration::days(1))
            .format("%Y%m%d")
            .to_string();
    }
    let last = key - Duration::seconds(1);
    match event.zone {
        Zone::Floating => last.format("%Y%m%dT%H%M%S").to_string(),
        Zone::Utc => format!("{}Z", last.format("%Y%m%dT%H%M%S")),
        zone => format!("{}Z", zone.instant(last, Zone::Utc).format("%Y%m%dT%H%M%S")),
    }
}

/// The rule for what remains after `used` occurrences: a counted rule counts
/// what is left, any other is as it was.
fn count_after(rule: &str, used: usize) -> String {
    rule.split(';')
        .map(|part| {
            let upper = part.trim().to_ascii_uppercase();
            match upper
                .strip_prefix("COUNT=")
                .and_then(|n| n.parse::<usize>().ok())
            {
                Some(total) => format!("COUNT={}", total.saturating_sub(used).max(1)),
                None => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Moves the plain weekdays of `BYDAY` by `days` (an nth weekday, such as
/// `2TU`, is left: moving the day does not say which week it means).
fn shift_weekdays(rule: &str, days: i64) -> String {
    if days == 0 {
        return rule.to_string();
    }
    const NAMES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
    rule.split(';')
        .map(|part| {
            let Some(list) = part.strip_prefix("BYDAY=") else {
                return part.to_string();
            };
            let shifted: Vec<String> = list
                .split(',')
                .map(|day| match NAMES.iter().position(|name| *name == day) {
                    Some(index) => NAMES[(index as i64 + days).rem_euclid(7) as usize].to_string(),
                    None => day.to_string(),
                })
                .collect();
            format!("BYDAY={}", shifted.join(","))
        })
        .collect::<Vec<_>>()
        .join(";")
}

// ---------------------------------------------------------------------------
// Rules a person picks

/// The common ways an event repeats, as a menu offers them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repeat {
    Never,
    Daily,
    Weekdays,
    Weekly,
    Fortnightly,
    Monthly,
    Yearly,
    /// A rule with more in it than the menu can say: kept as it is.
    Custom(String),
}

/// When a repeat stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepeatEnd {
    Never,
    On(NaiveDate),
    After(u32),
}

impl Repeat {
    /// Names the rule: a menu choice and when it ends.
    pub fn from_rule(rule: Option<&str>) -> (Repeat, RepeatEnd) {
        let Some(rule) = rule else {
            return (Repeat::Never, RepeatEnd::Never);
        };
        let mut freq = "";
        let mut interval = 1u32;
        let mut end = RepeatEnd::Never;
        let mut byday = "";
        let mut other = false;
        for part in rule.split(';').filter(|part| !part.is_empty()) {
            let Some((name, value)) = part.split_once('=') else {
                other = true;
                continue;
            };
            match name.to_ascii_uppercase().as_str() {
                "FREQ" => freq = value,
                "INTERVAL" => interval = value.parse().unwrap_or(1),
                "COUNT" => end = RepeatEnd::After(value.parse().unwrap_or(1)),
                "UNTIL" => {
                    let date = value
                        .get(..8)
                        .and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok());
                    if let Some(date) = date {
                        end = RepeatEnd::On(date);
                    }
                }
                "BYDAY" => byday = value,
                "WKST" => {}
                _ => other = true,
            }
        }
        if other {
            return (Repeat::Custom(rule.to_string()), end);
        }
        let repeat = match (freq, interval, byday) {
            ("DAILY", 1, "") => Repeat::Daily,
            ("WEEKLY", 1, "") => Repeat::Weekly,
            ("WEEKLY", 1, "MO,TU,WE,TH,FR") => Repeat::Weekdays,
            ("WEEKLY", 2, "") => Repeat::Fortnightly,
            ("MONTHLY", 1, "") => Repeat::Monthly,
            ("YEARLY", 1, "") => Repeat::Yearly,
            _ => Repeat::Custom(rule.to_string()),
        };
        (repeat, end)
    }

    /// The rule for this choice and end; `None` for a single event.
    pub fn rule(&self, end: &RepeatEnd) -> Option<String> {
        let base = match self {
            Repeat::Never => return None,
            Repeat::Daily => "FREQ=DAILY".to_string(),
            Repeat::Weekdays => "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string(),
            Repeat::Weekly => "FREQ=WEEKLY".to_string(),
            Repeat::Fortnightly => "FREQ=WEEKLY;INTERVAL=2".to_string(),
            Repeat::Monthly => "FREQ=MONTHLY".to_string(),
            Repeat::Yearly => "FREQ=YEARLY".to_string(),
            Repeat::Custom(rule) => return Some(rule.clone()),
        };
        Some(match end {
            RepeatEnd::Never => base,
            RepeatEnd::After(count) => format!("{base};COUNT={}", (*count).max(1)),
            RepeatEnd::On(date) => format!("{base};UNTIL={}", date.format("%Y%m%d")),
        })
    }
}

/// A rule in words: "Every week on Monday and Wednesday, 10 times".
pub fn describe(rule: &str) -> String {
    let mut freq = "";
    let mut interval = 1u32;
    let mut byday: Vec<String> = Vec::new();
    let mut count = None;
    let mut until = None;
    let mut bymonthday = String::new();
    for part in rule.split(';') {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        match name.to_ascii_uppercase().as_str() {
            "FREQ" => freq = value,
            "INTERVAL" => interval = value.parse().unwrap_or(1),
            "COUNT" => count = value.parse::<u32>().ok(),
            "UNTIL" => {
                until = value
                    .get(..8)
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok())
            }
            "BYDAY" => byday = value.split(',').map(day_name).collect(),
            "BYMONTHDAY" => bymonthday = value.to_string(),
            _ => {}
        }
    }
    let (unit, plural) = match freq {
        "DAILY" => ("day", "days"),
        "WEEKLY" => ("week", "weeks"),
        "MONTHLY" => ("month", "months"),
        "YEARLY" => ("year", "years"),
        "HOURLY" => ("hour", "hours"),
        "MINUTELY" => ("minute", "minutes"),
        _ => return "Custom rule".to_string(),
    };
    let mut text = if interval == 1 {
        format!("Every {unit}")
    } else {
        format!("Every {interval} {plural}")
    };
    if !byday.is_empty() {
        text.push_str(" on ");
        text.push_str(&join_names(&byday));
    }
    if !bymonthday.is_empty() {
        text.push_str(&format!(" on day {bymonthday}"));
    }
    if let Some(count) = count {
        text.push_str(&if count == 1 {
            ", once".to_string()
        } else {
            format!(", {count} times")
        });
    }
    if let Some(until) = until {
        text.push_str(&format!(", until {}", until.format("%-d %B %Y")));
    }
    text
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

fn day_name(code: &str) -> String {
    let (nth, day) = code.split_at(code.len().saturating_sub(2));
    let name = match day {
        "MO" => "Monday",
        "TU" => "Tuesday",
        "WE" => "Wednesday",
        "TH" => "Thursday",
        "FR" => "Friday",
        "SA" => "Saturday",
        "SU" => "Sunday",
        _ => return code.to_string(),
    };
    match nth {
        "" => name.to_string(),
        "-1" => format!("the last {name}"),
        n => match n.parse::<i32>() {
            Ok(1) => format!("the first {name}"),
            Ok(2) => format!("the second {name}"),
            Ok(3) => format!("the third {name}"),
            Ok(4) => format!("the fourth {name}"),
            _ => name.to_string(),
        },
    }
}

/// The weekday abbreviation of `date`, as a rule writes it.
pub fn rule_weekday(date: NaiveDate) -> &'static str {
    match date.weekday() {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

/// A key for an all-day date.
pub fn date_key(date: NaiveDate) -> Key {
    ics::midnight(date)
}
