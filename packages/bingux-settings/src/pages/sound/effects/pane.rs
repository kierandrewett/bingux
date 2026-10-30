//! The pane of the selected effect: input and output level meters, then the
//! effect's controls as titled cards in a grid, and a footer that names where
//! the plugin comes from and resets it.
//!
//! The controls of a plugin come from its own port metadata (range, default,
//! logarithmic, integer, toggled), so every plugin has them with no hand-written
//! description. They are grouped into cards by what they measure: Level (dB and
//! ratios), Timing (ms), Frequency (Hz), Mix (%) and Settings (the rest).
//! The equalizer has its own cards (`equalizer.rs`).

use bingux_audio_effects::catalog::{Control, PluginInfo, Unit};
use bingux_audio_effects::graph::{self, Live};
use bingux_audio_effects::preset::{CONVOLVER, Direction, EQUALIZER, PluginSlot};
use freya::prelude::*;

use bingux_ui::style::*;

use super::super::meter::{LevelMeter, ReductionMeter, Source};
use super::{Effect, Fx, Loaded, Runtime, equalizer};
use crate::ui::files::{ChooserRequest, choose};
use bingux_ui::theme::Tokens;
use crate::ui::{Group, Status, action_button, caption, hstack, level_row, row, status, switch_row, use_tokens};

/// A titled card of rows.
pub(super) struct Card {
    pub title: String,
    pub rows: Vec<Element>,
    /// Spans the whole grid, as the equalizer curve does.
    pub wide: bool,
}

impl Card {
    pub fn new(title: impl Into<String>, rows: Vec<Element>) -> Self {
        Self {
            title: title.into(),
            rows,
            wide: false,
        }
    }

    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    fn element(self) -> Element {
        Group::titled(&self.title).rows(self.rows).into()
    }
}

/// Cards in two columns when the pane is wide enough, else one.
#[derive(PartialEq)]
struct Grid {
    wide: Vec<(usize, Element)>,
    cards: Vec<Element>,
}

/// The narrowest pane, in px, that lays cards out in two columns.
const TWO_COLUMNS: f32 = 560.0;

impl Component for Grid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| 0.0_f32);
        let columns = if *width.read() >= TWO_COLUMNS { 2 } else { 1 };
        let mut layout = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
            .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()));
        // Wide cards come first at their place; the rest are dealt into columns
        // in order, so a card keeps its column when others change.
        let mut pending: Vec<Element> = Vec::new();
        let flush = |layout: Rect, pending: &mut Vec<Element>, columns: usize| -> Rect {
            if pending.is_empty() {
                return layout;
            }
            let mut sides: Vec<Vec<Element>> = vec![Vec::new(); columns];
            for (index, card) in pending.drain(..).enumerate() {
                sides[index % columns].push(card);
            }
            let tokens = tokens.clone();
            let mut line = rect().tw(&tokens, &[flex_row(), grow_children(), w_full(), gap(S4), items(Align::Start)]);
            for side in sides {
                line = line.child(
                    rect()
                        .width(Size::flex(1.0))
                        .tw(&tokens, &[flex_col(), gap(S4)])
                        .children(side),
                );
            }
            layout.child(line)
        };
        let mut wide = self.wide.iter().peekable();
        for (index, card) in self.cards.iter().enumerate() {
            while let Some((at, element)) = wide.peek() {
                if *at != index {
                    break;
                }
                layout = flush(layout, &mut pending, columns);
                layout = layout.child(element.clone());
                wide.next();
            }
            pending.push(card.clone());
        }
        flush(layout, &mut pending, columns)
    }
}

