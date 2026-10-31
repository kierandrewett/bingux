//! The equalizer page: a bank of faders, one for each band (its gain), with
//! the band's frequency and Q under it, and the selected band's full editor
//! (type, frequency, Q, gain, slope, mute) under the bank. The mode (IIR, FIR,
//! FFT, SPM) is a menu above. Both sides of the stereo image get the same
//! bands, except in the mid/side equalizer, where Mid and Side are edited one
//! at a time.

use bingux_audio_effects::catalog::{Control, PluginInfo};
use bingux_audio_effects::eq::{self, BANDS, BandKind, Side, Symbols};
use bingux_audio_effects::{PluginSlot, Unit};
use bingux_ui::prelude::{dropdown, fader, segmented};
use bingux_ui::style::*;
use freya::prelude::*;

use super::Fx;
use super::page::level_row;
use super::page::{control_row, set_control};
use crate::ui::{Group, action_button, caption, hstack, row, switch_row, use_tokens};

/// The gain at the top and bottom of a fader, in dB.
const RANGE_DB: f64 = 24.0;

#[derive(PartialEq)]
pub(super) struct EqPage {
    pub fx: Fx,
    pub slot: PluginSlot,
    pub info: PluginInfo,
    pub mid_side: bool,
}

/// A frequency in the room of a band's strip: 60, 250, 1k, 8k, 15k.
fn short_hz(hz: f64) -> String {
    if hz >= 1000.0 {
        let k = hz / 1000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{k:.0}k")
        } else {
            format!("{k:.1}k")
        }
    } else {
        format!("{hz:.0}")
    }
}

fn db_of(linear: f64) -> f64 {
    20.0 * linear.max(1e-4).log10()
}

impl Component for EqPage {
    fn render(&self) -> impl IntoElement {
        let (fx, slot, info, mid_side) = (self.fx, &self.slot, &self.info, self.mid_side);
        let tokens = use_tokens();
        let mut chosen = use_state(|| 0usize);
        let mut side_state = use_state(|| 0usize);
        let sides: Vec<Side> = if mid_side {
            vec![if *side_state.read() == 0 {
                Side::First
            } else {
                Side::Second
            }]
        } else {
            vec![Side::First, Side::Second]
        };
        let lead = sides[0];
        let value = |symbol: &str| -> f64 {
            let control = info.control(symbol);
            slot.value(symbol, control.map_or(0.0, |control| control.default))
        };
        let bands = (0..BANDS)
            .rev()
            .find(|index| value(&Symbols::of(*index, lead, mid_side).kind) != 0.0)
            .map_or(1, |last| last + 1);
        let selected = (*chosen.read()).min(bands - 1);
        let id = slot.id.clone();

        let mut mode_row = Group::titled("Mode");
        if let Some(control) = info.control("mode") {
            mode_row = mode_row.row(control_row(fx, &slot.id, control, value("mode")));
        }
        if mid_side {
            let tabs = segmented(
                vec![
                    ("0".to_string(), "Mid".to_string()),
                    ("1".to_string(), "Side".to_string()),
                ],
                &side_state.read().to_string(),
                move |picked: String| side_state.set(picked.parse().unwrap_or(0)),
            );
            mode_row = mode_row.row(row("Channel").trailing(tabs));
        }

        let mut strips = rect().tw(&tokens, &[flex_row(), w_full(), gap(S1), py(S2)]);
        for index in 0..bands {
            let symbols = Symbols::of(index, lead, mid_side);
            let active = value(&symbols.kind) != 0.0;
            let gain_db = db_of(value(&symbols.gain)).clamp(-RANGE_DB, RANGE_DB);
            let frequency = value(&symbols.frequency);
            let on_id = id.clone();
            let gain_sides = sides.clone();
            let strip_fader = fader(
                (gain_db + RANGE_DB) / (2.0 * RANGE_DB) * 100.0,
                move |percent: f64| {
                    let db = (percent / 100.0 * 2.0 * RANGE_DB - RANGE_DB).round();
                    for side in &gain_sides {
                        set_control(
                            fx,
                            &on_id,
                            &Symbols::of(index, *side, mid_side).gain,
                            Unit::Gain.stored(db),
                        );
                    }
                },
            )
            .step(100.0 / (2.0 * RANGE_DB))
            .enabled(active)
            .label(format!("Band {} gain", index + 1));
            let pick = if index == selected {
                Paint::Accent
            } else {
                Paint::Control
            };
            strips = strips.child(
                rect()
                    .tw(
                        &tokens,
                        &[flex_col(), items(Align::Center), gap(S1), w(Dim::Box9)],
                    )
                    .child(
                        rect()
                            .tw(
                                &tokens,
                                &[
                                    flex_col(),
                                    items(Align::Center),
                                    px(S2),
                                    py(S1),
                                    rounded(Radius::Pill),
                                    bg(pick),
                                ],
                            )
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt(format!("Edit band {}", index + 1))
                            .on_pointer_down(move |_| chosen.set(index))
                            .child(caption(format!("{}", index + 1))),
                    )
                    .child(strip_fader)
                    .child(caption(short_hz(frequency)))
                    .child(caption(format!("{:+.0}", gain_db))),
            );
        }

        let editor = band_editor(fx, slot, info, &sides, mid_side, selected);
        let add_id = id.clone();
        let (add_sides, remove_id, remove_sides) = (sides.clone(), id.clone(), sides.clone());
        let actions = hstack(S2)
            .child(
                action_button("Add band", move |_| {
                    if bands < BANDS {
                        chosen.set(bands);
                        for side in &add_sides {
                            let symbols = Symbols::of(bands, *side, mid_side);
                            set_control(fx, &add_id, &symbols.kind, BandKind::Bell.code());
                            set_control(fx, &add_id, &symbols.frequency, 1000.0);
                            set_control(fx, &add_id, &symbols.quality, eq::DEFAULT_Q);
                            set_control(fx, &add_id, &symbols.gain, 1.0);
                        }
                    }
                })
                .enabled(bands < BANDS)
                .compact(),
            )
            .child(
                action_button("Remove band", move |_| {
                    if bands > 1 {
                        for side in &remove_sides {
                            set_control(
                                fx,
                                &remove_id,
                                &Symbols::of(bands - 1, *side, mid_side).kind,
                                0.0,
                            );
                        }
                    }
                })
                .enabled(bands > 1)
                .compact(),
            )
            .child(
                action_button("Flat response", {
                    let (flat_id, flat_sides) = (id.clone(), sides.clone());
                    move |_| {
                        for index in 0..BANDS {
                            for side in &flat_sides {
                                set_control(
                                    fx,
                                    &flat_id,
                                    &Symbols::of(index, *side, mid_side).gain,
                                    1.0,
                                );
                            }
                        }
                    }
                })
                .compact(),
            );
        rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
            .child(mode_row)
            .child(
                Group::titled("Bands").content(
                    rect()
                        .tw(
                            &tokens,
                            &[flex_col(), w_full(), gap(S3), px(GUTTER), py(S3)],
                        )
                        .child(strips)
                        .child(actions),
                ),
            )
            .child(editor)
    }
}

