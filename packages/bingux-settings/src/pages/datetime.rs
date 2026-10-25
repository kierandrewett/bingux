//! Date and time: the live clock, whether the clock sets itself, the time
//! zone and the clock format. The time zone is a subpage with a world map and
//! a list, and setting the clock by hand is a sheet. Backends: `timedatectl`,
//! `gsettings` (clock format, weekday and date) and `bingux-config` (clock
//! seconds).

mod backend;
mod timezone_map;

use std::time::Duration;

use bingux_ui::prelude::{
    PageHead, PopupAction, PopupActions, Window, popup, use_popup_actions, use_window,
};
use bingux_ui::runtime::optimistic_then;
use bingux_ui::shell::MenuItem;
use bingux_ui::style::*;
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};
use freya::prelude::*;
use timezone_map::TimeZoneMap;

use super::loading::veiled;
use crate::pages::bingux_config;
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Overlays, Status, caption, choice_row, dropdown, form_row, menu_item, page_column,
    popup_column, row, search_field, skeleton_row, status, stepper, styled_label, switch_row,
    typography, use_overlays, use_page_busy,
};

const INTERFACE: &str = "org.gnome.desktop.interface";
const DATETIME: &str = "org.gnome.desktop.datetime";
const CALENDAR: &str = "org.gnome.desktop.calendar";
/// The most zones drawn at once. Typing narrows a longer list.
const ZONE_LIMIT: usize = 60;
/// The placeholder rows drawn while the zone list loads.
const PLACEHOLDER_ROWS: usize = 8;

/// Every zone name, as `timedatectl` lists them. Saved between runs, since the
/// list rarely changes.
fn list_zones() -> Result<Vec<String>, CommandError> {
    backend::run("timedatectl", &["list-timezones"])
        .map(|text| text.lines().map(str::to_string).collect())
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    timezone: String,
    /// Offset from UTC, as `+01:00`.
    utc_offset: String,
    ntp: bool,
    clock_format: String,
    clock_seconds: Option<bool>,
    show_weekday: Option<bool>,
    show_date: Option<bool>,
    auto_zone: Option<bool>,
    week_numbers: Option<bool>,
}

#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
}

/// The page's shared state. The subpage and the page read the same cache.
fn context(overlays: Overlays) -> Ctx {
    Ctx {
        model: system::cached_persistent::<Model, CommandError>("datetime"),
        overlays,
    }
}

/// Parses the `Key=value` lines of `timedatectl show`.
fn parse_timedatectl(text: &str) -> Result<(String, bool), CommandError> {
    let mut timezone = None;
    let mut ntp = None;
    for line in text.lines() {
        match line.split_once('=') {
            Some(("Timezone", value)) => timezone = Some(value.trim().to_string()),
            Some(("NTP", value)) => ntp = Some(value.trim() == "yes"),
            _ => {}
        }
    }
    let timezone = timezone
        .filter(|zone| !zone.is_empty())
        .ok_or_else(|| CommandError("The time zone could not be read.".into()))?;
    let ntp =
        ntp.ok_or_else(|| CommandError("Whether the clock sets itself could not be read.".into()))?;
    Ok((timezone, ntp))
}

fn load() -> Result<Model, CommandError> {
    let show = backend::run("timedatectl", &["show"])?;
    let (timezone, ntp) = parse_timedatectl(&show)?;
    let clock_format = system::unquote(&backend::gsettings_get(INTERFACE, "clock-format")?);
    // A missing bingux-config only loses the Show seconds row, not the page.
    let clock_seconds = backend::run(&bingux_config(), &["get", "desktop.clock_seconds"])
        .ok()
        .map(|value| value == "true");
    let show_weekday = backend::gsettings_get(INTERFACE, "clock-show-weekday")
        .ok()
        .map(|value| value == "true");
    let show_date = backend::gsettings_get(INTERFACE, "clock-show-date")
        .ok()
        .map(|value| value == "true");
    let auto_zone = backend::gsettings_get(DATETIME, "automatic-timezone")
        .ok()
        .map(|value| value == "true");
    let week_numbers = backend::gsettings_get(CALENDAR, "show-weekdate")
        .ok()
        .map(|value| value == "true");
    let utc_offset = backend::run("date", &["+%:z"])?;
    Ok(Model {
        timezone,
        utc_offset,
        ntp,
        clock_format,
        clock_seconds,
        show_weekday,
        show_date,
        auto_zone,
        week_numbers,
    })
}

