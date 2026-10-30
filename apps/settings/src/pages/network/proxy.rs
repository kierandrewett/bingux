//! The system proxy, through gsettings `org.gnome.system.proxy`: the keys
//! GNOME Settings, GLib and libproxy read, so every application that follows
//! the desktop proxy follows this page.
//!
//! The method (None, Manual, Automatic) applies at once. The text fields
//! (hosts, ports, ignore list, configuration URL) apply together through the
//! pending bar, only while they are valid, because a half-typed port must never
//! reach the system.

use freya::prelude::*;

use super::model::NetCtx;
use crate::routes;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, PageHead, Status, choice_row, form_field, page_column, pending_bar, row, skeleton_rows, status,
    switch_row, use_overlays,
};
use bingux_ui::controls::form::validate;

const SCHEMA: &str = "org.gnome.system.proxy";
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

// ---------------------------------------------------------------------------
// gsettings values
// ---------------------------------------------------------------------------

/// Read one quoted GVariant string from `chars`, positioned on its opening
/// quote. gsettings prints `'text'`, or `"text"` when the text holds a single
/// quote; a backslash escapes the next character.
fn read_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let quote = chars.next().filter(|c| matches!(c, '\'' | '"'))?;
    let mut text = String::new();
    loop {
        match chars.next()? {
            '\\' => match chars.next()? {
                'n' => text.push('\n'),
                't' => text.push('\t'),
                other => text.push(other),
            },
            c if c == quote => return Some(text),
            c => text.push(c),
        }
    }
}

/// A gsettings string value (`'manual'`, `@s ''`) as plain text.
fn parse_string(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_prefix("@s").unwrap_or(value).trim_start();
    read_quoted(&mut value.chars().peekable()).unwrap_or_default()
}

/// A gsettings string list (`['a', 'b']`, `@as []`) as plain strings.
fn parse_string_list(value: &str) -> Vec<String> {
    let value = value.trim();
    let value = value.strip_prefix("@as").unwrap_or(value).trim();
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Vec::new();
    };
    let mut chars = inner.chars().peekable();
    let mut items = Vec::new();
    while let Some(&next) = chars.peek() {
        if next.is_whitespace() || next == ',' {
            chars.next();
            continue;
        }
        match read_quoted(&mut chars) {
            Some(item) => items.push(item),
            None => break,
        }
    }
    items
}

/// A gsettings integer (`9090`, `uint32 8080`, `int32 -1`).
fn parse_int(value: &str) -> Option<i64> {
    value.split_whitespace().last()?.parse().ok()
}

/// A string as a GVariant literal: single quotes, with `\` and `'` escaped.
fn quote(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// A string list as a GVariant literal.
fn quote_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| quote(item)).collect();
    format!("[{}]", quoted.join(", "))
}

// ---------------------------------------------------------------------------
// The settings
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    None,
    Manual,
    Auto,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Manual => "Manual",
            Self::Auto => "Automatic",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "manual" => Self::Manual,
            "auto" => Self::Auto,
            _ => Self::None,
        }
    }
}

/// One protocol's proxy server. A port of 0 means none is set.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Endpoint {
    host: String,
    port: u16,
}

/// Everything the Proxy page reads and writes.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ProxyConfig {
    mode: Mode,
    /// `use-same-proxy`: the HTTP proxy serves every protocol.
    same: bool,
    http: Endpoint,
    https: Endpoint,
    /// `None` when this system's schema has no FTP proxy.
    ftp: Option<Endpoint>,
    socks: Endpoint,
    ignore: Vec<String>,
    pac: String,
}

/// The `(schema, key, value)` rows of `gsettings list-recursively`.
fn parse_dump(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            Some((
                parts.next()?.to_string(),
                parts.next()?.to_string(),
                parts.next()?.to_string(),
            ))
        })
        .collect()
}

