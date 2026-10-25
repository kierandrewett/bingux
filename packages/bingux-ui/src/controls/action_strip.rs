//! The action strip: two to four equal actions in one rounded bar, each an
//! icon over a short label, such as Forget, Disconnect and Settings on a
//! network's details page. Use it for the verbs that apply to the thing the
//! page is about; use a row for a setting.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BODY, Ink};

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
        // The divider is as tall as the cell less a gutter at each end.
        let mut bar = rect().tw(&tokens, BOX).tw(
            &tokens,
            &[
                h(Dim::StripCell),
                flex_row(),
                grow_children(),
                items(Align::Center),
            ],
        );
        for (index, action) in self.actions.iter().enumerate() {
            if index > 0 {
                bar = bar.child(rect().tw(
                    &tokens,
                    &[w(Dim::Hairline), h_full(), my(S4), bg(Paint::Separator)],
                ));
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
        let ink = if enabled { Ink::Text } else { Ink::Muted };
        let on_press = action.on_press.clone();
        let on_key = on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(action.label.clone())
            .tw(
                &tokens,
                &[
                    // The cell fills its focus wrapper; the wrapper takes the
                    // cell's share of the strip.
                    w_full(),
                    h_full(),
                    center(),
                    gap(S1),
                    rounded(Radius::Card),
                    surface(
                        Paint::Transparent,
                        enabled && hovering(),
                        enabled && pressing(),
                        false,
                    ),
                ],
            )
            // Opacity is a live visual state, not a token, so it stays raw.
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
            .child(icons::icon(action.icon, IconSize::Row, ink.color(&tokens)))
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, action.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .focusable(focus(), Radius::Card)
            .tw(&tokens, &[w_flex(), h_full()])
    }
}
