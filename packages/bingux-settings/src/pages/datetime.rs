//! Date and time: the live clock, whether the clock sets itself, the time
//! zone and the clock format. The time zone and the manual clock setting are
//! subpages. Backends: `timedatectl`, `gsettings` (clock format, weekday and date) and
//! `bingux-config` (clock seconds).

use std::time::Duration;

use bingux_ui::scale::{GUTTER, S1, S2, TEXT_DISPLAY};
use freya::prelude::*;

use crate::pages::bingux_config;
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Status, action_row, caption, choice_list, choice_row, dropdown, page_column,
    primary_button, row, skeleton, status, stepper, switch_row,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const INTERFACE: &str = "org.gnome.desktop.interface";

#[derive(Clone)]
struct Model {
    timezone: String,
    /// Offset from UTC, as `+01:00`.
    utc_offset: String,
    ntp: bool,
    clock_format: String,
    clock_seconds: Option<bool>,
    show_weekday: Option<bool>,
    show_date: Option<bool>,
    /// `YYYY-MM-DD HH:MM:SS` in the local zone, where the manual time starts.
    local_time: String,
}

#[derive(Clone, Copy)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
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
    let show = system::run("timedatectl", &["show"], TIMEOUT)?;
    let (timezone, ntp) = parse_timedatectl(&show)?;
    let clock_format = system::unquote(&system::gsettings_get(INTERFACE, "clock-format")?);
    // A missing bingux-config only loses the Show seconds row, not the page.
    let clock_seconds = system::run(&bingux_config(), &["get", "desktop.clock_seconds"], TIMEOUT)
        .ok()
        .map(|value| value == "true");
    let show_weekday = system::gsettings_get(INTERFACE, "clock-show-weekday")
        .ok()
        .map(|value| value == "true");
    let show_date = system::gsettings_get(INTERFACE, "clock-show-date")
        .ok()
        .map(|value| value == "true");
    let utc_offset = system::run("date", &["+%:z"], TIMEOUT)?;
    let local_time = system::run("date", &["+%Y-%m-%d %H:%M:%S"], TIMEOUT)?;
    Ok(Model {
        timezone,
        utc_offset,
        ntp,
        clock_format,
        clock_seconds,
        show_weekday,
        show_date,
        local_time,
    })
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
    let text = system::run("date", &["+%Y %m %d %H %M %S %A"], TIMEOUT).map_err(|e| e.0)?;
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

fn set_timezone(zone: &str) -> CommandResult {
    system::run("timedatectl", &["set-timezone", zone], TIMEOUT)
}

fn set_time(text: &str) -> CommandResult {
    system::run("timedatectl", &["set-time", text], TIMEOUT)
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

/// Reloads the model in the background and refreshes the drafts from it.
fn reload(mut ctx: Ctx) {
    background_then(load, move |result| {
        ctx.model.set(Some(result));
    });
}

/// Runs a write in the background, then clears or shows the error and reloads.
fn commit(mut ctx: Ctx, work: impl FnOnce() -> CommandResult + Send + 'static) {
    background_then(work, move |result| match result {
        Ok(_) => {
            ctx.error.set(None);
            reload(ctx);
        }
        Err(e) => ctx.error.set(Some(e.0)),
    });
}

/// Writes a true or false setting in the interface schema.
fn set_interface_flag(ctx: Ctx, key: &'static str, next: bool) {
    let value = if next { "true" } else { "false" };
    commit(ctx, move || system::gsettings_set(INTERFACE, key, value));
}

#[derive(PartialEq)]
pub struct DateTimePage;

impl Component for DateTimePage {
    fn render(&self) -> impl IntoElement {
        let model = system::cached("datetime");
        let navigator = use_navigator();
        let error = use_state::<Option<String>>(|| None);
        let now = use_state::<Option<Result<Now, String>>>(|| None);
        let ctx = Ctx { model, error };

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

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };

        let model_data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column()
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
        let clock_face = rect()
            .width(Size::fill())
            .padding((S2, GUTTER))
            .spacing(S1)
            .child(
                label()
                    .text(clock)
                    .font_size(TEXT_DISPLAY)
                    .font_weight(FontWeight::SEMI_BOLD),
            )
            .child(caption(date));

        let ntp = model_data.ntp;
        let clock_format = model_data.clock_format.clone();

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        let time_row = row("Set date and time")
            .subtitle(if ntp {
                "Turn off Set automatically to change the clock yourself."
            } else {
                ""
            })
            .enabled(!ntp)
            .navigates()
            .on_press(move |_| navigator.push(Route::SetTime));
        let zone_row = row("Time zone")
            .subtitle(format!("UTC{}", model_data.utc_offset))
            .value(zone_label(&model_data.timezone))
            .navigates()
            .on_press(move |_| navigator.push(Route::TimeZone));

        page.child(
            Group::titled("Clock")
                .row(clock_face)
                .row(switch_row("Set automatically", ntp, move |next: bool| {
                    let value = if next { "true" } else { "false" };
                    commit(ctx, move || {
                        system::run("timedatectl", &["set-ntp", value], TIMEOUT)
                    });
                }))
                .row(time_row)
                .row(zone_row),
        )
        .child(
            Group::titled("Format")
                .row(choice_row(
                    "Clock format",
                    vec![
                        ("24h".to_string(), "24-hour".to_string()),
                        ("12h".to_string(), "12-hour".to_string()),
                    ],
                    &clock_format,
                    move |key: String| {
                        let quoted = format!("'{key}'");
                        commit(ctx, move || {
                            system::gsettings_set(INTERFACE, "clock-format", &quoted)
                        });
                    },
                ))
                .rows(model_data.clock_seconds.map(|clock_seconds| {
                    Element::from(switch_row(
                        "Show seconds",
                        clock_seconds,
                        move |next: bool| {
                            let value = if next { "true" } else { "false" };
                            commit(ctx, move || {
                                system::run(
                                    &bingux_config(),
                                    &["set", "desktop.clock_seconds", value],
                                    TIMEOUT,
                                )
                            });
                        },
                    ))
                }))
                .rows(model_data.show_weekday.map(|on| {
                    Element::from(switch_row(
                        "Show weekday in the top bar",
                        on,
                        move |next: bool| set_interface_flag(ctx, "clock-show-weekday", next),
                    ))
                }))
                .rows(model_data.show_date.map(|on| {
                    Element::from(switch_row(
                        "Show date in the top bar",
                        on,
                        move |next: bool| set_interface_flag(ctx, "clock-show-date", next),
                    ))
                })),
        )
    }
}

