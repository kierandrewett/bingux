//! `bingux-calendar --snapshot DIR [empty]`: the window to PNG without a
//! compositor, on a calendar of made-up events around today.
//!
//! The events are written to a folder of their own (the real calendars are
//! not touched), then the window is driven through its views with the keys a
//! person would press. `BINGUX_CALENDAR_SNAPSHOT_SIZE="WxH"` picks the window
//! size, and `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light|prefer-dark` the
//! colour scheme. With `empty` as the third argument the calendar has no
//! events, for the first-launch state; with `editor` the frames are an event
//! chosen with the keyboard and its editor (a popup stays open in a snapshot,
//! so it has a run of its own).

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bingux_calendar_core::chrono::{Datelike, Duration as Span, Local, NaiveDate};
use bingux_ui::nav::{Location, RouteTable, provide_router};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::app;

const SIZE: (f32, f32) = (1180.0, 780.0);

/// A calendar folder: its name, its colour and its events.
fn calendar(root: &Path, id: &str, name: &str, colour: &str, events: &str) {
    let folder = root.join("bingux-calendar").join("calendars").join(id);
    fs::create_dir_all(&folder).expect("a snapshot folder");
    fs::write(folder.join("displayname"), name).expect("a name");
    fs::write(folder.join("color"), colour).expect("a colour");
    for (index, event) in events
        .split("\n\n")
        .filter(|e| !e.trim().is_empty())
        .enumerate()
    {
        let text = format!(
            "BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Bingux//Snapshot//EN\n{event}\nEND:VCALENDAR\n"
        );
        fs::write(folder.join(format!("{id}-{index}.ics")), text).expect("an event");
    }
}

fn stamp(date: NaiveDate, time: &str) -> String {
    format!("{}T{time}", date.format("%Y%m%d"))
}

fn event(uid: &str, summary: &str, start: &str, end: &str, extra: &str) -> String {
    format!(
        "BEGIN:VEVENT\nUID:{uid}\nSUMMARY:{summary}\nDTSTART:{start}\nDTEND:{end}\n{extra}END:VEVENT\n"
    )
}

fn all_day(uid: &str, summary: &str, first: NaiveDate, last: NaiveDate, extra: &str) -> String {
    format!(
        "BEGIN:VEVENT\nUID:{uid}\nSUMMARY:{summary}\nDTSTART;VALUE=DATE:{}\nDTEND;VALUE=DATE:{}\n{extra}END:VEVENT\n",
        first.format("%Y%m%d"),
        (last + Span::days(1)).format("%Y%m%d")
    )
}

const ALARM: &str =
    "BEGIN:VALARM\nACTION:DISPLAY\nTRIGGER:-PT10M\nDESCRIPTION:Reminder\nEND:VALARM\n";

