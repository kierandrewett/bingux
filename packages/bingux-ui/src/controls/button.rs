//! Buttons: `ActionButton`, the accent primary button, the small row pill
//! and the square `IconButton`.

use freya::prelude::*;

use crate::controls::progress::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{focus_border, is_activation_key, surface_color, use_enabled, use_tokens};

/// The shell's `ActionButton`: 36 px tall on `control` at radius 8, with no
/// idle outline.
#[derive(PartialEq)]
pub struct ActionButton {
    text: String,
    icon: Option<Icon>,
    flat: bool,
    accent: bool,
    destructive: bool,
    enabled: bool,
    loading: bool,
    grow: bool,
    compact: bool,
    on_press: EventHandler<()>,
}

pub fn action_button(
    text: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> ActionButton {
    ActionButton {
        text: text.into(),
        icon: None,
        flat: false,
        accent: false,
        destructive: false,
        enabled: true,
        loading: false,
        grow: false,
        compact: false,
        on_press: on_press.into(),
    }
}

impl ActionButton {
    /// Fill with the accent colour (the same as [`primary_button`]).
    pub fn suggested(mut self) -> Self {
        self.accent = true;
        self
    }

    /// Fill with the danger colour, for actions that lose data.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Show a small spinner instead of the icon and refuse presses.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Share the parent's width equally with sibling buttons (dialogs).
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// 32 px high instead of 36, for a button inside a row.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for ActionButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let usable = use_enabled(self.enabled);
        let enabled = usable && !self.loading;
        let filled = self.accent || self.destructive;
        let fill = if self.destructive {
            tokens.danger
        } else {
            tokens.accent
        };
        let base = if filled {
            fill
        } else if self.flat {
            Color::TRANSPARENT
        } else {
            tokens.control
        };
        let background = if filled {
            if pressing() {
                Color::lerp(fill, Color::BLACK, 0.16)
            } else if hovering() {
                Color::lerp(fill, Color::WHITE, 0.08)
            } else {
                base
            }
        } else {
            surface_color(&tokens, base, hovering(), pressing(), false)
        };
        let color = if !usable {
            tokens.muted
        } else if filled {
            tokens.accent_foreground
        } else {
            tokens.text
        };
        // No idle outline: the shell's buttons are filled or flat. Only the
        // keyboard focus ring draws a border.
        let border = focus_border(&tokens, focus());
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .height(Size::px(if self.compact {
                CONTROL_HEIGHT
            } else {
                BUTTON_HEIGHT
            }))
            .min_width(Size::px(BOX_LG))
            .maybe(self.grow, |element| element.width(Size::flex(1.0)))
            .padding((0.0, S3))
            .spacing(S2)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .main_align(Alignment::Center)
            .corner_radius(RADIUS_PILL)
            .background(background)
            .maybe(!usable, |element| element.opacity(0.5))
            .border(border)
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        pressing.set(true);
                    })
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        on_press.call(());
                        a11y_id.request_focus();
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_key.call(());
                        }
                    })
            })
            .maybe_child(
                self.loading
                    .then(|| rect().child(spinner(SpinnerSize::Small).color(color))),
            )
            .maybe_child(
                (!self.loading)
                    .then(|| self.icon.map(|icon| icons::icon(icon, ICON_MD, color)))
                    .flatten(),
            )
            .child(
                label()
                    .text(self.text.clone())
                    .color(color)
                    .font_size(TEXT_MD)
                    .max_lines(1),
            )
    }
}

/// The page's primary action, filled with the accent colour.
pub fn primary_button(
    text: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> ActionButton {
    let mut button = action_button(text, on_press);
    button.accent = true;
    button
}

/// A row's secondary action: the same filled button as every other control.
#[derive(PartialEq)]
pub struct RowButton {
    text: String,
    prominent: bool,
    on_press: EventHandler<()>,
}

pub fn row_button(text: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> RowButton {
    RowButton {
        text: text.into(),
        prominent: false,
        on_press: on_press.into(),
    }
}

impl RowButton {
    /// Text colour instead of muted, for banners and toasts.
    pub fn prominent(mut self) -> Self {
        self.prominent = true;
        self
    }
}

impl Component for RowButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .maybe(!enabled, |element| element.opacity(0.5))
            .height(Size::px(CONTROL_HEIGHT))
            .padding((0.0, S4))
            .center()
            .corner_radius(RADIUS_PILL)
            .background(surface_color(
                &tokens,
                if self.prominent {
                    Color::TRANSPARENT
                } else {
                    tokens.control
                },
                hovering(),
                pressing(),
                false,
            ))
            .border(focus_border(&tokens, focus()))
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        pressing.set(true);
                    })
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        on_press.call(());
                        a11y_id.request_focus();
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_key.call(());
                        }
                    })
            })
            .child(
                label()
                    .text(self.text.clone())
                    .color(tokens.text)
                    .font_size(TEXT_MD)
                    .font_weight(if self.prominent {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .max_lines(1),
            )
    }
}

/// The shell's `IconButton`: a square labelled icon action with a tooltip.
#[derive(PartialEq)]
pub struct IconButton {
    icon: Icon,
    size: f32,
    tooltip: Option<String>,
    on_press: EventHandler<()>,
    danger: bool,
}

pub fn icon_button(icon: Icon, on_press: impl Into<EventHandler<()>>) -> IconButton {
    IconButton {
        icon,
        size: BUTTON_HEIGHT,
        tooltip: None,
        on_press: on_press.into(),
        danger: false,
    }
}

impl IconButton {
    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_string());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Hover fills with the danger colour, for close buttons.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

impl Component for IconButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let background = if self.danger && hovering() {
            tokens.danger.with_af32(if pressing() { 1.0 } else { 0.8 })
        } else {
            surface_color(&tokens, Color::TRANSPARENT, hovering(), pressing(), false)
        };
        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .maybe(self.tooltip.is_some(), |element| {
                element.a11y_alt(self.tooltip.clone().unwrap_or_default())
            })
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .center()
            .corner_radius(RADIUS_PILL)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
                a11y_id.request_focus();
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    event.stop_propagation();
                    on_key.call(());
                }
            })
            .child(icons::icon(self.icon, ICON_MD, tokens.text));
        match &self.tooltip {
            Some(text) => TooltipContainer::new(Tooltip::new_text(text.clone()))
                .child(button)
                .into(),
            None => Element::from(button),
        }
    }
}
