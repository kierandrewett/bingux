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
    tr,
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
    items.push(menu_item(&tr!("sound-preset-new"), move |_| new_preset(fx)));
    items.push(menu_item(&tr!("sound-preset-duplicate"), {
        let current = current.clone();
        move |_| duplicate(fx, current.clone())
    }));
    items.push(menu_item(&tr!("sound-preset-rename"), {
        let current = current.clone();
        move |_| rename(fx, current.clone())
    }));
    items.push(menu_separator());
    items.push(menu_item(&tr!("sound-preset-import"), move |_| {
        import_file(fx)
    }));
    items.push(menu_item(&tr!("sound-preset-export"), {
        let current = current.clone();
        move |_| export_file(fx, current.clone())
    }));
    items.push(menu_separator());
    items.push(menu_item(&tr!("sound-preset-delete"), {
        let current = current.clone();
        move |_| confirm_delete(fx, current.clone())
    }));
    menu_button(
        if is_compact() {
            tr!("sound-preset-menu-compact")
        } else {
            tr!("sound-preset-menu", name = current.clone())
        },
        items,
    )
    .into()
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
            Err(raw) => super::super::report(overlays, &tr!("sound-preset-switch-failed"), &raw),
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
    fx.overlays.show_dialog(
        dialog(title, "")
            .content(entry)
            .cancel(&tr!("sound-action-cancel"))
            .response(accept_label, ResponseKind::Suggested, move |_| {
                let name = text.borrow().trim().to_string();
                if !name.is_empty() {
                    accept(name);
                }
            }),
    );
}
/// Marks a store error as "name already taken" so the UI can localise it
/// without matching on English text.
const DUPLICATE_MARK: &str = "\u{1}duplicate:";

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
            Err(message) if message.starts_with(DUPLICATE_MARK) => {
                let name = &message[DUPLICATE_MARK.len()..];
                overlays.toast(tr!("sound-preset-exists", name = name.to_string()))
            }
            Err(raw) => super::super::report(overlays, &tr!("sound-preset-change-failed"), &raw),
        },
    );
}

fn new_preset(fx: Fx) {
    ask_name(
        fx,
        &tr!("sound-preset-new-title"),
        "",
        &tr!("sound-preset-create"),
        move |name| {
            change(fx, move |store, direction| {
                if store.exists(direction, &name) {
                    return Err(format!("{DUPLICATE_MARK}{name}"));
                }
                store
                    .save(&Preset::new(name.clone(), direction))
                    .map_err(|e| e.0)?;
                Ok(Some(name))
            });
        },
    );
}

fn duplicate(fx: Fx, name: String) {
    change(fx, move |store, direction| {
        store.duplicate(direction, &name).map(Some).map_err(|e| e.0)
    });
}

fn rename(fx: Fx, name: String) {
    let initial = name.clone();
    ask_name(
        fx,
        &tr!("sound-preset-rename-title"),
        &initial,
        &tr!("sound-preset-rename"),
        move |new_name| {
            let name = name.clone();
            change(fx, move |store, direction| {
                store.rename(direction, &name, &new_name).map_err(|e| e.0)?;
                Ok(Some(new_name))
            });
        },
    );
}

fn confirm_delete(fx: Fx, name: String) {
    fx.overlays.show_dialog(
        dialog(
            tr!("sound-preset-delete-title", name = name.clone()),
            &tr!("sound-preset-delete-warning"),
        )
        .cancel(&tr!("sound-action-cancel"))
        .default_action(CANCEL_ID)
        .response(
            &tr!("sound-preset-delete"),
            ResponseKind::Destructive,
            move |_| {
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
            },
        ),
    );
}

fn import_file(fx: Fx) {
    choose(
        fx.window,
        ChooserRequest::open()
            .title(&tr!("sound-preset-import-title"))
            .filter_spec(&tr!("sound-preset-filter"), "*.json *.txt")
            .filter_spec(&tr!("sound-file-filter-all"), "*"),
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
                .map_err(|e| tr!("sound-preset-read-failed", error = e.to_string()))?;
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
                            tr!("sound-preset-imported", name = stem.clone()),
                            tr!("sound-preset-import-approximate", lines = lines.join("\n")),
                        )
                        .cancel(&tr!("sound-action-close")),
                    );
                }
                match chosen {
                    Some(name) => select(fx, name),
                    None => fx.reload(),
                }
            }
            Err(raw) => super::super::report(overlays, &tr!("sound-preset-import-failed"), &raw),
        },
    );
}

fn export_file(fx: Fx, name: String) {
    let direction = fx.active_direction();
    let overlays = fx.overlays;
    let file_name = format!("{name}.json");
    choose(
        fx.window,
        ChooserRequest::save(file_name).title(&tr!("sound-preset-export-title")),
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
                    Ok(()) => overlays.toast(&tr!("sound-preset-exported")),
                    Err(raw) => {
                        super::super::report(overlays, &tr!("sound-preset-export-failed"), &raw)
                    }
                },
            );
        },
    );
}
