//! Date and time: the live clock, whether the clock sets itself, the time
//! zone and the clock format. The time zone is a subpage and setting the clock
//! by hand is a sheet. Backends: `timedatectl`, `gsettings` (clock format,
//! weekday and date) and `bingux-config` (clock seconds).

use std::time::Duration;

use bingux_ui::layout::vstack;
use bingux_ui::style::*;
use freya::prelude::*;

use crate::pages::bingux_config;
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Overlays, Status, caption, choice_list, choice_row, dropdown, form_row, page_column,
    row, sheet, skeleton, status, stepper, styled_label, switch_row, typography, use_overlays,
    use_page_busy,
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
}

#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
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
    Ok(Model {
        timezone,
        utc_offset,
        ntp,
        clock_format,
        clock_seconds,
        show_weekday,
        show_date,
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

/// Reloads the model in the background.
fn reload(ctx: Ctx) {
    let mut model = ctx.model;
    background_then(load, move |result| {
        model.set(Some(result));
    });
}

/// Runs a write in the background, then reloads, or shows a failure as a toast.
fn commit(ctx: Ctx, work: impl FnOnce() -> CommandResult + Send + 'static) {
    background_then(work, move |result| match result {
        Ok(_) => reload(ctx),
        Err(e) => ctx.overlays.toast(e.0),
    });
}

/// Writes a true or false setting in the interface schema.
fn set_interface_flag(ctx: Ctx, key: &'static str, next: bool) {
    let value = if next { "true" } else { "false" };
    commit(ctx, move || system::gsettings_set(INTERFACE, key, value));
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
fn open_date_sheet(form: DateForm, now: State<Option<Result<Now, String>>>) {
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
    form.ctx.overlays.show_sheet(
        sheet("Set date and time", DateBody { form }).primary("Set", move |_| apply_date(form)),
    );
}

/// Sets the clock to the sheet's fields. The sheet closes once the write
/// succeeds; a failure is a toast and the sheet stays open.
fn apply_date(form: DateForm) {
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
                    form.ctx.overlays.close_sheet();
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
        let last_day = days_in_month(year(), month());
        let month_options = MONTHS
            .iter()
            .enumerate()
            .map(|(index, name)| ((index + 1).to_string(), name.to_string()))
            .collect();
        vstack(S2)
            .tw_layout(&[w_full()])
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
    }
}

#[derive(PartialEq)]
pub struct DateTimePage;

impl Component for DateTimePage {
    fn render(&self) -> impl IntoElement {
        let model = system::cached("datetime");
        let navigator = use_navigator();
        let overlays = use_overlays();
        let now = use_state::<Option<Result<Now, String>>>(|| None);
        let setting = use_state(|| false);
        let ctx = Ctx { model, overlays };
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

        use_page_busy(setting());

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
        // Free content in the card: the card body's 16 px on every side.
        let clock_face = rect()
            .tw_layout(CARD_BODY)
            .tw_layout(&[gap(S1)])
            .child(styled_label(typography::HERO, clock))
            .child(caption(date));

        let ntp = model_data.ntp;
        let clock_format = model_data.clock_format.clone();

        let time_row = row("Set date and time")
            .subtitle(if ntp {
                "Turn off Set automatically to change the clock yourself."
            } else {
                ""
            })
            .enabled(!ntp)
            .on_press(move |_| open_date_sheet(form, now));
        let zone_row = row("Time zone")
            .subtitle(format!("UTC{}", model_data.utc_offset))
            .value(zone_label(&model_data.timezone))
            .navigates()
            .on_press(move |_| navigator.push(Route::TimeZone));

        page_column()
            .child(
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

/// The time zone subpage: every zone, narrowed by typing. The UTC offset is
/// shown only on the selected zone.
#[derive(PartialEq)]
pub struct TimeZonePage;

impl Component for TimeZonePage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Result<Model, CommandError>>> = system::cached("datetime");
        let navigator = use_navigator();
        let overlays = use_overlays();
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
        let (current, current_offset) = match model.read().as_ref() {
            Some(Ok(data)) => (data.timezone.clone(), data.utc_offset.clone()),
            _ => (String::new(), String::new()),
        };
        let options = list
            .iter()
            .map(|zone| {
                let label = zone_label(zone);
                let label = if *zone == current && !current_offset.is_empty() {
                    format!("{label} (UTC{current_offset})")
                } else {
                    label
                };
                (zone.clone(), label)
            })
            .collect();
        page_column().child(
            choice_list(options, current, move |zone: String| {
                background_then(
                    move || set_timezone(&zone),
                    move |result| match result {
                        Ok(_) => {
                            reload(Ctx { model, overlays });
                            navigator.pop();
                        }
                        Err(failure) => overlays.toast(failure.0),
                    },
                );
            })
            .placeholder("Search for a city"),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("datetime", load);
}