/// Events around the week that holds today.
fn fixture(root: &Path, empty: bool) {
    let today = Local::now().date_naive();
    let monday = today - Span::days(i64::from(today.weekday().num_days_from_monday()));
    let day = |offset: i64| monday + Span::days(offset);
    let at = |offset: i64, time: &str| stamp(day(offset), time);
    let mut work = String::new();
    let mut home = String::new();
    let mut family = String::new();
    if !empty {
        work.push_str(&event(
            "standup",
            "Stand-up",
            &at(0, "093000"),
            &at(0, "094500"),
            "RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\n",
        ));
        work.push('\n');
        work.push_str(&event(
            "review",
            "Design review",
            &at(1, "100000"),
            &at(1, "113000"),
            ALARM,
        ));
        work.push('\n');
        work.push_str(&event(
            "one-one",
            "1:1 with Sam",
            &at(1, "103000"),
            &at(1, "110000"),
            "",
        ));
        work.push('\n');
        work.push_str(&event(
            "roadmap",
            "Roadmap",
            &at(1, "101500"),
            &at(1, "120000"),
            "",
        ));
        work.push('\n');
        work.push_str(&event(
            "planning",
            "Planning",
            &at(2, "140000"),
            &at(2, "160000"),
            "LOCATION:Room 2\n",
        ));
        work.push('\n');
        work.push_str(&event(
            "tokyo",
            "Call with Tokyo",
            &at(3, "090000"),
            &at(3, "100000"),
            "",
        ));
        work.push('\n');
        work.push_str(&all_day("offsite", "Team offsite", day(7), day(9), ""));
        home.push_str(&event(
            "lunch",
            "Lunch with Sam",
            &at(3, "123000"),
            &at(3, "133000"),
            "LOCATION:Nando's\n",
        ));
        home.push('\n');
        home.push_str(&event(
            "gym",
            "Gym",
            &at(0, "180000"),
            &at(0, "190000"),
            "RRULE:FREQ=WEEKLY;BYDAY=MO,WE\n",
        ));
        home.push('\n');
        home.push_str(&event(
            "dentist",
            "Dentist",
            &at(4, "160000"),
            &at(4, "164500"),
            ALARM,
        ));
        home.push('\n');
        home.push_str(&event(
            "dinner",
            "Dinner out",
            &at(5, "190000"),
            &at(5, "213000"),
            "LOCATION:Bella Italia\n",
        ));
        family.push_str(&all_day(
            "birthday",
            "Mum\u{2019}s birthday",
            day(5),
            day(5),
            "RRULE:FREQ=YEARLY\n",
        ));
        family.push('\n');
        family.push_str(&all_day(
            "holiday",
            "Holiday in Lisbon",
            day(11),
            day(14),
            "",
        ));
        family.push('\n');
        family.push_str(&all_day(
            "bins",
            "Bins out",
            day(2),
            day(2),
            "RRULE:FREQ=WEEKLY;INTERVAL=2\n",
        ));
        family.push('\n');
        family.push_str(&event(
            "parents",
            "Parents\u{2019} evening",
            &at(8, "170000"),
            &at(8, "183000"),
            "",
        ));
    }
    calendar(root, "personal", "Personal", "#3584e4", &home);
    calendar(root, "work", "Work", "#5a9a53", &work);
    calendar(root, "family", "Family", "#d5619a", &family);
}

pub fn run(dir: &str, mode: Option<&str>) {
    let empty = mode == Some("empty");
    let scratch =
        std::env::temp_dir().join(format!("bingux-calendar-snapshot-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    fixture(&scratch, empty);
    // SAFETY: the snapshot runs single threaded here, before any window or
    // worker exists.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", &scratch);
        std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
        std::env::set_var("XDG_STATE_HOME", scratch.join("state"));
    }
    fs::create_dir_all(dir).expect("the snapshot folder");

    let size = std::env::var("BINGUX_CALENDAR_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE);
    let mut shot = Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            let table = RouteTable::new("bingux-calendar");
            provide_router(Arc::new(table), &Location::new("bingux-calendar"));
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    );
    shot.settle(Duration::from_millis(2500));
    let suffix = std::env::var("BINGUX_CALENDAR_SNAPSHOT_NAME").unwrap_or_default();
    let compact = size.0 < 740.0;
    let frames: &[(&str, &str)] = if empty {
        &[("empty", "")]
    } else if mode == Some("editor") {
        &[
            (
                "selected",
                "key 2; wait 300; key down; key down; key down; wait 400",
            ),
            ("editor", "key return; wait 900"),
        ]
    } else if compact {
        &[
            ("week", ""),
            ("month", "key 3; wait 600"),
            ("agenda", "key 5; wait 600"),
            ("quick", "key 2; wait 400; key n; wait 700"),
        ]
    } else {
        &[
            ("week", ""),
            ("day", "key 1; wait 600"),
            ("month", "key 3; wait 600"),
            ("year", "key 4; wait 600"),
            ("agenda", "key 5; wait 600"),
            (
                "quick",
                "key 2; wait 400; key n; wait 700; type Lunch with Sam at Nando's tomorrow 1pm; wait 500",
            ),
        ]
    };
    for (name, script) in frames {
        if let Err(error) = shot.run_script(script, "bingux-calendar") {
            eprintln!("[snapshot] {name}: script stopped: {error}");
        }
        let path = format!("{dir}/calendar-{name}{suffix}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}
