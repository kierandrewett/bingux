//! Desktop: the Bingux shell's dock, sidebar, top bar, Control Centre tiles
//! and public IP lookup. Everything is read from `bingux-config dump` and
//! written through `bingux-config set|unset|save`; the shell file is the
//! source of truth, so the model is reloaded after every successful write.

use std::time::Duration;

use crate::ui::{
    Group, Icon, Overlays, Row, Status, dropdown, level_row, page_column, row, segmented, skeleton,
    status, switch_row, use_overlays, use_tokens,
};
use bingux_ui::style::*;
use freya::prelude::*;

use crate::pages::bingux_config;
use crate::system::{self, CommandError, background, background_then, optimistic_then};

pub(crate) mod preview;

use preview::desktop_preview;

/// Control Centre tiles: the camelCase key in `dump`, its label and its icon.
const TILES: [(&str, &str, Icon); 5] = [
    ("vpn", "VPN", Icon::NetworkVpn),
    ("dnd", "Do not disturb", Icon::NotificationsDisabled),
    ("nightLight", "Night light", Icon::NightLight),
    ("power", "Power mode", Icon::PowerProfileBalanced),
    ("awake", "Keep awake", Icon::ChangesPrevent),
];
/// What the shell shows when `controlCentre` is absent (`ControlCentreServices.qml`).
const TILE_DEFAULTS: [bool; 5] = [true, true, false, false, false];
const UNREADABLE: &str = "Could not read the desktop settings.";

const DOCK_SIZE_MIN: f64 = 32.0;
const DOCK_SIZE_RANGE: f64 = 48.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Model {
    dock: bool,
    sidebar: bool,
    /// `None` is "Automatic": the key is null or absent in the shell file.
    sidebar_edge: Option<String>,
    /// Absolute path of the wallpaper, or empty for none.
    wallpaper: String,
    metrics: bool,
    clock_seconds: bool,
    dock_size: u32,
    dock_alignment: String,
    dock_click: String,
    dock_middle_click: String,
    dock_scroll: String,
    dock_scroll_direction: String,
    tiles: [bool; 5],
    public_ip_lookup: bool,
}

impl Model {
    fn from_json(root: &Json) -> Result<Self, String> {
        let data = root.get("data").ok_or(UNREADABLE)?;
        let desktop = data.get("desktop").ok_or(UNREADABLE)?;
        let privacy = data.get("privacy").ok_or(UNREADABLE)?;
        // A fresh config has no `controlCentre`; the shell then shows its
        // defaults, which are the `TILES` defaults here.
        let mut tiles = TILE_DEFAULTS;
        if let Some(control) = desktop
            .get("controlCentre")
            .filter(|value| !matches!(value, Json::Null))
        {
            for (slot, (key, _, _)) in tiles.iter_mut().zip(TILES) {
                *slot = flag(control, key)?;
            }
        }
        Ok(Self {
            dock: flag(desktop, "dock")?,
            sidebar: flag(desktop, "sidebar")?,
            sidebar_edge: desktop
                .get("sidebarEdge")
                .and_then(Json::as_str)
                .map(str::to_string),
            metrics: flag(desktop, "metrics")?,
            clock_seconds: flag(desktop, "clockSeconds")?,
            dock_size: number(desktop, "dockSize")?.round() as u32,
            wallpaper: desktop
                .get("wallpaper")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            dock_alignment: text(desktop, "dockAlignment")?,
            dock_click: text(desktop, "dockClick")?,
            dock_middle_click: text(desktop, "dockMiddleClick")?,
            dock_scroll: text(desktop, "dockScroll")?,
            dock_scroll_direction: text(desktop, "dockScrollDirection")?,
            tiles,
            public_ip_lookup: flag(privacy, "publicIpLookup")?,
        })
    }
}

fn flag(object: &Json, key: &str) -> Result<bool, String> {
    object
        .get(key)
        .and_then(Json::as_bool)
        .ok_or_else(|| UNREADABLE.to_string())
}

fn text(object: &Json, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| UNREADABLE.to_string())
}

