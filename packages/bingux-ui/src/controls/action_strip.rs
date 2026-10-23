//! The action strip: two to four equal actions in one rounded bar, each an
//! icon over a short label, such as Forget, Disconnect and Settings on a
//! network's details page. Use it for the verbs that apply to the thing the
//! page is about; use a row for a setting.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{focus_border, is_activation_key, surface_color, use_tokens};
use crate::typography::{BODY, Ink};

/// A strip cell is this tall: icon, a small gap and one line of label.
const CELL_HEIGHT: f32 = 76.0;
const DISABLED_OPACITY: f32 = 0.45;

/// One action in a strip.
#[derive(PartialEq, Clone)]
pub struct StripAction {
    icon: Icon,
    label: String,
    on_press: EventHandler<()>,
    enabled: bool,
}

/// An action with an icon and a label, such as `strip_action(Icon::Delete, "Forget", ...)`.
pub fn strip_action(
    icon: Icon,
    label: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> StripAction {
    StripAction {
        icon,
        label: label.into(),
        on_press: on_press.into(),
        enabled: true,
    }
}

impl StripAction {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// The bar that holds the actions, with a hairline between neighbours.
#[derive(PartialEq)]
pub struct ActionStrip {
    actions: Vec<StripAction>,
}

pub fn action_strip(actions: Vec<StripAction>) -> ActionStrip {
    ActionStrip { actions }
}

impl Component for ActionStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut bar = rect()
            .width(Size::fill())
            .height(Size::px(CELL_HEIGHT))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .corner_radius(RADIUS_LG)
            .background(tokens.control);
        for (index, action) in self.actions.iter().enumerate() {
            if index > 0 {
                bar = bar.child(
                    rect()
                        .width(Size::px(1.0))
                        .height(Size::px(CELL_HEIGHT - 2.0 * S4))
                        .background(tokens.bar_divider),
                );
            }
            bar = bar.child(StripCell(action.clone()));
        }
        bar
    }
}

#[derive(PartialEq)]
struct StripCell(StripAction);

impl Component for StripCell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let action = &self.0;
        let enabled = action.enabled;
        let background = surface_color(
            &tokens,
            Color::TRANSPARENT,
            enabled && hovering(),
            enabled && pressing(),
            false,
        );
        let ink = if enabled { Ink::Text } else { Ink::Muted };
        let on_press = action.on_press.clone();
        let on_key = on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(action.label.clone())
            .width(Size::flex(1.0))
            .height(Size::fill())
            .center()
            .spacing(S1)
            .corner_radius(RADIUS_LG)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .opacity(if enabled { 1.0 } else { DISABLED_OPACITY })
            .maybe(enabled, |cell| {
                cell.on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |_: Event<PressEventData>| {
                        on_press.call(());
                        a11y_id.request_focus();
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            on_key.call(());
                        }
                    })
            })
            .child(icons::icon(action.icon, ICON_LG, ink.color(&tokens)))
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, action.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}
