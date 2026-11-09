//! Reminders: which alarms fall in a span of time, firing one once, and
//! showing it through `org.freedesktop.Notifications` with a fallback.
//!
//! Two programs can run this at once, the window and the headless
//! `bingux-calendar --reminders`. An alarm is fired by whichever one first
//! creates its marker file under the state folder, so it shows once.

use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration as StdDuration, SystemTime};

use chrono::{DateTime, Duration, Utc};

use crate::event::{Item, Key, Trigger};
use crate::recur;
use crate::store::{Snapshot, Store, state_dir};
use crate::zone::Zone;

/// How long after its time an alarm still shows, for a machine that was
/// asleep or an app that was not running.
pub const GRACE: Duration = Duration::minutes(30);

/// An alarm: when it goes off, and for what.
#[derive(Clone, Debug, PartialEq)]
pub struct Alarm {
    pub uid: String,
    pub calendar: String,
    pub key: Option<Key>,
    pub summary: String,
    pub location: String,
    pub all_day: bool,
    /// When it goes off.
    pub at: DateTime<Utc>,
    /// When the event starts.
    pub starts: DateTime<Utc>,
}

impl Alarm {
    /// "Today, 13:00 at Nando's" is for the body of the notification; the
    /// caller formats times, since it knows the clock the person reads.
    pub fn identity(&self) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.uid.hash(&mut hasher);
        self.key.hash(&mut hasher);
        self.at.timestamp().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// The alarms of `snapshot` that go off in `after..=until`, soonest first.
pub fn upcoming(
    snapshot: &Snapshot,
    after: DateTime<Utc>,
    until: DateTime<Utc>,
    view: Zone,
) -> Vec<Alarm> {
    let mut alarms = Vec::new();
    // An alarm can be set days ahead of its event: look that far past `until`.
    let reach = Duration::days(8);
    let from = view.wall(after - Duration::days(1));
    let to = view.wall(until + reach);
    for item in snapshot.items.iter().filter(|item| has_reminders(item)) {
        for occurrence in recur::occurrences(item, from, to, view) {
            let event = recur::event_at(item, occurrence.key);
            let starts = if occurrence.all_day {
                Zone::Floating.instant(occurrence.start, view)
            } else {
                view.instant(occurrence.start, view)
            };
            let ends = if occurrence.all_day {
                Zone::Floating.instant(occurrence.end, view)
            } else {
                view.instant(occurrence.end, view)
            };
            for reminder in &event.reminders {
                let at = match reminder.trigger {
                    Trigger::FromStart(offset) => starts + offset,
                    Trigger::FromEnd(offset) => ends + offset,
                    Trigger::At(at) => {
                        // An absolute alarm is for the event as first written.
                        if occurrence.key.is_some() && occurrence.key != Some(item.event.start) {
                            continue;
                        }
                        at
                    }
                };
                if at > after && at <= until {
                    alarms.push(Alarm {
                        uid: item.uid.clone(),
                        calendar: item.calendar.clone(),
                        key: occurrence.key,
                        summary: occurrence.summary.clone(),
                        location: occurrence.location.clone(),
                        all_day: occurrence.all_day,
                        at,
                        starts,
                    });
                }
            }
        }
    }
    alarms.sort_by_key(|alarm| alarm.at);
    alarms.dedup_by(|a, b| a.identity() == b.identity());
    alarms
}

fn has_reminders(item: &Item) -> bool {
    !item.event.reminders.is_empty()
        || item
            .exceptions
            .iter()
            .any(|exception| !exception.event.reminders.is_empty())
}

fn fired_dir() -> PathBuf {
    state_dir().join("fired")
}

/// Takes the right to show `alarm`. `true` for the first caller; `false`
/// when it has been shown already, here or by the other program.
pub fn claim(alarm: &Alarm) -> bool {
    let dir = fired_dir();
    if fs::create_dir_all(&dir).is_err() {
        // Without a place to record it, show it: a duplicate is better than none.
        return true;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(alarm.identity()))
        .is_ok()
}

/// Removes markers older than a month, so the folder does not grow.
pub fn forget_old() {
    let Ok(entries) = fs::read_dir(fired_dir()) else {
        return;
    };
    let cutoff = SystemTime::now() - StdDuration::from_secs(31 * 24 * 3600);
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified < cutoff);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// How an alarm was shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// By the notification service on the session bus.
    Service,
    /// By the `notify-send` program.
    Program,
    /// Not shown: neither is there. The caller shows it in its own way.
    None,
}

