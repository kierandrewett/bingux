//! Buttons: `ActionButton`, the accent primary button, the small row pill
//! and the square `IconButton`.

use super::press::use_press;
use freya::prelude::*;

use crate::controls::progress::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{BODY, EMPHASIS, Ink};

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
        let press = use_press();
        let usable = use_enabled(self.enabled);
        let enabled = usable && !self.loading;
        let filled = self.accent || self.destructive;
        // The fill is a paint; the state layer on top comes from `surface`.
        // No idle outline: the shell's buttons are filled or flat. Only the
        // keyboard focus ring draws a border.
        let base = if self.destructive {
            Paint::Danger
        } else if filled {
            Paint::Accent
        } else if self.flat {
            Paint::Transparent
        } else {
            Paint::Control
        };
        let ink = if !usable {
            Ink::Muted
        } else if filled {
            Ink::OnAccent
        } else {
            Ink::Text
        };
        let color = ink.color(&tokens);
        let ink_paint = if !usable {
            Paint::Muted
        } else if filled {
            Paint::OnAccent
        } else {
            Paint::Text
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .tw(&tokens, if self.compact { BUTTON_COMPACT } else { BUTTON })
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    justify(Align::Center),
                    px(S3),
                    gap(S2),
                    surface(base, hovering(), pressing(), false),
                ],
            )
            // A flat button has no fill, so a minimum width would only centre
            // a short label inside empty space and push it off the row's edge.
            .maybe(!self.flat, |element| {
                element.tw(&tokens, &[min_w(Dim::Box9)])
            })
            .maybe(self.grow, |element| element.tw(&tokens, &[w_flex()]))
            .maybe(!usable, |element| element.opacity(0.5))
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
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
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
                    .then(|| rect().child(spinner(SpinnerSize::Small).ink(ink_paint))),
            )
            .maybe_child(
                (!self.loading)
                    .then(|| {
                        self.icon
                            .map(|icon| icons::icon(icon, IconSize::Control, color))
                    })
                    .flatten(),
            )
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .focusable(focus(), Radius::Control)
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
        let press = use_press();
        let base = if self.prominent {
            Paint::Transparent
        } else {
            Paint::Control
        };
        let style = if self.prominent { EMPHASIS } else { BODY };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .maybe(!enabled, |element| element.opacity(0.5))
            .tw(&tokens, BUTTON_COMPACT)
            .tw(
                &tokens,
                &[
                    center(),
                    px(S3),
                    surface(base, hovering(), pressing(), false),
                ],
            )
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
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
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
                style
                    .in_ink(Ink::Text)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .focusable(focus(), Radius::Control)
    }
}

/// The shell's `IconButton`: a square labelled icon action with a tooltip.
#[derive(PartialEq)]
pub struct IconButton {
    icon: Icon,
    size: Dim,
    tooltip: Option<String>,
    on_press: EventHandler<()>,
    danger: bool,
    filled: bool,
    active: bool,
    enabled: bool,
}

pub fn icon_button(icon: Icon, on_press: impl Into<EventHandler<()>>) -> IconButton {
    IconButton {
        icon,
        size: Dim::Button,
        tooltip: None,
        on_press: on_press.into(),
        danger: false,
        filled: false,
        active: false,
        enabled: true,
    }
}

impl IconButton {
    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_string());
        self
    }

    pub fn size(mut self, size: Dim) -> Self {
        self.size = size;
        self
    }

    /// Hover fills with the danger colour, for close buttons.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// A control fill at rest, for a button that stands on its own rather
    /// than in a header bar.
    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }

    /// Ring the button in the accent colour, for the one button of a set
    /// that is on or playing.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Dim the button and ignore presses, for an action that is not
    /// available yet. It stays on screen so a header keeps its shape.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
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
        let usable = use_enabled(self.enabled);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let press = use_press();
        // Only a hovered danger button takes the danger fill; the state
        // layer from `surface` then sits over it.
        let base = if self.danger && hovering() {
            Paint::Danger
        } else if self.filled {
            Paint::Control
        } else {
            Paint::Transparent
        };
        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(usable)
            .a11y_enabled(usable)
            .a11y_role(AccessibilityRole::Button)
            .maybe(self.tooltip.is_some(), |element| {
                element.a11y_alt(self.tooltip.clone().unwrap_or_default())
            })
            .tw(&tokens, ICON_BUTTON)
            .tw(
                &tokens,
                &[
                    w(self.size),
                    h(self.size),
                    surface(base, hovering(), pressing(), false),
                ],
            )
            .maybe(self.active, |element| {
                element.tw(&tokens, &[border(Line::Stroke(Dim::Ring, Paint::Accent))])
            })
            .maybe(!usable, |element| element.opacity(0.5))
            .maybe(usable, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        pressing.set(true);
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
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
            .child(icons::icon(
                self.icon,
                IconSize::Control,
                Ink::Text.color(&tokens),
            ))
            .focusable(focus(), Radius::Pill);
        match &self.tooltip {
            Some(text) => TooltipContainer::new(Tooltip::new_text(text.clone()))
                .child(button)
                .into(),
            None => Element::from(button),
        }
    }
}