impl ProxyConfig {
    fn from_dump(text: &str) -> Self {
        let entries = parse_dump(text);
        let find = |schema: &str, key: &str| {
            entries
                .iter()
                .find(|(s, k, _)| s == schema && k == key)
                .map(|(_, _, value)| value.as_str())
        };
        let endpoint = |child: &str| -> Option<Endpoint> {
            let schema = format!("{SCHEMA}.{child}");
            let host = find(&schema, "host")?;
            let port = find(&schema, "port").and_then(parse_int).unwrap_or(0);
            Some(Endpoint {
                host: parse_string(host),
                port: u16::try_from(port).unwrap_or(0),
            })
        };
        Self {
            mode: Mode::from_key(&parse_string(find(SCHEMA, "mode").unwrap_or_default())),
            same: find(SCHEMA, "use-same-proxy").is_some_and(|v| v.trim() == "true"),
            http: endpoint("http").unwrap_or_default(),
            https: endpoint("https").unwrap_or_default(),
            ftp: endpoint("ftp"),
            socks: endpoint("socks").unwrap_or_default(),
            ignore: parse_string_list(find(SCHEMA, "ignore-hosts").unwrap_or_default()),
            pac: parse_string(find(SCHEMA, "autoconfig-url").unwrap_or_default()),
        }
    }
}

fn load() -> Result<ProxyConfig, CommandError> {
    system::run("gsettings", &["list-recursively", SCHEMA], TIMEOUT)
        .map(|text| ProxyConfig::from_dump(&text))
}

type Model = State<Option<Result<ProxyConfig, CommandError>>>;

/// The proxy settings, shared by the Network page's row and this page so a
/// change shows in both at once.
fn model() -> Model {
    thread_local! {
        static MODEL: Model = State::create_global(None);
    }
    MODEL.with(|model| *model)
}

fn reload(model: Model) {
    background_then(load, move |loaded| {
        let mut model = model;
        model.set(Some(loaded));
    });
}

// ---------------------------------------------------------------------------
// The form draft
// ---------------------------------------------------------------------------

/// What the text fields hold: the part of the settings that applies together.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Draft {
    same: bool,
    http: (String, String),
    https: (String, String),
    ftp: (String, String),
    socks: (String, String),
    ignore: String,
    pac: String,
}

fn fields_of(endpoint: &Endpoint) -> (String, String) {
    let port = if endpoint.port == 0 {
        String::new()
    } else {
        endpoint.port.to_string()
    };
    (endpoint.host.clone(), port)
}

/// One endpoint's fields are valid when a host comes with a port, and any
/// port that is typed is a real one.
fn endpoint_error(host: &str, port: &str) -> Option<String> {
    if !host.is_empty() {
        if let Err(message) = validate::hostname_or_ip(host) {
            return Some(message);
        }
        if port.is_empty() {
            return Some("Enter a port number from 1 to 65535.".to_string());
        }
    }
    if !port.is_empty() {
        return validate::port(port).err();
    }
    None
}

/// Snapshot builds can show the inline messages without a person leaving the
/// field (`BINGUX_SETTINGS_SHOW_FIELD_ERRORS=1`), because a capture cannot move focus.
#[cfg(feature = "snapshot")]
fn show_errors_now() -> bool {
    std::env::var_os("BINGUX_SETTINGS_SHOW_FIELD_ERRORS").is_some()
}

#[cfg(not(feature = "snapshot"))]
fn show_errors_now() -> bool {
    false
}

/// The ignore list, split on commas and line breaks.
fn split_ignore(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// A message when an ignore entry has a space inside it.
fn ignore_error(text: &str) -> Option<String> {
    split_ignore(text)
        .iter()
        .any(|entry| entry.chars().any(char::is_whitespace))
        .then(|| "Separate hosts with commas. A host has no spaces.".to_string())
}

fn pac_error(text: &str) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        validate::url(text).err()
    }
}

fn endpoint_of(fields: &(String, String)) -> Endpoint {
    Endpoint {
        host: fields.0.clone(),
        port: fields.1.parse().unwrap_or(0),
    }
}

