//! The shell's slider (`SeekSlider.qml`).

use freya::prelude::*;

use crate::surface::{CONTROL_HEIGHT, ROW_RADIUS, focus_border, use_tokens};

/// The shell's slider: a 6 px track that thickens to 8 px while dragging,
/// filled in text colour and accent while interacting, with a thin tick as
/// the handle.
#[derive(PartialEq)]
pub struct Slider {
    value: f64,
    enabled: bool,
    on_moved: EventHandler<f64>,
    width: f32,
}

pub fn slider(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Slider {
    Slider {
        value: value.clamp(0.0, 100.0),
        enabled: true,
        on_moved: on_moved.into(),
        width: 220.0,
    }
}

impl Slider {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl Component for Slider {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut dragging = use_state(|| false);
        let mut size = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_moved = self.on_moved.clone();
        let value = self.value;
        let enabled = self.enabled;
        let track_height = if dragging() { 8.0 } else { 6.0 };
        let fill_color = if !enabled {
            tokens.muted
        } else if dragging() {
            tokens.accent
        } else {
            tokens.text
        };

        let percent_at = move |x: f64| -> f64 {
            let width = size.read().width() as f64;
            if width <= 0.0 {
                0.0
            } else {
                (x / width * 100.0).clamp(0.0, 100.0)
            }
        };
        let on_pointer_down = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                a11y_id.request_focus();
                dragging.set(true);
                on_moved.call(percent_at(event.element_location().x));
            }
        };
        let on_move = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if *dragging.peek() {
                    event.prevent_default();
                    let x = event.global_location().x - size.read().min_x() as f64;
                    on_moved.call(percent_at(x));
                }
            }
        };
        let on_key_down = {
            let on_moved = on_moved.clone();
            move |event: Event<KeyboardEventData>| {
                let step = match event.key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => -5.0,
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => 5.0,
                    _ => return,
                };
                on_moved.call((value + step).clamp(0.0, 100.0));
            }
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(100.0);
            })
            .width(Size::px(self.width))
            .height(Size::px(CONTROL_HEIGHT))
            .main_align(Alignment::Center)
            .corner_radius(ROW_RADIUS)
            .border(focus_border(&tokens, focus()))
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .maybe(enabled, |element| {
                element
                    .on_pointer_down(on_pointer_down)
                    .on_capture_global_pointer_move(on_move)
                    .on_global_pointer_up(move |_| dragging.set_if_modified(false))
                    .on_key_down(on_key_down)
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(track_height))
                    .corner_radius(track_height / 2.0)
                    .background(tokens.outline)
                    .child(
                        rect()
                            .width(Size::percent(value as f32))
                            .height(Size::fill())
                            .corner_radius(track_height / 2.0)
                            .background(fill_color)
                            .main_align(Alignment::End)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::Center)
                            .child(
                                rect()
                                    .width(Size::px(2.0))
                                    .height(Size::px((track_height - 2.0).max(2.0)))
                                    .corner_radius(1.0)
                                    .margin((0.0, 2.0, 0.0, 0.0))
                                    .background(tokens.bg),
                            ),
                    ),
            )
    }
}
