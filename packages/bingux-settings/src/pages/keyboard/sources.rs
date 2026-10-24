//! Gnoblin's input sources, read from and written to the Lua configuration.
//!
//! The list lives in `gnoblin.configure { input_sources = { ... } }`. The
//! file is edited as text so its comments and layout survive: a small Lua
//! lexer finds the table, and only that table is rewritten. A file that uses
//! any other form of the table is refused rather than guessed at.

use std::sync::{Condvar, Mutex, PoisonError};

use crate::pages::gnoblin;

const UNSUPPORTED: &str =
    "This page cannot change the input sources in the configuration file. Edit the file directly.";
const UNREADABLE: &str = "The configuration file could not be read.";
const UNWRITABLE: &str = "The configuration file could not be saved.";
const XKB_EVDEV: &str = "/usr/share/X11/xkb/rules/evdev.xml";
const MAX_ID: usize = 64;

/// A keyboard layout or input method, as the configuration names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub kind: String,
    pub id: String,
}

/// The whole `input_sources` table.
#[derive(Clone, Debug, PartialEq)]
pub struct SourcesConfig {
    pub sources: Vec<Source>,
    pub per_window: bool,
}

/// One change to the list. Indexes refer to the list before the change.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Add(Source),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    SetPerWindow(bool),
}

impl SourcesConfig {
    /// The table with `edit` applied. Refuses changes the file could not
    /// hold: an invalid or repeated source, or removing the last one.
    pub fn apply(&self, edit: &Edit) -> Result<SourcesConfig, String> {
        let mut next = self.clone();
        let count = next.sources.len();
        match edit {
            Edit::Add(source) => {
                if !valid_source(source) {
                    return Err("That input source cannot be added.".to_string());
                }
                if next.sources.contains(source) {
                    return Err("That input source is already in the list.".to_string());
                }
                next.sources.push(source.clone());
            }
            Edit::Remove(index) => {
                if *index >= count {
                    return Err("That input source is no longer in the list.".to_string());
                }
                if count == 1 {
                    return Err("At least one input source is needed.".to_string());
                }
                next.sources.remove(*index);
            }
            Edit::MoveUp(index) => {
                if *index == 0 || *index >= count {
                    return Err("That input source cannot move up.".to_string());
                }
                next.sources.swap(index - 1, *index);
            }
            Edit::MoveDown(index) => {
                if *index + 1 >= count {
                    return Err("That input source cannot move down.".to_string());
                }
                next.sources.swap(*index, index + 1);
            }
            Edit::SetPerWindow(value) => next.per_window = *value,
        }
        Ok(next)
    }
}

/// The input sources in the configuration file. Snapshot builds read the
/// fake list when `BINGUX_SETTINGS_FAKE_INPUT` is set.
pub fn read() -> Result<SourcesConfig, String> {
    if let Some(config) = fake_config() {
        return Ok(config);
    }
    let path = gnoblin::config_path().map_err(|e| e.0)?;
    let text = std::fs::read_to_string(&path).map_err(|_| UNREADABLE.to_string())?;
    parse(&text)
}

/// Edits take a turn in the order they were made on the UI thread, so the
/// writes cannot overtake one another when several run in the background.
#[derive(Clone, Copy)]
struct Turns {
    next: u64,
    running: u64,
}

static TURNS: Mutex<Turns> = Mutex::new(Turns {
    next: 0,
    running: 0,
});
static TURN_ADVANCED: Condvar = Condvar::new();

/// Takes the next turn. Call it on the UI thread, before the edit runs.
pub fn take_turn() -> u64 {
    let mut turns = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
    let turn = turns.next;
    turns.next += 1;
    turn
}

/// Advances the turn queue when dropped, even if the edit panics.
struct Finish;

impl Drop for Finish {
    fn drop(&mut self) {
        let mut turns = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
        turns.running += 1;
        TURN_ADVANCED.notify_all();
    }
}

