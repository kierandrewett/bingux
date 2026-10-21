//! The shell's segmented control (`SegmentedControl.qml`).

use freya::prelude::*;

use crate::scale::*;
use crate::surface::{focus_border, surface_color, use_tokens};

/// The shell's segmented control: a quiet `elevated` rail where only the
/// selected segment is raised.
#[derive(PartialEq)]
pub struct Segmented {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
}

pub fn segmented(
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Segmented {
    Segmented {
        options,
        selected: selected.to_string(),
        on_select: on_select.into(),
    }
}

impl Component for Segmented {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .height(Size::px(CONTROL_HEIGHT))
            .corner_radius(RADIUS)
            .background(tokens.elevated)
            .padding(S1)
            .spacing(S1)
            .direction(Direction::Horizontal)
            .children(self.options.iter().map(|(key, text)| {
                Element::from(Segment {
                    key: key.clone(),
                    text: text.clone(),
                    selected: *key == self.selected,
                    on_select: self.on_select.clone(),
                })
            }))
    }
}

#[derive(PartialEq)]
struct Segment {
    key: String,
    text: String,
    selected: bool,
    on_select: EventHandler<String>,
}

impl Component for Segment {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let base = if self.selected {
            tokens.elevated_pressed
        } else {
            Color::TRANSPARENT
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .height(Size::fill())
            .padding((0.0, S2))
            .center()
            .corner_radius(RADIUS)
            .background(surface_color(&tokens, base, hovering(), pressing(), false))
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
                on_select.call(key.clone());
                a11y_id.request_focus();
            })
            .child(
                label()
                    .text(self.text.clone())
                    .color(if self.selected {
                        tokens.text
                    } else {
                        tokens.muted
                    })
                    .font_size(TEXT_MD)
                    .max_lines(1),
            )
    }
}
