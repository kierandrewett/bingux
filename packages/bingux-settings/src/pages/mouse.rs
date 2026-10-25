//! Mouse and touchpad preferences from `org.gnome.desktop.peripherals`.
//!
//! Gnoblin's Lua `input.mouse` and `input.touchpad` fields override these
//! keys, so a row whose field is set in the committed snapshot is shown
//! read-only. The Touchpad group is hidden when no touchpad is attached.
//! Snapshot builds can stand in a touchpad with `BINGUX_SETTINGS_FAKE_TOUCHPAD`.

use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::pages::gnoblin::{self, Json};
use crate::pages::loading::{has_touchpad_hint, veiled};
use crate::system::{self, CommandError, CommandResult, background, optimistic_then};
use crate::theme::Tokens;
use crate::ui::typography::{BODY, EMPHASIS};
use crate::ui::{
    Group, Overlays, PageHead, Row, Status, caption, choice_row, level_row, page_column, row, status,
    switch_row, use_overlays, use_tokens,
};
use bingux_ui::style::*;

const MOUSE: &str = "org.gnome.desktop.peripherals.mouse";

const TOUCHPAD: &str = "org.gnome.desktop.peripherals.touchpad";

const INTERFACE: &str = "org.gnome.desktop.interface";

/// The range of the double-click time slider, in milliseconds.
const DOUBLE_CLICK_MIN: u32 = 100;
const DOUBLE_CLICK_MAX: u32 = 1000;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Mouse {
    natural_scroll: bool,
    speed: f64,
    left_handed: bool,
    accel_profile: String,
    double_click: u32,
    primary_paste: bool,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Touchpad {
    tap_to_click: bool,
    natural_scroll: bool,
    two_finger_scrolling: bool,
    edge_scrolling: bool,
    click_method: String,
    speed: f64,
    disable_while_typing: bool,
    send_events: String,
    accel_profile: String,
    left_handed: String,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

fn get_u32(schema: &str, key: &str) -> Result<u32, CommandError> {
    let raw = system::gsettings_get(schema, key)?;
    raw.split_whitespace()
        .last()
        .and_then(|word| word.parse::<u32>().ok())
        .ok_or_else(|| CommandError(format!("The {key} setting could not be read.")))
}

/// A touchpad standing in for hardware. Snapshot builds only:
/// `BINGUX_SETTINGS_FAKE_TOUCHPAD` shows the Touchpad group without a device.
#[cfg(feature = "snapshot")]
fn fake_touchpad() -> Option<Touchpad> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_TOUCHPAD")?;
    Some(Touchpad {
        tap_to_click: true,
        natural_scroll: true,
        two_finger_scrolling: true,
        edge_scrolling: false,
        click_method: "fingers".to_string(),
        speed: 0.0,
        disable_while_typing: true,
        send_events: "enabled".to_string(),
        accel_profile: "default".to_string(),
        left_handed: "mouse".to_string(),
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_touchpad() -> Option<Touchpad> {
    None
}

fn load_touchpad() -> Result<Option<Touchpad>, CommandError> {
    if let Some(pad) = fake_touchpad() {
        return Ok(Some(pad));
    }
    if !gnoblin::has_touchpad() {
        return Ok(None);
    }
    Ok(Some(Touchpad {
        tap_to_click: get_bool(TOUCHPAD, "tap-to-click")?,
        natural_scroll: get_bool(TOUCHPAD, "natural-scroll")?,
        two_finger_scrolling: get_bool(TOUCHPAD, "two-finger-scrolling-enabled")?,
        edge_scrolling: get_bool(TOUCHPAD, "edge-scrolling-enabled")?,
        click_method: system::unquote(&system::gsettings_get(TOUCHPAD, "click-method")?),
        speed: get_speed(TOUCHPAD)?,
        disable_while_typing: get_bool(TOUCHPAD, "disable-while-typing")?,
        send_events: system::unquote(&system::gsettings_get(TOUCHPAD, "send-events")?),
        accel_profile: system::unquote(&system::gsettings_get(TOUCHPAD, "accel-profile")?),
        left_handed: system::unquote(&system::gsettings_get(TOUCHPAD, "left-handed")?),
    }))
}

fn load() -> Result<Model, CommandError> {
    let mouse = Mouse {
        natural_scroll: get_bool(MOUSE, "natural-scroll")?,
        speed: get_speed(MOUSE)?,
        left_handed: get_bool(MOUSE, "left-handed")?,
        accel_profile: system::unquote(&system::gsettings_get(MOUSE, "accel-profile")?),
        double_click: get_u32(MOUSE, "double-click")?,
        primary_paste: get_bool(INTERFACE, "gtk-enable-primary-paste")?,
    };
    Ok(Model {
        mouse,
        touchpad: load_touchpad()?,
        overrides: gnoblin::overrides(),
    })
}

/// The page as it shows before its first read lands: GNOME's defaults in the
/// real layout, so the rows keep their places when the values arrive. A
/// touchpad is shown when one is attached or stood in for.
fn placeholder() -> Model {
    Model {
        mouse: Mouse {
            natural_scroll: false,
            speed: -0.071,
            left_handed: false,
            accel_profile: "default".to_string(),
            double_click: 400,
            primary_paste: true,
        },
        touchpad: fake_touchpad().or_else(|| {
            has_touchpad_hint().then(|| Touchpad {
                tap_to_click: true,
                natural_scroll: true,
                two_finger_scrolling: true,
                edge_scrolling: false,
                click_method: "fingers".to_string(),
                speed: 0.0,
                disable_while_typing: false,
                send_events: "enabled".to_string(),
                accel_profile: "default".to_string(),
                left_handed: "mouse".to_string(),
            })
        }),
        overrides: Json::Null,
    }
}

/// Loads the page again and saves the result for the next start.
fn refresh() -> Result<Model, CommandError> {
    system::load_persistent("mouse", load)
}

/// Shows `change` on the model at once, writes it in the background and
/// reloads the model once the write has landed. A failed write puts the
/// previous model back with a toast.
fn commit(
    loaded: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
    change: impl FnOnce(&mut Model),
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    let previous = loaded.read().clone();
    let Some(Ok(current)) = previous.clone() else {
        return;
    };
    let mut value = current;
    change(&mut value);
    optimistic_then(
        loaded,
        Some(overlays),
        previous,
        Some(Ok(value)),
        move || work().map(|_| ()),
        move || background(loaded, refresh),
    );
}

/// Writes several touchpad keys in order, stopping at the first failure.
fn set_touchpad(writes: &[(&str, &str)]) -> CommandResult {
    for (key, value) in writes {
        system::gsettings_set(TOUCHPAD, key, value)?;
    }
    Ok(String::new())
}

/// Runs `change` on the touchpad in `model`, when one is attached.
fn with_touchpad(model: &mut Model, change: impl FnOnce(&mut Touchpad)) {
    if let Some(pad) = model.touchpad.as_mut() {
        change(pad);
    }
}

/// Speed is -1..1 in GSettings and 0..100 on the slider.
fn speed_percent(speed: f64) -> f64 {
    ((speed + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0)
}

fn speed_from_percent(percent: f64) -> f64 {
    (percent / 100.0 * 2.0 - 1.0).clamp(-1.0, 1.0)
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

/// The double-click time slider runs from the shortest to the longest time
/// in milliseconds.
fn double_click_percent(ms: u32) -> f64 {
    let ms = ms.clamp(DOUBLE_CLICK_MIN, DOUBLE_CLICK_MAX);
    f64::from(ms - DOUBLE_CLICK_MIN) / f64::from(DOUBLE_CLICK_MAX - DOUBLE_CLICK_MIN) * 100.0
}

fn double_click_from_percent(percent: f64) -> u32 {
    let span = f64::from(DOUBLE_CLICK_MAX - DOUBLE_CLICK_MIN);
    let ms = f64::from(DOUBLE_CLICK_MIN) + percent.clamp(0.0, 100.0) / 100.0 * span;
    ms.round() as u32
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

/// One illustrated choice: its label, how its picture is drawn, whether it
/// is the current value, the change it makes to the touchpad, and the
/// touchpad keys it writes.
struct MethodCard {
    label: &'static str,
    picture: fn(&Tokens) -> Element,
    selected: bool,
    apply: fn(&mut Touchpad),
    writes: &'static [(&'static str, &'static str)],
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
    overlays: Overlays,
    tokens: Tokens,
}

impl Setting<'_> {
    fn lua(&self) -> Option<String> {
        gnoblin::lua_override(self.overrides, &["input", self.group, self.key])
    }

    fn switch(&self, on: bool, apply: fn(&mut Model, bool)) -> Row {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(if value == "true" { "On" } else { "Off" }),
            None => {
                let (schema, key, loaded, overlays) =
                    (self.schema, self.key, self.loaded, self.overlays);
                switch_row(self.title, on, move |next: bool| {
                    commit(
                        loaded,
                        overlays,
                        move |model| apply(model, next),
                        move || system::gsettings_set(schema, key, bool_value(next)),
                    )
                })
            }
        }
    }

    /// A pointer speed slider. The value text names the current speed in
    /// words.
    fn speed(&self, speed: f64, apply: fn(&mut Model, f64)) -> Element {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(value)
                .into(),
            None => {
                let (schema, loaded, overlays) = (self.schema, self.loaded, self.overlays);
                level_row(self.title, speed_percent(speed), move |percent: f64| {
                    let next = speed_from_percent(percent);
                    commit(
                        loaded,
                        overlays,
                        move |model| apply(model, next),
                        move || system::gsettings_set(schema, "speed", &format!("{next:.3}")),
                    )
                })
                .value_text(speed_label(speed))
                .into()
            }
        }
    }

    /// The double-click time as a slider, shown in milliseconds.
    fn double_click(&self, ms: u32, apply: fn(&mut Model, u32)) -> Element {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(format!("{value} ms"))
                .into(),
            None => {
                let (loaded, overlays) = (self.loaded, self.overlays);
                level_row(self.title, double_click_percent(ms), move |percent: f64| {
                    let next = double_click_from_percent(percent);
                    commit(
                        loaded,
                        overlays,
                        move |model| apply(model, next),
                        move || system::gsettings_set(MOUSE, "double-click", &next.to_string()),
                    )
                })
                .value_text(format!("{ms} ms"))
                .into()
            }
        }
    }

    fn choice(
        &self,
        options: &[(&str, &str)],
        selected: &str,
        apply: fn(&mut Model, String),
    ) -> Row {
        match self.lua() {
            Some(value) => row(self.title)
                .subtitle(gnoblin::MANAGED)
                .value(option_label(options, &value)),
            None => {
                let (schema, key, loaded, overlays) =
                    (self.schema, self.key, self.loaded, self.overlays);
                choice_row(
                    self.title,
                    options
                        .iter()
                        .map(|(k, label)| (k.to_string(), label.to_string()))
                        .collect(),
                    selected,
                    move |choice: String| {
                        let quoted = format!("'{choice}'");
                        commit(
                            loaded,
                            overlays,
                            move |model| apply(model, choice),
                            move || system::gsettings_set(schema, key, &quoted),
                        )
                    },
                )
            }
        }
    }

    /// A choice drawn as illustrated cards, with a caption naming the current
    /// value when no card is chosen. A Lua override on any of `keys` replaces
    /// the cards with a managed row.
    fn cards(
        &self,
        title: &str,
        keys: &[&str],
        current: String,
        cards: Vec<MethodCard>,
    ) -> Element {
        let overridden = keys
            .iter()
            .find_map(|key| gnoblin::lua_override(self.overrides, &["input", self.group, *key]));
        if let Some(value) = overridden {
            return row(title).subtitle(gnoblin::MANAGED).value(value).into();
        }
        let tokens = self.tokens;
        let (loaded, overlays) = (self.loaded, self.overlays);
        let any_selected = cards.iter().any(|card| card.selected);
        let mut grid = rect().tw(&tokens, &[flex_row(), wrap(S4), gap(S4), w_full()]);
        for card in cards {
            let (apply, writes) = (card.apply, card.writes);
            grid = grid.child(method_card(&tokens, card, move |_| {
                commit(
                    loaded,
                    overlays,
                    move |model| with_touchpad(model, apply),
                    move || set_touchpad(writes),
                )
            }));
        }
        let mut column = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S3), px(S4), py(S3)])
            .child(EMPHASIS.label(&tokens, title))
            .child(grid);
        if !any_selected {
            column = column.child(caption(format!("Set to {current}.")));
        }
        column.into()
    }
}

