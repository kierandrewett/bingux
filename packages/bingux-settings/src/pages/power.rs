//! Power: battery state, power mode, screen blanking and locking, automatic
//! suspend and the power button action. Everything is read in the background
//! when the page opens and reloaded after each successful change.

use std::collections::BTreeMap;
use std::time::Duration;

use bingux_ui::runtime::{optimistic, optimistic_then};
use bingux_ui::style::*;
use freya::prelude::*;

use crate::icons::Icon;
use crate::system::{self, CommandError, CommandResult, background};
use crate::ui::typography::{CAPTION, EMPHASIS};
use crate::ui::{
    Curve, Group, Overlays, PageHeader, Series, Status, Unit, choice_row, icon, line_chart,
    page_column, ring, row, skeleton, status, switch_row, use_overlays, use_page_header,
    use_tokens, vstack,
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
    /// Full charge capacity against the design capacity, as `93%`. UPower
    /// reports `capacity` for most batteries; `None` hides the row.
    health: Option<String>,
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
    /// Whether `powerprofilesctl` is installed, checked off the UI thread.
    profiles_tool: bool,
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
        profiles_tool: system::which("powerprofilesctl"),
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
    let mut health = None;
    for line in info.lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "percentage" => percentage = Some(value.to_string()),
            "capacity" => health = parse_health(value),
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
        health,
    }))
}

/// Capacity such as `93.4%` as a whole-number health label, `93%`.
fn parse_health(text: &str) -> Option<String> {
    let value: f64 = text.trim().strip_suffix('%')?.trim().parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| format!("{}%", value.round() as i64))
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

/// The header line: the charge and its state, `Plugged in` without a battery,
/// and nothing while the battery cannot be read.
fn header_subtitle(battery: &Result<Option<Battery>, CommandError>) -> Option<String> {
    match battery {
        Ok(Some(battery)) if battery.percentage.is_empty() => Some(battery.state.to_lowercase()),
        Ok(Some(battery)) => Some(format!(
            "{}, {}",
            battery.percentage,
            battery.state.to_lowercase()
        )),
        Ok(None) => Some("Plugged in".to_string()),
        Err(_) => None,
    }
}

/// Choices shown at once, by key. A value here wins over the loaded one until
/// the page is left, as switches do.
type Picks = State<BTreeMap<String, String>>;

fn picked(picks: Picks, key: &str, loaded: &str) -> String {
    picks
        .read()
        .get(key)
        .cloned()
        .unwrap_or_else(|| loaded.to_string())
}

/// Show a choice at once and run its command in the background. A failure
/// reverts the choice and shows the error as a toast; success reloads the
/// model.
fn choose(
    picks: Picks,
    loaded: State<Option<Model>>,
    overlays: Overlays,
    key: &str,
    value: String,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    let previous = picks.read().clone();
    let mut next = previous.clone();
    next.insert(key.to_string(), value);
    optimistic_then(
        picks,
        Some(overlays),
        previous,
        next,
        move || work().map(|_| ()),
        move || background(loaded, load),
    );
}

/// The three power modes as cards with icons, side by side. A mode the
/// computer does not offer stays visible but cannot be chosen.
fn mode_group(
    loaded: State<Option<Model>>,
    picks: Picks,
    overlays: Overlays,
    profiles: &Profiles,
) -> Group {
    let tokens = use_tokens();
    let mut cards = rect().tw(&tokens, &[flex_row(), w_full(), gap(S2)]);
    let chosen = picked(picks, "profile", &profiles.active);
    for name in ["power-saver", "balanced", "performance"] {
        let offered = profiles.choices.iter().any(|(key, _)| key == name);
        let active = chosen == name;
        let key = name.to_string();
        let state = if !offered {
            "Not available"
        } else if active {
            "In use"
        } else {
            ""
        };
        let mut card = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(profile_label(name))
            .a11y_focusable(offered)
            .tw(
                &tokens,
                &[
                    flex_col(),
                    w_flex(),
                    gap(S2),
                    items(Align::Center),
                    p(S3),
                    rounded(Radius::Card),
                    bg(Paint::Control),
                    border(Line::Ring(if active {
                        Paint::Accent
                    } else {
                        Paint::Control
                    })),
                ],
            )
            .child(icon(
                profile_icon(name),
                IconSize::Hero,
                Paint::Text.color(&tokens),
            ))
            .child(EMPHASIS.label(&tokens, profile_label(name)))
            .child(CAPTION.label(&tokens, profile_meaning(name)));
        if !state.is_empty() {
            card = card.child(CAPTION.label(&tokens, state));
        }
        if offered && !active {
            card = card.on_press(move |_| {
                let key = key.clone();
                choose(picks, loaded, overlays, "profile", key.clone(), move || {
                    system::run("powerprofilesctl", &["set", &key], Duration::from_secs(5))
                });
            });
        }
        cards = cards.child(card);
    }
    Group::titled("Power mode").row(cards)
}

