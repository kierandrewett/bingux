//! Displays: monitor layout, resolution, refresh rate, scale and orientation,
//! plus night light.
//!
//! The layout comes from Mutter's DisplayConfig over D-Bus, read and written
//! with `busctl`. When that service cannot be read, the page shows
//! `gnoblinctl`'s monitor list read-only. Night light lives in GSettings.

use std::time::Duration;

use bytes::Bytes;

use freya::animation::*;
use freya::prelude::*;

use crate::pages::gnoblin;
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    ARRANGEMENT_HEIGHT, ARRANGEMENT_WIDTH, Group, Icon, Overlays, RADIUS_LG, RADIUS_SM,
    ResponseKind, Status, Tokens, action_row, caption, choice_row, dialog, icon, icon_button,
    page_column, primary_button, row, skeleton, status, switch_row, typography, use_overlays,
    use_tokens,
};
use bingux_ui::scale::{
    ARRANGEMENT_SNAP, FIELD_WIDTH, ICON_SM, RING_WIDTH, S0, S1, S2, S4, S5, button_height,
};

const SERVICE: &str = "org.gnome.Mutter.DisplayConfig";
const OBJECT: &str = "/org/gnome/Mutter/DisplayConfig";

/// How long a display takes to ease into its settled place after a drag.
const DRAG_SETTLE_MS: u64 = 120;
const INTERFACE: &str = "org.gnome.Mutter.DisplayConfig";
/// `ApplyMonitorsConfig(serial, method, logical monitors, properties)`.
const APPLY_SIGNATURE: &str = "uua(iiduba(ssa{sv}))a{sv}";
/// Method 2 stores the configuration persistently, method 1 is temporary.
const METHOD_PERSISTENT: &str = "2";
/// A trial applies the layout for this session only. Nothing is saved.
const METHOD_TEMPORARY: &str = "1";
/// Seconds the user has to keep a trial layout before it reverts.
const KEEP_SECONDS: u64 = 15;
const COLOR: &str = "org.gnome.settings-daemon.plugins.color";
const TEMP_MIN: u32 = 1700;
const TEMP_MAX: u32 = 4700;
const TIMEOUT: Duration = Duration::from_secs(5);
const TEMP_DEBOUNCE: Duration = Duration::from_millis(300);
const SCALE_EPSILON: f64 = 1e-3;

#[derive(PartialEq)]
pub struct DisplaysPage;

/// A minimal JSON value. `busctl --json=short` and `gnoblinctl --json` are the
/// only inputs, so this covers exactly what they emit.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(name, _)| name.as_str() == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn index(&self, position: usize) -> Option<&Json> {
        match self {
            Json::Array(items) => items.get(position),
            _ => None,
        }
    }

    fn items(&self) -> &[Json] {
        match self {
            Json::Array(items) => items,
            _ => &[],
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Json::Number(number) => Some(*number),
            _ => None,
        }
    }

    fn boolean(&self) -> Option<bool> {
        match self {
            Json::Bool(flag) => Some(*flag),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

fn parse_json(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    let value = parser.value()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(format!("unexpected text at byte {}", parser.pos));
    }
    Ok(value)
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!(
                "expected '{}' at byte {}",
                char::from(byte),
                self.pos
            ))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Text),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("unexpected character at byte {}", self.pos)),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            fields.push((key, self.value()?));
            self.skip_ws();
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
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
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

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let byte = self.peek().ok_or("unterminated string")?;
            self.pos += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(out).map_err(|_| "invalid UTF-8 in string".into());
                }
                b'\\' => {
                    let escape = self.peek().ok_or("unterminated escape")?;
                    self.pos += 1;
                    let ch = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let bytes = self.bytes;
                            let hex = bytes
                                .get(self.pos..self.pos + 4)
                                .ok_or("short unicode escape")?;
                            self.pos += 4;
                            let code = std::str::from_utf8(hex)
                                .ok()
                                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                                .ok_or("bad unicode escape")?;
                            char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
                        }
                        _ => return Err("bad string escape".into()),
                    };
                    let mut buffer = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                }
                _ => out.push(byte),
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|e| e.to_string())?;
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("bad number at byte {start}"))
    }
}

/// One mode Mutter offers for a monitor.
#[derive(Clone, PartialEq)]
struct Mode {
    id: String,
    width: u32,
    height: u32,
    refresh: f64,
    scales: Vec<f64>,
    preferred: bool,
}

impl Mode {
    fn resolution(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    fn refresh_label(&self) -> String {
        let preferred = if self.preferred { " (recommended)" } else { "" };
        format!("{} Hz{preferred}", trim_number(self.refresh))
    }
}

/// A monitor and its draft settings. Edits change these fields; Apply sends
/// them to Mutter.
#[derive(Clone, PartialEq)]
struct Monitor {
    connector: String,
    name: String,
    modes: Vec<Mode>,
    /// Index into `modes` of the mode to apply.
    mode: usize,
    enabled: bool,
    primary: bool,
    x: i32,
    y: i32,
    scale: f64,
    transform: u32,
}

impl Monitor {
    fn selected(&self) -> &Mode {
        &self.modes[self.mode]
    }

    /// Logical size in pixels after scale and rotation.
    fn extent(&self) -> (i32, i32) {
        let mode = self.selected();
        let width = (f64::from(mode.width) / self.scale).round() as i32;
        let height = (f64::from(mode.height) / self.scale).round() as i32;
        if self.transform % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        }
    }

    fn resolutions(&self) -> Vec<(String, String)> {
        let mut seen: Vec<(u32, u32)> = Vec::new();
        for mode in &self.modes {
            if !seen.contains(&(mode.width, mode.height)) {
                seen.push((mode.width, mode.height));
            }
        }
        seen.into_iter()
            .map(|(width, height)| (format!("{width}x{height}"), format!("{width} × {height}")))
            .collect()
    }

    fn refresh_options(&self) -> Vec<(String, String)> {
        let current = self.selected();
        self.modes
            .iter()
            .filter(|mode| mode.width == current.width && mode.height == current.height)
            .map(|mode| (mode.id.clone(), mode.refresh_label()))
            .collect()
    }

    /// Keys are indexes into the selected mode's supported scales.
    fn scale_options(&self) -> Vec<(String, String)> {
        self.selected()
            .scales
            .iter()
            .enumerate()
            .map(|(index, scale)| {
                (
                    index.to_string(),
                    format!("{}%", (scale * 100.0).round() as u32),
                )
            })
            .collect()
    }

    fn scale_key(&self) -> String {
        self.selected()
            .scales
            .iter()
            .position(|scale| (scale - self.scale).abs() < SCALE_EPSILON)
            .map(|index| index.to_string())
            .unwrap_or_default()
    }

    fn orientation_options(&self) -> Vec<(String, String)> {
        let mut transforms: Vec<u32> = (0..4).collect();
        if !transforms.contains(&self.transform) {
            transforms.push(self.transform);
        }
        transforms
            .into_iter()
            .map(|transform| {
                (
                    transform.to_string(),
                    transform_label(transform).to_string(),
                )
            })
            .collect()
    }

    /// The best mode among `candidates`: same refresh rate if possible, then
    /// the preferred mode, then the fastest.
    fn best_mode(&self, candidates: &[usize]) -> Option<usize> {
        let refresh = self.selected().refresh;
        candidates
            .iter()
            .copied()
            .find(|&index| (self.modes[index].refresh - refresh).abs() < 0.01)
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|&index| self.modes[index].preferred)
            })
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .max_by(|&a, &b| self.modes[a].refresh.total_cmp(&self.modes[b].refresh))
            })
    }

    fn select_mode(&mut self, index: usize) {
        self.mode = index;
        let scale = self.scale;
        if !self.modes[index]
            .scales
            .iter()
            .any(|supported| (supported - scale).abs() < SCALE_EPSILON)
        {
            self.scale = default_scale(&self.modes[index]);
        }
    }
}

