//! The event editor: a popup (a sheet on a phone) for making an event or
//! changing one, and the question a repeating event asks before a change
//! or a removal: this one, this and the following, or all of them.

use std::sync::Arc;

use bingux_calendar_core::chrono::{Duration, NaiveDateTime, Utc};
use bingux_calendar_core::recur::{self, Change, Repeat, RepeatEnd, Scope};
use bingux_calendar_core::zone::common_zones;
use bingux_calendar_core::{Availability, Event, Item, Key, Reminder, Series, Zone, ics};
use bingux_ui::calendar::{Date, date_picker, format_clock, time_picker};
use bingux_ui::prelude::*;
use bingux_ui::window::{PopupFlags, WindowLike};
use freya::prelude::*;

use crate::convert;
use crate::model::{EventRef, Model};

/// What the editor opens on.
#[derive(Clone, Debug, PartialEq)]
pub enum Seed {
    /// An event that is in the store.
    Existing(EventRef),
    /// One that is not yet, with what is known of it. Times are on the clock
    /// the views show.
    New(NewEvent),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewEvent {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub all_day: bool,
    pub title: String,
    pub location: String,
    pub rule: Option<String>,
    pub reminder: Option<i64>,
}

impl Seed {
    /// A new event from `start` to `end`.
    pub fn new_at(start: NaiveDateTime, end: NaiveDateTime) -> Seed {
        Seed::New(NewEvent {
            start,
            end,
            all_day: false,
            title: String::new(),
            location: String::new(),
            rule: None,
            reminder: None,
        })
    }

    /// A new event on `day`: at nine, or at the next hour if it is today.
    pub fn at_day(model: &Model, day: Date) -> Seed {
        let clock = *model.clock.read();
        let length = model.prefs.read().default_length.max(15) as u32;
        let start = if day == clock.today {
            ((clock.minutes / 60 + 1) * 60).min(23 * 60)
        } else {
            9 * 60
        };
        Seed::new_at(
            convert::at_minutes(day, start),
            convert::at_minutes(day, start + length),
        )
    }
}

/// Opens the editor.
pub fn open(model: Model, window: Window, overlays: Overlays, seed: Seed) {
    let title = match &seed {
        Seed::Existing(_) => "Event",
        Seed::New(_) => "New event",
    };
    window.open(
        popup(
            title,
            Editor {
                model,
                seed,
                overlays,
            },
        )
        .width(Dim::PopupNarrow)
        .flags(PopupFlags::FORM | PopupFlags::NO_DISMISS_OUTSIDE),
    );
}

/// Asks which of a repeating event's occurrences a change is for, then calls
/// `then` with the answer.
pub fn ask_scope(
    overlays: Overlays,
    title: &str,
    message: &str,
    verb: &str,
    then: impl Fn(Scope) + Clone + 'static,
) {
    let destructive = verb == "Delete";
    let kind = if destructive {
        ResponseKind::Destructive
    } else {
        ResponseKind::Default
    };
    let (first, second, third) = (then.clone(), then.clone(), then);
    overlays.show_dialog(
        dialog(title, message)
            .cancel("Cancel")
            .response(format!("{verb} this event only"), kind, move |_| {
                first(Scope::This)
            })
            .response(
                format!("{verb} this and following events"),
                kind,
                move |_| second(Scope::Following),
            )
            .response(format!("{verb} all events"), kind, move |_| {
                third(Scope::All)
            })
            .default_action(CANCEL_ID),
    );
}

/// The words for a reminder of `minutes` before the start.
fn reminder_label(minutes: i64) -> String {
    match minutes {
        0 => "At the time of the event".to_string(),
        m if m % 10_080 == 0 => {
            let n = m / 10_080;
            if n == 1 {
                "1 week before".to_string()
            } else {
                format!("{n} weeks before")
            }
        }
        m if m % 1_440 == 0 => {
            let n = m / 1_440;
            if n == 1 {
                "1 day before".to_string()
            } else {
                format!("{n} days before")
            }
        }
        m if m % 60 == 0 => {
            let n = m / 60;
            if n == 1 {
                "1 hour before".to_string()
            } else {
                format!("{n} hours before")
            }
        }
        m => {
            if m == 1 {
                "1 minute before".to_string()
            } else {
                format!("{m} minutes before")
            }
        }
    }
}

/// How the editor writes a repeat choice.
fn repeat_key(repeat: &Repeat) -> &'static str {
    match repeat {
        Repeat::Never => "never",
        Repeat::Daily => "daily",
        Repeat::Weekdays => "weekdays",
        Repeat::Weekly => "weekly",
        Repeat::Fortnightly => "fortnightly",
        Repeat::Monthly => "monthly",
        Repeat::Yearly => "yearly",
        Repeat::Custom(_) => "custom",
    }
}

