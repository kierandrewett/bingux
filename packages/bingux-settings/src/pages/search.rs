//! Search: which providers run, the website engines and the default one, the
//! folders file search indexes, file previews and the optional AI assistant.
//! Backend: `bingux-config`. Reads come from `dump`, writes use `set` and
//! `unset`, and every write is followed by a reload.

use std::path::Path;
use std::time::Duration;

use freya::prelude::*;

use crate::icons::Icon;
use crate::pages::bingux_config;
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Overlays, ResponseKind, Row, Status, choice_row, dialog, icon_button, page_column,
    primary_button, row, skeleton, slider_row, status, switch, switch_row, text_field,
    use_overlays,
};
use bingux_ui::scale::*;

const TIMEOUT: Duration = Duration::from_secs(5);
const PROVIDERS: [(&str, &str); 7] = [
    ("applications", "Applications"),
    ("files", "Files"),
    ("calculation", "Calculation"),
    ("conversions", "Conversions"),
    ("web", "Web"),
    ("web-shortcuts", "Web shortcuts"),
    ("external", "External"),
];
const MIN_MEGABYTES: i64 = 1;
const MAX_MEGABYTES: i64 = 20;

#[derive(Clone, PartialEq)]
struct Engine {
    id: String,
    name: String,
    shortcut: String,
    url: String,
    enabled: bool,
}

#[derive(Clone, PartialEq)]
struct Ai {
    harness: String,
    model: String,
    executable: Option<String>,
}

#[derive(Clone, PartialEq)]
struct Model {
    disabled_providers: Vec<String>,
    default_engine: String,
    engines: Vec<Engine>,
    ai: Option<Ai>,
    file_roots: Option<Vec<String>>,
    previews_enabled: bool,
    prewarm: bool,
    max_megabytes: i64,
}

#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
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

fn missing(key: &str) -> CommandError {
    CommandError(format!("bingux-config did not report {key}."))
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
        .map_err(|e| CommandError(format!("Could not read the search settings: {e}.")))?;
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
    })
}

fn load() -> Result<Model, CommandError> {
    let dump = system::run(&bingux_config(), &["dump"], TIMEOUT)?;
    parse_dump(&dump)
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
    background(ctx.model, load);
}

/// Runs one write in the background and reports whether it succeeded. The
/// error state is set or cleared, but nothing reloads.
fn write_then(ctx: Ctx, args: Vec<String>, after: impl FnOnce(bool) + 'static) {
    background_then(
        move || run_config(args),
        move |result| match result {
            Ok(_) => {
                let mut error = ctx.error;
                error.set(None);
                after(true);
            }
            Err(e) => {
                let mut error = ctx.error;
                error.set(Some(e.0));
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
        let mut error = ctx.error;
        error.set(Some("At least one engine must stay enabled.".into()));
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
        let model = system::cached("search");
        let error = use_state::<Option<String>>(|| None);
        let overlays = use_overlays();
        let ctx = Ctx {
            model,
            error,
            overlays,
        };

        use_hook(move || reload(ctx));

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton()).into_element();
        };

        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column()
                    .child(status(Status::Error(e.0)).action("Try again", move |_| reload(ctx)))
                    .into_element();
            }
        };

        let mut page = page_column();

        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        page = page
            .child(providers_group(&data, ctx))
            .child(web_group(&data, ctx))
            .child(AddEngine {
                engines: data.engines.clone(),
                ctx,
            })
            .child(files_group(&data, ctx))
            .child(FileRootAdd {
                roots: data.file_roots.clone().unwrap_or_default(),
                ctx,
            })
            .child(previews_group(&data, ctx))
            .child(assistant_group(&data, ctx));

        page.into_element()
    }
}

/// A form's action row: the button sits at the right, on the row height.
fn form_footer(button: impl IntoElement) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(ROW_HEIGHT))
        .direction(Direction::Horizontal)
        .main_align(Alignment::End)
        .cross_align(Alignment::Center)
        .padding((0.0, GUTTER))
        .child(button)
        .into()
}

fn providers_group(data: &Model, ctx: Ctx) -> Group {
    let disabled = data.disabled_providers.clone();
    PROVIDERS
        .iter()
        .fold(Group::titled("Providers"), |group, (id, label)| {
            let id = id.to_string();
            let enabled = !disabled.contains(&id);
            let current = disabled.clone();
            group.row(switch_row(*label, enabled, move |on: bool| {
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
            }))
        })
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
    group
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
    let trailing = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .child(switch(engine.enabled, move |_: bool| {
            write_engines(ctx, &default_for_toggle, toggled.clone());
        }))
        .child(
            rect().margin((0.0, 0.0, 0.0, S2)).child(
                icon_button(Icon::Delete, move |_| {
                    let default = default_for_remove.clone();
                    let without = without.clone();
                    ctx.overlays.show_dialog(
                        dialog(
                            format!("Remove {}?", engine_name),
                            "Its shortcut stops working. You can add the engine again later.",
                        )
                        .cancel("Cancel")
                        .response(
                            "Remove",
                            ResponseKind::Destructive,
                            move |_| write_engines(ctx, &default, without.clone()),
                        ),
                    );
                })
                .tooltip("Remove engine"),
            ),
        );
    row(engine.name.clone())
        .subtitle(format!("Shortcut {} · {}", engine.shortcut, engine.url))
        .trailing(trailing)
}

