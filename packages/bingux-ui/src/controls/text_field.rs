//! Text fields: Freya's `Input` in the shell's clothing (SwiftUI
//! `TextField` and `SecureField`, libadwaita `GtkEntry`).
//!
//! A text field sits on `control` at the control radius and is one control
//! row tall. Like every control, it has no idle outline; focus draws the
//! accent ring, an invalid value the danger one. A password field has a
//! reveal button, and a right
//! click opens Cut, Copy, Paste and Select all. Enter submits; Escape
//! restores the last committed text and leaves the field.

use freya::prelude::*;

use crate::controls::button::icon_button;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::shell::menu::{MenuItem, menu_item, menu_separator};
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{BODY, CAPTION, Ink};

/// A multiline field is this many controls high.
const MULTILINE_ROWS: f32 = 3.0;

/// A single-line (or multi-line) text field bound to a `State<String>`.
#[derive(PartialEq)]
pub struct TextField {
    value: State<String>,
    placeholder: Option<String>,
    width: Dim,
    fill: bool,
    well: bool,
    dense: bool,
    flat: bool,
    password: bool,
    multiline: bool,
    enabled: bool,
    leading: Option<Icon>,
    leading_column: Option<Dim>,
    clearable: bool,
    invalid: Option<String>,
    validates: bool,
    revert_to: Option<String>,
    on_submit: EventHandler<String>,
    on_change: Option<EventHandler<String>>,
    /// Up and Down, with the step Shift (10) or Alt (0.1) gives, for scrubbing.
    on_arrow: Option<EventHandler<f64>>,
    a11y_id: Option<AccessibilityId>,
    autofocus: bool,
}

