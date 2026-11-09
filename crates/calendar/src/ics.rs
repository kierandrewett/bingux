//! Reading and writing iCalendar (RFC 5545).
//!
//! Reading is the `ical` crate's tokeniser (unfolding, parameters, nesting)
//! and the rules of this module for what the lines mean. Writing is here:
//! folding at 75 octets, escaping, and a `VTIMEZONE` for each named zone.
//! What the app does not edit (attendees, the organiser, `X-` properties) is
//! kept as it was read and written back.

use std::collections::BTreeMap;
use std::io::BufReader;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use ical::parser::ical::component::{IcalAlarm, IcalEvent};
use ical::property::Property;

use crate::event::{
    Availability, Event, Exception, Item, Key, RawProp, Reminder, Series, Status, Trigger,
};
use crate::zone::Zone;

/// What a file held.
#[derive(Debug, Default)]
pub struct Parsed {
    pub items: Vec<Item>,
    /// Things that were read in a forgiving way, in plain words.
    pub warnings: Vec<String>,
}

/// The product identifier written to files.
pub const PRODID: &str = "-//Bingux//Calendar//EN";

/// Properties the model holds in fields; every other property of an event is
/// kept as it was.
const HANDLED: &[&str] = &[
    "UID",
    "DTSTAMP",
    "CREATED",
    "LAST-MODIFIED",
    "SEQUENCE",
    "SUMMARY",
    "DESCRIPTION",
    "LOCATION",
    "URL",
    "STATUS",
    "TRANSP",
    "DTSTART",
    "DTEND",
    "DURATION",
    "RRULE",
    "EXDATE",
    "RDATE",
    "RECURRENCE-ID",
];

/// Reads every event in `text` into items of `calendar`. Events with one
/// `UID` become one item; a file of several events gives several items.
pub fn parse(text: &str, calendar: &str) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    // Some producers write a lone LF, some a BOM.
    let text = text.trim_start_matches('\u{feff}');
    let reader = ical::IcalParser::new(BufReader::new(text.as_bytes()));
    let mut found_calendar = false;
    let mut by_uid: BTreeMap<String, Vec<Read>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for parsed_calendar in reader {
        let parsed_calendar = parsed_calendar.map_err(|error| format!("{error}"))?;
        found_calendar = true;
        for component in &parsed_calendar.events {
            let Some(read) = read_event(component, &mut parsed.warnings) else {
                continue;
            };
            let uid = read.uid.clone();
            if !by_uid.contains_key(&uid) {
                order.push(uid.clone());
            }
            by_uid.entry(uid).or_default().push(read);
        }
    }
    if !found_calendar {
        return Err("The file is not an iCalendar file.".to_string());
    }
    for uid in order {
        let reads = by_uid.remove(&uid).unwrap_or_default();
        if let Some(item) = assemble(&uid, calendar, reads, &mut parsed.warnings) {
            parsed.items.push(item);
        }
    }
    Ok(parsed)
}

/// One `VEVENT` as read.
struct Read {
    uid: String,
    event: Event,
    series: Option<Series>,
    recurrence_id: Option<(Stamp, bool)>,
    sequence: u32,
    created: Option<DateTime<Utc>>,
    modified: Option<DateTime<Utc>>,
}

/// A date or a time, as a file wrote it.
#[derive(Clone, Copy, Debug)]
enum Stamp {
    Date(NaiveDate),
    Wall(NaiveDateTime, Zone),
}

fn assemble(
    uid: &str,
    calendar: &str,
    reads: Vec<Read>,
    warnings: &mut Vec<String>,
) -> Option<Item> {
    let mut masters: Vec<Read> = Vec::new();
    let mut exceptions: Vec<Read> = Vec::new();
    for read in reads {
        if read.recurrence_id.is_some() {
            exceptions.push(read);
        } else {
            masters.push(read);
        }
    }
    // Two events with one UID and no recurrence id: keep the later version.
    let master = masters.pop();
    let Some(master) = master else {
        // Only exceptions: the first stands alone as an event of its own.
        let mut first = exceptions.into_iter().next()?;
        warnings.push(format!(
            "\"{}\" changes an event that is not in the file; it is kept on its own.",
            first.event.summary
        ));
        first.recurrence_id = None;
        let mut item = Item::new(uid, calendar, first.event);
        item.sequence = first.sequence;
        item.created = first.created;
        item.modified = first.modified;
        return Some(item);
    };
    let mut item = Item::new(uid, calendar, master.event);
    item.series = master.series;
    item.sequence = master.sequence;
    item.created = master.created;
    item.modified = master.modified;
    for read in exceptions {
        let Some((stamp, and_future)) = read.recurrence_id else {
            continue;
        };
        let key = key_of(stamp, &item.event);
        item.exceptions.push(Exception {
            key,
            event: read.event,
            and_future,
        });
    }
    Some(item)
}

