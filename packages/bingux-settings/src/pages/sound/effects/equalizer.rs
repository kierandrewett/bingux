//! The parametric equalizer's view: the response curve of the whole
//! equalizer, the preamp, and a row for each band with its type, frequency,
//! gain and Q. The curve is computed from the same filter coefficients the
//! audio path uses (`engine::eq`), and follows every edit as it is made.

use bingux_audio_effects::catalog::{Control, Unit};
use bingux_audio_effects::eq;
use bingux_audio_effects::graph::{BandField, Live};
use bingux_audio_effects::preset::{Band, BandKind, PluginSlot};
use freya::prelude::*;

use bingux_ui::charts::{Series, line_chart};
use bingux_ui::prelude::dropdown;
use bingux_ui::style::*;

use super::pane::Card;
use super::{Effect, Fx};
use crate::ui::{Icon, level_row, row, switch_row};
use bingux_ui::theme::Tokens;

/// The lowest and highest frequency of the curve, in Hz.
const LOW: f64 = 20.0;
const HIGH: f64 = 20_000.0;
/// Points on the curve.
const POINTS: usize = 96;

fn preamp_control() -> Control {
    Control {
        name: "Preamp".to_string(),
        min: -24.0,
        max: 24.0,
        default: 0.0,
        log: false,
        integer: false,
        toggle: false,
        unit: Unit::Db,
    }
}

fn frequency_control() -> Control {
    Control {
        name: "Frequency".to_string(),
        min: LOW,
        max: HIGH,
        default: 1000.0,
        log: true,
        integer: false,
        toggle: false,
        unit: Unit::Hz,
    }
}

fn gain_control() -> Control {
    Control {
        name: "Gain".to_string(),
        min: -24.0,
        max: 24.0,
        default: 0.0,
        log: false,
        integer: false,
        toggle: false,
        unit: Unit::Db,
    }
}

fn q_control() -> Control {
    Control {
        name: "Q".to_string(),
        min: 0.1,
        max: 10.0,
        default: 1.0,
        log: true,
        integer: false,
        toggle: false,
        unit: Unit::None,
    }
}

/// Text for a point on the frequency axis, which plots the logarithm of the frequency.
fn frequency_label(log_frequency: f64) -> String {
    let frequency = 10f64.powf(log_frequency);
    if frequency >= 1000.0 {
        format!("{} kHz", trim(frequency / 1000.0))
    } else {
        format!("{} Hz", trim(frequency))
    }
}