/// The page's shape with blank values, drawn under the veil while the first
/// load runs. Every row a loaded page shows is already placed.
fn placeholder() -> Model {
    Model {
        timezone: "UTC".to_string(),
        utc_offset: "+00:00".to_string(),
        ntp: true,
        clock_format: "24h".to_string(),
        clock_seconds: Some(false),
        show_weekday: Some(false),
        show_date: Some(false),
        auto_zone: Some(false),
        week_numbers: Some(false),
    }
}

/// The local date and time, read once a second and formatted in `render`.
#[derive(Clone)]
struct Now {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    weekday: String,
}

fn read_now() -> Result<Now, String> {
    let failed = || "The time could not be read.".to_string();
    let text = backend::run("date", &["+%Y %m %d %H %M %S %A"]).map_err(|e| e.0)?;
    let parts: Vec<&str> = text.split_whitespace().collect();
    let &[year, month, day, hour, minute, second, weekday] = parts.as_slice() else {
        return Err(failed());
    };
    let number = |text: &str| text.parse::<i64>().map_err(|_| failed());
    Ok(Now {
        year: number(year)?,
        month: number(month)?,
        day: number(day)?,
        hour: number(hour)?,
        minute: number(minute)?,
        second: number(second)?,
        weekday: weekday.to_string(),
    })
}

/// The clock face: `09:05` in 24-hour form, `9:05 AM` in 12-hour form.
fn clock_text(now: &Now, twelve_hour: bool, seconds: bool) -> String {
    let mut text = if twelve_hour {
        let hour = match now.hour % 12 {
            0 => 12,
            hour => hour,
        };
        format!("{hour}:{:02}", now.minute)
    } else {
        format!("{:02}:{:02}", now.hour, now.minute)
    };
    if seconds {
        text.push_str(&format!(":{:02}", now.second));
    }
    if twelve_hour {
        text.push_str(if now.hour < 12 { " AM" } else { " PM" });
    }
    text
}

/// The date line under the clock, such as `Friday, 23 October 2026`.
fn date_text(now: &Now) -> String {
    let month = usize::try_from(now.month - 1)
        .ok()
        .and_then(|index| MONTHS.get(index))
        .copied()
        .unwrap_or("");
    format!("{}, {} {month} {}", now.weekday, now.day, now.year)
}

fn flag(on: bool) -> &'static str {
    if on { "true" } else { "false" }
}

fn set_timezone(zone: &str) -> CommandResult {
    backend::run("timedatectl", &["set-timezone", zone])
}

fn set_time(text: &str) -> CommandResult {
    backend::run("timedatectl", &["set-time", text])
}

/// `Europe/London` as "London, Europe"; `America/Argentina/Buenos_Aires` as
/// "Buenos Aires, America / Argentina".
fn zone_label(zone: &str) -> String {
    let mut parts: Vec<&str> = zone.split('/').collect();
    let city = parts.pop().unwrap_or(zone).replace('_', " ");
    if parts.is_empty() {
        city
    } else {
        format!("{city}, {}", parts.join(" / "))
    }
}

/// Reloads the model in the background, so the page shows what the backend
/// holds once a write has landed.
fn reload(ctx: Ctx) {
    let mut model = ctx.model;
    background_then(
        move || system::load_persistent("datetime", load),
        move |result| model.set(Some(result)),
    );
}

/// Shows `edit` on the model at once, then runs `work` in the background. A
/// failure puts the model back as it was and shows the error as a toast; a
/// success reloads the model from the backend.
fn apply(
    ctx: Ctx,
    edit: impl FnOnce(&mut Model),
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    let Some(Ok(model)) = ctx.model.read().clone() else {
        return;
    };
    let mut value = model.clone();
    edit(&mut value);
    optimistic_then(
        ctx.model,
        Some(ctx.overlays),
        Some(Ok(model)),
        Some(Ok(value)),
        move || work().map(|_| ()),
        move || reload(ctx),
    );
}

/// The right-click item that flips a switch: "Turn on" while it is off.
fn toggle_item(on: bool, set: impl Fn(bool) + 'static) -> MenuItem {
    menu_item(if on { "Turn off" } else { "Turn on" }, move |_| set(!on))
}

fn set_ntp(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.ntp = on,
        move || backend::run("timedatectl", &["set-ntp", flag(on)]),
    );
}

fn set_clock_format(ctx: Ctx, key: String) {
    let quoted = format!("'{key}'");
    apply(
        ctx,
        move |model| model.clock_format = key,
        move || backend::gsettings_set(INTERFACE, "clock-format", &quoted),
    );
}

fn set_seconds(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.clock_seconds = Some(on),
        move || {
            backend::run(
                &bingux_config(),
                &["set", "desktop.clock_seconds", flag(on)],
            )
        },
    );
}

