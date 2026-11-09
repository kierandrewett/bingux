//! Repeating events, checked against the cases that go wrong: month ends,
//! leap days, week starts, clock changes, the start that does not match its
//! rule, `UNTIL` on another clock, changed and removed occurrences, and
//! edits that split a series.

use chrono::NaiveDateTime;

use crate::event::{Item, Key};
use crate::ics;
use crate::recur::{self, Change, Scope};
use crate::zone::Zone;

fn wall(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
}

/// An item from the lines of one VEVENT (and any others sharing its UID).
fn item(lines: &str) -> Item {
    let text = format!("BEGIN:VCALENDAR\nVERSION:2.0\n{lines}\nEND:VCALENDAR\n");
    let parsed = ics::parse(&text, "test").unwrap();
    assert_eq!(parsed.items.len(), 1, "{:?}", parsed.warnings);
    parsed.items.into_iter().next().unwrap()
}

fn event(extra: &str) -> Item {
    item(&format!(
        "BEGIN:VEVENT\nUID:t\nSUMMARY:T\n{extra}\nEND:VEVENT"
    ))
}

/// The starts, as `YYYY-MM-DD HH:MM` on `view`, of what touches the window.
fn starts(item: &Item, from: &str, to: &str, view: Zone) -> Vec<String> {
    recur::occurrences(item, wall(from), wall(to), view)
        .iter()
        .map(|o| o.start.format("%Y-%m-%d %H:%M").to_string())
        .collect()
}

fn utc_starts(item: &Item, from: &str, to: &str) -> Vec<String> {
    starts(item, from, to, Zone::Utc)
}

fn london() -> Zone {
    Zone::from_tzid("Europe/London").unwrap()
}

fn saved(changes: &[Change], at: usize) -> Item {
    match &changes[at] {
        Change::Save(item) => item.clone(),
        other => panic!("not a save: {other:?}"),
    }
}

#[test]
fn monthly_on_the_31st_skips_short_months() {
    let e =
        event("DTSTART:20250131T090000Z\nDTEND:20250131T100000Z\nRRULE:FREQ=MONTHLY;BYMONTHDAY=31");
    assert_eq!(
        utc_starts(&e, "2025-01-01 00:00", "2025-09-01 00:00"),
        [
            "2025-01-31 09:00",
            "2025-03-31 09:00",
            "2025-05-31 09:00",
            "2025-07-31 09:00",
            "2025-08-31 09:00"
        ]
    );
}

#[test]
fn monthly_on_the_last_friday() {
    let e =
        event("DTSTART:20250131T090000Z\nDTEND:20250131T100000Z\nRRULE:FREQ=MONTHLY;BYDAY=-1FR");
    assert_eq!(
        utc_starts(&e, "2025-01-01 00:00", "2025-04-01 00:00"),
        ["2025-01-31 09:00", "2025-02-28 09:00", "2025-03-28 09:00"]
    );
}

#[test]
fn the_last_weekday_of_the_month_by_set_position() {
    let e = event(
        "DTSTART:20250131T090000Z\nDTEND:20250131T100000Z\nRRULE:FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1",
    );
    assert_eq!(
        utc_starts(&e, "2025-01-01 00:00", "2025-06-01 00:00"),
        [
            "2025-01-31 09:00",
            "2025-02-28 09:00",
            "2025-03-31 09:00",
            "2025-04-30 09:00",
            "2025-05-30 09:00"
        ]
    );
}

#[test]
fn yearly_on_the_29th_of_february_only_in_leap_years() {
    let e = event("DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY");
    assert_eq!(
        utc_starts(&e, "2024-01-01 00:00", "2033-01-01 00:00"),
        ["2024-02-29 00:00", "2028-02-29 00:00", "2032-02-29 00:00"]
    );
}

#[test]
fn thanksgiving_is_the_fourth_thursday_of_november() {
    let e = event(
        "DTSTART:20250101T120000Z\nDTEND:20250101T130000Z\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=4TH",
    );
    assert_eq!(
        utc_starts(&e, "2025-01-01 00:00", "2027-01-01 00:00"),
        ["2025-01-01 12:00", "2025-11-27 12:00", "2026-11-26 12:00"]
    );
}