/// The editor of one band.
fn band_editor(
    fx: Fx,
    slot: &PluginSlot,
    info: &PluginInfo,
    sides: &[Side],
    mid_side: bool,
    index: usize,
) -> Element {
    let lead = Symbols::of(index, sides[0], mid_side);
    let value = |symbol: &str| {
        slot.value(
            symbol,
            info.control(symbol).map_or(0.0, |control| control.default),
        )
    };
    let mut rows: Vec<Element> = Vec::new();
    let id = slot.id.clone();
    let rows_for: [(&str, fn(&Symbols) -> &String); 5] = [
        ("Type", |s| &s.kind),
        ("Frequency", |s| &s.frequency),
        ("Quality factor", |s| &s.quality),
        ("Slope", |s| &s.slope),
        ("Mute", |s| &s.mute),
    ];
    for (title, symbol_of) in rows_for {
        let symbol = symbol_of(&lead).clone();
        let Some(control) = info.control(&symbol) else {
            continue;
        };
        let mut named = control.clone();
        named.name = title.to_string();
        // A change goes to every side being edited.
        let sides: Vec<Side> = sides.to_vec();
        let id = id.clone();
        let current = value(&symbol);
        rows.push(side_row(
            fx, &id, &named, current, &sides, mid_side, index, symbol_of,
        ));
    }
    if let Some(control) = info.control(&lead.gain) {
        let (id, sides) = (id.clone(), sides.to_vec());
        let db = db_of(value(&lead.gain)).clamp(-RANGE_DB, RANGE_DB);
        let range = (control.min, control.max);
        rows.push(
            level_row(
                "Gain",
                (db + RANGE_DB) / (2.0 * RANGE_DB) * 100.0,
                move |percent: f64| {
                    let db = ((percent / 100.0 * 2.0 * RANGE_DB - RANGE_DB) * 2.0).round() / 2.0;
                    for side in &sides {
                        set_control(
                            fx,
                            &id,
                            &Symbols::of(index, *side, mid_side).gain,
                            Unit::Gain.stored(db).clamp(range.0, range.1),
                        );
                    }
                },
            )
            .value_text(format!("{db:+.1} dB"))
            .into(),
        );
    }
    Group::titled(&format!("Band {}", index + 1))
        .rows(rows)
        .into()
}

/// One row of the band editor that writes to every side.
#[allow(clippy::too_many_arguments)]
fn side_row(
    fx: Fx,
    id: &str,
    control: &Control,
    value: f64,
    sides: &[Side],
    mid_side: bool,
    index: usize,
    symbol_of: fn(&Symbols) -> &String,
) -> Element {
    let sides: Vec<Side> = sides.to_vec();
    let id = id.to_string();
    let apply = move |new: f64| {
        for side in &sides {
            set_control(
                fx,
                &id,
                symbol_of(&Symbols::of(index, *side, mid_side)),
                new,
            );
        }
    };
    if control.toggle {
        return switch_row(control.name.clone(), value >= 0.5, move |on: bool| {
            apply(f64::from(u8::from(on)))
        })
        .into();
    }
    if !control.choices.is_empty() {
        let options: Vec<(String, String)> = control
            .choices
            .iter()
            .map(|choice| (choice.value.to_string(), choice.label.clone()))
            .collect();
        let current = control
            .choices
            .iter()
            .find(|choice| (choice.value - value).abs() < 1e-6)
            .map_or_else(
                || control.choices[0].value.to_string(),
                |choice| choice.value.to_string(),
            );
        return row(control.name.clone())
            .trailing(dropdown(options, current, move |picked: String| {
                apply(picked.parse().unwrap_or(0.0))
            }))
            .into();
    }
    let range = control.clone();
    level_row(
        control.name.clone(),
        control.position_of(value) * 100.0,
        move |position: f64| apply(range.value_at(position / 100.0)),
    )
    .value_text(control.display(value))
    .into()
}
