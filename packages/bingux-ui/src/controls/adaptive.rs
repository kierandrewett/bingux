//! The adaptive line: a label and the control that goes with it, laid side by
//! side when they fit and stacked when they do not.
//!
//! Freya's flex gives a `w_flex()` child what its fixed-width siblings leave,
//! and never asks a fixed-width child to give some back. A label beside a
//! 240 px field in a 266 px row got 14 px, and a label with 14 px wraps a
//! letter to a line, taller than its row and over the next one. The rule
//! here is the one every row and form line of the library follows:
//!
//! 1. A label keeps its **floor**: its natural one-line width, or
//!    [`LABEL_SHARE`] of the line when that is less, and never less than its
//!    longest word. A label breaks between words only.
//! 2. The control gets the rest, at its natural width ([`LineMode::Beside`]).
//! 3. When the control's natural width would take the label's floor, a
//!    control that can shrink (a text field or dropdown, which fill what they
//!    are given) shrinks to what is left, at least [`Dim::FieldMin`]
//!    ([`LineMode::Shrunk`]); the label keeps its natural width.
//! 4. When even that does not leave the label its floor, the line stacks:
//!    the label above, the control under it at the full width
//!    ([`LineMode::Stacked`]).
//!
//! [`choose_line`] is the rule as a pure function of widths, so it is tested
//! without a window. [`AdaptiveLine`] measures the line and its control and
//! applies it; [`form_row`](crate::group::form_row) is one. A control that can
//! shrink calls `use_line_fill()` and fills its cell when it says so.
//! `FormField` uses [`choose_line`] too.
//!
//! A control that *fills* (a text field in a row on a phone) is not left at
//! its natural width when that would fit: [`choose_filled_line`] gives the
//! label its floor, at most [`LABEL_SHARE`] of the line, and the control
//! every pixel after it, at least [`Dim::FieldMin`]; when the label's floor
//! leaves less than that, the line stacks. [`Row`](crate::rows::Row) uses it
//! for a trailing text field on a phone, and tells the field to fill through
//! the same [`LineFit`] a form line gives it.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::path_label::text_width;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, TextStyle};

/// Room added to a measured label width so rounding never wraps its last glyph.
const LABEL_SLACK: f32 = 1.0;

/// The width of the cell a shrunk line gives its label of `label` px.
pub(crate) fn label_cell_width(label: f32) -> f32 {
    label.ceil() + LABEL_SLACK
}

/// The share of a line a label may be held to when its natural width is more.
/// A label wider than this wraps between words in that much.
pub const LABEL_SHARE: f32 = 0.4;

/// What a label needs, in px at its type style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelFacts {
    /// The width of the whole label on one line.
    pub natural: f32,
    /// The width of its longest word, the least it can be set in without
    /// breaking inside a word.
    pub longest_word: f32,
}

impl LabelFacts {
    /// Measures `text` in `style` with the engine that draws it, so the width
    /// agrees with the label, at the current text scale and locale.
    pub fn measure(text: &str, style: TextStyle) -> Self {
        let longest_word = text
            .split_whitespace()
            .map(|word| text_width(word, style))
            .fold(0.0, f32::max);
        Self {
            natural: text_width(text, style),
            longest_word,
        }
    }
}

/// What the control of a line needs, in px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlFacts {
    /// Its width when nothing limits it; 0 when it is not measured yet.
    pub natural: f32,
    /// The least it shrinks to, `None` for a control that cannot shrink.
    pub least: Option<f32>,
}

/// How a line is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineMode {
    /// Label and control in a row, the label taking what the control leaves.
    Beside,
    /// In a row, the control shrunk to fill what the label's width leaves.
    Shrunk {
        /// The label's width in px.
        label: f32,
    },
    /// The label above the control.
    Stacked,
}