/// Switches shown at once: a value here wins over the loaded one until the
/// page is left, so a switch never waits on its command.
type Pending = State<BTreeMap<String, bool>>;

fn shown(pending: Pending, key: &str, loaded: bool) -> bool {
    pending.read().get(key).copied().unwrap_or(loaded)
}

/// Flip a switch at once and run its command in the background. A failure
/// reverts the switch and shows the error as a toast.
fn set_switch(
    pending: Pending,
    overlays: Overlays,
    key: &str,
    on: bool,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    let previous = pending.read().clone();
    let mut value = previous.clone();
    value.insert(key.to_string(), on);
    optimistic(pending, Some(overlays), previous, value, move || {
        work().map(|_| ())
    });
}

/// A switch for automatic suspend on one power source. The delay is nested
/// under the switch and shown only while the switch is on.
fn suspend_rows(
    loaded: State<Option<Model>>,
    overlays: Overlays,
    pending: Pending,
    picks: Picks,
    source: &'static str,
    title: &str,
    state: &Suspend,
) -> Group {
    let key_type = format!("sleep-inactive-{source}-type");
    let key_timeout = format!("sleep-inactive-{source}-timeout");
    let pending_key = format!("suspend-{source}");
    let on = shown(pending, &pending_key, state.enabled);
    let toggle_key = key_type.clone();
    let toggle_pending_key = pending_key.clone();
    let delay_key = format!("suspend-delay-{source}");
    let delay = choice_row(
        "Suspend after",
        timeout_choices(SUSPEND_DELAYS, state.delay, "Never"),
        &picked(picks, &delay_key, &state.delay.to_string()),
        move |key: String| {
            let key_timeout = key_timeout.clone();
            choose(
                picks,
                loaded,
                overlays,
                &delay_key,
                key.clone(),
                move || system::gsettings_set(POWER_PLUGIN, &key_timeout, &key),
            );
        },
    );
    let toggle = switch_row("Suspend when inactive", on, move |on: bool| {
        let value = if on { "'suspend'" } else { "'nothing'" };
        let key = toggle_key.clone();
        set_switch(pending, overlays, &toggle_pending_key, on, move || {
            system::gsettings_set(POWER_PLUGIN, &key, value)
        });
    })
    .nested(on, vec![delay.into()]);
    Group::titled(title).row(toggle)
}

