//! Buttons: `ActionButton`, the accent primary button, the small row pill
//! and the square `IconButton`.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::surface::{
    BUTTON_HEIGHT, CONTROL_HEIGHT, FONT_SIZE, FONT_SMALL, ROW_RADIUS, focus_border, surface_color,
    use_tokens,
};

/// The shell's `ActionButton`: 36 px tall on `elevated` at radius 8.
#[derive(PartialEq)]
pub struct ActionButton {
    text: String,
    icon: Option<Icon>,
    flat: bool,
    accent: bool,
    enabled: bool,
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
        enabled: true,
        on_press: on_press.into(),
    }
}

impl ActionButton {
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
        let base = if self.accent {
            tokens.accent
        } else if self.flat {
            Color::TRANSPARENT
        } else {
            tokens.elevated
        };
        let background = if self.accent {
            if pressing() {
                Color::lerp(tokens.accent, Color::BLACK, 0.16)
            } else if hovering() {
                Color::lerp(tokens.accent, Color::WHITE, 0.08)
            } else {
                base
            }
        } else if pressing() {
            tokens.pressed
        } else if hovering() {
            tokens.hover
        } else {
            base
        };
        let color = if !self.enabled {
            tokens.muted
        } else if self.accent {
            tokens.accent_foreground
        } else {
            tokens.text
        };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(2.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.accent)
        } else {
            Border::new()
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(self.enabled)
            .a11y_enabled(self.enabled)
            .a11y_role(AccessibilityRole::Button)
            .height(Size::px(BUTTON_HEIGHT))
            .min_width(Size::px(36.0))
            .padding((0.0, 12.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .main_align(Alignment::Center)
            .corner_radius(ROW_RADIUS)
            .background(background.mul_if(!self.enabled, 0.6))
            .border(border)
            .cursor(if self.enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
            .maybe(self.enabled, |element| {
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
            })
            .maybe_child(self.icon.map(|icon| icons::icon(icon, 16.0, color)))
            .child(
                label()
                    .text(self.text.clone())
                    .color(color)
                    .font_size(FONT_SIZE)
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

/// A small muted pill for a row's secondary action (`ControlRow` actionLabel).
#[derive(PartialEq)]
pub struct RowButton {
    text: String,
    on_press: EventHandler<()>,
}

pub fn row_button(text: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> RowButton {
    RowButton {
        text: text.into(),
        on_press: on_press.into(),
    }
}

impl Component for RowButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .height(Size::px(CONTROL_HEIGHT))
            .padding((0.0, 8.0))
            .center()
            .corner_radius(7.0)
            .background(surface_color(
                &tokens,
                Color::TRANSPARENT,
                hovering(),
                pressing(),
                false,
            ))
            .border(focus_border(&tokens, focus()))
            .cursor(CursorIcon::Pointer)
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
            .child(
                label()
                    .text(self.text.clone())
                    .color(tokens.muted)
                    .font_size(FONT_SMALL)
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
        size: CONTROL_HEIGHT,
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
            .corner_radius(ROW_RADIUS)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .cursor(CursorIcon::Pointer)
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
            .child(icons::icon(self.icon, 16.0, tokens.text));
        match &self.tooltip {
            Some(text) => TooltipContainer::new(Tooltip::new_text(text.clone()))
                .child(button)
                .into(),
            None => Element::from(button),
        }
    }
}
