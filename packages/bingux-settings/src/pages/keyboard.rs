//! Keyboard: key repeat and Num Lock from GSettings, the Caps Lock and
//! Compose keys from the XKB options, input sources from Gnoblin, and a
//! pointer at the Lua config for shortcuts.
//!
//! Gnoblin's Lua `input.keyboard` fields override the GSettings keys, so a
//! row whose field is set in the committed snapshot is shown read-only.
//! Input sources can be picked but not added: Gnoblin offers no way to add
//! one from here.

use freya::prelude::*;

use crate::icons::Icon;
use crate::pages::gnoblin::{self, InputSource, Json};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Overlays, Status, choice_row, page_column, row, row_button, skeleton, slider_row,
    status, switch_row, text_field, use_overlays,
};

const KEYBOARD: &str = "org.gnome.desktop.peripherals.keyboard";
const INPUT_SOURCES: &str = "org.gnome.desktop.input-sources";
const DELAY_RANGE: (f64, f64) = (100.0, 2000.0);
const INTERVAL_RANGE: (f64, f64) = (10.0, 200.0);

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

#[derive(Clone, PartialEq)]
struct Model {
    repeat: bool,
    delay_ms: f64,
    interval_ms: f64,
    remember_numlock: bool,
    numlock_on: bool,
    xkb_options: Vec<String>,
    overrides: Json,
    sources: Result<Vec<InputSource>, String>,
    config_path: Option<String>,
}