#[test]
fn the_week_start_changes_a_fortnightly_rule() {
    // RFC 5545 section 3.3.10: the same rule, two answers.
    let monday = event(
        "DTSTART:19970805T090000Z\nDTEND:19970805T100000Z\nRRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO",
    );
    assert_eq!(
        utc_starts(&monday, "1997-08-01 00:00", "1997-09-30 00:00"),
        [
            "1997-08-05 09:00",
            "1997-08-10 09:00",
            "1997-08-19 09:00",
            "1997-08-24 09:00"
        ]
    );
    let sunday = event(
        "DTSTART:19970805T090000Z\nDTEND:19970805T100000Z\nRRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU",
    );
    assert_eq!(
        utc_starts(&sunday, "1997-08-01 00:00", "1997-09-30 00:00"),
        [
            "1997-08-05 09:00",
            "1997-08-17 09:00",
            "1997-08-19 09:00",
            "1997-08-31 09:00"
        ]
    );
}

#[test]
fn a_start_that_does_not_match_its_rule_is_still_the_first_occurrence() {
    // 2025-03-12 is a Wednesday; the rule says Mondays.
    let e = event(
        "DTSTART:20250312T090000Z\nDTEND:20250312T100000Z\nRRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=3",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-05-01 00:00"),
        ["2025-03-12 09:00", "2025-03-17 09:00", "2025-03-24 09:00"]
    );
}

#[test]
fn a_daily_event_keeps_its_wall_clock_across_the_change() {
    // London springs forward on 2025-03-30.
    let e = event(
        "DTSTART;TZID=Europe/London:20250328T090000\nDTEND;TZID=Europe/London:20250328T100000\nRRULE:FREQ=DAILY;COUNT=4",
    );
    assert_eq!(
        starts(&e, "2025-03-27 00:00", "2025-04-05 00:00", london()),
        [
            "2025-03-28 09:00",
            "2025-03-29 09:00",
            "2025-03-30 09:00",
            "2025-03-31 09:00"
        ]
    );
    // Seen from UTC, 09:00 BST is 08:00.
    assert_eq!(
        utc_starts(&e, "2025-03-27 00:00", "2025-04-05 00:00"),
        [
            "2025-03-28 09:00",
            "2025-03-29 09:00",
            "2025-03-30 08:00",
            "2025-03-31 08:00"
        ]
    );
}

#[test]
fn an_occurrence_in_the_hour_that_does_not_exist_is_kept() {
    // 01:30 does not exist in London on 2025-03-30; the day still has its occurrence.
    let e = event(
        "DTSTART;TZID=Europe/London:20250329T013000\nDTEND;TZID=Europe/London:20250329T020000\nRRULE:FREQ=DAILY;COUNT=3",
    );
    let found = starts(&e, "2025-03-29 00:00", "2025-04-02 00:00", london());
    assert_eq!(found.len(), 3, "{found:?}");
    assert_eq!(found[1], "2025-03-30 02:30");
}

#[test]
fn an_occurrence_in_the_repeated_hour_is_shown_once() {
    // London falls back on 2025-10-26: 01:30 happens twice.
    let e = event(
        "DTSTART;TZID=Europe/London:20251025T013000\nDTEND;TZID=Europe/London:20251025T020000\nRRULE:FREQ=DAILY;COUNT=3",
    );
    let found = starts(&e, "2025-10-25 00:00", "2025-10-29 00:00", london());
    assert_eq!(
        found,
        ["2025-10-25 01:30", "2025-10-26 01:30", "2025-10-27 01:30"]
    );
}

#[test]
fn an_hour_long_event_is_an_hour_across_the_change() {
    let e = event(
        "DTSTART;TZID=Europe/London:20250330T003000\nDTEND;TZID=Europe/London:20250330T023000\nRRULE:FREQ=DAILY;COUNT=1",
    );
    let o = &recur::occurrences(
        &e,
        wall("2025-03-30 00:00"),
        wall("2025-03-31 00:00"),
        Zone::Utc,
    )[0];
    // 00:30 GMT to 02:30 BST is one hour.
    assert_eq!(o.end - o.start, chrono::Duration::hours(1));
}

