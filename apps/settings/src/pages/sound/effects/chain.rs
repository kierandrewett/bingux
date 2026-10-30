//! The list of effects: one row for each plugin of the chain, in the order
//! the sound passes through them. A row has the plugin's name, its power
//! switch, a delete button and a grip to drag it to another place. The
//! selected row is highlighted and its controls are shown in the pane
//! (`pane.rs`); on a phone a row opens the effect's own page.

use bingux_audio_effects::catalog::PluginInfo;
use bingux_audio_effects::preset::PluginSlot;
use freya::prelude::*;

use bingux_ui::prelude::{icon_button, sortable, use_router};
use bingux_ui::style::*;

use super::{Effect, Fx, Loaded, Runtime, picker, token};
use crate::routes;
use crate::ui::{Group, Icon, Status, action_button, hstack, row, status, switch};

/// The list of effects, with the buttons that add one and hide the list, and
/// the device the chain works on.
pub(super) fn list(fx: Fx, data: &Loaded, runtime: &Runtime) -> Element {
    let compact = is_compact();
    let catalog = fx.catalog.read().clone();
    let selected = fx.selected_id(data);
    let router = use_router();
    let direction = data.direction;

    let mut buttons = hstack(S2).child(
        action_button("Add effect", move |_| picker::open(fx))
            .icon(Icon::ListAdd)
            .compact(),
    );
    if !compact {
        buttons = buttons.child(
            action_button("Close", move |_| {
                let mut shown = fx.list_shown;
                shown.set(false);
            })
            .compact(),
        );
    }

    let plugins = &data.preset.plugins;
    let body: Element = if plugins.is_empty() {
        Group::new(None::<String>)
            .row(status(Status::Empty("No effects.".to_string())))
            .into()
    } else {
        let rows: Vec<Element> = plugins
            .iter()
            .map(|slot| {
                let info = catalog.as_ref().and_then(|catalog| catalog.find(&slot.kind));
                let id = slot.id.clone();
                let name = display_name(slot, info);
                let power_id = id.clone();
                let delete_id = id.clone();
                let press_id = id.clone();
                let press_name = name.clone();
                let controls = hstack(S1)
                    .child(switch(slot.enabled, move |on: bool| {
                        let id = power_id.clone();
                        fx.edit(
                            move |preset| {
                                if let Some(slot) = preset.slot_mut(&id) {
                                    slot.enabled = on;
                                }
                            },
                            Effect::Restart,
                        );
                    }))
                    .child(
                        icon_button(Icon::Delete, move |_| remove_plugin(fx, &delete_id))
                            .tooltip("Remove")
                            .danger(),
                    );
                let mut item = row(name)
                    .selected(selected.as_deref() == Some(slot.id.as_str()) && !compact)
                    .trailing(controls)
                    .on_press(move |_| {
                        fx.select(&press_id);
                        if compact {
                            router.open(routes::sound_effect(&token(direction, &press_id), &press_name));
                        }
                    });
                if compact {
                    item = item.navigates();
                }
                item.into()
            })
            .collect();
        let labels: Vec<String> = plugins
            .iter()
            .map(|slot| display_name(slot, catalog.as_ref().and_then(|catalog| catalog.find(&slot.kind))))
            .collect();
        sortable(rows, move |(from, to): (usize, usize)| move_plugin(fx, from, to))
            .labels(labels)
            .into()
    };

    let (title, device) = match direction {
        bingux_audio_effects::Direction::Output => ("Output device", "Speakers"),
        bingux_audio_effects::Direction::Input => ("Input device", "Microphone"),
    };
    let device_name = runtime.device_label.clone().unwrap_or_else(|| device.to_string());
    hstack(S0)
        .tw_layout(&[flex_col(), w_full(), gap(S4), items(Align::Start)])
        .child(buttons)
        .child(body)
        .child(Group::titled(title).row(row(device_name)))
        .into()
}

pub(super) fn move_plugin(fx: Fx, from: usize, to: usize) {
    fx.edit(
        |preset| {
            if from < preset.plugins.len() && to < preset.plugins.len() {
                let slot = preset.plugins.remove(from);
                preset.plugins.insert(to, slot);
            }
        },
        Effect::Restart,
    );
}

pub(super) fn remove_plugin(fx: Fx, id: &str) {
    let id = id.to_string();
    fx.edit(move |preset| preset.plugins.retain(|slot| slot.id != id), Effect::Restart);
}

/// The name a plugin is shown by.
pub(super) fn display_name(slot: &PluginSlot, info: Option<&PluginInfo>) -> String {
    match info {
        Some(info) if info.id == bingux_audio_effects::preset::EQUALIZER => "Equalizer".to_string(),
        Some(info) => info.name.strip_prefix("Bingux ").unwrap_or(&info.name).to_string(),
        None => slot.kind.rsplit(':').next().unwrap_or(&slot.kind).to_string(),
    }
}
