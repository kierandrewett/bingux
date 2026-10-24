//! Keyboard: key repeat, Num Lock and cursor blinking from GSettings, the
//! Caps Lock, Compose and alternate characters keys from the XKB options,
//! the input sources from the Gnoblin configuration file, and a subpage that
//! lists the shortcuts, which are set in the Lua config.
//!
//! Gnoblin's Lua `input.keyboard` fields override the GSettings keys, so a
//! row whose field is set in the committed snapshot is shown read-only.
//! Input sources are edited in the configuration file, see `sources`.

use freya::prelude::*;

use crate::icons::Icon;
use crate::pages::gnoblin::{self, Json};
use crate::pages::loading::veiled;
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{
    self, CommandError, CommandResult, background, background_then, optimistic_then,
};
use crate::ui::{
    Group, Overlays, Status, caption, choice_row, page_column, popup_column, row, row_button,
    search_field, skeleton, slider_row, status, switch_row, text_field, use_overlays,
};
use bingux_ui::prelude::{Window, popup, use_window};
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};

mod input_source;
pub mod sources;

use sources::{Edit, Source, SourcesConfig};

pub use input_source::InputSourcePage;

const KEYBOARD: &str = "org.gnome.desktop.peripherals.keyboard";
const INPUT_SOURCES: &str = "org.gnome.desktop.input-sources";
const INTERFACE: &str = "org.gnome.desktop.interface";
const WM_KEYS: &str = "org.gnome.desktop.wm.keybindings";
const SHORTCUTS: &str = "keyboard-shortcuts";
const DELAY_RANGE: (f64, f64) = (100.0, 2000.0);
const INTERVAL_RANGE: (f64, f64) = (10.0, 200.0);
const BLINK_TIME_RANGE: (f64, f64) = (100.0, 2500.0);

/// Caps Lock choices: each is an XKB option, and "" leaves Caps Lock alone.
const CAPS_LOCK: [(&str, &str); 4] = [
    ("", "Default"),
    ("caps:escape", "Works as Escape"),
    ("ctrl:nocaps", "Works as Control"),
    ("caps:none", "Turned off"),
];
/// Compose key choices: the key that starts a sequence for accented letters.
const COMPOSE_KEY: [(&str, &str); 4] = [
    ("", "Off"),
    ("compose:ralt", "Right Alt"),
    ("compose:rctrl", "Right Ctrl"),
    ("compose:menu", "Menu key"),
];
/// Alternate characters key choices: the key that types the third-level
/// characters, such as symbols. "" leaves the key alone.
const ALTERNATE_KEY: [(&str, &str); 5] = [
    ("", "Default"),
    ("lv3:lalt_switch", "Left Alt"),
    ("lv3:ralt_switch", "Right Alt"),
    ("lv3:lwin_switch", "Left Super"),
    ("lv3:rwin_switch", "Right Super"),
];

#[derive(Clone, PartialEq)]
struct Model {
    repeat: bool,
    delay_ms: f64,
    interval_ms: f64,
    remember_numlock: bool,
    numlock_on: bool,
    xkb_options: Vec<String>,
    overrides: Json,
    cursor_blink: bool,
    cursor_blink_time: f64,
}

/// One built-in shortcut action, as Gnoblin lists it.
#[derive(Clone, PartialEq)]
struct Shortcut {
    group: String,
    title: String,
    bindings: String,
}

impl Shortcut {
    fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.title.to_lowercase().contains(needle)
            || self.bindings.to_lowercase().contains(needle)
            || self.group.to_lowercase().contains(needle)
    }
}

#[derive(Clone, PartialEq)]
struct ShortcutList {
    shortcuts: Vec<Shortcut>,
    config_path: Option<String>,
}

fn gsettings_u32(schema: &str, key: &str) -> Result<f64, CommandError> {
    let raw = system::gsettings_get(schema, key)?;
    raw.trim()
        .trim_start_matches("uint32 ")
        .parse::<f64>()
        .map_err(|_| CommandError(format!("The keyboard {key} setting could not be read.")))
}

