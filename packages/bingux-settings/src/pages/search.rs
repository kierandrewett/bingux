//! Search: which providers run, the website engines and the default one, the
//! folders file search indexes, file previews and the optional AI assistant.
//! Backend: `bingux-config`. Reads come from `dump`, writes use `set` and
//! `unset`, and every write is followed by a reload.

use std::path::Path;
use std::time::Duration;

use freya::prelude::*;

use crate::icons::Icon;
use crate::pages::bingux_config;
use crate::pages::loading::veiled;
use crate::system::{
    self, CommandError, CommandResult, background, background_then, optimistic_then,
};
use crate::ui::{
    Group, Overlays, PageHead, PopupAction, PopupActions, ResponseKind, Row, Status, Window,
    caption, choice_row, dialog, form_row, icon_button, page_column, popup, popup_column,
    primary_button, row, slider_row, status, switch, switch_row, text_field,
    use_overlays, use_popup_actions, use_tokens, use_window,
};
use bingux_ui::shell::menu_item;
use bingux_ui::style::*;
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

const TIMEOUT: Duration = Duration::from_secs(5);
const PROVIDERS: [(&str, &str, Icon); 7] = [
    ("applications", "Applications", Icon::Applications),
    ("files", "Files", Icon::Folder),
    ("calculation", "Calculation", Icon::AccessoriesCalculator),
    ("conversions", "Conversions", Icon::Refresh),
    ("web", "Web", Icon::WebBrowser),
    ("web-shortcuts", "Website shortcuts", Icon::Bookmarks),
    ("external", "Add-on sources", Icon::AddOn),
];
const MIN_MEGABYTES: i64 = 1;
const MAX_MEGABYTES: i64 = 20;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Engine {
    id: String,
    name: String,
    shortcut: String,
    url: String,
    enabled: bool,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Ai {
    harness: String,
    model: String,
    executable: Option<String>,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    disabled_providers: Vec<String>,
    default_engine: String,
    engines: Vec<Engine>,
    ai: Option<Ai>,
    file_roots: Option<Vec<String>>,
    previews_enabled: bool,
    prewarm: bool,
    max_megabytes: i64,
    recursive_search: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
}

/// A parsed JSON value. Only what `bingux-config dump` produces is supported.
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser {
            chars: text.chars().collect(),
            pos: 0,
        };
        let value = parser.value()?;
        parser.skip_ws();
        if parser.pos != parser.chars.len() {
            return Err("unexpected text after the value".into());
        }
        Ok(value)
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items.as_slice()),
            _ => None,
        }
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let mut fields = Vec::new();
                self.skip_ws();
                if self.peek() == Some('}') {
                    self.pos += 1;
                    return Ok(Json::Object(fields));
                }
                loop {
                    let key = self.string()?;
                    self.skip_ws();
                    if self.bump() != Some(':') {
                        return Err("expected ':'".into());
                    }
                    fields.push((key, self.value()?));
                    self.skip_ws();
                    match self.bump() {
                        Some(',') => {}
                        Some('}') => return Ok(Json::Object(fields)),
                        _ => return Err("expected ',' or '}'".into()),
                    }
                }
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == Some(']') {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.skip_ws();
                    match self.bump() {
                        Some(',') => {}
                        Some(']') => return Ok(Json::Array(items)),
                        _ => return Err("expected ',' or ']'".into()),
                    }
                }
            }
            Some('"') => self.string().map(Json::Text),
            Some('t') => self.literal("true", Json::Bool(true)),
            Some('f') => self.literal("false", Json::Bool(false)),
            Some('n') => self.literal("null", Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            _ => Err("unexpected character".into()),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        for expected in word.chars() {
            if self.bump() != Some(expected) {
                return Err(format!("expected {word}"));
            }
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || "+-.eE".contains(c)) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("bad number {text}"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0;
        for _ in 0..4 {
            let digit = self
                .bump()
                .and_then(|c| c.to_digit(16))
                .ok_or("bad unicode escape")?;
            code = code * 16 + digit;
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<String, String> {
        self.skip_ws();
        if self.bump() != Some('"') {
            return Err("expected a string".into());
        }
        let mut out = String::new();
        loop {
            match self.bump().ok_or("unterminated string")? {
                '"' => return Ok(out),
                '\\' => match self.bump().ok_or("unterminated escape")? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let code = self.hex4()?;
                        let scalar = if (0xD800..0xDC00).contains(&code) {
                            if self.bump() != Some('\\') || self.bump() != Some('u') {
                                return Err("unpaired surrogate".into());
                            }
                            let low = self.hex4()?;
                            0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF)
                        } else {
                            code
                        };
                        out.push(char::from_u32(scalar).ok_or("bad unicode escape")?);
                    }
                    _ => return Err("bad escape".into()),
                },
                c => out.push(c),
            }
        }
    }
}