/// Applies `edit` to the configuration file when it is `turn`, then asks
/// Gnoblin to reload it. The file is restored if Gnoblin rejects it.
pub fn apply(turn: u64, edit: Edit) -> Result<(), String> {
    {
        let mut turns = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
        while turns.running != turn {
            turns = TURN_ADVANCED
                .wait(turns)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
    let _finish = Finish;
    apply_now(edit)
}

fn apply_now(edit: Edit) -> Result<(), String> {
    let fake = fake_path();
    if fake.is_none() && fake_config().is_some() {
        return Ok(());
    }
    static WRITING: Mutex<()> = Mutex::new(());
    let _writing = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    let path = config_file()?;
    let path =
        std::fs::canonicalize(&path).map_or(path, |real| real.to_string_lossy().into_owned());
    let original = std::fs::read_to_string(&path).map_err(|_| UNREADABLE.to_string())?;
    let current = parse(&original)?;
    let next = current.apply(&edit)?;
    if next == current {
        return Ok(());
    }
    let updated = rewrite(&original, &next)?;
    write_file(&path, &updated)?;
    if fake.is_some() {
        // The fake file stands in for the configuration, so there is no Gnoblin to reload.
        return Ok(());
    }
    reload(&path, &original)
}

/// Asks Gnoblin to reload the configuration. When the desktop is not running
/// there is nothing to reload, and the file is kept as written.
fn reload(path: &str, original: &str) -> Result<(), String> {
    match gnoblin::gnoblinctl(&["config", "reload"]) {
        Ok(_) => Ok(()),
        Err(e) if e.0 == gnoblin::NOT_RUNNING => Ok(()),
        Err(e) => {
            let restored = write_file(path, original).is_ok();
            if restored {
                let _ = gnoblin::gnoblinctl(&["config", "reload"]);
            }
            Err(format!("Gnoblin did not accept the change: {}", e.0))
        }
    }
}

/// Writes through a temporary file beside the target, so a failed write
/// never leaves a half-written configuration.
fn write_file(path: &str, text: &str) -> Result<(), String> {
    let temp = format!("{path}.bingux-settings-tmp");
    std::fs::write(&temp, text).map_err(|_| UNWRITABLE.to_string())?;
    std::fs::rename(&temp, path).map_err(|_| UNWRITABLE.to_string())
}

/// The XKB layouts and layout variants in `evdev.xml`, with their descriptions.
/// Variants are named `layout+variant`, as Gnoblin names them. Empty when the
/// XKB database is not installed.
fn xkb_layouts() -> Vec<(Source, String)> {
    let Ok(rules) = std::fs::read_to_string(XKB_EVDEV) else {
        return Vec::new();
    };
    let mut layouts = Vec::new();
    let mut layout = String::new();
    let mut in_layouts = false;
    let mut in_variant = false;
    let mut name = String::new();
    let mut description = String::new();
    for line in rules.lines().map(str::trim) {
        match line {
            "<layoutList>" => in_layouts = true,
            "</layoutList>" => break,
            "<variant>" => in_variant = true,
            "</variant>" => in_variant = false,
            _ if !in_layouts => {}
            _ => {
                if let Some(text) = element(line, "name") {
                    name = unescape(text);
                    if !in_variant {
                        layout.clone_from(&name);
                    }
                } else if let Some(text) = element(line, "description") {
                    description = unescape(text);
                } else if line == "</configItem>" {
                    let id = if in_variant {
                        format!("{layout}+{name}")
                    } else {
                        name.clone()
                    };
                    let source = Source {
                        kind: "xkb".to_string(),
                        id,
                    };
                    layouts.push((source, description.clone()));
                }
            }
        }
    }
    layouts
}

/// The IBus engines that are installed, named by their engine id. Empty when
/// IBus is not running or not installed, so it never fails the catalogue.
fn ibus_engines() -> Vec<(Source, String)> {
    let Ok(output) = std::process::Command::new("ibus")
        .args(["list-engine", "--name-only"])
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|id| valid_id(id))
        .map(|id| {
            let source = Source {
                kind: "ibus".to_string(),
                id: id.to_string(),
            };
            (source, id.to_string())
        })
        .collect()
}

/// Every input source the catalogue knows, with its display name.
pub fn catalog() -> Vec<(Source, String)> {
    let mut sources = xkb_layouts();
    sources.extend(ibus_engines());
    sources
}

/// The text of `<tag>text</tag>` when `line` is exactly that element.
fn element<'a>(line: &'a str, tag: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("<{tag}>"))?
        .strip_suffix(&format!("</{tag}>"))
}