/// Decides how a line of `inner` px (its content width), with `gap` px
/// between label and control, is laid out. See the [module](self).
pub fn choose_line(inner: f32, gap: f32, label: LabelFacts, control: ControlFacts) -> LineMode {
    // Not measured yet: the natural layout, which measures it.
    if inner <= 0.0 || control.natural <= 0.0 {
        return LineMode::Beside;
    }
    let floor = label.natural.min(LABEL_SHARE * inner);
    // A word that does not fit the floor would break inside itself.
    if label.longest_word > floor + 0.5 {
        return LineMode::Stacked;
    }
    if inner - gap - control.natural >= floor {
        return LineMode::Beside;
    }
    if let Some(least) = control.least
        && inner - gap - least >= floor
    {
        return LineMode::Shrunk {
            label: label.natural.min(inner - gap - least),
        };
    }
    LineMode::Stacked
}

/// Decides how a line of `inner` px, with `gap` px between label and control,
/// is laid out when its control fills what the label leaves, to at least
/// `least` px, whatever its natural width: the label keeps its floor (its
/// natural width, or [`LABEL_SHARE`] of the line when that is less, and never
/// less than its longest word) and the control takes the rest
/// ([`LineMode::Shrunk`], never [`LineMode::Beside`]). When the floor leaves
/// the control less than `least`, the line stacks. A line that has not been
/// measured yet is laid out as it will settle when its label fits its share:
/// the label at its natural width, the control filling the rest.
pub fn choose_filled_line(inner: f32, gap: f32, label: LabelFacts, least: f32) -> LineMode {
    if inner <= 0.0 {
        return LineMode::Shrunk {
            label: label.natural,
        };
    }
    let floor = label.natural.min(LABEL_SHARE * inner);
    if label.longest_word > floor + 0.5 {
        return LineMode::Stacked;
    }
    if inner - gap - floor >= least {
        LineMode::Shrunk { label: floor }
    } else {
        LineMode::Stacked
    }
}

/// What a read-only value needs, in px at its type style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueFacts {
    /// The width of the whole value on one line.
    pub natural: f32,
    /// The width of its longest word, `0` for a value that may break
    /// anywhere (a path, a hash, an address), which has no word to protect.
    pub longest_word: f32,
}

impl ValueFacts {
    /// Measures `text` in `style` with the engine that draws it. A value that
    /// `breaks_anywhere` has no longest word.
    pub fn measure(text: &str, style: TextStyle, breaks_anywhere: bool) -> Self {
        Self {
            // The room the cell is rounded up by, so a value that fits to the
            // pixel is not wrapped at its last word.
            natural: text_width(text, style) + LABEL_SLACK * 2.0,
            longest_word: if breaks_anywhere {
                0.0
            } else {
                text.split_whitespace()
                    .map(|word| text_width(word, style))
                    .fold(0.0, f32::max)
            },
        }
    }
}

/// How a row with a read-only value is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueLine {
    /// Title and value on one line: the value in one line at its natural
    /// width, the title in what it leaves (never less than its floor).
    Beside {
        /// The title's cell, in px.
        label: f32,
    },
    /// Title and value in a row, the value wrapped onto a few lines in what
    /// the title's floor leaves, aligned to the row's end.
    Wrapped {
        /// The title's cell, in px.
        label: f32,
    },
    /// The title above the value, which takes the full width and wraps.
    Stacked,
}

/// The most lines a value may wrap to beside its title before the row stacks.
pub const VALUE_LINES_BESIDE: f32 = 2.0;
/// The part of a line that a wrapped value's last line may not fill, so the
/// estimate of its lines errs towards stacking.
const WRAP_SLACK: f32 = 0.9;