/// One illustrated card: a small picture in a frame, ringed in the accent
/// colour when chosen, with its name under it. Mirrors the colour scheme
/// cards on the Appearance page.
fn method_card(
    tokens: &Tokens,
    card: MethodCard,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let ring = if card.selected {
        Paint::Accent
    } else {
        Paint::Outline
    };
    let pad = rect()
        .tw(
            tokens,
            &[
                w_full(),
                h_full(),
                rounded(Radius::Control),
                bg(Paint::Control),
                clip(),
            ],
        )
        .child((card.picture)(tokens));
    let frame = rect()
        .tw(
            tokens,
            &[
                w_full(),
                h(Dim::PreviewHeight),
                rounded(Radius::Card),
                border(Line::Ring(ring)),
                p(S0_5),
            ],
        )
        .child(pad);
    let label_style = if card.selected { EMPHASIS } else { BODY };
    rect()
        .a11y_role(AccessibilityRole::Button)
        .a11y_alt(card.label.to_string())
        .a11y_focusable(true)
        .tw(
            tokens,
            &[
                w(Dim::PreviewWidth),
                flex_col(),
                gap(S2),
                items(Align::Center),
            ],
        )
        .on_press(on_press)
        .child(frame)
        .child(label_style.label(tokens, card.label))
        .into()
}