/// `value` is the live draft; `on_submit` fires on Enter with the text.
pub fn text_field(value: State<String>, on_submit: impl Into<EventHandler<String>>) -> TextField {
    TextField {
        value,
        placeholder: None,
        width: Dim::Field,
        fill: false,
        well: false,
        flat: false,
        dense: false,
        password: false,
        multiline: false,
        enabled: true,
        leading: None,
        leading_column: None,
        clearable: false,
        invalid: None,
        validates: false,
        revert_to: None,
        on_submit: on_submit.into(),
        on_change: None,
        on_arrow: None,
        a11y_id: None,
        autofocus: false,
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

    pub fn width(mut self, width: Dim) -> Self {
        self.width = width;
        self
    }

    /// Fill the container's width, for a filter field above a list.
    pub fn fill_width(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Fill with `elevated`, for a field on the page or sidebar. A field on a
    /// card, dialog or popover keeps the lighter `control` fill.
    pub fn well(mut self) -> Self {
        self.well = true;
        self
    }

    /// A square, borderless field with no fill, `Dim::FlatField` tall, for a
    /// strip docked in a pane such as a find bar.
    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    /// A compact row for property editors and inspectors: the field is
    /// `Dim::DenseControl` tall and its inset is S0_5 by S2.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// Mask the text with bullets. A reveal button shows it while held open.
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

    /// Draws the leading glyph centred in a box `column` wide, at row size,
    /// followed by a gap of S3, so it lines up with a row icon column.
    pub fn leading_column(mut self, column: Dim) -> Self {
        self.leading_column = Some(column);
        self
    }

    /// Show a clear button while the field has text.
    pub fn clearable(mut self) -> Self {
        self.clearable = true;
        self
    }

    /// Outline in the danger colour and show `message` under the field.
    /// `None` clears the error. The message line is always reserved once this
    /// is called, so the row keeps its height when an error comes and goes.
    pub fn invalid(mut self, message: Option<String>) -> Self {
        self.invalid = message;
        self.validates = true;
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

    /// Up and Down arrows call this with the signed step: 1, 10 with Shift,
    /// 0.1 with Alt. The field keeps the keys so the caret does not move.
    pub fn on_arrow(mut self, on_arrow: impl Into<EventHandler<f64>>) -> Self {
        self.on_arrow = Some(on_arrow.into());
        self
    }

    /// Give the field an accessibility id, so a window can focus it.
    pub fn a11y_id(mut self, id: AccessibilityId) -> Self {
        self.a11y_id = Some(id);
        self
    }

    /// Take keyboard focus when the field first appears, such as the first
    /// field of a popup. Only one field should ask for it.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
}

/// One context-menu entry; greyed out when `enabled` is false.
fn edit_item(label: &str, enabled: bool, on_press: impl FnMut(()) + 'static) -> MenuItem {
    let item = menu_item(label, on_press);
    if enabled { item } else { item.disabled() }
}

impl Component for TextField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut value = self.value;
        let on_submit = self.on_submit.clone();
        let on_change = self.on_change.clone();
        let on_arrow = self.on_arrow.clone();
        let revert_to = self.revert_to.clone();
        let multiline = self.multiline;
        let invalid = self.invalid.is_some();
        let own_id = use_a11y();
        let input_id = self.a11y_id.unwrap_or(own_id);
        let focus = use_focus(input_id);
        let mut revealed = use_state(|| false);
        let mut command = use_state::<Option<InputCommand>>(|| None);
        let overlays: Option<Overlays> = try_consume_context::<Overlays>();
        // Freya's Input takes its look as theme values, not a box style, so
        // the tokens are resolved here: the CONTROL recipe's radius and
        // inset, the control or well fill, and the danger edge when invalid.
        let paint = |paint: Paint| Preference::Specific(paint.color(&tokens));
        let fill = if self.flat {
            Paint::Transparent
        } else if self.well {
            Paint::Card
        } else {
            Paint::Control
        };
        // While the field's own menu is open, focus sits on the menu, so the
        // edge keeps the focus colour rather than flickering unfocused.
        let menu_open = overlays.is_some_and(|overlays| overlays.menu_owned_by(input_id));
        let edge = if invalid {
            Paint::Danger
        } else {
            Paint::Accent
        };
        let colors = InputColorsThemePartial {
            background: Some(paint(fill)),
            focus_background: Some(paint(fill)),
            border_fill: Some(paint(if menu_open {
                edge
            } else if invalid {
                Paint::Danger
            } else {
                Paint::Transparent
            })),
            // Keyboard focus is the animated ring, so the field draws no border of
            // its own for it. While the field's own menu is open, the edge keeps
            // the accent colour.
            focus_border_fill: Some(paint(if invalid {
                Paint::Danger
            } else if menu_open {
                Paint::Accent
            } else {
                Paint::Transparent
            })),
            color: Some(paint(Paint::Text)),
            placeholder_color: Some(paint(Paint::Muted)),
        };
        // The inset: S2 above and below and S3 across, or S0_5 and S2 when dense.
        let (pad_vertical, pad_horizontal) = if self.flat {
            (S0, S2)
        } else if self.dense {
            (S0_5, S2)
        } else {
            (S2, S3)
        };
        let radius = if self.flat { 0.0 } else { Radius::Control.px() };
        let layout = InputLayoutThemePartial {
            corner_radius: Some(Preference::Specific(CornerRadius::new_all(radius))),
            padding: Some(Preference::Specific(Gaps::new_symmetric(
                pad_vertical.px(),
                pad_horizontal.px(),
            ))),
        };
        let width = self.width.px();
        let control_height = if self.flat {
            Dim::FlatField.px()
        } else if self.dense {
            Dim::DenseControl.px()
        } else {
            Dim::Control.px()
        };

        let has_text = !value.read().is_empty();
        let masked = self.password && !revealed();
        let mode = if masked {
            InputMode::new_password()
        } else {
            InputMode::Shown
        };

        let mut input = Input::new(value)
            .width(if self.fill {
                Size::fill()
            } else {
                Size::px(width)
            })
            .enabled(use_enabled(self.enabled))
            .theme_colors(colors)
            .theme_layout(layout)
            .mode(mode)
            .command(command)
            .a11y_id(input_id)
            .on_submit(move |text: String| on_submit.call(text))
            .auto_focus(self.autofocus)
            .on_pre_key_down(move |event: Event<KeyboardEventData>| {
                match &event.key {
                    Key::Named(NamedKey::Escape) => {
                        if let Some(text) = revert_to.clone() {
                            value.set(text);
                        }
                        event.stop_propagation();
                        input_id.request_unfocus();
                        false
                    }
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown)
                        if on_arrow.is_some() =>
                    {
                        let sign = if matches!(event.key, Key::Named(NamedKey::ArrowUp)) {
                            1.0
                        } else {
                            -1.0
                        };
                        let scale = if event.modifiers.contains(Modifiers::SHIFT) {
                            10.0
                        } else if event.modifiers.contains(Modifiers::ALT) {
                            0.1
                        } else {
                            1.0
                        };
                        if let Some(on_arrow) = &on_arrow {
                            on_arrow.call(sign * scale);
                        }
                        event.stop_propagation();
                        event.prevent_default();
                        false
                    }
                    // A single-line Enter bubbles up, so a popup can act on it.
                    Key::Named(NamedKey::Enter) if !multiline => true,
                    // Tab moves focus, as Freya's own field does.
                    Key::Named(NamedKey::Tab) => false,
                    _ => {
                        // Keys the field edits stay here: a multiline Enter
                        // must not trigger a popup's primary action.
                        event.stop_propagation();
                        event.prevent_default();
                        true
                    }
                }
            });
        if !self.multiline {
            input = input.height(Size::px(control_height));
        } else {
            input = input
                .multiline(true)
                .height(Size::px(control_height * MULTILINE_ROWS));
        }
        if let Some(placeholder) = &self.placeholder {
            input = input.placeholder(placeholder.clone());
        }
        if let Some(icon) = self.leading {
            // With a column the glyph is a fixed-width box at row size, with the
            // rows' icon-to-text gap, so the glyph and the text line up with the
            // rows that share the column. Without one the glyph sits at control
            // size, tucked against the text.
            let (glyph_size, after) = match self.leading_column {
                Some(_) => (IconSize::Row, S2),
                None => (IconSize::Control, S1),
            };
            let mut glyph = rect()
                .height(Size::fill())
                .tw(&tokens, &[mr(after), flex_row(), center()]);
            if let Some(column) = self.leading_column {
                glyph = glyph.tw(&tokens, &[w(column)]);
            }
            input = input.leading(glyph.child(icons::icon(
                icon,
                glyph_size,
                Paint::Muted.color(&tokens),
            )));
        }
        let clear = self.clearable && has_text;
        if clear || self.password {
            let on_change = on_change.clone();
            let mut trailing = rect().tw(&tokens, &[flex_row(), center(), gap(S1)]);
            if clear {
                trailing = trailing.child(
                    rect()
                        .tw(
                            &tokens,
                            &[w(Dim::Box5), h(Dim::Box5), rounded(Radius::Pill), center()],
                        )
                        .on_pointer_down(move |event: Event<PointerEventData>| {
                            let left = matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Left));
                            if !left {
                                return;
                            }
                            event.stop_propagation();
                            value.set(String::new());
                            if let Some(on_change) = &on_change {
                                on_change.call(String::new());
                            }
                        })
                        .child(icons::icon(
                            Icon::EditClear,
                            IconSize::Mark,
                            Paint::Muted.color(&tokens),
                        )),
                );
            }
            if self.password {
                let (icon, tip) = if revealed() {
                    (Icon::ViewConceal, "Hide password")
                } else {
                    (Icon::ViewReveal, "Show password")
                };
                trailing = trailing.child(
                    icon_button(icon, move |_| revealed.set(!revealed()))
                        .size(Dim::Box5)
                        .tooltip(tip),
                );
            }
            input = input.trailing(trailing);
        }
        if let Some(on_change) = on_change {
            input = input.on_validate(move |validator: InputValidator| {
                on_change.call(validator.text().clone());
                validator.set_valid(true);
            });
        }

        // Right click: the edit commands, greyed out when they do not apply.
        // A password is never copied out, so Cut and Copy stay off for it.
        let copyable = has_text && !self.password;
        let items = vec![
            edit_item("Cut", copyable, move |_| {
                command.set(Some(InputCommand::Cut))
            }),
            edit_item("Copy", copyable, move |_| {
                command.set(Some(InputCommand::Copy));
            }),
            edit_item("Paste", true, move |_| {
                command.set(Some(InputCommand::Paste));
            }),
            menu_separator(),
            edit_item("Select all", has_text, move |_| {
                command.set(Some(InputCommand::SelectAll));
            }),
        ];
        let field = rect()
            .width(if self.fill {
                Size::fill()
            } else {
                Size::px(width)
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right));
                if !right {
                    return;
                }
                event.stop_propagation();
                if let Some(overlays) = overlays {
                    // The field takes focus so the edit commands act on it,
                    // and the menu hands focus back when it closes.
                    input_id.request_focus();
                    let at = event.global_location();
                    overlays.show_menu_from(
                        Some(input_id),
                        (at.x as f32, at.y as f32),
                        items.clone(),
                        false,
                    );
                }
            })
            .child(input)
            .focusable(focus(), Radius::Control)
            .maybe(self.fill, |wrapper| wrapper.tw(&tokens, &[w_flex()]));

        rect()
            .tw(&tokens, &[flex_col(), gap(S1), font(BODY)])
            .child(field)
            .maybe(self.validates, |column| {
                column.child(
                    rect()
                        .tw(&tokens, &[w_full(), min_h(Dim::CaptionLine)])
                        .maybe_child(self.invalid.clone().map(|message| {
                            CAPTION
                                .in_ink(Ink::Danger)
                                .paragraph(&tokens, message)
                                .max_lines(2)
                        })),
                )
            })
    }
}

/// The search well is the same field with a search glyph and a clear button.
pub fn search_field(value: State<String>, on_change: impl Into<EventHandler<String>>) -> TextField {
    text_field(value, |_| {})
        .leading(Icon::Search)
        .placeholder("Search")
        .clearable()
        .on_change(on_change)
}
