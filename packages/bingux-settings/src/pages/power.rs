//! Power: battery state, power mode, screen blanking and locking, automatic
//! suspend and the power button action. Everything is read in the background
//! when the page opens and reloaded after each successful change.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::Icon;
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Curve, Group, Series, Status, Unit, choice_row, line_chart, page_column, ring, row, skeleton,
    status, switch_row,
};

const SESSION: &str = "org.gnome.desktop.session";
const SCREENSAVER: &str = "org.gnome.desktop.screensaver";
const POWER_PLUGIN: &str = "org.gnome.settings-daemon.plugins.power";

/// Delays offered for automatic suspend, in seconds.
const SUSPEND_DELAYS: &[u32] = &[900, 1200, 1800, 2700, 3600, 5400, 7200];

#[derive(Clone, PartialEq)]
struct Battery {
    /// Charge as a fraction in `0..=1`, when UPower reports a percentage.
    fraction: Option<f64>,
    percentage: String,
    state: String,
    time: Option<(String, String)>,
    /// Charge over the last day as (Unix seconds, percent). Empty when UPower
    /// keeps no history for the battery.
    history: Vec<(f64, f64)>,
    /// Local offset from UTC in seconds, for the time axis.
    offset: i64,
}

#[derive(Clone, PartialEq)]
struct Profiles {
    choices: Vec<(String, String)>,
    active: String,
}

/// Automatic suspend for one power source: whether it is on and the delay in
/// seconds before it happens.
#[derive(Clone, PartialEq)]
struct Suspend {
    enabled: bool,
    delay: u32,
}

#[derive(Clone, PartialEq)]
struct Model {
    battery: Result<Option<Battery>, CommandError>,
    profiles: Result<Profiles, CommandError>,
    idle_delay: Result<u32, CommandError>,
    lock_enabled: Result<bool, CommandError>,
    lock_delay: Result<u32, CommandError>,
    /// `None` when the power plugin schema is missing: the row is hidden.
    power_button: Option<String>,
    /// `None` when the key is missing: the row is hidden.
    dim: Option<bool>,
    suspend_ac: Option<Suspend>,
    suspend_battery: Option<Suspend>,
}

fn load() -> Model {
    Model {
        battery: battery(),
        profiles: profiles(),
        idle_delay: gsettings_u32(SESSION, "idle-delay"),
        lock_enabled: system::gsettings_get(SCREENSAVER, "lock-enabled")
            .map(|raw| raw.trim() == "true"),
        lock_delay: gsettings_u32(SCREENSAVER, "lock-delay"),
        power_button: system::gsettings_get(POWER_PLUGIN, "power-button-action")
            .ok()
            .map(|raw| system::unquote(&raw)),
        dim: system::gsettings_get(POWER_PLUGIN, "idle-dim")
            .ok()
            .map(|raw| raw.trim() == "true"),
        suspend_ac: suspend("ac"),
        suspend_battery: suspend("battery"),
    }
}

fn battery() -> Result<Option<Battery>, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(fake) = fake_battery() {
        return Ok(Some(fake));
    }
    let devices = system::run("upower", &["-e"], Duration::from_secs(5))?;
    let Some(path) = devices
        .lines()
        .map(str::trim)
        .find(|line| line.contains("battery"))
    else {
        return Ok(None);
    };
    let info = system::run("upower", &["-i", path], Duration::from_secs(5))?;

    let mut percentage = None;
    let mut state = None;
    let mut time = None;
    for line in info.lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "percentage" => percentage = Some(value.to_string()),
            "state" => state = Some(battery_state(value)),
            "time to empty" => time = Some(("Time to empty".to_string(), value.to_string())),
            "time to full" => time = Some(("Time to full".to_string(), value.to_string())),
            _ => {}
        }
    }

    let fraction = percentage.as_deref().and_then(parse_fraction);
    Ok(Some(Battery {
        fraction,
        percentage: percentage.unwrap_or_else(|| "Unknown".to_string()),
        state: state.unwrap_or_else(|| "Unknown".to_string()),
        time,
        history: charge_history(path).unwrap_or_default(),
        offset: utc_offset(),
    }))
}