/// Shows a notification. Tries the service, then `notify-send`.
pub fn notify(summary: &str, body: &str) -> Delivery {
    if notify_service(summary, body).is_ok() {
        return Delivery::Service;
    }
    let sent = Command::new("notify-send")
        .args([
            "--app-name=Calendar",
            "--icon=bingux-calendar",
            "--category=reminder",
            "--hint=string:desktop-entry:bingux-calendar",
            "--",
            summary,
            body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if sent {
        Delivery::Program
    } else {
        Delivery::None
    }
}

fn notify_service(summary: &str, body: &str) -> Result<(), zbus::Error> {
    use zbus::zvariant::Value;
    let connection = zbus::blocking::Connection::session()?;
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("desktop-entry", Value::from("bingux-calendar"));
    hints.insert("category", Value::from("reminder"));
    connection.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "Calendar",
            0u32,
            "bingux-calendar",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    )?;
    Ok(())
}

/// The words of a notification for `alarm`: the title, and a line saying
/// when it starts and where. `format` writes an instant the way the person
/// reads it.
pub fn message(
    alarm: &Alarm,
    now: DateTime<Utc>,
    view: Zone,
    twelve_hour: bool,
) -> (String, String) {
    let start = view.wall(alarm.starts);
    let minutes = (alarm.starts - now).num_minutes();
    let when = if alarm.all_day {
        let days = (start.date() - view.wall(now).date()).num_days();
        match days {
            0 => "Today".to_string(),
            1 => "Tomorrow".to_string(),
            n if n > 1 => format!("In {n} days"),
            _ => start.format("%-d %B").to_string(),
        }
    } else if (0..60).contains(&minutes) {
        match minutes {
            0 => "Now".to_string(),
            1 => "In 1 minute".to_string(),
            n => format!("In {n} minutes"),
        }
    } else if minutes < 0 && minutes > -120 {
        let ago = -minutes;
        if ago == 1 {
            "Started 1 minute ago".to_string()
        } else {
            format!("Started {ago} minutes ago")
        }
    } else {
        let clock = if twelve_hour {
            start.format("%-I:%M %p").to_string()
        } else {
            start.format("%H:%M").to_string()
        };
        if start.date() == view.wall(now).date() {
            format!("Today at {clock}")
        } else {
            format!("{} at {clock}", start.format("%A %-d %B"))
        }
    };
    let body = if alarm.location.is_empty() {
        when
    } else {
        format!("{when}, {}", alarm.location)
    };
    let summary = if alarm.summary.is_empty() {
        "Event".to_string()
    } else {
        alarm.summary.clone()
    };
    (summary, body)
}

/// Runs reminders until `stop` is set: reads the store when it changes,
/// shows each alarm as it comes due, and calls `shown` for each (with how it
/// was shown) so a window can fall back to its own notice.
pub fn run(
    store: &Store,
    stop: &Arc<AtomicBool>,
    twelve_hour: impl Fn() -> bool,
    mut shown: impl FnMut(&Alarm, Delivery),
) {
    forget_old();
    let mut snapshot = store.load().snapshot;
    let mut print = store.fingerprint();
    let mut checked = Utc::now() - GRACE;
    while !stop.load(Ordering::Relaxed) {
        let now = Utc::now();
        let view = Zone::detect_local();
        let latest = store.fingerprint();
        if latest != print {
            snapshot = store.load().snapshot;
            print = latest;
        }
        for alarm in upcoming(&snapshot, checked, now, view) {
            if claim(&alarm) {
                let (title, body) = message(&alarm, now, view, twelve_hour());
                let delivery = notify(&title, &body);
                shown(&alarm, delivery);
            }
        }
        checked = now;
        // Sleep in short steps so a stop is heard, and so a change of the
        // clock (resume from sleep) is noticed within seconds.
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(StdDuration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{NaiveDate, TimeZone};

    use super::*;
    use crate::event::{Event, Reminder, Series};

    fn utc(h: u32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 3, d, h, m, 0).unwrap()
    }

    fn snapshot_with(item: Item) -> Snapshot {
        Snapshot {
            calendars: Vec::new(),
            items: vec![Arc::new(item)],
        }
    }

    #[test]
    fn a_reminder_goes_off_before_the_start() {
        let mut event = Event::new(
            "Call",
            NaiveDate::from_ymd_opt(2025, 3, 10)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            Zone::Utc,
        );
        event.reminders.push(Reminder::before(15));
        let alarms = upcoming(
            &snapshot_with(Item::new("a", "c", event)),
            utc(0, 0, 10),
            utc(23, 0, 10),
            Zone::Utc,
        );
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].at, utc(8, 45, 10));
    }

    #[test]
    fn a_repeating_event_has_an_alarm_for_each_occurrence() {
        let mut event = Event::new(
            "Daily",
            NaiveDate::from_ymd_opt(2025, 3, 3)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            Zone::Utc,
        );
        event.reminders.push(Reminder::before(10));
        let mut item = Item::new("d", "c", event);
        item.series = Some(Series {
            rule: "FREQ=DAILY".into(),
            ..Series::default()
        });
        let alarms = upcoming(
            &snapshot_with(item),
            utc(0, 0, 5),
            utc(23, 59, 7),
            Zone::Utc,
        );
        assert_eq!(alarms.len(), 3);
    }

    #[test]
    fn an_alarm_is_claimed_once() {
        // The state folder is the user's; this test only needs a unique alarm.
        let alarm = Alarm {
            uid: format!("claim-test-{}", std::process::id()),
            calendar: "c".into(),
            key: None,
            summary: "x".into(),
            location: String::new(),
            all_day: false,
            at: utc(9, 0, 1),
            starts: utc(9, 15, 1),
        };
        let dir = fired_dir();
        let _ = fs::remove_file(dir.join(alarm.identity()));
        assert!(claim(&alarm));
        assert!(!claim(&alarm));
        let _ = fs::remove_file(dir.join(alarm.identity()));
    }

    #[test]
    fn the_message_says_when() {
        let alarm = Alarm {
            uid: "u".into(),
            calendar: "c".into(),
            key: None,
            summary: "Lunch".into(),
            location: "Nando's".into(),
            all_day: false,
            at: utc(12, 45, 1),
            starts: utc(13, 0, 1),
        };
        let (title, body) = message(&alarm, utc(12, 45, 1), Zone::Utc, false);
        assert_eq!(title, "Lunch");
        assert_eq!(body, "In 15 minutes, Nando's");
    }
}
