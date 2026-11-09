//! What can be done with a calendar as a whole: add, rename, recolour,
//! remove, import events into it and export it.

use std::path::PathBuf;
use std::sync::Arc;

use bingux_calendar_core::recur::Change;
use bingux_calendar_core::store::Store;
use bingux_calendar_core::{Snapshot, ics};
use bingux_ui::calendar::palette::{accent_hex, calendar_accents, calendar_colour};
use bingux_ui::files::{ChooserRequest, Filter, choose};
use bingux_ui::prelude::*;
use bingux_ui::task::spawn_blocking;
use freya::prelude::*;

use crate::model::Model;

/// The accent names, as a colour choice offers them.
fn colour_options() -> Vec<(String, String)> {
    calendar_accents()
        .into_iter()
        .map(|name| {
            let mut label = name.to_string();
            if let Some(first) = label.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            (accent_hex(name), label)
        })
        .collect()
}

/// The swatches that go with `colour_options`, in the scheme on show.
fn colour_swatches(tokens: &Tokens) -> Vec<Color> {
    colour_options()
        .iter()
        .map(|(hex, _)| calendar_colour(Some(hex), 0, tokens.scheme.is_dark()))
        .collect()
}

/// Runs `work` against the store on a worker, then reads the store again.
fn on_store(
    model: Model,
    work: impl FnOnce(&Store, &Snapshot) -> Result<(), String> + Send + 'static,
    overlays: Overlays,
) {
    let Some(backend) = model.backend.read().clone() else {
        return;
    };
    let snapshot = model.data();
    spawn(async move {
        let result = spawn_blocking(move || work(&backend.store, &snapshot)).await;
        if result.is_err() {
            overlays.toast(tr!("calendar-change-failed"));
        }
        model.reload();
    });
}

/// The dialog that asks for a new calendar's name and colour.
pub fn add(model: Model, overlays: Overlays, tokens: &Tokens) {
    let mut name = model.draft_name;
    let mut colour = model.draft_colour;
    name.set(String::new());
    let taken = model.data().calendars.len();
    let options = colour_options();
    colour.set(options[taken % options.len()].0.clone());
    let content = vstack(S2)
        .child(
            text_field(name, move |_: String| {})
                .placeholder(tr!("calendar-name-placeholder"))
                .autofocus()
                .fill_width(),
        )
        .child(
            dropdown(options, colour.peek().clone(), move |key: String| {
                colour.set(key)
            })
            .colours(colour_swatches(tokens)),
        );
    overlays.show_dialog(
        dialog(
            tr!("calendar-new-calendar"),
            tr!("calendar-new-calendar-description"),
        )
        .content(content)
        .cancel(tr!("calendar-cancel"))
        .response(tr!("calendar-add"), ResponseKind::Suggested, move |_| {
            let text = name.peek().trim().to_string();
            if text.is_empty() {
                return;
            }
            let hex = colour.peek().clone();
            on_store(
                model,
                move |store, snapshot| {
                    store
                        .create_calendar(&text, Some(&hex), &snapshot.calendars)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                },
                overlays,
            );
        }),
    );
}

/// The dialog that renames a calendar.
pub fn rename(model: Model, overlays: Overlays, id: String, current: String) {
    let mut name = model.draft_name;
    name.set(current);
    overlays.show_dialog(
        dialog(tr!("calendar-rename-title"), "")
            .content(
                text_field(name, move |_: String| {})
                    .autofocus()
                    .select_all_on_open()
                    .fill_width(),
            )
            .cancel(tr!("calendar-cancel"))
            .response(tr!("calendar-rename"), ResponseKind::Suggested, move |_| {
                let text = name.peek().trim().to_string();
                if text.is_empty() {
                    return;
                }
                let id = id.clone();
                on_store(
                    model,
                    move |store, _| store.rename_calendar(&id, &text).map_err(|e| e.to_string()),
                    overlays,
                );
            }),
    );
}

/// Gives a calendar a colour.
pub fn recolour(model: Model, overlays: Overlays, id: String, hex: String) {
    on_store(
        model,
        move |store, _| {
            store
                .set_calendar_colour(&id, &hex)
                .map_err(|e| e.to_string())
        },
        overlays,
    );
}

/// The colours a calendar can be given, for a menu.
pub fn colours() -> Vec<(String, String)> {
    colour_options()
}

/// Asks before a calendar and its events are removed.
pub fn remove(model: Model, overlays: Overlays, id: String, name: String, count: usize) {
    let events = tr!("calendar-remove-events", count = count as i64);
    overlays.show_dialog(
        dialog(tr!("calendar-remove-title", name = name.as_str()), events)
            .icon(DialogIcon::Warning)
            .cancel(tr!("calendar-cancel"))
            .response(
                tr!("calendar-delete"),
                ResponseKind::Destructive,
                move |_| {
                    let id = id.clone();
                    on_store(
                        model,
                        move |store, _| store.delete_calendar(&id).map_err(|e| e.to_string()),
                        overlays,
                    );
                },
            ),
    );
}