impl Draft {
    fn from_config(config: &ProxyConfig) -> Self {
        Self {
            same: config.same,
            http: fields_of(&config.http),
            https: fields_of(&config.https),
            ftp: config.ftp.as_ref().map(fields_of).unwrap_or_default(),
            socks: fields_of(&config.socks),
            ignore: config.ignore.join(", "),
            pac: config.pac.clone(),
        }
    }

    /// The endpoints the current method and switch show, in page order.
    fn visible<'a>(&'a self, config: &ProxyConfig) -> Vec<&'a (String, String)> {
        let mut list = vec![&self.http];
        if !self.same {
            list.push(&self.https);
            if config.ftp.is_some() {
                list.push(&self.ftp);
            }
            list.push(&self.socks);
        }
        list
    }

    /// Whether every field that applies is valid.
    fn valid(&self, config: &ProxyConfig) -> bool {
        match config.mode {
            Mode::None => true,
            Mode::Auto => pac_error(&self.pac).is_none(),
            Mode::Manual => {
                self.visible(config)
                    .iter()
                    .all(|(host, port)| endpoint_error(host, port).is_none())
                    && ignore_error(&self.ignore).is_none()
            }
        }
    }

    /// Whether the visible fields differ from what is applied.
    fn dirty(&self, config: &ProxyConfig) -> bool {
        let saved = Self::from_config(config);
        match config.mode {
            Mode::None => false,
            Mode::Auto => self.pac != saved.pac,
            Mode::Manual => {
                self.same != saved.same
                    || split_ignore(&self.ignore) != config.ignore
                    || self
                        .visible(config)
                        .iter()
                        .zip(saved.visible(config))
                        .any(|(mine, theirs)| *mine != theirs)
            }
        }
    }

    /// The settings after applying this draft.
    fn applied(&self, config: &ProxyConfig) -> ProxyConfig {
        let mut next = config.clone();
        match config.mode {
            Mode::None => {}
            Mode::Auto => next.pac = self.pac.clone(),
            Mode::Manual => {
                next.same = self.same;
                next.http = endpoint_of(&self.http);
                if !self.same {
                    next.https = endpoint_of(&self.https);
                    if next.ftp.is_some() {
                        next.ftp = Some(endpoint_of(&self.ftp));
                    }
                    next.socks = endpoint_of(&self.socks);
                }
                next.ignore = split_ignore(&self.ignore);
            }
        }
        next
    }
}

/// The gsettings writes that turn `before` into `after`: only the keys that
/// changed, as `(schema, key, GVariant text)`.
fn writes(before: &ProxyConfig, after: &ProxyConfig) -> Vec<(String, &'static str, String)> {
    let mut list = Vec::new();
    let mut add = |schema: &str, key: &'static str, changed: bool, value: String| {
        if changed {
            list.push((schema.to_string(), key, value));
        }
    };
    add(
        SCHEMA,
        "mode",
        before.mode != after.mode,
        quote(after.mode.key()),
    );
    add(
        SCHEMA,
        "use-same-proxy",
        before.same != after.same,
        after.same.to_string(),
    );
    add(
        SCHEMA,
        "autoconfig-url",
        before.pac != after.pac,
        quote(&after.pac),
    );
    add(
        SCHEMA,
        "ignore-hosts",
        before.ignore != after.ignore,
        quote_list(&after.ignore),
    );
    let mut endpoint = |child: &str, old: &Endpoint, new: &Endpoint| {
        let schema = format!("{SCHEMA}.{child}");
        add(&schema, "host", old.host != new.host, quote(&new.host));
        add(&schema, "port", old.port != new.port, new.port.to_string());
    };
    endpoint("http", &before.http, &after.http);
    endpoint("https", &before.https, &after.https);
    if let (Some(old), Some(new)) = (&before.ftp, &after.ftp) {
        endpoint("ftp", old, new);
    }
    endpoint("socks", &before.socks, &after.socks);
    list
}