/// A small dot standing for a finger touch.
fn dot(tokens: &Tokens) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w(Dim::Box5),
                h(Dim::Box5),
                rounded(Radius::Pill),
                bg(Paint::Muted),
            ],
        )
        .into()
}

/// Two fingers together in the middle of the pad.
fn picture_fingers(tokens: &Tokens) -> Element {
    rect()
        .tw(tokens, &[flex_row(), w_full(), h_full(), center(), gap(S2)])
        .child(dot(tokens))
        .child(dot(tokens))
        .into()
}

/// Two touches in the bottom corners of the pad.
fn picture_areas(tokens: &Tokens) -> Element {
    rect()
        .tw(
            tokens,
            &[
                flex_row(),
                w_full(),
                h_full(),
                justify(Align::Between),
                items(Align::End),
                p(S2),
            ],
        )
        .child(dot(tokens))
        .child(dot(tokens))
        .into()
}

/// Two fingers one above the other, for scrolling with two fingers.
fn picture_two_finger(tokens: &Tokens) -> Element {
    rect()
        .tw(tokens, &[flex_col(), w_full(), h_full(), center(), gap(S2)])
        .child(dot(tokens))
        .child(dot(tokens))
        .into()
}

/// A strip along the right edge of the pad, for scrolling along the edge.
fn picture_edge(tokens: &Tokens) -> Element {
    rect()
        .tw(
            tokens,
            &[flex_row(), w_full(), h_full(), justify(Align::End)],
        )
        .child(rect().tw(tokens, &[w(Dim::Box5), h_full(), bg(Paint::Muted)]))
        .into()
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

/// Touchpad primary button choices. `mouse` maps the touchpad's buttons like a
/// mouse's, with the same primary button.
const PRIMARY_BUTTON: [(&str, &str); 3] =
    [("right", "Right"), ("left", "Left"), ("mouse", "Mouse")];

/// How a touchpad click is told apart: by fingers, by the bottom corners, or
/// not at all.
const CLICK_METHOD: [(&str, &str); 4] = [
    ("default", "Automatic"),
    ("fingers", "Two fingers"),
    ("areas", "Bottom corners"),
    ("none", "Off"),
];

/// A box that answers a double-click made within the configured double-click
/// time. It checks the setting without changing it.
#[derive(PartialEq)]
struct ClickTest {
    within_ms: u32,
}

impl Component for ClickTest {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut last = use_state::<Option<Instant>>(|| None);
        let mut hit = use_state::<bool>(|| false);
        let window = Duration::from_millis(u64::from(self.within_ms));
        let on_press = move |_: Event<PressEventData>| {
            let now = Instant::now();
            let previous = *last.read();
            let double = previous.is_some_and(|at| now.duration_since(at) <= window);
            if double {
                last.set(None);
                let on = *hit.read();
                hit.set(!on);
            } else {
                last.set(Some(now));
            }
        };
        let on = *hit.read();
        let (text, fill) = if on {
            ("Double-click detected", Paint::AccentSoft)
        } else {
            ("Double-click this box", Paint::Control)
        };
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(text.to_string())
            .a11y_focusable(true)
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::RowTall),
                    center(),
                    rounded(Radius::Card),
                    bg(fill),
                    border(Line::Hairline),
                ],
            )
            .on_press(on_press)
            .child(EMPHASIS.label(&tokens, text))
    }
}