fn default_scale(mode: &Mode) -> f64 {
    mode.scales
        .iter()
        .copied()
        .find(|scale| (scale - 1.0).abs() < SCALE_EPSILON)
        .or_else(|| mode.scales.first().copied())
        .unwrap_or(1.0)
}

fn transform_label(transform: u32) -> &'static str {
    match transform {
        0 => "Normal",
        1 => "90°",
        2 => "180°",
        3 => "270°",
        4 => "Flipped",
        5 => "Flipped 90°",
        6 => "Flipped 180°",
        7 => "Flipped 270°",
        _ => "Unknown",
    }
}

fn orientation_name(value: &str) -> &'static str {
    match value {
        "normal" => "Normal",
        "90" => "90°",
        "180" => "180°",
        "270" => "270°",
        "flipped" => "Flipped",
        "flipped-90" => "Flipped 90°",
        "flipped-180" => "Flipped 180°",
        "flipped-270" => "Flipped 270°",
        _ => "Unknown",
    }
}

/// A short word for a colour temperature, for the night light slider.
fn temperature_word(kelvin: u32) -> &'static str {
    match kelvin {
        k if k < 2500 => "Very warm",
        k if k < 3500 => "Warm",
        k if k < 4200 => "Neutral",
        _ => "Cool",
    }
}