/// The time zone subpage: every zone, narrowed by typing.
#[derive(PartialEq)]
pub struct TimeZonePage;

impl Component for TimeZonePage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Result<Model, CommandError>>> = system::cached("datetime");
        let navigator = use_navigator();
        let error = use_state::<Option<String>>(|| None);
        let zones = use_state::<Option<Result<Vec<String>, CommandError>>>(|| None);
        use_hook(move || {
            background(zones, || {
                system::run("timedatectl", &["list-timezones"], TIMEOUT)
                    .map(|text| text.lines().map(str::to_string).collect())
            })
        });
        let Some(Ok(list)) = zones.read().clone() else {
            return match zones.read().clone() {
                Some(Err(failure)) => page_column().child(status(Status::Error(failure.0))),
                _ => page_column().child(skeleton()),
            };
        };
        let current = match model.read().as_ref() {
            Some(Ok(data)) => data.timezone.clone(),
            _ => String::new(),
        };
        let options = list
            .iter()
            .map(|zone| (zone.clone(), zone_label(zone)))
            .collect();
        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        if let Some(Ok(data)) = model.read().as_ref() {
            page = page.child(caption(format!(
                "Current time zone: {} (UTC{}).",
                zone_label(&data.timezone),
                data.utc_offset
            )));
        }
        page.child(
            choice_list(options, current, move |zone: String| {
                let mut error = error;
                background_then(
                    move || set_timezone(&zone),
                    move |result| match result {
                        Ok(_) => {
                            reload(Ctx { model, error });
                            navigator.pop();
                        }
                        Err(failure) => error.set(Some(failure.0)),
                    },
                );
            })
            .placeholder("Search for a city"),
        )
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

/// The manual clock subpage: the date and time, set with steppers.
#[derive(PartialEq)]
pub struct SetTimePage;

impl Component for SetTimePage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Result<Model, CommandError>>> = system::cached("datetime");
        let navigator = use_navigator();
        let error = use_state::<Option<String>>(|| None);
        let start = match model.read().as_ref() {
            Some(Ok(data)) => data.local_time.clone(),
            _ => String::new(),
        };
        // `YYYY-MM-DD HH:MM:SS`
        let number = |from: usize, to: usize| -> i64 {
            start
                .get(from..to)
                .and_then(|text| text.parse().ok())
                .unwrap_or(0)
        };
        let mut year = use_state(|| number(0, 4));
        let mut month = use_state(|| number(5, 7).max(1));
        let mut day = use_state(|| number(8, 10).max(1));
        let mut hour = use_state(|| number(11, 13));
        let mut minute = use_state(|| number(14, 16));
        if start.is_empty() {
            return page_column().child(skeleton());
        }
        let last_day = days_in_month(year(), month());
        let month_options = MONTHS
            .iter()
            .enumerate()
            .map(|(index, name)| ((index + 1).to_string(), name.to_string()))
            .collect();

        let apply = move |_| {
            let text = format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:00",
                year(),
                month(),
                day().min(days_in_month(year(), month())),
                hour(),
                minute()
            );
            let mut error = error;
            background_then(
                move || set_time(&text),
                move |result| match result {
                    Ok(_) => {
                        reload(Ctx { model, error });
                        navigator.pop();
                    }
                    Err(failure) => error.set(Some(failure.0)),
                },
            );
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        page.child(
            Group::new(None::<String>)
                .row(row("Day").trailing(
                    stepper(day().min(last_day), move |next| day.set(next)).range(1, last_day),
                ))
                .row(
                    row("Month").trailing(
                        dropdown(month_options, month().to_string(), move |key: String| {
                            month.set(key.parse().unwrap_or(1))
                        })
                        .min_width(bingux_ui::scale::FIELD_WIDTH),
                    ),
                )
                .row(
                    row("Year")
                        .trailing(stepper(year(), move |next| year.set(next)).range(2000, 2099)),
                )
                .row(row("Hour").trailing(stepper(hour(), move |next| hour.set(next)).range(0, 23)))
                .row(
                    row("Minute")
                        .trailing(stepper(minute(), move |next| minute.set(next)).range(0, 59)),
                )
                .row(action_row(None, primary_button("Set", apply).compact())),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("datetime", load);
}