/// Decides how a row of `inner` px (its content width), with `gap` px between
/// title and value, is laid out. A value is never cut with an ellipsis: it
/// fits beside its title on one line ([`ValueLine::Beside`]), or on a desktop
/// wraps onto up to [`VALUE_LINES_BESIDE`] lines beside it
/// ([`ValueLine::Wrapped`]), or the row stacks the value under its title at
/// the full width ([`ValueLine::Stacked`]). A compact (phone) row skips the
/// wrapped form: a value that does not fit on one line stacks. The title
/// keeps its floor in every form (see [`choose_line`]): at `inner <= 0` the
/// row has not been measured and is laid out as `Wrapped`, which is valid at
/// any width.
pub fn choose_value_line(
    inner: f32,
    gap: f32,
    label: LabelFacts,
    value: ValueFacts,
    compact: bool,
) -> ValueLine {
    if inner <= 0.0 {
        return ValueLine::Wrapped {
            label: label.natural,
        };
    }
    let floor = label.natural.min(LABEL_SHARE * inner);
    // A title word that does not fit the floor would break inside itself.
    if label.longest_word > floor + 0.5 {
        return ValueLine::Stacked;
    }
    let room = inner - gap - floor;
    if value.natural <= room {
        return ValueLine::Beside {
            label: label.natural.min(inner - gap - value.natural),
        };
    }
    if !compact && room > 0.0 && value.longest_word <= room {
        let lines = (value.natural / (room * WRAP_SLACK)).ceil();
        if lines <= VALUE_LINES_BESIDE {
            return ValueLine::Wrapped { label: floor };
        }
    }
    ValueLine::Stacked
}

/// What a line tells its control: whether to fill the cell it is given, and
/// the control's reply that it can.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct LineFit {
    fill: State<bool>,
    fluid: State<bool>,
}

impl LineFit {
    /// Gives the controls below the calling component a fit it decides:
    /// `fill` says whether they fill their cell. For a component that lays
    /// out its own line (a [`Row`](crate::rows::Row)) instead of being an
    /// [`AdaptiveLine`]. Every render takes the same hooks, whatever `fill`.
    pub(crate) fn provide(fill: bool) {
        let mut fit = use_provide_context(|| LineFit {
            fill: State::create(fill),
            fluid: State::create(false),
        });
        fit.fill.set_if_modified(fill);
    }
}

/// For a control that can shrink or grow to its cell (a text field, a
/// dropdown): whether the line around it asks it to fill its cell. Reading it
/// also tells the line that this control can.
pub(crate) fn use_line_fill() -> bool {
    let fit = use_try_consume::<LineFit>();
    use_hook(move || {
        if let Some(fit) = fit {
            let mut fluid = fit.fluid;
            fluid.set(true);
        }
    });
    fit.is_some_and(|fit| (fit.fill)())
}

/// A label and a control on one line, laid out by [`choose_line`]. The label
/// is a row's title in the body style; the control is anything.
#[derive(PartialEq)]
pub struct AdaptiveLine {
    component: &'static str,
    label: String,
    control: Element,
}

impl AdaptiveLine {
    pub fn new(component: &'static str, label: impl Into<String>, control: impl Into<Element>) -> Self {
        Self {
            component,
            label: label.into(),
            control: control.into(),
        }
    }
}