fn missing(_key: &str) -> CommandError {
    CommandError("Some search settings could not be read.".into())
}

fn json_text(object: &Json, key: &str) -> Result<String, CommandError> {
    object
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| missing(key))
}

fn parse_engine(object: &Json) -> Result<Engine, CommandError> {
    Ok(Engine {
        id: json_text(object, "id")?,
        name: json_text(object, "name")?,
        shortcut: json_text(object, "shortcut")?,
        url: json_text(object, "url")?,
        enabled: object
            .get("enabled")
            .and_then(Json::as_bool)
            .ok_or_else(|| missing("enabled"))?,
    })
}

fn strings(value: Option<&Json>) -> Vec<String> {
    value
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads the `search` and `previews` sections from `bingux-config dump`.
fn parse_dump(text: &str) -> Result<Model, CommandError> {
    let root = Json::parse(text)
        .map_err(|_| CommandError("The search settings could not be read.".into()))?;
    if let Some(message) = root.get("error").and_then(Json::as_str) {
        return Err(CommandError(message.to_string()));
    }
    let data = root.get("data").ok_or_else(|| missing("data"))?;
    let search = data.get("search").ok_or_else(|| missing("search"))?;
    let previews = data.get("previews").ok_or_else(|| missing("previews"))?;

    let engines = search
        .get("engines")
        .and_then(Json::as_array)
        .ok_or_else(|| missing("search.engines"))?
        .iter()
        .map(parse_engine)
        .collect::<Result<Vec<_>, _>>()?;

    let ai = match search.get("ai") {
        Some(object @ Json::Object(_)) => Some(Ai {
            harness: json_text(object, "harness")?,
            model: object
                .get("model")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            executable: object
                .get("executable")
                .and_then(Json::as_str)
                .map(str::to_string),
        }),
        _ => None,
    };

    let file_roots = match search.get("fileRoots") {
        Some(Json::Array(_)) => Some(strings(search.get("fileRoots"))),
        _ => None,
    };

    let max_megabytes = match previews.get("maxMegabytes") {
        Some(Json::Number(value)) => value.round() as i64,
        _ => return Err(missing("previews.maxMegabytes")),
    };

    Ok(Model {
        disabled_providers: strings(search.get("disabledProviders")),
        default_engine: json_text(search, "defaultEngine")?,
        engines,
        ai,
        file_roots,
        previews_enabled: previews
            .get("enabled")
            .and_then(Json::as_bool)
            .ok_or_else(|| missing("previews.enabled"))?,
        prewarm: previews
            .get("prewarm")
            .and_then(Json::as_bool)
            .ok_or_else(|| missing("previews.prewarm"))?,
        max_megabytes,
        recursive_search: None,
    })
}

/// Loads the search settings again and saves the result for the next start.
fn refresh() -> Result<Model, CommandError> {
    system::load_persistent("search", load)
}

fn load() -> Result<Model, CommandError> {
    let dump = system::run(&bingux_config(), &["dump"], TIMEOUT)?;
    let mut model = parse_dump(&dump)?;
    model.recursive_search = recursive_search();
    Ok(model)
}

const NAUTILUS: &str = "org.gnome.nautilus.preferences";
const RECURSIVE_SEARCH: &str = "recursive-search";
const RECURSIVE_SEARCH_OPTIONS: [(&str, &str); 3] = [
    ("always", "Always"),
    ("local-only", "Local folders only"),
    ("never", "Never"),
];

/// Nautilus's `recursive-search`, read with `gsettings`. `None` without
/// gsettings or with a value the page does not offer, which hides the row.
fn recursive_search() -> Option<String> {
    let value = system::gsettings_get(NAUTILUS, RECURSIVE_SEARCH).ok()?;
    let value = value.trim().trim_matches('\'').to_string();
    RECURSIVE_SEARCH_OPTIONS
        .iter()
        .any(|(id, _)| *id == value)
        .then_some(value)
}

/// Shows `value` at once, writes it with `gsettings` and reloads. A failed
/// write puts the previous value back with a toast.
fn set_recursive_search(ctx: Ctx, value: String) {
    let previous = ctx.model.read().clone();
    let Some(Ok(current)) = previous.clone() else {
        return;
    };
    let mut shown = current;
    shown.recursive_search = Some(value.clone());
    let quoted = format!("'{value}'");
    optimistic_then(
        ctx.model,
        Some(ctx.overlays),
        previous,
        Some(Ok(shown)),
        move || system::gsettings_set(NAUTILUS, RECURSIVE_SEARCH, &quoted).map(|_| ()),
        move || reload(ctx),
    );
}

/// Quotes `value` as a TOML basic string.
fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\x7f' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn string_array(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| toml_string(item)).collect();
    format!("[{}]", quoted.join(", "))
}

