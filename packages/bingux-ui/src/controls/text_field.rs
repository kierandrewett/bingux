//! Text fields: Freya's `Input` in the shell's clothing (SwiftUI
//! `TextField` and `SecureField`, libadwaita `GtkEntry`).
//!
//! A text field sits on `control` at the control radius and is one control
//! row tall. Like every control, it has no idle outline; focus draws the
//! accent ring, an invalid value the danger one. A password field has a
//! reveal button, and a right click opens Cut, Copy, Paste and Select all.
//! A touch gets the edit bar instead, over the caret or selection (see
//! `text_edit`). Enter submits; Escape restores the last committed text and
//! leaves the field.

use std::time::Instant;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::icon_button;
use crate::controls::press::{is_touch, point_of, use_press};
use crate::controls::text_edit::{self, DOUBLE_TAP};
use crate::icons::{self, Icon};
use crate::focus::owner::use_visual_focus;
use crate::focus::{press_claimed, use_focus_core};
use crate::motion::Focusable;
use crate::shell::menu::{MenuItem, menu_item, menu_separator};
use crate::shell::overlays::Overlays;
use crate::shell::panel::{Panel, PanelRect, PanelTrigger, text_bar_rect};
use crate::style::*;
use crate::surface::{surface_color, use_enabled, use_tokens};
use crate::typography::{BODY, CAPTION, Ink, mono_family};

/// What a password field shows for each character: a bullet, as the system's
/// password fields do. Freya's own default is an asterisk.
const MASK: char = '\u{2022}';

/// A multiline field is this many controls high.
const MULTILINE_ROWS: f32 = 3.0;

/// A tap this close to the previous one is on the same spot: a second tap there shows the edit bar.
const SAME_SPOT: f32 = 16.0;

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
    /// How many control heights a multiline field is tall.
    lines: f32,
    monospace: bool,
    /// Draw the danger outline without the field's own message line, for a
    /// form that shows the message itself.
    danger_outline: bool,
    /// A muted unit inside the field's right end, such as `bytes`.
    suffix: Option<String>,
    enabled: bool,
    /// The text can be selected and copied but not edited.
    read_only: bool,
    leading: Option<Icon>,
    /// A search field: the search role, and Escape clears the text first.
    search: bool,
    /// A removable token before the text; see [`TextField::leading_chip`].
    leading_chip: Option<(String, String, EventHandler<()>)>,
    clearable: bool,
    /// One icon button at the field's trailing edge; see [`TextField::trailing_action`].
    action: Option<(Icon, String, EventHandler<()>)>,
    invalid: Option<String>,
    validates: bool,
    revert_to: Option<String>,
    on_submit: EventHandler<String>,
    on_change: Option<EventHandler<String>>,
    /// Up and Down, with the step Shift (10) or Alt (0.1) gives, for scrubbing.
    on_arrow: Option<EventHandler<f64>>,
    a11y_id: Option<AccessibilityId>,
    autofocus: bool,
    /// Select all the text when the field first appears.
    select_all: bool,
    /// Select this range of characters when the field first appears.
    select_range: Option<(usize, usize)>,
    /// Called when Escape is pressed, before the field gives up focus.
    on_escape: Option<EventHandler<()>>,
    /// Told which modifiers were held, just before Enter submits.
    on_enter_modifiers: Option<EventHandler<Modifiers>>,
    /// Apps' own entries, appended to the full edit menu after the general ones.
    edit_extra: Vec<MenuItem>,
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
        lines: MULTILINE_ROWS,
        monospace: false,
        danger_outline: false,
        suffix: None,
        enabled: true,
        read_only: false,
        leading: None,
        search: false,
        leading_chip: None,
        clearable: false,
        action: None,
        invalid: None,
        validates: false,
        revert_to: None,
        on_submit: on_submit.into(),
        on_change: None,
        on_arrow: None,
        a11y_id: None,
        autofocus: false,
        select_all: false,
        select_range: None,
        on_escape: None,
        on_enter_modifiers: None,
        edit_extra: Vec::new(),
    }
}