/// A percentage such as `87%` as a fraction in `0..=1`.
fn parse_fraction(text: &str) -> Option<f64> {
    let value: f64 = text.trim().strip_suffix('%')?.trim().parse().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

/// Charge over the last day from UPower's history of `device`, as (Unix
/// seconds, percent). The daemon refuses the call when it keeps no history.
fn charge_history(device: &str) -> Result<Vec<(f64, f64)>, CommandError> {
    let reply = system::run(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.UPower",
            "--object-path",
            device,
            "--method",
            "org.freedesktop.UPower.Device.GetHistory",
            "charge",
            "86400",
            "600",
        ],
        Duration::from_secs(5),
    )?;
    Ok(parse_history(&reply))
}

/// Read gdbus's reply, `([(time, percent, state), …],)`, into (time, percent)
/// pairs. Each tuple sits between a `(` and the `)` after it.
fn parse_history(reply: &str) -> Vec<(f64, f64)> {
    reply
        .split(')')
        .filter_map(|piece| {
            let inner = &piece[piece.rfind('(')? + 1..];
            let mut fields = inner.split(',').map(str::trim);
            let time: f64 = last_word(fields.next()?)?.parse().ok()?;
            let percent: f64 = last_word(fields.next()?)?.parse().ok()?;
            Some((time, percent))
        })
        .collect()
}

/// The last word of a field, so `uint64 1700000000` reads as its number.
fn last_word(field: &str) -> Option<&str> {
    field.split_whitespace().last()
}

/// The local offset from UTC in seconds, from `date +%z` (`+0100`).
fn utc_offset() -> i64 {
    system::run("date", &["+%z"], Duration::from_secs(5))
        .ok()
        .and_then(|text| parse_offset(text.trim()))
        .unwrap_or(0)
}

fn parse_offset(text: &str) -> Option<i64> {
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let hours: i64 = text.get(1..3)?.parse().ok()?;
    let minutes: i64 = text.get(3..5)?.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

fn profiles() -> Result<Profiles, CommandError> {
    let list = system::run("powerprofilesctl", &["list"], Duration::from_secs(5))?;
    let active = system::run("powerprofilesctl", &["get"], Duration::from_secs(5))?;
    let choices = list
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let name = line
                .trim()
                .trim_start_matches('*')
                .trim()
                .trim_end_matches(':')
                .trim();
            (!name.is_empty()).then(|| (name.to_string(), profile_label(name)))
        })
        .collect();
    Ok(Profiles {
        choices,
        active: active.trim().to_string(),
    })
}

fn profile_label(name: &str) -> String {
    match name {
        "performance" => "Performance".to_string(),
        "balanced" => "Balanced".to_string(),
        "power-saver" => "Power saver".to_string(),
        other => sentence(other),
    }
}

/// The plain sentence under each power mode.
fn profile_meaning(name: &str) -> &'static str {
    match name {
        "power-saver" => "Uses less power. Slower under heavy load.",
        "balanced" => "The usual choice for most work.",
        "performance" => "Fastest. Uses more power and runs warmer.",
        _ => "",
    }
}

fn profile_icon(name: &str) -> Icon {
    match name {
        "power-saver" => Icon::PowerProfilePowerSaver,
        "performance" => Icon::PowerProfilePerformance,
        _ => Icon::PowerProfileBalanced,
    }
}

fn gsettings_u32(schema: &str, key: &str) -> Result<u32, CommandError> {
    let raw = system::gsettings_get(schema, key)?;
    raw.trim()
        .strip_prefix("uint32 ")
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| CommandError("This setting could not be read.".into()))
}