impl Component for AdaptiveLine {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| 0.0_f32);
        let mut natural = use_state(|| 0.0_f32);
        let mut fit = use_provide_context(|| LineFit {
            fill: State::create(false),
            fluid: State::create(false),
        });

        let facts = LabelFacts::measure(&self.label, BODY);
        let inner = (width() - 2.0 * GUTTER.px()).max(0.0);
        let control = ControlFacts {
            natural: natural(),
            least: (fit.fluid)().then(|| Dim::FieldMin.px()),
        };
        let mode = choose_line(inner, S3.px(), facts, control);
        fit.fill.set_if_modified(mode != LineMode::Beside);

        let label = BODY.label(&tokens, self.label.clone());
        let beside = mode == LineMode::Beside;
        let (label_cell, control_cell) = match mode {
            LineMode::Beside => (
                rect().tw(&tokens, &[w_flex()]).min_width(Size::px(0.0)),
                rect(),
            ),
            LineMode::Shrunk { label } => (
                rect().width(Size::px(label_cell_width(label))),
                rect().tw(&tokens, &[w_flex()]).min_width(Size::px(0.0)),
            ),
            LineMode::Stacked => (rect().tw(&tokens, &[w_full()]), rect().tw(&tokens, &[w_full()])),
        };
        let line = rect()
            .component(self.component)
            .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()));
        let line = if mode == LineMode::Stacked {
            line.tw(
                &tokens,
                &[flex_col(), w_full(), px(GUTTER), py(S3), gap(S2)],
            )
        } else {
            line.tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    min_h(Dim::Row),
                    px(GUTTER),
                    py(S2),
                    gap(S3),
                ],
            )
        };
        line.child(label_cell.child(label)).child(
            control_cell
                .on_sized(move |event: Event<SizedEventData>| {
                    // Only the control's own width counts: the width it is
                    // given in a shrunk or stacked line says nothing about it.
                    if beside {
                        natural.set_if_modified(event.area.width());
                    }
                })
                .child(self.control.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: f32 = 12.0;

    fn label(natural: f32, longest_word: f32) -> LabelFacts {
        LabelFacts {
            natural,
            longest_word,
        }
    }

    fn field() -> ControlFacts {
        ControlFacts {
            natural: 240.0,
            least: Some(120.0),
        }
    }

    fn switch() -> ControlFacts {
        ControlFacts {
            natural: 44.0,
            least: None,
        }
    }

    #[test]
    fn a_line_that_has_not_been_measured_is_laid_out_beside() {
        assert_eq!(choose_line(0.0, GAP, label(90.0, 60.0), field()), LineMode::Beside);
        let unknown = ControlFacts {
            natural: 0.0,
            least: Some(120.0),
        };
        assert_eq!(choose_line(266.0, GAP, label(90.0, 60.0), unknown), LineMode::Beside);
    }

    #[test]
    fn a_wide_popup_keeps_the_label_beside_a_full_size_field() {
        assert_eq!(choose_line(488.0, GAP, label(90.0, 60.0), field()), LineMode::Beside);
    }

    #[test]
    fn the_popup_of_the_screenshot_shrinks_the_field_and_keeps_the_label_whole() {
        // 'Network name' (90 px) and a 240 px field in a 266 px line: the
        // field used to take 240 and leave the label 14.
        let mode = choose_line(266.0, GAP, label(90.0, 60.0), field());
        assert_eq!(mode, LineMode::Shrunk { label: 90.0 });
        let LineMode::Shrunk { label } = mode else { unreachable!() };
        assert!(266.0 - GAP - label >= 120.0, "the field keeps its least");
    }

    #[test]
    fn a_label_that_leaves_the_field_less_than_its_least_stacks() {
        // 150 px: a 60 px label and 12 px gap leave a field 78 px, under 120.
        assert_eq!(choose_line(150.0, GAP, label(60.0, 40.0), field()), LineMode::Stacked);
        // The label may also wrap in its share and let the field shrink.
        assert_eq!(
            choose_line(266.0, GAP, label(160.0, 90.0), field()),
            LineMode::Shrunk { label: 134.0 }
        );
    }

    #[test]
    fn a_label_wider_than_its_share_wraps_between_words_when_each_word_fits() {
        // 'Connect automatically' at 155 px beside a switch in 266 px: the
        // label may take 40 % (106 px) and wraps into two lines of words of
        // 55 and 95 px.
        assert_eq!(
            choose_line(266.0, GAP, label(155.0, 95.0), switch()),
            LineMode::Beside
        );
    }

    #[test]
    fn a_word_wider_than_the_labels_share_stacks_the_line() {
        // At 200 % text the same label has a 190 px word: 40 % of the line
        // cannot hold it, and the label would break inside it.
        assert_eq!(
            choose_line(266.0, GAP, label(310.0, 190.0), switch()),
            LineMode::Stacked
        );
    }

    #[test]
    fn a_control_that_cannot_shrink_stacks_rather_than_squeezing_the_label() {
        let stepper = ControlFacts {
            natural: 160.0,
            least: None,
        };
        assert_eq!(choose_line(200.0, GAP, label(120.0, 60.0), stepper), LineMode::Stacked);
    }

    #[test]
    fn the_label_never_gets_less_than_its_floor_at_any_width() {
        for inner in 80..700 {
            let inner = inner as f32;
            for (label, control) in [
                (label(90.0, 60.0), field()),
                (label(155.0, 95.0), switch()),
                (label(310.0, 190.0), field()),
            ] {
                let floor = label.natural.min(LABEL_SHARE * inner);
                match choose_line(inner, GAP, label, control) {
                    LineMode::Beside => {
                        assert!(inner - GAP - control.natural >= floor, "{inner}")
                    }
                    LineMode::Shrunk { label: taken } => {
                        assert!(taken >= floor, "{inner}");
                        assert!(inner - GAP - taken >= control.least.unwrap(), "{inner}");
                    }
                    LineMode::Stacked => {}
                }
            }
        }
    }

    /// The gap a row puts between its title and its trailing control (`S3`
    /// and the trailing area's `S1`).
    const ROW_GAP: f32 = 16.0;
    const FIELD_MIN: f32 = 120.0;

    #[test]
    fn a_short_title_keeps_its_width_and_the_field_takes_every_other_pixel() {
        // 'Try it' (36 px) in a row of a 390 px phone (322 px of content) and
        // of a 320 px one (252): the field is not left at its 168 px.
        for (inner, field) in [(322.0, 270.0), (252.0, 200.0)] {
            let mode = choose_filled_line(inner, ROW_GAP, label(36.0, 36.0), FIELD_MIN);
            assert_eq!(mode, LineMode::Shrunk { label: 36.0 }, "{inner}");
            assert_eq!(inner - ROW_GAP - 36.0, field, "{inner}");
        }
    }

    #[test]
    fn a_title_wider_than_its_share_is_held_to_the_share() {
        // 200 px of title in 322: the title takes 40 % and wraps between
        // words, the field gets the 60 % that is left.
        let mode = choose_filled_line(322.0, ROW_GAP, label(200.0, 70.0), FIELD_MIN);
        let LineMode::Shrunk { label } = mode else {
            panic!("{mode:?}")
        };
        assert!((label - 0.4 * 322.0).abs() < 0.01, "{label}");
        assert!(322.0 - ROW_GAP - label >= FIELD_MIN);
    }

    #[test]
    fn a_word_wider_than_the_titles_share_stacks_the_line() {
        assert_eq!(
            choose_filled_line(252.0, ROW_GAP, label(190.0, 150.0), FIELD_MIN),
            LineMode::Stacked
        );
    }

    #[test]
    fn a_line_too_narrow_to_leave_the_field_its_least_stacks() {
        // 150 px of content: a 60 px title and the gap leave 74 px.
        assert_eq!(
            choose_filled_line(150.0, ROW_GAP, label(60.0, 40.0), FIELD_MIN),
            LineMode::Stacked
        );
    }

    #[test]
    fn a_line_that_has_not_been_measured_is_laid_out_as_it_settles() {
        assert_eq!(
            choose_filled_line(0.0, ROW_GAP, label(36.0, 36.0), FIELD_MIN),
            LineMode::Shrunk { label: 36.0 }
        );
    }

    #[test]
    fn a_filling_control_is_never_beside_and_never_squeezes_the_title_or_the_field() {
        for inner in 80..700 {
            let inner = inner as f32;
            for label in [label(36.0, 36.0), label(155.0, 95.0), label(310.0, 190.0)] {
                let floor = label.natural.min(LABEL_SHARE * inner);
                match choose_filled_line(inner, ROW_GAP, label, FIELD_MIN) {
                    LineMode::Beside => panic!("beside at {inner}"),
                    LineMode::Shrunk { label: taken } => {
                        assert!(taken >= floor, "{inner}");
                        assert!(inner - ROW_GAP - taken >= FIELD_MIN, "{inner}");
                    }
                    LineMode::Stacked => {}
                }
            }
        }
    }

    fn value(natural: f32, longest_word: f32) -> ValueFacts {
        ValueFacts { natural, longest_word }
    }

    #[test]
    fn a_short_value_sits_beside_its_title_whatever_the_width() {
        // 'Licence' and 'GPL-3.0-or-later' (110 px) in a phone row of 322 px.
        assert_eq!(
            choose_value_line(322.0, ROW_GAP, label(60.0, 60.0), value(110.0, 110.0), true),
            ValueLine::Beside { label: 60.0 }
        );
    }

    #[test]
    fn the_licence_expression_of_the_screenshot_stacks_on_a_phone_and_wraps_on_a_desktop() {
        // 'Apache-2.0 AND MIT AND Zlib AND BSD-3-Clause' is 330 px. On a
        // phone the row has 322 px of content and 'Licences' (70 px)
        // leaves 236: it does not fit, so it stacks. A 560 px desktop row
        // leaves 560 - 16 - 70 = 474, so it fits beside.
        let licences = value(330.0, 70.0);
        assert_eq!(
            choose_value_line(322.0, ROW_GAP, label(70.0, 70.0), licences, true),
            ValueLine::Stacked
        );
        assert!(matches!(
            choose_value_line(560.0, ROW_GAP, label(70.0, 70.0), licences, false),
            ValueLine::Beside { .. }
        ));
        // A 340 px desktop row leaves 254 px: two lines.
        assert!(matches!(
            choose_value_line(340.0, ROW_GAP, label(70.0, 70.0), licences, false),
            ValueLine::Wrapped { .. }
        ));
    }

    #[test]
    fn a_value_that_needs_more_than_two_lines_beside_its_title_stacks() {
        // A 64-character hash, 520 px, may break anywhere, in a 340 px row.
        assert_eq!(
            choose_value_line(340.0, ROW_GAP, label(70.0, 70.0), value(520.0, 0.0), false),
            ValueLine::Stacked
        );
    }

    #[test]
    fn a_word_wider_than_the_room_beside_the_title_stacks_instead_of_breaking() {
        assert_eq!(
            choose_value_line(340.0, ROW_GAP, label(70.0, 70.0), value(300.0, 280.0), false),
            ValueLine::Stacked
        );
    }

    #[test]
    fn a_row_that_has_not_been_measured_wraps_the_value_and_keeps_the_title_whole() {
        assert_eq!(
            choose_value_line(0.0, ROW_GAP, label(70.0, 70.0), value(900.0, 0.0), true),
            ValueLine::Wrapped { label: 70.0 }
        );
    }

    #[test]
    fn the_title_never_gets_less_than_its_floor_and_a_value_is_never_cut_at_any_width() {
        for inner in 80..900 {
            let inner = inner as f32;
            for compact in [false, true] {
                for (label, value) in [
                    (label(70.0, 70.0), value(110.0, 110.0)),
                    (label(155.0, 95.0), value(330.0, 70.0)),
                    (label(70.0, 70.0), value(520.0, 0.0)),
                    (label(310.0, 190.0), value(90.0, 90.0)),
                ] {
                    let floor = label.natural.min(LABEL_SHARE * inner);
                    match choose_value_line(inner, ROW_GAP, label, value, compact) {
                        ValueLine::Beside { label: taken } => {
                            assert!(taken >= floor - 0.01, "{inner}");
                            assert!(taken + ROW_GAP + value.natural <= inner + 0.01, "{inner}: it fits on one line");
                        }
                        ValueLine::Wrapped { label: taken } => {
                            assert!(!compact, "{inner}: a phone row never wraps beside");
                            assert!(taken >= floor - 0.01, "{inner}");
                            assert!(inner - ROW_GAP - taken >= value.longest_word, "{inner}");
                        }
                        ValueLine::Stacked => {}
                    }
                }
            }
        }
    }
}
