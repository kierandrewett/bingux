//! The page of the selected effect: its name and Reset, the input and output
//! gain, then the controls as titled cards, and a footer that names the project
//! the plugin comes from.
//!
//! The equalizer, compressor, limiter, gate, expander, bass enhancer, reverb,
//! delay and filter have curated cards (`curated`): which controls, in which
//! order, under which title. Every other effect gets cards generated from the
//! plugin's own port metadata (range, default, scale, unit, enumeration
//! choices), so a plugin added to `effects.rs` has a page with no code here.

use bingux_audio_effects::catalog::{Control, PluginInfo};
use bingux_audio_effects::effects::{self, Effect};
use bingux_audio_effects::{PluginSlot, Unit};
use bingux_ui::prelude::{dropdown, slider};
use bingux_ui::style::*;
use freya::prelude::*;

use super::{Fx, Loaded, Runtime, eq_page};
use crate::ui::files::{ChooserRequest, choose};
use crate::ui::{
    Group, Status, action_button, caption, hstack, row, status, switch_row, use_tokens,
};

#[derive(PartialEq)]
pub(super) struct PageView {
    pub fx: Fx,
    pub data: Loaded,
    pub runtime: Runtime,
    pub selected: Option<String>,
}

/// The cards of a curated page: a title and the port symbols under it.
type Cards = &'static [(&'static str, &'static [&'static str])];

const SIDECHAIN: &[&str] = &["sct", "scm", "scr", "sla", "shpm", "shpf", "slpm", "slpf"];

fn curated(kind: &str) -> Option<Cards> {
    Some(match kind {
        "compressor" => &[
            ("Compression", &["cm", "al", "cr", "kn", "mk"]),
            ("Timing", &["at", "rt", "hold", "rrl"]),
            ("Mix", &["cdw"]),
            ("Side-chain", SIDECHAIN),
        ],
        "limiter" => &[
            ("Limit", &["mode", "th", "knee", "boost"]),
            ("Timing", &["lk", "at", "rt"]),
            ("Level regulation", &["alr", "alr_at", "alr_rt"]),
            ("Quality", &["ovs", "dith", "slink"]),
        ],
        "gate" => &[
            ("Gate", &["gt", "gz", "gr", "mk"]),
            ("Timing", &["at", "hold", "rt"]),
            ("Hysteresis", &["gh", "ht", "hz"]),
            ("Mix", &["cdw"]),
            ("Side-chain", SIDECHAIN),
        ],
        "expander" => &[
            ("Expansion", &["em", "al", "er", "kn", "mk"]),
            ("Timing", &["at", "rt", "hold"]),
            ("Mix", &["cdw"]),
            ("Side-chain", SIDECHAIN),
        ],
        "bass_enhancer" => &[(
            "Controls",
            &[
                "amount",
                "drive",
                "blend",
                "freq",
                "floor_active",
                "floor",
                "listen",
            ],
        )],
        "reverb" => &[
            (
                "Room",
                &["room_size", "decay_time", "predelay", "diffusion"],
            ),
            ("Tone", &["hf_damp", "bass_cut", "treble_cut"]),
            ("Mix", &["amount", "dry"]),
        ],
        "delay" => &[
            (
                "Left",
                &[
                    "mode_l", "time_l", "samp_l", "m_l", "cm_l", "t_l", "dwmix_l", "phase_l",
                ],
            ),
            (
                "Right",
                &[
                    "mode_r", "time_r", "samp_r", "m_r", "cm_r", "t_r", "dwmix_r", "phase_r",
                ],
            ),
        ],
        "filter" => &[
            ("Filter", &["ft", "fm", "s", "f", "w", "q", "g"]),
            ("Mode", &["mode", "decramp"]),
        ],
        _ => return None,
    })
}

/// A control that switches the plugin itself on and off: the stack's own
/// switch does that, so the page does not repeat it.
fn is_bypass(control: &Control) -> bool {
    matches!(control.symbol.as_str(), "enabled" | "bypass" | "on")
}