/// Parses a GSettings string list such as `['caps:escape', 'compose:ralt']`
/// or the empty `@as []`.
fn parse_list(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn list_value(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("'{item}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// The option from `group` that is set, or "" when none is.
fn selected_in(current: &[String], group: &[(&str, &str)]) -> String {
    current
        .iter()
        .find(|item| group.iter().any(|(key, _)| *key == item.as_str()))
        .cloned()
        .unwrap_or_default()
}

/// `current` with every option from `group` removed and `choice` added. Other
/// options in the list are kept as they are.
fn with_choice(current: &[String], group: &[(&str, &str)], choice: &str) -> Vec<String> {
    let mut next: Vec<String> = current
        .iter()
        .filter(|item| !group.iter().any(|(key, _)| *key == item.as_str()))
        .cloned()
        .collect();
    if !choice.is_empty() {
        next.push(choice.to_string());
    }
    next
}

/// Snapshot builds only: the shortcut list that `BINGUX_SETTINGS_FAKE_INPUT`
/// stands in for, so the Shortcuts subpage can be checked without Gnoblin.
#[cfg(feature = "snapshot")]
fn fake_shortcuts() -> Option<ShortcutList> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_INPUT")?;
    let shortcut = |group: &str, title: &str, bindings: &str| Shortcut {
        group: group.to_string(),
        title: title.to_string(),
        bindings: bindings.to_string(),
    };
    Some(ShortcutList {
        shortcuts: vec![
            shortcut("wm", "Close window", "Super+Q"),
            shortcut("wm", "Toggle fullscreen", "Super+F11"),
            shortcut("wm", "Move window to the left", "Super+Left"),
            shortcut("shell", "Open the launcher", "Super"),
            shortcut("shell", "Lock the screen", "Super+L"),
            shortcut("shell", "Show every window", "Super+A"),
        ],
        config_path: None,
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_shortcuts() -> Option<ShortcutList> {
    None
}

fn load() -> Result<Model, CommandError> {
    let repeat = system::gsettings_get(KEYBOARD, "repeat")? == "true";
    let delay_ms = gsettings_u32(KEYBOARD, "delay")?;
    let interval_ms = gsettings_u32(KEYBOARD, "repeat-interval")?;
    let remember_numlock = system::gsettings_get(KEYBOARD, "remember-numlock-state")? == "true";
    let numlock_on = system::gsettings_get(KEYBOARD, "numlock-state")? == "true";
    let xkb_options = parse_list(&system::gsettings_get(INPUT_SOURCES, "xkb-options")?);
    let cursor_blink = system::gsettings_get(INTERFACE, "cursor-blink")? == "true";
    let cursor_blink_time = gsettings_u32(INTERFACE, "cursor-blink-time")?;
    Ok(Model {
        repeat,
        delay_ms,
        interval_ms,
        remember_numlock,
        numlock_on,
        xkb_options,
        cursor_blink,
        cursor_blink_time,
        overrides: gnoblin::overrides(),
    })
}

/// The page as it shows before its first read lands: the values this machine
/// reads, so the rows keep their places when the real values arrive.
fn placeholder() -> Model {
    Model {
        repeat: true,
        delay_ms: 500.0,
        interval_ms: 30.0,
        remember_numlock: false,
        numlock_on: false,
        xkb_options: Vec::new(),
        overrides: Json::Null,
        cursor_blink: true,
        cursor_blink_time: 1200.0,
    }
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
        move || background(loaded, load),
    );
}

/// Shows an XKB options list at once and writes it in the background.
fn commit_xkb(
    loaded: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
    next: Vec<String>,
) {
    let shown = next.clone();
    commit(
        loaded,
        overlays,
        move |current| current.xkb_options = shown,
        move || system::gsettings_set(INPUT_SOURCES, "xkb-options", &list_value(&next)),
    );
}

/// Maps a slider percentage onto a millisecond range, rounded to whole ms.
fn from_percent(percent: f64, (low, high): (f64, f64)) -> u32 {
    (low + (high - low) * (percent / 100.0)).round() as u32
}

fn to_percent(value: f64, (low, high): (f64, f64)) -> f64 {
    ((value - low) / (high - low) * 100.0).clamp(0.0, 100.0)
}

/// Opens `path` in the default app. The child is reaped on a thread so the
/// page never waits for it.
fn open_file(path: &str) -> CommandResult {
    let mut child = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| CommandError("The file could not be opened.".to_string()))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(String::new())
}

/// Opens the Gnoblin configuration file, reporting a failure as a toast.
fn open_config(path: String, overlays: Overlays) {
    background_then(
        move || open_file(&path),
        move |result| {
            if let Err(e) = result {
                overlays.toast(e.0);
            }
        },
    );
}

/// The page's input source actions. `busy` is set while an edit is written,
/// and the controls wait for it, so an index always refers to the list as it
/// is on disk.
#[derive(Clone, Copy, PartialEq)]
struct SourceEdits {
    state: State<Option<Result<SourcesConfig, String>>>,
    catalog: State<Option<Vec<(Source, String)>>>,
    overlays: Overlays,
    window: Window,
    busy: State<bool>,
}

impl SourceEdits {
    /// Writes `edit` in the background, then shows the list it leaves behind.
    fn run(self, edit: Edit) {
        if *self.busy.read() {
            return;
        }
        let mut busy = self.busy;
        busy.set(true);
        let turn = sources::take_turn();
        let mut state = self.state;
        let overlays = self.overlays;
        background_then(
            move || {
                sources::apply(turn, edit)?;
                sources::read()
            },
            move |result| {
                busy.set(false);
                match result {
                    Ok(config) => state.set(Some(Ok(config))),
                    Err(message) => overlays.toast(message),
                }
            },
        );
    }
}

/// Opens the sheet that adds a source from the catalogue.
fn open_add_sheet(edits: SourceEdits) {
    edits
        .window
        .open(popup("Add an input source", AddBody { edits }));
}

/// The most catalogue rows the add sheet lists at once. The rest are one
/// search away, so the sheet opens without building hundreds of rows.
const SOURCE_LIST_CAP: usize = 40;

/// The add sheet: the catalogue minus the sources already in the list, with a
/// search box. Picking one adds it and closes the sheet.
#[derive(Clone, PartialEq)]
struct AddBody {
    edits: SourceEdits,
}

impl Component for AddBody {
    fn render(&self) -> impl IntoElement {
        let edits = self.edits;
        let window = use_window();
        let query = use_state(String::new);
        let config = match edits.state.read().clone() {
            Some(Ok(config)) => config,
            Some(Err(message)) => {
                return Element::from(popup_column().child(status(Status::Error(message))));
            }
            None => return Element::from(popup_column().child(skeleton())),
        };
        // Borrowed, not cloned: the catalogue has hundreds of entries.
        let catalog_guard = edits.catalog.read();
        let Some(catalog) = &*catalog_guard else {
            return Element::from(popup_column().child(skeleton()));
        };
        let needle = query.read().trim().to_lowercase();
        let matches: Vec<&(Source, String)> = catalog
            .iter()
            .filter(|(source, _)| !config.sources.contains(source))
            .filter(|(source, name)| {
                needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || source.id.to_lowercase().contains(&needle)
            })
            .collect();
        let total = matches.len();
        let mut column = popup_column().child(search_field(query, |_| {}).fill_width());
        if total == 0 {
            let message = if needle.is_empty() {
                "Every input source is already in the list."
            } else {
                "No input sources match."
            };
            column = column.child(status(Status::Empty(message.into())));
        } else {
            let rows = matches
                .into_iter()
                .take(SOURCE_LIST_CAP)
                .map(|(source, name)| {
                    let add = source.clone();
                    row(name.clone())
                        .leading(Icon::Keyboard)
                        .subtitle(kind_label(&source.kind))
                        .on_press(move |_| {
                            edits.run(Edit::Add(add.clone()));
                            window.close();
                        })
                        .into()
                });
            column = column.child(Group::titled("Available").rows(rows));
            if total > SOURCE_LIST_CAP {
                let more = total - SOURCE_LIST_CAP;
                column = column.child(caption(format!("Type to narrow {more} more.")));
            }
        }
        Element::from(column)
    }
}

/// The label for a source kind: an XKB layout or an IBus input method.
fn kind_label(kind: &str) -> &'static str {
    if kind == "xkb" {
        "Keyboard layout"
    } else {
        "Input method"
    }
}

/// The name a source is shown with: the catalogue's name, or its id when the
/// catalogue does not know it.
fn source_name(source: &Source, catalog: &[(Source, String)]) -> String {
    catalog
        .iter()
        .find(|(known, _)| known == source)
        .map_or_else(|| source.id.clone(), |(_, name)| name.clone())
}

/// The actions for the source at `index` of `count`: move it up or down, or
/// remove it. An action that cannot apply is greyed out: the first cannot
/// move up, the last cannot move down, and the last one left cannot go.
fn source_menu(index: usize, count: usize, edits: SourceEdits) -> Vec<MenuItem> {
    let up = menu_item("Move up", move |_| edits.run(Edit::MoveUp(index))).icon(Icon::PanUp);
    let down =
        menu_item("Move down", move |_| edits.run(Edit::MoveDown(index))).icon(Icon::PanDown);
    let remove = menu_item("Remove", move |_| edits.run(Edit::Remove(index)))
        .icon(Icon::Delete)
        .destructive();
    vec![
        if index > 0 { up } else { up.disabled() },
        if index + 1 < count {
            down
        } else {
            down.disabled()
        },
        menu_separator(),
        if count > 1 { remove } else { remove.disabled() },
    ]
}

/// One source in the list. Its row opens the source's own page, where the
/// actions are, and its right-click menu offers the same actions.
fn source_row(
    index: usize,
    count: usize,
    source: &Source,
    edits: SourceEdits,
    navigator: Navigator,
    catalog: &[(Source, String)],
) -> Element {
    let name = source_name(source, catalog);
    let open = Route::InputSource {
        source: source.clone(),
        name: name.clone(),
    };
    row(name)
        .leading(Icon::Keyboard)
        .subtitle(kind_label(&source.kind))
        .navigates()
        .on_press(move |_| navigator.push(open.clone()))
        .context_menu(source_menu(index, count, edits))
        .into()
}

/// The heading for a shortcut group, from its Gnoblin name such as `wm`.
fn group_title(group: &str) -> String {
    if group == "wm" {
        return "Windows".to_string();
    }
    let mut chars = group.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A GTK accelerator such as `<Super>Left`, shown as `Super+Left`.
fn accelerator_text(accelerator: &str) -> String {
    accelerator.replace('>', "+").replace('<', "")
}

/// The bindings of one shortcut, or `Not set` when it has none.
fn bindings_text(bindings: &[String]) -> String {
    if bindings.is_empty() {
        "Not set".to_string()
    } else {
        bindings.join(", ")
    }
}

/// The shortcut list: Gnoblin's actions, then the input source switch. Gnoblin
/// does not list the switch, so it is read from GSettings here.
fn load_shortcuts() -> Result<ShortcutList, CommandError> {
    let mut list = match fake_shortcuts() {
        Some(list) => list,
        None => gnoblin_shortcuts()?,
    };
    let switch: Vec<String> = parse_list(&system::gsettings_get(WM_KEYS, "switch-input-source")?)
        .iter()
        .map(|key| accelerator_text(key))
        .collect();
    list.shortcuts.push(Shortcut {
        group: "input".to_string(),
        title: "Switch input source".to_string(),
        bindings: bindings_text(&switch),
    });
    Ok(list)
}

/// The actions Gnoblin lists, each with its default bindings.
fn gnoblin_shortcuts() -> Result<ShortcutList, CommandError> {
    let unreadable = || CommandError("The shortcuts could not be read.".to_string());
    let output = gnoblin::gnoblinctl(&["--json", "shortcut", "actions"])?;
    let actions = Json::parse(&output).map_err(|_| unreadable())?;
    let actions = actions.as_array().ok_or_else(unreadable)?;
    let shortcuts = actions
        .iter()
        .filter_map(|action| {
            let key = action.get(&["key"])?.as_str()?;
            let group = action.get(&["group"])?.as_str()?.to_string();
            let title = action
                .get(&["description"])
                .and_then(Json::as_str)
                .unwrap_or(key)
                .to_string();
            let bindings: Vec<String> = action
                .get(&["default_bindings"])
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|binding| binding.as_str())
                .map(accelerator_text)
                .collect();
            Some(Shortcut {
                group,
                title,
                bindings: bindings_text(&bindings),
            })
        })
        .collect();
    Ok(ShortcutList {
        shortcuts,
        config_path: gnoblin::config_path().ok(),
    })
}

#[derive(PartialEq)]
pub struct KeyboardPage;

impl Component for KeyboardPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("keyboard", load);
        let sources_state = system::use_cached("keyboard-sources", sources::read);
        let catalog_state = system::use_cached("keyboard-catalog", sources::catalog);
        let sources_busy = use_state(|| false);
        let overlays = use_overlays();
        let window = use_window();
        let navigator = use_navigator();
        let try_text = use_state(String::new);
        // Snapshot only: BINGUX_SETTINGS_SNAPSHOT_OPEN=add-input opens the
        // add sheet once the page has loaded, so it can be captured.
        #[cfg(feature = "snapshot")]
        use_hook(move || {
            if std::env::var("BINGUX_SETTINGS_SNAPSHOT_OPEN").is_ok_and(|v| v == "add-input") {
                spawn(async move {
                    timer(std::time::Duration::from_millis(2500)).await;
                    open_add_sheet(SourceEdits {
                        state: sources_state,
                        catalog: catalog_state,
                        overlays,
                        window,
                        busy: sources_busy,
                    });
                });
            }
        });

        let result = loaded.read().clone();
        let loading = result.is_none();
        let model = match result.unwrap_or_else(|| Ok(placeholder())) {
            Ok(model) => model,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(loaded, load)),
                );
            }
        };

        let overridden =
            |field: &str| gnoblin::lua_override(&model.overrides, &["input", "keyboard", field]);
        let repeat_lua = overridden("repeat");
        let delay_lua = overridden("delay");
        let interval_lua = overridden("repeat-interval");
        // A Lua value wins over GSettings, so it decides which rows show.
        let repeat_on = repeat_lua
            .as_deref()
            .map_or(model.repeat, |value| value == "true");

        let repeat_switch = match repeat_lua {
            Some(value) => row("Repeat keys").subtitle(gnoblin::MANAGED).value(value),
            None => switch_row("Repeat keys", model.repeat, move |next: bool| {
                commit(
                    loaded,
                    overlays,
                    move |current| current.repeat = next,
                    move || {
                        system::gsettings_set(
                            KEYBOARD,
                            "repeat",
                            if next { "true" } else { "false" },
                        )
                    },
                )
            }),
        };
        // Delay, speed and the test box only matter while keys repeat, so
        // they are nested under the switch and shown while it is on.
        let repeat_nested: Vec<Element> = if repeat_on {
            let delay: Element = match delay_lua {
                Some(value) => row("Delay before repeat")
                    .subtitle(gnoblin::MANAGED)
                    .value(format!("{value} ms"))
                    .into(),
                None => slider_row(
                    "Delay before repeat",
                    to_percent(model.delay_ms, DELAY_RANGE),
                    move |percent: f64| {
                        let ms = from_percent(percent, DELAY_RANGE);
                        commit(
                            loaded,
                            overlays,
                            move |current| current.delay_ms = f64::from(ms),
                            move || system::gsettings_set(KEYBOARD, "delay", &ms.to_string()),
                        )
                    },
                )
                .value_text(format!("{} ms", model.delay_ms))
                .into(),
            };
            let interval: Element = match interval_lua {
                Some(value) => row("Repeat speed")
                    .subtitle(gnoblin::MANAGED)
                    .value(format!("{value} ms"))
                    .into(),
                None => slider_row(
                    "Repeat speed",
                    100.0 - to_percent(model.interval_ms, INTERVAL_RANGE),
                    move |percent: f64| {
                        let ms = from_percent(100.0 - percent, INTERVAL_RANGE);
                        commit(
                            loaded,
                            overlays,
                            move |current| current.interval_ms = f64::from(ms),
                            move || {
                                system::gsettings_set(KEYBOARD, "repeat-interval", &ms.to_string())
                            },
                        )
                    },
                )
                .value_text(format!("{} ms", model.interval_ms))
                .into(),
            };
            let try_it: Element = row("Try it")
                .subtitle("Hold a key in this box to test the repeat.")
                .trailing(text_field(try_text, |_| {}))
                .into();
            vec![delay, interval, try_it]
        } else {
            Vec::new()
        };
        let repeat_group =
            Group::titled("Key repeat").row(repeat_switch.nested(repeat_on, repeat_nested));

        // Num Lock: the remembered state wins when remembering is on, so the
        // starting state is only shown when remembering is off.
        let numlock_remember_lua = overridden("remember-numlock-state");
        let numlock_state_lua = overridden("numlock-state");
        let numlock_remembered = numlock_remember_lua
            .as_deref()
            .map_or(model.remember_numlock, |value| value == "true");
        let mut typing_group = Group::titled("Typing");
        let remember_numlock_row = match numlock_remember_lua {
            Some(value) => row("Remember Num Lock state")
                .subtitle(gnoblin::MANAGED)
                .value(value),
            None => switch_row(
                "Remember Num Lock state",
                model.remember_numlock,
                move |next: bool| {
                    commit(
                        loaded,
                        overlays,
                        move |current| current.remember_numlock = next,
                        move || {
                            system::gsettings_set(
                                KEYBOARD,
                                "remember-numlock-state",
                                if next { "true" } else { "false" },
                            )
                        },
                    )
                },
            ),
        };
        // The sign-in state only applies when Num Lock is not remembered, so
        // it is nested under the switch and shown while remembering is off.
        let numlock_nested: Vec<Element> = if numlock_remembered {
            Vec::new()
        } else {
            let sign_in: Element = match numlock_state_lua {
                Some(value) => row("Num Lock on at sign-in")
                    .subtitle(gnoblin::MANAGED)
                    .value(value)
                    .into(),
                None => switch_row(
                    "Num Lock on at sign-in",
                    model.numlock_on,
                    move |next: bool| {
                        commit(
                            loaded,
                            overlays,
                            move |current| current.numlock_on = next,
                            move || {
                                system::gsettings_set(
                                    KEYBOARD,
                                    "numlock-state",
                                    if next { "true" } else { "false" },
                                )
                            },
                        )
                    },
                )
                .into(),
            };
            vec![sign_in]
        };
        typing_group =
            typing_group.row(remember_numlock_row.nested(!numlock_remembered, numlock_nested));

        // Caps Lock and Compose are XKB options in the input-sources list.
        // Writing one replaces only its own group, so other options stay.
        let xkb_lua = overridden("xkb-options");
        let caps_current = selected_in(&model.xkb_options, &CAPS_LOCK);
        let compose_current = selected_in(&model.xkb_options, &COMPOSE_KEY);
        let alternate_current = selected_in(&model.xkb_options, &ALTERNATE_KEY);
        let caps_row = match xkb_lua.clone() {
            Some(_) => row("Caps Lock key").subtitle(gnoblin::MANAGED),
            None => {
                let options = model.xkb_options.clone();
                choice_row(
                    "Caps Lock key",
                    CAPS_LOCK
                        .iter()
                        .map(|(key, label)| (key.to_string(), label.to_string()))
                        .collect(),
                    &caps_current,
                    move |choice: String| {
                        let next = with_choice(&options, &CAPS_LOCK, &choice);
                        commit_xkb(loaded, overlays, next);
                    },
                )
            }
        };
        let compose_row = match xkb_lua.clone() {
            Some(_) => row("Compose key").subtitle(gnoblin::MANAGED),
            None => {
                let options = model.xkb_options.clone();
                choice_row(
                    "Compose key",
                    COMPOSE_KEY
                        .iter()
                        .map(|(key, label)| (key.to_string(), label.to_string()))
                        .collect(),
                    &compose_current,
                    move |choice: String| {
                        let next = with_choice(&options, &COMPOSE_KEY, &choice);
                        commit_xkb(loaded, overlays, next);
                    },
                )
                .subtitle("Type two keys to make accented letters and symbols.")
            }
        };
        // The blink speed only matters while the cursor blinks, so it is nested
        // under the switch and shown while it is on.
        let blink_nested: Vec<Element> = if model.cursor_blink {
            let blink: Element = slider_row(
                "Blink speed",
                to_percent(model.cursor_blink_time, BLINK_TIME_RANGE),
                move |percent: f64| {
                    let ms = from_percent(percent, BLINK_TIME_RANGE);
                    commit(
                        loaded,
                        overlays,
                        move |current| current.cursor_blink_time = f64::from(ms),
                        move || {
                            system::gsettings_set(INTERFACE, "cursor-blink-time", &ms.to_string())
                        },
                    )
                },
            )
            .value_text(format!("{} ms", model.cursor_blink_time))
            .into();
            vec![blink]
        } else {
            Vec::new()
        };
        let cursor_blink_row =
            switch_row("Cursor blinking", model.cursor_blink, move |next: bool| {
                commit(
                    loaded,
                    overlays,
                    move |current| current.cursor_blink = next,
                    move || {
                        system::gsettings_set(
                            INTERFACE,
                            "cursor-blink",
                            if next { "true" } else { "false" },
                        )
                    },
                )
            })
            .nested(model.cursor_blink, blink_nested);
        typing_group = typing_group.row(cursor_blink_row).row(caps_row);

        // Compose and the alternate characters key both type accented letters
        // and symbols, so they share a group.
        let alternate_row = match xkb_lua {
            Some(_) => row("Alternate characters key").subtitle(gnoblin::MANAGED),
            None => {
                let options = model.xkb_options.clone();
                choice_row(
                    "Alternate characters key",
                    ALTERNATE_KEY
                        .iter()
                        .map(|(key, label)| (key.to_string(), label.to_string()))
                        .collect(),
                    &alternate_current,
                    move |choice: String| {
                        let next = with_choice(&options, &ALTERNATE_KEY, &choice);
                        commit_xkb(loaded, overlays, next);
                    },
                )
                .subtitle("Types the third-level characters, such as symbols.")
            }
        };
        let special_group = Group::titled("Special character entry")
            .row(compose_row)
            .row(alternate_row);

        // The list is read from the configuration file, so it shows even
        // while Gnoblin is not running. Its controls wait while an edit is
        // being written.
        let edits = SourceEdits {
            state: sources_state,
            catalog: catalog_state,
            overlays,
            window,
            busy: sources_busy,
        };
        let busy = *sources_busy.read();
        let sources_group = Group::titled("Input sources");
        let sources: Element = match sources_state.read().clone() {
            None => sources_group.row(skeleton()).into(),
            Some(Err(message)) => sources_group.row(status(Status::Error(message))).into(),
            Some(Ok(config)) => {
                let count = config.sources.len();
                let catalog = edits.catalog.read().clone().unwrap_or_default();
                let group = if config.sources.is_empty() {
                    sources_group.row(status(Status::Empty("No input sources are set up.".into())))
                } else {
                    sources_group.rows(config.sources.iter().enumerate().map(|(index, source)| {
                        source_row(index, count, source, edits, navigator, &catalog)
                    }))
                };
                group
                    .row(switch_row(
                        "Separate input per window",
                        config.per_window,
                        move |next: bool| edits.run(Edit::SetPerWindow(next)),
                    ))
                    .row(
                        row("Add an input source")
                            .leading(Icon::ListAdd)
                            .enabled(!busy)
                            .on_press(move |_| open_add_sheet(edits)),
                    )
                    .into()
            }
        };

        // The count comes from the shortcut list once it has loaded, so the
        // row never waits for Gnoblin.
        let shortcut_count = match system::cached::<Result<ShortcutList, CommandError>>(SHORTCUTS)
            .read()
            .as_ref()
        {
            Some(Ok(list)) => Some(list.shortcuts.len()),
            _ => None,
        };
        let mut shortcuts_row = row("View and customise shortcuts")
            .leading(Icon::PreferencesDesktopKeyboardShortcuts)
            .subtitle("See every shortcut and what it does.")
            .navigates()
            .on_press(move |_| navigator.push(Route::Shortcuts));
        if let Some(count) = shortcut_count {
            shortcuts_row = shortcuts_row.value(format!("{count} shortcuts"));
        }
        let shortcuts = Group::titled("Shortcuts").row(shortcuts_row);

        let page = page_column()
            .child(repeat_group)
            .child(typing_group)
            .child(special_group)
            .child(sources)
            .child(shortcuts);
        veiled(page, loading)
    }
}