fn repeat_from(key: &str, custom: &Option<String>) -> Repeat {
    match key {
        "daily" => Repeat::Daily,
        "weekdays" => Repeat::Weekdays,
        "weekly" => Repeat::Weekly,
        "fortnightly" => Repeat::Fortnightly,
        "monthly" => Repeat::Monthly,
        "yearly" => Repeat::Yearly,
        "custom" => custom.clone().map_or(Repeat::Never, Repeat::Custom),
        _ => Repeat::Never,
    }
}

/// The values the editor's fields hold when it opens.
#[derive(Clone, PartialEq)]
struct Draft {
    title: String,
    location: String,
    notes: String,
    url: String,
    all_day: bool,
    start_date: Date,
    start_minutes: u32,
    end_date: Date,
    end_minutes: u32,
    zone: Zone,
    repeat: Repeat,
    repeat_end: RepeatEnd,
    custom_rule: Option<String>,
    calendar: String,
    reminder: Option<i64>,
    busy: bool,
}

impl Draft {
    fn from_seed(model: &Model, seed: &Seed) -> (Draft, Option<Arc<Item>>, Option<Key>) {
        let calendar = model.default_calendar().unwrap_or_default();
        let default_reminder = model.prefs.read().default_reminder;
        let view = model.zone();
        match seed {
            Seed::New(new) => {
                let zone = if view == Zone::Floating {
                    Zone::Utc
                } else {
                    view
                };
                let (start, end) = (new.start, new.end);
                let (repeat, repeat_end) = Repeat::from_rule(new.rule.as_deref());
                let custom_rule = match &repeat {
                    Repeat::Custom(rule) => Some(rule.clone()),
                    _ => None,
                };
                let draft = Draft {
                    title: new.title.clone(),
                    location: new.location.clone(),
                    notes: String::new(),
                    url: String::new(),
                    all_day: new.all_day,
                    start_date: convert::date_of(start.date()),
                    start_minutes: convert::minutes_of(start),
                    end_date: convert::date_of(if new.all_day {
                        (end - Duration::days(1)).date()
                    } else {
                        end.date()
                    }),
                    end_minutes: convert::minutes_of(end),
                    zone,
                    repeat,
                    repeat_end,
                    custom_rule,
                    calendar,
                    reminder: new.reminder.or(default_reminder),
                    busy: true,
                };
                (draft, None, None)
            }
            Seed::Existing(reference) => {
                let Some(item) = model.item(&reference.uid) else {
                    return (Draft::empty(model), None, None);
                };
                let event = recur::event_at(&item, reference.key);
                let (repeat, repeat_end) =
                    Repeat::from_rule(item.series.as_ref().map(|series| series.rule.as_str()));
                let custom_rule = match &repeat {
                    Repeat::Custom(rule) => Some(rule.clone()),
                    _ => None,
                };
                let last_day = if event.all_day {
                    event.last_day()
                } else {
                    event.end.date()
                };
                let draft = Draft {
                    title: event.summary.clone(),
                    location: event.location.clone(),
                    notes: event.description.clone(),
                    url: event.url.clone(),
                    all_day: event.all_day,
                    start_date: convert::date_of(event.start.date()),
                    start_minutes: convert::minutes_of(event.start),
                    end_date: convert::date_of(last_day),
                    end_minutes: convert::minutes_of(event.end),
                    zone: event.zone,
                    repeat,
                    repeat_end,
                    custom_rule,
                    calendar: item.calendar.clone(),
                    reminder: event.reminders.iter().find_map(Reminder::minutes_before),
                    busy: event.availability == Availability::Busy,
                };
                (draft, Some(item), reference.key)
            }
        }
    }

    fn empty(model: &Model) -> Draft {
        let clock = *model.clock.read();
        Draft {
            title: String::new(),
            location: String::new(),
            notes: String::new(),
            url: String::new(),
            all_day: false,
            start_date: clock.today,
            start_minutes: 9 * 60,
            end_date: clock.today,
            end_minutes: 10 * 60,
            zone: clock.zone,
            repeat: Repeat::Never,
            repeat_end: RepeatEnd::Never,
            custom_rule: None,
            calendar: model.default_calendar().unwrap_or_default(),
            reminder: None,
            busy: true,
        }
    }
}

/// The editor's body.
#[derive(PartialEq)]
struct Editor {
    model: Model,
    seed: Seed,
    overlays: Overlays,
}