fn number(object: &Json, key: &str) -> Result<f64, String> {
    object
        .get(key)
        .and_then(Json::as_number)
        .ok_or_else(|| UNREADABLE.to_string())
}

/// The `save` patch for all Control Centre tiles. `save` takes every tile, so
/// the patch always carries all five.
fn tiles_patch(tiles: [bool; 5]) -> String {
    let fields = TILES
        .iter()
        .enumerate()
        .map(|(i, (key, _, _))| format!("\"{key}\":{}", tiles[i]))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"desktop\":{{\"controlCentre\":{{{fields}}}}}}}")
}

/// The dock slider works in percent (0 to 100) and maps onto 32 to 80 px.
fn percent_for_size(size: u32) -> f64 {
    (size as f64 - DOCK_SIZE_MIN) / DOCK_SIZE_RANGE * 100.0
}

fn size_for_percent(percent: f64) -> u32 {
    (DOCK_SIZE_MIN + percent.clamp(0.0, 100.0) / 100.0 * DOCK_SIZE_RANGE).round() as u32
}

pub(crate) fn load() -> Result<Model, CommandError> {
    let text = system::run(&bingux_config(), &["dump"], Duration::from_secs(5))?;
    let json = Json::parse(&text).map_err(|_| CommandError(UNREADABLE.to_string()))?;
    Model::from_json(&json).map_err(CommandError)
}

