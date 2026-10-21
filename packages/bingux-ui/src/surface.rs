//! Sizes and the interaction surface every control shares.
//!
//! One scale for the whole library. A row is 40 px, or 56 px with a
//! subtitle; a control is 36 px; a group card has a 12 px radius and a row
//! or button 8 px. Interaction is a set of alpha layers drawn over the
//! surface a control sits on: selection, then hover or press, as
//! `ControlCentreButtonSurface.qml` does. Keyboard focus is a 2 px accent
//! ring on controls. Idle outlines are not used; they turn lists into a grid
//! of boxes.

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

/// Source-over composition: `top` drawn over `bottom`, both possibly
/// translucent. The result keeps the combined alpha, so layers stack the
/// way they do on screen.
pub fn over(top: Color, bottom: Color) -> Color {
    let top_alpha = f32::from(top.a()) / 255.0;
    let bottom_alpha = f32::from(bottom.a()) / 255.0;
    let out_alpha = top_alpha + bottom_alpha * (1.0 - top_alpha);
    if out_alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |t: u8, b: u8| {
        let mixed = (f32::from(t) * top_alpha + f32::from(b) * bottom_alpha * (1.0 - top_alpha))
            / out_alpha;
        mixed.round().clamp(0.0, 255.0) as u8
    };
    Color::from((
        channel(top.r(), bottom.r()),
        channel(top.g(), bottom.g()),
        channel(top.b(), bottom.b()),
        (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ))
}

/// The colour of an interactive surface: `base`, then the selection layer
/// when selected, then the press layer when pressed or the hover layer when
/// hovered. Each layer is an alpha overlay composed over what is beneath it,
/// as `ControlCentreButtonSurface` draws them. Press replaces hover, as the
/// shell hides the hover layer while the control is down.
pub fn surface_color(
    tokens: &Tokens,
    base: Color,
    hovering: bool,
    pressing: bool,
    selected: bool,
) -> Color {
    let mut color = if selected {
        over(tokens.selection, base)
    } else {
        base
    };
    if pressing {
        color = over(tokens.pressed, color);
    } else if hovering {
        color = over(tokens.hover, color);
    }
    color
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