/// The icon button a field carries at its trailing edge: [`Dim::InlineButton`]
/// with the corner of the field less [`FIELD_INSET`]. A bar placed where a
/// field will replace it draws its own button with this, inside a box with
/// `pr(FIELD_INSET)`, so the button stays where it is when the field comes.
pub(crate) fn field_action(
    icon: Icon,
    on_press: impl Into<EventHandler<()>>,
) -> crate::controls::button::IconButton {
    icon_button(icon, on_press)
        .size(Dim::InlineButton)
        .radius(Radius::Control.nested(FIELD_INSET.px()))
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

    /// How many lines tall a multiline field is (default 3). Has no effect
    /// on a single-line field.
    pub fn lines(mut self, lines: usize) -> Self {
        self.lines = lines.max(1) as f32;
        self
    }

    /// Draw the text in the monospace face, for keys, addresses and code.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    /// Outline the field in the danger colour without showing a message
    /// under it, for a form that draws the message itself (see `form_field`).
    pub fn danger_outline(mut self, outlined: bool) -> Self {
        self.danger_outline = outlined;
        self
    }

    /// A muted unit inside the right end of the field, such as `bytes` or
    /// `s`, so fields with and without one keep the same width.
    pub fn suffix(mut self, text: impl Into<String>) -> Self {
        self.suffix = Some(text.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A field whose text can be selected and copied but not changed: it
    /// keeps its focus, caret and selection, its right-click menu offers
    /// Copy and Select all, and Cut and Paste are greyed out. Use it for a
    /// value the person may copy, such as an address or a key.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// A muted icon before the text, such as a folder glyph. It sits in a
    /// square slot as wide as the field is tall (so the field's own height
    /// decides the slot at every density), centred in it on both axes; the
    /// text starts [`S1`] after the slot.
    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// The field is a search field: the accessibility role is search and
    /// Escape clears the text before it does anything else. Reached through
    /// [`SearchField`](crate::controls::SearchField), the only search control.
    pub(crate) fn search(mut self) -> Self {
        self.search = true;
        self
    }

    /// A removable token after the leading glyph and before the text, such as
    /// the folder a search is limited to (`in Downloads`). `label` is drawn
    /// in the token and `remove_label` names its close button for the
    /// pointer's tooltip and for assistive technology; `on_remove` runs when
    /// the button is pressed. The token sits inside the field, so adding or
    /// removing it never changes the field's own size.
    pub fn leading_chip(
        mut self,
        label: impl Into<String>,
        remove_label: impl Into<String>,
        on_remove: impl Into<EventHandler<()>>,
    ) -> Self {
        self.leading_chip = Some((label.into(), remove_label.into(), on_remove.into()));
        self
    }

    /// Show a clear button while the field has text.
    pub fn clearable(mut self) -> Self {
        self.clearable = true;
        self
    }

    /// An icon button at the field's trailing edge, in the place and size of
    /// the clear and reveal buttons ([`field_action`]), always shown. A bar
    /// that turns into this field puts its own button in the same place, so
    /// the swap moves nothing.
    pub fn trailing_action(
        mut self,
        icon: Icon,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.action = Some((icon, label.into(), on_press.into()));
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

    /// Select the whole text when the field first appears, so typing replaces
    /// it and Ctrl+C copies it, as a browser's address bar does when it is
    /// focused. Pair it with [`autofocus`](Self::autofocus).
    pub fn select_all_on_open(mut self) -> Self {
        self.select_all = true;
        self
    }

    /// Select the characters from `start` to `end` (positions in characters)
    /// when the field first appears, such as a file name without its
    /// extension. Pair it with [`autofocus`](Self::autofocus).
    pub fn select_range(mut self, start: usize, end: usize) -> Self {
        self.select_range = Some((start, end));
        self
    }

    /// Escape is the owner's: `on_escape` runs, and the field neither restores
    /// its `revert_to` text nor gives up focus. The owner closes what the
    /// field is in, or keeps typing going, as it decides. A field with no
    /// `on_escape` reverts and gives up focus.
    pub fn on_escape(mut self, on_escape: impl Into<EventHandler<()>>) -> Self {
        self.on_escape = Some(on_escape.into());
        self
    }

    /// Told which modifiers were held (Ctrl, Shift, Alt) each time Enter is
    /// pressed, just before the field submits, so the submit handler can tell
    /// Enter from Ctrl+Enter or Shift+Enter. A synthetic key press carries
    /// its modifiers here even where the window system's live state does not.
    pub fn on_enter_modifiers(mut self, handler: impl Into<EventHandler<Modifiers>>) -> Self {
        self.on_enter_modifiers = Some(handler.into());
        self
    }

    /// Append app-specific entries to the full edit menu, after Cut, Copy,
    /// Paste and Select all. The touch bar's chevron opens that menu.
    pub fn edit_menu_extra(mut self, items: Vec<MenuItem>) -> Self {
        self.edit_extra = items;
        self
    }
}

/// Whether the clipboard holds text, so the touch bar offers Paste.
fn clipboard_has_text() -> bool {
    freya::clipboard::Clipboard::get().is_ok_and(|text| !text.is_empty())
}

/// One context-menu entry; greyed out when `enabled` is false.
fn edit_item(label: &str, enabled: bool, on_press: impl FnMut(()) + 'static) -> MenuItem {
    let item = menu_item(label, on_press);
    if enabled { item } else { item.disabled() }
}

impl Component for TextField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // A form line that is too narrow for the field's width asks it to
        // fill what it has instead.
        let fills = self.fill || crate::controls::adaptive::use_line_fill();
        let mut value = self.value;
        let on_submit = self.on_submit.clone();
        let on_change = self.on_change.clone();
        let on_arrow = self.on_arrow.clone();
        let revert_to = self.revert_to.clone();
        let on_escape = self.on_escape.clone();
        let escape_change = self.on_change.clone();
        let search = self.search;
        let on_enter_modifiers = self.on_enter_modifiers.clone();
        let multiline = self.multiline;
        let invalid = self.invalid.is_some() || self.danger_outline;
        let own_id = use_a11y();
        let input_id = self.a11y_id.unwrap_or(own_id);
        let visual = use_visual_focus(input_id);
        let enabled = use_enabled(self.enabled);
        let mut revealed = use_state(|| false);
        // (hover is the input's own: see `hover_background` below)
        let mut command = use_state::<Option<InputCommand>>(|| None);
        let touch = use_press();
        let probe = use_state(InputProbe::default);
        let select_all = self.select_all;
        use_hook(move || {
            if select_all {
                command.set(Some(InputCommand::SelectAll));
            }
        });
        let select_range = self.select_range;
        let mut request = use_state::<Option<InputRequest>>(|| {
            select_range.map(|(start, end)| InputRequest::SelectRange { start, end })
        });
        let mut bar_open = use_state(|| false);
        let bar_size = use_state::<Option<(f32, f32)>>(|| None);
        let mut field_origin = use_state(|| (0.0_f32, 0.0_f32));
        // The field's window rect, for touches: the input swallows the pointer
        // press, so the field hears a touch by its global down instead.
        let mut field_area = use_state::<Option<(f32, f32, f32, f32)>>(|| None);
        let mut pressed_at = use_state(|| (0.0_f32, 0.0_f32));
        let mut last_tap = use_state::<Option<(Instant, (f32, f32))>>(|| None);
        let overlays: Option<Overlays> = try_consume_context::<Overlays>();
        let focus_core = use_focus_core();
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
        // Focus and the invalid state are both the ring the field is wrapped in
        // (see `Focusable`), so the field draws no edge of its own, and the
        // look while its context menu is open comes from `visual`, the one
        // source of truth for "is this focused".
        // A field under the pointer takes the control's hover layer, as every
        // other control does; a focused one keeps its focus fill under the ring.
        let rest = if self.flat {
            Paint::Transparent.color(&tokens)
        } else {
            fill.color(&tokens)
        };
        let hover = if self.flat || self.read_only {
            rest
        } else {
            surface_color(&tokens, rest, true, false, false)
        };
        let colors = InputColorsThemePartial {
            background: Some(Preference::Specific(rest)),
            focus_background: Some(paint(fill)),
            border_fill: Some(paint(Paint::Transparent)),
            focus_border_fill: Some(paint(Paint::Transparent)),
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
        // After a glyph slot the slot is the field's inset, so the text
        // starts S1 past it; a chip carries its own margin.
        let pad_left = if self.leading.is_some() && self.leading_chip.is_none() {
            S1
        } else {
            pad_horizontal
        };
        let radius = if self.flat { 0.0 } else { Radius::Control.px() };
        let layout = InputLayoutThemePartial {
            corner_radius: Some(Preference::Specific(CornerRadius::new_all(radius))),
            padding: Some(Preference::Specific(Gaps::new(
                pad_vertical.px(),
                pad_horizontal.px(),
                pad_vertical.px(),
                pad_left.px(),
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
            InputMode::Hidden(MASK)
        } else {
            InputMode::Shown
        };

        // Right click and the Menu key: the edit commands, greyed out when
        // they do not apply. A password is never copied out, so Cut and Copy
        // stay off for it, and a read-only field cannot be cut into or pasted
        // over.
        let changes_text = !self.read_only;
        let copyable = has_text && !self.password;
        let edit_menu = vec![
            edit_item("Cut", copyable && changes_text, move |_| {
                command.set(Some(InputCommand::Cut))
            }),
            edit_item("Copy", copyable, move |_| {
                command.set(Some(InputCommand::Copy));
            }),
            edit_item("Paste", changes_text, move |_| {
                command.set(Some(InputCommand::Paste));
            }),
            menu_separator(),
            edit_item("Select all", has_text, move |_| {
                command.set(Some(InputCommand::SelectAll));
            }),
        ];
        // The Menu key and Shift+F10 open the same menu against the caret, or
        // against the field when it shows none. The menu is highlighted from
        // the keyboard, and the field stays the focus owner behind it.
        let key_menu = {
            let edit_menu = edit_menu.clone();
            move || {
                let Some(overlays) = overlays else {
                    return;
                };
                let Some((x0, y0, x1, y1)) = *field_area.peek() else {
                    return;
                };
                let field = PanelRect {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                };
                let trigger = match text_edit::anchor(&probe.peek()) {
                    Some(caret) => PanelTrigger::Text { field, caret },
                    None => PanelTrigger::Control(field),
                };
                input_id.request_focus();
                overlays.show_menu_from(Some(input_id), trigger, edit_menu.clone(), true);
            }
        };

        let mut input = Input::new(value)
            .width(if fills {
                Size::fill()
            } else {
                Size::px(width)
            })
            .enabled(enabled)
            .theme_colors(colors)
            .theme_layout(layout)
            .hover_background(hover)
            .mode(mode)
            .focus_within(visual.within_owner)
            .read_only(self.read_only)
            .command(command)
            .probe(probe)
            .request(request)
            .a11y_id(input_id)
            .on_submit(move |text: String| on_submit.call(text))
            .auto_focus(self.autofocus)
            .on_pre_key_down(move |event: Event<KeyboardEventData>| {
                match &event.key {
                    Key::Named(NamedKey::Escape) => {
                        if let Some(on_escape) = &on_escape {
                            on_escape.call(());
                            event.stop_propagation();
                            bar_open.set(false);
                            return false;
                        }
                        // A search field with text clears it and stays put; the
                        // next Escape, with the field empty, leaves it.
                        if search && !value.peek().is_empty() {
                            value.set(String::new());
                            if let Some(on_change) = &escape_change {
                                on_change.call(String::new());
                            }
                            event.stop_propagation();
                            return false;
                        }
                        if let Some(text) = revert_to.clone() {
                            value.set(text);
                        }
                        event.stop_propagation();
                        bar_open.set(false);
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
                    Key::Named(NamedKey::Enter) if !multiline => {
                        if let Some(on_enter) = &on_enter_modifiers {
                            on_enter.call(event.modifiers);
                        }
                        true
                    }
                    // The Menu key and Shift+F10 open the edit menu.
                    Key::Named(NamedKey::ContextMenu) => {
                        event.stop_propagation();
                        event.prevent_default();
                        key_menu();
                        false
                    }
                    Key::Named(NamedKey::F10) if event.modifiers.contains(Modifiers::SHIFT) => {
                        event.stop_propagation();
                        event.prevent_default();
                        key_menu();
                        false
                    }
                    // Tab moves focus, as Freya's own field does.
                    Key::Named(NamedKey::Tab) => false,
                    // A Ctrl or Meta chord the field has no edit for (Ctrl+F,
                    // Ctrl+L, Ctrl+Shift+F...) belongs to the window: it must
                    // not be typed as text, and it keeps bubbling. The field's
                    // own are select all, copy, cut, paste, undo and redo.
                    // Alt with Ctrl is how some layouts type a letter (AltGr),
                    // so that stays text.
                    Key::Character(text)
                        if event.modifiers.intersects(Modifiers::ctrl_or_meta())
                            && !event.modifiers.contains(Modifiers::ALT)
                            && !matches!(text.to_lowercase().as_str(), "a" | "c" | "x" | "v" | "z" | "y") =>
                    {
                        false
                    }
                    _ => {
                        // Keys the field edits stay here: a multiline Enter
                        // must not trigger a popup's primary action.
                        event.stop_propagation();
                        event.prevent_default();
                        true
                    }
                }
            });
        if self.search {
            input = input.role(AccessibilityRole::SearchInput);
        }
        if !self.multiline {
            input = input.height(Size::px(control_height));
        } else {
            input = input
                .multiline(true)
                .height(Size::px(control_height * self.lines));
        }
        if self.monospace {
            input = input.font_family(mono_family());
        }
        if let Some(placeholder) = &self.placeholder {
            input = input.placeholder(placeholder.clone());
        }
        let mut leading: Option<Rect> = None;
        if let Some(icon) = self.leading {
            // The slot is as wide as the field is tall and as tall as the
            // field, the glyph centred in it. The field's text cursor belongs
            // to the text: a glyph or button beside it shows the normal pointer.
            leading = Some(
                rect()
                    .component("IconSlot")
                    .width(Size::px(control_height))
                    .height(Size::fill())
                    .cursor(CursorIcon::default())
                    .tw(&tokens, &[flex_row(), center()])
                    .child(icons::icon(
                        icon,
                        IconSize::Control,
                        Paint::Muted.color(&tokens),
                    )),
            );
        }
        if let Some((label, remove_label, on_remove)) = &self.leading_chip {
            let on_remove = on_remove.clone();
            let chip = rect()
                .component("TextFieldChip")
                .a11y_alt(label.clone())
                .cursor(CursorIcon::default())
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        gap(S1),
                        pl(S2),
                        rounded(Radius::Mark),
                        bg(Paint::AccentSoft),
                        mr(S2),
                    ],
                )
                .child(
                    CAPTION
                        .in_ink(Ink::Text)
                        .label(&tokens, label.clone())
                        .max_lines(1),
                )
                .child(
                    icon_button(Icon::WindowClose, move |_| on_remove.call(()))
                        .size(Dim::InlineButton)
                        .tooltip(remove_label),
                );
            let bare = leading.is_none();
            leading = Some(
                rect()
                    .height(Size::fill())
                    .tw(&tokens, &[flex_row(), items(Align::Center)])
                    .maybe(bare, |row| row.tw(&tokens, &[pl(S2)]))
                    .maybe_child(leading.take())
                    .child(chip),
            );
        }
        if let Some(leading) = leading {
            input = input.leading(leading);
        }
        // A disabled field is inert, so it offers no clear button either.
        let clear = self.clearable && has_text && enabled;
        if clear || self.password || self.suffix.is_some() || self.action.is_some() {
            let on_change = on_change.clone();
            // Every action in a field is the standard icon button, centred in
            // a square slot at the trailing edge as wide as the field is
            // tall: the inset is half of what the button leaves of the
            // height, so it is the same on the cross axis and from the end.
            // Its corner is the field's less the inset.
            let small = self.flat || self.dense;
            let (button, nested) = if small {
                (Dim::Box5, Radius::Mark)
            } else {
                (Dim::InlineButton, Radius::Control.nested(FIELD_INSET.px()))
            };
            let inset = ((control_height - button.px()) / 2.0).max(0.0);
            let has_button = clear || self.password || self.action.is_some();
            let mut trailing = rect()
                .cursor(CursorIcon::default())
                .tw(&tokens, &[flex_row(), center(), gap(S1)])
                // ui-lint: allow(the inset is derived from the measured control and button heights)
                .padding((0.0, if has_button { inset } else { pad_horizontal.px() }, 0.0, 0.0));
            if let Some(suffix) = &self.suffix {
                trailing = trailing.child(
                    BODY.in_ink(Ink::Muted)
                        .label(&tokens, suffix.clone())
                        .max_lines(1),
                );
            }
            if clear {
                trailing = trailing.child(
                    icon_button(Icon::EditClear, move |_| {
                        value.set(String::new());
                        if let Some(on_change) = &on_change {
                            on_change.call(String::new());
                        }
                    })
                    .size(button)
                    .radius(nested)
                    .refocus(input_id)
                    .tooltip("Clear"),
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
                        .size(button)
                        .radius(nested)
                        .refocus(input_id)
                        .tooltip(tip),
                );
            }
            if let Some((icon, label, on_press)) = &self.action {
                trailing = trailing.child(
                    icon_button(*icon, on_press.clone())
                        .size(button)
                        .radius(nested)
                        .tooltip(label),
                );
            }
            input = input.trailing(trailing);
        }
        // An edit that changes the text hides the touch bar; Select all, which
        // validates unchanged text, keeps it. The app's on_change sees every edit.
        input = input.on_validate(move |validator: InputValidator| {
            if *validator.text() != *value.peek() {
                bar_open.set(false);
            }
            if let Some(on_change) = &on_change {
                on_change.call(validator.text().clone());
            }
            validator.set_valid(true);
        });

        // Touch: the bar's entries follow the selection, and its chevron opens the
        // full menu. Mouse keeps the right-click menu on the field.
        let seen = *probe.read();
        let show_bar = bar_open() && seen.focused;
        let (handle_start, handle_end) = match seen.selection {
            Some((start, end)) if show_bar => (Some(start), Some(end)),
            _ => (None, None),
        };
        // The bar sits against the caret or selection, inside the field. With no
        // caret the field is not focused, so there is no bar to place.
        let text_trigger = match (*field_area.read(), text_edit::anchor(&seen)) {
            (Some((x0, y0, x1, y1)), Some(caret)) => Some(PanelTrigger::Text {
                field: PanelRect {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                },
                caret,
            }),
            _ => None,
        };
        // The bar's window rectangle once measured. A press inside it is the bar's.
        let window = *Platform::get().current_window().root_size.read();
        let direction = crate::controls::direction::use_direction();
        let bar_hit = text_trigger.zip(*bar_size.read()).map(|(trigger, size)| {
            text_bar_rect(trigger.anchor(), size, (window.width, window.height), direction)
        });
        // A touch on the bar or a handle is not a tap outside the field, which
        // would take its focus.
        let companions: Vec<Area> = bar_hit
            .into_iter()
            .chain(
                [handle_start, handle_end]
                    .into_iter()
                    .flatten()
                    .map(text_edit::handle_rect),
            )
            .filter(|_| show_bar)
            .map(text_edit::area)
            .collect();
        // The release of a press the menu's shield took (an outside click that
        // dismisses this field's menu) is not a click outside the field: the
        // field keeps its focus. The shield claims that press with the focus
        // core, or the menu is still up for a quick click.
        let press_core = focus_core.clone();
        input = input.keep_focus_in(companions).keep_focus_when(move |_: ()| {
            press_claimed(&press_core)
                || overlays.is_some_and(|overlays| overlays.menu_owned_by(input_id))
        });
        let cut = edit_item("Cut", copyable && changes_text, move |_| {
            command.set(Some(InputCommand::Cut));
            bar_open.set(false);
        });
        let copy = edit_item("Copy", copyable, move |_| {
            command.set(Some(InputCommand::Copy));
            bar_open.set(false);
        });
        let paste = edit_item("Paste", changes_text, move |_| {
            command.set(Some(InputCommand::Paste));
            bar_open.set(false);
        });
        let select = edit_item("Select", has_text, move |_| {
            request.set(Some(InputRequest::SelectWordAtCursor));
        });
        let select_all = edit_item("Select all", has_text, move |_| {
            command.set(Some(InputCommand::SelectAll));
        });
        // Paste is offered only while the clipboard holds text. A selection shows
        // Cut, Copy and Paste; a bare caret shows Paste, Select and Select all.
        let can_paste = show_bar && clipboard_has_text();
        let bar_entries: Vec<MenuItem> = match seen.selection {
            Some(_) => [Some(cut), Some(copy), can_paste.then_some(paste)],
            None => [can_paste.then_some(paste), Some(select), Some(select_all)],
        }
        .into_iter()
        .flatten()
        .collect();
        let mut full_menu = edit_menu.clone();
        full_menu.extend(self.edit_extra.clone());
        let on_more = EventHandler::new(move |_: ()| {
            bar_open.set(false);
            if let (Some(overlays), Some(trigger)) = (overlays, text_trigger) {
                input_id.request_focus();
                overlays.show_menu_from(Some(input_id), trigger, full_menu.clone(), false);
            }
        });
        let field = rect()
            .width(if fills {
                Size::fill()
            } else {
                Size::px(width)
            })
            // The field's box is the ring's face: a flat field is a line with
            // no radius, so its ring is square.
            .tw(
                &tokens,
                &[rounded_on(
                    Radius::Control,
                    if self.flat { Corners::None } else { Corners::All },
                    0.0,
                )],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                let area = event.area;
                field_area.set(Some((area.min_x(), area.min_y(), area.max_x(), area.max_y())));
                let next = (event.area.min_x(), event.area.min_y());
                let moved = *field_origin.read() != next;
                if moved {
                    field_origin.set(next);
                    // Scrolling or a relayout moves the caret away from the bar.
                    bar_open.set(false);
                }
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right));
                // A disabled field is inert: no menu, and it takes no focus.
                if !right || !enabled {
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
                        PanelTrigger::Pointer((at.x as f32, at.y as f32)),
                        edit_menu.clone(),
                        false,
                    );
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| touch.moved(&event))
            .on_global_pointer_down(move |event: Event<PointerEventData>| {
                // The input swallows a press that lands on it, so a touch is heard
                // here, by its global down, when it lands inside the field.
                let at = point_of(&event);
                let inside = (*field_area.read()).is_some_and(|(x0, y0, x1, y1)| {
                    at.0 >= x0 && at.0 <= x1 && at.1 >= y0 && at.1 <= y1
                });
                // A press outside the field and its bar hides the bar. A press on the
                // bar is left to reach its entry.
                let on_bar = bar_hit.is_some_and(|rect| rect.contains(at));
                if !inside && !on_bar && *bar_open.read() {
                    bar_open.set(false);
                }
                if is_touch(&event) && inside {
                    pressed_at.set(at);
                    touch.begin(
                        &event,
                        Some(EventHandler::new(move |at: (f32, f32)| {
                            request.set(Some(InputRequest::SelectWordAt { x: at.0, y: at.1 }));
                            bar_open.set(true);
                        })),
                    );
                }
            })
            .on_global_pointer_up(move |_: Event<PointerEventData>| {
                // A touch tap. A double tap selects the word and shows the bar; a
                // second tap on the same spot shows it; a tap on an empty field too.
                if !touch.lift() {
                    return;
                }
                let at = *pressed_at.read();
                let now = Instant::now();
                let previous = *last_tap.read();
                last_tap.set(Some((now, at)));
                let near = previous
                    .is_some_and(|(_, spot)| (spot.0 - at.0).hypot(spot.1 - at.1) <= SAME_SPOT);
                let double = near
                    && previous.is_some_and(|(then, _)| now.duration_since(then) <= DOUBLE_TAP);
                if double {
                    request.set(Some(InputRequest::SelectWordAt { x: at.0, y: at.1 }));
                    bar_open.set(true);
                } else if near || !has_text {
                    bar_open.set(true);
                } else {
                    bar_open.set(false);
                }
            })
            .child(input)
            .maybe_child(handle_start.map(|at| text_edit::handle(&tokens, at, true, request)))
            .maybe_child(handle_end.map(|at| text_edit::handle(&tokens, at, false, request)))
            .maybe_child(text_trigger.map(|trigger| {
                Panel::bar(
                    show_bar,
                    trigger,
                    bar_size,
                    text_edit::bar(&tokens, bar_entries, on_more),
                )
            }))
            .focusable_invalid(
                // A text field shows its ring for any focus, pointer or key, and
                // while its own menu is open (the owner keeps the focus look).
                if visual.is_focused() {
                    Focus::Keyboard
                } else {
                    Focus::Not
                },
                invalid,
            )
            .maybe(fills, |wrapper| wrapper.tw(&tokens, &[w_flex()]));

        rect()
            .component("TextField")
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


#[cfg(all(test, feature = "testing"))]
#[path = "text_field_focus_tests.rs"]
mod focus_tests;