/// `1.5` -> `"1.5"`, `180.0` -> `"180"`, `59.94` -> `"59.94"`.
fn trim_number(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// A read-only monitor from `gnoblinctl`, with rows already formatted.
#[derive(Clone, PartialEq)]
struct Record {
    title: String,
    rows: Vec<(String, String)>,
}

#[derive(Clone, PartialEq)]
enum Displays {
    Editable {
        serial: u32,
        monitors: Vec<Monitor>,
    },
    ReadOnly {
        reason: String,
        records: Vec<Record>,
    },
}

#[derive(Clone, PartialEq)]
struct Night {
    enabled: bool,
    kelvin: u32,
    /// Whether the night light follows the schedule.
    automatic: bool,
    /// Schedule hours, from 0 to 24.
    from: f64,
    to: f64,
}

#[derive(Clone, PartialEq)]
struct Page {
    displays: Result<Displays, CommandError>,
    night: Result<Night, CommandError>,
}

fn load() -> Page {
    Page {
        displays: load_displays(),
        night: load_night(),
    }
}

/// Snapshot builds only. `BINGUX_SETTINGS_FAKE_DISPLAYS` swaps in a two-display
/// layout, so the arrangement can be checked on a machine with one monitor.
/// The layout is never sent to Mutter: `apply` refuses it.
#[cfg(feature = "snapshot")]
fn fake_layout() -> Option<Displays> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_DISPLAYS")?;
    let mode = |id: &str, width: u32, height: u32, refresh: f64, preferred: bool| Mode {
        id: id.to_string(),
        width,
        height,
        refresh,
        scales: vec![1.0, 1.25, 1.5, 2.0],
        preferred,
    };
    let monitor = |connector: &str, modes: Vec<Mode>, primary: bool, x: i32, y: i32| Monitor {
        connector: connector.to_string(),
        name: format!("Test display {connector}"),
        modes,
        mode: 0,
        enabled: true,
        primary,
        x,
        y,
        scale: 1.0,
        transform: 0,
    };
    Some(Displays::Editable {
        serial: 0,
        monitors: vec![
            monitor(
                "DP-1",
                vec![
                    mode("2560x1440@59.95", 2560, 1440, 59.95, true),
                    mode("1920x1080@60", 1920, 1080, 60.0, false),
                ],
                true,
                0,
                0,
            ),
            monitor(
                "HDMI-1",
                vec![mode("1920x1080@60", 1920, 1080, 60.0, true)],
                false,
                2560,
                180,
            ),
        ],
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_layout() -> Option<Displays> {
    None
}

fn load_displays() -> Result<Displays, CommandError> {
    if let Some(layout) = fake_layout() {
        return Ok(layout);
    }
    if let Ok((serial, monitors)) = mutter_call(true, "GetCurrentState", None, &[])
        .and_then(|text| parse_mutter(&text).map_err(CommandError))
    {
        return Ok(Displays::Editable { serial, monitors });
    }
    // The editable layout is not available here, so show the read-only list.
    match load_gnoblin() {
        Ok(records) => Ok(Displays::ReadOnly {
            reason: "Display settings cannot be changed right now.".to_string(),
            records,
        }),
        Err(_) => Err(CommandError("Display settings could not be read.".into())),
    }
}

/// Calls a DisplayConfig method through `busctl`. `values` are the arguments
/// for `signature`, one busctl word each.
fn mutter_call(
    json: bool,
    member: &str,
    signature: Option<&str>,
    values: &[String],
) -> CommandResult {
    let mut args = vec!["--user"];
    if json {
        args.push("--json=short");
    }
    // `--` keeps negative positions from being read as busctl options.
    args.extend(["call", "--", SERVICE, OBJECT, INTERFACE, member]);
    if let Some(signature) = signature {
        args.push(signature);
    }
    args.extend(values.iter().map(String::as_str));
    system::run("busctl", &args, TIMEOUT)
}

/// Reads serial, monitors and the logical layout from `GetCurrentState`.
fn parse_mutter(text: &str) -> Result<(u32, Vec<Monitor>), String> {
    let root = parse_json(text)?;
    let data = root.get("data").ok_or("DisplayConfig reply has no data")?;
    let serial = data
        .index(0)
        .and_then(Json::number)
        .ok_or("DisplayConfig reply has no serial")? as u32;
    let mut monitors = data
        .index(1)
        .ok_or("DisplayConfig reply has no monitors")?
        .items()
        .iter()
        .map(parse_monitor)
        .collect::<Result<Vec<_>, _>>()?;

    for logical in data
        .index(2)
        .ok_or("DisplayConfig reply has no logical monitors")?
        .items()
    {
        let connector = logical
            .index(5)
            .and_then(|list| list.items().first())
            .and_then(|spec| spec.index(0))
            .and_then(Json::text)
            .ok_or("logical monitor has no connector")?;
        let monitor = monitors
            .iter_mut()
            .find(|monitor| monitor.connector == connector)
            .ok_or_else(|| format!("logical monitor {connector} is not in the monitor list"))?;
        monitor.enabled = true;
        monitor.x = logical.index(0).and_then(Json::number).ok_or("no x")? as i32;
        monitor.y = logical.index(1).and_then(Json::number).ok_or("no y")? as i32;
        monitor.scale = logical.index(2).and_then(Json::number).ok_or("no scale")?;
        monitor.transform = logical
            .index(3)
            .and_then(Json::number)
            .ok_or("no transform")? as u32;
        monitor.primary = logical.index(4).and_then(Json::boolean).unwrap_or(false);
    }
    Ok((serial, monitors))
}

fn parse_monitor(json: &Json) -> Result<Monitor, String> {
    let spec = json.index(0).ok_or("monitor has no spec")?;
    let connector = spec
        .index(0)
        .and_then(Json::text)
        .ok_or("monitor has no connector")?
        .to_string();
    let display_name = json
        .index(2)
        .and_then(|props| props.get("display-name"))
        .and_then(|value| value.get("data"))
        .and_then(Json::text)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let name = display_name
        .or_else(|| {
            let vendor_product = [spec.index(1), spec.index(2)]
                .into_iter()
                .flatten()
                .filter_map(Json::text)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (!vendor_product.is_empty()).then_some(vendor_product)
        })
        .unwrap_or_else(|| connector.clone());

    let mut modes = Vec::new();
    let mut current = None;
    for (index, mode) in json
        .index(1)
        .ok_or("monitor has no modes")?
        .items()
        .iter()
        .enumerate()
    {
        let props = mode.index(6);
        let flag = |key: &str| {
            props
                .and_then(|props| props.get(key))
                .and_then(|value| value.get("data"))
                .and_then(Json::boolean)
                == Some(true)
        };
        if flag("is-current") {
            current = Some(index);
        }
        modes.push(Mode {
            id: mode
                .index(0)
                .and_then(Json::text)
                .ok_or("mode has no id")?
                .to_string(),
            width: mode
                .index(1)
                .and_then(Json::number)
                .ok_or("mode has no width")? as u32,
            height: mode
                .index(2)
                .and_then(Json::number)
                .ok_or("mode has no height")? as u32,
            refresh: mode
                .index(3)
                .and_then(Json::number)
                .ok_or("mode has no refresh")?,
            scales: mode.index(5).map_or(Vec::new(), |list| {
                list.items().iter().filter_map(Json::number).collect()
            }),
            preferred: flag("is-preferred"),
        });
    }
    if modes.is_empty() {
        return Err(format!("monitor {connector} has no modes"));
    }
    let selected = current
        .or_else(|| modes.iter().position(|mode| mode.preferred))
        .unwrap_or(0);
    let scale = default_scale(&modes[selected]);
    Ok(Monitor {
        connector,
        name,
        modes,
        mode: selected,
        enabled: false,
        primary: false,
        x: 0,
        y: 0,
        scale,
        transform: 0,
    })
}

fn load_gnoblin() -> Result<Vec<Record>, CommandError> {
    let text = gnoblin::gnoblinctl(&["--json", "monitor", "list"])?;
    let list = parse_json(&text).map_err(CommandError)?;
    list.items()
        .iter()
        .map(parse_record)
        .collect::<Result<Vec<_>, _>>()
        .map_err(CommandError)
}

fn parse_record(record: &Json) -> Result<Record, String> {
    let name = record
        .get("name")
        .and_then(Json::text)
        .ok_or("monitor record has no name")?;
    let make = record.get("make").and_then(Json::text).unwrap_or("");
    let model = record.get("model").and_then(Json::text).unwrap_or("");
    let title = [make, model]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let title = if title.is_empty() {
        name.to_string()
    } else {
        title
    };
    let mode = record
        .get("mode")
        .and_then(|mode| {
            let width = mode.get("width")?.number()?;
            let height = mode.get("height")?.number()?;
            let refresh = mode.get("refresh_rate")?.number()?;
            Some(format!(
                "{} × {} @ {} Hz",
                width as u32,
                height as u32,
                trim_number(refresh)
            ))
        })
        .unwrap_or_else(|| "Off".to_string());
    let position = match (
        record.get("x").and_then(Json::number),
        record.get("y").and_then(Json::number),
    ) {
        (Some(x), Some(y)) => format!("{}, {}", x as i64, y as i64),
        _ => "Unknown".to_string(),
    };
    let scale = record
        .get("scale")
        .and_then(Json::number)
        .map_or("Unknown".to_string(), |scale| {
            format!("{}%", (scale * 100.0).round() as u32)
        });
    let orientation =
        orientation_name(record.get("transform").and_then(Json::text).unwrap_or("")).to_string();
    let rows = [
        ("Port", name.to_string()),
        (
            "Enabled",
            yes_no(record.get("enabled").and_then(Json::boolean)),
        ),
        (
            "Primary",
            yes_no(record.get("primary").and_then(Json::boolean)),
        ),
        ("Mode", mode),
        ("Position", position),
        ("Scale", scale),
        ("Orientation", orientation),
    ]
    .into_iter()
    .map(|(label, value)| (label.to_string(), value))
    .collect();
    Ok(Record { title, rows })
}

fn yes_no(flag: Option<bool>) -> String {
    match flag {
        Some(true) => "Yes".to_string(),
        Some(false) => "No".to_string(),
        None => "Unknown".to_string(),
    }
}

fn load_night() -> Result<Night, CommandError> {
    let enabled = system::gsettings_get(COLOR, "night-light-enabled")?;
    let temperature = system::gsettings_get(COLOR, "night-light-temperature")?;
    let automatic = system::gsettings_get(COLOR, "night-light-schedule-automatic")?;
    let from = system::gsettings_get(COLOR, "night-light-schedule-from")?;
    let to = system::gsettings_get(COLOR, "night-light-schedule-to")?;
    let kelvin = temperature
        .split_whitespace()
        .last()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| CommandError("Night light settings could not be read.".into()))?;
    Ok(Night {
        enabled: enabled.trim() == "true",
        kelvin,
        automatic: automatic.trim() == "true",
        from: hours(&from)?,
        to: hours(&to)?,
    })
}

/// Reads a schedule hour such as `18.0`.
fn hours(text: &str) -> Result<f64, CommandError> {
    text.trim()
        .parse::<f64>()
        .map_err(|_| CommandError("Night light settings could not be read.".into()))
}

/// Maps 1700..4700 K onto the slider's 0..100.
fn kelvin_level(kelvin: u32) -> f64 {
    f64::from(kelvin.clamp(TEMP_MIN, TEMP_MAX) - TEMP_MIN) / f64::from(TEMP_MAX - TEMP_MIN) * 100.0
}

fn kelvin_for_level(level: f64) -> u32 {
    let level = level.clamp(0.0, 100.0);
    TEMP_MIN + ((level / 100.0) * f64::from(TEMP_MAX - TEMP_MIN)).round() as u32
}

fn reload(mut model: State<Option<Page>>) {
    model.set(None);
    background(model, load);
}

/// Changes the draft monitors and shows the error, if any. A rejected change
/// leaves the monitors as they were.
fn edit(
    mut model: State<Option<Page>>,
    mut error: State<Option<String>>,
    change: impl FnOnce(&mut [Monitor]) -> Result<(), String>,
) {
    let mut page = model.write();
    let Some(Page {
        displays: Ok(Displays::Editable { monitors, .. }),
        ..
    }) = page.as_mut()
    else {
        return;
    };
    match change(monitors.as_mut_slice()) {
        Ok(()) => error.set(None),
        Err(message) => error.set(Some(message)),
    }
}

fn overlaps(a: &Monitor, b: &Monitor) -> bool {
    let (a_width, a_height) = a.extent();
    let (b_width, b_height) = b.extent();
    a.x < b.x + b_width && b.x < a.x + a_width && a.y < b.y + b_height && b.y < a.y + a_height
}

/// Moves an enabled monitor right of the others when it overlaps one of them.
fn place(monitors: &mut [Monitor], index: usize) {
    if !monitors[index].enabled {
        return;
    }
    let clashes = monitors.iter().enumerate().any(|(other, monitor)| {
        other != index && monitor.enabled && overlaps(&monitors[index], monitor)
    });
    if !clashes {
        return;
    }
    let right = monitors
        .iter()
        .enumerate()
        .filter(|(other, monitor)| *other != index && monitor.enabled)
        .map(|(_, monitor)| monitor.x + monitor.extent().0)
        .max()
        .unwrap_or(0);
    monitors[index].x = right;
    monitors[index].y = 0;
}

fn set_enabled(monitors: &mut [Monitor], index: usize, on: bool) -> Result<(), String> {
    if monitors[index].enabled == on {
        return Ok(());
    }
    if on {
        monitors[index].enabled = true;
        place(monitors, index);
        if !monitors
            .iter()
            .any(|monitor| monitor.enabled && monitor.primary)
        {
            monitors[index].primary = true;
        }
        return Ok(());
    }
    if monitors.iter().filter(|monitor| monitor.enabled).count() <= 1 {
        return Err("Keep at least one display enabled.".to_string());
    }
    let was_primary = monitors[index].primary;
    monitors[index].enabled = false;
    monitors[index].primary = false;
    if was_primary {
        if let Some(next) = monitors.iter_mut().find(|monitor| monitor.enabled) {
            next.primary = true;
        }
    }
    Ok(())
}

fn set_primary(monitors: &mut [Monitor], index: usize) -> Result<(), String> {
    if !monitors[index].enabled {
        return Err("Enable this display before making it primary.".to_string());
    }
    for (position, monitor) in monitors.iter_mut().enumerate() {
        monitor.primary = position == index && monitor.enabled;
    }
    Ok(())
}

fn set_resolution(monitors: &mut [Monitor], index: usize, key: &str) -> Result<(), String> {
    let monitor = &mut monitors[index];
    let candidates: Vec<usize> = monitor
        .modes
        .iter()
        .enumerate()
        .filter(|(_, mode)| mode.resolution() == key)
        .map(|(position, _)| position)
        .collect();
    let best = monitor
        .best_mode(&candidates)
        .ok_or("No mode has that resolution.")?;
    monitor.select_mode(best);
    place(monitors, index);
    Ok(())
}

fn set_refresh(monitors: &mut [Monitor], index: usize, id: &str) -> Result<(), String> {
    let monitor = &mut monitors[index];
    let best = monitor
        .modes
        .iter()
        .position(|mode| mode.id == id)
        .ok_or("Unknown refresh rate.")?;
    monitor.select_mode(best);
    place(monitors, index);
    Ok(())
}

fn set_scale(monitors: &mut [Monitor], index: usize, key: &str) -> Result<(), String> {
    let monitor = &mut monitors[index];
    let position: usize = key.parse().map_err(|_| "Unknown scale.".to_string())?;
    let scale = *monitor
        .selected()
        .scales
        .get(position)
        .ok_or("Unknown scale.")?;
    monitor.scale = scale;
    place(monitors, index);
    Ok(())
}

fn set_transform(monitors: &mut [Monitor], index: usize, key: &str) -> Result<(), String> {
    let transform: u32 = key
        .parse()
        .map_err(|_| "Unknown orientation.".to_string())?;
    monitors[index].transform = transform;
    place(monitors, index);
    Ok(())
}

/// The orientation after a quarter turn. Flipped orientations (4 to 7) keep
/// their flip.
fn next_orientation(transform: u32) -> u32 {
    if transform >= 4 {
        4 + (transform - 3) % 4
    } else {
        (transform + 1) % 4
    }
}

/// The hint for the arrangement card: the live position while a display is
/// dragged, or how to arrange the displays.
fn arrangement_hint(ctx: Ctx, monitors: &[Monitor]) -> String {
    let enabled = monitors.iter().filter(|monitor| monitor.enabled).count();
    match *ctx.drag.read() {
        Some(drag) => format!(
            "Position {}, {}",
            monitors[drag.index].x, monitors[drag.index].y
        ),
        None if enabled >= 2 => "Drag displays to arrange them.".to_string(),
        None => String::new(),
    }
}

/// The height of the footer on the arrangement card: one button high, with a
/// gap of `S2` above it and below it.
fn arrangement_footer_height() -> f32 {
    button_height() + S2 * 2.0
}

/// The width of the hint text in the arrangement footer. A fixed width stops
/// the text from taking the row, which would push the quick controls off the card.
const ARRANGEMENT_HINT_WIDTH: f32 = 260.0;

/// The footer of the arrangement card: the hint on the left and the quick
/// controls for the selected display on the right, on one baseline.
fn arrangement_footer(ctx: Ctx, monitors: &[Monitor], selected: usize) -> Element {
    rect()
        .position(Position::new_absolute().left(S2).right(S2).bottom(S2))
        .width(Size::fill())
        .height(Size::px(button_height()))
        .direction(Direction::Horizontal)
        .main_align(Alignment::SpaceBetween)
        .cross_align(Alignment::Center)
        .child(
            rect()
                .width(Size::px(ARRANGEMENT_HINT_WIDTH))
                .child(caption(arrangement_hint(ctx, monitors))),
        )
        .maybe_child(canvas_controls(ctx, monitors, selected))
        .into()
}

/// The rotate and main display buttons, at the right of the arrangement footer.
/// Shown for the selected display when it is on.
fn canvas_controls(ctx: Ctx, monitors: &[Monitor], selected: usize) -> Option<Element> {
    let monitor = monitors.get(selected).filter(|monitor| monitor.enabled)?;
    let index = selected;
    let primary = monitor.primary;
    Some(
        rect()
            .width(Size::Inner)
            .direction(Direction::Horizontal)
            .spacing(S2)
            .child(
                icon_button(Icon::ObjectRotateRight, move |_| {
                    edit(ctx.model, ctx.error, move |monitors| {
                        let next = next_orientation(monitors[index].transform);
                        set_transform(monitors, index, &next.to_string())
                    });
                })
                .tooltip("Rotate"),
            )
            .child(
                icon_button(
                    if primary {
                        Icon::Starred
                    } else {
                        Icon::NonStarred
                    },
                    move |_| {
                        if !primary {
                            edit(ctx.model, ctx.error, move |monitors| {
                                set_primary(monitors, index)
                            });
                        }
                    },
                )
                .tooltip(if primary {
                    "Main display"
                } else {
                    "Use as main display"
                }),
            )
            .into(),
    )
}

/// The bounds, scale and origin of the enabled displays in a canvas of `area`
/// size, or `None` when none is on. The layout fits inside the canvas inset by
/// `S4` on every side and is centred there.
fn frame_of(monitors: &[Monitor], area: (f32, f32)) -> Option<Frame> {
    let enabled: Vec<&Monitor> = monitors.iter().filter(|monitor| monitor.enabled).collect();
    if enabled.is_empty() {
        return None;
    }
    let left = enabled.iter().map(|monitor| monitor.x).min().unwrap_or(0);
    let top = enabled.iter().map(|monitor| monitor.y).min().unwrap_or(0);
    let right = enabled
        .iter()
        .map(|monitor| monitor.x + monitor.extent().0)
        .max()
        .unwrap_or(left);
    let bottom = enabled
        .iter()
        .map(|monitor| monitor.y + monitor.extent().1)
        .max()
        .unwrap_or(top);
    let span_x = f64::from((right - left).max(1));
    let span_y = f64::from((bottom - top).max(1));
    let inner_x = f64::from(area.0 - 2.0 * S4).max(1.0);
    let inner_y = f64::from(area.1 - 2.0 * S4).max(1.0);
    let fit = (inner_x / span_x).min(inner_y / span_y);
    let origin = (
        S4 + ((inner_x - span_x * fit) / 2.0) as f32,
        S4 + ((inner_y - span_y * fit) / 2.0) as f32,
    );
    Some(Frame {
        left,
        top,
        fit,
        origin,
    })
}

/// Whether two enabled displays share an edge segment of at least one pixel.
fn touches(a: &Monitor, b: &Monitor) -> bool {
    let (a_width, a_height) = a.extent();
    let (b_width, b_height) = b.extent();
    let side_by_side = (a.x + a_width == b.x || b.x + b_width == a.x)
        && a.y < b.y + b_height
        && b.y < a.y + a_height;
    let stacked = (a.y + a_height == b.y || b.y + b_height == a.y)
        && a.x < b.x + b_width
        && b.x < a.x + a_width;
    side_by_side || stacked
}

/// Whether the enabled displays form one group that touches end to end. Mutter
/// needs a contiguous layout.
fn connected(monitors: &[Monitor]) -> bool {
    let enabled: Vec<usize> = (0..monitors.len())
        .filter(|&index| monitors[index].enabled)
        .collect();
    let Some(&start) = enabled.first() else {
        return true;
    };
    let mut seen = vec![false; monitors.len()];
    seen[start] = true;
    let mut stack = vec![start];
    while let Some(current) = stack.pop() {
        for &other in &enabled {
            if !seen[other] && touches(&monitors[current], &monitors[other]) {
                seen[other] = true;
                stack.push(other);
            }
        }
    }
    enabled.iter().all(|&index| seen[index])
}

/// The start, along one axis, of a display of length `own` that touches the
/// edge of an anchor starting at `anchor_start` with length `anchor_span`. A
/// start within `ARRANGEMENT_SNAP` of the anchor's start or end aligns with
/// it. Otherwise it is the nearest start that still overlaps the anchor by a
/// pixel.
fn touching_start(raw: i32, own: i32, anchor_start: i32, anchor_span: i32) -> i32 {
    let aligned_start = anchor_start;
    let aligned_end = anchor_start + anchor_span - own;
    if (raw - aligned_start).abs() <= ARRANGEMENT_SNAP {
        aligned_start
    } else if (raw - aligned_end).abs() <= ARRANGEMENT_SNAP {
        aligned_end
    } else {
        raw.clamp(anchor_start - own + 1, anchor_start + anchor_span - 1)
    }
}

/// Whether display `index` can sit at (`x`, `y`) without overlapping another
/// enabled display and with the layout still connected.
fn placement_valid(monitors: &[Monitor], index: usize, x: i32, y: i32) -> bool {
    let mut trial = monitors.to_vec();
    trial[index].x = x;
    trial[index].y = y;
    let moved = &trial[index];
    let clash = trial
        .iter()
        .enumerate()
        .any(|(other, monitor)| other != index && monitor.enabled && overlaps(moved, monitor));
    !clash && connected(&trial)
}

/// The nearest place to (`x`, `y`) for display `index` that touches another
/// enabled display, does not overlap any, and keeps the layout connected.
/// `None` when there is no such place.
fn settled_point(monitors: &[Monitor], index: usize, x: i32, y: i32) -> Option<(i32, i32)> {
    let (width, height) = monitors[index].extent();
    let mut best = None;
    let mut best_distance = i64::MAX;
    for (_, anchor) in monitors
        .iter()
        .enumerate()
        .filter(|(other, monitor)| *other != index && monitor.enabled)
    {
        let (anchor_width, anchor_height) = anchor.extent();
        let beside_y = touching_start(y, height, anchor.y, anchor_height);
        let beside_x = touching_start(x, width, anchor.x, anchor_width);
        let spots = [
            (anchor.x + anchor_width, beside_y),
            (anchor.x - width, beside_y),
            (beside_x, anchor.y + anchor_height),
            (beside_x, anchor.y - height),
        ];
        for (spot_x, spot_y) in spots {
            let dx = i64::from(spot_x - x);
            let dy = i64::from(spot_y - y);
            let distance = dx * dx + dy * dy;
            if distance < best_distance && placement_valid(monitors, index, spot_x, spot_y) {
                best = Some((spot_x, spot_y));
                best_distance = distance;
            }
        }
    }
    best
}

fn validate(monitors: &[Monitor]) -> Result<(), String> {
    if !monitors.iter().any(|monitor| monitor.enabled) {
        return Err("Enable at least one display.".to_string());
    }
    let primaries = monitors
        .iter()
        .filter(|monitor| monitor.enabled && monitor.primary)
        .count();
    if primaries != 1 {
        return Err("Choose exactly one primary display.".to_string());
    }
    Ok(())
}

/// The `ApplyMonitorsConfig` arguments after the signature, for the enabled
/// monitors only. Each logical monitor carries one physical monitor.
fn apply_values(serial: u32, monitors: &[Monitor], method: &str) -> Vec<String> {
    let logical: Vec<&Monitor> = monitors.iter().filter(|monitor| monitor.enabled).collect();
    let mut values = vec![
        serial.to_string(),
        method.to_string(),
        logical.len().to_string(),
    ];
    for monitor in logical {
        values.extend([
            monitor.x.to_string(),
            monitor.y.to_string(),
            monitor.scale.to_string(),
            monitor.transform.to_string(),
            monitor.primary.to_string(),
            "1".to_string(),
            monitor.connector.clone(),
            monitor.selected().id.clone(),
            "0".to_string(),
        ]);
    }
    values.push("0".to_string());
    values
}

/// The serial and the current layout, read fresh so a write never uses a
/// stale serial.
fn read_layout() -> Result<(u32, Vec<Monitor>), CommandError> {
    mutter_call(true, "GetCurrentState", None, &[])
        .and_then(|text| parse_mutter(&text).map_err(CommandError))
}

/// Applies the draft for a trial. Nothing is saved until the user keeps it.
fn apply(ctx: Ctx) {
    let Some(Page {
        displays: Ok(Displays::Editable { monitors, .. }),
        ..
    }) = ctx.model.read().clone()
    else {
        return;
    };
    let mut error = ctx.error;
    if fake_layout().is_some() {
        error.set(Some(
            "The test layout is for previewing only and cannot be applied.".to_string(),
        ));
        return;
    }
    if let Err(message) = validate(&monitors) {
        error.set(Some(message));
        return;
    }
    let trial = monitors.clone();
    background_then(
        move || -> Result<Vec<Monitor>, CommandError> {
            let (serial, before) = read_layout()?;
            let values = apply_values(serial, &trial, METHOD_TEMPORARY);
            mutter_call(false, "ApplyMonitorsConfig", Some(APPLY_SIGNATURE), &values)?;
            Ok(before)
        },
        move |result| match result {
            Ok(before) => {
                error.set(None);
                start_keep_prompt(ctx, monitors, before);
            }
            // busctl reports Mutter's stale-serial rejection as "Access denied".
            // The layout changed under the draft, so reload and ask the user to redo it.
            Err(failure) if failure.0.contains("Access denied") => {
                error.set(Some(
                    "The display layout changed since it was loaded. It has been reloaded; apply again.".to_string(),
                ));
                reload(ctx.model);
            }
            Err(failure) => error.set(Some(failure.0)),
        },
    );
}

/// Shows the trial as a dialog with a countdown. Keep saves it. Revert, or the
/// countdown reaching zero, restores the layout from before the trial.
fn start_keep_prompt(ctx: Ctx, draft: Vec<Monitor>, before: Vec<Monitor>) {
    let mut prompt = ctx.prompt;
    let mut remaining = ctx.remaining;
    let ticket = *prompt.read() + 1;
    prompt.set(ticket);
    remaining.set(KEEP_SECONDS);
    let on_timeout = before.clone();
    ctx.overlays.show_dialog(
        dialog(
            "Keep these display settings?",
            "The new layout is active for now.",
        )
        .content(Countdown { remaining })
        .response("Revert", ResponseKind::Default, move |_| {
            close_prompt(ctx);
            revert_layout(ctx, before.clone());
        })
        .response("Keep", ResponseKind::Suggested, move |_| {
            close_prompt(ctx);
            keep_layout(ctx, draft.clone());
        }),
    );
    spawn(async move {
        loop {
            timer(Duration::from_secs(1)).await;
            // The prompt was answered or replaced by a newer one.
            if *prompt.read() != ticket {
                return;
            }
            let left = *remaining.read();
            if left <= 1 {
                close_prompt(ctx);
                revert_layout(ctx, on_timeout);
                return;
            }
            remaining.set(left - 1);
        }
    });
}

/// Closes the keep prompt and stops its countdown.
fn close_prompt(ctx: Ctx) {
    let mut prompt = ctx.prompt;
    let next = *prompt.read() + 1;
    prompt.set(next);
    ctx.overlays.dismiss_dialog();
}

/// Saves the trial layout with a fresh serial.
fn keep_layout(ctx: Ctx, draft: Vec<Monitor>) {
    let model = ctx.model;
    let mut error = ctx.error;
    background_then(
        move || -> CommandResult {
            let (serial, _) = read_layout()?;
            let values = apply_values(serial, &draft, METHOD_PERSISTENT);
            mutter_call(false, "ApplyMonitorsConfig", Some(APPLY_SIGNATURE), &values)
        },
        move |result| match result {
            Ok(_) => {
                error.set(None);
                reload(model);
            }
            Err(failure) => {
                error.set(Some(failure.0));
                reload(model);
            }
        },
    );
}

/// Restores the layout from before the trial. Temporary, so the saved layout
/// is left as it was.
fn revert_layout(ctx: Ctx, before: Vec<Monitor>) {
    let model = ctx.model;
    let mut error = ctx.error;
    background_then(
        move || -> CommandResult {
            let (serial, _) = read_layout()?;
            let values = apply_values(serial, &before, METHOD_TEMPORARY);
            mutter_call(false, "ApplyMonitorsConfig", Some(APPLY_SIGNATURE), &values)
        },
        move |result| match result {
            Ok(_) => reload(model),
            Err(failure) => {
                error.set(Some(failure.0));
                reload(model);
            }
        },
    );
}

fn update_night(mut model: State<Option<Page>>, change: impl FnOnce(&mut Night)) {
    let mut page = model.write();
    if let Some(Page {
        night: Ok(night), ..
    }) = page.as_mut()
    {
        change(night);
    }
}

/// Writes one night-light key. On failure the page reloads to show the real
/// value again.
fn write_night(
    model: State<Option<Page>>,
    mut error: State<Option<String>>,
    key: &'static str,
    value: String,
) {
    background_then(
        move || system::gsettings_set(COLOR, key, &value),
        move |result| match result {
            Ok(_) => error.set(None),
            Err(failure) => {
                error.set(Some(failure.0));
                reload(model);
            }
        },
    );
}

fn set_night_enabled(model: State<Option<Page>>, error: State<Option<String>>, on: bool) {
    update_night(model, |night| night.enabled = on);
    let value = if on { "true" } else { "false" };
    write_night(model, error, "night-light-enabled", value.to_string());
}

/// Updates the displayed temperature at once and writes it once the slider
/// has been still for `TEMP_DEBOUNCE`.
fn set_night_level(
    model: State<Option<Page>>,
    error: State<Option<String>>,
    mut ticket: State<u64>,
    level: f64,
) {
    let kelvin = kelvin_for_level(level);
    update_night(model, move |night| night.kelvin = kelvin);
    let next = *ticket.read() + 1;
    ticket.set(next);
    spawn(async move {
        timer(TEMP_DEBOUNCE).await;
        if *ticket.read() == next {
            write_night(
                model,
                error,
                "night-light-temperature",
                format!("uint32 {kelvin}"),
            );
        }
    });
}

fn record_group(record: &Record) -> Group {
    Group::titled(&record.title).rows(
        record
            .rows
            .iter()
            .map(|(label, value)| Element::from(row(label.clone()).value(value.clone()))),
    )
}

/// The bounds and scale of the arrangement, frozen while a display is dragged
/// so the canvas does not rescale under the pointer.
#[derive(Clone, Copy, PartialEq)]
struct Frame {
    left: i32,
    top: i32,
    fit: f64,
    /// Where the layout's top-left corner sits inside the canvas.
    origin: (f32, f32),
}

/// A display being dragged: where the pointer went down, where the display was
/// then, and the frame at that moment.
#[derive(Clone, Copy, PartialEq)]
struct Drag {
    index: usize,
    pointer: (f64, f64),
    origin: (i32, i32),
    frame: Frame,
}

/// Everything the page's actions need. All fields are handles, so the struct
/// is copied into each callback.
#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Page>>,
    error: State<Option<String>>,
    night_ticket: State<u64>,
    prompt: State<u64>,
    remaining: State<u64>,
    selected: State<usize>,
    drag: State<Option<Drag>>,
    overlays: Overlays,
}

/// The line in the keep prompt that counts down to the revert.
#[derive(Clone, PartialEq)]
struct Countdown {
    remaining: State<u64>,
}

impl Component for Countdown {
    fn render(&self) -> impl IntoElement {
        let seconds = *self.remaining.read();
        caption(format!(
            "It reverts in {seconds} seconds if you do not keep it."
        ))
    }
}

/// The layout as boxes: each enabled display at its place, scaled to fit.
/// The primary display is filled with the accent colour. The selected display
/// has an accent outline. Dragging a box moves it; releasing settles it beside
/// another display.
#[derive(Clone, PartialEq)]
struct Arrangement {
    ctx: Ctx,
    monitors: Vec<Monitor>,
    selected: usize,
    tokens: Tokens,
}

impl Component for Arrangement {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let monitors = &self.monitors;
        let selected = self.selected;
        let tokens = self.tokens;
        let mut size = use_state(Area::default);
        let measured = size.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            ARRANGEMENT_WIDTH
        };
        let active = *ctx.drag.read();
        let Some(current) = frame_of(
            monitors,
            (width, ARRANGEMENT_HEIGHT - arrangement_footer_height()),
        ) else {
            return caption("No displays are on.").into_element();
        };
        let frame = active.map_or(current, |drag| drag.frame);

        let mut canvas = rect()
            .width(Size::fill())
            .height(Size::px(ARRANGEMENT_HEIGHT))
            .corner_radius(RADIUS_LG)
            .background(tokens.control)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.separator),
            )
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .on_capture_global_pointer_move(move |event: Event<PointerEventData>| {
                let Some(drag) = *ctx.drag.peek() else {
                    return;
                };
                let pointer = event.global_location();
                let dx = ((pointer.x - drag.pointer.0) / drag.frame.fit).round() as i32;
                let dy = ((pointer.y - drag.pointer.1) / drag.frame.fit).round() as i32;
                let (x, y) = (drag.origin.0 + dx, drag.origin.1 + dy);
                edit(ctx.model, ctx.error, move |monitors| {
                    monitors[drag.index].x = x;
                    monitors[drag.index].y = y;
                    Ok(())
                });
            })
            .on_global_pointer_up(move |_| {
                let Some(drag) = *ctx.drag.peek() else {
                    return;
                };
                let mut drag_state = ctx.drag;
                drag_state.set(None);
                edit(ctx.model, ctx.error, move |monitors| {
                    let (x, y) = (monitors[drag.index].x, monitors[drag.index].y);
                    match settled_point(monitors, drag.index, x, y) {
                        Some((x, y)) => {
                            monitors[drag.index].x = x;
                            monitors[drag.index].y = y;
                            Ok(())
                        }
                        None => {
                            monitors[drag.index].x = drag.origin.0;
                            monitors[drag.index].y = drag.origin.1;
                            Err("Keep each display touching another one.".to_string())
                        }
                    }
                });
            });

        let dragging = active.map(|drag| drag.index);
        for (index, monitor) in monitors
            .iter()
            .enumerate()
            .filter(|(_, monitor)| monitor.enabled)
        {
            canvas = canvas.child(Tile {
                ctx,
                index,
                monitor: monitor.clone(),
                frame,
                selected: index == selected,
                dragging: dragging == Some(index),
                tokens,
            });
        }
        canvas
            .child(arrangement_footer(ctx, monitors, selected))
            .into_element()
    }
}

