//! Keyboard stops for boxes an app builds itself.
//!
//! A pressable box that is not one of the library's controls, such as a
//! colour swatch or a profile card, becomes a keyboard stop through
//! [`focus_stop`]. It gets the same stop, ring, press and activation keys as
//! a library control, so apps never opt in to focus themselves.
//!
//! [`focus_stop_menu`] adds a context menu to the same box: a right click, a
//! touch that rests, and the Menu key or Shift+F10 open it.

use freya::prelude::*;

use super::press::{point_of, use_press};
use crate::focus::{press_focus, use_focus_core};
use crate::motion::Focusable;
use crate::shell::menu::MenuItem;
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::style::{Radius, Tw, rounded};
use crate::surface::{is_activation_key, use_enabled, use_tokens};

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
    focus_stop_menu(
        content,
        label,
        role,
        enabled,
        radius,
        on_activate,
        Vec::new(),
    )
}

/// [`focus_stop`] that sets `pressing` while a button is down on the box, for
/// the fill's pressed layer: the stop owns the box's pointer handlers, so a
/// caller cannot set its own on `content` (ST-06).
pub fn focus_stop_press(
    content: Rect,
    label: impl Into<String>,
    role: AccessibilityRole,
    enabled: bool,
    radius: Radius,
    on_activate: impl Into<EventHandler<()>>,
    pressing: State<bool>,
) -> Rect {
    focus_stop_pressed(
        use_a11y(),
        content,
        label,
        role,
        enabled,
        radius,
        on_activate,
        Vec::new(),
        StopRing::Outside,
        Some(pressing),
    )
}

/// [`focus_stop`] with a context menu. A right click opens `menu` at the
/// pointer, a touch that rests opens it where the finger is, and the Menu
/// key or Shift+F10 opens it against the box. An empty `menu` opens nothing.
/// The menu needs the window's overlays; without them it stays closed.
///
/// Another file-manager style list, a tab strip or a card grid needs the
/// same, which is why it lives here and not in the file chooser.
#[allow(clippy::too_many_arguments)]
pub fn focus_stop_menu(
    content: Rect,
    label: impl Into<String>,
    role: AccessibilityRole,
    enabled: bool,
    radius: Radius,
    on_activate: impl Into<EventHandler<()>>,
    menu: Vec<MenuItem>,
) -> Rect {
    focus_stop_with(
        use_a11y(),
        content,
        label,
        role,
        enabled,
        radius,
        on_activate,
        menu,
        StopRing::Outside,
    )
}

/// Where a focus stop draws its ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StopRing {
    /// Just outside the box. The default.
    Outside,
    /// Just inside the box, so a clipped container edge cannot cut it off.
    Inside,
}

/// [`focus_stop_menu`] on a focus node the caller made. A list that moves
/// focus itself (Home, End, type-ahead) keeps one node per row and hands
/// each to its row here, so it can ask for any row by position. `ring` says
/// where the ring is drawn.
#[allow(clippy::too_many_arguments)]
pub fn focus_stop_with(
    a11y_id: AccessibilityId,
    content: Rect,
    label: impl Into<String>,
    role: AccessibilityRole,
    enabled: bool,
    radius: Radius,
    on_activate: impl Into<EventHandler<()>>,
    menu: Vec<MenuItem>,
    ring: StopRing,
) -> Rect {
    focus_stop_pressed(a11y_id, content, label, role, enabled, radius, on_activate, menu, ring, None)
}

/// [`focus_stop_with`] that also tells the caller when the box is held down.
/// The stop owns the box's pointer handlers, so the caller cannot set its own
/// press state on `content`: `pressing` is set while a button is down on the
/// box and cleared when it comes up, for the fill's pressed layer (ST-06).
#[allow(clippy::too_many_arguments)]
pub fn focus_stop_pressed(
    a11y_id: AccessibilityId,
    content: Rect,
    label: impl Into<String>,
    role: AccessibilityRole,
    enabled: bool,
    radius: Radius,
    on_activate: impl Into<EventHandler<()>>,
    menu: Vec<MenuItem>,
    ring: StopRing,
    pressing: Option<State<bool>>,
) -> Rect {
    let core = use_focus_core();
    let focus = use_focus(a11y_id);
    let press = use_press();
    let usable = use_enabled(enabled);
    let mut area = use_state(Area::default);
    let overlays = try_consume_context::<Overlays>().filter(|_| !menu.is_empty());
    let on_activate: EventHandler<()> = on_activate.into();
    let on_key = on_activate.clone();
    let tokens = use_tokens();
    // The stop's radius is the face's own, painted here, and the ring reads it.
    let control = content
        .tw(&tokens, &[rounded(radius)])
        .a11y_id(a11y_id)
        .a11y_role(role)
        .a11y_alt(label.into())
        .a11y_enabled(usable)
        .a11y_focusable(usable)
        .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
        .maybe(usable, |element| {
            let menu_down = menu.clone();
            let menu_key = menu.clone();
            element
                .on_pointer_down(move |event: Event<PointerEventData>| {
                    event.stop_propagation();
                    if let Some(mut pressing) = pressing {
                        pressing.set_if_modified(true);
                    }
                    let right = matches!(
                        event.data(),
                        PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                    );
                    if let (true, Some(overlays)) = (right, overlays) {
                        overlays.show_menu_from(
                            Some(a11y_id),
                            PanelTrigger::Pointer(point_of(&event)),
                            menu_down.clone(),
                            false,
                        );
                        return;
                    }
                    let long = overlays.map(|overlays| {
                        let items = menu_down.clone();
                        EventHandler::new(move |at: (f32, f32)| {
                            overlays.show_menu_from(
                                Some(a11y_id),
                                PanelTrigger::Pointer(at),
                                items.clone(),
                                false,
                            );
                        })
                    });
                    press.begin(&event, long);
                })
                .on_global_pointer_move(move |event: Event<PointerEventData>| {
                    press.moved(&event);
                })
                .on_global_pointer_up(move |_| {
                    press.release();
                    if let Some(mut pressing) = pressing {
                        pressing.set_if_modified(false);
                    }
                })
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    if !press.tap(&event) {
                        return;
                    }
                    on_activate.call(());
                    press_focus(&core, a11y_id);
                })
                .on_key_down(move |event: Event<KeyboardEventData>| {
                    // Alt+Enter is Properties across the file views, not a press.
                    if is_activation_key(&event.key) && !event.modifiers.contains(Modifiers::ALT) {
                        event.stop_propagation();
                        on_key.call(());
                        return;
                    }
                    let asks = event.key == Key::Named(NamedKey::ContextMenu)
                        || (event.key == Key::Named(NamedKey::F10)
                            && event.modifiers.contains(Modifiers::SHIFT));
                    if let (true, Some(overlays)) = (asks, overlays) {
                        event.stop_propagation();
                        let below = area.read().clone();
                        overlays.show_menu_from(
                            Some(a11y_id),
                            PanelTrigger::Control(PanelRect::from_area(&below)),
                            menu_key.clone(),
                            true,
                        );
                    }
                })
        });
    match ring {
        StopRing::Outside => control.focusable(focus()),
        StopRing::Inside => control.focusable_inside(focus()),
    }
}
