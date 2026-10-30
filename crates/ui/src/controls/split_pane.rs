//! Split pane: two panes with a draggable divider between them. The ratio is
//! the share of the length the first pane takes; the caller owns it, so the
//! position survives a rebuild. By default the panes sit side by side;
//! `.vertical()` stacks them.
//!
//! Dragging the divider sets the ratio from the pointer. The ratio is kept so
//! that each pane stays at least `MIN_PANE_PX` long; when the container is too
//! short for that, the panes split it evenly. The divider is a 6px hit zone
//! with a hairline centred in it, and a keyboard focus stop: the arrow keys
//! across its axis move it by `KEY_STEP`.

use freya::prelude::*;

use super::direction::use_direction;
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;

/// Smallest share either pane may take before the length is known.
const MIN_RATIO: f32 = 0.15;
/// Largest share either pane may take before the length is known.
const MAX_RATIO: f32 = 0.85;
/// Share of the length one arrow key press moves the divider.
const KEY_STEP: f32 = 0.05;
/// Narrowest either pane may get, in logical pixels, so neither side collapses.
const MIN_PANE_PX: f32 = 220.0;
/// Width of the divider's hit zone in logical pixels; the hairline sits centred in it.
const HIT_PX: f32 = 6.0;

/// The share range that keeps both panes at least `MIN_PANE_PX` long along a
/// container of `length`. Before the length is known, the fallback range applies.
fn share_bounds(length: f32) -> (f32, f32) {
    if length <= 0.0 {
        return (MIN_RATIO, MAX_RATIO);
    }
    let floor = (MIN_PANE_PX / length).clamp(MIN_RATIO, 0.5);
    (floor, 1.0 - floor)
}

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
        let direction = use_direction();
        let mut area = use_state(Area::default);
        let mut dragging = use_state(|| false);
        let mut hovering = use_state(|| false);
        let mut ratio = self.ratio.clone();
        let vertical = self.vertical;
        let bounds = *area.read();
        let length = if vertical {
            bounds.height()
        } else {
            bounds.width()
        };
        let (low, high) = share_bounds(length);
        let value = ratio.read().clamp(low, high);

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
            let (low, high) = share_bounds(length);
            // The first pane starts at the start edge: the right one when the panes read
            // from the right, so the pointer is measured from there.
            let along = if direction.is_rtl() && !vertical {
                origin + length - pointer as f32
            } else {
                pointer as f32 - origin
            };
            let share = (along / length).clamp(low, high);
            ratio.set_if_modified(share);
        };

        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut keyed = self.ratio.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            // Along the reading direction: the first pane grows towards the end, which is
            // Left when the panes read from the right.
            let delta = match (&event.key, vertical) {
                (Key::Named(NamedKey::ArrowUp), true) => -KEY_STEP,
                (Key::Named(NamedKey::ArrowDown), true) => KEY_STEP,
                (Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight), false) => {
                    direction.along(&event.key).unwrap_or(0) as f32 * KEY_STEP
                }
                _ => return,
            };
            event.stop_propagation();
            keyed.set_if_modified((value + delta).clamp(low, high));
        };
        // The divider is the pane's whole length, so its ring is drawn inside it: outside, a
        // clipped container (the card the panes sit in) cuts the ring off at both ends. It
        // lights under the pointer and while it is held.
        let lit = surface(Paint::Transparent, hovering(), dragging(), false);
        let hit = if vertical {
            rect()
                .width(Size::fill())
                .height(Size::px(HIT_PX))
                .tw(&tokens, &[items(Align::Center), justify(Align::Center), lit])
                .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
        } else {
            rect()
                .width(Size::px(HIT_PX))
                .height(Size::fill())
                .tw(&tokens, &[items(Align::Center), justify(Align::Center), lit])
                .child(rect().tw(&tokens, &[h_full(), w(Dim::Hairline), bg(Paint::Separator)]))
        };
        let divider = hit
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Splitter)
            .a11y_alt("Pane divider")
            .on_key_down(on_key_down)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                dragging.set(true);
            })
            .focusable_inside(focus());

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
            .component("SplitPane")
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    if vertical { flex_col() } else { flex_row() },
                    grow_children(),
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