#[test]
fn until_in_utc_is_read_on_the_events_clock() {
    // Tuesdays at 23:30 New York; the last wanted one is 2025-03-11 (EDT, UTC-4),
    // which is 03:30 UTC on the 12th, so UNTIL=20250312T033000Z includes it.
    let e = event(
        "DTSTART;TZID=America/New_York:20250304T233000\nDTEND;TZID=America/New_York:20250305T003000\nRRULE:FREQ=WEEKLY;UNTIL=20250312T033000Z",
    );
    let ny = Zone::from_tzid("America/New_York").unwrap();
    assert_eq!(
        starts(&e, "2025-03-01 00:00", "2025-04-30 00:00", ny),
        ["2025-03-04 23:30", "2025-03-11 23:30"]
    );
}

#[test]
fn an_all_day_until_date_is_included() {
    let e = event("DTSTART;VALUE=DATE:20250303\nRRULE:FREQ=DAILY;UNTIL=20250305");
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-03-31 00:00"),
        ["2025-03-03 00:00", "2025-03-04 00:00", "2025-03-05 00:00"]
    );
}

#[test]
fn removed_and_added_occurrences() {
    let e = event(
        "DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=DAILY;COUNT=4\nEXDATE:20250304T090000Z\nRDATE:20250310T090000Z",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-03-31 00:00"),
        [
            "2025-03-03 09:00",
            "2025-03-05 09:00",
            "2025-03-06 09:00",
            "2025-03-10 09:00"
        ]
    );
}

#[test]
fn an_exdate_in_utc_removes_a_zoned_occurrence() {
    // 09:00 London in winter is 09:00 UTC.
    let e = event(
        "DTSTART;TZID=Europe/London:20250303T090000\nDTEND;TZID=Europe/London:20250303T100000\nRRULE:FREQ=DAILY;COUNT=3\nEXDATE:20250304T090000Z",
    );
    assert_eq!(
        starts(&e, "2025-03-01 00:00", "2025-03-31 00:00", london()),
        ["2025-03-03 09:00", "2025-03-05 09:00"]
    );
}

#[test]
fn a_changed_occurrence_is_where_it_moved_and_not_where_it_was() {
    let e = item(
        "BEGIN:VEVENT\nUID:t\nSUMMARY:T\nDTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=DAILY;COUNT=3\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:t\nSUMMARY:Moved\nRECURRENCE-ID:20250304T090000Z\nDTSTART:20250310T140000Z\nDTEND:20250310T150000Z\nEND:VEVENT",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-03-09 00:00"),
        ["2025-03-03 09:00", "2025-03-05 09:00"]
    );
    let moved = recur::occurrences(
        &e,
        wall("2025-03-10 00:00"),
        wall("2025-03-11 00:00"),
        Zone::Utc,
    );
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].summary, "Moved");
    assert!(moved[0].changed);
}

#[test]
fn this_and_future_changes_shift_every_later_occurrence() {
    let e = item(
        "BEGIN:VEVENT\nUID:t\nSUMMARY:T\nDTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=DAILY;COUNT=5\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:t\nSUMMARY:Later\nRECURRENCE-ID;RANGE=THISANDFUTURE:20250305T090000Z\nDTSTART:20250305T110000Z\nDTEND:20250305T120000Z\nEND:VEVENT",
    );
    let found = recur::occurrences(
        &e,
        wall("2025-03-01 00:00"),
        wall("2025-03-31 00:00"),
        Zone::Utc,
    );
    let shown: Vec<_> = found
        .iter()
        .map(|o| format!("{} {}", o.start.format("%d %H:%M"), o.summary))
        .collect();
    assert_eq!(
        shown,
        [
            "03 09:00 T",
            "04 09:00 T",
            "05 11:00 Later",
            "06 11:00 Later",
            "07 11:00 Later"
        ]
    );
}

#[test]
fn a_zoned_event_is_on_another_day_in_another_zone() {
    // 08:00 on 2 January in Tokyo is 15:00 on 1 January in Los Angeles.
    let e = event("DTSTART;TZID=Asia/Tokyo:20250102T080000\nDTEND;TZID=Asia/Tokyo:20250102T090000");
    let la = Zone::from_tzid("America/Los_Angeles").unwrap();
    assert_eq!(
        starts(&e, "2025-01-01 00:00", "2025-01-02 00:00", la),
        ["2025-01-01 15:00"]
    );
}

