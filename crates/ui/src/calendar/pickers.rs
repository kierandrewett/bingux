//! Choosing a date and choosing a time of day.
//!
//! [`date_picker`] is a button that shows the date and opens a month under
//! it (a bottom sheet on a phone). Moving over days with the arrow keys does
//! not choose; a click or Enter does, and closes it. [`time_picker`] is a
//! dropdown of the times of the day every quarter hour, which already has the
//! keyboard, type-ahead and the phone sheet a dropdown has; a time that is
//! not on a quarter is listed too while it is the value.

use freya::prelude::*;

use super::date::{Date, Weekday, format_clock};
use super::month_grid::month_grid;
use crate::component::ComponentName;
use crate::controls::{action_button, dropdown};
use crate::icons::Icon;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;

/// A button that chooses a date.
#[derive(PartialEq)]
pub struct DatePicker {
    value: Date,
    today: Date,
    first_weekday: Weekday,
    enabled: bool,
    on_change: EventHandler<Date>,
}

/// A picker for `value`. `today` marks the day in the month it opens.
pub fn date_picker(
    value: Date,
    today: Date,
    on_change: impl Into<EventHandler<Date>>,
) -> DatePicker {
    DatePicker {
        value,
        today,
        first_weekday: Weekday::Mon,
        enabled: true,
        on_change: on_change.into(),
    }
}

impl DatePicker {
    pub fn first_weekday(mut self, day: Weekday) -> Self {
        self.first_weekday = day;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for DatePicker {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::surface::use_tokens();
        let mut open = use_state(|| false);
        let mut shown = use_state(|| self.value.first_of_month());
        let mut draft = use_state(|| self.value);
        let mut area = use_state(Area::default);
        let value = self.value;
        let on_change = self.on_change.clone();

        let button = action_button(value.short() + &format!(" {}", value.year), move |_| {
            let next = !*open.peek();
            if next {
                shown.set(value.first_of_month());
                draft.set(value);
            }
            open.set(next);
        })
        .icon(Icon::XOfficeCalendar)
        .enabled(self.enabled);

        let grid = month_grid(*shown.read(), self.today, move |date: Date| {
            draft.set(date);
            shown.set_if_modified(date.first_of_month());
        })
        .selected(*draft.read())
        .first_weekday(self.first_weekday)
        .on_choose(move |date: Date| {
            on_change.call(date);
            open.set(false);
        })
        .on_page(move |delta: i32| {
            let next = shown.peek().add_months(delta);
            shown.set(next);
        });

        // A header, a weekday row and six rows of days, with the gaps between.
        let height = Dim::Control.px() * 7.0
            + Dim::CalendarWeekNumber.px()
            + S1.px() * 2.0
            + S0_5.px() * 6.0;
        let width = Dim::Menu.px() + S4.px();
        let panel = Panel::new(
            PanelKind::Choice,
            open() && self.enabled,
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            vec![Element::from(
                rect().tw(&tokens, &[w_full(), p(S1)]).child(grid),
            )],
        )
        .size(width + S2.px() * 2.0, height + S2.px() * 2.0)
        .on_dismiss(EventHandler::new(move |_: ()| open.set(false)));

        rect()
            .component("DatePicker")
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(button)
            .child(Element::from(panel))
    }
}

/// A dropdown that chooses a time of day.
#[derive(PartialEq)]
pub struct TimePicker {
    minutes: u32,
    twelve_hour: bool,
    from: Option<u32>,
    enabled: bool,
    on_change: EventHandler<u32>,
}

/// A picker for `minutes` after midnight.
pub fn time_picker(
    minutes: u32,
    twelve_hour: bool,
    on_change: impl Into<EventHandler<u32>>,
) -> TimePicker {
    TimePicker {
        minutes,
        twelve_hour,
        from: None,
        enabled: true,
        on_change: on_change.into(),
    }
}

impl TimePicker {
    /// Shows each time with how long after `start` it is ("10:30 AM, 1.5 h"),
    /// as the end of an event does. Times before `start` are listed without.
    pub fn after(mut self, start: u32) -> Self {
        self.from = Some(start);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// "30 min", "1 h", "1.5 h".
pub fn format_length(minutes: u32) -> String {
    match minutes {
        0..=59 => format!("{minutes} min"),
        _ if minutes % 60 == 0 => format!("{} h", minutes / 60),
        _ => format!("{:.1} h", minutes as f32 / 60.0),
    }
}

impl Component for TimePicker {
    fn render(&self) -> impl IntoElement {
        let mut times: Vec<u32> = (0..96).map(|slot| slot * 15).collect();
        if !times.contains(&self.minutes) {
            times.push(self.minutes);
            times.sort_unstable();
        }
        let from = self.from;
        let twelve_hour = self.twelve_hour;
        let options = times
            .iter()
            .map(|minutes| {
                let label = format_clock(*minutes, twelve_hour);
                let label = match from {
                    Some(start) if *minutes > start => {
                        format!("{label}, {}", format_length(*minutes - start))
                    }
                    _ => label,
                };
                (minutes.to_string(), label)
            })
            .collect();
        let on_change = self.on_change.clone();
        dropdown(options, self.minutes.to_string(), move |key: String| {
            if let Ok(minutes) = key.parse::<u32>() {
                on_change.call(minutes);
            }
        })
        .enabled(self.enabled)
        .title("Time")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_are_short() {
        assert_eq!(format_length(30), "30 min");
        assert_eq!(format_length(60), "1 h");
        assert_eq!(format_length(90), "1.5 h");
        assert_eq!(format_length(120), "2 h");
    }
}