#[derive(Clone, Debug, PartialEq)]
enum Change {
    Set(&'static str, String),
    Unset(&'static str),
    Tiles([bool; 5]),
}

impl Model {
    /// Applies `change` to the model the page shows, before the shell confirms it.
    fn apply_local(&mut self, change: &Change) {
        match change {
            Change::Set(key, value) => match *key {
                "desktop.dock" => self.dock = value == "true",
                "desktop.sidebar" => self.sidebar = value == "true",
                "desktop.metrics" => self.metrics = value == "true",
                "desktop.clock_seconds" => self.clock_seconds = value == "true",
                "desktop.dock_size" => self.dock_size = value.parse().unwrap_or(self.dock_size),
                "desktop.dock_alignment" => self.dock_alignment = value.clone(),
                "desktop.dock_click" => self.dock_click = value.clone(),
                "desktop.dock_middle_click" => self.dock_middle_click = value.clone(),
                "desktop.dock_scroll" => self.dock_scroll = value.clone(),
                "desktop.dock_scroll_direction" => self.dock_scroll_direction = value.clone(),
                "desktop.sidebar_edge" => self.sidebar_edge = Some(value.clone()),
                "privacy.public_ip_lookup" => self.public_ip_lookup = value == "true",
                _ => {}
            },
            Change::Unset("desktop.sidebar_edge") => self.sidebar_edge = None,
            Change::Unset(_) => {}
            Change::Tiles(tiles) => self.tiles = *tiles,
        }
    }
}

fn apply(change: &Change) -> Result<(), CommandError> {
    let program = bingux_config();
    let timeout = Duration::from_secs(5);
    let result = match change {
        Change::Set(key, value) => system::run(&program, &["set", *key, value.as_str()], timeout),
        Change::Unset(key) => system::run(&program, &["unset", *key], timeout),
        Change::Tiles(tiles) => {
            let patch = tiles_patch(*tiles);
            system::run_with_stdin(&program, &["save"], Some(patch.as_str()), timeout)
        }
    };
    result.map(|_| ())
}

#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
    /// The dock slider's percent while it is dragged, until the write lands.
    pending: State<Option<f64>>,
}

/// Shows `change` at once, writes it in the background and reloads the model
/// from the shell once the write has landed. A failed write puts the previous
/// model back with a toast.
fn write(ctx: Ctx, change: Change) {
    let previous = ctx.model.read().clone();
    let Some(Ok(current)) = previous.clone() else {
        return;
    };
    let mut value = current;
    value.apply_local(&change);
    let mut pending = ctx.pending;
    pending.set(None);
    let mut model = ctx.model;
    optimistic_then(
        ctx.model,
        Some(ctx.overlays),
        previous,
        Some(Ok(value)),
        move || apply(&change),
        move || background_then(load, move |loaded| model.set(Some(loaded))),
    );
}

/// Commit the dragged dock size once the pointer is released.
fn release_dock_size(ctx: Ctx, current: u32) {
    let dragged = *ctx.pending.read();
    let Some(percent) = dragged else {
        return;
    };
    let size = size_for_percent(percent);
    if size == current {
        let mut pending = ctx.pending;
        pending.set(None);
        return;
    }
    write(ctx, Change::Set("desktop.dock_size", size.to_string()));
}

fn dock_size_row(ctx: Ctx, size: u32) -> Element {
    let dragged = *ctx.pending.read();
    let percent = dragged.unwrap_or_else(|| percent_for_size(size));
    rect()
        .tw_layout(&[w_full()])
        .on_global_pointer_up(move |_: Event<PointerEventData>| release_dock_size(ctx, size))
        .child(
            level_row("Icon size", percent, move |value: f64| {
                let mut pending = ctx.pending;
                pending.set(Some(value));
            })
            .value_text(format!("{} px", size_for_percent(percent))),
        )
        .into()
}

/// Segmented control on the right, for two or three short options.
fn segment_row(
    title: &str,
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Row {
    row(title).trailing(segmented(options, selected, on_select))
}

/// Dropdown on the right at `Dim::Field`, the same width as the icon size
/// slider, so every control in the right column lines up.
fn menu_row(
    title: &str,
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Row {
    row(title).trailing(dropdown(options, selected, on_select).min_width(Dim::Field))
}

#[derive(PartialEq)]
pub struct DesktopPage;

impl Component for DesktopPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("desktop", load);
        let overlays = use_overlays();
        let pending = use_state::<Option<f64>>(|| None);
        let ctx = Ctx {
            model,
            overlays,
            pending,
        };
        let tokens = use_tokens();
        let searching = bingux_ui::search_highlight::current().is_some();

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let m = match loaded {
            Ok(model_value) => model_value,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| background(model, load)),
                );
            }
        };
        let edge = m.sidebar_edge.clone().unwrap_or_else(|| "auto".to_string());
        let mut dock =
            Group::titled("Dock").row(switch_row("Show dock", m.dock, move |on: bool| {
                write(ctx, Change::Set("desktop.dock", on.to_string()))
            }));
        if m.dock {
            dock = dock.row(dock_size_row(ctx, m.dock_size)).row(segment_row(
                "Alignment",
                vec![
                    ("left".into(), "Left".into()),
                    ("center".into(), "Centre".into()),
                    ("right".into(), "Right".into()),
                ],
                &m.dock_alignment,
                move |key: String| write(ctx, Change::Set("desktop.dock_alignment", key)),
            ));
        }
        let mut sidebar =
            Group::titled("Sidebar").row(switch_row("Show sidebar", m.sidebar, move |on: bool| {
                write(ctx, Change::Set("desktop.sidebar", on.to_string()))
            }));
        if m.sidebar {
            sidebar = sidebar.row(menu_row(
                "Side of the screen",
                vec![
                    ("auto".into(), "Automatic".into()),
                    ("left".into(), "Left".into()),
                    ("right".into(), "Right".into()),
                    ("top".into(), "Top".into()),
                ],
                &edge,
                move |key: String| {
                    if key == "auto" {
                        write(ctx, Change::Unset("desktop.sidebar_edge"))
                    } else {
                        write(ctx, Change::Set("desktop.sidebar_edge", key))
                    }
                },
            ));
        }
        let mut page = page_column();
        if !searching {
            page = page.child(desktop_preview(&tokens, ctx, &m));
        }
        page.child(dock)
            .child(
                Group::titled("Dock mouse actions")
                    .description(
                        "What happens when you click or scroll on an app icon in the dock.",
                    )
                    .row(menu_row(
                        "Click",
                        vec![
                            ("toggle".into(), "Show or hide windows".into()),
                            ("focus".into(), "Bring the window to the front".into()),
                            ("launch".into(), "Open a new window".into()),
                        ],
                        &m.dock_click,
                        move |key: String| write(ctx, Change::Set("desktop.dock_click", key)),
                    ))
                    .row(menu_row(
                        "Middle click",
                        vec![
                            ("launch".into(), "Open a new window".into()),
                            ("close".into(), "Close all its windows".into()),
                            ("none".into(), "Nothing".into()),
                        ],
                        &m.dock_middle_click,
                        move |key: String| {
                            write(ctx, Change::Set("desktop.dock_middle_click", key))
                        },
                    ))
                    .row(menu_row(
                        "Scroll",
                        vec![
                            ("cycle".into(), "Switch between windows".into()),
                            ("none".into(), "Nothing".into()),
                        ],
                        &m.dock_scroll,
                        move |key: String| write(ctx, Change::Set("desktop.dock_scroll", key)),
                    ))
                    .row(segment_row(
                        "Scroll direction",
                        vec![
                            ("natural".into(), "Like a touchscreen".into()),
                            ("reverse".into(), "Reversed".into()),
                        ],
                        &m.dock_scroll_direction,
                        move |key: String| {
                            write(ctx, Change::Set("desktop.dock_scroll_direction", key))
                        },
                    )),
            )
            .child(sidebar)
            .child(
                Group::titled("Top bar")
                    .row(
                        switch_row("Show system usage", m.metrics, move |on: bool| {
                            write(ctx, Change::Set("desktop.metrics", on.to_string()))
                        })
                        .subtitle("Shows how busy the processor, memory and network are."),
                    )
                    .row(switch_row(
                        "Show seconds in the clock",
                        m.clock_seconds,
                        move |on: bool| {
                            write(ctx, Change::Set("desktop.clock_seconds", on.to_string()))
                        },
                    )),
            )
            .child(
                Group::titled("Control Centre")
                    .description("Choose which tiles the Control Centre shows.")
                    .rows(TILES.iter().enumerate().map(|(index, (_, label, icon))| {
                        let tiles = m.tiles;
                        Element::from(
                            switch_row(*label, tiles[index], move |on: bool| {
                                let mut next = tiles;
                                next[index] = on;
                                write(ctx, Change::Tiles(next))
                            })
                            .leading(*icon),
                        )
                    })),
            )
            .child(
                Group::titled("Privacy").row(
                    switch_row(
                        "Show my public IP address",
                        m.public_ip_lookup,
                        move |on: bool| {
                            write(ctx, Change::Set("privacy.public_ip_lookup", on.to_string()))
                        },
                    )
                    .subtitle("Looks it up with an online service. Shown in the Control Centre.")
                    .leading(Icon::ViewConceal),
                ),
            )
    }
}