#[test]
fn a_multi_day_event_shows_in_a_window_that_starts_inside_it() {
    let e =
        event("DTSTART;VALUE=DATE:20250301\nDTEND;VALUE=DATE:20250305\nRRULE:FREQ=WEEKLY;COUNT=2");
    let found = recur::occurrences(
        &e,
        wall("2025-03-03 00:00"),
        wall("2025-03-04 00:00"),
        Zone::Utc,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].days(),
        (
            wall("2025-03-01 00:00").date(),
            wall("2025-03-04 00:00").date()
        )
    );
}

#[test]
fn a_rule_started_long_ago_expands_in_a_far_window() {
    let e = event("DTSTART:20150101T090000Z\nDTEND:20150101T100000Z\nRRULE:FREQ=DAILY");
    assert_eq!(
        utc_starts(&e, "2040-06-10 00:00", "2040-06-13 00:00"),
        ["2040-06-10 09:00", "2040-06-11 09:00", "2040-06-12 09:00"]
    );
}

#[test]
fn a_rule_that_cannot_be_read_leaves_one_event() {
    let e = event("DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=SOMETIMES");
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-04-01 00:00"),
        ["2025-03-03 09:00"]
    );
}

#[test]
fn a_floating_event_is_the_same_wall_clock_in_every_zone() {
    let e = event("DTSTART:20250303T090000\nDTEND:20250303T100000");
    let la = Zone::from_tzid("America/Los_Angeles").unwrap();
    assert_eq!(
        starts(&e, "2025-03-03 00:00", "2025-03-04 00:00", la),
        ["2025-03-03 09:00"]
    );
}

// --- edits ------------------------------------------------------------------

fn weekly() -> Item {
    // Mondays at 09:00, six of them from 3 March.
    event("DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=WEEKLY;COUNT=6")
}

fn key(text: &str) -> Key {
    wall(text)
}

#[test]
fn deleting_this_occurrence_leaves_the_rest() {
    let changes = recur::remove(&weekly(), Some(key("2025-03-10 09:00")), Scope::This);
    let item = saved(&changes, 0);
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-05-01 00:00").len(),
        5
    );
    assert!(
        !utc_starts(&item, "2025-03-01 00:00", "2025-05-01 00:00")
            .contains(&"2025-03-10 09:00".to_string())
    );
}

#[test]
fn deleting_this_and_following_ends_the_series_before_it() {
    let changes = recur::remove(&weekly(), Some(key("2025-03-17 09:00")), Scope::Following);
    let item = saved(&changes, 0);
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-06-01 00:00"),
        ["2025-03-03 09:00", "2025-03-10 09:00"]
    );
}

#[test]
fn deleting_the_first_and_following_deletes_the_event() {
    let changes = recur::remove(&weekly(), Some(key("2025-03-03 09:00")), Scope::Following);
    assert_eq!(changes, [Change::Delete("t".to_string())]);
}

#[test]
fn deleting_following_in_an_unbounded_series_uses_until() {
    let e = event(
        "DTSTART;TZID=Europe/London:20250303T090000\nDTEND;TZID=Europe/London:20250303T100000\nRRULE:FREQ=WEEKLY",
    );
    let item = saved(
        &recur::remove(&e, Some(key("2025-04-07 09:00")), Scope::Following),
        0,
    );
    let found = starts(&item, "2025-03-01 00:00", "2025-09-01 00:00", london());
    assert_eq!(found.last().map(String::as_str), Some("2025-03-31 09:00"));
    assert_eq!(found.len(), 5);
}

#[test]
fn editing_this_occurrence_changes_only_it() {
    let e = weekly();
    let mut changed = recur::event_at(&e, Some(key("2025-03-17 09:00")));
    changed.summary = "Special".into();
    let item = saved(
        &recur::edit(
            &e,
            Some(key("2025-03-17 09:00")),
            Scope::This,
            changed,
            None,
            "new",
        ),
        0,
    );
    let found = recur::occurrences(
        &item,
        wall("2025-03-01 00:00"),
        wall("2025-06-01 00:00"),
        Zone::Utc,
    );
    assert_eq!(found.len(), 6);
    assert_eq!(found.iter().filter(|o| o.summary == "Special").count(), 1);
    assert_eq!(found[2].summary, "Special");
}