fn engines_toml(engines: &[Engine]) -> String {
    let tables: Vec<String> = engines
        .iter()
        .map(|engine| {
            format!(
                "{{id = {}, name = {}, shortcut = {}, url = {}, enabled = {}}}",
                toml_string(&engine.id),
                toml_string(&engine.name),
                toml_string(&engine.shortcut),
                toml_string(&engine.url),
                engine.enabled
            )
        })
        .collect();
    format!("[{}]", tables.join(", "))
}

fn ai_toml(ai: &Ai) -> String {
    let mut fields = vec![format!("harness = {}", toml_string(&ai.harness))];
    if !ai.model.is_empty() {
        fields.push(format!("model = {}", toml_string(&ai.model)));
    }
    if let Some(executable) = ai.executable.as_ref().filter(|path| !path.is_empty()) {
        fields.push(format!("executable = {}", toml_string(executable)));
    }
    format!("{{{}}}", fields.join(", "))
}

/// Lowercase identifier from a shortcut: letters and digits kept, the rest hyphens.
fn engine_id(shortcut: &str) -> String {
    shortcut
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn set_args(key: &str, value: String) -> Vec<String> {
    vec!["set".into(), key.into(), value]
}

fn run_config(args: Vec<String>) -> CommandResult {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    system::run(&bingux_config(), &refs, TIMEOUT)
}

fn reload(ctx: Ctx) {
    background(ctx.model, refresh);
}

/// Runs one write in the background and reports whether it succeeded. A
/// failure is a toast; nothing reloads.
fn write_then(ctx: Ctx, args: Vec<String>, after: impl FnOnce(bool) + 'static) {
    background_then(
        move || run_config(args),
        move |result| match result {
            Ok(_) => after(true),
            Err(e) => {
                ctx.overlays.toast(e.0);
                after(false);
            }
        },
    );
}

/// Runs one write, then reloads the settings if it succeeded.
fn commit(ctx: Ctx, args: Vec<String>) {
    write_then(ctx, args, move |ok| {
        if ok {
            reload(ctx);
        }
    });
}

/// Replaces the whole engines array. `bingux-config` rejects a default engine
/// that is missing or disabled, so when `list` drops the default, the default
/// moves to the first enabled engine before the array is written.
fn write_engines(ctx: Ctx, default: &str, list: Vec<Engine>) {
    if list
        .iter()
        .any(|engine| engine.enabled && engine.id == default)
    {
        commit(ctx, set_args("search.engines", engines_toml(&list)));
        return;
    }
    let Some(next) = list
        .iter()
        .find(|engine| engine.enabled)
        .map(|engine| engine.id.clone())
    else {
        ctx.overlays.toast("At least one engine must stay enabled.");
        return;
    };
    write_then(
        ctx,
        set_args("search.default_engine", toml_string(&next)),
        move |ok| {
            if ok {
                commit(ctx, set_args("search.engines", engines_toml(&list)));
            }
        },
    );
}

fn megabytes_from_percent(percent: f64) -> i64 {
    let fraction = percent.clamp(0.0, 100.0) / 100.0;
    let value = MIN_MEGABYTES as f64 + fraction * (MAX_MEGABYTES - MIN_MEGABYTES) as f64;
    (value.round() as i64).clamp(MIN_MEGABYTES, MAX_MEGABYTES)
}

fn percent_from_megabytes(megabytes: i64) -> f64 {
    let fraction = (megabytes - MIN_MEGABYTES) as f64 / (MAX_MEGABYTES - MIN_MEGABYTES) as f64;
    fraction * 100.0
}

/// Writes `value` and, if the slider moved meanwhile, writes the newest value
/// next. Only one write runs at a time, so the last write wins.
fn save_megabytes(ctx: Ctx, draft: State<i64>, mut busy: State<bool>, value: i64) {
    busy.set(true);
    write_then(
        ctx,
        set_args("previews.max_megabytes", value.to_string()),
        move |ok| {
            busy.set(false);
            if ok {
                reload(ctx);
                let latest = *draft.read();
                if latest != value {
                    save_megabytes(ctx, draft, busy, latest);
                }
            }
        },
    );
}

#[derive(PartialEq)]
pub struct SearchPage;

impl Component for SearchPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached_persistent("search", load);
        let overlays = use_overlays();
        let ctx = Ctx { model, overlays };

        // Snapshot only: BINGUX_SETTINGS_SNAPSHOT_OPEN=remove-engine or
        // remove-folder asks that question once the page has loaded, so the
        // dialog can be captured.
        #[cfg(feature = "snapshot")]
        use_hook(move || {
            let which = std::env::var("BINGUX_SETTINGS_SNAPSHOT_OPEN").unwrap_or_default();
            if which == "remove-engine" || which == "remove-folder" {
                spawn(async move {
                    timer(std::time::Duration::from_millis(2500)).await;
                    let Some(Ok(data)) = model.read().clone() else {
                        return;
                    };
                    if which == "remove-engine" {
                        if let Some(engine) = data.engines.first() {
                            let without = &data.engines[1..];
                            confirm_remove(ctx, &engine.name, &data.default_engine, without);
                        }
                    } else {
                        // With no folders listed, ask about a sample one.
                        let roots = data.file_roots.clone().unwrap_or_default();
                        let folder = roots
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "/home/kieran/Documents".to_string());
                        confirm_remove_folder(ctx, folder, roots.iter().skip(1).cloned().collect());
                    }
                });
            }
        });

        let loaded = model.read().clone();
        let loading = loaded.is_none();
        let loaded = loaded.unwrap_or_else(|| Ok(placeholder()));
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(e.0)).action("Try again", move |_| reload(ctx)))
                    .into_element();
            }
        };

        let mut page = page_column().child(PageHead::new());
        page = page
            .child(providers_group(&data, ctx))
            .child(web_group(&data, ctx))
            .child(files_group(&data, ctx))
            .child(previews_group(&data, ctx))
            .child(assistant_group(&data, ctx));

        veiled(page, loading).into_element()
    }
}

