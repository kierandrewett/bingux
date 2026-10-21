//! Rows: the building block of every preference list.
//!
//! Mirrors `ControlRow.qml`: a leading icon, a title, an optional subtitle
//! and trailing content. Convenience constructors pair a row with a
//! switch, slider, choice or text field.

use freya::prelude::*;

use crate::controls::segmented::segmented;
use crate::controls::slider::slider;
use crate::controls::switch::switch;
use crate::icons::{self, Icon};
use crate::surface::{
    FONT_SIZE, FONT_SMALL, ROW_HEIGHT, ROW_RADIUS, TALL_ROW_HEIGHT, focus_border, surface_color,
    use_tokens,
};

/// One row: optional leading icon, title, optional subtitle, and trailing
/// content. The whole row is the target when `on_press` is set.
#[derive(PartialEq)]
pub struct Row {
    title: String,
    subtitle: Option<String>,
    trailing: Option<Element>,
    leading: Option<Icon>,
    on_press: Option<EventHandler<()>>,
    enabled: bool,
    navigates: bool,
    selected: bool,
}

pub fn row(title: impl Into<String>) -> Row {
    Row {
        title: title.into(),
        subtitle: None,
        trailing: None,
        leading: None,
        on_press: None,
        enabled: true,
        navigates: false,
        selected: false,
    }
}

impl Row {
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let subtitle = subtitle.into();
        self.subtitle = (!subtitle.is_empty()).then_some(subtitle);
        self
    }

    pub fn maybe_subtitle(mut self, subtitle: Option<String>) -> Self {
        self.subtitle = subtitle.filter(|text| !text.is_empty());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<Element>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn on_press(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.on_press = Some(on_press.into());
        self
    }

    /// Show a chevron and act like a link.
    pub fn navigates(mut self) -> Self {
        self.navigates = true;
        self
    }

    /// A restrained selection fill and an accent checkmark.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A muted value on the right, for read-only rows.
    pub fn value(self, value: impl Into<String>) -> Self {
        let value = value.into();
        if value.is_empty() {
            return self;
        }
        self.trailing(ValueText { text: value })
    }
}

#[derive(PartialEq)]
struct ValueText {
    text: String,
}

impl Component for ValueText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        label()
            .text(self.text.clone())
            .color(tokens.muted)
            .font_size(FONT_SMALL)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .max_width(Size::px(340.0))
    }
}

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let interactive = self.on_press.is_some() && self.enabled;
        let height = if self.subtitle.is_some() {
            TALL_ROW_HEIGHT
        } else {
            ROW_HEIGHT
        };
        let background = surface_color(
            &tokens,
            Color::TRANSPARENT,
            interactive && hovering(),
            interactive && pressing(),
            self.selected,
        );
        let text_color = if self.enabled {
            tokens.text
        } else {
            tokens.muted
        };
        let icon_color = if self.selected {
            tokens.accent
        } else {
            tokens.muted
        };

        let text_column = rect()
            .width(Size::flex(1.0))
            .main_align(Alignment::Center)
            .spacing(1.0)
            .child(
                label()
                    .text(self.title.clone())
                    .color(text_color)
                    .font_size(FONT_SIZE)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(self.subtitle.as_ref().map(|subtitle| {
                label()
                    .text(subtitle.clone())
                    .color(tokens.muted)
                    .font_size(FONT_SMALL)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }));

        let on_press = self.on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(interactive)
            .a11y_role(if interactive {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::GenericContainer
            })
            .width(Size::fill())
            .height(Size::px(height))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((0.0, 12.0))
            .spacing(10.0)
            .corner_radius(ROW_RADIUS)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .maybe(interactive, |element| {
                element
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |_: Event<PressEventData>| {
                        if let Some(on_press) = &on_press {
                            on_press.call(());
                        }
                        a11y_id.request_focus();
                    })
            })
            .maybe_child(
                self.leading
                    .map(|leading| icons::icon(leading, 18.0, icon_color)),
            )
            .child(text_column)
            .maybe_child(self.trailing.clone())
            .maybe_child(
                (self.selected && !self.navigates)
                    .then(|| icons::icon(Icon::Check, 12.0, tokens.accent)),
            )
            .maybe_child(
                self.navigates
                    .then(|| icons::icon(Icon::GoNext, 12.0, tokens.muted)),
            )
    }
}

/// A row with a switch on the right. The whole row toggles it.
pub fn switch_row(
    title: impl Into<String>,
    on: bool,
    on_toggle: impl Into<EventHandler<bool>>,
) -> Row {
    let on_toggle = on_toggle.into();
    let press = on_toggle.clone();
    row(title)
        .trailing(switch(on, move |next: bool| on_toggle.call(next)))
        .on_press(move |_| press.call(!on))
}

/// A row with a slider on the right. `value` and the callback use 0 to 100.
pub fn slider_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> Row {
    row(title).trailing(slider(value, on_moved))
}

/// A row with a choice on the right: a segmented control for up to three
/// short options, otherwise a dropdown. `options` are `(key, label)` pairs.
pub fn choice_row(
    title: impl Into<String>,
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Row {
    let on_select = on_select.into();
    let short = options.len() <= 3 && options.iter().all(|(_, label)| label.chars().count() <= 12);
    if short {
        return row(title).trailing(segmented(options, selected, on_select));
    }
    let current = options
        .iter()
        .find(|(key, _)| key == selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| selected.to_string());
    let items = options.into_iter().map(|(key, label)| {
        let on_select = on_select.clone();
        let is_selected = key == selected;
        MenuItem::new()
            .selected(is_selected)
            .on_press(move |_| on_select.call(key.clone()))
            .child(label)
    });
    row(title).trailing(Select::new().selected_item(current).children(items))
}

/// A row with a text field on the right; `on_submit` fires on Enter.
pub fn text_row(
    title: impl Into<String>,
    value: State<String>,
    on_submit: impl Into<EventHandler<String>>,
) -> Row {
    let on_submit = on_submit.into();
    row(title).trailing(
        Input::new(value)
            .width(Size::px(260.0))
            .on_submit(move |text: String| on_submit.call(text)),
    )
}