/// Decodes the XML entities that appear in the XKB descriptions.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn valid_source(source: &Source) -> bool {
    matches!(source.kind.as_str(), "xkb" | "ibus") && valid_id(&source.id)
}

/// Ids are written into the Lua file unescaped, so only plain characters are
/// allowed: letters, digits and the separators XKB and IBus use.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+_.:()-".contains(&b))
}

/// The Lua file a snapshot build reads and edits in place of the live
/// configuration. `BINGUX_SETTINGS_FAKE_INPUT` names it when it ends in `.lua`.
#[cfg(feature = "snapshot")]
fn fake_path() -> Option<String> {
    let value = std::env::var("BINGUX_SETTINGS_FAKE_INPUT").ok()?;
    value.ends_with(".lua").then_some(value)
}

#[cfg(not(feature = "snapshot"))]
fn fake_path() -> Option<String> {
    None
}

/// The configuration file to edit: the snapshot fixture when one is set,
/// otherwise Gnoblin's active configuration.
fn config_file() -> Result<String, String> {
    if let Some(path) = fake_path() {
        return Ok(path);
    }
    gnoblin::config_path().map_err(|e| e.0)
}

#[cfg(feature = "snapshot")]
fn fake_config() -> Option<SourcesConfig> {
    if let Some(path) = fake_path() {
        let text = std::fs::read_to_string(path).ok()?;
        return parse(&text).ok();
    }
    std::env::var_os("BINGUX_SETTINGS_FAKE_INPUT")?;
    let source = |id: &str| Source {
        kind: "xkb".to_string(),
        id: id.to_string(),
    };
    Some(SourcesConfig {
        sources: vec![source("us"), source("gb"), source("fr")],
        per_window: false,
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_config() -> Option<SourcesConfig> {
    None
}

/// Renders the `input_sources` table. `indent` is the indentation of the
/// line the key sits on; the entries go four spaces deeper.
fn render_table(config: &SourcesConfig, indent: &str) -> String {
    let mut out = String::from("input_sources = {\n");
    out.push_str(&format!("{indent}    sources = {{\n"));
    for source in &config.sources {
        out.push_str(&format!(
            "{indent}        {{type = \"{}\", id = \"{}\"}},\n",
            source.kind, source.id
        ));
    }
    out.push_str(&format!("{indent}    }},\n"));
    out.push_str(&format!(
        "{indent}    per_window = {},\n",
        config.per_window
    ));
    out.push_str(&format!("{indent}}}"));
    out
}

/// The text with the table replaced. An existing table is replaced in
/// place; otherwise it is added to the first `gnoblin.configure` table, or a
/// new one is appended to the file.
fn rewrite(text: &str, config: &SourcesConfig) -> Result<String, String> {
    let tokens = lex(text)?;
    let location = locate(&tokens)?;
    if let Some((key, _, close)) = location.key {
        let start = tokens[key].start;
        let end = tokens[close].end;
        let indent = line_indent(text, start);
        return Ok(format!(
            "{}{}{}",
            &text[..start],
            render_table(config, &indent),
            &text[end..]
        ));
    }
    if let Some(open) = location.table {
        let at = tokens[open].end;
        let inner = format!("{}    ", line_indent(text, tokens[open].start));
        return Ok(format!(
            "{}\n{}{},{}",
            &text[..at],
            inner,
            render_table(config, &inner),
            &text[at..]
        ));
    }
    Ok(format!(
        "{text}\n\ngnoblin.configure {{\n    {},\n}}\n",
        render_table(config, "    ")
    ))
}

/// The leading spaces and tabs of the line that holds byte `position`.
fn line_indent(text: &str, position: usize) -> String {
    let line_start = text[..position]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn parse(text: &str) -> Result<SourcesConfig, String> {
    let tokens = lex(text)?;
    let location = locate(&tokens)?;
    match location.key {
        None => Ok(SourcesConfig {
            sources: Vec::new(),
            per_window: false,
        }),
        Some((_, open, close)) => parse_table(&tokens, open, close),
    }
}

/// Where the `input_sources` table is. `key` is the key token, the table's
/// opening brace and its closing brace. `table` is the opening brace of the
/// first `gnoblin.configure` table, where a missing key would go.
struct Location {
    key: Option<(usize, usize, usize)>,
    table: Option<usize>,
}

/// Finds the last `input_sources` key among the `gnoblin.configure` tables,
/// since a later table overrides an earlier one.
fn locate(tokens: &[Token]) -> Result<Location, String> {
    let mut tables = Vec::new();
    let mut key = None;
    for index in 0..tokens.len() {
        let configure = is_ident(tokens, index, "gnoblin")
            && is_punct(tokens, index + 1, b'.')
            && is_ident(tokens, index + 2, "configure")
            && is_punct(tokens, index + 3, b'{');
        if !configure {
            continue;
        }
        let open = index + 3;
        let close = matching(tokens, open)?;
        tables.push(open);
        for (key_index, value) in top_keys(tokens, open, close) {
            if matches!(&tokens[key_index].tok, Tok::Ident(name) if name == "input_sources") {
                if !is_punct(tokens, value, b'{') {
                    return Err(UNSUPPORTED.to_string());
                }
                key = Some((key_index, value, matching(tokens, value)?));
            }
        }
    }
    Ok(Location {
        key,
        table: tables.first().copied(),
    })
}

/// The keys written at the top level of a table, with the index of the first
/// token of each value. Only tokens at depth zero count, so keys inside
/// nested tables and functions are skipped.
fn top_keys(tokens: &[Token], open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut depth = 0usize;
    for index in open + 1..close {
        match &tokens[index].tok {
            Tok::Punct(c) if is_open(*c) => depth += 1,
            Tok::Punct(c) if is_close(*c) => depth = depth.saturating_sub(1),
            Tok::Ident(_)
                if depth == 0
                    && is_punct(tokens, index + 1, b'=')
                    && (index == open + 1 || is_punct(tokens, index - 1, b',')) =>
            {
                found.push((index, index + 2));
            }
            _ => {}
        }
    }
    found
}

/// One `key = value` entry of a table: the key, and the token range of the
/// value, which ends before the separating comma or the closing brace.
struct Entry {
    key: String,
    start: usize,
    end: usize,
}

/// The entries of a table. Anything other than `key = value` entries
/// separated by commas is refused, so the table can be rewritten safely.
fn entries(tokens: &[Token], open: usize, close: usize) -> Result<Vec<Entry>, String> {
    let mut found = Vec::new();
    let mut index = open + 1;
    while index < close {
        if is_punct(tokens, index, b',') {
            index += 1;
            continue;
        }
        let Tok::Ident(key) = &tokens[index].tok else {
            return Err(UNSUPPORTED.to_string());
        };
        if !is_punct(tokens, index + 1, b'=') {
            return Err(UNSUPPORTED.to_string());
        }
        let start = index + 2;
        let mut end = start;
        let mut depth = 0usize;
        while end < close {
            match &tokens[end].tok {
                Tok::Punct(c) if is_open(*c) => depth += 1,
                Tok::Punct(c) if is_close(*c) => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| UNSUPPORTED.to_string())?;
                }
                Tok::Punct(b',') if depth == 0 => break,
                _ => {}
            }
            end += 1;
        }
        if end == start {
            return Err(UNSUPPORTED.to_string());
        }
        found.push(Entry {
            key: key.clone(),
            start,
            end,
        });
        index = end;
    }
    Ok(found)
}

/// The single token of a value, if the value is one token.
fn sole<'a>(tokens: &'a [Token], entry: &Entry) -> Option<&'a Tok> {
    (entry.end == entry.start + 1).then(|| &tokens[entry.start].tok)
}

