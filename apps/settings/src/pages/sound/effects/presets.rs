//! Presets: pick one, make a new one, duplicate, rename, delete, import and
//! export, and have a preset follow a device.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use bingux_audio_effects::import;
use bingux_audio_effects::preset::{Direction, Preset};
use bingux_audio_effects::store::Store;
use freya::prelude::*;

use bingux_ui::prelude::{
    CANCEL_ID, ResponseKind, dialog, dropdown, menu_button, menu_item, menu_separator, text_field,
};
use bingux_ui::style::*;

use super::{Fx, Loaded};
use crate::system::background_then;
use crate::ui::files::{ChooserRequest, choose};

/// The presets menu of the top bar: the presets of the direction to pick from,
/// and the actions on the one in use.
pub(super) fn menu(fx: Fx, data: &Loaded) -> Element {
    let current = data.preset.name.clone();
    let mut items: Vec<_> = data
        .presets
        .iter()
        .map(|name| {
            let picked = name.clone();
            menu_item(name.clone(), move |_| select(fx, picked.clone())).checked(*name == current)
        })
        .collect();
    items.push(menu_separator());
    items.push(menu_item("New preset…", move |_| new_preset(fx)));
    items.push(menu_item("Duplicate", {
        let current = current.clone();
        move |_| duplicate(fx, current.clone())
    }));
    items.push(menu_item("Rename…", {
        let current = current.clone();
        move |_| rename(fx, current.clone())
    }));
    items.push(menu_separator());
    items.push(menu_item("Import…", move |_| import_file(fx)));
    items.push(menu_item("Export…", {
        let current = current.clone();
        move |_| export_file(fx, current.clone())
    }));
    items.push(menu_separator());
    items.push(menu_item("Delete", {
        let current = current.clone();
        move |_| confirm_delete(fx, current.clone())
    }));
    menu_button(format!("Preset: {current}"), items).into()
}

/// Make `name` the preset in use and play it.
fn select(fx: Fx, name: String) {
    let direction = fx.active_direction();
    let overlays = fx.overlays;
    background_then(
        move || {
            let store = Store::default();
            let preset = store.load(direction, &name).map_err(|e| e.0)?;
            store
                .update_state(|state| state.of_mut(direction).preset = name)
                .map_err(|e| e.0)?;
            Ok::<Preset, String>(preset)
        },
        move |result| match result {
            Ok(preset) => {
                fx.reload();
                let mut data = fx.data;
                data.write().preset = preset;
                fx.commit_now(direction);
            }
            Err(message) => overlays.toast(message),
        },
    );
}

/// The text field of a naming dialog. What is typed is kept in `text`, which
/// the dialog's button reads.
#[derive(Clone)]
struct NameEntry {
    initial: String,
    text: Rc<RefCell<String>>,
}

impl PartialEq for NameEntry {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.text, &other.text) && self.initial == other.initial
    }
}

impl Component for NameEntry {
    fn render(&self) -> impl IntoElement {
        let value = use_state(|| self.initial.clone());
        let text = Rc::clone(&self.text);
        text_field(value, |_| {})
            .on_change(move |typed: String| *text.borrow_mut() = typed)
            .fill_width()
            .autofocus()
            .select_all_on_open()
    }
}

/// Ask for a name, then call `accept` with it.
fn ask_name(
    fx: Fx,
    title: &str,
    initial: &str,
    accept_label: &str,
    accept: impl Fn(String) + 'static,
) {
    let text = Rc::new(RefCell::new(initial.to_string()));
    let entry = NameEntry {
        initial: initial.to_string(),
        text: Rc::clone(&text),
    };
    fx.overlays
        .show_dialog(dialog(title, "").content(entry).cancel("Cancel").response(
            accept_label,
            ResponseKind::Suggested,
            move |_| {
                let name = text.borrow().trim().to_string();
                if !name.is_empty() {
                    accept(name);
                }
            },
        ));
}

/// Run a store change in the background, then show the result, and select
/// `chosen` when it names the preset to play next.
fn change(
    fx: Fx,
    work: impl FnOnce(&Store, Direction) -> Result<Option<String>, String> + Send + 'static,
) {
    let direction = fx.active_direction();
    let overlays = fx.overlays;
    background_then(
        move || work(&Store::default(), direction),
        move |result| match result {
            Ok(Some(name)) => select(fx, name),
            Ok(None) => fx.reload(),
            Err(message) => overlays.toast(message),
        },
    );
}

