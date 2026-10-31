//! The interaction surface every control shares.
//!
//! Interaction is a set of alpha layers drawn over the surface a control
//! sits on: selection, then hover or press, as
//! `ControlCentreButtonSurface.qml` does. Apply it with
//! [`crate::style::surface`]; keyboard focus is [`crate::motion::Focusable`].

use freya::prelude::*;

use crate::theme::{SharedTokens, Tokens};

/// Whether the container around a control is enabled. A disabled row provides
/// `false`, so the controls in it dim and ignore the pointer without each
/// caller disabling them one by one.
#[derive(Clone, Copy, PartialEq)]
struct ContainerEnabled(State<bool>);

/// Provide `enabled` to every control below this component. Call inside
/// `render`; this uses hooks.
pub fn provide_enabled(enabled: bool) {
    let mut flag = use_provide_context(|| ContainerEnabled(State::create(enabled)));
    flag.0.set_if_modified(enabled);
}

/// A control's own `enabled` flag combined with its container's. Call inside
/// `render`; this uses a hook.
pub fn use_enabled(own: bool) -> bool {
    let container = use_try_consume::<ContainerEnabled>();
    own && container.is_none_or(|flag| (flag.0)())
}

/// The window's size in logical pixels, the unit every length of the toolkit is in.
///
/// `PlatformWindow::root_size` is what the window system reports: the size in physical pixels.
/// On a display scaled 2x a 1000 x 700 window reports 2000 x 1400, and a length taken from it as
/// it is (a slide's travel, how far a menu can reach, the width a layout compares with the narrow
/// breakpoint) is twice what the window holds. Reading it subscribes the caller to a resize and to
/// a change of scale.
pub fn window_size() -> torin::prelude::Size2D {
    let window = Platform::get().current_window();
    let size = *window.root_size.read();
    let scale = (*window.scale_factor.read() as f32).max(f32::EPSILON);
    torin::prelude::Size2D::new(size.width / scale, size.height / scale)
}

/// [`window_size`] without subscribing: for a handler, which reads the size at the moment it runs.
pub fn window_size_peek() -> torin::prelude::Size2D {
    let window = Platform::get().current_window();
    let size = *window.root_size.peek();
    let scale = (*window.scale_factor.peek() as f32).max(f32::EPSILON);
    torin::prelude::Size2D::new(size.width / scale, size.height / scale)
}

/// The live palette the window root provided. Reading it subscribes the
/// component to accent and colour-scheme changes.
pub fn use_tokens() -> Tokens {
    *use_consume::<SharedTokens>().0.read()
}

/// The live palette, as [`use_tokens`] gives it, but not a hook: it may be
/// called conditionally, from a helper that only some renders reach, without
/// changing the number of hooks the component runs. Use it only where a hook
/// is not allowed; everywhere else `use_tokens` is the call.
pub fn tokens_now() -> Tokens {
    *try_consume_context::<SharedTokens>()
        .expect("the window root provides the tokens")
        .0
        .read()
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