/// A scroll area with enough lines to scroll. Scrolling it checks the
/// scrolling settings.
#[derive(PartialEq)]
struct ScrollTest;

impl Component for ScrollTest {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut lines = rect().tw(&tokens, &[flex_col(), w_full(), p(S3), gap(S2)]);
        for n in 1..=20 {
            lines = lines.child(BODY.label(&tokens, format!("Line {n}")));
        }
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::PreviewHeight),
                    rounded(Radius::Card),
                    border(Line::Hairline),
                    clip(),
                ],
            )
            .child(ScrollView::new().drag_scrolling(false).child(lines))
    }
}

#[derive(PartialEq)]
pub struct MousePage;

impl Component for MousePage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached_persistent("mouse", load);
        let overlays = use_overlays();
        let tokens = use_tokens();

        let result = loaded.read().clone();
        let loading = result.is_none();
        let model = match result.unwrap_or_else(|| Ok(placeholder())) {
            Ok(model) => model,
            Err(e) => {
                return page_column().child(PageHead::new()).child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(loaded, refresh)),
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
            overlays,
            tokens,
        };
        let mouse = &model.mouse;
        let mouse_group = Group::titled("Mouse")
            .row(
                setting(MOUSE, "mouse", "natural-scroll", "Natural scrolling")
                    .switch(mouse.natural_scroll, |model, on| {
                        model.mouse.natural_scroll = on
                    }),
            )
            .row(
                setting(MOUSE, "mouse", "speed", "Pointer speed")
                    .speed(mouse.speed, |model, speed| model.mouse.speed = speed),
            )
            .row(
                setting(MOUSE, "mouse", "left-handed", "Left-handed buttons")
                    .switch(mouse.left_handed, |model, on| model.mouse.left_handed = on),
            )
            .row(
                setting(MOUSE, "mouse", "accel-profile", "Pointer acceleration").choice(
                    &ACCEL_PROFILES,
                    &mouse.accel_profile,
                    |model, value| model.mouse.accel_profile = value,
                ),
            )
            .row(
                setting(MOUSE, "mouse", "double-click", "Double-click speed")
                    .double_click(mouse.double_click, |model, ms| {
                        model.mouse.double_click = ms
                    }),
            )
            .row(
                setting(
                    INTERFACE,
                    "mouse",
                    "gtk-enable-primary-paste",
                    "Middle click paste",
                )
                .switch(mouse.primary_paste, |model, on| {
                    model.mouse.primary_paste = on
                }),
            );

        let touchpad_group = model.touchpad.as_ref().map(|pad| {
            let touchpad = |key, title| setting(TOUCHPAD, "touchpad", key, title);
            let send_events = touchpad("send-events", "Use touchpad");
            let effective = send_events.lua().unwrap_or_else(|| pad.send_events.clone());
            let group = Group::titled("Touchpad").row(send_events.choice(
                &SEND_EVENTS,
                &pad.send_events,
                |model, value| with_touchpad(model, |tp| tp.send_events = value),
            ));
            // With the touchpad off, its other settings have no effect, so
            // they are not shown.
            if effective == "disabled" {
                return group;
            }
            let click_cards = vec![
                MethodCard {
                    label: "Bottom corners",
                    picture: picture_areas,
                    selected: pad.click_method == "areas",
                    apply: |tp| tp.click_method = "areas".to_string(),
                    writes: &[("click-method", "'areas'")],
                },
                MethodCard {
                    label: "Two fingers",
                    picture: picture_fingers,
                    selected: pad.click_method == "fingers",
                    apply: |tp| tp.click_method = "fingers".to_string(),
                    writes: &[("click-method", "'fingers'")],
                },
            ];
            let scroll_cards = vec![
                MethodCard {
                    label: "Two-finger",
                    picture: picture_two_finger,
                    selected: pad.two_finger_scrolling && !pad.edge_scrolling,
                    apply: |tp| {
                        tp.two_finger_scrolling = true;
                        tp.edge_scrolling = false;
                    },
                    writes: &[
                        ("two-finger-scrolling-enabled", "true"),
                        ("edge-scrolling-enabled", "false"),
                    ],
                },
                MethodCard {
                    label: "Edge",
                    picture: picture_edge,
                    selected: pad.edge_scrolling && !pad.two_finger_scrolling,
                    apply: |tp| {
                        tp.two_finger_scrolling = false;
                        tp.edge_scrolling = true;
                    },
                    writes: &[
                        ("two-finger-scrolling-enabled", "false"),
                        ("edge-scrolling-enabled", "true"),
                    ],
                },
            ];
            group
                .row(
                    touchpad("tap-to-click", "Tap to click")
                        .switch(pad.tap_to_click, |model, on| {
                            with_touchpad(model, |tp| tp.tap_to_click = on)
                        }),
                )
                .row(
                    touchpad("natural-scroll", "Natural scrolling")
                        .switch(pad.natural_scroll, |model, on| {
                            with_touchpad(model, |tp| tp.natural_scroll = on)
                        }),
                )
                .row(touchpad("click-method", "Secondary click").cards(
                    "Secondary click",
                    &["click-method"],
                    option_label(&CLICK_METHOD, &pad.click_method),
                    click_cards,
                ))
                .row(touchpad("two-finger-scrolling-enabled", "Scrolling").cards(
                    "Scrolling",
                    &["two-finger-scrolling-enabled", "edge-scrolling-enabled"],
                    "Off".to_string(),
                    scroll_cards,
                ))
                .row(
                    touchpad("speed", "Pointer speed").speed(pad.speed, |model, speed| {
                        with_touchpad(model, |tp| tp.speed = speed)
                    }),
                )
                .row(touchpad("accel-profile", "Pointer acceleration").choice(
                    &ACCEL_PROFILES,
                    &pad.accel_profile,
                    |model, value| with_touchpad(model, |tp| tp.accel_profile = value),
                ))
                .row(touchpad("left-handed", "Primary button").choice(
                    &PRIMARY_BUTTON,
                    &pad.left_handed,
                    |model, value| with_touchpad(model, |tp| tp.left_handed = value),
                ))
                .row(
                    touchpad("disable-while-typing", "Disable while typing")
                        .switch(pad.disable_while_typing, |model, on| {
                            with_touchpad(model, |tp| tp.disable_while_typing = on)
                        }),
                )
        });

        let test_group = Group::titled("Test your settings")
            .description(
                "Double-click the box to test double-click speed. Scroll the list to test scrolling.",
            )
            .row(ClickTest { within_ms: mouse.double_click }.into_element())
            .row(ScrollTest.into_element());

        let page = page_column().child(PageHead::new())
            .child(mouse_group)
            .maybe_child(touchpad_group)
            .child(test_group);
        veiled(page, loading)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("mouse", load);
}
