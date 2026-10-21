//! Sizes and the interaction surface every control shares.
//!
//! One scale for the whole library. A row is 50 px, or 62 px with a
//! subtitle; a control is 34 px; a group and a card have a 12 px radius and
//! a row or button 8 px. Interaction is a transparent base with a white
//! hover layer, a stronger pressed layer and a 2 px keyboard-focus ring.
//! Idle outlines are not used; they turn lists into a grid of boxes.

use freya::prelude::*;

use crate::theme::{SharedTokens, Tokens};

pub use crate::scale::*;

/// The live palette the window root provided. Reading it subscribes the
/// component to accent and colour-scheme changes.
pub fn use_tokens() -> Tokens {
    *use_consume::<SharedTokens>().0.read()
}

/// The 1 px outline around buttons, dropdowns, fields and segmented
/// controls. Keyboard focus replaces it with the accent ring.
pub fn field_border(tokens: &Tokens, focus: Focus) -> Border {
    if focus == Focus::Keyboard {
        return focus_border(tokens, focus);
    }
    Border::new()
        .width(1.0)
        .alignment(BorderAlignment::Inner)
        .fill(tokens.outline)
}

pub fn focus_border(tokens: &Tokens, focus: Focus) -> Border {
    if focus == Focus::Keyboard {
        Border::new()
            .width(RING_WIDTH)
            .alignment(BorderAlignment::Inner)
            .fill(tokens.accent)
    } else {
        Border::new()
    }
}

/// The hover, pressed and selected layers of `ControlCentreButtonSurface`.
pub fn surface_color(
    tokens: &Tokens,
    base: Color,
    hovering: bool,
    pressing: bool,
    selected: bool,
) -> Color {
    if pressing {
        tokens.pressed
    } else if hovering {
        tokens.hover
    } else if selected {
        tokens.selection
    } else {
        base
    }
}

/// Hover and press state for one interactive element, with the handlers
/// that keep it in step with the pointer.
#[derive(Clone, Copy)]
pub struct Interaction {
    pub hovering: State<bool>,
    pub pressing: State<bool>,
}

impl Interaction {
    /// Call inside `render`; this uses two hooks.
    pub fn use_interaction() -> Self {
        Self {
            hovering: use_state(|| false),
            pressing: use_state(|| false),
        }
    }

    pub fn hovering(&self) -> bool {
        (self.hovering)()
    }

    pub fn pressing(&self) -> bool {
        (self.pressing)()
    }

    /// Attach enter, leave, down and global-up handlers to an element.
    pub fn attach<E>(self, element: E) -> E
    where
        E: EventHandlersExt,
    {
        let Self {
            mut hovering,
            mut pressing,
        } = self;
        element
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
    }
}

/// Enter and Space activate buttons, switches and other pressable controls
/// from the keyboard.
pub fn is_activation_key(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Enter) => true,
        Key::Character(text) => text == " ",
        _ => false,
    }
}