fn set_weekday(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.show_weekday = Some(on),
        move || backend::gsettings_set(INTERFACE, "clock-show-weekday", flag(on)),
    );
}

fn set_show_date(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.show_date = Some(on),
        move || backend::gsettings_set(INTERFACE, "clock-show-date", flag(on)),
    );
}

fn set_auto_zone(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.auto_zone = Some(on),
        move || backend::gsettings_set(DATETIME, "automatic-timezone", flag(on)),
    );
}

fn set_week_numbers(ctx: Ctx, on: bool) {
    apply(
        ctx,
        move |model| model.week_numbers = Some(on),
        move || backend::gsettings_set(CALENDAR, "show-weekdate", flag(on)),
    );
}

/// Sets the time zone, shown at once.
fn pick_zone(ctx: Ctx, zone: String) {
    let work = zone.clone();
    apply(
        ctx,
        move |model| model.timezone = zone,
        move || set_timezone(&work),
    );
}

/// Copies `text` to the clipboard and says so with a toast.
fn copy_text(overlays: Overlays, text: String, done: &'static str) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => overlays.toast(done),
        Err(_) => overlays.toast("The text could not be copied."),
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Days in `month` (1 to 12) of `year`.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The fields of the set-date sheet. Copied from the live clock when the sheet
/// opens; the sheet body and its Set action both read them.
#[derive(Clone, Copy, PartialEq)]
struct DateForm {
    year: State<i64>,
    month: State<i64>,
    day: State<i64>,
    hour: State<i64>,
    minute: State<i64>,
    /// True while the clock is being set, so the header shows a spinner.
    setting: State<bool>,
    ctx: Ctx,
}

/// Copies the live clock into the sheet's fields, then opens the sheet.
fn open_date_sheet(window: Window, form: DateForm, now: State<Option<Result<Now, String>>>) {
    if let Some(Ok(reading)) = now.read().clone() {
        let DateForm {
            mut year,
            mut month,
            mut day,
            mut hour,
            mut minute,
            ..
        } = form;
        year.set(reading.year);
        month.set(reading.month);
        day.set(reading.day);
        hour.set(reading.hour);
        minute.set(reading.minute);
    }
    window.open(popup("Set date and time", DateBody { form }).flags(PopupFlags::FORM));
}

/// Sets the clock to the sheet's fields. The sheet closes once the write
/// succeeds; a failure is a toast and the sheet stays open.
fn apply_date(form: DateForm, window: Window) {
    let mut setting = form.setting;
    if setting() {
        return;
    }
    let year = form.year.read().to_owned();
    let month = form.month.read().to_owned();
    let day = (*form.day.read()).min(days_in_month(year, month));
    let text = format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:00",
        form.hour.read(),
        form.minute.read()
    );
    setting.set(true);
    background_then(
        move || set_time(&text),
        move |result| {
            let mut setting = form.setting;
            setting.set(false);
            match result {
                Ok(_) => {
                    window.close();
                    reload(form.ctx);
                }
                Err(failure) => form.ctx.overlays.toast(failure.0),
            }
        },
    );
}

/// The set-date sheet body: the date and the time as steppers and a month
/// dropdown. It reads the fields, so it redraws as they change.
#[derive(Clone, PartialEq)]
struct DateBody {
    form: DateForm,
}

impl Component for DateBody {
    fn render(&self) -> impl IntoElement {
        let DateForm {
            mut year,
            mut month,
            mut day,
            mut hour,
            mut minute,
            ..
        } = self.form;
        let window = use_window();
        let form = self.form;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Set".into(),
                enabled: true,
                on_press: EventHandler::new(move |_: ()| apply_date(form, window)),
            }),
        });
        let setting = self.form.setting;
        let last_day = days_in_month(year(), month());
        let month_options = MONTHS
            .iter()
            .enumerate()
            .map(|(index, name)| ((index + 1).to_string(), name.to_string()))
            .collect();
        popup_column()
            .child(
                Group::titled("Date")
                    .row(form_row(
                        "Day",
                        stepper(day().min(last_day), move |next| day.set(next)).range(1, last_day),
                    ))
                    .row(form_row(
                        "Month",
                        dropdown(month_options, month().to_string(), move |key: String| {
                            month.set(key.parse().unwrap_or(1))
                        })
                        .min_width(Dim::Field),
                    ))
                    .row(form_row(
                        "Year",
                        stepper(year(), move |next| year.set(next)).range(2000, 2099),
                    )),
            )
            .child(
                Group::titled("Time")
                    .row(form_row(
                        "Hour",
                        stepper(hour(), move |next| hour.set(next)).range(0, 23),
                    ))
                    .row(form_row(
                        "Minute",
                        stepper(minute(), move |next| minute.set(next)).range(0, 59),
                    )),
            )
            .maybe_child(setting().then(|| {
                Element::from(status(Status::Loading("Setting the clock...".to_string())))
            }))
    }
}