/// The page with the shipped defaults, drawn under the veil while the first
/// read runs: the default engine, no listed folders and the assistant row.
fn placeholder() -> Model {
    Model {
        disabled_providers: Vec::new(),
        default_engine: "duckduckgo".to_string(),
        engines: vec![Engine {
            id: "duckduckgo".to_string(),
            name: "DuckDuckGo".to_string(),
            shortcut: "ddg".to_string(),
            url: "https://duckduckgo.com/?q={query}".to_string(),
            enabled: true,
        }],
        ai: Some(Ai {
            harness: "none".to_string(),
            model: String::new(),
            executable: None,
        }),
        file_roots: None,
        previews_enabled: true,
        prewarm: false,
        max_megabytes: 20,
        recursive_search: None,
    }
}

fn providers_group(data: &Model, ctx: Ctx) -> Group {
    let disabled = data.disabled_providers.clone();
    PROVIDERS.iter().fold(
        Group::titled("Search sources"),
        |group, (id, label, icon)| {
            let id = id.to_string();
            let enabled = !disabled.contains(&id);
            let current = disabled.clone();
            group.row(
                switch_row(*label, enabled, move |on: bool| {
                    let mut next: Vec<String> = current
                        .iter()
                        .filter(|provider| provider.as_str() != id)
                        .cloned()
                        .collect();
                    if !on {
                        next.push(id.clone());
                    }
                    commit(
                        ctx,
                        set_args("search.disabled_providers", string_array(&next)),
                    );
                })
                .leading(*icon),
            )
        },
    )
}