fn parse_table(tokens: &[Token], open: usize, close: usize) -> Result<SourcesConfig, String> {
    let mut sources = Vec::new();
    let mut per_window = false;
    for entry in entries(tokens, open, close)? {
        match entry.key.as_str() {
            "sources" => sources = parse_sources(tokens, entry.start, entry.end)?,
            "per_window" => {
                per_window = match sole(tokens, &entry) {
                    Some(Tok::Ident(value)) if value == "true" => true,
                    Some(Tok::Ident(value)) if value == "false" => false,
                    _ => return Err(UNSUPPORTED.to_string()),
                }
            }
            _ => return Err(UNSUPPORTED.to_string()),
        }
    }
    Ok(SourcesConfig {
        sources,
        per_window,
    })
}

/// The list of source tables in `start..end`, which must be one table.
fn parse_sources(tokens: &[Token], start: usize, end: usize) -> Result<Vec<Source>, String> {
    if !is_punct(tokens, start, b'{') || matching(tokens, start)? + 1 != end {
        return Err(UNSUPPORTED.to_string());
    }
    let close = end - 1;
    let mut found = Vec::new();
    let mut index = start + 1;
    while index < close {
        if is_punct(tokens, index, b',') {
            index += 1;
            continue;
        }
        if !is_punct(tokens, index, b'{') {
            return Err(UNSUPPORTED.to_string());
        }
        let inner_close = matching(tokens, index)?;
        if inner_close >= close {
            return Err(UNSUPPORTED.to_string());
        }
        found.push(parse_source(tokens, index, inner_close)?);
        index = inner_close + 1;
    }
    Ok(found)
}