#[derive(PartialEq)]
pub struct DateTimePage;

impl Component for DateTimePage {
    fn render(&self) -> impl IntoElement {
        let navigator = use_navigator();
        let overlays = use_overlays();
        let ctx = context(overlays);
        let now = use_state::<Option<Result<Now, String>>>(|| None);
        let setting = use_state(|| false);
        let form = DateForm {
            year: use_state(|| 2000),
            month: use_state(|| 1),
            day: use_state(|| 1),
            hour: use_state(|| 0),
            minute: use_state(|| 0),
            setting,
            ctx,
        };

        use_hook(move || {
            reload(ctx);
            background(now, read_now);
            spawn(async move {
                loop {
                    timer(Duration::from_secs(1)).await;
                    background(now, read_now);
                }
            });
        });

        let window = use_window();
        use_page_busy(setting());

        let loaded = ctx.model.read().clone();
        let loading = loaded.is_none();
        let model_data = match loaded {
            None => placeholder(),
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                return page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(e.0)).action("Try again", move |_| reload(ctx)));
            }
        };

        let seconds = model_data.clock_seconds.unwrap_or(false);
        let (clock, date) = match now.read().clone() {
            Some(Ok(reading)) => (
                clock_text(&reading, model_data.clock_format == "12h", seconds),
                date_text(&reading),
            ),
            Some(Err(message)) => (message, String::new()),
            None => ("Reading the clock.".to_string(), String::new()),
        };
        // Free content in the card: the card body's 16 px on every side.
        let clock_face = rect()
            .tw_layout(BOX_BODY)
            .tw_layout(&[gap(S1)])
            .child(styled_label(typography::HERO, clock))
            .child(caption(date));

        let ntp = model_data.ntp;
        let twelve_hour = model_data.clock_format == "12h";

        // Shown only while "Set automatically" is off, so it needs no subtitle
        // or disabled state of its own.
        let time_row = row("Set date and time")
            .on_press(move |_| open_date_sheet(window, form, now))
            .context_menu(vec![menu_item("Set date and time...", move |_| {
                open_date_sheet(window, form, now)
            })]);
        let auto_zone_on = model_data.auto_zone.unwrap_or(false);
        let zone = model_data.timezone.clone();
        let zone_row = row("Time zone")
            .subtitle(format!("UTC{}", model_data.utc_offset))
            .value(zone_label(&model_data.timezone))
            .navigates()
            .enabled(!auto_zone_on)
            .on_press(move |_| navigator.push(Route::TimeZone))
            .context_menu(vec![
                menu_item("Change time zone", move |_| navigator.push(Route::TimeZone)),
                menu_item("Copy time zone name", move |_| {
                    copy_text(overlays, zone.clone(), "Time zone name copied.")
                }),
            ]);

        let other_format = if twelve_hour { "24h" } else { "12h" }.to_string();
        let format_row = choice_row(
            "Clock format",
            vec![
                ("24h".to_string(), "24-hour".to_string()),
                ("12h".to_string(), "12-hour".to_string()),
            ],
            if twelve_hour { "12h" } else { "24h" },
            move |key: String| set_clock_format(ctx, key),
        )
        .context_menu(vec![menu_item(
            if twelve_hour {
                "Use 24-hour"
            } else {
                "Use 12-hour"
            },
            move |_| set_clock_format(ctx, other_format.clone()),
        )]);

        let page = page_column()
            .child(PageHead::new())
            .child(
                Group::titled("Clock")
                    .row(clock_face)
                    .row(
                        switch_row("Set automatically", ntp, move |next: bool| {
                            set_ntp(ctx, next)
                        })
                        .context_menu(vec![toggle_item(ntp, move |next| set_ntp(ctx, next))])
                        .nested(!ntp, vec![time_row.into()]),
                    )
                    .rows(model_data.auto_zone.map(|on| {
                        Element::from(
                            switch_row("Set time zone automatically", on, move |next: bool| {
                                set_auto_zone(ctx, next)
                            })
                            .context_menu(vec![toggle_item(on, move |next| {
                                set_auto_zone(ctx, next)
                            })]),
                        )
                    }))
                    .row(zone_row),
            )
            .child(
                Group::titled("Format")
                    .row(format_row)
                    .rows(model_data.clock_seconds.map(|on| {
                        Element::from(
                            switch_row("Show seconds", on, move |next: bool| {
                                set_seconds(ctx, next)
                            })
                            .context_menu(vec![toggle_item(on, move |next| {
                                set_seconds(ctx, next)
                            })]),
                        )
                    }))
                    .rows(model_data.show_weekday.map(|on| {
                        Element::from(
                            switch_row("Show weekday in the top bar", on, move |next: bool| {
                                set_weekday(ctx, next)
                            })
                            .context_menu(vec![toggle_item(on, move |next| {
                                set_weekday(ctx, next)
                            })]),
                        )
                    }))
                    .rows(model_data.show_date.map(|on| {
                        Element::from(
                            switch_row("Show date in the top bar", on, move |next: bool| {
                                set_show_date(ctx, next)
                            })
                            .context_menu(vec![toggle_item(on, move |next| {
                                set_show_date(ctx, next)
                            })]),
                        )
                    }))
                    .rows(model_data.week_numbers.map(|on| {
                        Element::from(
                            switch_row(
                                "Show week numbers in the calendar",
                                on,
                                move |next: bool| set_week_numbers(ctx, next),
                            )
                            .context_menu(vec![toggle_item(on, move |next| {
                                set_week_numbers(ctx, next)
                            })]),
                        )
                    })),
            );
        veiled(page, loading)
    }
}