/// The meters before and after the chain, as two cards.
fn levels(tokens: &Tokens, data: &Loaded, runtime: &Runtime) -> Element {
    let tokens = tokens.clone();
    let live = runtime.running && runtime.virtual_node.is_some();
    let (before, after) = match data.direction {
        Direction::Output => ((runtime.virtual_node, Source::Output), (runtime.device_node, Source::Output)),
        Direction::Input => ((runtime.device_node, Source::Input), (runtime.virtual_node, Source::Input)),
    };
    let meter = move |(node, source): (Option<u32>, Source)| -> Element {
        rect()
            .tw(&tokens, &[w_full(), px(GUTTER), py(S4)])
            .child(LevelMeter {
                node: node.unwrap_or(0),
                source,
                compact: false,
                live: live && node.is_some(),
            })
            .into()
    };
    hstack(S4)
        .tw_layout(&[flex_row(), grow_children(), w_full(), gap(S4)])
        .child(rect().width(Size::flex(1.0)).child(Group::titled("Input").row(meter(before))))
        .child(rect().width(Size::flex(1.0)).child(Group::titled("Output").row(meter(after))))
        .into()
}

/// The pane as a component of its own, so the page's hooks do not change
/// when the window changes between the desktop and the phone layout.
#[derive(PartialEq)]
pub(super) struct PaneView {
    pub fx: Fx,
    pub data: Loaded,
    pub runtime: Runtime,
    pub selected: Option<String>,
}

impl Component for PaneView {
    fn render(&self) -> impl IntoElement {
        pane(self.fx, &self.data, &self.runtime, self.selected.as_deref())
    }
}

/// The pane for effect `selected`, or a hint when the chain is empty.
pub(super) fn pane(fx: Fx, data: &Loaded, runtime: &Runtime, selected: Option<&str>) -> Element {
    let tokens = use_tokens();
    let Some(slot) = selected.and_then(|id| data.preset.slot(id)) else {
        return Group::new(None::<String>)
            .row(status(Status::Empty("Add an effect to the chain.".to_string())))
            .into();
    };
    let catalog = fx.catalog.read().clone();
    let info = catalog.as_ref().and_then(|catalog| catalog.find(&slot.kind)).cloned();

    let mut cards: Vec<Card> = Vec::new();
    match (&info, slot.kind.as_str()) {
        (_, EQUALIZER) => cards.extend(equalizer::cards(fx, slot, &tokens)),
        (Some(info), _) => {
            let number = catalog
                .as_ref()
                .and_then(|catalog| graph::meter_slots(&data.preset, catalog).into_iter().find(|(id, _)| *id == slot.id))
                .map(|(_, number)| number);
            cards.extend(generic_cards(fx, data, slot, info, number));
        }
        (None, _) => {
            return Group::new(None::<String>)
                .row(status(Status::Empty("This plugin is not installed.".to_string())))
                .into();
        }
    }
    let mut wide = Vec::new();
    let mut elements = Vec::new();
    for card in cards {
        if card.wide {
            wide.push((elements.len(), card.element()));
        } else {
            elements.push(card.element());
        }
    }

    let provider = provider(info.as_ref());
    let reset_id = slot.id.clone();
    let mut footer = hstack(S2)
        .tw_layout(&[w_full(), justify(Align::Between)])
        .child(caption(format!("Using {provider}")));
    let mut actions = hstack(S2);
    if !*fx.list_shown.read() && !is_compact() {
        actions = actions.child(
            action_button("Show effects", move |_| {
                let mut shown = fx.list_shown;
                shown.set(true);
            })
            .compact(),
        );
    }
    actions = actions.child(action_button("Reset", move |_| reset(fx, &reset_id)).compact());
    footer = footer.child(actions);

    rect()
        .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
        .child(levels(&tokens, data, runtime))
        .child(Grid { wide, cards: elements })
        .child(footer)
        .into()
}

/// Where a plugin comes from, for the footer.
fn provider(info: Option<&PluginInfo>) -> String {
    let Some(info) = info else {
        return "an unknown plugin".to_string();
    };
    match info.library_stem() {
        Some("bingux_dsp") => "Bingux DSP".to_string(),
        _ if info.maker == "PipeWire" => "PipeWire".to_string(),
        _ if info.maker.is_empty() => info.library_stem().unwrap_or("a LADSPA plugin").to_string(),
        _ => info.maker.clone(),
    }
}