#[test]
fn editing_this_occurrence_twice_replaces_the_first_edit() {
    let e = weekly();
    let k = key("2025-03-17 09:00");
    let mut first = recur::event_at(&e, Some(k));
    first.summary = "One".into();
    let once = saved(&recur::edit(&e, Some(k), Scope::This, first, None, "n"), 0);
    let mut second = recur::event_at(&once, Some(k));
    second.summary = "Two".into();
    let twice = saved(
        &recur::edit(&once, Some(k), Scope::This, second, None, "n"),
        0,
    );
    assert_eq!(twice.exceptions.len(), 1);
    assert_eq!(twice.exceptions[0].event.summary, "Two");
}

#[test]
fn editing_all_moves_the_whole_series() {
    let e = weekly();
    let mut changed = recur::event_at(&e, Some(key("2025-03-17 09:00")));
    changed.start = wall("2025-03-17 10:30");
    changed.end = wall("2025-03-17 11:30");
    let item = saved(
        &recur::edit(
            &e,
            Some(key("2025-03-17 09:00")),
            Scope::All,
            changed,
            None,
            "n",
        ),
        0,
    );
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-05-01 00:00")[..3],
        ["2025-03-03 10:30", "2025-03-10 10:30", "2025-03-17 10:30"]
    );
}

#[test]
fn editing_all_to_another_day_moves_the_weekday_of_the_rule() {
    let e = event(
        "DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=4",
    );
    let k = key("2025-03-05 09:00");
    let mut changed = recur::event_at(&e, Some(k));
    changed.start = wall("2025-03-06 09:00");
    changed.end = wall("2025-03-06 10:00");
    let item = saved(&recur::edit(&e, Some(k), Scope::All, changed, None, "n"), 0);
    // Monday and Wednesday became Tuesday and Thursday; the start moved a day.
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-03-15 00:00"),
        [
            "2025-03-04 09:00",
            "2025-03-06 09:00",
            "2025-03-11 09:00",
            "2025-03-13 09:00"
        ]
    );
}

#[test]
fn editing_following_splits_the_series() {
    let e = weekly();
    let k = key("2025-03-17 09:00");
    let mut changed = recur::event_at(&e, Some(k));
    changed.summary = "From now".into();
    changed.start = wall("2025-03-17 11:00");
    changed.end = wall("2025-03-17 12:00");
    let changes = recur::edit(&e, Some(k), Scope::Following, changed, None, "tail");
    assert_eq!(changes.len(), 2);
    let (head, tail) = (saved(&changes, 0), saved(&changes, 1));
    assert_eq!(head.uid, "t");
    assert_eq!(tail.uid, "tail");
    // Six in all: two before, four after (COUNT=6 is shared between them).
    assert_eq!(
        utc_starts(&head, "2025-03-01 00:00", "2025-06-01 00:00"),
        ["2025-03-03 09:00", "2025-03-10 09:00"]
    );
    assert_eq!(
        utc_starts(&tail, "2025-03-01 00:00", "2025-06-01 00:00"),
        [
            "2025-03-17 11:00",
            "2025-03-24 11:00",
            "2025-03-31 11:00",
            "2025-04-07 11:00"
        ]
    );
}

#[test]
fn editing_following_keeps_the_exceptions_after_the_split_with_the_tail() {
    let e = item(
        "BEGIN:VEVENT\nUID:t\nSUMMARY:T\nDTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=WEEKLY;COUNT=5\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:t\nSUMMARY:Odd\nRECURRENCE-ID:20250324T090000Z\nDTSTART:20250324T090000Z\nDTEND:20250324T100000Z\nEND:VEVENT",
    );
    let k = key("2025-03-17 09:00");
    let mut changed = recur::event_at(&e, Some(k));
    changed.summary = "Renamed".into();
    let changes = recur::edit(&e, Some(k), Scope::Following, changed, None, "tail");
    let (head, tail) = (saved(&changes, 0), saved(&changes, 1));
    assert!(head.exceptions.is_empty());
    assert_eq!(tail.exceptions.len(), 1);
    let found = recur::occurrences(
        &tail,
        wall("2025-03-01 00:00"),
        wall("2025-06-01 00:00"),
        Zone::Utc,
    );
    assert_eq!(
        found.iter().map(|o| o.summary.as_str()).collect::<Vec<_>>(),
        ["Renamed", "Odd", "Renamed"]
    );
}

