//! Keyboard: key repeat from GSettings, input sources from Gnoblin, and a
//! pointer at the Lua config for shortcuts.
//!
//! Gnoblin's Lua `input.keyboard` fields override the GSettings keys, so a
//! row whose field is set in the committed snapshot is shown read-only.

use freya::prelude::*;

use crate::pages::gnoblin::{self, InputSource, Json};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::ui::{
    Group, Status, page_column, row, row_button, skeleton, slider_row, status, switch_row,
};

const KEYBOARD: &str = "org.gnome.desktop.peripherals.keyboard";
const DELAY_RANGE: (f64, f64) = (100.0, 2000.0);
const INTERVAL_RANGE: (f64, f64) = (10.0, 200.0);

#[derive(Clone, PartialEq)]
struct Model {
    repeat: bool,
    delay_ms: f64,
    interval_ms: f64,
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

fn load() -> Result<Model, CommandError> {
    let repeat = system::gsettings_get(KEYBOARD, "repeat")? == "true";
    let delay_ms = gsettings_u32("delay")?;
    let interval_ms = gsettings_u32("repeat-interval")?;
    Ok(Model {
        repeat,
        delay_ms,
        interval_ms,
        overrides: gnoblin::overrides(),
        sources: gnoblin::input_sources().map_err(|e| e.0),
        config_path: gnoblin::config_path().ok(),
    })
}

fn commit(
    loaded: State<Option<Result<Model, CommandError>>>,
    mut error: State<Option<String>>,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    background_then(work, move |result| match result {
        Ok(_) => {
            error.set(None);
            background(loaded, load);
        }
        Err(e) => error.set(Some(e.0)),
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
        let mut error = use_state::<Option<String>>(|| None);

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
                    commit(loaded, error, move || {
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
                row("Delay")
                    .subtitle(gnoblin::MANAGED)
                    .value(format!("{value} ms")),
            ),
            None => repeat_group.row(
                slider_row(
                    "Delay",
                    to_percent(model.delay_ms, DELAY_RANGE),
                    move |percent: f64| {
                        let ms = from_percent(percent, DELAY_RANGE).to_string();
                        commit(loaded, error, move || {
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
                        commit(loaded, error, move || {
                            system::gsettings_set(KEYBOARD, "repeat-interval", &ms)
                        })
                    },
                )
                .subtitle(format!("{} ms", model.interval_ms))
                .enabled(model.repeat),
            ),
        };

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
                            .subtitle(if source.kind == "xkb" {
                                "Keyboard layout"
                            } else {
                                "Input method"
                            })
                            .value(if active { "Active" } else { "" })
                            .enabled(!active)
                            .on_press(move |_| {
                                let (kind, id) = (kind.clone(), id.clone());
                                commit(loaded, error, move || {
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
        let mut shortcuts_row =
            row("Keyboard shortcuts").subtitle("Shortcuts are set in a configuration file.");
        if let Some(path) = model.config_path.clone() {
            shortcuts_row = shortcuts_row.trailing(row_button("Open file", move |_| {
                let path = path.clone();
                background_then(
                    move || open_file(&path),
                    move |result| error.set(result.err().map(|e| e.0)),
                );
            }));
        }
        let shortcuts = Group::titled("Shortcuts").row(shortcuts_row);

        page_column()
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
            .child(repeat_group)
            .child(sources)
            .child(shortcuts)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("keyboard", load);
}