/// Put every control of the plugin back to its default.
fn reset(fx: Fx, id: &str) {
    let id = id.to_string();
    fx.edit(
        move |preset| {
            if let Some(slot) = preset.slot_mut(&id) {
                slot.controls.clear();
                if slot.kind == EQUALIZER {
                    slot.preamp = Some(0.0);
                    for band in &mut slot.bands {
                        band.gain = 0.0;
                    }
                }
            }
        },
        Effect::Restart,
    );
}

/// The card a control belongs in.
fn card_of(control: &Control) -> &'static str {
    if control.toggle {
        return "Options";
    }
    match control.unit {
        Unit::Db | Unit::Ratio => "Level",
        Unit::Ms => "Timing",
        Unit::Hz => "Frequency",
        Unit::Percent => "Mix",
        Unit::None => "Settings",
    }
}

const CARD_ORDER: [&str; 6] = ["Level", "Timing", "Frequency", "Mix", "Settings", "Options"];

fn generic_cards(fx: Fx, data: &Loaded, slot: &PluginSlot, info: &PluginInfo, meter: Option<u32>) -> Vec<Card> {
    let mut cards: Vec<Card> = Vec::new();
    if let Some(number) = meter {
        let rows = info
            .meters
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let range = if info.label.contains("gate") { 60.0 } else { 24.0 };
                Element::from(row(name.clone()).trailing(
                    rect().tw_layout(&[w_full()]).child(ReductionMeter {
                        direction: data.direction,
                        slot: number,
                        index,
                        range_db: range,
                    }),
                ))
            })
            .collect();
        cards.push(Card::new("Gain reduction", rows).wide());
    }
    if slot.kind.ends_with(CONVOLVER) {
        cards.push(Card::new("Impulse response", vec![convolver_row(fx, slot)]));
    }
    for title in CARD_ORDER {
        let rows: Vec<Element> = info
            .visible_controls()
            .filter(|control| card_of(control) == title)
            .map(|control| {
                let value = slot.controls.get(&control.name).copied().unwrap_or(control.default);
                control_row(fx, &slot.id, control, value)
            })
            .collect();
        if !rows.is_empty() {
            cards.push(Card::new(title, rows));
        }
    }
    cards
}

fn control_row(fx: Fx, slot_id: &str, control: &Control, value: f64) -> Element {
    let id = slot_id.to_string();
    if control.toggle {
        let name = control.name.clone();
        return switch_row(control.name.clone(), value >= 0.5, move |on: bool| {
            set_control(fx, &id, &name, if on { 1.0 } else { 0.0 });
        })
        .into();
    }
    let range = control.clone();
    level_row(control.name.clone(), control.position_of(value) * 100.0, move |position: f64| {
        set_control(fx, &id, &range.name, range.value_at(position / 100.0));
    })
    .value_text(control.unit.format(value))
    .into()
}

/// Set one control: shown at once, saved soon, and sent to the running chain.
pub(super) fn set_control(fx: Fx, slot_id: &str, name: &str, value: f64) {
    let (id, control) = (slot_id.to_string(), name.to_string());
    let change = Live::Control {
        slot: slot_id.to_string(),
        name: name.to_string(),
        value,
    };
    fx.edit(
        move |preset| {
            if let Some(slot) = preset.slot_mut(&id) {
                slot.controls.insert(control, value);
            }
        },
        Effect::Live(vec![change]),
    );
}

fn convolver_row(fx: Fx, slot: &PluginSlot) -> Element {
    let id = slot.id.clone();
    let name = slot
        .path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .map_or_else(|| "None".to_string(), |name| name.to_string_lossy().into_owned());
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
                        fx.edit(
                            move |preset| {
                                if let Some(slot) = preset.slot_mut(&id) {
                                    slot.path = Some(path);
                                }
                            },
                            Effect::Restart,
                        );
                    }
                },
            );
        }))
        .into()
}