impl Component for Editor {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let model = self.model;
        let overlays = self.overlays;
        let clock = *model.clock.read();
        let twelve = model.twelve_hour();
        let first_weekday = model.first_weekday();
        let (draft, item, key) = use_hook({
            let seed = self.seed.clone();
            move || Draft::from_seed(&model, &seed)
        });

        let title = use_state(|| draft.title.clone());
        let location = use_state(|| draft.location.clone());
        let notes = use_state(|| draft.notes.clone());
        let url = use_state(|| draft.url.clone());
        let mut all_day = use_state(|| draft.all_day);
        let mut start_date = use_state(|| draft.start_date);
        let mut start_minutes = use_state(|| draft.start_minutes);
        let mut end_date = use_state(|| draft.end_date);
        let mut end_minutes = use_state(|| draft.end_minutes);
        let mut zone = use_state(|| draft.zone);
        let mut repeat = use_state(|| repeat_key(&draft.repeat).to_string());
        let mut repeat_end = use_state(|| draft.repeat_end.clone());
        let mut calendar = use_state(|| draft.calendar.clone());
        let mut reminder = use_state(|| draft.reminder);
        let mut busy = use_state(|| draft.busy);

        let custom_rule = draft.custom_rule.clone();
        let existing = item.is_some();
        let data = model.data();

        // The end moves with the start, so the length stays.
        let mut move_start = move |date: Date, minutes: u32| {
            let old = convert::at_minutes(*start_date.peek(), *start_minutes.peek());
            let old_end = convert::at_minutes(*end_date.peek(), *end_minutes.peek());
            let length = old_end - old;
            let next = convert::at_minutes(date, minutes);
            let next_end = next + length;
            start_date.set(date);
            start_minutes.set(minutes);
            if *all_day.peek() {
                let shift = date.days() - convert::date_of(old.date()).days();
                let moved = end_date.peek().add_days(shift);
                end_date.set(moved);
            } else {
                end_date.set(convert::date_of(next_end.date()));
                end_minutes.set(convert::minutes_of(next_end));
            }
        };

        // Is it a valid event?
        let all = *all_day.read();
        let (sd, sm, ed, em) = (
            *start_date.read(),
            *start_minutes.read(),
            *end_date.read(),
            *end_minutes.read(),
        );
        let ends_after = if all {
            ed >= sd
        } else {
            convert::at_minutes(ed, em) >= convert::at_minutes(sd, sm)
        };
        let has_calendar = !calendar.read().is_empty();
        let valid = ends_after && has_calendar;

