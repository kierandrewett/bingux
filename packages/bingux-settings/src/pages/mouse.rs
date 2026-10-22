//! Mouse and touchpad preferences from `org.gnome.desktop.peripherals`.
//!
//! Gnoblin's Lua `input.mouse` and `input.touchpad` fields override these
//! keys, so a row whose field is set in the committed snapshot is shown
//! read-only. The Touchpad group is hidden when no touchpad is attached.

use freya::prelude::*;

use crate::pages::gnoblin::{self, Json};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Row, Status, choice_row, page_column, row, skeleton, slider, status, switch_row,
};

const MOUSE: &str = "org.gnome.desktop.peripherals.mouse";
const TOUCHPAD: &str = "org.gnome.desktop.peripherals.touchpad";

#[derive(Clone, PartialEq)]
struct Mouse {
    natural_scroll: bool,
    speed: f64,
    left_handed: bool,
    accel_profile: String,
}

#[derive(Clone, PartialEq)]
struct Touchpad {
    tap_to_click: bool,
    natural_scroll: bool,
    two_finger_scrolling: bool,
    speed: f64,
    disable_while_typing: bool,
    send_events: String,
}

#[derive(Clone, PartialEq)]
struct Model {
    mouse: Mouse,
    touchpad: Option<Touchpad>,
    overrides: Json,
}

fn get_bool(schema: &str, key: &str) -> Result<bool, CommandError> {
    Ok(system::gsettings_get(schema, key)? == "true")
}

fn get_speed(schema: &str) -> Result<f64, CommandError> {
    let raw = system::gsettings_get(schema, "speed")?;
    raw.trim()
        .parse::<f64>()
        .map_err(|_| CommandError("The pointer speed could not be read.".to_string()))
}

fn load() -> Result<Model, CommandError> {
    let mouse = Mouse {
        natural_scroll: get_bool(MOUSE, "natural-scroll")?,
        speed: get_speed(MOUSE)?,
        left_handed: get_bool(MOUSE, "left-handed")?,
        accel_profile: system::unquote(&system::gsettings_get(MOUSE, "accel-profile")?),
    };
    let touchpad = if gnoblin::has_touchpad() {
        Some(Touchpad {
            tap_to_click: get_bool(TOUCHPAD, "tap-to-click")?,
            natural_scroll: get_bool(TOUCHPAD, "natural-scroll")?,
            two_finger_scrolling: get_bool(TOUCHPAD, "two-finger-scrolling-enabled")?,
            speed: get_speed(TOUCHPAD)?,
            disable_while_typing: get_bool(TOUCHPAD, "disable-while-typing")?,
            send_events: system::unquote(&system::gsettings_get(TOUCHPAD, "send-events")?),
        })
    } else {
        None
    };
    Ok(Model {
        mouse,
        touchpad,
        overrides: gnoblin::overrides(),
    })
}