impl Component for PageView {
    fn render(&self) -> impl IntoElement {
        let (fx, data, runtime) = (self.fx, &self.data, &self.runtime);
        let tokens = use_tokens();
        let Some(slot) = self.selected.as_deref().and_then(|id| data.preset.slot(id)) else {
            return rect()
                .tw(&tokens, &[w_full(), p(S4)])
                .child(Group::new(None::<String>).row(status(Status::Empty(
                    "Add an effect to the stack.".to_string(),
                ))));
        };
        let Some(effect) = effects::find(&slot.kind) else {
            return rect()
                .tw(&tokens, &[w_full(), p(S4)])
                .child(Group::new(None::<String>).row(status(Status::Empty(
                    "This effect is not one Bingux offers.".to_string(),
                ))));
        };
        let catalog = fx.catalog.read().clone();
        let info = catalog
            .as_ref()
            .and_then(|catalog| catalog.plugin_of(effect))
            .cloned();
        let reset_id = slot.id.clone();

        let mut column = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
            .child(
                hstack(S2)
                    .tw_layout(&[w_full(), items(Align::Center), justify(Align::Between)])
                    .child(crate::ui::heading(effect.name))
                    .child(action_button("Reset", move |_| reset(fx, &reset_id)).compact()),
            );
        if let Some(reason) = runtime.failures.get(&slot.id) {
            column =
                column.child(Group::new(None::<String>).row(status(Status::Error(reason.clone()))));
        }
        match info {
            None if catalog.is_none() => {
                column = column.child(
                    Group::new(None::<String>)
                        .row(status(Status::Loading("Reading the plugin.".to_string()))),
                );
            }
            None => {
                column =
                    column.child(Group::new(None::<String>).row(status(Status::Empty(format!(
                        "{} is not installed. Install {} to use it.",
                        effect.name,
                        effect.package.fedora()
                    )))));
            }
            Some(info) => {
                if matches!(effect.layout, "equalizer") {
                    column = column.child(eq_page::EqPage {
                        fx,
                        slot: slot.clone(),
                        info: info.clone(),
                        mid_side: effect.key == "midside_equalizer",
                    });
                } else {
                    for card in cards(fx, slot, effect, &info) {
                        column = column.child(card);
                    }
                }
            }
        }
        column.child(caption(format!("Using {}", effect.package.project())))
    }
}

/// A gain control in decibels: the plugin stores a linear gain, the page shows
/// and moves decibels from -24 to +24.
fn gain_row(fx: Fx, slot: &PluginSlot, control: &Control, title: &str) -> Element {
    let value = slot.value(&control.symbol, control.default);
    let db = control.unit.shown(value).clamp(-24.0, 24.0);
    let (id, symbol, min, max) = (
        slot.id.clone(),
        control.symbol.clone(),
        control.min,
        control.max,
    );
    level_row(title, (db + 24.0) / 48.0 * 100.0, move |percent: f64| {
        let db = (percent / 100.0 * 48.0 - 24.0 * 1.0).round() / 1.0;
        set_control(fx, &id, &symbol, Unit::Gain.stored(db).clamp(min, max));
    })
    .value_text(format!("{} dB", (db * 10.0).round() / 10.0))
    .into()
}

fn cards(fx: Fx, slot: &PluginSlot, effect: &Effect, info: &PluginInfo) -> Vec<Element> {
    let mut cards: Vec<Element> = Vec::new();
    let mut gains = Vec::new();
    for (symbol, title) in [
        (effect.input_gain, "Input gain"),
        (effect.output_gain, "Output gain"),
    ] {
        if let Some(control) = symbol.and_then(|symbol| info.control(symbol)) {
            gains.push(gain_row(fx, slot, control, title));
        }
    }
    if !gains.is_empty() {
        cards.push(Group::titled("Gain").rows(gains).into());
    }
    if effect.layout == "convolver" {
        cards.push(
            Group::titled("Impulse response")
                .row(convolver_row(fx, slot))
                .into(),
        );
    }
    let gain_symbols = [effect.input_gain, effect.output_gain];
    let wanted = |control: &&Control| {
        !control.hidden
            && !is_bypass(control)
            && !gain_symbols.contains(&Some(control.symbol.as_str()))
    };
    if let Some(spec) = curated(effect.key) {
        for (title, symbols) in spec {
            let rows: Vec<Element> = symbols
                .iter()
                .filter_map(|symbol| info.control(symbol))
                .filter(wanted)
                .map(|control| {
                    control_row(
                        fx,
                        &slot.id,
                        control,
                        slot.value(&control.symbol, control.default),
                    )
                })
                .collect();
            if !rows.is_empty() {
                cards.push(Group::titled(*title).rows(rows).into());
            }
        }
        return cards;
    }
    // Generated: controls with a trailing index (`cr_3`) are bands, one card each;
    // the rest are grouped by what they measure.
    let mut titles: Vec<String> = Vec::new();
    let mut groups: Vec<Vec<Element>> = Vec::new();
    for control in info.controls.iter().filter(wanted) {
        let title = match control
            .symbol
            .rsplit_once('_')
            .and_then(|(_, tail)| tail.parse::<usize>().ok())
        {
            Some(index) => format!("Band {}", index + 1),
            None => generic_title(control).to_string(),
        };
        let row = control_row(
            fx,
            &slot.id,
            control,
            slot.value(&control.symbol, control.default),
        );
        match titles.iter().position(|existing| *existing == title) {
            Some(at) => groups[at].push(row),
            None => {
                titles.push(title);
                groups.push(vec![row]);
            }
        }
    }
    for (title, rows) in titles.into_iter().zip(groups) {
        cards.push(Group::titled(&title).rows(rows).into());
    }
    cards
}