/// The key a stamp names, on the event's own clock.
fn key_of(stamp: Stamp, master: &Event) -> Key {
    match stamp {
        Stamp::Date(date) => {
            if master.all_day {
                date.and_hms_opt(0, 0, 0).unwrap_or_default()
            } else {
                date.and_time(master.start.time())
            }
        }
        Stamp::Wall(wall, zone) => {
            if master.all_day {
                wall.date().and_hms_opt(0, 0, 0).unwrap_or_default()
            } else if zone == master.zone || zone == Zone::Floating {
                wall
            } else {
                zone.convert(wall, master.zone)
            }
        }
    }
}

fn read_event(component: &IcalEvent, warnings: &mut Vec<String>) -> Option<Read> {
    let props = &component.properties;
    let text = |name: &str| prop(props, name).map(|p| unescape(p.value.as_deref().unwrap_or("")));
    let uid = text("UID")
        .filter(|uid| !uid.is_empty())
        .unwrap_or_else(new_uid);
    let summary = text("SUMMARY").unwrap_or_default();
    let label = if summary.is_empty() {
        "An event".to_string()
    } else {
        format!("\"{summary}\"")
    };
    let Some(start_prop) = prop(props, "DTSTART") else {
        warnings.push(format!("{label} has no start and was skipped."));
        return None;
    };
    let Some(start) = read_stamp(start_prop, warnings) else {
        warnings.push(format!(
            "{label} has a start that could not be read and was skipped."
        ));
        return None;
    };
    let mut event = match start {
        Stamp::Date(date) => {
            let mut event = Event::all_day(summary, date, date);
            event.zone = Zone::Floating;
            event
        }
        Stamp::Wall(wall, zone) => Event::new(summary, wall, zone),
    };
    // The end: DTEND, else DURATION, else the start (all-day: one day).
    let end = prop(props, "DTEND").and_then(|p| read_stamp(p, warnings));
    match (end, prop(props, "DURATION")) {
        (Some(Stamp::Date(date)), _) if event.all_day => {
            event.end = date.and_hms_opt(0, 0, 0).unwrap_or_default();
        }
        (Some(Stamp::Date(date)), _) => {
            event.end = date.and_hms_opt(0, 0, 0).unwrap_or_default();
        }
        (Some(Stamp::Wall(wall, zone)), _) if event.all_day => {
            event.end = wall.date().and_hms_opt(0, 0, 0).unwrap_or_default();
            let _ = zone;
        }
        (Some(Stamp::Wall(wall, zone)), _) => {
            event.end =
                if zone == event.zone || zone == Zone::Floating || event.zone == Zone::Floating {
                    wall
                } else {
                    zone.convert(wall, event.zone)
                };
        }
        (None, Some(duration)) => {
            if let Some(duration) = parse_duration(duration.value.as_deref().unwrap_or("")) {
                event.end = event.start + duration;
            }
        }
        (None, None) => {}
    }
    if event.all_day && event.end <= event.start {
        event.end = event.start + Duration::days(1);
    }
    if event.end < event.start {
        event.end = event.start;
    }
    event.location = text("LOCATION").unwrap_or_default();
    event.description = text("DESCRIPTION").unwrap_or_default();
    event.url = text("URL").unwrap_or_default();
    event.status = match text("STATUS")
        .as_deref()
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("TENTATIVE") => Status::Tentative,
        Some("CANCELLED") => Status::Cancelled,
        _ => Status::Confirmed,
    };
    event.availability = match text("TRANSP")
        .as_deref()
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("TRANSPARENT") => Availability::Free,
        _ => Availability::Busy,
    };
    for alarm in &component.alarms {
        if let Some(reminder) = read_alarm(alarm) {
            event.reminders.push(reminder);
        }
    }
    for property in props {
        if HANDLED
            .iter()
            .any(|name| property.name.eq_ignore_ascii_case(name))
        {
            continue;
        }
        event.extra.push(RawProp {
            name: property.name.to_ascii_uppercase(),
            params: property.params.clone().unwrap_or_default(),
            value: property.value.clone().unwrap_or_default(),
        });
    }

    let series = prop(props, "RRULE").map(|rule| {
        let mut series = Series {
            rule: rule.value.clone().unwrap_or_default(),
            ..Series::default()
        };
        for property in props
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("EXDATE"))
        {
            for stamp in read_stamp_list(property, warnings) {
                series.exclusions.push(key_of(stamp, &event));
            }
        }
        for property in props
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("RDATE"))
        {
            for stamp in read_stamp_list(property, warnings) {
                series.additions.push(key_of(stamp, &event));
            }
        }
        series
    });
    let recurrence_id = prop(props, "RECURRENCE-ID").and_then(|p| {
        let stamp = read_stamp(p, warnings)?;
        let and_future =
            param(p, "RANGE").is_some_and(|range| range.eq_ignore_ascii_case("THISANDFUTURE"));
        Some((stamp, and_future))
    });
    let sequence = text("SEQUENCE")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let created = text("CREATED").and_then(|s| parse_utc(&s));
    let modified = text("LAST-MODIFIED")
        .or_else(|| text("DTSTAMP"))
        .and_then(|s| parse_utc(&s));
    Some(Read {
        uid,
        event,
        series,
        recurrence_id,
        sequence,
        created,
        modified,
    })
}