/// One source table, `{type = "xkb", id = "us"}`, with exactly those keys.
fn parse_source(tokens: &[Token], open: usize, close: usize) -> Result<Source, String> {
    let mut kind = None;
    let mut id = None;
    for entry in entries(tokens, open, close)? {
        let Some(Tok::Str(value)) = sole(tokens, &entry) else {
            return Err(UNSUPPORTED.to_string());
        };
        match entry.key.as_str() {
            "type" => kind = Some(value.clone()),
            "id" => id = Some(value.clone()),
            _ => return Err(UNSUPPORTED.to_string()),
        }
    }
    let (Some(kind), Some(id)) = (kind, id) else {
        return Err(UNSUPPORTED.to_string());
    };
    let source = Source { kind, id };
    if !valid_source(&source) {
        return Err(UNSUPPORTED.to_string());
    }
    Ok(source)
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(u8),
    Other,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    start: usize,
    end: usize,
}

fn is_punct(tokens: &[Token], index: usize, c: u8) -> bool {
    matches!(tokens.get(index), Some(Token { tok: Tok::Punct(p), .. }) if *p == c)
}

fn is_ident(tokens: &[Token], index: usize, name: &str) -> bool {
    matches!(tokens.get(index), Some(Token { tok: Tok::Ident(n), .. }) if n == name)
}

fn is_open(c: u8) -> bool {
    matches!(c, b'{' | b'(' | b'[')
}

fn is_close(c: u8) -> bool {
    matches!(c, b'}' | b')' | b']')
}

