//! The one quiet interaction surface every control shares.
//!
//! Mirrors `ControlCentreButtonSurface.qml`: a transparent base with a
//! hover layer, a pressed layer, a selection layer and a keyboard-focus
//! outline. Idle outlines are not used; they turn lists into a grid of
//! boxes.

use freya::prelude::*;

use crate::theme::{SharedTokens, Tokens};

/// Group surface radius: `Theme.insetRadius(cardRadius, gap)`.
pub const CORNER: f32 = 12.0;
/// Hover radius of rows and buttons inside a group.
pub const ROW_RADIUS: f32 = 8.0;
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const BUTTON_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 44.0;
pub const TALL_ROW_HEIGHT: f32 = 60.0;
pub const FONT_SIZE: f32 = 14.0;
pub const FONT_SMALL: f32 = 12.0;
pub const FONT_HEADING: f32 = 16.0;

/// The live palette the window root provided. Reading it subscribes the
/// component to accent and colour-scheme changes.
pub fn use_tokens() -> Tokens {
    *use_consume::<SharedTokens>().0.read()
}

pub fn focus_border(tokens: &Tokens, focus: Focus) -> Border {
    if focus == Focus::Keyboard {
        Border::new()
            .width(1.0)
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
        tokens.pressed.with_af32(0.72)
    } else if hovering {
        tokens.hover.with_af32(0.42)
    } else if selected {
        tokens.selection.with_af32(0.44)
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
