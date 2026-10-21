//! The shell's switch (`ControlSwitch.qml`).

use freya::prelude::*;

use crate::surface::use_tokens;

/// The shell's switch: a 36 x 20 track, accent when on, with a 16 px thumb.
#[derive(PartialEq)]
pub struct Switch {
    on: bool,
    enabled: bool,
    on_toggle: EventHandler<bool>,
}

pub fn switch(on: bool, on_toggle: impl Into<EventHandler<bool>>) -> Switch {
    Switch {
        on,
        enabled: true,
        on_toggle: on_toggle.into(),
    }
}

impl Switch {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for Switch {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on = self.on;
        let on_toggle = self.on_toggle.clone();
        let track = if on { tokens.accent } else { tokens.elevated };
        let thumb = if on { Color::WHITE } else { tokens.text };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.text)
        } else if on {
            Border::new()
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.outline)
        };
        let overlay = if self.enabled && pressing() {
            Color::BLACK.with_af32(0.16)
        } else if self.enabled && hovering() {
            Color::WHITE.with_af32(0.08)
        } else {
            Color::TRANSPARENT
        };
        let thumb_width = if pressing() { 18.0 } else { 16.0 };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(self.enabled)
            .a11y_enabled(self.enabled)
            .a11y_role(AccessibilityRole::Switch)
            .width(Size::px(36.0))
            .height(Size::px(20.0))
            .corner_radius(10.0)
            .background(track.mul_if(!self.enabled, 0.45))
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
                        on_toggle.call(!on);
                        a11y_id.request_focus();
                    })
            })
            .child(rect().expanded().corner_radius(10.0).background(overlay))
            .child(
                rect()
                    .position(Position::new_absolute().top(2.0).left(if on {
                        36.0 - thumb_width - 2.0
                    } else {
                        2.0
                    }))
                    .width(Size::px(thumb_width))
                    .height(Size::px(16.0))
                    .corner_radius(8.0)
                    .background(thumb),
            )
    }
}