/// The shortcut list with a search box. Gnoblin reads the list and the
/// shortcuts are set in the configuration file, which the button opens.
#[derive(PartialEq)]
pub struct ShortcutsPage;

impl Component for ShortcutsPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached(SHORTCUTS, load_shortcuts);
        let overlays = use_overlays();
        let query = use_state(String::new);

        let Some(result) = loaded.read().clone() else {
            return page_column().child(skeleton());
        };
        let list = match result {
            Ok(list) => list,
            Err(e) if e.0 == gnoblin::NOT_RUNNING => {
                return page_column().child(status(Status::Empty(e.0)));
            }
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(loaded, load_shortcuts)),
                );
            }
        };

        let needle = query.read().trim().to_lowercase();
        let mut groups: Vec<(String, Vec<&Shortcut>)> = Vec::new();
        for shortcut in list.shortcuts.iter().filter(|s| s.matches(&needle)) {
            match groups
                .iter_mut()
                .find(|(group, _)| *group == shortcut.group)
            {
                Some((_, items)) => items.push(shortcut),
                None => groups.push((shortcut.group.clone(), vec![shortcut])),
            }
        }

        let mut page = page_column().child(search_field(query, |_| {}));
        if groups.is_empty() {
            page = page.child(status(Status::Empty("No shortcuts match.".into())));
        }
        for (group, items) in &groups {
            page = page.child(Group::titled(&group_title(group)).rows(items.iter().map(
                |shortcut| {
                    Element::from(row(shortcut.title.clone()).value(shortcut.bindings.clone()))
                },
            )));
        }
        if let Some(path) = list.config_path.clone() {
            page = page.child(
                Group::titled("Configuration").row(
                    row("Configuration file")
                        .subtitle("Shortcuts are set in this file.")
                        .trailing(row_button("Open file", move |_| {
                            open_config(path.clone(), overlays)
                        })),
                ),
            );
        }
        page
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("keyboard", load);
    system::prefetch("keyboard-sources", sources::read);
    system::prefetch("keyboard-catalog", sources::catalog);
    system::prefetch(SHORTCUTS, load_shortcuts);
}
