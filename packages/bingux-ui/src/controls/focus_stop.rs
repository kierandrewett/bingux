//! Keyboard stops for boxes an app builds itself.
//!
//! A pressable box that is not one of the library's controls, such as a
//! colour swatch or a profile card, becomes a keyboard stop through
//! [`focus_stop`]. It gets the same stop, ring, press and activation keys as
//! a library control, so apps never opt in to focus themselves.

use freya::prelude::*;

use super::press::use_press;
use crate::focus::{press_focus, use_focus_core};
use crate::motion::Focusable;
use crate::style::Radius;
use crate::surface::{is_activation_key, use_enabled};

/// Makes `content` one Tab stop with the library's focus ring.
///
/// `content` is the box the stop lives on. It is focusable when `enabled`
/// and is labelled `label` for assistive technology. A press or an
/// activation key (Enter, Space) runs `on_activate` when enabled. A press
/// focuses the box without a ring. The returned wrapper is what the caller
/// places in the tree; it draws the ring for `radius`.
pub fn focus_stop(
    content: Rect,
    label: impl Into<String>,
    role: AccessibilityRole,
    enabled: bool,
    radius: Radius,
    on_activate: impl Into<EventHandler<()>>,
) -> Rect {
    let a11y_id = use_a11y();
    let core = use_focus_core();
    let focus = use_focus(a11y_id);
    let press = use_press();
    let usable = use_enabled(enabled);
    let on_activate: EventHandler<()> = on_activate.into();
    let on_key = on_activate.clone();
    let control = content
        .a11y_id(a11y_id)
        .a11y_role(role)
        .a11y_alt(label.into())
        .a11y_enabled(usable)
        .a11y_focusable(usable)
        .maybe(usable, |element| {
            element
                .on_pointer_down(move |event: Event<PointerEventData>| {
                    event.stop_propagation();
                    press.begin(&event, None);
                })
                .on_global_pointer_move(move |event: Event<PointerEventData>| {
                    press.moved(&event);
                })
                .on_global_pointer_up(move |_| press.release())
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    if !press.tap(&event) {
                        return;
                    }
                    on_activate.call(());
                    press_focus(&core, a11y_id);
                })
                .on_key_down(move |event: Event<KeyboardEventData>| {
                    if is_activation_key(&event.key) {
                        event.stop_propagation();
                        on_key.call(());
                    }
                })
        });
    control.focusable(focus(), radius)
}