fn trim(value: f64) -> String {
    let text = if value >= 100.0 { format!("{value:.0}") } else { format!("{value:.1}") };
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

fn curve(slot: &PluginSlot, tokens: &Tokens) -> Element {
    let preamp = slot.preamp.unwrap_or(0.0);
    let points = eq::log_frequencies(LOW, HIGH, POINTS)
        .into_iter()
        .map(|frequency| (frequency.log10(), eq::response_db(preamp, &slot.bands, frequency)));
    let chart = line_chart([Series::new("Gain", points)])
        .x_format(frequency_label)
        .y_format(|db| format!("{:+} dB", db.round() as i64))
        .height(Dim::Chart)
        .label("Equalizer response");
    rect()
        .tw(&tokens, &[w_full(), px(GUTTER), py(S2)])
        .child(chart)
        .into()
}

/// The cards of an equalizer: the response curve across the pane, the
/// preamp, and one card for each band.
pub(super) fn cards(fx: Fx, slot: &PluginSlot, tokens: &Tokens) -> Vec<Card> {
    let id = slot.id.clone();
    let preamp = slot.preamp.unwrap_or(0.0);
    let control = preamp_control();
    let preamp_row: Element = level_row("Preamp", control.position_of(preamp) * 100.0, {
        let id = id.clone();
        let range = control.clone();
        move |position: f64| set_preamp(fx, &id, preamp, range.value_at(position / 100.0))
    })
    .value_text(control.unit.format(preamp))
    .into();
    let mut cards = vec![Card::new("Response", vec![curve(slot, tokens)]).wide(), Card::new("Preamp", vec![preamp_row])];
    for (index, band) in slot.bands.iter().enumerate() {
        cards.push(band_card(fx, &slot.id, index, band));
    }
    let add_id = slot.id.clone();
    cards.push(Card::new(
        "New band",
        vec![
            row("Add band")
                .leading(Icon::ListAdd)
                .on_press(move |_| {
                    let id = add_id.clone();
                    fx.edit(
                        move |preset| {
                            if let Some(slot) = preset.slot_mut(&id) {
                                slot.bands.push(Band::peaking(1000.0, 0.0, 1.0));
                            }
                        },
                        Effect::Restart,
                    );
                })
                .into(),
        ],
    ));
    cards
}

/// The preamp changes the sound live when the chain already has a preamp
/// stage to set; one that appears or disappears is a new shape.
fn set_preamp(fx: Fx, id: &str, before: f64, db: f64) {
    let slot = id.to_string();
    let effect = if before != 0.0 && db != 0.0 {
        Effect::Live(vec![Live::Preamp {
            slot: slot.clone(),
            db,
        }])
    } else {
        Effect::Restart
    };
    fx.edit(
        move |preset| {
            if let Some(slot) = preset.slot_mut(&slot) {
                slot.preamp = Some(db);
            }
        },
        effect,
    );
}

/// One band as a card: its switch, type and three controls.
fn band_card(fx: Fx, slot_id: &str, index: usize, band: &Band) -> Card {
    let id = slot_id.to_string();
    let mut rows: Vec<Element> = Vec::new();
    let enabled_id = id.clone();
    rows.push(
        switch_row("Enabled", band.enabled, move |on: bool| {
            let id = enabled_id.clone();
            fx.edit(
                move |preset| {
                    if let Some(band) = preset.slot_mut(&id).and_then(|slot| slot.bands.get_mut(index)) {
                        band.enabled = on;
                    }
                },
                Effect::Restart,
            );
        })
        .into(),
    );
    let options: Vec<(String, String)> = BandKind::ALL
        .iter()
        .map(|kind| (kind.key().to_string(), kind.title().to_string()))
        .collect();
    let type_id = id.clone();
    rows.push(
        row("Type")
            .trailing(dropdown(options, band.kind.key(), move |chosen: String| {
                let id = type_id.clone();
                fx.edit(
                    move |preset| {
                        let kind = BandKind::ALL.into_iter().find(|kind| kind.key() == chosen);
                        if let (Some(kind), Some(band)) =
                            (kind, preset.slot_mut(&id).and_then(|slot| slot.bands.get_mut(index)))
                        {
                            band.kind = kind;
                        }
                    },
                    Effect::Restart,
                );
            }))
            .into(),
    );
    let frequency = frequency_control();
    rows.push(
        level_row("Frequency", frequency.position_of(band.frequency) * 100.0, {
            let (id, range) = (id.clone(), frequency.clone());
            move |position: f64| set_field(fx, &id, index, BandField::Frequency, range.value_at(position / 100.0))
        })
        .value_text(frequency.unit.format(band.frequency))
        .into(),
    );
    if band.kind.has_gain() {
        let gain = gain_control();
        rows.push(
            level_row("Gain", gain.position_of(band.gain) * 100.0, {
                let (id, range) = (id.clone(), gain.clone());
                move |position: f64| set_field(fx, &id, index, BandField::Gain, range.value_at(position / 100.0))
            })
            .value_text(gain.unit.format(band.gain))
            .into(),
        );
    }
    let q = q_control();
    rows.push(
        level_row("Q", q.position_of(band.q) * 100.0, {
            let (id, range) = (id.clone(), q.clone());
            move |position: f64| set_field(fx, &id, index, BandField::Q, range.value_at(position / 100.0))
        })
        .value_text(q.unit.format(band.q))
        .into(),
    );
    let remove_id = id;
    rows.push(
        row("Remove band")
            .leading(Icon::Delete)
            .destructive()
            .on_press(move |_| {
                let id = remove_id.clone();
                fx.edit(
                    move |preset| {
                        if let Some(slot) = preset.slot_mut(&id)
                            && index < slot.bands.len()
                        {
                            slot.bands.remove(index);
                        }
                    },
                    Effect::Restart,
                );
            })
            .into(),
    );
    Card::new(format!("Band {}, {}", index + 1, Unit::Hz.format(band.frequency)), rows)
}

/// Move one field of a band while the sound plays.
fn set_field(fx: Fx, id: &str, index: usize, field: BandField, value: f64) {
    let slot = id.to_string();
    let change = Live::Band {
        slot: slot.clone(),
        index,
        field,
        value,
    };
    fx.edit(
        move |preset| {
            if let Some(band) = preset.slot_mut(&slot).and_then(|slot| slot.bands.get_mut(index)) {
                match field {
                    BandField::Frequency => band.frequency = value,
                    BandField::Gain => band.gain = value,
                    BandField::Q => band.q = value,
                }
            }
        },
        Effect::Live(vec![change]),
    );
}