fn generic_title(control: &Control) -> &'static str {
    if control.toggle {
        return "Options";
    }
    if !control.choices.is_empty() {
        return "Modes";
    }
    match control.unit {
        Unit::Db | Unit::Gain => "Level",
        Unit::Ms | Unit::Seconds => "Timing",
        Unit::Hz => "Frequency",
        Unit::Percent => "Mix",
        _ => "Settings",
    }
}

/// One control as a row: a switch, a menu of choices, or a slider with its value.
pub(super) fn control_row(fx: Fx, slot_id: &str, control: &Control, value: f64) -> Element {
    let id = slot_id.to_string();
    let symbol = control.symbol.clone();
    if control.toggle {
        return switch_row(control.name.clone(), value >= 0.5, move |on: bool| {
            set_control(fx, &id, &symbol, f64::from(u8::from(on)))
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
                set_control(fx, &id, &symbol, picked.parse().unwrap_or(0.0));
            }))
            .into();
    }
    let range = control.clone();
    level_row(
        control.name.clone(),
        control.position_of(value) * 100.0,
        move |position: f64| {
            set_control(fx, &id, &range.symbol, range.value_at(position / 100.0));
        },
    )
    .value_text(control.display(value))
    .into()
}

/// Set one control: shown at once, written and sent to the engine soon.
pub(super) fn set_control(fx: Fx, slot_id: &str, symbol: &str, value: f64) {
    let (id, symbol) = (slot_id.to_string(), symbol.to_string());
    fx.edit(move |preset| {
        if let Some(slot) = preset.slot_mut(&id) {
            slot.controls.insert(symbol, value);
        }
    });
}

/// Put every control of the effect back to what a new one starts with.
fn reset(fx: Fx, id: &str) {
    let id = id.to_string();
    fx.edit(move |preset| {
        if let Some(slot) = preset.slot_mut(&id) {
            slot.controls.clear();
            if let Some(effect) = effects::find(&slot.kind) {
                for (symbol, value) in effect.initial_controls() {
                    slot.controls.insert(symbol, value);
                }
            }
        }
    });
}

fn convolver_row(fx: Fx, slot: &PluginSlot) -> Element {
    let id = slot.id.clone();
    let name = slot
        .path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .map_or_else(
            || "None".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
    row("File")
        .value(name)
        .trailing(action_button("Choose…", move |_| {
            let id = id.clone();
            choose(
                fx.window,
                ChooserRequest::open()
                    .title("Choose an impulse response")
                    .filter_spec("Audio files", "*.wav *.flac *.ogg *.aiff"),
                move |answer| {
                    if let Some(path) = answer.and_then(|answer| answer.paths.into_iter().next()) {
                        let path = path.to_string_lossy().into_owned();
                        fx.edit(move |preset| {
                            if let Some(slot) = preset.slot_mut(&id) {
                                slot.path = Some(path);
                            }
                        });
                    }
                },
            );
        }))
        .into()
}

/// A row with a slider and its value on the row's line: the title at the left,
/// the track and the readout at the right, at one width in every card.
pub(super) struct SliderLine {
    title: String,
    value: f64,
    on_moved: EventHandler<f64>,
    text: String,
}

/// A slider row over 0 to 100 percent.
pub(super) fn level_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> SliderLine {
    SliderLine {
        title: title.into(),
        value,
        on_moved: on_moved.into(),
        text: String::new(),
    }
}

impl SliderLine {
    pub fn value_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
}

impl From<SliderLine> for Element {
    fn from(line: SliderLine) -> Element {
        let on_moved = line.on_moved;
        row(line.title.clone())
            .trailing(
                hstack(S3)
                    .tw_layout(&[items(Align::Center)])
                    .child(
                        slider(line.value, move |value: f64| on_moved.call(value))
                            .width(Dim::Field)
                            .label(line.title),
                    )
                    .child(
                        rect()
                            .tw_layout(&[w(Dim::SliderValue), flex_row(), justify(Align::End)])
                            .child(caption(line.text)),
                    ),
            )
            .into()
    }
}