#[test]
fn a_one_off_event_edit_just_replaces_it() {
    let e = event("DTSTART:20250303T090000Z\nDTEND:20250303T100000Z");
    let mut changed = e.event.clone();
    changed.summary = "New".into();
    let item = saved(
        &recur::edit(
            &e,
            None,
            Scope::All,
            changed,
            Some(Some("FREQ=DAILY;COUNT=2".into())),
            "n",
        ),
        0,
    );
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-04-01 00:00").len(),
        2
    );
}

#[test]
fn the_menu_choices_make_and_read_back_rules() {
    use crate::recur::{Repeat, RepeatEnd};
    for (repeat, end) in [
        (Repeat::Daily, RepeatEnd::Never),
        (Repeat::Weekdays, RepeatEnd::After(10)),
        (Repeat::Fortnightly, RepeatEnd::Never),
        (
            Repeat::Monthly,
            RepeatEnd::On(wall("2026-01-01 00:00").date()),
        ),
    ] {
        let rule = repeat.rule(&end).unwrap();
        assert_eq!(Repeat::from_rule(Some(&rule)), (repeat, end), "{rule}");
    }
    assert_eq!(
        recur::describe("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10"),
        "Every week on Monday and Wednesday, 10 times"
    );
    assert_eq!(
        recur::describe("FREQ=MONTHLY;INTERVAL=2;BYDAY=-1FR"),
        "Every 2 months on the last Friday"
    );
}

// --- the rest of the awkward ones -----------------------------------------------

#[test]
fn the_monday_of_week_20() {
    // RFC 5545 section 3.8.5.3.
    let e = event(
        "DTSTART:19970512T090000Z\nDTEND:19970512T100000Z\nRRULE:FREQ=YEARLY;BYWEEKNO=20;BYDAY=MO",
    );
    assert_eq!(
        utc_starts(&e, "1997-01-01 00:00", "2000-01-01 00:00"),
        ["1997-05-12 09:00", "1998-05-11 09:00", "1999-05-17 09:00"]
    );
}

#[test]
fn the_third_of_tuesday_wednesday_thursday_each_month() {
    // RFC 5545: the third instance of TU, WE, TH in the month.
    let e = event(
        "DTSTART:19970904T090000Z\nDTEND:19970904T100000Z\nRRULE:FREQ=MONTHLY;COUNT=3;BYDAY=TU,WE,TH;BYSETPOS=3",
    );
    assert_eq!(
        utc_starts(&e, "1997-09-01 00:00", "1998-01-01 00:00"),
        ["1997-09-04 09:00", "1997-10-07 09:00", "1997-11-06 09:00"]
    );
}

#[test]
fn every_three_hours() {
    let e = event(
        "DTSTART:20250303T090000Z\nDTEND:20250303T093000Z\nRRULE:FREQ=HOURLY;INTERVAL=3;COUNT=4",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-03 00:00", "2025-03-04 00:00"),
        [
            "2025-03-03 09:00",
            "2025-03-03 12:00",
            "2025-03-03 15:00",
            "2025-03-03 18:00"
        ]
    );
}

#[test]
fn an_until_before_the_start_leaves_the_start() {
    let e = event(
        "DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=DAILY;UNTIL=20250301T000000Z",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-04-01 00:00"),
        ["2025-03-03 09:00"]
    );
}

#[test]
fn a_daily_event_in_the_new_york_gap_is_shifted_not_dropped() {
    // The clocks in New York go from 02:00 to 03:00 on 2025-03-09.
    let e = event(
        "DTSTART;TZID=America/New_York:20250308T023000\nDTEND;TZID=America/New_York:20250308T033000\nRRULE:FREQ=DAILY;COUNT=3",
    );
    let ny = Zone::from_tzid("America/New_York").unwrap();
    assert_eq!(
        starts(&e, "2025-03-08 00:00", "2025-03-11 00:00", ny),
        ["2025-03-08 02:30", "2025-03-09 03:30", "2025-03-10 02:30"]
    );
}