fn web_group(data: &Model, ctx: Ctx) -> Group {
    let enabled_engines: Vec<(String, String)> = data
        .engines
        .iter()
        .filter(|engine| engine.enabled)
        .map(|engine| (engine.id.clone(), engine.name.clone()))
        .collect();
    let mut group = Group::titled("Web search").row(choice_row(
        "Default engine",
        enabled_engines,
        &data.default_engine,
        move |id: String| {
            commit(ctx, set_args("search.default_engine", toml_string(&id)));
        },
    ));
    for engine in &data.engines {
        group = group.row(engine_row(
            engine.clone(),
            &data.engines,
            &data.default_engine,
            ctx,
        ));
    }
    group.row(AddEngine {
        engines: data.engines.clone(),
        ctx,
    })
}

fn engine_row(engine: Engine, engines: &[Engine], default: &str, ctx: Ctx) -> Row {
    let toggled: Vec<Engine> = engines
        .iter()
        .map(|item| {
            if item.id == engine.id {
                Engine {
                    enabled: !item.enabled,
                    ..item.clone()
                }
            } else {
                item.clone()
            }
        })
        .collect();
    let without: Vec<Engine> = engines
        .iter()
        .filter(|item| item.id != engine.id)
        .cloned()
        .collect();
    let default_for_toggle = default.to_string();
    let default_for_remove = default.to_string();
    let engine_name = engine.name.clone();
    let is_default = engine.id == default;
    let enabled = engine.enabled;
    let set_id = engine.id.clone();
    let toggle_default = default.to_string();
    let toggle_list = toggled.clone();
    let remove_name = engine.name.clone();
    let remove_default = default.to_string();
    let remove_list = without.clone();
    let menu = vec![
        menu_item("Set as default", move |_| {
            commit(ctx, set_args("search.default_engine", toml_string(&set_id)));
        })
        .checked(is_default),
        menu_item(if enabled { "Turn off" } else { "Turn on" }, move |_| {
            write_engines(ctx, &toggle_default, toggle_list.clone());
        }),
        menu_item("Remove", move |_| {
            confirm_remove(ctx, &remove_name, &remove_default, &remove_list);
        })
        .destructive(),
    ];
    let trailing = rect()
        .tw_layout(&[flex_row(), items(Align::Center), gap(S2)])
        .child(switch(engine.enabled, move |_: bool| {
            write_engines(ctx, &default_for_toggle, toggled.clone());
        }))
        .child(
            rect().child(
                icon_button(Icon::Delete, move |_| {
                    confirm_remove(ctx, &engine_name, &default_for_remove, &without);
                })
                .tooltip("Remove engine"),
            ),
        );
    row(engine.name.clone())
        .leading(Icon::WebBrowser)
        .subtitle(format!("Shortcut {}", engine.shortcut))
        .trailing(trailing)
        .context_menu(menu)
}

/// Asks before removing an engine, then writes the engines without it.
fn confirm_remove(ctx: Ctx, name: &str, default: &str, without: &[Engine]) {
    let default = default.to_string();
    let without = without.to_vec();
    ctx.overlays.show_dialog(
        dialog(
            format!("Remove {name}?"),
            "Its shortcut stops working. You can add the engine again later.",
        )
        .cancel("Cancel")
        .response("Remove", ResponseKind::Destructive, move |_| {
            write_engines(ctx, &default, without.clone())
        }),
    );
}

/// The add-engine row. Its popup writes the full engines array with the new engine appended.
#[derive(PartialEq)]
struct AddEngine {
    engines: Vec<Engine>,
    ctx: Ctx,
}

impl Component for AddEngine {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let form = EngineForm {
            name: use_state(String::new),
            shortcut: use_state(String::new),
            url: use_state(String::new),
            ctx: self.ctx,
        };
        let engines = self.engines.clone();
        // Snapshot only: BINGUX_SETTINGS_SNAPSHOT_OPEN=add-engine opens the
        // add popup once the page has loaded, so it can be captured.
        #[cfg(feature = "snapshot")]
        {
            let engines = self.engines.clone();
            use_hook(move || {
                if std::env::var("BINGUX_SETTINGS_SNAPSHOT_OPEN").is_ok_and(|v| v == "add-engine") {
                    spawn(async move {
                        timer(std::time::Duration::from_millis(2500)).await;
                        open_engine_sheet(window, form, engines);
                    });
                }
            });
        }
        let open = move |_| {
            let (mut name, mut shortcut, mut url) = (form.name, form.shortcut, form.url);
            name.set(String::new());
            shortcut.set(String::new());
            url.set(String::new());
            open_engine_sheet(window, form, engines.clone());
        };
        row("Add a search engine…").trailing(primary_button("Add", open).compact())
    }
}