/// A minimal JSON reader for `bingux-config dump`, which the shell prints as
/// standard JSON. Covers objects, arrays, strings, numbers and literals.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let value = parser.value()?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err(format!("unexpected data after byte {}", parser.pos));
        }
        Ok(value)
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(value) => Some(value),
            _ => None,
        }
    }

    fn as_number(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("unexpected input at byte {}", self.pos)),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut fields = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at byte {}", self.pos));
            }
            self.pos += 1;
            fields.push((key, self.value()?));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(format!("unexpected input at byte {}", self.pos))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()
            .and_then(|digits| digits.parse::<f64>().ok())
            .map(Json::Number)
            .ok_or_else(|| format!("bad number at byte {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("expected a string at byte {}", self.pos));
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err("unterminated string".to_string());
            };
            self.pos += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err("unterminated escape".to_string());
                    };
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let code = self
                                .bytes
                                .get(self.pos..self.pos + 4)
                                .and_then(|hex| std::str::from_utf8(hex).ok())
                                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                                .ok_or_else(|| {
                                    format!("bad unicode escape at byte {}", self.pos)
                                })?;
                            self.pos += 4;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(format!("bad escape at byte {}", self.pos)),
                    }
                }
                _ => {
                    let start = self.pos - 1;
                    let mut end = self.pos;
                    while end < self.bytes.len() && !matches!(self.bytes[end], b'"' | b'\\') {
                        end += 1;
                    }
                    let chunk = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|error| error.to_string())?;
                    out.push_str(chunk);
                    self.pos = end;
                }
            }
        }
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("desktop", load);
}