/// The time zone subpage: a world map to click, and below it the list of every
/// zone, narrowed by typing. Right click a zone to set it or copy its name.
#[derive(PartialEq)]
pub struct TimeZonePage;

impl Component for TimeZonePage {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let ctx = context(overlays);
        let query = use_state(String::new);
        let zones = system::use_cached_persistent("timezones", list_zones);
        let zone_list = zones.read().clone();
        let list: &[String] = match &zone_list {
            Some(Ok(list)) => list,
            _ => &[],
        };
        let (current, offset) = match displayed(ctx) {
            Some(model) => (model.timezone, model.utc_offset),
            None => (String::new(), String::new()),
        };

        let needle = query.read().trim().to_lowercase();
        let mut matching: Vec<&String> = list
            .iter()
            .filter(|zone| {
                needle.is_empty()
                    || zone.to_lowercase().contains(&needle)
                    || zone_label(zone).to_lowercase().contains(&needle)
            })
            .collect();
        // Before anything is typed, the current zone comes first, so it is in
        // view whichever part of the long list it belongs to.
        if needle.is_empty()
            && let Some(at) = matching.iter().position(|zone| **zone == current)
        {
            let chosen = matching.remove(at);
            matching.insert(0, chosen);
        }
        let hidden = matching.len().saturating_sub(ZONE_LIMIT);

        let rows = matching.iter().take(ZONE_LIMIT).map(|zone| {
            let zone = zone.to_string();
            let chosen = zone == current;
            let mut item = row(zone_label(&zone)).selected(chosen);
            if chosen && !offset.is_empty() {
                item = item.subtitle(format!("UTC{offset}"));
            }
            let set = zone.clone();
            let copy = zone.clone();
            let mut set_item = menu_item("Set time zone", move |_| pick_zone(ctx, set.clone()));
            if chosen {
                set_item = set_item.disabled();
            }
            let copy_item = menu_item("Copy time zone name", move |_| {
                copy_text(overlays, copy.clone(), "Time zone name copied.")
            });
            Element::from(
                item.on_press(move |_| pick_zone(ctx, zone.clone()))
                    .context_menu(vec![set_item, copy_item]),
            )
        });
        let mut group = Group::new(None::<String>);
        match &zone_list {
            None => group = group.rows((0..PLACEHOLDER_ROWS).map(|_| skeleton_row())),
            Some(Err(failure)) => {
                group = group.row(status(Status::Error(failure.0.clone())));
            }
            Some(Ok(_)) => {
                group = group.rows(rows);
                if matching.is_empty() {
                    group = group.row(status(Status::Empty("Nothing matches.".to_string())));
                }
            }
        }

        let mut page = page_column()
            .child(PageHead::new())
            .child(TimeZoneMap::new(current.clone(), ctx))
            .child(
                search_field(query, |_| {})
                    .placeholder("Search for a city")
                    .fill_width(),
            )
            .child(group);
        if hidden > 0 {
            page = page.child(caption(format!(
                "{hidden} more. Keep typing to narrow the list."
            )));
        }
        page
    }
}

/// The model as the screen shows it, or `None` while it loads or has failed.
fn displayed(ctx: Ctx) -> Option<Model> {
    ctx.model.read().clone()?.ok()
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("datetime", load);
}
