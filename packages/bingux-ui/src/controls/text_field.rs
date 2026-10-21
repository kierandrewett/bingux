//! Text fields: Freya's `Input` in the shell's clothing (SwiftUI
//! `TextField` and `SecureField`, libadwaita `GtkEntry`).
//!
//! A text field is the one control that keeps an idle 1 px outline, so
//! the reader can see where to type. It sits on `elevated` at radius 8
//! and is 32 px tall.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::use_tokens;

/// A multiline field is this many controls high.
const MULTILINE_ROWS: f32 = 3.0;

/// A single-line (or multi-line) text field bound to a `State<String>`.
#[derive(PartialEq)]
pub struct TextField {
    value: State<String>,
    placeholder: Option<String>,
    width: f32,
    password: bool,
    multiline: bool,
    enabled: bool,
    leading: Option<Icon>,
    clearable: bool,
    invalid: Option<String>,
    revert_to: Option<String>,
    on_submit: EventHandler<String>,
    on_change: Option<EventHandler<String>>,
}

/// `value` is the live draft; `on_submit` fires on Enter with the text.
pub fn text_field(value: State<String>, on_submit: impl Into<EventHandler<String>>) -> TextField {
    TextField {
        value,
        placeholder: None,
        width: 260.0,
        password: false,
        multiline: false,
        enabled: true,
        leading: None,
        clearable: false,
        invalid: None,
        revert_to: None,
        on_submit: on_submit.into(),
        on_change: None,
    }
}

impl TextField {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn maybe_placeholder(mut self, text: Option<String>) -> Self {
        self.placeholder = text;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Mask the text with bullets.
    pub fn password(mut self) -> Self {
        self.password = true;
        self
    }

    /// Grow to about `rows` lines and accept newlines.
    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A muted icon before the text, such as the search glyph.
    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// Show a clear button while the field has text.
    pub fn clearable(mut self) -> Self {
        self.clearable = true;
        self
    }

    /// Outline in the danger colour and show `message` under the field.
    /// `None` clears the error.
    pub fn invalid(mut self, message: Option<String>) -> Self {
        self.invalid = message;
        self
    }

    /// Escape restores this text (the last committed value).
    pub fn revert_to(mut self, text: impl Into<String>) -> Self {
        self.revert_to = Some(text.into());
        self
    }

    /// Called on every edit with the draft text.
    pub fn on_change(mut self, on_change: impl Into<EventHandler<String>>) -> Self {
        self.on_change = Some(on_change.into());
        self
    }
}

impl Component for TextField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut value = self.value;
        let on_submit = self.on_submit.clone();
        let on_change = self.on_change.clone();
        let revert_to = self.revert_to.clone();
        let invalid = self.invalid.is_some();
        let outline = if invalid {
            tokens.danger
        } else {
            tokens.outline
        };
        let focus_outline = if invalid {
            tokens.danger
        } else {
            tokens.accent
        };

        let colors = InputColorsThemePartial {
            background: Some(Preference::Specific(tokens.elevated)),
            focus_background: Some(Preference::Specific(tokens.elevated)),
            border_fill: Some(Preference::Specific(outline)),
            focus_border_fill: Some(Preference::Specific(focus_outline)),
            color: Some(Preference::Specific(tokens.text)),
            placeholder_color: Some(Preference::Specific(tokens.muted.with_a(170))),
        };
        let layout = InputLayoutThemePartial {
            corner_radius: Some(Preference::Specific(CornerRadius::new_all(RADIUS))),
            padding: Some(Preference::Specific(Gaps::new(6.0, 10.0, 6.0, 10.0))),
        };

        let mut input = Input::new(value)
            .width(Size::px(self.width))
            .enabled(self.enabled)
            .theme_colors(colors)
            .theme_layout(layout)
            .on_submit(move |text: String| on_submit.call(text))
            .on_pre_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape)
                    && let Some(text) = revert_to.clone()
                {
                    value.set(text);
                    event.stop_propagation();
                    return false;
                }
                true
            });
        if !self.multiline {
            input = input.height(Size::px(CONTROL_HEIGHT));
        } else {
            input = input
                .multiline(true)
                .height(Size::px(CONTROL_HEIGHT * MULTILINE_ROWS));
        }
        if let Some(placeholder) = &self.placeholder {
            input = input.placeholder(placeholder.clone());
        }
        if self.password {
            input = input.mode(InputMode::new_password());
        }
        if let Some(icon) = self.leading {
            input = input.leading(rect().margin((0.0, S1, 0.0, 0.0)).child(icons::icon(
                icon,
                ICON_MD,
                tokens.muted,
            )));
        }
        if self.clearable && !value.read().is_empty() {
            let on_change = on_change.clone();
            input = input.trailing(
                rect()
                    .width(Size::px(BOX_SM))
                    .height(Size::px(BOX_SM))
                    .corner_radius(RADIUS_LG)
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        value.set(String::new());
                        if let Some(on_change) = &on_change {
                            on_change.call(String::new());
                        }
                    })
                    .child(icons::icon(Icon::EditClear, ICON_SM, tokens.muted)),
            );
        }
        if let Some(on_change) = on_change {
            input = input.on_validate(move |validator: InputValidator| {
                on_change.call(validator.text().clone());
                validator.set_valid(true);
            });
        }

        rect()
            .spacing(S1)
            .font_size(TEXT_MD)
            .child(input)
            .maybe_child(self.invalid.clone().map(|message| {
                label()
                    .text(message)
                    .font_size(TEXT_SM)
                    .color(tokens.danger)
                    .max_lines(2)
            }))
    }
}

/// The rounded search well with a search glyph and a clear button.
pub fn search_field(value: State<String>, on_change: impl Into<EventHandler<String>>) -> TextField {
    text_field(value, |_| {})
        .leading(Icon::Search)
        .placeholder("Search")
        .clearable()
        .on_change(on_change)
}
