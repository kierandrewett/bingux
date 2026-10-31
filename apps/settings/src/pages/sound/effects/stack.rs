//! The stack: the effects in the order the sound passes through them, under a
//! section header that names the direction. A row has the effect's icon and
//! name, its power switch, a delete button and a grip to drag it to another
//! place. The selected row is highlighted and its page is shown beside the
//! stack (pushed over it on a phone). The device the sound goes to closes the
//! stack, and the level of the whole chain, before and after, sits under it.
//!
//! Keyboard: Up and Down move between rows, Space switches the row's effect on
//! or off, Delete removes it, Alt+Up and Alt+Down move it in the stack (see
//! `sortable`).

use bingux_audio_effects::effects::{self, Category};
use bingux_audio_effects::{Direction, PluginSlot};
use bingux_ui::prelude::{icon_button, level_meter, sortable};
use bingux_ui::style::*;
use freya::prelude::*;

use super::super::meter::{LevelMeter, Source};
use super::{Fx, Loaded, Runtime, picker};
use crate::ui::{
    Group, Icon, Status, action_button, caption, hstack, row, status, switch, use_tokens,
};

/// The stack as a component of its own, so the popup's hooks do not change
/// when the window changes between the desktop and the phone layout.
#[derive(PartialEq)]
pub(super) struct StackView {
    pub fx: Fx,
    pub data: Loaded,
    pub runtime: Runtime,
    pub selected: Option<String>,
}

fn icon_of(kind: &str) -> Icon {
    match effects::find(kind).map(|effect| effect.category) {
        Some(Category::Equalizer) => Icon::PreferencesOther,
        Some(Category::Dynamics) => Icon::AudioVolumeMedium,
        Some(Category::Space) => Icon::Speakers,
        Some(Category::Enhance | Category::Stereo) => Icon::Headphones,
        Some(Category::Pitch) => Icon::AudioInputMicrophone,
        _ => Icon::Sound,
    }
}

/// The name an effect is shown by.
pub(super) fn display_name(slot: &PluginSlot) -> String {
    effects::find(&slot.kind).map_or_else(|| slot.kind.clone(), |effect| effect.name.to_string())
}

impl Component for StackView {
    fn render(&self) -> impl IntoElement {
        let (fx, data, runtime) = (self.fx, &self.data, &self.runtime);
        let tokens = use_tokens();
        let compact = is_compact();
        let direction = data.direction;
        let plugins = &data.preset.plugins;

        let body: Element = if plugins.is_empty() {
            Group::new(None::<String>)
                .row(status(Status::Empty(
                    "No effects yet. Add one to start.".to_string(),
                )))
                .into()
        } else {
            let rows: Vec<Element> = plugins
                .iter()
                .map(|slot| {
                    let id = slot.id.clone();
                    let (power_id, delete_id, press_id) = (id.clone(), id.clone(), id.clone());
                    let name = display_name(slot);
                    let controls = hstack(S1)
                        .child(switch(slot.enabled, move |on: bool| {
                            let id = power_id.clone();
                            fx.edit(move |preset| {
                                if let Some(slot) = preset.slot_mut(&id) {
                                    slot.enabled = on;
                                }
                            });
                        }))
                        .child(
                            icon_button(Icon::Delete, move |_| remove(fx, &delete_id))
                                .tooltip("Remove")
                                .danger(),
                        );
                    let mut item = row(name)
                        .leading(icon_of(&slot.kind))
                        .selected(self.selected.as_deref() == Some(slot.id.as_str()) && !compact)
                        .trailing(controls)
                        .on_press(move |_| {
                            fx.select(&press_id);
                            let mut open = fx.page_open;
                            open.set(true);
                        });
                    if let Some(reason) = runtime.failures.get(&slot.id) {
                        item = item
                            .subtitle(reason.clone())
                            .leading_tone(bingux_ui::prelude::Tone::Warning);
                    } else if !slot.enabled {
                        item = item.subtitle("Off");
                    }
                    if compact {
                        item = item.navigates();
                    }
                    // Delete on a focused row removes its effect; Space on its
                    // switch toggles it; Alt+Up and Alt+Down move it (`sortable`).
                    let key_id = id.clone();
                    rect()
                        .width(Size::fill())
                        .on_key_down(move |event: Event<KeyboardEventData>| {
                            if matches!(event.key, Key::Named(NamedKey::Delete)) {
                                remove(fx, &key_id);
                            }
                        })
                        .child(item)
                        .into()
                })
                .collect();
            let labels: Vec<String> = plugins.iter().map(display_name).collect();
            sortable(rows, move |(from, to): (usize, usize)| {
                fx.edit(|preset| preset.move_slot(from, to));
            })
            .labels(labels)
            .into()
        };

        let (header, end) = match direction {
            Direction::Output => ("Output", "Output device"),
            Direction::Input => ("Input", "Input device"),
        };
        let device = runtime
            .device_label
            .clone()
            .unwrap_or_else(|| match direction {
                Direction::Output => "Speakers".to_string(),
                Direction::Input => "Microphone".to_string(),
            });
        let live = runtime.running && runtime.virtual_node.is_some();
        let (before, after) = match direction {
            Direction::Output => (
                (runtime.virtual_node, Source::Output),
                (runtime.device_node, Source::Output),
            ),
            Direction::Input => (
                (runtime.device_node, Source::Input),
                (runtime.virtual_node, Source::Input),
            ),
        };
        let meter = |label: &'static str, (node, source): (Option<u32>, Source)| -> Element {
            hstack(S3)
                .tw_layout(&[w_full(), items(Align::Center)])
                .child(rect().tw(&tokens, &[w(Dim::Box9)]).child(caption(label)))
                .child(rect().width(Size::flex(1.0)).child(LevelMeter {
                    node: node.unwrap_or(0),
                    source,
                    compact: false,
                    live: live && node.is_some(),
                }))
                .into()
        };
        let _ = level_meter;
        rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
            .child(
                hstack(S2)
                    .tw_layout(&[w_full(), items(Align::Center), justify(Align::Between)])
                    .child(caption(header))
                    .child(
                        action_button("Add effect", move |_| picker::open(fx))
                            .icon(Icon::ListAdd)
                            .compact(),
                    ),
            )
            .child(body)
            .child(
                Group::titled(end).row(row(device)).row(Element::from(
                    rect()
                        .tw(
                            &tokens,
                            &[flex_col(), gap(S2), w_full(), px(GUTTER), py(S3)],
                        )
                        .child(meter("In", before))
                        .child(meter("Out", after)),
                )),
            )
    }
}

/// Take the effect `id` out of the stack. The sound changes first and the
/// selection moves to a neighbour.
fn remove(fx: Fx, id: &str) {
    let id = id.to_string();
    let next = {
        let data = fx.data.peek();
        let at = data
            .preset
            .plugins
            .iter()
            .position(|slot| slot.id == id)
            .unwrap_or(0);
        data.preset
            .plugins
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != at)
            .map(|(_, slot)| slot.id.clone())
            .nth(
                at.saturating_sub(1)
                    .min(data.preset.plugins.len().saturating_sub(2)),
            )
    };
    fx.edit(move |preset| {
        preset.remove(&id);
    });
    let mut selected = fx.selected;
    selected.set(next);
}