/// Write `after` over `before`, stopping at the first failure.
fn write_all(before: ProxyConfig, after: ProxyConfig) -> Result<(), CommandError> {
    for (schema, key, value) in writes(&before, &after) {
        system::gsettings_set(&schema, key, &value)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The Network page row
// ---------------------------------------------------------------------------

/// The Proxy row of the More group.
pub fn proxy_row(ctx: NetCtx) -> Element {
    ProxyRow { ctx }.into()
}

#[derive(PartialEq)]
struct ProxyRow {
    ctx: NetCtx,
}

impl Component for ProxyRow {
    fn render(&self) -> impl IntoElement {
        let model = model();
        use_hook(move || reload(model));
        let router = self.ctx.router;
        let value = match &*model.read() {
            Some(Ok(config)) => config.mode.label(),
            _ => "",
        };
        row("Proxy")
            .value(value)
            .navigates()
            .on_press(move |_| router.open(routes::proxy()))
    }
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct ProxyPage;

impl Component for ProxyPage {
    fn render(&self) -> impl IntoElement {
        let model = model();
        use_hook(move || reload(model));
        let loaded = model.read().clone();
        match loaded {
            None => page_column()
                .child(PageHead::new())
                .child(status(Status::Loading("Reading proxy settings.".into()))),
            Some(Err(error)) => page_column()
                .child(PageHead::new())
                .child(status(Status::Error(error.0)).action("Try again", move |_| reload(model))),
            Some(Ok(config)) => page_column().child(ProxyForm { initial: config }),
        }
    }
}

#[derive(PartialEq)]
struct ProxyForm {
    /// The settings when the page opened; only seeds the fields.
    initial: ProxyConfig,
}

impl Component for ProxyForm {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let model = model();
        let seed = Draft::from_config(&self.initial);
        let same = use_state(|| seed.same);
        let http_host = use_state(|| seed.http.0.clone());
        let http_port = use_state(|| seed.http.1.clone());
        let https_host = use_state(|| seed.https.0.clone());
        let https_port = use_state(|| seed.https.1.clone());
        let ftp_host = use_state(|| seed.ftp.0.clone());
        let ftp_port = use_state(|| seed.ftp.1.clone());
        let socks_host = use_state(|| seed.socks.0.clone());
        let socks_port = use_state(|| seed.socks.1.clone());
        let ignore = use_state(|| seed.ignore.clone());
        let pac = use_state(|| seed.pac.clone());

        let Some(Ok(config)) = model.read().clone() else {
            return page_column();
        };
        let draft = Draft {
            same: *same.read(),
            http: (http_host.read().clone(), http_port.read().clone()),
            https: (https_host.read().clone(), https_port.read().clone()),
            ftp: (ftp_host.read().clone(), ftp_port.read().clone()),
            socks: (socks_host.read().clone(), socks_port.read().clone()),
            ignore: ignore.read().clone(),
            pac: pac.read().clone(),
        };
        let dirty = draft.dirty(&config);
        let valid = draft.valid(&config);

        let apply = {
            let config = config.clone();
            let draft = draft.clone();
            move |_| {
                let next = draft.applied(&config);
                let before = config.clone();
                let mut shown = model;
                shown.set(Some(Ok(next.clone())));
                background_then(
                    move || write_all(before.clone(), next).map_err(|e| (e, before)),
                    move |result| {
                        if let Err((error, before)) = result {
                            let mut shown = model;
                            shown.set(Some(Ok(before)));
                            overlays.toast(error.0);
                        }
                    },
                );
            }
        };
        let revert = {
            let config = config.clone();
            move |_| {
                let saved = Draft::from_config(&config);
                let (mut same, mut ignore, mut pac) = (same, ignore, pac);
                same.set(saved.same);
                ignore.set(saved.ignore);
                pac.set(saved.pac);
                for (mut host, mut port, fields) in [
                    (http_host, http_port, saved.http),
                    (https_host, https_port, saved.https),
                    (ftp_host, ftp_port, saved.ftp),
                    (socks_host, socks_port, saved.socks),
                ] {
                    host.set(fields.0);
                    port.set(fields.1);
                }
            }
        };

        let method = {
            let config = config.clone();
            choice_row(
                "Method",
                [Mode::None, Mode::Manual, Mode::Auto]
                    .into_iter()
                    .map(|mode| (mode.key().to_string(), mode.label().to_string()))
                    .collect(),
                config.mode.key(),
                move |key: String| {
                    let mut next = config.clone();
                    next.mode = Mode::from_key(&key);
                    let before = config.clone();
                    let write = (before.clone(), next.clone());
                    system::optimistic_then(
                        model,
                        Some(overlays),
                        Some(Ok(before)),
                        Some(Ok(next)),
                        move || write_all(write.0, write.1),
                        || {},
                    );
                },
            )
        };

        let mut page = page_column()
            .child(PageHead::new())
            .child(pending_bar(dirty, valid, apply, revert))
            .child(Group::new(None::<String>).row(method));

        match config.mode {
            Mode::None => {}
            Mode::Auto => {
                page = page.child(
                    Group::titled("Automatic configuration").row(
                        form_field("Configuration URL", pac)
                            .placeholder("http://wpad.example.com/proxy.pac")
                            .hint("Leave empty to find the configuration automatically.")
                            .validate(|text| validate::url(text))
                            .optional(),
                    ),
                );
            }
            Mode::Manual => {
                let endpoint_group = |title: &str, host: State<String>, port: State<String>| {
                    Group::titled(title)
                        .row(
                            form_field("Host", host)
                                .optional()
                                .validate(validate::hostname_or_ip),
                        )
                        .row(
                            form_field("Port", port).validate(validate::port).error(
                                show_errors_now()
                                    .then(|| endpoint_error("", &port.read()))
                                    .flatten(),
                            ),
                        )
                };
                let http = endpoint_group("HTTP proxy", http_host, http_port).row(switch_row(
                    "Use this proxy for every protocol",
                    draft.same,
                    move |on: bool| {
                        let mut same = same;
                        same.set(on);
                    },
                ));
                page = page.child(http);
                if !draft.same {
                    page = page.child(endpoint_group("HTTPS proxy", https_host, https_port));
                    if config.ftp.is_some() {
                        page = page.child(endpoint_group("FTP proxy", ftp_host, ftp_port));
                    }
                    page = page.child(endpoint_group("SOCKS host", socks_host, socks_port));
                }
                page = page.child(
                    Group::titled("Ignore hosts").row(
                        form_field("Hosts", ignore)
                            .hint(
                                "Addresses that skip the proxy. Separate them with commas. \
                                 Networks can be written like 192.168.0.0/16.",
                            )
                            .monospace()
                            .multiline(3)
                            .validate(|text| ignore_error(text).map_or(Ok(()), Err)),
                    ),
                );
            }
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "\
org.gnome.system.proxy autoconfig-url ''
org.gnome.system.proxy ignore-hosts ['localhost', '127.0.0.0/8', '::1']
org.gnome.system.proxy mode 'manual'
org.gnome.system.proxy use-same-proxy false
org.gnome.system.proxy.ftp host ''
org.gnome.system.proxy.ftp port 0
org.gnome.system.proxy.http host 'proxy.example.com'
org.gnome.system.proxy.http port 3128
org.gnome.system.proxy.https host '127.0.0.1'
org.gnome.system.proxy.https port 9090
org.gnome.system.proxy.socks host ''
org.gnome.system.proxy.socks port 0
";

    #[test]
    fn strings_unquote() {
        assert_eq!(parse_string("'manual'"), "manual");
        assert_eq!(parse_string("''"), "");
        assert_eq!(parse_string("@s ''"), "");
        assert_eq!(parse_string(r"'it\'s'"), "it's");
        assert_eq!(parse_string("\"it's\""), "it's");
        assert_eq!(parse_string(r"'a\\b'"), r"a\b");
    }

    #[test]
    fn strings_quote_and_round_trip() {
        assert_eq!(quote("auto"), "'auto'");
        assert_eq!(quote("it's"), r"'it\'s'");
        assert_eq!(quote(r"a\b"), r"'a\\b'");
        for text in [
            "",
            "plain",
            "it's",
            r"a\b",
            "two\nlines",
            "http://h/p?x='1'",
        ] {
            assert_eq!(parse_string(&quote(text)), text);
        }
    }

    #[test]
    fn lists_parse_and_quote() {
        assert_eq!(
            parse_string_list("['localhost', '127.0.0.0/8', '::1']"),
            ["localhost", "127.0.0.0/8", "::1"]
        );
        assert!(parse_string_list("@as []").is_empty());
        assert!(parse_string_list("[]").is_empty());
        assert_eq!(parse_string_list("['a,b', \"c'd\"]"), ["a,b", "c'd"]);
        assert_eq!(quote_list(&[]), "[]");
        let items = vec!["a,b".to_string(), "c'd".to_string()];
        assert_eq!(parse_string_list(&quote_list(&items)), items);
    }

    #[test]
    fn ints_parse() {
        assert_eq!(parse_int("9090"), Some(9090));
        assert_eq!(parse_int("uint32 8080"), Some(8080));
        assert_eq!(parse_int("int32 -1"), Some(-1));
        assert_eq!(parse_int("'x'"), None);
    }

    #[test]
    fn dump_reads_every_key() {
        let config = ProxyConfig::from_dump(DUMP);
        assert_eq!(config.mode, Mode::Manual);
        assert!(!config.same);
        assert_eq!(config.http.host, "proxy.example.com");
        assert_eq!(config.http.port, 3128);
        assert_eq!(config.https.port, 9090);
        assert_eq!(config.ftp, Some(Endpoint::default()));
        assert_eq!(config.ignore, ["localhost", "127.0.0.0/8", "::1"]);
    }

    #[test]
    fn missing_ftp_schema_is_none() {
        let dump: String = DUMP
            .lines()
            .filter(|line| !line.contains(".ftp "))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(ProxyConfig::from_dump(&dump).ftp, None);
    }

    #[test]
    fn draft_validity_and_changes() {
        let config = ProxyConfig::from_dump(DUMP);
        let mut draft = Draft::from_config(&config);
        assert!(!draft.dirty(&config));
        assert!(draft.valid(&config));

        draft.http.1 = "99999".into();
        assert!(draft.dirty(&config));
        assert!(!draft.valid(&config));

        draft.http.1 = "8080".into();
        assert!(draft.valid(&config));
        let next = draft.applied(&config);
        assert_eq!(
            writes(&config, &next),
            vec![(
                "org.gnome.system.proxy.http".to_string(),
                "port",
                "8080".to_string()
            )]
        );
    }

    #[test]
    fn host_needs_port_and_empty_is_allowed() {
        assert!(endpoint_error("", "").is_none());
        assert!(endpoint_error("proxy.example.com", "").is_some());
        assert!(endpoint_error("proxy.example.com", "3128").is_none());
        assert!(endpoint_error("bad host", "3128").is_some());
    }

    #[test]
    fn ignore_list_splits_on_commas_and_lines() {
        assert_eq!(
            split_ignore("localhost, 10.0.0.0/8\n::1,,"),
            ["localhost", "10.0.0.0/8", "::1"]
        );
        assert!(ignore_error("a b").is_some());
        assert!(ignore_error("a, b").is_none());
    }

    #[test]
    fn same_proxy_ignores_hidden_endpoints() {
        let mut config = ProxyConfig::from_dump(DUMP);
        config.same = true;
        let mut draft = Draft::from_config(&config);
        draft.https.1 = "bad".into();
        assert!(draft.valid(&config));
        assert!(!draft.dirty(&config));
    }

    #[test]
    fn mode_change_writes_only_the_mode() {
        let before = ProxyConfig::from_dump(DUMP);
        let mut after = before.clone();
        after.mode = Mode::Auto;
        assert_eq!(
            writes(&before, &after),
            vec![(SCHEMA.to_string(), "mode", "'auto'".to_string())]
        );
    }
}