/// One enabled display in the arrangement. It is its own component so that
/// its position can ease into the settled place.
#[derive(Clone, PartialEq)]
struct Tile {
    ctx: Ctx,
    index: usize,
    monitor: Monitor,
    frame: Frame,
    selected: bool,
    dragging: bool,
    tokens: Tokens,
}

impl Component for Tile {
    fn render(&self) -> impl IntoElement {
        let tokens = self.tokens;
        let monitor = &self.monitor;
        let frame = self.frame;
        let (width, height) = monitor.extent();
        // Each box is inset by half the gap, so neighbouring boxes are S0 apart.
        let inset = S0 / 2.0;
        let target_left =
            frame.origin.0 + (f64::from(monitor.x - frame.left) * frame.fit) as f32 + inset;
        let target_top =
            frame.origin.1 + (f64::from(monitor.y - frame.top) * frame.fit) as f32 + inset;
        let box_width = ((f64::from(width) * frame.fit) as f32 - S0).max(0.0);
        let box_height = ((f64::from(height) * frame.fit) as f32 - S0).max(0.0);
        let time = if self.dragging {
            0
        } else {
            tokens.motion(DRAG_SETTLE_MS)
        };
        let left = use_animation_transition(target_left, move |from, to| {
            AnimNum::new(from, to)
                .time(time)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let top = use_animation_transition(target_top, move |from, to| {
            AnimNum::new(from, to)
                .time(time)
                .function(Function::Expo)
                .ease(Ease::Out)
        });

        let (fill, ink) = if monitor.primary {
            (tokens.accent, tokens.accent_foreground)
        } else {
            (tokens.elevated, tokens.text)
        };
        let border = if self.selected {
            Border::new()
                .width(RING_WIDTH)
                .alignment(BorderAlignment::Inner)
                .fill(if monitor.primary {
                    tokens.text
                } else {
                    tokens.accent
                })
        } else {
            Border::new()
        };
        let mut title = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .child(
                typography::EMPHASIS
                    .label(&tokens, monitor.name.clone())
                    .color(ink)
                    .width(Size::fill())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        if monitor.primary {
            title = title.child(icon(Icon::Starred, ICON_SM, ink));
        }
        let details = typography::CAPTION
            .label(&tokens, monitor.selected().resolution())
            .color(ink.with_af32(0.8));

        let ctx = self.ctx;
        let index = self.index;
        let origin = (monitor.x, monitor.y);
        let on_press = move |event: Event<PointerEventData>| {
            if !event.data().is_primary() {
                return;
            }
            let pointer = event.global_location();
            let mut selected_state = ctx.selected;
            selected_state.set(index);
            let mut drag_state = ctx.drag;
            drag_state.set(Some(Drag {
                index,
                pointer: (pointer.x, pointer.y),
                origin,
                frame,
            }));
        };

        rect()
            .position(
                Position::new_absolute()
                    .left(left.get().value())
                    .top(top.get().value()),
            )
            .width(Size::px(box_width))
            .height(Size::px(box_height))
            .corner_radius(RADIUS_SM)
            .padding(S2)
            .background(fill)
            .border(border)
            .on_pointer_down(on_press)
            .child(title)
            .child(details)
    }
}

fn arrangement_group(ctx: Ctx, monitors: &[Monitor], selected: usize, tokens: Tokens) -> Group {
    Group::titled("Arrangement")
        .description("Each box is one display, in the place it has on the desktop.")
        .row(
            Arrangement {
                ctx,
                monitors: monitors.to_vec(),
                selected,
                tokens,
            }
            .into_element(),
        )
}

fn monitor_group(
    model: State<Option<Page>>,
    error: State<Option<String>>,
    index: usize,
    monitor: &Monitor,
) -> Group {
    let enabled = monitor.enabled;
    let mode = monitor.selected();
    Group::titled(&monitor.name)
        .description(monitor_state(monitor))
        .row(row("Port").value(monitor.connector.clone()))
        .row(switch_row("Enabled", enabled, move |on: bool| {
            edit(model, error, move |monitors| {
                set_enabled(monitors, index, on)
            });
        }))
        .row(
            choice_row(
                "Resolution",
                monitor.resolutions(),
                &mode.resolution(),
                move |key: String| {
                    edit(model, error, move |monitors| {
                        set_resolution(monitors, index, &key)
                    });
                },
            )
            .enabled(enabled),
        )
        .row(
            choice_row(
                "Refresh rate",
                monitor.refresh_options(),
                &mode.id,
                move |id: String| {
                    edit(model, error, move |monitors| {
                        set_refresh(monitors, index, &id)
                    });
                },
            )
            .enabled(enabled),
        )
        .row(
            choice_row(
                "Scale",
                monitor.scale_options(),
                &monitor.scale_key(),
                move |key: String| {
                    edit(model, error, move |monitors| {
                        set_scale(monitors, index, &key)
                    });
                },
            )
            .enabled(enabled),
        )
        .row(
            choice_row(
                "Orientation",
                monitor.orientation_options(),
                &monitor.transform.to_string(),
                move |key: String| {
                    edit(model, error, move |monitors| {
                        set_transform(monitors, index, &key)
                    });
                },
            )
            .enabled(enabled),
        )
        .row(
            switch_row("Primary", monitor.primary, move |on: bool| {
                // Turning the primary display off would leave none; the only
                // way to change it is to make another display primary.
                if on {
                    edit(model, error, move |monitors| set_primary(monitors, index));
                }
            })
            .enabled(enabled),
        )
}

/// Hours of the day for the schedule pickers, as `"0"` to `"23"`.
fn hour_options() -> Vec<(String, String)> {
    (0..24u32)
        .map(|hour| (hour.to_string(), format!("{hour:02}:00")))
        .collect()
}

fn hour_key(hours: f64) -> String {
    ((hours.round() as u32) % 24).to_string()
}

fn set_night_schedule(model: State<Option<Page>>, error: State<Option<String>>, on: bool) {
    update_night(model, |night| night.automatic = on);
    write_night(
        model,
        error,
        "night-light-schedule-automatic",
        on.to_string(),
    );
}

fn set_night_hour(model: State<Option<Page>>, error: State<Option<String>>, from: bool, key: &str) {
    let Ok(hour) = key.parse::<f64>() else {
        return;
    };
    update_night(model, |night| {
        if from {
            night.from = hour;
        } else {
            night.to = hour;
        }
    });
    let gsettings_key = if from {
        "night-light-schedule-from"
    } else {
        "night-light-schedule-to"
    };
    write_night(model, error, gsettings_key, format!("{hour:.1}"));
}

/// The colour a display shows at `kelvin`, from Tanner Helland's fit of
/// blackbody radiation.
fn kelvin_rgb(kelvin: u32) -> (u8, u8, u8) {
    let temp = f64::from(kelvin) / 100.0;
    let red = if temp <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (temp - 60.0).powf(-0.133_204_759_2)
    };
    let green = if temp <= 66.0 {
        99.470_802_586_1 * temp.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (temp - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if temp >= 66.0 {
        255.0
    } else if temp <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (temp - 10.0).ln() - 305.044_792_730_7
    };
    let channel = |value: f64| value.clamp(0.0, 255.0).round() as u8;
    (channel(red), channel(green), channel(blue))
}

/// The night light control: a strip of colours from the warmest to the coolest
/// temperature. The strip is the control itself, and a white pill marks the
/// chosen temperature (see `KelvinStrip`).
fn temperature_control(kelvin: u32, on_moved: impl Into<EventHandler<f64>>) -> Element {
    use std::fmt::Write as _;
    const STOPS: u32 = 24;
    let mut stops = String::new();
    for step in 0..STOPS {
        let fraction = f64::from(step) / f64::from(STOPS - 1);
        let at = TEMP_MIN + (f64::from(TEMP_MAX - TEMP_MIN) * fraction).round() as u32;
        let (red, green, blue) = kelvin_rgb(at);
        let _ = write!(
            stops,
            r##"<stop offset="{}%" stop-color="#{red:02x}{green:02x}{blue:02x}"/>"##,
            fraction * 100.0
        );
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{FIELD_WIDTH}" height="{S2}" viewBox="0 0 {FIELD_WIDTH} {S2}"><defs><linearGradient id="kelvin" x1="0" y1="0" x2="1" y2="0">{stops}</linearGradient></defs><rect width="{FIELD_WIDTH}" height="{S2}" rx="{S1}" fill="url(#kelvin)"/></svg>"#
    );
    KelvinStrip {
        kelvin,
        svg,
        on_moved: on_moved.into(),
    }
    .into()
}

const SCALE_PRESSED: f32 = 1.1;
const SCALE_MS: u64 = 120;
const LEVEL_MAX: f64 = 100.0;
/// Arrow keys move the strip this many percent, Page Up and Page Down by ten.
const KELVIN_STEP: f64 = 5.0;
const KELVIN_PAGE_STEP: f64 = 10.0;

/// The night light strip. It is the control itself: press or drag anywhere on
/// it, or use the arrow, Home, End, Page Up and Page Down keys. The white pill
/// is the thumb, S2 wide and S5 tall.
#[derive(Clone, PartialEq)]
struct KelvinStrip {
    kelvin: u32,
    svg: String,
    on_moved: EventHandler<f64>,
}

impl Component for KelvinStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        let mut size = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_moved = self.on_moved.clone();
        let kelvin = self.kelvin;
        let level = kelvin_level(kelvin);
        let svg = self.svg.clone();

        let scale_target = if pressing() { SCALE_PRESSED } else { 1.0 };
        let motion = tokens.motion(SCALE_MS);
        let scale = use_animation_transition(scale_target, move |from, to| {
            AnimNum::new(from, to)
                .time(motion)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let thumb_width = S2 * scale.get().value();
        let thumb_height = S5 * scale.get().value();
        let travel = (FIELD_WIDTH - S2).max(1.0);
        let centre_x = S2 / 2.0 + travel * (level / LEVEL_MAX) as f32;
        let percent_at = move |x: f64| -> f64 {
            let half = f64::from(S2 / 2.0);
            ((x - half) / f64::from(travel) * LEVEL_MAX).clamp(0.0, LEVEL_MAX)
        };
        let on_pointer_down = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                a11y_id.request_focus();
                pressing.set(true);
                on_moved.call(percent_at(event.element_location().x));
            }
        };
        let on_move = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if *pressing.peek() {
                    event.prevent_default();
                    let x = event.global_location().x - size.read().min_x() as f64;
                    on_moved.call(percent_at(x));
                }
            }
        };
        let on_release = move |_| {
            pressing.set_if_modified(false);
        };
        let on_key_down = {
            let on_moved = on_moved.clone();
            move |event: Event<KeyboardEventData>| {
                let delta = match event.key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                        -KELVIN_STEP
                    }
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => KELVIN_STEP,
                    Key::Named(NamedKey::Home) => -level,
                    Key::Named(NamedKey::End) => LEVEL_MAX - level,
                    Key::Named(NamedKey::PageUp) => KELVIN_PAGE_STEP,
                    Key::Named(NamedKey::PageDown) => -KELVIN_PAGE_STEP,
                    _ => return,
                };
                on_moved.call((level + delta).clamp(0.0, LEVEL_MAX));
            }
        };

        let ring = S2 + RING_WIDTH * 2.0;
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(f64::from(kelvin));
                builder.set_min_numeric_value(f64::from(TEMP_MIN));
                builder.set_max_numeric_value(f64::from(TEMP_MAX));
            })
            .width(Size::px(FIELD_WIDTH))
            .height(Size::px(S5))
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .on_pointer_down(on_pointer_down)
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(on_release)
            .on_key_down(on_key_down)
            .child(
                rect()
                    .position(Position::new_absolute().top((S5 - S2) / 2.0).left(0.0))
                    .width(Size::px(FIELD_WIDTH))
                    .height(Size::px(S2))
                    .child(
                        SvgViewer::new((svg.clone(), Bytes::from(svg)))
                            .width(Size::px(FIELD_WIDTH))
                            .height(Size::px(S2)),
                    ),
            )
            .maybe_child((focus() == Focus::Keyboard).then(|| {
                rect()
                    .position(
                        Position::new_absolute()
                            .top((S5 - ring) / 2.0)
                            .left(centre_x - ring / 2.0),
                    )
                    .width(Size::px(ring))
                    .height(Size::px(ring))
                    .corner_radius(ring / 2.0)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.accent),
                    )
            }))
            .child(
                rect()
                    .layer(Layer::Relative(3))
                    .position(
                        Position::new_absolute()
                            .top((S5 - thumb_height) / 2.0)
                            .left(centre_x - thumb_width / 2.0),
                    )
                    .width(Size::px(thumb_width))
                    .height(Size::px(thumb_height))
                    .corner_radius(thumb_width / 2.0)
                    .background(Color::from_rgb(255, 255, 255))
                    .border(
                        Border::new()
                            .width(S0 / 2.0)
                            .alignment(BorderAlignment::Outer)
                            .fill(Color::from_rgb(64, 64, 64)),
                    ),
            )
            .into_element()
    }
}