/// The add-engine fields. Copy, so the row, the popup body and the Add action share them.
#[derive(Clone, Copy, PartialEq)]
struct EngineForm {
    name: State<String>,
    shortcut: State<String>,
    url: State<String>,
    ctx: Ctx,
}

/// Why a shortcut or web address cannot be used. Empty fields are not problems: Add stays off until they are filled in.
#[derive(Default)]
struct EngineProblems {
    shortcut: Option<&'static str>,
    url: Option<&'static str>,
}

fn engine_fields(form: EngineForm) -> (String, String, String) {
    (
        form.name.read().trim().to_string(),
        form.shortcut.read().trim().to_string(),
        form.url.read().trim().to_string(),
    )
}

fn engine_problems(shortcut: &str, url: &str, engines: &[Engine]) -> EngineProblems {
    let shortcut_problem = if shortcut.chars().any(char::is_whitespace) {
        Some("A shortcut has no spaces.")
    } else if !shortcut.is_empty()
        && engines
            .iter()
            .any(|engine| engine.id == engine_id(shortcut))
    {
        Some("Another engine uses this shortcut.")
    } else {
        None
    };
    let url_problem = if url.is_empty() {
        None
    } else if !(url.starts_with("https://") || url.starts_with("http://")) {
        Some("Start the address with https:// or http://.")
    } else if !url.contains("{query}") {
        Some("Put {query} where the search words go.")
    } else {
        None
    };
    EngineProblems {
        shortcut: shortcut_problem,
        url: url_problem,
    }
}

fn engine_ready(name: &str, shortcut: &str, url: &str, engines: &[Engine]) -> bool {
    let problems = engine_problems(shortcut, url, engines);
    !name.is_empty()
        && !shortcut.is_empty()
        && !url.is_empty()
        && problems.shortcut.is_none()
        && problems.url.is_none()
}

/// Opens the add-engine popup. Its body reads the form, so the Add button follows the fields.
fn open_engine_sheet(window: Window, form: EngineForm, engines: Vec<Engine>) {
    window.open(popup("Add a search engine", EngineBody { form, engines }).flags(PopupFlags::FORM));
}

/// Writes the engines with the new one appended. The popup closes only once the write succeeds.
fn add_engine(form: EngineForm, engines: Vec<Engine>, window: Window) {
    let (name, shortcut, url) = engine_fields(form);
    if !engine_ready(&name, &shortcut, &url, &engines) {
        return;
    }
    let mut list = engines;
    list.push(Engine {
        id: engine_id(&shortcut),
        name,
        shortcut,
        url,
        enabled: true,
    });
    let ctx = form.ctx;
    write_then(
        ctx,
        set_args("search.engines", engines_toml(&list)),
        move |ok| {
            if ok {
                window.close();
                reload(ctx);
            }
        },
    );
}

/// The add-engine popup body. It reads the form state, so the fields and problems redraw as they change.
#[derive(Clone, PartialEq)]
struct EngineBody {
    form: EngineForm,
    engines: Vec<Engine>,
}

impl Component for EngineBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let form = self.form;
        let engines = self.engines.clone();
        let (name, shortcut, url) = engine_fields(form);
        let problems = engine_problems(&shortcut, &url, &self.engines);
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Add".into(),
                enabled: engine_ready(&name, &shortcut, &url, &self.engines),
                on_press: EventHandler::new(move |_: ()| add_engine(form, engines.clone(), window)),
            }),
        });
        let fields = Group::new(None::<String>)
            .row(form_row(
                "Name",
                rect().tw(&tokens, &[w(Dim::Field)]).child(
                    text_field(form.name, |_: String| {})
                        .width(Dim::Field)
                        .placeholder("Example Search"),
                ),
            ))
            .row(form_row(
                "Shortcut",
                rect().tw(&tokens, &[w(Dim::Field)]).child(
                    text_field(form.shortcut, |_: String| {})
                        .width(Dim::Field)
                        .placeholder("ex"),
                ),
            ))
            .row(form_row(
                "Web address",
                rect().tw(&tokens, &[w(Dim::Field)]).child(
                    text_field(form.url, |_: String| {})
                        .width(Dim::Field)
                        .placeholder("https://example.com/?q={query}"),
                ),
            ));
        // The problem replaces the hint under the fields, so a field never
        // reserves a line of its own and stays level with its label.
        let hint = problems.shortcut.or(problems.url).unwrap_or(
            "Type the shortcut in search to use this engine. Put {query} in the address where the search words go.",
        );
        popup_column().child(fields).child(caption(hint))
    }
}