/// The index of the token that closes the bracket at `open`.
fn matching(tokens: &[Token], open: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if let Tok::Punct(c) = token.tok {
            if is_open(c) {
                depth += 1;
            } else if is_close(c) {
                depth -= 1;
                if depth == 0 {
                    return Ok(index);
                }
            }
        }
    }
    Err(UNSUPPORTED.to_string())
}

/// The level of a long bracket `[==[` starting at `at`, or `None`.
fn long_bracket(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let level = bytes[at + 1..].iter().take_while(|b| **b == b'=').count();
    (bytes.get(at + 1 + level) == Some(&b'[')).then_some(level)
}

/// The position of the `]==]` that closes a long bracket of `level` from `from`.
fn long_close(bytes: &[u8], from: usize, level: usize) -> Option<usize> {
    (from..bytes.len()).find(|&at| {
        bytes[at] == b']'
            && bytes[at + 1..].iter().take_while(|b| **b == b'=').count() == level
            && bytes.get(at + 1 + level) == Some(&b']')
    })
}

/// Splits Lua source into tokens, dropping comments and whitespace. Only the
/// token kinds the table needs are kept apart; everything else is `Other`.
fn lex(text: &str) -> Result<Vec<Token>, String> {
    let bytes = text.as_bytes();
    let unterminated = || UNSUPPORTED.to_string();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let c = bytes[at];
        if c.is_ascii_whitespace() {
            at += 1;
        } else if bytes[at..].starts_with(b"--") {
            at += 2;
            if let Some(level) = long_bracket(bytes, at) {
                let close = long_close(bytes, at + level + 2, level).ok_or_else(unterminated)?;
                at = close + level + 2;
            } else {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
        } else if let Some(level) = long_bracket(bytes, at) {
            let body = at + level + 2;
            let close = long_close(bytes, body, level).ok_or_else(unterminated)?;
            let value =
                String::from_utf8(bytes[body..close].to_vec()).map_err(|_| unterminated())?;
            let end = close + level + 2;
            tokens.push(Token {
                tok: Tok::Str(value),
                start: at,
                end,
            });
            at = end;
        } else if c == b'"' || c == b'\'' {
            let start = at;
            at += 1;
            let mut value = Vec::new();
            loop {
                match bytes.get(at) {
                    None | Some(b'\n') => return Err(unterminated()),
                    Some(&b) if b == c => {
                        at += 1;
                        break;
                    }
                    Some(b'\\') => {
                        let escaped = *bytes.get(at + 1).ok_or_else(unterminated)?;
                        value.push(match escaped {
                            b'n' => b'\n',
                            b't' => b'\t',
                            other => other,
                        });
                        at += 2;
                    }
                    Some(&b) => {
                        value.push(b);
                        at += 1;
                    }
                }
            }
            let value = String::from_utf8(value).map_err(|_| unterminated())?;
            tokens.push(Token {
                tok: Tok::Str(value),
                start,
                end: at,
            });
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            tokens.push(Token {
                tok: Tok::Ident(text[start..at].to_string()),
                start,
                end: at,
            });
        } else if c.is_ascii_digit() {
            let start = at;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' || bytes[at] == b'.')
            {
                at += 1;
            }
            tokens.push(Token {
                tok: Tok::Other,
                start,
                end: at,
            });
        } else if c == b'=' && bytes.get(at + 1) == Some(&b'=') {
            tokens.push(Token {
                tok: Tok::Other,
                start: at,
                end: at + 2,
            });
            at += 2;
        } else {
            let tok = if matches!(
                c,
                b'{' | b'}' | b'(' | b')' | b'[' | b']' | b',' | b'=' | b'.' | b';'
            ) {
                Tok::Punct(c)
            } else {
                Tok::Other
            };
            tokens.push(Token {
                tok,
                start: at,
                end: at + 1,
            });
            at += 1;
        }
    }
    Ok(tokens)
}
