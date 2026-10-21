//! Power: battery state, power profile, blank-screen and lock timeouts, and
//! the power button action. Everything is read in the background when the
//! page opens and reloaded after each successful change.

use std::time::Duration;

use freya::prelude::*;

use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{Group, Status, choice_row, page_column, row, status, switch_row};

const SESSION: &str = "org.gnome.desktop.session";
const SCREENSAVER: &str = "org.gnome.desktop.screensaver";
const POWER_PLUGIN: &str = "org.gnome.settings-daemon.plugins.power";

#[derive(Clone, PartialEq)]
struct Battery {
    percentage: String,
    state: String,
    time: Option<(String, String)>,
}

#[derive(Clone, PartialEq)]
struct Profiles {
    choices: Vec<(String, String)>,
    active: String,
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
    }
}

fn battery() -> Result<Option<Battery>, CommandError> {
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
            "state" => state = Some(sentence(value)),
            "time to empty" => time = Some(("Time to empty".to_string(), value.to_string())),
            "time to full" => time = Some(("Time to full".to_string(), value.to_string())),
            _ => {}
        }
    }

    Ok(Some(Battery {
        percentage: percentage.unwrap_or_else(|| "Unknown".to_string()),
        state: state.unwrap_or_else(|| "Unknown".to_string()),
        time,
    }))
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
    Ok(Profiles { choices, active })
}

fn profile_label(name: &str) -> String {
    match name {
        "performance" => "Performance".to_string(),
        "balanced" => "Balanced".to_string(),
        "power-saver" => "Power saver".to_string(),
        other => other.to_string(),
    }
}

fn gsettings_u32(schema: &str, key: &str) -> Result<u32, CommandError> {
    let raw = system::gsettings_get(schema, key)?;
    raw.trim()
        .strip_prefix("uint32 ")
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| CommandError(format!("Unexpected {key} value: {raw}")))
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

#[derive(PartialEq)]
pub struct PowerPage;

impl Component for PowerPage {
    fn render(&self) -> impl IntoElement {
        let loaded = use_state::<Option<Model>>(|| None);
        let error = use_state::<Option<String>>(|| None);
        use_hook(move || background(loaded, load));

        let Some(model) = loaded.read().clone() else {
            return page_column().child(status(Status::Loading("Reading power settings.".into())));
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        // Battery
        page = match model.battery.clone() {
            Ok(Some(battery)) => {
                let mut group = Group::titled("Battery")
                    .row(row("Charge").value(battery.percentage))
                    .row(row("State").value(battery.state));
                if let Some((label, value)) = battery.time {
                    group = group.row(row(label).value(value));
                }
                page.child(group)
            }
            Ok(None) => {
                page.child(Group::titled("Battery").row(status(Status::Empty("No battery".into()))))
            }
            Err(e) => page.child(Group::titled("Battery").row(status(Status::Error(e.0)))),
        };

        // Power mode
        page = match model.profiles.clone() {
            Ok(profiles) => {
                let active = profiles.active.clone();
                page.child(Group::titled("Power mode").row(choice_row(
                    "Profile",
                    profiles.choices,
                    &active,
                    move |key: String| {
                        apply(loaded, error, move || {
                            system::run("powerprofilesctl", &["set", &key], Duration::from_secs(5))
                        })
                    },
                )))
            }
            Err(e) => {
                let kind = if e.0.contains("not installed") {
                    Status::Info(e.0)
                } else {
                    Status::Error(e.0)
                };
                page.child(Group::titled("Power mode").row(status(kind)))
            }
        };

        // Power saving
        let lock_on = matches!(model.lock_enabled, Ok(true));
        let mut saving = Group::titled("Power saving");
        saving = match model.idle_delay.clone() {
            Ok(current) => saving.row(choice_row(
                "Blank screen after",
                timeout_choices(&[0, 60, 120, 300, 600, 900, 1800], current, "Never"),
                &current.to_string(),
                move |key: String| {
                    apply(loaded, error, move || {
                        system::gsettings_set(SESSION, "idle-delay", &format!("uint32 {key}"))
                    })
                },
            )),
            Err(e) => saving.row(row("Blank screen after").subtitle(e.0).enabled(false)),
        };
        saving = match model.lock_enabled.clone() {
            Ok(enabled) => saving.row(switch_row(
                "Automatic screen lock",
                enabled,
                move |on: bool| {
                    let value = if on { "true" } else { "false" };
                    apply(loaded, error, move || {
                        system::gsettings_set(SCREENSAVER, "lock-enabled", value)
                    })
                },
            )),
            Err(e) => saving.row(row("Automatic screen lock").subtitle(e.0).enabled(false)),
        };
        saving = match model.lock_delay.clone() {
            Ok(current) => saving.row(
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
            Err(e) => saving.row(row("Lock after").subtitle(e.0).enabled(false)),
        };
        page = page.child(saving);

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
                options.push((current.clone(), current.clone()));
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
