//! Date and time: the live clock, NTP, the time zone, a manual clock setting,
//! the clock format and whether seconds are shown. Backends: `timedatectl`,
//! `gsettings` (clock format) and `bingux-config` (clock seconds).

use std::time::Duration;

use freya::prelude::*;

use crate::pages::bingux_config;
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{Group, Status, choice_row, page_column, row, status, switch_row, text_row};

const TIMEOUT: Duration = Duration::from_secs(5);
const INTERFACE: &str = "org.gnome.desktop.interface";

#[derive(Clone)]
struct Model {
    timezone: String,
    ntp: bool,
    clock_format: String,
    clock_seconds: Option<bool>,
    /// `YYYY-MM-DD HH:MM:SS` in the local zone, used as the manual time draft.
    local_time: String,
}

#[derive(Clone, Copy)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    zone_input: State<String>,
    time_input: State<String>,
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
        .ok_or_else(|| CommandError("timedatectl did not report a time zone.".into()))?;
    let ntp = ntp.ok_or_else(|| CommandError("timedatectl did not report NTP state.".into()))?;
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
    let local_time = system::run("date", &["+%Y-%m-%d %H:%M:%S"], TIMEOUT)?;
    Ok(Model {
        timezone,
        ntp,
        clock_format,
        clock_seconds,
        local_time,
    })
}

fn read_now() -> Result<String, String> {
    system::run("date", &["+%a %-d %b %Y, %H:%M:%S %Z"], TIMEOUT).map_err(|e| e.0)
}

fn set_timezone(zone: &str) -> CommandResult {
    if zone.is_empty() {
        return Err(CommandError(
            "Enter a time zone such as Europe/London.".into(),
        ));
    }
    let zones = system::run("timedatectl", &["list-timezones"], TIMEOUT)?;
    if !zones.lines().any(|line| line == zone) {
        return Err(CommandError(format!("Unknown time zone: {zone}.")));
    }
    system::run("timedatectl", &["set-timezone", zone], TIMEOUT)
}

fn set_time(text: &str) -> CommandResult {
    if text.is_empty() {
        return Err(CommandError("Enter a time as YYYY-MM-DD HH:MM:SS.".into()));
    }
    system::run("timedatectl", &["set-time", text], TIMEOUT)
}

/// Reloads the model in the background and refreshes the drafts from it.
fn reload(mut ctx: Ctx) {
    background_then(load, move |result| {
        if let Ok(model) = &result {
            ctx.zone_input.set(model.timezone.clone());
            ctx.time_input.set(model.local_time.clone());
        }
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

#[derive(PartialEq)]
pub struct DateTimePage;

impl Component for DateTimePage {
    fn render(&self) -> impl IntoElement {
        let model = use_state::<Option<Result<Model, CommandError>>>(|| None);
        let zone_input = use_state(String::new);
        let time_input = use_state(String::new);
        let error = use_state::<Option<String>>(|| None);
        let now = use_state::<Option<Result<String, String>>>(|| None);
        let ctx = Ctx {
            model,
            zone_input,
            time_input,
            error,
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

        let Some(loaded) = model.read().clone() else {
            return page_column().child(status(Status::Loading(
                "Reading the clock settings.".into(),
            )));
        };

        let model_data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column()
                    .child(status(Status::Error(e.0)).action("Try again", move |_| reload(ctx)));
            }
        };

        let now_text = match now.read().clone() {
            Some(Ok(text)) => text,
            Some(Err(message)) => message,
            None => "Reading the clock.".to_string(),
        };

        let ntp = model_data.ntp;
        let zone_caption = format!("Current: {}", model_data.timezone);
        let clock_format = model_data.clock_format.clone();

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        let time_row = if ntp {
            row("Set time manually")
                .subtitle("Turn off Automatic time to set the clock by hand.")
                .enabled(false)
        } else {
            text_row("Set time manually", time_input, move |text: String| {
                let text = text.trim().to_string();
                commit(ctx, move || set_time(&text));
            })
            .subtitle("Format: YYYY-MM-DD HH:MM:SS")
        };

        page.child(
            Group::titled("Clock")
                .row(row("Date and time").value(now_text))
                .row(switch_row("Automatic time", ntp, move |next: bool| {
                    let value = if next { "true" } else { "false" };
                    commit(ctx, move || {
                        system::run("timedatectl", &["set-ntp", value], TIMEOUT)
                    });
                }))
                .row(time_row),
        )
        .child(
            Group::titled("Time zone").row(
                text_row("Zone", zone_input, move |text: String| {
                    let zone = text.trim().to_string();
                    commit(ctx, move || set_timezone(&zone));
                })
                .subtitle(zone_caption),
            ),
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
                })),
        )
    }
}