fn commit(
    loaded: State<Option<Result<Model, CommandError>>>,
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

/// Speed is -1..1 in GSettings and 0..100 on the slider.
fn speed_percent(speed: f64) -> f64 {
    ((speed + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0)
}

fn speed_value(percent: f64) -> String {
    format!("{:.3}", (percent / 100.0 * 2.0 - 1.0).clamp(-1.0, 1.0))
}

/// The pointer speed as shown to the user: a word, not the -1 to 1 value.
fn speed_label(speed: f64) -> String {
    match speed_percent(speed) {
        percent if percent < 25.0 => "Slow",
        percent if percent < 45.0 => "Slower than normal",
        percent if percent <= 55.0 => "Normal",
        percent if percent <= 75.0 => "Faster than normal",
        _ => "Fast",
    }
    .to_string()
}

fn bool_value(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// The label for a stored option value, or the value itself when unknown.
fn option_label(options: &[(&str, &str)], value: &str) -> String {
    options
        .iter()
        .find(|(key, _)| *key == value)
        .map_or_else(|| value.to_string(), |(_, label)| label.to_string())
}

/// One editable setting: the GSettings key, how to show it, and the Lua
/// field that may override it.
struct Setting<'a> {
    schema: &'static str,
    key: &'static str,
    group: &'static str,
    title: &'static str,
    overrides: &'a Json,
    loaded: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
}

impl Setting<'_> {
    fn lua(&self) -> Option<String> {
        gnoblin::lua_override(self.overrides, &["input", self.group, self.key])
    }

    fn switch(&self, on: bool) -> Row {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(if value == "true" { "On" } else { "Off" }),
            None => {
                let (schema, key, loaded, error) = (self.schema, self.key, self.loaded, self.error);
                switch_row(self.title, on, move |next: bool| {
                    commit(loaded, error, move || {
                        system::gsettings_set(schema, key, bool_value(next))
                    })
                })
            }
        }
    }

    fn speed(&self, speed: f64) -> Row {
        match self.lua() {
            Some(value) => row(self.title).subtitle(gnoblin::MANAGED).value(value),
            None => {
                let (schema, loaded, error) = (self.schema, self.loaded, self.error);
                row(self.title)
                    .subtitle(speed_label(speed))
                    .trailing(slider(speed_percent(speed), move |percent: f64| {
                        let value = speed_value(percent);
                        commit(loaded, error, move || {
                            system::gsettings_set(schema, "speed", &value)
                        })
                    }))
            }
        }
    }

    fn choice(&self, options: &[(&str, &str)], selected: &str) -> Row {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(option_label(options, &value)),
            None => {
                let (schema, key, loaded, error) = (self.schema, self.key, self.loaded, self.error);
                choice_row(
                    self.title,
                    options
                        .iter()
                        .map(|(k, label)| (k.to_string(), label.to_string()))
                        .collect(),
                    selected,
                    move |choice: String| {
                        let quoted = format!("'{choice}'");
                        commit(loaded, error, move || {
                            system::gsettings_set(schema, key, &quoted)
                        })
                    },
                )
            }
        }
    }
}

const ACCEL_PROFILES: [(&str, &str); 3] = [
    ("default", "Default"),
    ("flat", "Flat"),
    ("adaptive", "Adaptive"),
];
const SEND_EVENTS: [(&str, &str); 3] = [
    ("enabled", "Always on"),
    (
        "disabled-on-external-mouse",
        "Off when a mouse is connected",
    ),
    ("disabled", "Off"),
];

#[derive(PartialEq)]
pub struct MousePage;

impl Component for MousePage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("mouse", load);
        let error = use_state::<Option<String>>(|| None);

        let Some(result) = loaded.read().clone() else {
            return page_column().child(skeleton());
        };
        let model = match result {
            Ok(model) => model,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(loaded, load)),
                );
            }
        };
        let setting = |schema, group, key, title| Setting {
            schema,
            key,
            group,
            title,
            overrides: &model.overrides,
            loaded,
            error,
        };
        let mouse = &model.mouse;
        let mouse_group = Group::titled("Mouse")
            .row(
                setting(MOUSE, "mouse", "natural-scroll", "Natural scrolling")
                    .switch(mouse.natural_scroll),
            )
            .row(setting(MOUSE, "mouse", "speed", "Pointer speed").speed(mouse.speed))
            .row(
                setting(MOUSE, "mouse", "left-handed", "Left-handed buttons")
                    .switch(mouse.left_handed),
            )
            .row(
                setting(MOUSE, "mouse", "accel-profile", "Pointer acceleration")
                    .choice(&ACCEL_PROFILES, &mouse.accel_profile),
            );

        let touchpad_group = model.touchpad.as_ref().map(|pad| {
            Group::titled("Touchpad")
                .row(
                    setting(TOUCHPAD, "touchpad", "send-events", "Use touchpad")
                        .choice(&SEND_EVENTS, &pad.send_events),
                )
                .row(
                    setting(TOUCHPAD, "touchpad", "tap-to-click", "Tap to click")
                        .switch(pad.tap_to_click),
                )
                .row(
                    setting(TOUCHPAD, "touchpad", "natural-scroll", "Natural scrolling")
                        .switch(pad.natural_scroll),
                )
                .row(
                    setting(
                        TOUCHPAD,
                        "touchpad",
                        "two-finger-scrolling-enabled",
                        "Two-finger scrolling",
                    )
                    .switch(pad.two_finger_scrolling),
                )
                .row(setting(TOUCHPAD, "touchpad", "speed", "Pointer speed").speed(pad.speed))
                .row(
                    setting(
                        TOUCHPAD,
                        "touchpad",
                        "disable-while-typing",
                        "Disable while typing",
                    )
                    .switch(pad.disable_while_typing),
                )
        });

        page_column()
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
            .child(mouse_group)
            .maybe_child(touchpad_group)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("mouse", load);
}