fn night_group(ctx: Ctx, night: &Night) -> Group {
    let model = ctx.model;
    let error = ctx.error;
    let mut group = Group::titled("Night light")
        .row(switch_row("Enabled", night.enabled, move |on: bool| {
            set_night_enabled(model, error, on);
        }))
        .row(
            row("Colour temperature")
                .subtitle(temperature_word(night.kelvin))
                .trailing(temperature_control(night.kelvin, move |level: f64| {
                    set_night_level(model, error, ctx.night_ticket, level)
                })),
        )
        .row(switch_row(
            "On a schedule",
            night.automatic,
            move |on: bool| set_night_schedule(model, error, on),
        ));
    if night.automatic {
        group = group
            .row(choice_row(
                "From",
                hour_options(),
                &hour_key(night.from),
                move |key: String| set_night_hour(model, error, true, &key),
            ))
            .row(choice_row(
                "Until",
                hour_options(),
                &hour_key(night.to),
                move |key: String| set_night_hour(model, error, false, &key),
            ));
    }
    group
}

/// The line under a monitor's name: whether it is on, and whether it is the
/// primary display.
fn monitor_state(monitor: &Monitor) -> &'static str {
    if !monitor.enabled {
        "Off"
    } else if monitor.primary {
        "Primary display"
    } else {
        "Secondary display"
    }
}

