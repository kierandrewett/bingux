//! Quick add: type "Lunch with Sam at Nando's tomorrow 1pm" and it becomes an
//! event. The text is read on this computer, and what was understood is shown
//! as it is typed.

use bingux_calendar_core::chrono::{Duration, Utc};
use bingux_calendar_core::quickadd::{self, Quick};
use bingux_calendar_core::recur::Change;
use bingux_calendar_core::{Availability, Event, Item, Reminder, Series, Zone, ics};
use bingux_ui::calendar::format_clock;
use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, CAPTION, Ink};
use bingux_ui::window::{PopupFlags, WindowLike};
use freya::prelude::*;

use crate::convert;
use crate::editor::{self, NewEvent, Seed};
use crate::model::Model;

/// Opens the quick add popup.
pub fn open(model: Model, window: Window, overlays: Overlays) {
    window.open(
        popup("New event", QuickBody { model, overlays })
            .width(Dim::PopupNarrow)
            .flags(PopupFlags::FORM),
    );
}

/// What the text becomes: the times on the clock the views show.
pub fn read(model: &Model, text: &str) -> (Quick, NewEvent) {
    let clock = *model.clock.read();
    let now = convert::at_minutes(clock.today, clock.minutes);
    let month_first = bingux_calendar_core::Locale::current().month_first;
    let quick = quickadd::parse(text, now, month_first);
    let length = model.prefs.read().default_length.max(15);
    let anchor = *model.anchor.read();
    let (start, end, all_day) = match quick.start {
        Some(start) if quick.all_day => {
            (start, quick.end.unwrap_or(start + Duration::days(1)), true)
        }
        Some(start) => (
            start,
            quick.end.unwrap_or(start + Duration::minutes(length)),
            false,
        ),
        None => {
            let seed = Seed::at_day(model, anchor);
            match seed {
                Seed::New(new) => (new.start, new.end, false),
                Seed::Existing(_) => (now, now + Duration::hours(1), false),
            }
        }
    };
    let new = NewEvent {
        start,
        end,
        all_day,
        title: quick.summary.clone(),
        location: quick.location.clone(),
        rule: quick.rule.clone(),
        reminder: quick.reminder,
    };
    (quick, new)
}

/// The event a seed describes, ready to save.
pub fn item_of(model: &Model, new: &NewEvent) -> Option<Item> {
    let calendar = model.default_calendar()?;
    let zone = if new.all_day {
        Zone::Floating
    } else {
        model.zone()
    };
    let (start, end) = (new.start, new.end);
    let mut event = if new.all_day {
        let mut event = Event::all_day(
            new.title.clone(),
            start.date(),
            (end - Duration::days(1)).date().max(start.date()),
        );
        event.zone = Zone::Floating;
        event
    } else {
        let mut event = Event::new(new.title.clone(), start, zone);
        event.end = end;
        event
    };
    event.location = new.location.clone();
    event.availability = Availability::Busy;
    let reminder = new.reminder.or(model.prefs.read().default_reminder);
    if let Some(minutes) = reminder {
        event.reminders.push(Reminder::before(minutes));
    }
    let now = Utc::now();
    let mut item = Item::new(ics::new_uid(), calendar, event);
    item.created = Some(now);
    item.modified = Some(now);
    item.series = new.rule.clone().map(|rule| Series {
        rule,
        ..Series::default()
    });
    Some(item)
}

#[derive(PartialEq)]
struct QuickBody {
    model: Model,
    overlays: Overlays,
}

impl Component for QuickBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let model = self.model;
        let overlays = self.overlays;
        let text = use_state(String::new);
        let typed = text.read().clone();
        let (quick, new) = read(&model, &typed);
        let ready = !quick.summary.trim().is_empty();
        let twelve = model.twelve_hour();

        let add = {
            let new = new.clone();
            move || {
                let Some(item) = item_of(&model, &new) else {
                    overlays.toast("There is no calendar to add the event to.");
                    return;
                };
                let day = convert::date_of(new.start.date());
                model.apply(vec![Change::Save(item)]);
                model.go(day);
                window.close();
            }
        };
        let press = add.clone();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Add".into(),
                enabled: ready,
                on_press: EventHandler::new(move |_: ()| press()),
            }),
        });

        // What was understood, a line at a time.
        let mut lines: Vec<String> = Vec::new();
        if ready {
            lines.push(quick.summary.clone());
            let when = if new.all_day {
                format!("{}, all day", convert::date_of(new.start.date()).short())
            } else {
                format!(
                    "{}, {} to {}",
                    convert::date_of(new.start.date()).short(),
                    format_clock(convert::minutes_of(new.start), twelve),
                    format_clock(convert::minutes_of(new.end), twelve)
                )
            };
            lines.push(when);
            if !new.location.is_empty() {
                lines.push(new.location.clone());
            }
            if let Some(rule) = &new.rule {
                lines.push(bingux_calendar_core::recur::describe(rule));
            }
        }
        let preview: Element = if lines.is_empty() {
            CAPTION
                .label(&tokens, "Say what, when and where. For example: Lunch with Sam at Nando\u{2019}s tomorrow 1pm.")
                .into_element()
        } else {
            let mut column = vstack(S1);
            for (index, line) in lines.iter().enumerate() {
                column = column.child(if index == 0 {
                    BODY.in_ink(Ink::Text)
                        .label(&tokens, line.clone())
                        .max_lines(1)
                        .into_element()
                } else {
                    CAPTION
                        .label(&tokens, line.clone())
                        .max_lines(1)
                        .into_element()
                });
            }
            column.into_element()
        };

        let more = {
            let new = new.clone();
            move |_: ()| {
                window.close();
                editor::open(model, window, overlays, Seed::New(new.clone()));
            }
        };
        let enter = add.clone();
        popup_column()
            .child(
                Group::new(None::<String>).row(
                    text_field(text, move |_: String| {
                        if ready {
                            enter();
                        }
                    })
                    .placeholder("Lunch with Sam tomorrow 1pm")
                    .autofocus()
                    .fill_width(),
                ),
            )
            .child(preview)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), justify(Align::End)])
                    .child(action_button("More Options", more).flat().enabled(true)),
            )
    }
}
