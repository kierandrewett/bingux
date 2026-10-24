//! A numeric stepper (SwiftUI `Stepper`, libadwaita `GtkSpinButton`): a
//! value label with `-` and `+` buttons in one 32 px pill on `elevated`.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::surface::{
    CONTROL_HEIGHT, FONT_SIZE, ROW_RADIUS, focus_border, surface_color, use_tokens,
};

/// An integer stepper bounded by `min..=max`, moving by `step`.
#[derive(PartialEq)]
pub struct Stepper {
    value: i64,
    min: i64,
    max: i64,
    step: i64,
    suffix: Option<String>,
    enabled: bool,
    on_change: EventHandler<i64>,
}

pub fn stepper(value: i64, on_change: impl Into<EventHandler<i64>>) -> Stepper {
    Stepper {
        value,
        min: 0,
        max: 100,
        step: 1,
        suffix: None,
        enabled: true,
        on_change: on_change.into(),
    }
}

impl Stepper {
    pub fn range(mut self, min: i64, max: i64) -> Self {
        self.min = min;
        self.max = max.max(min);
        self
    }

    pub fn step(mut self, step: i64) -> Self {
        self.step = step.max(1);
        self
    }

    /// Text drawn after the value, such as `" px"` or `" min"`.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for Stepper {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let value = self.value.clamp(self.min, self.max);
        let (min, max, step) = (self.min, self.max, self.step);
        let enabled = self.enabled;
        let on_change = self.on_change.clone();
        let can_decrement = enabled && value > min;
        let can_increment = enabled && value < max;
        let decrement = {
            let on_change = on_change.clone();
            move || on_change.call((value - step).max(min))
        };
        let increment = {
            let on_change = on_change.clone();
            move || on_change.call((value + step).min(max))
        };
        let text = match &self.suffix {
            Some(suffix) => format!("{value}{suffix}"),
            None => value.to_string(),
        };
        let on_key_down = {
            let decrement = decrement.clone();
            let increment = increment.clone();
            move |event: Event<KeyboardEventData>| {
                match event.key {
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowLeft) => {
                        if can_decrement {
                            decrement()
                        }
                    }
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowRight) => {
                        if can_increment {
                            increment()
                        }
                    }
                    Key::Named(NamedKey::Home) => on_change.call(min),
                    Key::Named(NamedKey::End) => on_change.call(max),
                    _ => return,
                }
                event.stop_propagation();
            }
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::SpinButton)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value as f64);
                builder.set_min_numeric_value(min as f64);
                builder.set_max_numeric_value(max as f64);
                builder.set_numeric_value_step(step as f64);
            })
            .height(Size::px(CONTROL_HEIGHT))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .corner_radius(ROW_RADIUS)
            .background(tokens.elevated)
            .maybe(!enabled, |element| element.opacity(0.45))
            .border(if focus() == Focus::Keyboard {
                focus_border(&tokens, focus())
            } else {
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.outline)
            })
            .padding((0.0, 2.0))
            .maybe(enabled, |element| element.on_key_down(on_key_down))
            .child(step_button(Icon::ListRemove, can_decrement, move |_| {
                decrement();
                a11y_id.request_focus();
            }))
            .child(
                rect()
                    .min_width(Size::px(44.0))
                    .height(Size::fill())
                    .padding((0.0, 6.0))
                    .center()
                    .child(
                        label()
                            .text(text)
                            .font_size(FONT_SIZE)
                            .color(tokens.text)
                            .max_lines(1),
                    ),
            )
            .child(step_button(Icon::ListAdd, can_increment, move |_| {
                increment();
                a11y_id.request_focus();
            }))
    }
}

/// One 28 px square half of the stepper pill.
#[derive(PartialEq)]
struct StepButton {
    icon: Icon,
    enabled: bool,
    on_press: EventHandler<()>,
}

fn step_button(icon: Icon, enabled: bool, on_press: impl Into<EventHandler<()>>) -> StepButton {
    StepButton {
        icon,
        enabled,
        on_press: on_press.into(),
    }
}

impl Component for StepButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let enabled = self.enabled;
        let on_press = self.on_press.clone();
        rect()
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(6.0)
            .center()
            .background(if enabled {
                surface_color(&tokens, Color::TRANSPARENT, hovering(), pressing(), false)
            } else {
                Color::TRANSPARENT
            })
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
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
                    })
            })
            .child(icons::icon(
                self.icon,
                14.0,
                if enabled { tokens.text } else { tokens.muted },
            ))
    }
}
