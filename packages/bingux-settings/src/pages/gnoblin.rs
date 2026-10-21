//! Shared access to the running Gnoblin session: the committed config
//! snapshot (to detect Lua overrides of GSettings input keys), input sources
//! and detected devices. Written by the keyboard and mouse pages.

use std::time::Duration;

use crate::system::{self, CommandError, CommandResult};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A parsed JSON document. Objects keep their member order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let value = parser.value()?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err(parser.error("trailing characters"));
        }
        Ok(value)
    }

    /// Follow object keys from this value. `None` if any step is missing or
    /// is not an object.
    pub fn get(&self, path: &[&str]) -> Option<&Json> {
        path.iter().try_fold(self, |node, key| match node {
            Json::Object(members) => members
                .iter()
                .find(|(name, _)| name == *key)
                .map(|(_, value)| value),
            _ => None,
        })
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> String {
        format!("{what} at byte {}", self.pos)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.skip_whitespace();
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        let byte = self
            .peek()
            .ok_or_else(|| self.error("unexpected end of input"))?;
        match byte {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Json::String),
            b't' => self.literal("true", Json::Bool(true)),
            b'f' => self.literal("false", Json::Bool(false)),
            b'n' => self.literal("null", Json::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.error("unexpected character")),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(members));
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            members.push((key, self.value()?));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
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
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.skip_whitespace();
        if self.peek() != Some(b'"') {
            return Err(self.error("expected string"));
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            // Runs of plain bytes are copied as UTF-8; the input is a `&str`
            // and the scan stops only at ASCII delimiters, so every run is
            // valid UTF-8 on its own.
            let start = self.pos;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            let run = std::str::from_utf8(&self.bytes[start..self.pos])
                .map_err(|_| self.error("invalid UTF-8"))?;
            out.push_str(run);

            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escape = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.pos += 1;
                    let plain = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode_escape()?,
                        _ => return Err(self.error("invalid escape")),
                    };
                    out.push(plain);
                }
                _ => return Err(self.error("unterminated string")),
            }
        }
    }

    /// The code point after `\u`, combining a surrogate pair when needed.
    fn unicode_escape(&mut self) -> Result<char, String> {
        let high = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&high) {
            if !self.bytes[self.pos..].starts_with(b"\\u") {
                return Err(self.error("unpaired surrogate"));
            }
            self.pos += 2;
            let low = self.hex4()?;
            if !(0xDC00..0xE000).contains(&low) {
                return Err(self.error("invalid surrogate pair"));
            }
            0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
        } else {
            high
        };
        char::from_u32(code).ok_or_else(|| self.error("invalid code point"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("short unicode escape"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("invalid unicode escape"))?;
        let value =
            u32::from_str_radix(text, 16).map_err(|_| self.error("invalid unicode escape"))?;
        self.pos += 4;
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid number"))?;
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| self.error("invalid number"))
    }
}

fn parse_error(error: String) -> CommandError {
    CommandError(format!("Could not read Gnoblin's reply: {error}"))
}

/// The committed settings snapshot. Only fields set in the Lua config are
/// present; `inherit` fields are absent.
pub fn config_snapshot() -> Result<Json, CommandError> {
    let output = system::run("gnoblinctl", &["--json", "config", "show"], TIMEOUT)?;
    Json::parse(&output).map_err(parse_error)
}

/// The Lua overrides. When Gnoblin is not running the snapshot cannot be
/// read, and every setting is shown as unoverridden.
pub fn overrides() -> Json {
    config_snapshot().unwrap_or(Json::Object(Vec::new()))
}

/// The Lua value at `path` as display text, or `None` when the config does
/// not set it (absent or `"inherit"`).
pub fn lua_override(snapshot: &Json, path: &[&str]) -> Option<String> {
    match snapshot.get(path)? {
        Json::Null => None,
        Json::String(text) if text == "inherit" => None,
        Json::String(text) => Some(text.clone()),
        Json::Bool(true) => Some("On".to_string()),
        Json::Bool(false) => Some("Off".to_string()),
        Json::Number(number) => Some(number.to_string()),
        Json::Array(items) => Some(
            items
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Json::Object(_) => Some("Set in config".to_string()),
    }
}

/// The path of the active Gnoblin configuration file.
pub fn config_path() -> Result<String, CommandError> {
    let output = system::run("gnoblinctl", &["config", "path"], TIMEOUT)?;
    Ok(output.trim().to_string())
}

/// A keyboard layout or input method Gnoblin can switch to.
#[derive(Clone, Debug, PartialEq)]
pub struct InputSource {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// The configured input sources, with the active one flagged.
pub fn input_sources() -> Result<Vec<InputSource>, CommandError> {
    let output = system::run("gnoblinctl", &["--json", "input", "list"], TIMEOUT)?;
    let list = Json::parse(&output).map_err(parse_error)?;
    let sources = list
        .get(&["sources"])
        .and_then(Json::as_array)
        .ok_or_else(|| CommandError("Gnoblin's reply has no input source list".to_string()))?;
    Ok(sources
        .iter()
        .filter_map(|source| {
            let text = |key: &str| source.get(&[key]).and_then(Json::as_str);
            Some(InputSource {
                kind: text("type")?.to_string(),
                id: text("id")?.to_string(),
                name: text("name").or_else(|| text("short_name"))?.to_string(),
                active: source
                    .get(&["current"])
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect())
}

pub fn select_input_source(kind: &str, id: &str) -> CommandResult {
    system::run("gnoblinctl", &["input", "select", kind, id], TIMEOUT)
}

/// Whether a touchpad is attached. Uses Gnoblin's device list when it
/// answers, and the kernel's device list otherwise.
pub fn has_touchpad() -> bool {
    let devices = system::run("gnoblinctl", &["--json", "input", "devices"], TIMEOUT)
        .ok()
        .and_then(|output| Json::parse(&output).ok());
    match devices {
        Some(list) => list
            .get(&["devices"])
            .and_then(Json::as_array)
            .is_some_and(|devices| {
                devices.iter().any(|device| {
                    device.get(&["device_type"]).and_then(Json::as_str) == Some("touchpad")
                        || device
                            .get(&["capabilities"])
                            .and_then(Json::as_array)
                            .is_some_and(|caps| {
                                caps.iter().any(|cap| cap.as_str() == Some("touchpad"))
                            })
                })
            }),
        None => std::fs::read_to_string("/proc/bus/input/devices").is_ok_and(|text| {
            text.lines().any(|line| {
                line.starts_with("N: Name=") && line.to_ascii_lowercase().contains("touchpad")
            })
        }),
    }
}
