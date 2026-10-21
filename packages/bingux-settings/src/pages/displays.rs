//! Displays: monitor layout, resolution, refresh rate, scale and orientation,
//! plus night light.
//!
//! The layout comes from Mutter's DisplayConfig over D-Bus, read and written
//! with `busctl`. When that service cannot be read, the page shows
//! `gnoblinctl`'s monitor list read-only. Night light lives in GSettings.

use std::time::Duration;

use freya::prelude::*;

use crate::pages::gnoblin;
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Status, Tokens, caption, choice_row, divider, hstack, page_column, primary_button, row,
    skeleton, slider_row, status, switch_row, use_tokens,
};
use bingux_ui::scale::*;

const SERVICE: &str = "org.gnome.Mutter.DisplayConfig";
const OBJECT: &str = "/org/gnome/Mutter/DisplayConfig";
const INTERFACE: &str = "org.gnome.Mutter.DisplayConfig";
/// `ApplyMonitorsConfig(serial, method, logical monitors, properties)`.
const APPLY_SIGNATURE: &str = "uua(iiduba(ssa{sv}))a{sv}";
/// Method 2 stores the configuration persistently, method 1 is temporary.
const METHOD_PERSISTENT: &str = "2";
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
        let preferred = if self.preferred { " (preferred)" } else { "" };
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

fn load_displays() -> Result<Displays, CommandError> {
    let reason = match mutter_call(true, "GetCurrentState", None, &[])
        .and_then(|text| parse_mutter(&text).map_err(CommandError))
    {
        Ok((serial, monitors)) => return Ok(Displays::Editable { serial, monitors }),
        Err(failure) => failure.0,
    };
    // busctl reports an absent service as "not activatable"; that is the
    // normal case outside GNOME and needs no diagnostic text.
    let reason = if reason.contains("not activatable") {
        "Mutter's DisplayConfig is not available.".to_string()
    } else {
        format!("Mutter's DisplayConfig could not be read: {reason}")
    };
    match load_gnoblin() {
        Ok(records) => Ok(Displays::ReadOnly { reason, records }),
        Err(fallback) => Err(CommandError(format!("{reason} {}", fallback.0))),
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
    let orientation = record
        .get("transform")
        .and_then(Json::text)
        .unwrap_or("Unknown")
        .to_string();
    let rows = [
        ("Output", name.to_string()),
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
    let kelvin = temperature
        .split_whitespace()
        .last()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| {
            CommandError(format!("Unexpected night light temperature: {temperature}"))
        })?;
    Ok(Night {
        enabled: enabled.trim() == "true",
        kelvin,
    })
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
fn apply_values(serial: u32, monitors: &[Monitor]) -> Vec<String> {
    let logical: Vec<&Monitor> = monitors.iter().filter(|monitor| monitor.enabled).collect();
    let mut values = vec![
        serial.to_string(),
        METHOD_PERSISTENT.to_string(),
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

fn apply(model: State<Option<Page>>, mut error: State<Option<String>>) {
    let Some(Page {
        displays: Ok(Displays::Editable { serial, monitors }),
        ..
    }) = model.read().clone()
    else {
        return;
    };
    if let Err(message) = validate(&monitors) {
        error.set(Some(message));
        return;
    }
    let values = apply_values(serial, &monitors);
    background_then(
        move || mutter_call(false, "ApplyMonitorsConfig", Some(APPLY_SIGNATURE), &values),
        move |result| match result {
            Ok(_) => {
                error.set(None);
                reload(model);
            }
            // busctl reports Mutter's stale-serial rejection as "Access denied".
            // The layout changed under the draft, so reload and ask the user to redo it.
            Err(failure) if failure.0.contains("Access denied") => {
                error.set(Some(
                    "The display layout changed since it was loaded. It has been reloaded; apply again.".to_string(),
                ));
                reload(model);
            }
            Err(failure) => error.set(Some(failure.0)),
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
        .row(row("Output").value(monitor.connector.clone()))
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

fn night_group(
    model: State<Option<Page>>,
    error: State<Option<String>>,
    ticket: State<u64>,
    night: &Night,
) -> Group {
    Group::titled("Night light")
        .row(switch_row("Enabled", night.enabled, move |on: bool| {
            set_night_enabled(model, error, on);
        }))
        .row(slider_row(
            "Colour temperature",
            kelvin_level(night.kelvin),
            move |level: f64| set_night_level(model, error, ticket, level),
        ))
        .row(row("Temperature").value(format!("{} K", night.kelvin)))
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

/// Apply saves the draft layout. It sits under the last monitor on the
/// page inset, with a hairline above it like the group headings.
fn apply_footer(tokens: &Tokens, model: State<Option<Page>>, error: State<Option<String>>) -> Rect {
    rect().width(Size::fill()).child(divider(tokens)).child(
        hstack(S4)
            .width(Size::fill())
            .padding((S4, GUTTER))
            .content(Content::Flex)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .child(caption("Saved for future sessions.")),
            )
            .child(primary_button("Apply", move |_| apply(model, error))),
    )
}

impl Component for DisplaysPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("displays", load);
        let error = use_state::<Option<String>>(|| None);
        let night_ticket = use_state::<u64>(|| 0);
        let tokens = use_tokens();

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
                    "{reason} Showing gnoblinctl's read-only monitor list."
                ))));
                for record in &records {
                    page = page.child(record_group(record));
                }
                page
            }
            Ok(Displays::Editable { monitors, .. }) => {
                let mut page = page;
                for (index, monitor) in monitors.iter().enumerate() {
                    page = page.child(monitor_group(model, error, index, monitor));
                }
                page.child(apply_footer(&tokens, model, error))
            }
        };

        page = match loaded.night {
            Ok(night) => page.child(night_group(model, error, night_ticket, &night)),
            Err(failure) => page.child(status(Status::Error(failure.0))),
        };
        page
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("displays", load);
}