        // Saving.
        let save = {
            let item = item.clone();
            move || {
                let all = *all_day.peek();
                let zone = if all { Zone::Floating } else { *zone.peek() };
                let (sd, sm) = (*start_date.peek(), *start_minutes.peek());
                let (ed, em) = (*end_date.peek(), *end_minutes.peek());
                let mut event = match &item {
                    Some(item) => recur::event_at(item, key),
                    None => Event::new("", convert::midnight(sd), zone),
                };
                event.summary = title.peek().trim().to_string();
                event.location = location.peek().trim().to_string();
                event.description = notes.peek().trim().to_string();
                event.url = url.peek().trim().to_string();
                event.all_day = all;
                event.zone = zone;
                if all {
                    event.start = convert::midnight(sd);
                    event.end = convert::midnight(ed.max(sd).add_days(1));
                } else {
                    event.start = convert::at_minutes(sd, sm);
                    event.end = convert::at_minutes(ed, em);
                }
                event.availability = if *busy.peek() {
                    Availability::Busy
                } else {
                    Availability::Free
                };
                let rest: Vec<Reminder> = event.reminders.iter().skip(1).copied().collect();
                event.reminders = match *reminder.peek() {
                    Some(minutes) => std::iter::once(Reminder::before(minutes))
                        .chain(rest)
                        .collect(),
                    None => Vec::new(),
                };
                let chosen = repeat.peek().clone();
                let rule = repeat_from(&chosen, &custom_rule).rule(&repeat_end.peek().clone());
                let calendar = calendar.peek().clone();
                match &item {
                    None => {
                        let now = Utc::now();
                        let mut fresh = Item::new(ics::new_uid(), calendar, event);
                        fresh.created = Some(now);
                        fresh.modified = Some(now);
                        fresh.series = rule.map(|rule| Series {
                            rule,
                            ..Series::default()
                        });
                        model.apply(vec![Change::Save(fresh)]);
                        window.close();
                    }
                    Some(item) => {
                        let was = item.series.as_ref().map(|series| series.rule.clone());
                        let rule_change = if was == rule { None } else { Some(rule) };
                        let target = item.clone();
                        let finish = move |scope: Scope| {
                            let mut changes = recur::edit(
                                &target,
                                key,
                                scope,
                                event.clone(),
                                rule_change.clone(),
                                &ics::new_uid(),
                            );
                            for change in &mut changes {
                                if let Change::Save(saved) = change {
                                    saved.calendar = calendar.clone();
                                }
                            }
                            model.apply(changes);
                            window.close();
                        };
                        if item.series.is_none() {
                            finish(Scope::All);
                        } else {
                            ask_scope(
                                overlays,
                                "Save changes?",
                                "This event repeats.",
                                "Save",
                                finish,
                            );
                        }
                    }
                }
            }
        };
        let save_press = save.clone();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Save".into(),
                enabled: valid,
                on_press: EventHandler::new(move |_: ()| save_press()),
            }),
        });
        let on_enter = save.clone();

        // The fields.
        let heading = Group::new(None::<String>)
            .row(
                form_field("Title", title)
                    .placeholder("New event")
                    .autofocus()
                    .on_submit(move |_: ()| {
                        if valid {
                            on_enter();
                        }
                    }),
            )
            .row(
                form_field("Location", location)
                    .placeholder("Add a place")
                    .optional(),
            );

        let start_row = {
            let picker = date_picker(sd, clock.today, move |date: Date| {
                move_start(date, *start_minutes.peek());
            })
            .first_weekday(first_weekday);
            let mut line = hstack(S2).child(picker);
            if !all {
                line = line.child(time_picker(sm, twelve, move |minutes: u32| {
                    move_start(*start_date.peek(), minutes);
                }));
            }
            row("Starts").trailing(line)
        };
        let end_row = {
            let picker = date_picker(ed, clock.today, move |date: Date| end_date.set(date))
                .first_weekday(first_weekday);
            let mut line = hstack(S2).child(picker);
            if !all {
                let after = (sd == ed).then_some(sm);
                let mut picker =
                    time_picker(em, twelve, move |minutes: u32| end_minutes.set(minutes));
                if let Some(start) = after {
                    picker = picker.after(start);
                }
                line = line.child(picker);
            }
            row("Ends").trailing(line).subtitle(if ends_after {
                ""
            } else {
                "The event ends before it starts."
            })
        };
        let mut when = Group::new(None::<String>)
            .row(switch_row("All-day", all, move |on: bool| all_day.set(on)))
            .row(start_row)
            .row(end_row);
        if !all {
            let mut options: Vec<(String, String)> = common_zones()
                .iter()
                .map(|z| (z.tzid(), z.label()))
                .collect();
            let current = *zone.read();
            if !options.iter().any(|(key, _)| *key == current.tzid()) {
                options.insert(0, (current.tzid(), current.label()));
            }
            let note = if current != clock.zone && current != Zone::Floating {
                let start = convert::at_minutes(sd, sm);
                let there = current.convert(start, clock.zone);
                format!(
                    "{} in your time zone.",
                    if there.date() == start.date() {
                        format_clock(convert::minutes_of(there), twelve)
                    } else {
                        format!(
                            "{} on {}",
                            format_clock(convert::minutes_of(there), twelve),
                            there.format("%-d %B")
                        )
                    }
                )
            } else {
                String::new()
            };
            when = when.row(row("Time zone").subtitle(note).trailing(dropdown(
                options,
                current.tzid(),
                move |key: String| {
                    if let Some(next) = Zone::from_tzid(&key) {
                        zone.set(next);
                    }
                },
            )));
        }

        let repeat_options: Vec<(String, String)> = {
            let mut options = vec![
                ("never".to_string(), "Never".to_string()),
                ("daily".to_string(), "Every day".to_string()),
                ("weekdays".to_string(), "Every weekday".to_string()),
                ("weekly".to_string(), "Every week".to_string()),
                ("fortnightly".to_string(), "Every 2 weeks".to_string()),
                ("monthly".to_string(), "Every month".to_string()),
                ("yearly".to_string(), "Every year".to_string()),
            ];
            if let Some(rule) = &draft.custom_rule {
                options.push(("custom".to_string(), recur::describe(rule)));
            }
            options
        };
        let repeating = *repeat.read() != "never";
        let mut repeats = Group::new(None::<String>).row(row("Repeat").trailing(dropdown(
            repeat_options,
            repeat.read().clone(),
            move |key: String| repeat.set(key),
        )));
        if repeating && *repeat.read() != "custom" {
            let end_key = match &*repeat_end.read() {
                RepeatEnd::Never => "never",
                RepeatEnd::On(_) => "on",
                RepeatEnd::After(_) => "after",
            };
            let end_options = vec![
                ("never".to_string(), "Never".to_string()),
                ("on".to_string(), "On a date".to_string()),
                ("after".to_string(), "After a number of times".to_string()),
            ];
            let mut line = hstack(S2);
            match repeat_end.read().clone() {
                RepeatEnd::On(date) => {
                    line = line.child(
                        date_picker(convert::date_of(date), clock.today, move |day: Date| {
                            repeat_end.set(RepeatEnd::On(convert::naive(day)));
                        })
                        .first_weekday(first_weekday),
                    );
                }
                RepeatEnd::After(count) => {
                    line = line.child(
                        stepper(i64::from(count), move |value: i64| {
                            repeat_end.set(RepeatEnd::After(value.max(1) as u32));
                        })
                        .range(1, 999)
                        .suffix(" times"),
                    );
                }
                RepeatEnd::Never => {}
            }
            let line = line.child(dropdown(end_options, end_key, move |key: String| {
                repeat_end.set(match key.as_str() {
                    "on" => RepeatEnd::On(convert::naive(ed.add_days(30))),
                    "after" => RepeatEnd::After(10),
                    _ => RepeatEnd::Never,
                });
            }));
            repeats = repeats.row(row("Ends repeating").trailing(line));
        }
        if let Some(rule) = draft
            .custom_rule
            .as_ref()
            .filter(|_| *repeat.read() == "custom")
        {
            repeats = repeats.footer(recur::describe(rule));
        }

        let calendars: Vec<_> = data.calendars.iter().filter(|c| !c.read_only).collect();
        let calendar_row = {
            let options: Vec<(String, String)> = calendars
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect();
            let colours = data
                .calendars
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.read_only)
                .map(|(index, c)| convert::colour_of(Some(c), index, &tokens))
                .collect::<Vec<_>>();
            row("Calendar").trailing(
                dropdown(options, calendar.read().clone(), move |id: String| {
                    calendar.set(id)
                })
                .colours(colours),
            )
        };
        let reminder_row = {
            let mut minutes: Vec<i64> = vec![0, 5, 10, 15, 30, 60, 120, 1440, 2880, 10_080];
            if let Some(current) = *reminder.read() {
                if !minutes.contains(&current) {
                    minutes.push(current);
                    minutes.sort_unstable();
                }
            }
            let mut options = vec![("none".to_string(), "None".to_string())];
            options.extend(minutes.iter().map(|m| (m.to_string(), reminder_label(*m))));
            let selected = reminder
                .read()
                .map_or("none".to_string(), |m| m.to_string());
            row("Reminder").trailing(dropdown(options, selected, move |key: String| {
                reminder.set(key.parse::<i64>().ok());
            }))
        };
        let details = Group::new(None::<String>)
            .row(calendar_row)
            .row(reminder_row)
            .row(choice_row(
                "Show as",
                vec![
                    ("busy".to_string(), "Busy".to_string()),
                    ("free".to_string(), "Free".to_string()),
                ],
                if *busy.read() { "busy" } else { "free" },
                move |key: String| busy.set(key == "busy"),
            ));
        let more = Group::new(None::<String>)
            .row(form_field("Notes", notes).multiline(4).optional())
            .row(form_field("URL", url).optional());

        let mut column = popup_column()
            .child(heading)
            .child(when)
            .child(repeats)
            .child(details)
            .child(more);
        if let Some(existing_item) = item.clone().filter(|_| existing) {
            column = column.child(Group::new(None::<String>).row(
                row("Delete event").destructive().on_press(move |_| {
                    delete_event(model, overlays, existing_item.clone(), key, Some(window));
                }),
            ));
        }
        column
    }
}

/// Deletes an event, asking which of a repeating series first, and offers to
/// bring it back. `editor` is the window of an editor to close once it is done.
pub fn delete_event(
    model: Model,
    overlays: Overlays,
    item: Arc<Item>,
    key: Option<Key>,
    editor: Option<Window>,
) {
    let delete = {
        let item = item.clone();
        move |scope: Scope| {
            let undo = model.remove(&item, key, scope);
            if let Some(window) = editor {
                window.close();
            }
            overlays.toast_with_action("Event deleted", "Undo", move |_| {
                model.apply(undo.clone());
            });
        }
    };
    if item.series.is_some() {
        ask_scope(
            overlays,
            "Delete this event?",
            "This event repeats.",
            "Delete",
            delete,
        );
    } else {
        delete(Scope::All);
    }
}