fn files_group(data: &Model, ctx: Ctx) -> Group {
    let mut group = Group::titled("File search").description(
        "Files in these folders appear in search results. With none listed, the home folders are searched.",
    );
    if let Some(recursive) = &data.recursive_search {
        let options = RECURSIVE_SEARCH_OPTIONS
            .iter()
            .map(|(id, label)| (id.to_string(), label.to_string()))
            .collect();
        group = group.row(choice_row(
            "Search subfolders",
            options,
            recursive,
            move |value: String| set_recursive_search(ctx, value),
        ));
    }
    match &data.file_roots {
        None => {
            group = group.row(row("Folders").value("Home folders"));
        }
        Some(roots) => {
            for root in roots {
                let remaining: Vec<String> =
                    roots.iter().filter(|item| *item != root).cloned().collect();
                let folder = root.clone();
                let menu = vec![
                    menu_item("Stop searching", {
                        let (folder, remaining) = (folder.clone(), remaining.clone());
                        move |_| confirm_remove_folder(ctx, folder.clone(), remaining.clone())
                    })
                    .destructive(),
                ];
                let trailing = rect().child(
                    icon_button(Icon::Delete, move |_| {
                        confirm_remove_folder(ctx, folder.clone(), remaining.clone());
                    })
                    .tooltip("Remove folder"),
                );
                group = group.row(
                    row(folder_name(root))
                        .subtitle(folder_location(root))
                        .trailing(trailing)
                        .context_menu(menu),
                );
            }
        }
    }
    group.row(FileRootAdd {
        roots: data.file_roots.clone().unwrap_or_default(),
        ctx,
    })
}

/// Asks before a folder leaves the file search roots. Removing the last one restores the home folders.
fn confirm_remove_folder(ctx: Ctx, folder: String, remaining: Vec<String>) {
    ctx.overlays.show_dialog(
        dialog(
            format!("Stop searching {}?", folder_name(&folder)),
            "Files in this folder no longer appear in search results.",
        )
        .cancel("Cancel")
        .response("Remove", ResponseKind::Destructive, move |_| {
            let args = if remaining.is_empty() {
                vec!["unset".into(), "search.file_roots".into()]
            } else {
                set_args("search.file_roots", string_array(&remaining))
            };
            commit(ctx, args);
        }),
    );
}

/// Adds a folder to the file search roots. The folder is chosen with the
/// Bingux file chooser.
#[derive(PartialEq)]
struct FileRootAdd {
    roots: Vec<String>,
    ctx: Ctx,
}

impl Component for FileRootAdd {
    fn render(&self) -> impl IntoElement {
        let roots = self.roots.clone();
        let ctx = self.ctx;
        let window = use_window();

        let choose = move |_| {
            let roots = roots.clone();
            crate::ui::files::choose(
                window,
                crate::ui::files::ChooserRequest::select_folder().title("Choose a folder"),
                move |chosen| {
                    let Some(path) = chosen.and_then(|chosen| chosen.paths.into_iter().next())
                    else {
                        return;
                    };
                    let path = path.to_string_lossy().into_owned();
                    background_then(
                        // The folder check stats the disk, so it runs here, not on the UI thread.
                        move || (searchable(&path), path),
                        move |(searchable, path)| {
                            if searchable {
                                add_folder(ctx, &roots, path);
                            } else {
                                ctx.overlays.toast("That folder cannot be searched.");
                            }
                        },
                    );
                },
            );
        };

        row("Add a folder").trailing(primary_button("Choose folder", choose).compact())
    }
}

/// True for an absolute path to a folder. Call from a background task: it reads the disk.
fn searchable(path: &str) -> bool {
    Path::new(path).is_absolute() && Path::new(path).is_dir()
}