/// Apply shows the new layout at once and asks the user to keep it. It sits
/// under the last group.
fn apply_footer(ctx: Ctx) -> Element {
    action_row(
        Some("Asks you to keep the layout after it is applied."),
        primary_button("Apply", move |_| apply(ctx)).compact(),
    )
}

impl Component for DisplaysPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("displays", load);
        let error = use_state::<Option<String>>(|| None);
        let night_ticket = use_state::<u64>(|| 0);
        let prompt = use_state::<u64>(|| 0);
        let remaining = use_state::<u64>(|| KEEP_SECONDS);
        let selected = use_state::<usize>(|| 0);
        let drag = use_state::<Option<Drag>>(|| None);
        let tokens = use_tokens();
        let ctx = Ctx {
            model,
            error,
            night_ticket,
            prompt,
            remaining,
            selected,
            drag,
            overlays: use_overlays(),
        };

        let mut page = page_column();
        let Some(loaded) = model.read().clone() else {
            return page.child(skeleton());
        };
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        page = match loaded.displays {
            Err(failure) => page.child(
                status(Status::Error(failure.0)).action("Try again", move |_| reload(model)),
            ),
            Ok(Displays::ReadOnly { reason, records }) => {
                let mut page = page.child(status(Status::Info(format!(
                    "{reason} Showing the connected displays."
                ))));
                for record in &records {
                    page = page.child(record_group(record));
                }
                page
            }
            Ok(Displays::Editable { monitors, .. }) => {
                let chosen = (*selected.read()).min(monitors.len().saturating_sub(1));
                let mut page = page.child(arrangement_group(ctx, &monitors, chosen, tokens));
                if let Some(monitor) = monitors.get(chosen) {
                    page = page.child(monitor_group(model, error, chosen, monitor));
                }
                page.child(apply_footer(ctx))
            }
        };

        page = match loaded.night {
            Ok(night) => page.child(night_group(ctx, &night)),
            Err(failure) => page.child(status(Status::Error(failure.0))),
        };
        page
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("displays", load);
}
