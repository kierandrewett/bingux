//! Split pane: two panes with a draggable divider between them. The ratio is
//! the share of the length the first pane takes; the caller owns it, so the
//! position survives a rebuild. By default the panes sit side by side;
//! `.vertical()` stacks them.
//!
//! Dragging the divider sets the ratio from the pointer, kept between
//! `MIN_RATIO` and `MAX_RATIO` so neither pane can close. The divider is a
//! keyboard focus stop: the arrow keys across its axis move it by `KEY_STEP`.

use freya::prelude::*;

use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;

/// Smallest share either pane may take.
const MIN_RATIO: f32 = 0.15;
/// Largest share either pane may take.
const MAX_RATIO: f32 = 0.85;
/// Share of the length one arrow key press moves the divider.
const KEY_STEP: f32 = 0.05;

/// Two panes and the divider between them. See the module docs.
#[derive(Clone, PartialEq)]
pub struct SplitPane {
    first: Element,
    second: Element,
    ratio: State<f32>,
    vertical: bool,
}

/// Two panes side by side, the first taking `ratio` of the width.
pub fn split_pane(
    first: impl Into<Element>,
    second: impl Into<Element>,
    ratio: State<f32>,
) -> SplitPane {
    SplitPane {
        first: first.into(),
        second: second.into(),
        ratio,
        vertical: false,
    }
}

impl SplitPane {
    /// Stacks the panes: the first takes `ratio` of the height.
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }
}

impl Component for SplitPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut area = use_state(Area::default);
        let mut dragging = use_state(|| false);
        let mut ratio = self.ratio.clone();
        let vertical = self.vertical;
        let value = ratio.read().clamp(MIN_RATIO, MAX_RATIO);

        let on_move = move |event: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            event.prevent_default();
            let bounds = *area.peek();
            let (origin, length, pointer) = if vertical {
                (bounds.min_y(), bounds.height(), event.global_location().y)
            } else {
                (bounds.min_x(), bounds.width(), event.global_location().x)
            };
            if length <= 0.0 {
                return;
            }
            let share = ((pointer as f32 - origin) / length).clamp(MIN_RATIO, MAX_RATIO);
            ratio.set_if_modified(share);
        };

        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut keyed = self.ratio.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let delta = match (&event.key, vertical) {
                (Key::Named(NamedKey::ArrowLeft), false)
                | (Key::Named(NamedKey::ArrowUp), true) => -KEY_STEP,
                (Key::Named(NamedKey::ArrowRight), false)
                | (Key::Named(NamedKey::ArrowDown), true) => KEY_STEP,
                _ => return,
            };
            event.stop_propagation();
            keyed.set_if_modified((value + delta).clamp(MIN_RATIO, MAX_RATIO));
        };
        let divider = if vertical {
            rect()
                .tw(&tokens, &[w_full(), py(S1)])
                .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
        } else {
            rect()
                .tw(&tokens, &[h_full(), px(S1)])
                .child(rect().tw(&tokens, &[h_full(), w(Dim::Hairline), bg(Paint::Separator)]))
        }
        .a11y_id(a11y_id)
        .a11y_focusable(true)
        .a11y_role(AccessibilityRole::Splitter)
        .a11y_alt("Pane divider")
        .on_key_down(on_key_down)
        .on_pointer_down(move |event: Event<PointerEventData>| {
            if !event.data().is_primary() {
                return;
            }
            event.stop_propagation();
            dragging.set(true);
        })
        .focusable(focus(), Radius::Pill);

        // The first pane is a run-time share of the length, so a raw percent;
        // the second takes what the divider leaves.
        let first = rect()
            .maybe(vertical, |element| {
                element
                    .height(Size::percent(value * 100.0))
                    .width(Size::fill())
            })
            .maybe(!vertical, |element| {
                element
                    .width(Size::percent(value * 100.0))
                    .height(Size::fill())
            })
            .child(self.first.clone());
        let second = rect()
            .maybe(vertical, |element| {
                element.height(Size::flex(1.0)).width(Size::fill())
            })
            .maybe(!vertical, |element| {
                element.width(Size::flex(1.0)).height(Size::fill())
            })
            .child(self.second.clone());

        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    if vertical { flex_col() } else { flex_row() },
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| area.set(event.area))
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(move |_| dragging.set_if_modified(false))
            .child(first)
            .child(divider)
            .child(second)
    }
}