/// Adds a checked folder to the file search roots.
fn add_folder(ctx: Ctx, roots: &[String], path: String) {
    if roots.contains(&path) {
        ctx.overlays
            .toast(format!("{} is already searched.", folder_name(&path)));
        return;
    }
    let mut list = roots.to_vec();
    list.push(path);
    write_then(
        ctx,
        set_args("search.file_roots", string_array(&list)),
        move |ok| {
            if ok {
                reload(ctx);
            }
        },
    );
}

/// The last part of a folder path, such as `Documents`.
fn folder_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(
        || path.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The folder that holds a folder. The home folder is shortened to `Home`.
fn folder_location(path: &str) -> String {
    let Some(parent) = Path::new(path).parent() else {
        return String::new();
    };
    let parent = parent.to_string_lossy().into_owned();
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        if parent == home {
            return "Home".into();
        }
        if let Some(rest) = parent.strip_prefix(format!("{home}/").as_str()) {
            return format!("Home/{rest}");
        }
    }
    parent
}

fn previews_group(data: &Model, ctx: Ctx) -> Group {
    Group::titled("Previews")
        .row(switch_row(
            "Preview files",
            data.previews_enabled,
            move |on: bool| {
                commit(ctx, set_args("previews.enabled", on.to_string()));
            },
        ))
        .row(switch_row(
            "Prepare nearby previews",
            data.prewarm,
            move |on: bool| {
                commit(ctx, set_args("previews.prewarm", on.to_string()));
            },
        ))
        .row(MaxMegabytes {
            current: data.max_megabytes,
            ctx,
        })
}

/// Largest previewed file. The slider drags a local draft; writes are serialized.
#[derive(PartialEq)]
struct MaxMegabytes {
    current: i64,
    ctx: Ctx,
}

impl Component for MaxMegabytes {
    fn render(&self) -> impl IntoElement {
        let draft = use_state(|| self.current);
        let busy = use_state(|| false);
        let ctx = self.ctx;
        let shown = *draft.read();

        slider_row(
            "Largest file to preview",
            percent_from_megabytes(shown),
            move |percent: f64| {
                let value = megabytes_from_percent(percent);
                let mut draft = draft;
                draft.set(value);
                if !*busy.read() {
                    save_megabytes(ctx, draft, busy, value);
                }
            },
        )
        .value_text(format!("{shown} MB"))
    }
}

fn assistant_group(data: &Model, ctx: Ctx) -> Group {
    let selected = data
        .ai
        .as_ref()
        .map(|ai| ai.harness.clone())
        .unwrap_or_else(|| "none".into());
    let current = data.ai.clone();
    let options = vec![
        ("none".to_string(), "None".to_string()),
        ("pi".to_string(), "Pi".to_string()),
        ("claude".to_string(), "Claude".to_string()),
    ];
    let mut group = Group::titled("Assistant")
        .description(
            "Choose the program Bingux asks. Leave Model empty to use the program's own choice.",
        )
        .row(choice_row(
            "Assistant program",
            options,
            &selected,
            move |harness: String| {
                if harness == "none" {
                    commit(ctx, vec!["unset".into(), "search.ai".into()]);
                    return;
                }
                // Keep the model when switching; an executable only fits its own harness.
                let next = match &current {
                    Some(ai) if ai.harness == harness => Ai {
                        harness: harness.clone(),
                        ..ai.clone()
                    },
                    Some(ai) => Ai {
                        harness: harness.clone(),
                        model: ai.model.clone(),
                        executable: None,
                    },
                    None => Ai {
                        harness: harness.clone(),
                        model: String::new(),
                        executable: None,
                    },
                };
                commit(ctx, set_args("search.ai", ai_toml(&next)));
            },
        ));
    if let Some(ai) = data.ai.clone() {
        group = group.row(AssistantModel { ai, ctx });
    }
    group
}

/// The model name for the assistant. Writes on Enter.
#[derive(PartialEq)]
struct AssistantModel {
    ai: Ai,
    ctx: Ctx,
}

impl Component for AssistantModel {
    fn render(&self) -> impl IntoElement {
        let draft = use_state(|| self.ai.model.clone());
        let ai = self.ai.clone();
        let ctx = self.ctx;

        row("Model").trailing(
            text_field(draft, move |text: String| {
                let next = Ai {
                    model: text.trim().to_string(),
                    ..ai.clone()
                };
                commit(ctx, set_args("search.ai", ai_toml(&next)));
            })
            .width(Dim::FieldWide),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("search", load);
}