#[test]
fn a_window_that_ends_when_an_event_starts_does_not_include_it() {
    let e = event("DTSTART:20250303T090000Z\nDTEND:20250303T100000Z");
    assert!(utc_starts(&e, "2025-03-03 08:00", "2025-03-03 09:00").is_empty());
    assert_eq!(
        utc_starts(&e, "2025-03-03 09:59", "2025-03-03 11:00").len(),
        1
    );
    assert!(utc_starts(&e, "2025-03-03 10:00", "2025-03-03 11:00").is_empty());
}

#[test]
fn an_exdate_with_only_a_date_removes_that_days_timed_occurrence() {
    let e = event(
        "DTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=DAILY;COUNT=3\nEXDATE;VALUE=DATE:20250304",
    );
    assert_eq!(
        utc_starts(&e, "2025-03-01 00:00", "2025-03-31 00:00"),
        ["2025-03-03 09:00", "2025-03-05 09:00"]
    );
}

#[test]
fn a_busy_minutely_rule_is_cut_short_not_endless() {
    let e = event("DTSTART:20250303T090000Z\nDTEND:20250303T090100Z\nRRULE:FREQ=MINUTELY");
    let found = utc_starts(&e, "2025-03-03 00:00", "2025-03-31 00:00");
    assert!(!found.is_empty() && found.len() <= 6000, "{}", found.len());
}

#[test]
fn editing_all_without_moving_keeps_the_changed_occurrences() {
    let e = item(
        "BEGIN:VEVENT\nUID:t\nSUMMARY:T\nDTSTART:20250303T090000Z\nDTEND:20250303T100000Z\nRRULE:FREQ=WEEKLY;COUNT=3\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:t\nSUMMARY:Odd\nRECURRENCE-ID:20250310T090000Z\nDTSTART:20250310T090000Z\nDTEND:20250310T100000Z\nEND:VEVENT",
    );
    let k = key("2025-03-17 09:00");
    let mut changed = recur::event_at(&e, Some(k));
    changed.summary = "Renamed".into();
    let item = saved(&recur::edit(&e, Some(k), Scope::All, changed, None, "n"), 0);
    let found = recur::occurrences(
        &item,
        wall("2025-03-01 00:00"),
        wall("2025-04-01 00:00"),
        Zone::Utc,
    );
    assert_eq!(
        found.iter().map(|o| o.summary.as_str()).collect::<Vec<_>>(),
        ["Renamed", "Odd", "Renamed"]
    );
}

#[test]
fn editing_following_on_the_last_occurrence_of_a_counted_series() {
    let e = weekly();
    let k = key("2025-04-07 09:00");
    let mut changed = recur::event_at(&e, Some(k));
    changed.summary = "Last".into();
    let changes = recur::edit(&e, Some(k), Scope::Following, changed, None, "tail");
    let (head, tail) = (saved(&changes, 0), saved(&changes, 1));
    assert_eq!(
        utc_starts(&head, "2025-03-01 00:00", "2025-06-01 00:00").len(),
        5
    );
    assert_eq!(
        utc_starts(&tail, "2025-03-01 00:00", "2025-06-01 00:00"),
        ["2025-04-07 09:00"]
    );
}

#[test]
fn an_edit_that_stops_a_series_from_repeating() {
    let e = weekly();
    let mut changed = e.event.clone();
    changed.summary = "Once".into();
    let item = saved(
        &recur::edit(
            &e,
            Some(key("2025-03-03 09:00")),
            Scope::All,
            changed,
            Some(None),
            "n",
        ),
        0,
    );
    assert_eq!(
        utc_starts(&item, "2025-03-01 00:00", "2025-06-01 00:00"),
        ["2025-03-03 09:00"]
    );
}

#[test]
fn a_two_day_timed_event_is_a_banner_and_an_overnight_one_is_not() {
    let two_days = event("DTSTART:20250303T090000Z\nDTEND:20250305T090000Z");
    let night = event("DTSTART:20250303T220000Z\nDTEND:20250304T060000Z");
    let a = &recur::occurrences(
        &two_days,
        wall("2025-03-01 00:00"),
        wall("2025-03-09 00:00"),
        Zone::Utc,
    )[0];
    let b = &recur::occurrences(
        &night,
        wall("2025-03-01 00:00"),
        wall("2025-03-09 00:00"),
        Zone::Utc,
    )[0];
    assert!(a.is_banner());
    assert!(!b.is_banner());
}