/// Asks for a file of events, then what to do with it.
pub fn import(model: Model, window: Window, overlays: Overlays) {
    choose(
        window,
        ChooserRequest::open()
            .title(tr!("calendar-import-events"))
            .accept_label(tr!("calendar-import"))
            .filter(
                Filter::new(tr!("calendar-files-filter"))
                    .glob("*.ics")
                    .glob("*.ical")
                    .glob("*.ifb"),
            )
            .filter(Filter::new(tr!("calendar-all-files-filter"))),
        move |chosen| {
            if let Some(chosen) = chosen {
                for path in chosen.paths {
                    import_file(model, overlays, path);
                }
            }
        },
    );
}

/// Why a file could not be imported.
enum Unreadable {
    Read,
    NotCalendar,
}

/// Reads a file of events and asks which calendar they go in.
pub fn import_file(model: Model, overlays: Overlays, path: PathBuf) {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    spawn(async move {
        let read = path.clone();
        let parsed = spawn_blocking(move || {
            let bytes = std::fs::read(&read).map_err(|_| Unreadable::Read)?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            ics::parse(&text, "").map_err(|_| Unreadable::NotCalendar)
        })
        .await;
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(Unreadable::Read) => {
                overlays.toast(format!(
                    "Could not read {name}. Check that you can open the file."
                ));
                return;
            }
            Err(Unreadable::NotCalendar) => {
                overlays.toast(format!(
                    "Could not import {name}. It is not a calendar file."
                ));
                return;
            }
        };
        if parsed.items.is_empty() {
            overlays.toast(format!("{name} has no events to import."));
            return;
        }
        confirm_import(model, overlays, name, Arc::new(parsed));
    });
}

/// "Import 12 events from holidays.ics?" with the calendar they go in.
fn confirm_import(model: Model, overlays: Overlays, name: String, parsed: Arc<ics::Parsed>) {
    let data = model.data();
    let writable: Vec<_> = data.calendars.iter().filter(|c| !c.read_only).collect();
    if writable.is_empty() {
        overlays.toast(tr!("calendar-import-no-calendar"));
        return;
    }
    let mut target = model.draft_target;
    target.set(model.default_calendar().unwrap_or_default());
    let options: Vec<(String, String)> = writable
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();
    let count = parsed.items.len();
    let message = match count {
        1 => format!("1 event from {name}."),
        n => format!("{n} events from {name}."),
    };
    let note = if parsed.warnings.is_empty() {
        message
    } else {
        format!("{message} {}", parsed.warnings[0])
    };
    overlays.show_dialog(
        dialog(tr!("calendar-import-events"), note)
            .content(dropdown(
                options,
                target.peek().clone(),
                move |id: String| target.set(id),
            ))
            .cancel(tr!("calendar-cancel"))
            .response(tr!("calendar-import"), ResponseKind::Suggested, move |_| {
                let calendar = target.peek().clone();
                let mut items = parsed.items.clone();
                // An event already in the store keeps its calendar's copy
                // replaced; the rest are new.
                let existing = model.data();
                let changes: Vec<Change> = items
                    .drain(..)
                    .map(|mut item| {
                        item.calendar = calendar.clone();
                        if let Some(old) = existing.item(&item.uid) {
                            item.calendar = old.calendar.clone();
                        }
                        Change::Save(item)
                    })
                    .collect();
                model.apply(changes);
                overlays.toast(match count {
                    1 => "Imported 1 event.".to_string(),
                    n => format!("Imported {n} events."),
                });
            }),
    );
}

/// Writes a calendar's events to a file the person names.
pub fn export(model: Model, window: Window, overlays: Overlays, id: String, name: String) {
    let text = {
        let data = model.data();
        let Some(backend) = model.backend.read().clone() else {
            return;
        };
        backend.store.export(&data, &id)
    };
    choose(
        window,
        ChooserRequest::save(format!("{name}.ics"))
            .title(tr!("calendar-export-title"))
            .accept_label(tr!("calendar-export"))
            .filter(Filter::new(tr!("calendar-files-filter")).glob("*.ics")),
        move |chosen| {
            let Some(path) = chosen.and_then(|chosen| chosen.paths.into_iter().next()) else {
                return;
            };
            let text = text.clone();
            spawn(async move {
                let written = spawn_blocking(move || std::fs::write(&path, text.as_bytes())).await;
                overlays.toast(if written.is_ok() {
                    tr!("calendar-exported")
                } else {
                    tr!("calendar-export-failed")
                });
            });
        },
    );
}