fn new_preset(fx: Fx) {
    ask_name(fx, "New preset", "", "Create", move |name| {
        change(fx, move |store, direction| {
            if store.exists(direction, &name) {
                return Err(format!("A preset called \"{name}\" already exists."));
            }
            store
                .save(&Preset::new(name.clone(), direction))
                .map_err(|e| e.0)?;
            Ok(Some(name))
        });
    });
}

fn duplicate(fx: Fx, name: String) {
    change(fx, move |store, direction| {
        store.duplicate(direction, &name).map(Some).map_err(|e| e.0)
    });
}

fn rename(fx: Fx, name: String) {
    let initial = name.clone();
    ask_name(fx, "Rename preset", &initial, "Rename", move |new_name| {
        let name = name.clone();
        change(fx, move |store, direction| {
            store.rename(direction, &name, &new_name).map_err(|e| e.0)?;
            Ok(Some(new_name))
        });
    });
}

fn confirm_delete(fx: Fx, name: String) {
    fx.overlays.show_dialog(
        dialog(
            format!("Delete \"{name}\"?"),
            "The preset file is removed. Other presets are not changed.",
        )
        .cancel("Cancel")
        .default_action(CANCEL_ID)
        .response("Delete", ResponseKind::Destructive, move |_| {
            let name = name.clone();
            change(fx, move |store, direction| {
                store.delete(direction, &name).map_err(|e| e.0)?;
                // Fall back to Flat, or the first preset left.
                let next = ["Flat".to_string()]
                    .into_iter()
                    .chain(store.list(direction))
                    .find(|candidate| store.exists(direction, candidate));
                match next {
                    Some(next) => Ok(Some(next)),
                    None => {
                        store
                            .save(&Preset::new("Flat", direction))
                            .map_err(|e| e.0)?;
                        Ok(Some("Flat".to_string()))
                    }
                }
            });
        }),
    );
}

fn import_file(fx: Fx) {
    choose(
        fx.window,
        ChooserRequest::open()
            .title("Import a preset")
            .filter_spec("Presets", "*.json *.txt")
            .filter_spec("All files", "*"),
        move |answer| {
            let Some(path) = answer.and_then(|answer| answer.paths.into_iter().next()) else {
                return;
            };
            import_path(fx, path);
        },
    );
}

fn import_path(fx: Fx, path: PathBuf) {
    let direction = fx.active_direction();
    let overlays = fx.overlays;
    background_then(
        move || -> Result<(Option<String>, Vec<String>, String), String> {
            let text = bingux_ui::runtime::read_to_string(&path)
                .map_err(|e| format!("Could not read the file. {e}"))?;
            let stem = path.file_stem().map_or_else(
                || "Imported".to_string(),
                |stem| stem.to_string_lossy().into_owned(),
            );
            let store = Store::default();
            let imported = import::read(
                &text,
                &stem,
                direction,
                &bingux_audio_effects::Catalog::load(),
            )
            .map_err(|e| e.0)?;
            let mut chosen = None;
            let mut unmapped = Vec::new();
            for item in imported {
                let mut preset = item.preset;
                preset.name = store.free_name(preset.direction, &preset.name);
                store.save(&preset).map_err(|e| e.0)?;
                if preset.direction == direction && chosen.is_none() {
                    chosen = Some(preset.name.clone());
                }
                unmapped.extend(item.unmapped);
            }
            Ok((chosen, unmapped, stem))
        },
        move |result| match result {
            Ok((chosen, unmapped, stem)) => {
                if !unmapped.is_empty() {
                    let lines: Vec<String> =
                        unmapped.iter().map(|line| format!("- {line}")).collect();
                    fx.overlays.show_dialog(
                        dialog(
                            format!("Imported \"{stem}\""),
                            format!("These settings have no equivalent and were left out or approximated:\n{}", lines.join("\n")),
                        )
                        .cancel("Close"),
                    );
                }
                match chosen {
                    Some(name) => select(fx, name),
                    None => fx.reload(),
                }
            }
            Err(message) => overlays.toast(message),
        },
    );
}

fn export_file(fx: Fx, name: String) {
    let direction = fx.active_direction();
    let overlays = fx.overlays;
    let file_name = format!("{name}.json");
    choose(
        fx.window,
        ChooserRequest::save(file_name).title("Export the preset"),
        move |answer| {
            let Some(path) = answer.and_then(|answer| answer.paths.into_iter().next()) else {
                return;
            };
            let name = name.clone();
            background_then(
                move || {
                    Store::default()
                        .export(direction, &name, &path)
                        .map_err(|e| e.0)
                },
                move |result| match result {
                    Ok(()) => overlays.toast("Preset exported."),
                    Err(message) => overlays.toast(message),
                },
            );
        },
    );
}