/// Form for a new engine. Writes the full array with the new engine appended.
#[derive(PartialEq)]
struct AddEngine {
    engines: Vec<Engine>,
    ctx: Ctx,
}

impl Component for AddEngine {
    fn render(&self) -> impl IntoElement {
        let name = use_state(String::new);
        let shortcut = use_state(String::new);
        let url = use_state(String::new);
        let engines = self.engines.clone();
        let ctx = self.ctx;

        let add = move |_| {
            let mut name = name;
            let mut shortcut = shortcut;
            let mut url = url;
            let name_text = name.read().trim().to_string();
            let shortcut_text = shortcut.read().trim().to_string();
            let url_text = url.read().trim().to_string();
            if name_text.is_empty() || shortcut_text.is_empty() || url_text.is_empty() {
                let mut error = ctx.error;
                error.set(Some(
                    "Enter a name, a shortcut and a URL for the engine.".into(),
                ));
                return;
            }
            let mut list = engines.clone();
            list.push(Engine {
                id: engine_id(&shortcut_text),
                name: name_text,
                shortcut: shortcut_text,
                url: url_text,
                enabled: true,
            });
            write_then(
                ctx,
                set_args("search.engines", engines_toml(&list)),
                move |ok| {
                    if ok {
                        name.set(String::new());
                        shortcut.set(String::new());
                        url.set(String::new());
                        reload(ctx);
                    }
                },
            );
        };

        Group::titled("Add engine")
            .description("The shortcut uses lowercase letters, digits and hyphens. The URL must contain {query} once.")
            .row(row("Name").trailing(text_field(name, |_: String| {}).width(FIELD_WIDTH * 2.0)))
            .row(row("Shortcut").trailing(text_field(shortcut, |_: String| {}).width(FIELD_WIDTH * 2.0)))
            .row(
                row("URL").trailing(
                    text_field(url, |_: String| {})
                        .width(FIELD_WIDTH * 2.0)
                        .placeholder("https://example.com/?q={query}"),
                ),
            )
            .row(form_footer(primary_button("Add", add).compact()))
    }
}

fn files_group(data: &Model, ctx: Ctx) -> Group {
    let mut group = Group::titled("File search").description(
        "Files in these folders appear in search results. With none listed, the home folders are searched.",
    );
    match &data.file_roots {
        None => {
            group = group.row(row("Folders").value("Built-in home folders"));
        }
        Some(roots) => {
            for root in roots {
                let remaining: Vec<String> =
                    roots.iter().filter(|item| *item != root).cloned().collect();
                let folder = root.clone();
                let trailing = rect().child(
                    icon_button(Icon::Delete, move |_| {
                        let remaining = remaining.clone();
                        ctx.overlays.show_dialog(
                            dialog(
                                format!("Stop searching {folder}?"),
                                "Files in this folder no longer appear in search results.",
                            )
                            .cancel("Cancel")
                            .response(
                                "Remove",
                                ResponseKind::Destructive,
                                move |_| {
                                    let args = if remaining.is_empty() {
                                        vec!["unset".into(), "search.file_roots".into()]
                                    } else {
                                        set_args("search.file_roots", string_array(&remaining))
                                    };
                                    commit(ctx, args);
                                },
                            ),
                        );
                    })
                    .tooltip("Remove folder"),
                );
                group = group.row(row(root.clone()).trailing(trailing));
            }
        }
    }
    group
}

/// Adds an absolute, existing directory to the file search roots.
#[derive(PartialEq)]
struct FileRootAdd {
    roots: Vec<String>,
    ctx: Ctx,
}

impl Component for FileRootAdd {
    fn render(&self) -> impl IntoElement {
        let path = use_state(String::new);
        let roots = self.roots.clone();
        let ctx = self.ctx;

        let add = EventHandler::new(move |_: ()| {
            let mut path = path;
            let text = path.read().trim().to_string();
            let mut error = ctx.error;
            if !Path::new(&text).is_absolute() || !Path::new(&text).is_dir() {
                error.set(Some(
                    "Enter an absolute path to an existing folder, such as /home/you/Documents."
                        .into(),
                ));
                return;
            }
            if roots.contains(&text) {
                error.set(Some(format!("{text} is already searched.")));
                return;
            }
            let mut list = roots.clone();
            list.push(text);
            write_then(
                ctx,
                set_args("search.file_roots", string_array(&list)),
                move |ok| {
                    if ok {
                        path.set(String::new());
                        reload(ctx);
                    }
                },
            );
        });
        let submit = add.clone();

        Group::titled("Add folder")
            .description("An absolute path to an existing folder.")
            .row(
                row("Path").trailing(
                    text_field(path, move |_: String| submit.call(()))
                        .width(FIELD_WIDTH * 2.0)
                        .placeholder("/home/user/Documents"),
                ),
            )
            .row(form_footer(primary_button("Add", add).compact()))
    }
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
            "Largest previewed file",
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
        .subtitle(format!("{shown} MB"))
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
        .description("The model is passed to the harness. Leave it empty for the harness default.")
        .row(choice_row(
            "Harness",
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
            .width(FIELD_WIDTH * 2.0),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("search", load);
}