fn gsettings_u32(key: &str) -> Result<f64, CommandError> {
    let raw = system::gsettings_get(KEYBOARD, key)?;
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

fn load() -> Result<Model, CommandError> {
    let repeat = system::gsettings_get(KEYBOARD, "repeat")? == "true";
    let delay_ms = gsettings_u32("delay")?;
    let interval_ms = gsettings_u32("repeat-interval")?;
    let remember_numlock = system::gsettings_get(KEYBOARD, "remember-numlock-state")? == "true";
    let numlock_on = system::gsettings_get(KEYBOARD, "numlock-state")? == "true";
    let xkb_options = parse_list(&system::gsettings_get(INPUT_SOURCES, "xkb-options")?);
    Ok(Model {
        repeat,
        delay_ms,
        interval_ms,
        remember_numlock,
        numlock_on,
        xkb_options,
        overrides: gnoblin::overrides(),
        sources: gnoblin::input_sources().map_err(|e| e.0),
        config_path: gnoblin::config_path().ok(),
    })
}

fn commit(
    loaded: State<Option<Result<Model, CommandError>>>,
    overlays: Overlays,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    background_then(work, move |result| match result {
        Ok(_) => {
            background(loaded, load);
        }
        Err(e) => overlays.toast(e.0),
    });
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

#[derive(PartialEq)]
pub struct KeyboardPage;

impl Component for KeyboardPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("keyboard", load);
        let overlays = use_overlays();
        let try_text = use_state(String::new);

        let Some(result) = loaded.read().clone() else {
            return page_column().child(skeleton());
        };
        let model = match result {
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

        let mut repeat_group = Group::titled("Key repeat");
        repeat_group = match repeat_lua {
            Some(value) => {
                repeat_group.row(row("Repeat keys").subtitle(gnoblin::MANAGED).value(value))
            }
            None => repeat_group.row(switch_row(
                "Repeat keys",
                model.repeat,
                move |next: bool| {
                    commit(loaded, overlays, move || {
                        system::gsettings_set(
                            KEYBOARD,
                            "repeat",
                            if next { "true" } else { "false" },
                        )
                    })
                },
            )),
        };
        repeat_group = match delay_lua {
            Some(value) => repeat_group.row(
                row("Delay before repeat")
                    .subtitle(gnoblin::MANAGED)
                    .value(format!("{value} ms")),
            ),
            None => repeat_group.row(
                slider_row(
                    "Delay before repeat",
                    to_percent(model.delay_ms, DELAY_RANGE),
                    move |percent: f64| {
                        let ms = from_percent(percent, DELAY_RANGE).to_string();
                        commit(loaded, overlays, move || {
                            system::gsettings_set(KEYBOARD, "delay", &ms)
                        })
                    },
                )
                .subtitle(format!("{} ms", model.delay_ms))
                .enabled(model.repeat),
            ),
        };
        repeat_group = match interval_lua {
            Some(value) => repeat_group.row(
                row("Repeat speed")
                    .subtitle(gnoblin::MANAGED)
                    .value(format!("{value} ms")),
            ),
            None => repeat_group.row(
                slider_row(
                    "Repeat speed",
                    100.0 - to_percent(model.interval_ms, INTERVAL_RANGE),
                    move |percent: f64| {
                        let ms = from_percent(100.0 - percent, INTERVAL_RANGE).to_string();
                        commit(loaded, overlays, move || {
                            system::gsettings_set(KEYBOARD, "repeat-interval", &ms)
                        })
                    },
                )
                .subtitle(format!("{} ms", model.interval_ms))
                .enabled(model.repeat),
            ),
        };
        repeat_group = repeat_group.row(
            row("Try it")
                .subtitle("Hold a key in this box to test the repeat.")
                .trailing(text_field(try_text, |_| {}))
                .enabled(model.repeat),
        );

        // Num Lock: the remembered state wins when remembering is on, so the
        // "Num Lock on" switch is only live when remembering is off.
        let numlock_remember_lua = overridden("remember-numlock-state");
        let numlock_state_lua = overridden("numlock-state");
        let mut typing_group = Group::titled("Typing");
        typing_group = match numlock_remember_lua {
            Some(value) => typing_group.row(
                row("Remember Num Lock state")
                    .subtitle(gnoblin::MANAGED)
                    .value(value),
            ),
            None => typing_group.row(switch_row(
                "Remember Num Lock state",
                model.remember_numlock,
                move |next: bool| {
                    commit(loaded, overlays, move || {
                        system::gsettings_set(
                            KEYBOARD,
                            "remember-numlock-state",
                            if next { "true" } else { "false" },
                        )
                    })
                },
            )),
        };
        typing_group = match numlock_state_lua {
            Some(value) => typing_group.row(
                row("Num Lock on at sign-in")
                    .subtitle(gnoblin::MANAGED)
                    .value(value),
            ),
            None => typing_group.row(
                switch_row(
                    "Num Lock on at sign-in",
                    model.numlock_on,
                    move |next: bool| {
                        commit(loaded, overlays, move || {
                            system::gsettings_set(
                                KEYBOARD,
                                "numlock-state",
                                if next { "true" } else { "false" },
                            )
                        })
                    },
                )
                .subtitle("Used when Num Lock is not remembered.")
                .enabled(!model.remember_numlock),
            ),
        };

        // Caps Lock and Compose are XKB options in the input-sources list.
        // Writing one replaces only its own group, so other options stay.
        let xkb_lua = overridden("xkb-options");
        let caps_current = selected_in(&model.xkb_options, &CAPS_LOCK);
        let compose_current = selected_in(&model.xkb_options, &COMPOSE_KEY);
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
                        commit(loaded, overlays, move || {
                            system::gsettings_set(INPUT_SOURCES, "xkb-options", &list_value(&next))
                        })
                    },
                )
            }
        };
        let compose_row = match xkb_lua {
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
                        commit(loaded, overlays, move || {
                            system::gsettings_set(INPUT_SOURCES, "xkb-options", &list_value(&next))
                        })
                    },
                )
                .subtitle("Type two keys to make accented letters and symbols.")
            }
        };
        typing_group = typing_group.row(caps_row).row(compose_row);

        // The section always opens with where the list comes from. A missing
        // desktop is a calm muted line; any other failure is an error.
        let sources_group = Group::titled("Input sources");
        let sources: Element = match &model.sources {
            Ok(sources) if sources.is_empty() => sources_group
                .row(status(Status::Empty("No input sources are set up.".into())))
                .into(),
            Ok(sources) => sources_group
                .rows(sources.iter().map(|source| {
                    let kind = source.kind.clone();
                    let id = source.id.clone();
                    let active = source.active;
                    Element::from(
                        row(source.name.clone())
                            .leading(Icon::Keyboard)
                            .subtitle(if source.kind == "xkb" {
                                "Keyboard layout"
                            } else {
                                "Input method"
                            })
                            .value(if active { "Active" } else { "" })
                            .enabled(!active)
                            .on_press(move |_| {
                                let (kind, id) = (kind.clone(), id.clone());
                                commit(loaded, overlays, move || {
                                    gnoblin::select_input_source(&kind, &id)
                                })
                            }),
                    )
                }))
                .into(),
            Err(message) if message == gnoblin::NOT_RUNNING => sources_group
                .row(status(Status::Empty(message.clone())))
                .into(),
            Err(message) => sources_group
                .row(status(Status::Error(message.clone())))
                .into(),
        };

        // Shortcuts are set in a configuration file. The button opens that file
        // in the default editor.
        let mut shortcuts_row = row("Shortcuts")
            .leading(Icon::PreferencesDesktopKeyboardShortcuts)
            .subtitle("Change shortcuts in the configuration file.");
        if let Some(path) = model.config_path.clone() {
            shortcuts_row = shortcuts_row.trailing(row_button("Open file", move |_| {
                let path = path.clone();
                background_then(
                    move || open_file(&path),
                    move |result| {
                        if let Err(e) = result {
                            overlays.toast(e.0);
                        }
                    },
                );
            }));
        }
        let shortcuts = Group::titled("Shortcuts").row(shortcuts_row);

        page_column()
            .child(repeat_group)
            .child(typing_group)
            .child(sources)
            .child(shortcuts)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("keyboard", load);
}