fn read_alarm(alarm: &IcalAlarm) -> Option<Reminder> {
    let trigger = prop(&alarm.properties, "TRIGGER")?;
    let value = trigger.value.as_deref()?.trim();
    let absolute = param(trigger, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE-TIME"));
    if absolute {
        return parse_utc(value).map(|at| Reminder {
            trigger: Trigger::At(at),
        });
    }
    let offset = parse_duration(value)?;
    let from_end = param(trigger, "RELATED").is_some_and(|v| v.eq_ignore_ascii_case("END"));
    Some(Reminder {
        trigger: if from_end {
            Trigger::FromEnd(offset)
        } else {
            Trigger::FromStart(offset)
        },
    })
}

fn prop<'a>(props: &'a [Property], name: &str) -> Option<&'a Property> {
    props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

fn param<'a>(property: &'a Property, name: &str) -> Option<&'a str> {
    property
        .params
        .as_ref()?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn read_stamp(property: &Property, warnings: &mut Vec<String>) -> Option<Stamp> {
    stamp_of(property.value.as_deref()?.trim(), property, warnings)
}

fn read_stamp_list(property: &Property, warnings: &mut Vec<String>) -> Vec<Stamp> {
    property
        .value
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|part| {
            // An RDATE may be a period, `start/end`: its start is the date.
            let part = part.split('/').next().unwrap_or(part).trim();
            if part.is_empty() {
                None
            } else {
                stamp_of(part, property, warnings)
            }
        })
        .collect()
}

fn stamp_of(value: &str, property: &Property, warnings: &mut Vec<String>) -> Option<Stamp> {
    let is_date = param(property, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
        || value.len() == 8;
    if is_date {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(Stamp::Date);
    }
    let (body, utc) = match value.strip_suffix(['Z', 'z']) {
        Some(body) => (body, true),
        None => (value, false),
    };
    let wall = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M"))
        .ok()?;
    let zone = if utc {
        Zone::Utc
    } else if let Some(tzid) = param(property, "TZID") {
        match Zone::from_tzid(tzid) {
            Some(zone) => zone,
            None => {
                let note = format!(
                    "The time zone \"{tzid}\" is not known; its times are read in your time zone."
                );
                if !warnings.contains(&note) {
                    warnings.push(note);
                }
                Zone::Floating
            }
        }
    } else {
        Zone::Floating
    };
    Some(Stamp::Wall(wall, zone))
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    let body = value
        .trim()
        .strip_suffix(['Z', 'z'])
        .unwrap_or(value.trim());
    NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S")
        .ok()
        .map(|wall| wall.and_utc())
}

/// `P1DT2H30M`, `-PT15M`, `P2W`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    let (negative, rest) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let rest = rest.strip_prefix('P')?;
    let mut seconds: i64 = 0;
    let mut number = String::new();
    let mut in_time = false;
    for ch in rest.chars() {
        match ch {
            'T' => in_time = true,
            '0'..='9' => number.push(ch),
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                seconds += match (unit, in_time) {
                    ('W', false) => n * 604_800,
                    ('D', false) => n * 86_400,
                    ('H', true) => n * 3_600,
                    ('M', true) => n * 60,
                    ('S', true) => n,
                    _ => return None,
                };
            }
        }
    }
    if !number.is_empty() {
        return None;
    }
    Some(Duration::seconds(if negative { -seconds } else { seconds }))
}