/// The charge as a centred ring with the state under the percentage, then the
/// time left. The ring is 120 px tall, so it sits in a block of its own
/// rather than inside a row, where it would overlap the rows around it.
/// A battery that reports no percentage gets plain rows instead.
fn battery_group(battery: &Battery) -> Group {
    let group = Group::titled("Battery");
    let group = match battery.fraction {
        Some(fraction) => group.row(
            vstack(S2)
                .tw_layout(&[w_full(), items(Align::Center), py(S2)])
                .child(
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
    let group = match &battery.time {
        Some((label, value)) => group.row(row(label.clone()).value(value.clone())),
        None => group,
    };
    match &battery.health {
        Some(health) => group.row(
            row("Battery health")
                .leading(Icon::BatteryLevel50)
                .subtitle("Full charge capacity compared with when the battery was new")
                .value(health.clone()),
        ),
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
        health: Some("94%".to_string()),
    })
}

#[derive(PartialEq)]
pub struct PowerPage;

impl Component for PowerPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("power", load);
        let overlays = use_overlays();
        let pending: Pending = use_state(BTreeMap::new);
        let picks: Picks = use_state(BTreeMap::new);
        let subtitle = loaded
            .read()
            .as_ref()
            .and_then(|model| header_subtitle(&model.battery));
        use_page_header(PageHeader {
            subtitle,
            action: None,
        });

        let Some(model) = loaded.read().clone() else {
            return page_column().child(skeleton());
        };

        let mut page = page_column();

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
            Ok(profiles) => page.child(mode_group(loaded, picks, overlays, &profiles)),
            Err(_) if !model.profiles_tool => page.child(Group::titled("Power mode").row(status(
                Status::Info("Power modes are not available on this computer.".into()),
            ))),
            Err(e) => page.child(Group::titled("Power mode").row(status(Status::Error(e.0)))),
        };

        // Screen: blanking, locking and dimming
        let mut screen = Group::titled("Screen");
        screen = match model.idle_delay.clone() {
            Ok(current) => screen.row(choice_row(
                "Blank screen after",
                timeout_choices(&[0, 60, 120, 300, 600, 900, 1800], current, "Never"),
                &picked(picks, "idle-delay", &current.to_string()),
                move |key: String| {
                    choose(
                        picks,
                        loaded,
                        overlays,
                        "idle-delay",
                        key.clone(),
                        move || {
                            system::gsettings_set(SESSION, "idle-delay", &format!("uint32 {key}"))
                        },
                    );
                },
            )),
            Err(e) => screen.row(row("Blank screen after").subtitle(e.0).enabled(false)),
        };
        if let Some(dim) = model.dim {
            let on = shown(pending, "dim", dim);
            screen = screen.row(switch_row(
                "Dim screen when inactive",
                on,
                move |on: bool| {
                    let value = if on { "true" } else { "false" };
                    set_switch(pending, overlays, "dim", on, move || {
                        system::gsettings_set(POWER_PLUGIN, "idle-dim", value)
                    })
                },
            ));
        }
        screen = match model.lock_enabled.clone() {
            Ok(enabled) => {
                let on = shown(pending, "lock", enabled);
                // The delay means nothing while the lock is off, so it is
                // nested under the switch and shown only while it is on.
                let delay = match model.lock_delay.clone() {
                    Ok(current) => choice_row(
                        "Lock after",
                        timeout_choices(&[0, 5, 30, 60, 300, 900], current, "Immediately"),
                        &picked(picks, "lock-delay", &current.to_string()),
                        move |key: String| {
                            choose(
                                picks,
                                loaded,
                                overlays,
                                "lock-delay",
                                key.clone(),
                                move || {
                                    system::gsettings_set(
                                        SCREENSAVER,
                                        "lock-delay",
                                        &format!("uint32 {key}"),
                                    )
                                },
                            );
                        },
                    )
                    .into(),
                    Err(e) => row("Lock after").subtitle(e.0).enabled(false).into(),
                };
                screen.row(
                    switch_row("Automatic screen lock", on, move |on: bool| {
                        let value = if on { "true" } else { "false" };
                        set_switch(pending, overlays, "lock", on, move || {
                            system::gsettings_set(SCREENSAVER, "lock-enabled", value)
                        })
                    })
                    .nested(on, vec![delay]),
                )
            }
            Err(e) => screen.row(row("Automatic screen lock").subtitle(e.0).enabled(false)),
        };
        page = page.child(screen);

        // Automatic suspend, one group per power source, only when the keys exist.
        if let Some(state) = &model.suspend_ac {
            page = page.child(suspend_rows(
                loaded,
                overlays,
                pending,
                picks,
                "ac",
                "When plugged in",
                state,
            ));
        }
        if let Some(state) = &model.suspend_battery {
            page = page.child(suspend_rows(
                loaded,
                overlays,
                pending,
                picks,
                "battery",
                "On battery",
                state,
            ));
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
                &picked(picks, "power-button", &current),
                move |key: String| {
                    choose(
                        picks,
                        loaded,
                        overlays,
                        "power-button",
                        key.clone(),
                        move || {
                            system::gsettings_set(
                                POWER_PLUGIN,
                                "power-button-action",
                                &format!("'{key}'"),
                            )
                        },
                    );
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