/// Automatic suspend for `source` (`ac` or `battery`). Missing keys hide the
/// whole pair.
fn suspend(source: &str) -> Option<Suspend> {
    let kind =
        system::gsettings_get(POWER_PLUGIN, &format!("sleep-inactive-{source}-type")).ok()?;
    let delay =
        system::gsettings_get(POWER_PLUGIN, &format!("sleep-inactive-{source}-timeout")).ok()?;
    // int32 values print without a type prefix.
    let seconds: i64 = delay
        .trim()
        .strip_prefix("int32 ")
        .unwrap_or(delay.trim())
        .parse()
        .ok()?;
    Some(Suspend {
        enabled: system::unquote(&kind) == "suspend",
        delay: u32::try_from(seconds.max(0)).unwrap_or(0),
    })
}

/// Capitalise the first letter and turn dashes into spaces (`fully-charged`
/// becomes `Fully charged`).
fn sentence(value: &str) -> String {
    let text = value.replace('-', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

/// A battery state as a plain word. UPower reports `pending-charge` and
/// `pending-discharge` when the battery is waiting for a change in the charger.
fn battery_state(value: &str) -> String {
    match value {
        "pending-charge" => "Waiting to charge".to_string(),
        "pending-discharge" => "Waiting to discharge".to_string(),
        other => sentence(other),
    }
}

fn duration_label(seconds: u32) -> String {
    if seconds % 60 == 0 {
        let minutes = seconds / 60;
        format!(
            "{minutes} {}",
            if minutes == 1 { "minute" } else { "minutes" }
        )
    } else {
        format!("{seconds} seconds")
    }
}

/// Timeout choices in seconds, with `zero` as the label for 0. A current
/// value outside the list is kept so the row still shows the real setting.
fn timeout_choices(values: &[u32], current: u32, zero: &str) -> Vec<(String, String)> {
    let label = |seconds: u32| {
        if seconds == 0 {
            zero.to_string()
        } else {
            duration_label(seconds)
        }
    };
    let mut options: Vec<(String, String)> = values
        .iter()
        .map(|&seconds| (seconds.to_string(), label(seconds)))
        .collect();
    if !values.contains(&current) {
        options.push((current.to_string(), label(current)));
    }
    options
}

/// Run a write in the background; on success reload the model, on failure
/// show the message at the top of the page.
fn apply(
    loaded: State<Option<Model>>,
    mut error: State<Option<String>>,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    background_then(work, move |result| match result {
        Ok(_) => {
            error.set(None);
            background(loaded, load);
        }
        Err(e) => error.set(Some(e.0)),
    });
}

/// The three power modes as rows with icons. A mode the computer does not
/// offer stays visible but cannot be chosen.
fn mode_group(
    loaded: State<Option<Model>>,
    error: State<Option<String>>,
    profiles: &Profiles,
) -> Group {
    let mut group = Group::titled("Power mode");
    for name in ["power-saver", "balanced", "performance"] {
        let offered = profiles.choices.iter().any(|(key, _)| key == name);
        let active = profiles.active == name;
        let key = name.to_string();
        let mut item = row(profile_label(name))
            .leading(profile_icon(name))
            .subtitle(profile_meaning(name))
            .selected(active)
            .enabled(offered);
        if !offered {
            item = item.value("Not available");
        } else if active {
            item = item.value("In use");
        } else {
            item = item.on_press(move |_| {
                let key = key.clone();
                apply(loaded, error, move || {
                    system::run("powerprofilesctl", &["set", &key], Duration::from_secs(5))
                });
            });
        }
        group = group.row(item);
    }
    group
}

/// A switch for automatic suspend on one power source, with the delay below
/// it. The delay is editable only while the switch is on.
fn suspend_rows(
    loaded: State<Option<Model>>,
    error: State<Option<String>>,
    source: &'static str,
    title: &str,
    state: &Suspend,
) -> Group {
    let key_type = format!("sleep-inactive-{source}-type");
    let key_timeout = format!("sleep-inactive-{source}-timeout");
    let toggle_key = key_type.clone();
    Group::titled(title)
        .row(switch_row(
            "Suspend when inactive",
            state.enabled,
            move |on: bool| {
                let value = if on { "'suspend'" } else { "'nothing'" };
                let key = toggle_key.clone();
                apply(loaded, error, move || {
                    system::gsettings_set(POWER_PLUGIN, &key, value)
                });
            },
        ))
        .row(
            choice_row(
                "Suspend after",
                timeout_choices(SUSPEND_DELAYS, state.delay, "Never"),
                &state.delay.to_string(),
                move |key: String| {
                    let key_timeout = key_timeout.clone();
                    apply(loaded, error, move || {
                        system::gsettings_set(POWER_PLUGIN, &key_timeout, &key)
                    });
                },
            )
            .enabled(state.enabled),
        )
}

/// The charge as a ring with the state under the percentage, then the time
/// left. A battery that reports no percentage gets plain rows instead.
fn battery_group(battery: &Battery) -> Group {
    let group = Group::titled("Battery");
    let group = match battery.fraction {
        Some(fraction) => group.row(
            row("Charge").leading(Icon::BatteryLevel50).trailing(
                ring(fraction)
                    .caption(battery.state.clone())
                    .label(format!("Charge {}, {}", battery.percentage, battery.state)),
            ),
        ),
        None => group
            .row(
                row("Charge")
                    .leading(Icon::BatteryLevel50)
                    .value(battery.percentage.clone()),
            )
            .row(row("State").value(battery.state.clone())),
    };
    match &battery.time {
        Some((label, value)) => group.row(row(label.clone()).value(value.clone())),
        None => group,
    }
}

/// The charge over the last day as an area chart, or nothing when UPower
/// keeps no history for the battery.
fn history_group(battery: &Battery) -> Option<Group> {
    if battery.history.is_empty() {
        return None;
    }
    let chart = line_chart([Series::new("Charge", battery.history.clone())])
        .y_unit(Unit::Percent)
        .x_unit(Unit::Clock {
            offset: battery.offset,
        })
        .curve(Curve::Monotone)
        .area()
        .label("Battery charge over the last 24 hours");
    Some(Group::titled("Last 24 hours").row(chart))
}

/// Snapshot builds only: `BINGUX_SETTINGS_FAKE_BATTERY` swaps in a charging
/// battery with a day of history, for rendering on a machine without one.
#[cfg(feature = "snapshot")]
fn fake_battery() -> Option<Battery> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_BATTERY")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    let history = (0..=144)
        .map(|step| {
            let progress = f64::from(step) / 144.0;
            let ago = f64::from(144 - step) * 600.0;
            let percent = 22.0 + 42.0 * progress + 3.0 * (progress * 11.0).sin() * (1.0 - progress);
            (now - ago, percent)
        })
        .collect();
    Some(Battery {
        fraction: Some(0.64),
        percentage: "64%".to_string(),
        state: "Charging".to_string(),
        time: Some(("Time to full".to_string(), "1 hour 20 minutes".to_string())),
        history,
        offset: utc_offset(),
    })
}

#[derive(PartialEq)]
pub struct PowerPage;

impl Component for PowerPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("power", load);
        let error = use_state::<Option<String>>(|| None);

        let Some(model) = loaded.read().clone() else {
            return page_column().child(skeleton());
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        // Battery: no group on a machine without one.
        page = match model.battery.clone() {
            Ok(Some(battery)) => {
                let page = page.child(battery_group(&battery));
                match history_group(&battery) {
                    Some(group) => page.child(group),
                    None => page,
                }
            }
            Ok(None) => page,
            Err(e) => page.child(Group::titled("Battery").row(status(Status::Error(e.0)))),
        };

        // Power mode
        page = match model.profiles.clone() {
            Ok(profiles) => page.child(mode_group(loaded, error, &profiles)),
            Err(_) if !system::which("powerprofilesctl") => {
                page.child(Group::titled("Power mode").row(status(Status::Info(
                    "Power modes are not available on this computer.".into(),
                ))))
            }
            Err(e) => page.child(Group::titled("Power mode").row(status(Status::Error(e.0)))),
        };

        // Screen: blanking, locking and dimming
        let lock_on = matches!(model.lock_enabled, Ok(true));
        let mut screen = Group::titled("Screen");
        screen = match model.idle_delay.clone() {
            Ok(current) => screen.row(choice_row(
                "Blank screen after",
                timeout_choices(&[0, 60, 120, 300, 600, 900, 1800], current, "Never"),
                &current.to_string(),
                move |key: String| {
                    apply(loaded, error, move || {
                        system::gsettings_set(SESSION, "idle-delay", &format!("uint32 {key}"))
                    })
                },
            )),
            Err(e) => screen.row(row("Blank screen after").subtitle(e.0).enabled(false)),
        };
        if let Some(dim) = model.dim {
            screen = screen.row(switch_row(
                "Dim screen when inactive",
                dim,
                move |on: bool| {
                    let value = if on { "true" } else { "false" };
                    apply(loaded, error, move || {
                        system::gsettings_set(POWER_PLUGIN, "idle-dim", value)
                    })
                },
            ));
        }
        screen = match model.lock_enabled.clone() {
            Ok(enabled) => screen.row(switch_row(
                "Automatic screen lock",
                enabled,
                move |on: bool| {
                    let value = if on { "true" } else { "false" };
                    apply(loaded, error, move || {
                        system::gsettings_set(SCREENSAVER, "lock-enabled", value)
                    })
                },
            )),
            Err(e) => screen.row(row("Automatic screen lock").subtitle(e.0).enabled(false)),
        };
        screen = match model.lock_delay.clone() {
            Ok(current) => screen.row(
                choice_row(
                    "Lock after",
                    timeout_choices(&[0, 5, 30, 60, 300, 900], current, "Immediately"),
                    &current.to_string(),
                    move |key: String| {
                        apply(loaded, error, move || {
                            system::gsettings_set(
                                SCREENSAVER,
                                "lock-delay",
                                &format!("uint32 {key}"),
                            )
                        })
                    },
                )
                .enabled(lock_on),
            ),
            Err(e) => screen.row(row("Lock after").subtitle(e.0).enabled(false)),
        };
        page = page.child(screen);

        // Automatic suspend, one group per power source, only when the keys exist.
        if let Some(state) = &model.suspend_ac {
            page = page.child(suspend_rows(loaded, error, "ac", "When plugged in", state));
        }
        if let Some(state) = &model.suspend_battery {
            page = page.child(suspend_rows(loaded, error, "battery", "On battery", state));
        }

        // Power button, only when the schema exists
        if let Some(current) = model.power_button.clone() {
            let mut options: Vec<(String, String)> = [
                ("nothing", "Do nothing"),
                ("suspend", "Suspend"),
                ("hibernate", "Hibernate"),
                ("interactive", "Show power off dialog"),
                ("shutdown", "Power off"),
            ]
            .into_iter()
            .map(|(key, label)| (key.to_string(), label.to_string()))
            .collect();
            if !options.iter().any(|(key, _)| *key == current) {
                options.push((current.clone(), sentence(&current)));
            }
            page = page.child(Group::titled("Power button").row(choice_row(
                "When the power button is pressed",
                options,
                &current,
                move |key: String| {
                    apply(loaded, error, move || {
                        system::gsettings_set(
                            POWER_PLUGIN,
                            "power-button-action",
                            &format!("'{key}'"),
                        )
                    })
                },
            )));
        }

        page
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("power", load);
}