/// The inverse of [`parse_duration`].
pub fn format_duration(duration: Duration) -> String {
    let total = duration.num_seconds();
    let sign = if total < 0 { "-" } else { "" };
    let mut rest = total.abs();
    if rest == 0 {
        return "PT0S".to_string();
    }
    if rest % 604_800 == 0 {
        return format!("{sign}P{}W", rest / 604_800);
    }
    let days = rest / 86_400;
    rest %= 86_400;
    let hours = rest / 3_600;
    rest %= 3_600;
    let minutes = rest / 60;
    let seconds = rest % 60;
    let mut out = format!("{sign}P");
    if days > 0 {
        out.push_str(&format!("{days}D"));
    }
    if hours + minutes + seconds > 0 {
        out.push('T');
        if hours > 0 {
            out.push_str(&format!("{hours}H"));
        }
        if minutes > 0 {
            out.push_str(&format!("{minutes}M"));
        }
        if seconds > 0 {
            out.push_str(&format!("{seconds}S"));
        }
    }
    out
}

/// Undoes `\n`, `\,`, `\;` and `\\`.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Escapes a text value.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// A unique id for a new event.
pub fn new_uid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = Utc::now();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut random = [0u8; 8];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read as _;
        let _ = file.read_exact(&mut random);
    }
    format!(
        "{}-{:04x}-{}@bingux",
        now.format("%Y%m%dT%H%M%S"),
        count & 0xffff,
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

// ---------------------------------------------------------------------------
// Writing

/// A whole `.ics` file for one item.
pub fn write(item: &Item) -> String {
    write_many(std::slice::from_ref(item))
}

/// A whole `.ics` file for several items, as an export is.
pub fn write_many(items: &[Item]) -> String {
    let mut out = Lines::default();
    out.push("BEGIN:VCALENDAR");
    out.push("VERSION:2.0");
    out.push(&format!("PRODID:{PRODID}"));
    out.push("CALSCALE:GREGORIAN");
    let mut zones: Vec<Zone> = Vec::new();
    for item in items {
        for event in std::iter::once(&item.event).chain(item.exceptions.iter().map(|e| &e.event)) {
            if matches!(event.zone, Zone::Named(_)) && !zones.contains(&event.zone) {
                zones.push(event.zone);
            }
        }
    }
    for zone in &zones {
        write_timezone(&mut out, *zone, items);
    }
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    for item in items {
        write_item(&mut out, item, &stamp);
    }
    out.push("END:VCALENDAR");
    out.finish()
}

fn write_item(out: &mut Lines, item: &Item, stamp: &str) {
    write_event(out, item, &item.event, None, stamp);
    for exception in &item.exceptions {
        write_event(out, item, &exception.event, Some(exception), stamp);
    }
}

fn write_event(
    out: &mut Lines,
    item: &Item,
    event: &Event,
    exception: Option<&Exception>,
    stamp: &str,
) {
    out.push("BEGIN:VEVENT");
    out.push(&format!("UID:{}", escape_uid(&item.uid)));
    out.push(&format!("DTSTAMP:{stamp}"));
    if let Some(created) = item.created {
        out.push(&format!("CREATED:{}", created.format("%Y%m%dT%H%M%SZ")));
    }
    if let Some(modified) = item.modified {
        out.push(&format!(
            "LAST-MODIFIED:{}",
            modified.format("%Y%m%dT%H%M%SZ")
        ));
    }
    if item.sequence > 0 {
        out.push(&format!("SEQUENCE:{}", item.sequence));
    }
    if let Some(exception) = exception {
        let range = if exception.and_future {
            ";RANGE=THISANDFUTURE"
        } else {
            ""
        };
        // The key is on the master's clock.
        let line = stamp_line(
            "RECURRENCE-ID",
            exception.key,
            item.event.zone,
            item.event.all_day,
        );
        out.push(&insert_param(&line, range));
    }
    out.push(&format!("SUMMARY:{}", escape(&event.summary)));
    out.push(&stamp_line(
        "DTSTART",
        event.start,
        event.zone,
        event.all_day,
    ));
    out.push(&stamp_line("DTEND", event.end, event.zone, event.all_day));
    if exception.is_none() {
        if let Some(series) = &item.series {
            out.push(&format!("RRULE:{}", series.rule));
            for key in &series.exclusions {
                out.push(&stamp_line("EXDATE", *key, event.zone, event.all_day));
            }
            for key in &series.additions {
                out.push(&stamp_line("RDATE", *key, event.zone, event.all_day));
            }
        }
    }
    if !event.location.is_empty() {
        out.push(&format!("LOCATION:{}", escape(&event.location)));
    }
    if !event.description.is_empty() {
        out.push(&format!("DESCRIPTION:{}", escape(&event.description)));
    }
    if !event.url.is_empty() {
        out.push(&format!("URL:{}", event.url));
    }
    match event.status {
        Status::Confirmed => {}
        Status::Tentative => out.push("STATUS:TENTATIVE"),
        Status::Cancelled => out.push("STATUS:CANCELLED"),
    }
    if event.availability == Availability::Free {
        out.push("TRANSP:TRANSPARENT");
    }
    for raw in &event.extra {
        let mut line = raw.name.clone();
        for (key, values) in &raw.params {
            let values: Vec<String> = values.iter().map(|v| quote_param(v)).collect();
            line.push_str(&format!(";{key}={}", values.join(",")));
        }
        line.push(':');
        line.push_str(&raw.value);
        out.push(&line);
    }
    for reminder in &event.reminders {
        out.push("BEGIN:VALARM");
        out.push("ACTION:DISPLAY");
        out.push(&format!("DESCRIPTION:{}", escape(&event.summary)));
        match reminder.trigger {
            Trigger::FromStart(offset) => out.push(&format!("TRIGGER:{}", format_duration(offset))),
            Trigger::FromEnd(offset) => {
                out.push(&format!("TRIGGER;RELATED=END:{}", format_duration(offset)))
            }
            Trigger::At(at) => out.push(&format!(
                "TRIGGER;VALUE=DATE-TIME:{}",
                at.format("%Y%m%dT%H%M%SZ")
            )),
        }
        out.push("END:VALARM");
    }
    out.push("END:VEVENT");
}

fn escape_uid(uid: &str) -> String {
    uid.replace(['\r', '\n'], "")
}

fn quote_param(value: &str) -> String {
    if value.contains([':', ';', ',']) {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// `NAME;PARAM:value` becomes `NAME;PARAM;extra:value`.
fn insert_param(line: &str, extra: &str) -> String {
    match line.find(':') {
        Some(at) if !extra.is_empty() => format!("{}{extra}{}", &line[..at], &line[at..]),
        _ => line.to_string(),
    }
}

fn stamp_line(name: &str, wall: NaiveDateTime, zone: Zone, all_day: bool) -> String {
    if all_day {
        return format!("{name};VALUE=DATE:{}", wall.format("%Y%m%d"));
    }
    let text = wall.format("%Y%m%dT%H%M%S");
    match zone {
        Zone::Floating => format!("{name}:{text}"),
        Zone::Utc => format!("{name}:{text}Z"),
        Zone::Named(tz) => format!("{name};TZID={}:{text}", tz.name()),
    }
}

/// A `VTIMEZONE` for `zone`. The observances are the clock changes of the
/// years the items use, one pair of rules for each year range where the rule
/// is the same, in the usual "nth weekday of the month" form. Clients that
/// know the IANA name use that; the rest read these.
fn write_timezone(out: &mut Lines, zone: Zone, items: &[Item]) {
    let Zone::Named(tz) = zone else {
        return;
    };
    let years = items
        .iter()
        .map(|item| item.event.start.year())
        .min()
        .unwrap_or_else(|| Utc::now().year());
    let year = years.clamp(1970, 2037);
    out.push("BEGIN:VTIMEZONE");
    out.push(&format!("TZID:{}", tz.name()));
    out.push(&format!("X-LIC-LOCATION:{}", tz.name()));
    let changes = transitions_in(zone, year);
    if changes.is_empty() {
        let offset = zone.offset_at(
            NaiveDate::from_ymd_opt(year, 1, 1)
                .unwrap_or_default()
                .and_hms_opt(0, 0, 0)
                .unwrap_or_default()
                .and_utc(),
        );
        out.push("BEGIN:STANDARD");
        out.push("DTSTART:19700101T000000");
        out.push(&format!("TZOFFSETFROM:{}", format_offset(offset)));
        out.push(&format!("TZOFFSETTO:{}", format_offset(offset)));
        out.push("END:STANDARD");
    } else {
        for change in &changes {
            let kind = if change.dst { "DAYLIGHT" } else { "STANDARD" };
            out.push(&format!("BEGIN:{kind}"));
            out.push(&format!("DTSTART:{}", change.local.format("%Y%m%dT%H%M%S")));
            out.push(&format!("TZOFFSETFROM:{}", format_offset(change.from)));
            out.push(&format!("TZOFFSETTO:{}", format_offset(change.to)));
            out.push(&format!(
                "RRULE:FREQ=YEARLY;BYMONTH={};BYDAY={}",
                change.local.month(),
                nth_weekday(change.local.date())
            ));
            out.push(&format!("END:{kind}"));
        }
    }
    out.push("END:VTIMEZONE");
}

struct Change {
    /// The wall-clock time of the change, on the clock before it.
    local: NaiveDateTime,
    from: i32,
    to: i32,
    dst: bool,
}

fn transitions_in(zone: Zone, year: i32) -> Vec<Change> {
    let mut changes = Vec::new();
    let start = NaiveDate::from_ymd_opt(year, 1, 1)
        .unwrap_or_default()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc();
    let mut previous = zone.offset_at(start);
    // `previous` is the offset in force; it moves at each change.
    // Hourly would be exact; every six hours finds every change the database
    // has in a year, and the minute is found by bisection below.
    let mut step = start;
    while step.year() == year {
        let next = step + Duration::hours(6);
        let offset = zone.offset_at(next);
        if offset != previous {
            let (mut low, mut high) = (step, next);
            while high - low > Duration::minutes(1) {
                let mid = low + (high - low) / 2;
                if zone.offset_at(mid) == previous {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            let local = zone.wall(high) - Duration::seconds(i64::from(offset - previous));
            // A change to a larger offset starts summer time, in either hemisphere.
            changes.push(Change {
                local,
                from: previous,
                to: offset,
                dst: offset > previous,
            });
            previous = offset;
        }
        step = next;
    }
    changes
}

fn nth_weekday(date: NaiveDate) -> String {
    let weekday = match date.weekday() {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    };
    let nth = (date.day() - 1) / 7 + 1;
    let last = date + Duration::days(7);
    if last.month() != date.month() {
        format!("-1{weekday}")
    } else {
        format!("{nth}{weekday}")
    }
}

fn format_offset(seconds: i32) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let seconds = seconds.abs();
    format!("{sign}{:02}{:02}", seconds / 3600, seconds % 3600 / 60)
}

/// Content lines, folded as they are added.
#[derive(Default)]
struct Lines {
    text: String,
}

impl Lines {
    fn push(&mut self, line: &str) {
        let mut width = 0;
        for ch in line.chars() {
            let size = ch.len_utf8();
            if width + size > 75 {
                self.text.push_str("\r\n ");
                width = 1;
            }
            self.text.push(ch);
            width += size;
        }
        self.text.push_str("\r\n");
    }

    fn finish(self) -> String {
        self.text
    }
}

/// Midnight of a date; a helper for callers that build keys.
pub fn midnight(date: NaiveDate) -> NaiveDateTime {
    date.and_time(NaiveTime::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
BEGIN:VEVENT\r\nUID:weekly-1\r\nDTSTAMP:20250101T000000Z\r\nSUMMARY:Stand-up\\, daily\r\n\
DTSTART;TZID=Europe/London:20250106T093000\r\nDTEND;TZID=Europe/London:20250106T100000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Europe/London:20250108T093000\r\n\
ATTENDEE;CN=Sam:mailto:sam@example.com\r\nX-CUSTOM:keep me\r\n\
BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nDESCRIPTION:x\r\nEND:VALARM\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:weekly-1\r\nDTSTAMP:20250101T000000Z\r\nRECURRENCE-ID;TZID=Europe/London:20250113T093000\r\n\
SUMMARY:Stand-up (moved)\r\nDTSTART;TZID=Europe/London:20250113T110000\r\nDTEND;TZID=Europe/London:20250113T113000\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:holiday\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20250801\r\nDTEND;VALUE=DATE:20250804\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn reads_series_exceptions_and_all_day() {
        let parsed = parse(SAMPLE, "home").unwrap();
        assert_eq!(parsed.items.len(), 2);
        let weekly = &parsed.items[0];
        assert_eq!(weekly.event.summary, "Stand-up, daily");
        assert_eq!(
            weekly.series.as_ref().unwrap().rule,
            "FREQ=WEEKLY;BYDAY=MO,WE"
        );
        assert_eq!(weekly.series.as_ref().unwrap().exclusions.len(), 1);
        assert_eq!(weekly.exceptions.len(), 1);
        assert_eq!(weekly.exceptions[0].event.summary, "Stand-up (moved)");
        assert_eq!(weekly.event.reminders[0].minutes_before(), Some(15));
        assert!(weekly.event.extra.iter().any(|p| p.name == "ATTENDEE"));
        let holiday = &parsed.items[1];
        assert!(holiday.event.all_day);
        assert_eq!(
            holiday.event.last_day(),
            NaiveDate::from_ymd_opt(2025, 8, 3).unwrap()
        );
    }

    #[test]
    fn writing_then_reading_keeps_everything() {
        let parsed = parse(SAMPLE, "home").unwrap();
        let text = write_many(&parsed.items);
        assert!(text.contains("BEGIN:VTIMEZONE"));
        let again = parse(&text, "home").unwrap();
        assert_eq!(again.items.len(), 2);
        let (a, b) = (&parsed.items[0], &again.items[0]);
        assert_eq!(a.event.summary, b.event.summary);
        assert_eq!(a.event.start, b.event.start);
        assert_eq!(a.series, b.series);
        assert_eq!(a.exceptions.len(), b.exceptions.len());
        assert_eq!(a.event.extra, b.event.extra);
        assert_eq!(a.event.reminders, b.event.reminders);
        assert_eq!(parsed.items[1].event, again.items[1].event);
    }

    #[test]
    fn long_lines_fold_at_75_octets_and_unfold_again() {
        let mut event = Event::new(
            "é".repeat(100),
            NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            Zone::Utc,
        );
        event.description = "line one\nline two; with, marks \\ and more".repeat(5);
        let item = Item::new("fold@test", "c", event);
        let text = write(&item);
        assert!(text.split("\r\n").all(|line| line.len() <= 75));
        let back = parse(&text, "c").unwrap();
        assert_eq!(back.items[0].event.summary, item.event.summary);
        assert_eq!(back.items[0].event.description, item.event.description);
    }

    #[test]
    fn durations_round_trip() {
        for text in ["-PT15M", "PT0S", "-P1D", "P2W", "PT1H30M", "-P1DT2H"] {
            assert_eq!(format_duration(parse_duration(text).unwrap()), text);
        }
        assert_eq!(parse_duration("P1DT"), Some(Duration::days(1)));
        assert_eq!(parse_duration("15M"), None);
    }

    #[test]
    fn an_end_before_the_start_is_the_start_and_a_missing_end_is_a_day() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nSUMMARY:A\nDTSTART:20250101T100000\nDTEND:20250101T090000\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:b\nSUMMARY:B\nDTSTART;VALUE=DATE:20250101\nEND:VEVENT\nEND:VCALENDAR\n";
        let parsed = parse(text, "c").unwrap();
        assert_eq!(parsed.items[0].event.end, parsed.items[0].event.start);
        assert_eq!(parsed.items[1].event.duration(), Duration::days(1));
    }

    #[test]
    fn an_unknown_zone_is_read_as_floating_with_a_warning() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nSUMMARY:A\nDTSTART;TZID=Mars/Olympus:20250101T100000\nEND:VEVENT\nEND:VCALENDAR\n";
        let parsed = parse(text, "c").unwrap();
        assert_eq!(parsed.items[0].event.zone, Zone::Floating);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn a_file_that_is_not_a_calendar_is_an_error() {
        assert!(parse("hello", "c").is_err());
    }

    #[test]
    fn the_timezone_block_has_the_london_rules() {
        let event = Event::new(
            "x",
            NaiveDate::from_ymd_opt(2025, 6, 1)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            Zone::from_tzid("Europe/London").unwrap(),
        );
        let text = write(&Item::new("z@test", "c", event));
        assert!(text.contains("BYMONTH=3;BYDAY=-1SU"), "{text}");
        assert!(text.contains("BYMONTH=10;BYDAY=-1SU"), "{text}");
        assert!(text.contains("TZOFFSETTO:+0100"));
    }
}
