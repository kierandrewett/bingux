//! Rows: the building block of every preference list.
//!
//! Mirrors `ControlRow.qml`: a leading icon, a title, an optional subtitle
//! and trailing content. Convenience constructors pair a row with a
//! switch, slider, choice or text field.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use accesskit::Toggled;
use freya::prelude::*;

use crate::app_icon::app_icon;
use crate::component::{ComponentName, Prop, Published};
use crate::controls::dropdown::dropdown;
use crate::controls::press::{Press, use_press};
use crate::controls::radio::{RadioMark, next_enabled};
use crate::controls::segmented::segmented;
use crate::controls::slider::slider;
use crate::controls::switch::{Switch, switch};
use crate::controls::{ActionButton, Checkbox, Dropdown, IconButton, RowButton, Segmented, SplitButton, Stepper};
use crate::controls::adaptive::{
    ControlFacts, LabelFacts, LineFit, LineMode, choose_filled_line, choose_line, label_cell_width,
};
use crate::controls::text_field::{TextField, text_field};
use crate::group::{SlotFilter, card_rows, use_row_match, use_row_slot};
use crate::icons::{self, Icon};
use crate::focus::use_focus_core;
use crate::motion::{
    MOVE_MS, RingSize, use_ring_size, use_tween, wrap_focusable_with,
};
use crate::search_highlight::{self, MatchSink};
use crate::shell::menu::{MenuItem, menu_item};
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::{provide_enabled, use_tokens};
use crate::typography::{BADGE, BODY, CAPTION, Ink, VALUE};

/// Whether the row around a value takes presses. Set by `Row`, read by
/// `ValueText` and by any selectable text through [`use_in_pressable_row`].
#[derive(Clone, Copy, PartialEq)]
struct RowPressable(State<bool>);
/// Whether the trailing control of the row around it is a button, dropdown or
/// segmented control that fills the row (see [`Dim::RowButton`]).
#[derive(Clone, Copy, PartialEq)]
struct RowFit(State<bool>);

/// Whether this control is the trailing button of a row. It then is
/// [`Dim::RowButton`] tall, so [`ROW_INSET`] is left above and below it, and
/// its radius is the group's less that inset ([`Radius::Inner`]). Every
/// control that can trail a row calls this once.
pub(crate) fn use_row_fitted() -> bool {
    use_try_consume::<RowFit>().is_some_and(|flag| (flag.0)())
}
/// The title of the [`Row`] being rendered, for its descendants. A dropdown
/// with no explicit `.title()` uses it as its phone sheet header.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowTitle(pub(crate) State<String>);

/// Whether this component sits inside a row that takes presses. Text there is
/// plain, never selectable: a press anywhere on the row opens it, and a text
/// cursor over it would promise a selection that cannot happen.
pub(crate) fn use_in_pressable_row() -> bool {
    use_try_consume::<RowPressable>().is_some_and(|flag| (flag.0)())
}

/// One row: optional leading icon, title, optional subtitle, and trailing
/// content. The whole row is the target when `on_press` is set.
#[derive(Clone, PartialEq)]
pub struct Row {
    title: String,
    subtitle: Option<String>,
    trailing: Option<Element>,
    leading: Option<Icon>,
    leading_app: Option<String>,
    leading_avatar: Option<String>,
    leading_signal: Option<(u8, bool)>,
    leading_space: bool,
    leading_tone: Option<Tone>,
    on_press: Option<EventHandler<()>>,
    enabled: bool,
    navigates: bool,
    /// Whether the row opens something: it shows a hover fill and a pointer.
    activatable: bool,
    selected: bool,
    badge: Option<String>,
    destructive: bool,
    context_menu: Vec<MenuItem>,
    nested: Option<(bool, Vec<Element>)>,
    font: Option<String>,
    /// Whether the trailing content is a control (a switch, button, checkbox
    /// or dropdown). With a chevron it is set apart by a divider and the
    /// chevron becomes its own target. See [`Row::trailing`].
    trailing_control: bool,
    /// Whether the trailing content is a button-like control that fills the
    /// row with [`ROW_INSET`] on every side. See [`Row::trailing`].
    trailing_fit: bool,
    /// Whether the trailing content is a switch, which sits [`SWITCH_INSET`]
    /// from the row's trailing edge, the same as above and below it.
    trailing_switch: bool,
    /// Whether the trailing content is a text field. On a phone it fills what
    /// the title leaves instead of keeping its width, or goes under the title
    /// when there is not room. See [`Row::trailing`].
    trailing_fluid: bool,
    /// Whether the trailing content is a read-only value. The title and the
    /// value then share the row evenly, and a long value is cut to its half.
    value_trailing: bool,
    /// Whether the subtitle is kept to one line, cut with an ellipsis, and the
    /// row takes the subtitle height. See [`Row::subtitle_one_line`].
    subtitle_one_line: bool,
    /// The radio of a radio row, set by [`radio_row`].
    radio: Option<RadioChoice>,
    /// Byte ranges of the title drawn on the search mark, from a search hit.
    title_marks: Option<Vec<Range<usize>>>,
    /// Byte ranges of the subtitle drawn on the search mark.
    subtitle_marks: Option<Vec<Range<usize>>>,
}

pub fn row(title: impl Into<String>) -> Row {
    Row {
        title: title.into(),
        subtitle: None,
        trailing: None,
        leading: None,
        leading_app: None,
        leading_avatar: None,
        leading_signal: None,
        leading_space: false,
        leading_tone: None,
        on_press: None,
        enabled: true,
        navigates: false,
        activatable: false,
        selected: false,
        badge: None,
        destructive: false,
        context_menu: Vec::new(),
        nested: None,
        font: None,
        value_trailing: false,
        subtitle_one_line: false,
        trailing_control: false,
        trailing_fit: false,
        trailing_switch: false,
        trailing_fluid: false,
        radio: None,
        title_marks: None,
        subtitle_marks: None,
    }
}

impl Row {
    /// Draws these byte ranges of the title on the search mark: where a search
    /// hit (`bingux_ui::search`) matched. The row then does not look for the
    /// pane's query itself, so a hit list marks what the engine matched, with
    /// typo tolerance, not what a substring test would.
    pub fn title_marks(mut self, ranges: Vec<Range<usize>>) -> Self {
        self.title_marks = Some(ranges);
        self
    }

    /// Draws these byte ranges of the subtitle on the search mark.
    pub fn subtitle_marks(mut self, ranges: Vec<Range<usize>>) -> Self {
        self.subtitle_marks = Some(ranges);
        self
    }

    /// Rows set in under this one, indented by a gutter and shown while
    /// `shown` is true, with the reveal an expander uses. They are not mounted
    /// while hidden, and a row among them that matches a search also makes this
    /// row match.
    pub fn nested(mut self, shown: bool, rows: Vec<Element>) -> Self {
        self.nested = Some((shown, rows));
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let subtitle = subtitle.into();
        self.subtitle = (!subtitle.is_empty()).then_some(subtitle);
        self
    }

    pub fn maybe_subtitle(mut self, subtitle: Option<String>) -> Self {
        self.subtitle = subtitle.filter(|text| !text.is_empty());
        self
    }

    /// Keeps the subtitle to one line, cut with an ellipsis when it is too
    /// wide. The row keeps its natural height, so a list of such rows has
    /// one height per row: `Dim::RowSubtitle` at mouse density.
    pub fn subtitle_one_line(mut self) -> Self {
        self.subtitle_one_line = true;
        self
    }

    /// Trailing content. A switch, checkbox, dropdown or button passed here
    /// directly counts as a control: in a row that also navigates, a divider
    /// then separates it from the chevron, and the chevron is a target of its
    /// own that opens the page, while the control only acts. Wrap a control
    /// in a container and call [`Row::trailing_control`] to get the same.
    ///
    /// A [`TextField`] passed directly fills the row on a phone: the title
    /// keeps its width (at most [`LABEL_SHARE`](crate::controls::adaptive::LABEL_SHARE)
    /// of the line) and the field takes the rest, at least [`Dim::FieldMin`];
    /// a title that leaves it less goes above the field. See
    /// [`choose_filled_line`](crate::controls::adaptive::choose_filled_line).
    pub fn trailing<E: Into<Element> + 'static>(mut self, trailing: E) -> Self {
        use std::any::TypeId;
        let id = TypeId::of::<E>();
        self.trailing_control = [
            TypeId::of::<Switch>(),
            TypeId::of::<Checkbox>(),
            TypeId::of::<Dropdown>(),
            TypeId::of::<ActionButton>(),
            TypeId::of::<IconButton>(),
            TypeId::of::<RowButton>(),
            TypeId::of::<Stepper>(),
            TypeId::of::<Segmented>(),
            TypeId::of::<SplitButton>(),
        ]
        .contains(&id);
        self.trailing_fit = [
            TypeId::of::<Dropdown>(),
            TypeId::of::<ActionButton>(),
            TypeId::of::<IconButton>(),
            TypeId::of::<RowButton>(),
            TypeId::of::<Segmented>(),
            TypeId::of::<SplitButton>(),
        ]
        .contains(&id);
        self.trailing_switch = id == TypeId::of::<Switch>();
        self.trailing_fluid = id == TypeId::of::<TextField>();
        self.trailing = Some(trailing.into());
        self
    }

    /// Marks the trailing content as a control, for one that is not passed
    /// directly, such as a switch beside a spinner.
    pub fn trailing_control(mut self) -> Self {
        self.trailing_control = true;
        self
    }

    /// Trailing content when there is any; a row without it has none.
    pub fn maybe_trailing(mut self, trailing: Option<impl Into<Element>>) -> Self {
        self.trailing = trailing.map(Into::into);
        self.trailing_control = false;
        self.trailing_fit = false;
        self.trailing_switch = false;
        self.trailing_fluid = false;
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// Draws the leading icon in a status tone (accent, warning or danger)
    /// instead of the muted ink, for a row that reports a problem or a state.
    pub fn leading_tone(mut self, tone: Tone) -> Self {
        self.leading_tone = Some(tone);
        self
    }

    /// Reserves the leading icon column blank when there is no leading icon,
    /// so the text lines up with icon rows in the same group.
    pub fn leading_space(mut self) -> Self {
        self.leading_space = true;
        self
    }

    /// A full-colour application icon, by icon name, in place of a glyph.
    /// Four signal bars at the start, `level` 0..=4.
    pub fn leading_signal(mut self, level: u8, secured: bool) -> Self {
        self.leading_signal = Some((level, secured));
        self
    }

    /// The avatar of the account with login name `user` before the text.
    pub fn leading_avatar(mut self, user: impl Into<String>) -> Self {
        self.leading_avatar = Some(user.into());
        self
    }

    pub fn leading_app(mut self, icon_name: impl Into<String>) -> Self {
        self.leading_app = Some(icon_name.into());
        self
    }

    /// The row is pressed as a whole. Such a row also gets the hover and
    /// pressed fill, as it opens something.
    pub fn on_press(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.activatable = true;
        self.pressed(on_press)
    }

    /// Sets the press handler without making the row activatable, for a row
    /// that toggles when its label is pressed (a switch) but does not open
    /// anything.
    fn pressed(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.on_press = Some(on_press.into());
        self
    }

    /// Show a chevron and act like a link.
    pub fn navigates(mut self) -> Self {
        self.navigates = true;
        self.activatable = true;
        self
    }

    /// An accent checkmark.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A small rounded count or status badge after the title.
    pub fn badge(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.badge = (!text.is_empty()).then_some(text);
        self
    }

    /// A danger-coloured title, for rows such as "Remove" or "Forget".
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// A muted value on the right, for read-only rows.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if value.is_empty() {
            return self;
        }
        self.value_trailing = true;
        self.trailing(ValueText {
            text: value,
            mono: false,
        })
    }

    /// Like [`Row::value`] in the monospace face, for addresses, keys and
    /// other values read character by character.
    pub fn mono_value(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if value.is_empty() {
            return self;
        }
        self.value_trailing = true;
        self.trailing(ValueText {
            text: value,
            mono: true,
        })
    }

    /// Like [`Row::value`] for a path: when it does not fit it is shortened
    /// by what matters least (see `path_label`), the full path is the
    /// tooltip, and a right click offers Copy path.
    pub fn path_value(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        if path.is_empty() {
            return self;
        }
        self.value_trailing = true;
        self.trailing(PathValue { path })
    }

    /// Right click opens `items` as a context menu at the pointer. Use it for
    /// the actions that belong to this item, such as Open or Uninstall.
    pub fn context_menu(mut self, items: Vec<MenuItem>) -> Self {
        self.context_menu = items;
        self
    }

    /// Sets the title and subtitle in the font family `family`, on one line
    /// each, so a list of fonts can show each in its own face. Search marks
    /// are not drawn on such a row.
    pub fn font(mut self, family: impl Into<String>) -> Self {
        self.font = Some(family.into());
        self
    }
}

/// On a right press opens `items` at the pointer and stops the press there,
/// so the row is not also pressed. Returns true when a menu opened.
fn open_row_menu(
    overlays: Option<Overlays>,
    items: &[MenuItem],
    event: &Event<PointerEventData>,
) -> bool {
    let right = matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right));
    match overlays {
        Some(overlays) if right && !items.is_empty() => {
            event.stop_propagation();
            let at = event.global_location();
            overlays.show_menu((at.x as f32, at.y as f32), items.to_vec());
            true
        }
        _ => false,
    }
}

/// Starts a touch on a row that has a menu. A touch that rests for a long
/// press opens the menu where it began, which is how a touch does what a
/// right click does with a mouse.
fn begin_row_touch(
    press: Press,
    overlays: Option<Overlays>,
    items: &[MenuItem],
    event: &Event<PointerEventData>,
) {
    let long = match overlays {
        Some(overlays) if !items.is_empty() => {
            let items = items.to_vec();
            Some(EventHandler::new(move |at: (f32, f32)| {
                overlays.show_menu(at, items.clone());
            }))
        }
        _ => None,
    };
    press.begin(event, long);
}

/// A read-only value on the right of a row. It is selectable: Ctrl+C copies
/// it, and a right click offers Copy. It stays on one line. It fills the
/// half of the row that `Row` gives a value, aligned to its end, and a long
/// value is cut there with an ellipsis.
#[derive(PartialEq)]
struct ValueText {
    text: String,
    mono: bool,
}

impl Component for ValueText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let press = use_press();
        let style = crate::typography::TextStyle {
            mono: self.mono,
            ..VALUE
        };
        let pressable = use_in_pressable_row();
        // A selection would take the press that opens a pressable row, so
        // there the value is plain text; elsewhere it can be selected.
        let value: Element = if pressable {
            style
                .label(&tokens, self.text.clone())
                .max_lines(1)
                .line_height(VALUE.leading)
                .text_align(TextAlign::End)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Size::fill())
                .into()
        } else {
            // Read-only text is not a control: it can be selected and copied
            // with the pointer, but it is not a Tab stop.
            SelectableText::new()
                .span(style.span(&tokens, self.text.clone()))
                .max_lines(1)
                .line_height(VALUE.leading)
                .text_align(TextAlign::End)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Size::fill())
                .a11y_focusable(false)
                .into()
        };
        let text = self.text.clone();
        rect()
            .component("ValueText")
            .tw(&tokens, &[w_flex()])
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let text = text.clone();
                let items = vec![menu_item("Copy", move |_| {
                    if let Some(overlays) = overlays {
                        copy_text(overlays, text.clone());
                    }
                })];
                open_row_menu(overlays, &items, &event);
                begin_row_touch(press, overlays, &items, &event);
            })
            .on_global_pointer_up(move |_| press.release())
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .child(value)
    }
}

/// The trailing path of [`Row::path_value`]: the half of the row that a
/// value gets, aligned to its end.
#[derive(PartialEq)]
struct PathValue {
    path: String,
}

impl Component for PathValue {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect().component("ValuePath").tw(&tokens, &[w_flex()]).child(
            crate::controls::path_label(self.path.clone())
                .style(VALUE)
                .end()
                .copyable(true),
        )
    }
}

/// Copies `text` to the clipboard and says so, or says that it could not.
fn copy_text(overlays: Overlays, text: String) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => overlays.toast("Copied."),
        Err(_) => overlays.toast("The text could not be copied."),
    }
}

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut face_size = use_state(|| (0.0f32, 0.0f32));
        let mut body_size = use_state(|| (0.0f32, 0.0f32));
        let mut zone_hovering = use_state(|| false);
        let mut zone_pressing = use_state(|| false);
        let zone_id = use_a11y();
        let zone_focus = use_focus(zone_id);
        let press = use_press();
        let generated_id = use_a11y();
        let radio = self.radio.as_ref();
        let a11y_id = radio
            .and_then(|choice| choice.a11y_id)
            .unwrap_or(generated_id);
        let focus = use_focus(a11y_id);
        // The focus rings of the body, the chevron zone and the face are made
        // through hooks, and which of them a row draws depends on its props
        // (a row split in two draws two, any other one). A row takes the hooks
        // of all three every render, so its hook count never changes.
        let ring_core = use_focus_core();
        let body_ring = use_ring_size();
        let zone_ring = use_ring_size();
        let face_ring = use_ring_size();
        provide_enabled(self.enabled);
        let interactive = self.on_press.is_some() && self.enabled;
        let focusable = interactive && radio.is_none_or(|choice| choice.tab_stop);
        let mut pressable = use_provide_context(|| RowPressable(State::create(interactive)));
        pressable.0.set_if_modified(interactive);
        let mut trailing_width = use_state(|| 0.0f32);
        // A control that cannot shrink (a segmented control, a button) beside
        // a title that would get less than its floor goes under the title, as
        // in `controls::adaptive`. Only a row of title, control and nothing
        // that navigates is laid out this way.
        let plain_line = self.trailing.is_some()
            && !self.navigates
            && !self.value_trailing
            && radio.is_none();
        let stackable = self.trailing_control && plain_line;
        // A text field on a phone fills what the title leaves, or goes under
        // the title when it would get less than `Dim::FieldMin`.
        let filling = self.trailing_fluid && plain_line && is_compact();
        // The room the title and the control share: the face less its gutters
        // and any icon or avatar before the title.
        let line_inner = || {
            let leading = if self.leading.is_some() || self.leading_space || self.leading_signal.is_some() {
                Dim::IconRow.px() + S3.px()
            } else if self.leading_avatar.is_some() || self.leading_app.is_some() {
                Dim::Box9.px() + S3.px()
            } else {
                0.0
            };
            (face_size().0 - 2.0 * GUTTER.px() - leading).max(0.0)
        };
        let filled = filling.then(|| {
            choose_filled_line(
                line_inner(),
                S3.px() + S1.px(),
                LabelFacts::measure(&self.title, BODY),
                Dim::FieldMin.px(),
            )
        });
        // Until the row and its control have been sized there is nothing to
        // choose (`choose_line` says `Beside`), and the title is not measured:
        // measuring shapes the title and each of its words, on the frame that
        // mounts the row.
        let stacked = (stackable && trailing_width() > 0.0 && line_inner() > 0.0 && {
            let control = ControlFacts {
                natural: trailing_width(),
                least: None,
            };
            choose_line(
                line_inner(),
                S3.px() + S1.px(),
                LabelFacts::measure(&self.title, BODY),
                control,
            ) == LineMode::Stacked
        }) || filled == Some(LineMode::Stacked);
        // The title's width in a line the field fills: its own, at most its share.
        let title_cell = match filled {
            Some(LineMode::Shrunk { label }) => Some(label_cell_width(label)),
            _ => None,
        };
        LineFit::provide(filled.is_some());
        let fit = self.trailing_fit && !stacked;
        let mut fitting = use_provide_context(|| RowFit(State::create(fit)));
        fitting.0.set_if_modified(fit);
        let mut row_title = use_provide_context(|| RowTitle(State::create(self.title.clone())));
        row_title.0.set_if_modified(self.title.clone());
        // Only a row that opens something, such as one with a chevron, lights
        // on hover and press. A switch, slider or value row does not.
        let activatable = interactive && self.activatable;
        let overlays = try_consume_context::<Overlays>();
        let menu = self.context_menu.clone();
        // A location's `#fragment` equal to the title's slug scrolls here and
        // flashes the row (see `nav::anchor`).
        let anchor = crate::nav::use_anchor(&crate::nav::slug(&self.title));
        let query = search_highlight::current();
        let row_hit = query.as_deref().is_some_and(|query| {
            search_highlight::hits(query, &self.title)
                || self
                    .subtitle
                    .as_deref()
                    .is_some_and(|subtitle| search_highlight::hits(query, subtitle))
        });
        let nested_shown = self.nested.as_ref().is_some_and(|(shown, _)| *shown);
        let nested_lit = use_state(Vec::<usize>::new);
        let nested_matched = use_state(Vec::<usize>::new);
        let nested_hit = !nested_matched.read().is_empty();
        use_row_slot(
            (activatable && (hovering() || pressing())) || self.selected,
            row_hit || nested_hit,
        );
        let single_line = self.subtitle.is_none() && !stacked;
        let ink = if !self.enabled {
            Ink::Muted
        } else if self.destructive {
            Ink::Danger
        } else {
            Ink::Text
        };
        let icon_paint = if self.selected {
            Paint::Accent
        } else {
            Paint::Muted
        };

        let title = match &self.font {
            Some(family) => BODY
                .in_ink(ink)
                .label(&tokens, self.title.clone())
                .font_family(family.clone())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into(),
            None => match &self.title_marks {
                Some(ranges) => {
                    search_highlight::highlighted_label(BODY.in_ink(ink), &tokens, &self.title, ranges)
                        .into()
                }
                None => search_highlight::label(BODY.in_ink(ink), &tokens, &self.title),
            },
        };
        // On a phone a long title wraps onto a second line rather than
        // being cut, as a narrow row has little room beside its control.
        let wrapped = is_compact()
            && self.badge.is_none()
            && self.font.is_none()
            && self.title_marks.is_none();
        let title_line: Element = match &self.badge {
            None if wrapped => search_highlight::marked(BODY.in_ink(ink), &tokens, &self.title)
                .unwrap_or_else(|| BODY.in_ink(ink).paragraph(&tokens, self.title.clone()))
                .width(Size::fill())
                .max_lines(2)
                .text_overflow(TextOverflow::Ellipsis)
                .into(),
            None => title,
            Some(badge) => rect()
                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                .child(title)
                // Optical: the title's caps sit about 2 px below its line-box
                // centre, so the pill is nudged down by half a 4 px margin.
                .child(
                    rect().tw(&tokens, &[mt(S1)]).child(
                        rect()
                            .tw(&tokens, BADGE_BOX)
                            .child(BADGE.label(&tokens, badge.clone()).max_lines(1)),
                    ),
                )
                .into(),
        };
        let subtitle_lines = if self.subtitle_one_line { 1 } else { 3 };
        // A subtitle wraps onto a second or third line, so a row that has one
        // grows to fit it. A row without one has the row height, and its
        // text is centred by the row.
        let subtitle_line: Option<Element> =
            self.subtitle.as_ref().map(|subtitle| match &self.font {
                Some(family) => CAPTION
                    .label(&tokens, subtitle.clone())
                    .font_family(family.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into(),
                None if self.subtitle_marks.is_some() => search_highlight::highlighted_label(
                    CAPTION,
                    &tokens,
                    subtitle,
                    self.subtitle_marks.as_deref().unwrap_or_default(),
                )
                .max_lines(subtitle_lines)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Size::fill())
                .into(),
                None => match search_highlight::marked(CAPTION, &tokens, subtitle) {
                    Some(marked) => marked
                        .max_lines(subtitle_lines)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .into(),
                    // Plain text, not selectable: Freya makes selectable text a
                    // Tab stop, and a subtitle is not a control.
                    None => CAPTION
                        .paragraph(&tokens, subtitle.clone())
                        .max_lines(subtitle_lines)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .into(),
                },
            });
        // A tall row with a trailing button keeps the button's inset above and
        // below, and its text takes the rest of the row's usual padding, so
        // the row is no taller than one without the button.
        let text_column = rect()
            .tw(&tokens, ROW_TEXT)
            .map(title_cell, |text, width| text.width(Size::px(width)))
            .maybe(fit && !single_line, |text| {
                text.tw(&tokens, &[py(S1)])
            })
            .child(title_line)
            .maybe_child(subtitle_line)
            .maybe_child(stacked.then(|| {
                rect()
                    .tw(&tokens, &[flex_row(), w_full(), mt(S2)])
                    .maybe_child(self.trailing.clone())
            }));
        // A row with a control and a chevron is split in two: the body, which
        // holds the text and the control and opens the page when its text is
        // pressed, and the chevron zone, a target of its own beyond a divider.
        let split =
            self.navigates && self.trailing_control && self.trailing.is_some() && radio.is_none();
        let trailing = (!stacked && (self.trailing.is_some() || self.selected || (self.navigates && !split)))
            .then(|| {
            rect()
                .tw(&tokens, ROW_TRAILING)
                .on_sized(move |event: Event<SizedEventData>| {
                    trailing_width.set_if_modified(event.area.width());
                })
                // A value takes the same share of the row as the title and
                // fills it, so a long value is cut before the chevron. Any
                // other trailing control keeps its own width. Freya's layout
                // cannot shrink an overflowing child, so a cap on this area
                // would push a chevron or a dropdown past the box edge. A text
                // field on a phone takes what the title's own width leaves.
                .maybe(self.value_trailing, |element| {
                    element.tw(&tokens, &[w_flex(), grow_children(), justify(Align::End)])
                })
                .maybe(title_cell.is_some(), |element| {
                    element
                        .tw(&tokens, &[w_flex(), grow_children()])
                        .min_width(Size::px(0.0))
                })
                .maybe_child(self.trailing.clone())
                .maybe_child((self.selected && !self.navigates).then(|| {
                    icons::icon(Icon::Check, IconSize::Mark, Paint::Accent.color(&tokens))
                }))
                .maybe_child((self.navigates && !split).then(|| {
                    icons::icon(Icon::GoNext, IconSize::Mark, Paint::Muted.color(&tokens))
                }))
        });

        let mut content: Vec<Element> = Vec::new();
        if let Some(choice) = radio {
            content.push(
                rect()
                    .opacity(if self.enabled { 1.0 } else { DISABLED_OPACITY })
                    .child(RadioMark {
                        selected: choice.selected,
                        hover: hovering() && self.enabled,
                        press: pressing() && self.enabled,
                    })
                    .into(),
            );
        }
        let leading: Option<Element> = match self.leading {
            Some(leading) => {
                let glyph_colour = match self.leading_tone {
                    Some(tone) => tone.color(&tokens),
                    None => icon_paint.color(&tokens),
                };
                let glyph = icons::icon(leading, IconSize::Row, glyph_colour);
                // With a subtitle the icon sits beside the text lines,
                // centred on them. The column fills the row's height
                // only once the row has been sized by its text
                // (`fill_minimum`); a plain fill measured as all the
                // space the row's parent had left, and stretched the
                // first such row of a list.
                Some(if single_line {
                    glyph.into()
                } else if split {
                    // The body centres its children, and it is not the row's
                    // own box, so there is no height to fill.
                    rect().tw(&tokens, &[flex_col(), pt(S0_5)]).child(glyph).into()
                } else {
                    rect()
                        .height(Size::fill_minimum())
                        .tw(&tokens, &[flex_col(), pt(S0_5)])
                        .child(glyph)
                        .into()
                })
            }
            None => self.leading_space.then(|| {
                rect()
                    .tw(&tokens, &[w(Dim::IconRow), h(Dim::IconRow)])
                    .into()
            }),
        };
        content.extend(leading);
        content.extend(
            self.leading_signal.map(|(level, secured)| {
                crate::controls::signal_bars(level, secured, &tokens).into_element()
            }),
        );
        content.extend(
            self.leading_avatar
                .as_ref()
                .map(|user| crate::controls::avatar(user.clone(), Dim::Box9).into_element()),
        );
        content.extend(
            self.leading_app
                .as_ref()
                .map(|name| app_icon(name.clone(), IconSize::App).into_element()),
        );
        content.push(text_column.into_element());
        content.extend(trailing.map(|trailing| trailing.into_element()));
        let parts: Vec<Element> = if split {
            // A one-line row has a fixed height, which the zone fills. A tall
            // row is as high as its text, so the zone takes the height its
            // body was measured at: `fill_minimum` here took all the room the
            // card had left.
            let zone_height = if single_line {
                Size::fill()
            } else {
                Size::px(body_size().1)
            };
            let lit_body = !zone_hovering();
            let mut body = rect()
                .component("RowBody")
                .tw(
                    &tokens,
                    if single_line {
                        ROW_SPLIT_BODY
                    } else {
                        ROW_TALL_SPLIT_BODY
                    },
                )
                .maybe(self.trailing_fit, |body| body.tw(&tokens, &[pr(ROW_INSET)]))
                .maybe(self.trailing_fit && !single_line, |body| {
                    body.tw(&tokens, &[py(S2)])
                })
                .maybe(single_line, |body| body.height(Size::fill()))
                .tw(
                    &tokens,
                    &[surface(
                        Paint::Transparent,
                        activatable && lit_body && hovering(),
                        activatable && lit_body && pressing(),
                        self.selected,
                    )],
                )
                .children(content)
                .on_sized(move |event: Event<SizedEventData>| {
                    body_size.set_if_modified((event.area.width(), event.area.height()));
                });
            body = wrap_focusable_with(
                &tokens,
                ring_core.clone(),
                body,
                focus(),
                true,
                RingSize::Measured(body_ring),
            )
            // The focus wrapper is what the face lays out: it takes the
            // body's share of the width, and the body fills it.
            .tw(&tokens, &[w_flex()])
            .maybe(single_line, |wrapper| wrapper.height(Size::fill()));
            // One hairline, 60% of a one-line row: the row's height less
            // a gap above and below. The height is measured, not a token.
            let divider_height = (face_size().1 - 2.0 * S3.px()).max(0.0);
            let divider = rect()
                .tw(&tokens, &[w(Dim::Hairline), bg(Paint::Separator)])
                .height(Size::px(divider_height));
            let open = self.on_press.clone();
            let zone_title = self.title.clone();
            let zone = rect()
                .component("RowChevron")
                .a11y_id(zone_id)
                .a11y_focusable(interactive)
                .a11y_role(AccessibilityRole::Button)
                .a11y_builder(move |builder| builder.set_label(format!("Open {zone_title}")))
                .tw(&tokens, ROW_CHEVRON_ZONE)
                .height(Size::fill())
                .tw(
                    &tokens,
                    &[surface(
                        Paint::Transparent,
                        interactive && zone_hovering(),
                        interactive && zone_pressing(),
                        false,
                    )],
                )
                .on_global_pointer_up(move |_| zone_pressing.set_if_modified(false))
                .maybe(interactive, |element| {
                    element
                        .on_pointer_down(move |_| zone_pressing.set(true))
                        .on_pointer_enter(move |_| zone_hovering.set(true))
                        .on_pointer_leave(move |_| {
                            zone_hovering.set(false);
                            zone_pressing.set_if_modified(false);
                        })
                        .on_press(move |event: Event<PressEventData>| {
                            event.stop_propagation();
                            if !press.tap(&event) {
                                return;
                            }
                            if let Some(open) = &open {
                                open.call(());
                            }
                            zone_id.request_focus();
                        })
                })
                .child(icons::icon(
                    Icon::GoNext,
                    IconSize::Mark,
                    Paint::Muted.color(&tokens),
                ));
            let zone = wrap_focusable_with(
                &tokens,
                ring_core.clone(),
                zone,
                zone_focus(),
                true,
                RingSize::Measured(zone_ring),
            )
            // The wrapper takes the height; the zone fills it.
            .height(zone_height);
            vec![body.into(), divider.into(), zone.into()]
        } else {
            content
        };
        let on_press = self.on_press.clone();
        let face = rect()
            .component("Row")
            .published(|| {
                Published::call("row")
                    .arg(Prop::text("title", self.title.clone()))
                    .maybe_method(
                        self.subtitle
                            .as_ref()
                            .map(|subtitle| Prop::text("subtitle", subtitle.clone())),
                    )
                    .maybe_method(
                        self.badge
                            .as_ref()
                            .map(|badge| Prop::text("badge", badge.clone())),
                    )
                    .maybe_method((!self.enabled).then(|| Prop::flag("enabled", false)))
                    .maybe_method(self.selected.then(|| Prop::flag("selected", true)))
            })
            .a11y_id(a11y_id)
            .a11y_focusable(focusable)
            .a11y_role(if radio.is_some() {
                AccessibilityRole::RadioButton
            } else if interactive {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::GenericContainer
            })
            .maybe(radio.is_some(), |element| {
                let checked = radio.is_some_and(|choice| choice.selected);
                element.a11y_builder(move |builder| builder.set_toggled(Toggled::from(checked)))
            })
            .tw(
                &tokens,
                match (split, single_line) {
                    (true, true) => ROW_SPLIT,
                    (true, false) => ROW_TALL_SPLIT,
                    (false, true) => ROW,
                    (false, false) => ROW_TALL,
                },
            )
            .maybe(fit && !split, |element| {
                element.tw(&tokens, &[pr(ROW_INSET)])
            })
            .maybe(fit && !split && !single_line, |element| {
                element.tw(&tokens, &[py(S2)])
            })
            .maybe(self.trailing_switch && !split && single_line, |element| {
                element.tw(&tokens, &[pr(SWITCH_INSET)])
            })
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    activatable && !split && hovering(),
                    activatable && !split && pressing(),
                    self.selected && !split,
                )],
            )
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if open_row_menu(overlays, &menu, &event) {
                    return;
                }
                begin_row_touch(press, overlays, &menu, &event);
                if activatable {
                    pressing.set(true);
                }
            })
            .on_global_pointer_up(move |_| {
                pressing.set_if_modified(false);
                press.release();
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .maybe(interactive, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        if !press.tap(&event) {
                            return;
                        }
                        if let Some(on_press) = &on_press {
                            on_press.call(());
                        }
                        a11y_id.request_focus();
                    })
            })
            .children(parts)
            .maybe_child(anchor.flash())
            .on_sized(move |event: Event<SizedEventData>| {
                face_size.set_if_modified((event.area.width(), event.area.height()));
                anchor.on_sized().call(event);
            });
        // The ring is measured from the face, so a padded row that takes its
        // height from its text needs no special case.
        let face = if split {
            face
        } else {
            wrap_focusable_with(
                &tokens,
                ring_core.clone(),
                face,
                focus(),
                true,
                RingSize::Measured(face_ring),
            )
        }
        .tw(&tokens, &[w_full()]);
        let face = match radio.and_then(|choice| choice.nav.clone()) {
            Some(nav) => {
                face.on_key_down(move |event: Event<KeyboardEventData>| radio_arrow(&nav, event))
            }
            None => face,
        };
        let Some((_, rows)) = &self.nested else {
            return face.into_element();
        };
        let filter = SlotFilter {
            matched: nested_matched,
            group_hit: row_hit,
            searching: query.is_some(),
        };
        rect()
            .component("RowWithNested")
            .tw(&tokens, &[flex_col(), w_full()])
            .child(face)
            .child(
                crate::motion::reveal(
                    nested_shown,
                    NestedRows {
                        rows: rows.clone(),
                        lit: nested_lit,
                        filter,
                    },
                )
                .lazy(),
            )
            .into_element()
    }
}

/// The rows under a row, set in by one gutter. They take the same hairlines,
/// lit handling and search filtering as the rows of a card, through the same
/// slots, so a row that is hovered or matches a search behaves as it would in
/// the card. Their matches count once, through the row above them.
#[derive(PartialEq)]
struct NestedRows {
    rows: Vec<Element>,
    lit: State<Vec<usize>>,
    filter: SlotFilter,
}

impl Component for NestedRows {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = use_state(|| 0usize);
        use_provide_context(move || MatchSink(count));
        rect()
            .component("NestedRows")
            .tw(&tokens, &[flex_col(), w_full(), pl(GUTTER)])
            .children(card_rows(self.rows.clone(), self.lit, self.filter))
    }
}

/// A row with a switch on the right. Only the switch toggles: a press on the
/// rest of the row does nothing, so a stray click on the text never changes
/// a setting.
pub fn switch_row(
    title: impl Into<String>,
    on: bool,
    on_toggle: impl Into<EventHandler<bool>>,
) -> Row {
    let on_toggle = on_toggle.into();
    row(title).trailing(switch(on, move |next: bool| on_toggle.call(next)))
}

/// A level row for a setting: a title on the left and a slider filling the
/// rest of the line. `value` and the callback use 0 to 100.
pub fn slider_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> LevelRow {
    level_row(title, value, on_moved)
}

/// The share of a level row's width the title takes on a desktop. Sliders in
/// one box then start at the same x, when they share the same control slot.
const TITLE_SHARE: f32 = 34.0;

/// A level row: a title, an optional control such as a mute button, and a
/// slider that fills the rest of the line. A value, if set, sits at the far
/// right. On a phone the title takes the first line and the slider the
/// second, under it.
#[derive(PartialEq)]
pub struct LevelRow {
    title: String,
    subtitle: Option<String>,
    leading: Option<Element>,
    control: Option<Element>,
    value: f64,
    value_text: Option<String>,
    max: f64,
    mark: Option<f64>,
    mark_label: Option<String>,
    danger_from: Option<f64>,
    enabled: bool,
    on_moved: EventHandler<f64>,
    footer: Option<Element>,
    context_menu: Vec<MenuItem>,
}

pub fn level_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> LevelRow {
    LevelRow {
        title: title.into(),
        subtitle: None,
        leading: None,
        control: None,
        value,
        value_text: None,
        max: 100.0,
        mark: None,
        mark_label: None,
        danger_from: None,
        enabled: true,
        footer: None,
        context_menu: Vec::new(),
        on_moved: on_moved.into(),
    }
}

impl LevelRow {
    /// The top of the travel, in percent. Above 100 is amplified output, such
    /// as a volume past full; the track and the keys respect it.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max.max(1.0);
        self
    }

    /// A tick on the track at this percent, such as the 100% clipping point.
    /// It does not limit the value.
    pub fn mark(mut self, percent: f64) -> Self {
        self.mark = Some(percent);
        self
    }

    /// A label under the track, centred on the mark, such as "100%" for a
    /// boost. It shows only with a mark. Its line is laid out from the first
    /// frame and stays invisible until the track is measured, so the row
    /// never grows when the width arrives.
    pub fn mark_label(mut self, text: impl Into<String>) -> Self {
        self.mark_label = Some(text.into());
        self
    }

    /// Paint the travel past `percent` in the danger colour, such as the
    /// clipping zone past 100%. The value still goes that high.
    pub fn danger_from(mut self, percent: f64) -> Self {
        self.danger_from = Some(percent);
        self
    }

    /// The text at the far right of the row, such as "500 ms". Without it the
    /// row shows no readout, and the value never wraps.
    pub fn value_text(mut self, text: impl Into<String>) -> Self {
        self.value_text = Some(text.into());
        self
    }

    /// A disabled row dims and its slider ignores input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// An element before the title, such as an app icon.
    pub fn leading(mut self, leading: impl Into<Element>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    /// An element between the title and the slider, such as a mute button.
    /// Give a blank `Dim::Box9` where a row has none, so its slider starts
    /// at the same x as the rows that do.
    pub fn control(mut self, control: impl Into<Element>) -> Self {
        self.control = Some(control.into());
        self
    }

    /// Text or controls under the slider, in the same column and indented
    /// like the track, such as a meter's readout.
    pub fn footer(mut self, footer: impl Into<Element>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    /// Right click opens `items` as a context menu at the pointer, such as
    /// Mute or Reset for a volume row.
    pub fn context_menu(mut self, items: Vec<MenuItem>) -> Self {
        self.context_menu = items;
        self
    }
}

impl Component for LevelRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let press = use_press();
        let menu = self.context_menu.clone();
        let value = self.value.clamp(0.0, self.max);
        provide_enabled(self.enabled);
        let row_hit = search_highlight::current().as_deref().is_some_and(|query| {
            search_highlight::hits(query, &self.title)
                || self
                    .subtitle
                    .as_deref()
                    .is_some_and(|subtitle| search_highlight::hits(query, subtitle))
        });
        use_row_match(row_hit);
        let ink = if self.enabled { Ink::Text } else { Ink::Muted };
        let compact = is_compact();
        // The track's measured width places the boost label under its tick.
        let mut track_width = use_state(|| 0.0_f32);

        let title = search_highlight::label(BODY.in_ink(ink), &tokens, &self.title);
        let subtitle: Option<Element> = self.subtitle.as_ref().map(|subtitle| -> Element {
            match search_highlight::marked(CAPTION, &tokens, subtitle) {
                Some(marked) => marked
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into(),
                // Subtitles are plain text, not focus stops.
                None => CAPTION
                    .paragraph(&tokens, subtitle.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into(),
            }
        });
        let title_block = rect()
            .tw(&tokens, &[flex_col(), gap(S0_5), w_flex()])
            .child(title)
            .maybe_child(subtitle);
        let readout: Option<Element> = self.value_text.as_ref().map(|text| -> Element {
            rect()
                .tw(&tokens, &[flex_row(), justify(Align::End), w_full()])
                // Tabular figures: a level that changes keeps its width.
                .child(
                    crate::typography::TextStyle {
                        tabular: true,
                        ..VALUE
                    }
                    .label(&tokens, text.clone())
                    .max_lines(1),
                )
                .into()
        });

        let mut track = slider(value, self.on_moved.clone()).max(self.max).fill();
        if let Some(mark) = self.mark {
            track = track.mark(mark);
        }
        if let Some(point) = self.danger_from {
            track = track.danger_from(point);
        }
        let measured = track_width();
        let boost_label: Option<Element> = match (self.mark, &self.mark_label) {
            (Some(point), Some(text)) => {
                // The tick is where the thumb's centre sits at `point`: half
                // a thumb in, then the travel up to it. The label is centred
                // on that x, so its box starts half a label to the left.
                let thumb = Dim::SliderThumb.px();
                let travel = (measured - thumb).max(1.0);
                let ratio = (point / self.max).clamp(0.0, 1.0) as f32;
                let left = (thumb / 2.0 + travel * ratio - Dim::Box9.px() / 2.0).max(0.0);
                Some(
                    rect()
                        .tw(&tokens, &[flex_row(), w_full()])
                        .opacity(if measured > 0.0 { 1.0 } else { 0.0 })
                        .child(rect().width(Size::px(left)))
                        .child(
                            rect()
                                .tw(&tokens, &[w(Dim::Box9), flex_row(), justify(Align::Center)])
                                .child(
                                    CAPTION
                                        .in_ink(Ink::Muted)
                                        .label(&tokens, text.clone())
                                        .max_lines(1),
                                ),
                        )
                        .into(),
                )
            }
            _ => None,
        };
        let track_block = rect()
            .tw(&tokens, &[flex_col(), w_flex(), gap(S1)])
            .child(
                rect()
                    .width(Size::fill())
                    .on_sized(move |event: Event<SizedEventData>| {
                        track_width.set_if_modified(event.area.width());
                    })
                    .child(track),
            )
            .maybe_child(boost_label)
            .maybe_child(readout)
            .maybe_child(self.footer.clone());

        let outer = rect()
            .component("LevelRow")
            .tw(&tokens, &[w_full(), px(GUTTER), py(S3)])
            .on_pointer_down(move |event: Event<PointerEventData>| {
                open_row_menu(overlays, &menu, &event);
                begin_row_touch(press, overlays, &menu, &event);
            })
            .on_global_pointer_up(move |_| press.release())
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event));
        if compact {
            let title_line = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S2), w_full()],
                )
                .maybe_child(self.leading.clone())
                .child(title_block);
            let track_line = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S4), w_full()],
                )
                .maybe_child(self.control.clone())
                .child(track_block);
            outer
                .tw(&tokens, &[flex_col(), gap(S2)])
                .child(title_line)
                .child(track_line)
        } else {
            let title_side = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S4), h(Dim::Control)],
                )
                .width(Size::percent(TITLE_SHARE))
                .maybe_child(self.leading.clone())
                .child(title_block);
            let control: Option<Element> = self.control.clone().map(|control| -> Element {
                rect()
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), h(Dim::Control)],
                    )
                    .child(control)
                    .into()
            });
            outer
                .tw(&tokens, &[flex_row(), items(Align::Start), gap(S4)])
                .child(title_side)
                .maybe_child(control)
                .child(track_block)
        }
    }
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
    // One option is a fact, not a choice.
    if let [(_, only)] = options.as_slice() {
        return row(title).value(only.clone());
    }
    let short = options.len() <= 3 && options.iter().all(|(_, label)| label.chars().count() <= 12);
    if short {
        return row(title).trailing(segmented(options, selected, on_select));
    }
    row(title).trailing(dropdown(options, selected, on_select).min_width(Dim::Field))
}

/// The selected state of one option in a radio group, set by [`radio_row`].
#[derive(Clone, PartialEq)]
struct RadioChoice {
    selected: bool,
    on_select: EventHandler<()>,
    /// Whether the row is the group's tab stop, the only row Tab reaches.
    tab_stop: bool,
    /// The row's focus target, set by the group.
    a11y_id: Option<AccessibilityId>,
    nav: Option<RadioNav>,
}

/// What a radio row needs to hand selection and focus to its neighbours.
#[derive(Clone, PartialEq)]
struct RadioNav {
    items: Rc<Vec<NavItem>>,
    index: usize,
}

#[derive(Clone, PartialEq)]
struct NavItem {
    id: AccessibilityId,
    /// `None` when the option cannot be chosen, as a disabled row.
    on_select: Option<EventHandler<()>>,
}

/// Moves a radio group's selection with the arrow keys: the next or previous
/// option that can be chosen is selected and takes focus, wrapping at the ends.
fn radio_arrow(nav: &RadioNav, event: Event<KeyboardEventData>) {
    let forward = match &event.key {
        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => true,
        Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => false,
        _ => return,
    };
    let items = &nav.items;
    let Some(next) = next_enabled(items.len(), nav.index, forward, |index| {
        items[index].on_select.is_some()
    }) else {
        return;
    };
    event.stop_propagation();
    let item = &items[next];
    if let Some(on_select) = &item.on_select {
        on_select.call(());
    }
    item.id.request_focus();
}

/// A row that is one option of a radio group: a radio on its leading edge,
/// set when `selected`. Pressing the row selects it. Put the rows in a
/// [`radio_rows`] group, which gives them the arrow keys and the tab stop.
pub fn radio_row(
    title: impl Into<String>,
    selected: bool,
    on_select: impl Into<EventHandler<()>>,
) -> Row {
    let on_select = on_select.into();
    let mut row = row(title).on_press(on_select.clone());
    row.radio = Some(RadioChoice {
        selected,
        on_select,
        tab_stop: false,
        a11y_id: None,
        nav: None,
    });
    row
}

/// A group of [`radio_row`]s under an optional title. Up and Down move the
/// selection between the rows, and only the selected row is in the tab order.
#[derive(PartialEq)]
pub struct RadioRows {
    title: Option<String>,
    rows: Vec<Row>,
    extra: Vec<Element>,
}

/// A group of radio rows; `title` is the card's heading, if any.
pub fn radio_rows(title: impl Into<Option<String>>, rows: Vec<Row>) -> RadioRows {
    RadioRows {
        title: title.into(),
        rows,
        extra: Vec::new(),
    }
}

impl RadioRows {
    /// A row after the options, such as an empty-state line.
    pub fn row(mut self, row: impl Into<Element>) -> Self {
        self.extra.push(row.into());
        self
    }
}

impl Component for RadioRows {
    fn render(&self) -> impl IntoElement {
        // One focus target per row, kept for the life of the group so a row
        // keeps its target when the options around it change.
        let ids = use_hook(|| Rc::new(RefCell::new(Vec::<AccessibilityId>::new())));
        let ids: Vec<AccessibilityId> = {
            let mut ids = ids.borrow_mut();
            while ids.len() < self.rows.len() {
                ids.push(AccessibilityId::new_unique());
            }
            ids[..self.rows.len()].to_vec()
        };
        let items: Rc<Vec<NavItem>> = Rc::new(
            self.rows
                .iter()
                .zip(&ids)
                .map(|(row, id)| NavItem {
                    id: *id,
                    on_select: row
                        .radio
                        .as_ref()
                        .filter(|_| row.enabled && row.on_press.is_some())
                        .map(|choice| choice.on_select.clone()),
                })
                .collect(),
        );
        // The selected option is the tab stop; with none selected, the first
        // option that can be chosen is.
        let tab_stop = self
            .rows
            .iter()
            .position(|row| row.radio.as_ref().is_some_and(|choice| choice.selected))
            .filter(|&index| items[index].on_select.is_some())
            .or_else(|| items.iter().position(|item| item.on_select.is_some()));
        let rows = self.rows.iter().enumerate().map(|(index, row)| {
            let mut row = row.clone();
            if let Some(choice) = row.radio.as_mut() {
                choice.a11y_id = Some(ids[index]);
                choice.tab_stop = tab_stop == Some(index);
                choice.nav = Some(RadioNav {
                    items: items.clone(),
                    index,
                });
            }
            Element::from(row)
        });
        crate::group::Group::new(self.title.clone())
            .rows(rows)
            .rows(self.extra.clone())
    }
}

/// A row with a text field on the right; `on_submit` fires on Enter.
pub fn text_row(
    title: impl Into<String>,
    value: State<String>,
    on_submit: impl Into<EventHandler<String>>,
) -> Row {
    row(title).trailing(text_field(value, on_submit))
}

/// A read-only fact: title on the left, value on the right, for About
/// pages. The value stays on one line; a long one is cut with an ellipsis,
/// and it can be selected and copied.
pub fn property_row(title: impl Into<String>, value: impl Into<String>) -> Row {
    let mut row = row(title);
    row.value_trailing = true;
    row.trailing(ValueText {
        text: value.into(),
        mono: false,
    })
}

/// A row whose trailing text is editable in place: click to edit, Enter
/// commits through `on_commit`, Escape restores the last committed value.
pub fn entry_row(
    title: impl Into<String>,
    value: impl Into<String>,
    on_commit: impl Into<EventHandler<String>>,
) -> EntryRow {
    EntryRow {
        title: title.into(),
        value: value.into(),
        placeholder: None,
        on_commit: on_commit.into(),
    }
}

/// See [`entry_row`].
#[derive(PartialEq)]
pub struct EntryRow {
    title: String,
    value: String,
    placeholder: Option<String>,
    on_commit: EventHandler<String>,
}

impl EntryRow {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }
}

impl Component for EntryRow {
    fn render(&self) -> impl IntoElement {
        let committed = self.value.clone();
        let draft = use_state(|| committed.clone());
        let on_commit = self.on_commit.clone();
        let field = text_field(draft, move |text: String| on_commit.call(text))
            .width(Dim::Field)
            .maybe_placeholder(self.placeholder.clone())
            .revert_to(committed);
        row(self.title.clone()).trailing(field)
    }
}

/// A row that reveals child rows below it when pressed. The chevron
/// rotates and the children fade in over 160 ms (none under reduced
/// motion).
pub fn expander_row(title: impl Into<String>, children: Vec<Element>) -> ExpanderRow {
    ExpanderRow {
        title: title.into(),
        subtitle: None,
        children,
        expanded: None,
    }
}

/// See [`expander_row`].
#[derive(PartialEq)]
pub struct ExpanderRow {
    title: String,
    subtitle: Option<String>,
    children: Vec<Element>,
    expanded: Option<bool>,
}

impl ExpanderRow {
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Start expanded (the row still toggles).
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
}

impl Component for ExpanderRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let initial = self.expanded.unwrap_or(false);
        let mut open = use_state(move || initial);
        let progress = use_tween(if open() { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        let chevron = rect().rotate(progress * 90.0).child(icons::icon(
            Icon::GoNext,
            IconSize::Mark,
            Paint::Muted.color(&tokens),
        ));
        let header = row(self.title.clone())
            .maybe_subtitle(self.subtitle.clone())
            .trailing(chevron)
            .on_press(move |_| open.set(!open()));
        // Child rows are set in by one gutter, with the same hairlines as
        // the rows of a card.
        let mut children = Vec::with_capacity(self.children.len() * 2);
        for child in &self.children {
            children.push(rect().tw(&tokens, SEPARATOR).into());
            children.push(child.clone());
        }
        rect()
            .component("ExpanderRow")
            .tw(&tokens, &[flex_col(), w_full()])
            .child(header)
            .child(crate::motion::reveal(
                open(),
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), pl(GUTTER)])
                    .children(children),
            ))
    }
}

/// How many options a [`choice_list`] draws at once. A list of hundreds of
/// time zones or fonts is narrowed by typing, not drawn whole.
const CHOICE_LIST_LIMIT: usize = 60;

/// A filter field above a card of options with a radio on the chosen one,
/// for a subpage that picks one value from a long list (a time zone, a font).
/// `options` are `(key, label)` pairs; `on_select` receives the chosen key.
#[derive(PartialEq)]
pub struct ChoiceList {
    options: Vec<(String, String)>,
    selected: String,
    placeholder: String,
    on_select: EventHandler<String>,
}

pub fn choice_list(
    options: Vec<(String, String)>,
    selected: impl Into<String>,
    on_select: impl Into<EventHandler<String>>,
) -> ChoiceList {
    ChoiceList {
        options,
        selected: selected.into(),
        placeholder: "Search".to_string(),
        on_select: on_select.into(),
    }
}

impl ChoiceList {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
}

impl Component for ChoiceList {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let query = use_state(String::new);
        let needle = query.read().trim().to_lowercase();
        let mut matching: Vec<&(String, String)> = self
            .options
            .iter()
            .filter(|(key, label)| {
                needle.is_empty()
                    || label.to_lowercase().contains(&needle)
                    || key.to_lowercase().contains(&needle)
            })
            .collect();
        // Before anything is typed, the current choice comes first, so it is
        // in view whichever part of a long list it belongs to.
        if needle.is_empty()
            && let Some(at) = matching.iter().position(|(key, _)| *key == self.selected)
        {
            let chosen = matching.remove(at);
            matching.insert(0, chosen);
        }
        let hidden = matching.len().saturating_sub(CHOICE_LIST_LIMIT);
        let rows: Vec<Row> = matching
            .iter()
            .take(CHOICE_LIST_LIMIT)
            .map(|(key, label)| {
                let on_select = self.on_select.clone();
                let chosen = key.clone();
                radio_row(label.clone(), *key == self.selected, move |_: ()| {
                    on_select.call(chosen.clone())
                })
            })
            .collect();
        let mut list = radio_rows(None::<String>, rows);
        if matching.is_empty() {
            list = list.row(crate::group::status(crate::group::Status::Empty(
                "Nothing matches.".to_string(),
            )));
        }
        rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S3)])
            .child(
                crate::controls::search_field(query, |_| {})
                    .placeholder(self.placeholder.clone())
                    .fill_width(),
            )
            .child(Element::from(list))
            .maybe_child((hidden > 0).then(|| {
                rect()
                    .tw(&tokens, &[w_full(), px(GUTTER)])
                    .child(CAPTION.label(
                        &tokens,
                        format!("{hidden} more. Keep typing to narrow the list."),
                    ))
            }))
    }
}

#[cfg(test)]
mod fit_tokens {
    use super::*;

    #[test]
    fn a_trailing_button_fills_the_row_less_the_inset_on_both_sides() {
        for class in [SizeClass::Regular, SizeClass::Compact] {
            assert_eq!(
                Dim::RowButton.px_in(class),
                Dim::Row.px_in(class) - 2.0 * ROW_INSET.px_in(class),
                "{class:?}",
            );
            assert!(Dim::RowButton.px_in(class) >= if class.is_compact() { TOUCH_MIN } else { 0.0 });
        }
    }

    #[test]
    fn a_trailing_button_rounds_as_the_card_less_the_inset() {
        assert_eq!(
            Radius::Inner.px(),
            Radius::Card.px() - ROW_INSET.fixed_px(),
        );
    }

    #[test]
    fn a_trailing_switch_is_the_same_distance_from_every_edge() {
        for class in [SizeClass::Regular, SizeClass::Compact] {
            assert_eq!(
                SWITCH_INSET.px_in(class),
                (Dim::Row.px_in(class) - Dim::SwitchHeight.px_in(class)) / 2.0,
                "{class:?}",
            );
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod fit_layout {
    use std::time::Duration;

    use super::*;
    use crate::group::Group;
    use crate::snapshot::Snapshot;

    #[derive(Clone, Copy, PartialEq)]
    enum Trailing {
        Button,
        Switch,
    }

    #[derive(PartialEq)]
    struct Scene(Trailing);

    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let tokens = crate::theme::provide_tokens();
            let row = match self.0 {
                Trailing::Button => row("Default")
                    .trailing(crate::controls::action_button("Action", |_| {}).compact()),
                Trailing::Switch => row("Default").trailing(switch(true, |_: bool| {})),
            };
            rect()
                .expanded()
                .tw(&tokens, &[p(S4)])
                .child(crate::shell::OverlayHost::new(
                    Group::new(None::<String>).rows(vec![row.into_element()]),
                ))
        }
    }

    /// Every box in the scene as (min x, max x, min y, max y).
    fn boxes(scene: Trailing, size: (f32, f32)) -> Vec<[f32; 4]> {
        let mut shot = Snapshot::new(move || Scene(scene).into(), size);
        shot.settle(Duration::from_millis(300));
        shot.runner().find_many(|node, _| {
            let area = node.layout().area;
            Some([area.min_x(), area.max_x(), area.min_y(), area.max_y()])
        })
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.51
    }

    /// A row box sits `inset` outside `child` on the top, bottom and right.
    fn row_around(boxes: &[[f32; 4]], child: [f32; 4], row_height: f32, inset: f32) -> bool {
        boxes.iter().any(|row| {
            near(row[3] - row[2], row_height)
                && near(row[2], child[2] - inset)
                && near(row[3], child[3] + inset)
                && near(row[1], child[1] + inset)
        })
    }

    #[test]
    fn a_trailing_button_is_inset_equally_from_the_top_bottom_and_trailing_edge() {
        for (class, size) in [
            (SizeClass::Regular, (900.0, 400.0)),
            (SizeClass::Compact, (390.0, 600.0)),
        ] {
            let boxes = boxes(Trailing::Button, size);
            let height = Dim::RowButton.px_in(class);
            let button = boxes
                .iter()
                .copied()
                .filter(|b| near(b[3] - b[2], height) && b[1] - b[0] > height)
                .max_by(|a, b| a[1].total_cmp(&b[1]))
                .unwrap_or_else(|| panic!("{class:?}: no button {height} px tall"));
            assert!(
                row_around(&boxes, button, Dim::Row.px_in(class), ROW_INSET.px_in(class)),
                "{class:?}: the button is not {} px from the row's top, bottom and right",
                ROW_INSET.px_in(class),
            );
        }
    }

    #[test]
    fn a_trailing_switch_is_inset_equally_from_the_top_bottom_and_trailing_edge() {
        for (class, size) in [
            (SizeClass::Regular, (900.0, 400.0)),
            (SizeClass::Compact, (390.0, 600.0)),
        ] {
            let boxes = boxes(Trailing::Switch, size);
            let switch = boxes
                .iter()
                .copied()
                .find(|b| {
                    near(b[3] - b[2], Dim::SwitchHeight.px_in(class))
                        && near(b[1] - b[0], Dim::SwitchWidth.px_in(class))
                })
                .unwrap_or_else(|| panic!("{class:?}: no switch"));
            assert!(
                row_around(&boxes, switch, Dim::Row.px_in(class), SWITCH_INSET.px_in(class)),
                "{class:?}: the switch is not {} px from the row's top, bottom and right",
                SWITCH_INSET.px_in(class),
            );
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod narrow_rows {
    use std::time::Duration;

    use super::*;
    /// 'Security' in the body face is about 60 px; a squeezed title is 0 to 15.
    const TITLE_MIN: f32 = 45.0;
    use crate::group::Group;
    use crate::snapshot::Snapshot;

    #[derive(PartialEq)]
    struct Scene;

    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let tokens = crate::theme::provide_tokens();
            let options = ["None", "WPA2", "WPA3 Enterprise"]
                .map(|name| (name.to_string(), name.to_string()))
                .to_vec();
            rect()
                .expanded()
                .tw(&tokens, &[p(S4)])
                .child(crate::shell::OverlayHost::new(
                    rect().tw(&tokens, &[w(Dim::Field)]).child(
                        Group::new(None::<String>).rows(vec![
                            row("Security")
                                .trailing(segmented(options, "WPA2", |_: String| {}))
                                .into_element(),
                        ]),
                    ),
                ))
        }
    }

    /// A control that cannot shrink goes under the title instead of taking
    /// its width: the title stays whole and on one line.
    #[test]
    fn a_title_beside_a_wide_control_keeps_its_width_or_the_control_stacks() {
        let mut shot = Snapshot::new(|| Scene.into(), (900.0, 400.0));
        shot.settle(Duration::from_millis(400));
        let labels: Vec<(String, f32)> = shot.runner().find_many(|node, element| {
            let text = match Label::try_downcast(element) {
                Some(label) => label.text.to_string(),
                None => Paragraph::try_downcast(element)?
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect(),
            };
            Some((text, node.layout().area.width()))
        });
        let (_, width) = labels
            .iter()
            .find(|(text, _)| text == "Security")
            .expect("the title");
        assert!(
            *width >= TITLE_MIN,
            "the title is {width} px wide, under {TITLE_MIN}",
        );
    }
}

/// A text field that trails a row fills the row on a phone, under a title that
/// keeps its width, or goes under the title when there is not room.
#[cfg(all(test, feature = "testing"))]
mod phone_fields {
    use std::time::Duration;

    use torin::prelude::Area;

    use super::*;
    use crate::group::Group;
    use crate::inspector::snapshot::Snapshot as Tree;
    use crate::snapshot::Snapshot;

    #[derive(PartialEq)]
    struct Scene(&'static str);

    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let tokens = crate::theme::provide_tokens();
            let value = use_state(String::new);
            rect()
                .expanded()
                .tw(&tokens, &[p(S4)])
                .child(crate::shell::OverlayHost::new(Group::new(None::<String>).rows(
                    vec![row(self.0).trailing(text_field(value, |_| {})).into_element()],
                )))
        }
    }

    /// Where the row, its title and its text field are.
    struct Line {
        row: Area,
        title: Area,
        field: Area,
    }

    fn line(title: &'static str, size: (f32, f32)) -> Line {
        let mut shot = Snapshot::new(move || Scene(title).into(), size);
        shot.settle(Duration::from_millis(500));
        let tree = Tree::capture_headless(shot.runner(), 1.0);
        let area = |name: &str| {
            tree.nodes
                .iter()
                .filter(|node| node.a11y.component.as_deref() == Some(name))
                .find_map(|node| node.area)
                .unwrap_or_else(|| panic!("no {name} in the tree"))
        };
        let titles: Vec<(String, Area)> = shot.runner().find_many(|node, element| {
            let text = match Label::try_downcast(element) {
                Some(label) => label.text.to_string(),
                None => Paragraph::try_downcast(element)?
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect(),
            };
            Some((text, node.layout().area))
        });
        let (_, title_area) = titles
            .into_iter()
            .find(|(text, _)| text == title)
            .unwrap_or_else(|| panic!("no title {title:?}"));
        Line {
            row: area("Row"),
            title: title_area,
            field: area("TextField"),
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.76
    }

    /// 'Try it' beside its field on the two phone widths the apps are
    /// held to: the field is no longer the 168 px it was, it ends at the
    /// row's gutter and starts one gap after the title.
    #[test]
    fn a_field_beside_a_short_title_takes_every_pixel_the_title_leaves() {
        for width in [390.0, 320.0] {
            let line = line("Try it", (width, 600.0));
            let gutter = GUTTER.px_in(SizeClass::Compact);
            let gap = S3.px_in(SizeClass::Compact) + S1.px_in(SizeClass::Compact);
            assert!(
                near(line.field.max_x(), line.row.max_x() - gutter),
                "{width}: the field ends at {}, not a gutter ({gutter}) from the row's end {}",
                line.field.max_x(),
                line.row.max_x(),
            );
            assert!(
                near(line.field.min_x(), line.title.max_x() + gap),
                "{width}: the field starts at {}, not a gap ({gap}) after the title's end {}",
                line.field.min_x(),
                line.title.max_x(),
            );
            assert!(
                line.field.width() > Dim::Field.px_in(SizeClass::Compact) + 20.0,
                "{width}: the field is {} px wide, no more than its 168 px",
                line.field.width(),
            );
            assert!(line.field.width() >= Dim::FieldMin.px_in(SizeClass::Compact), "{width}");
            // 'Try it' is about 36 px of body text: a title the field had
            // squeezed would be narrower.
            assert!(
                line.title.width() >= 33.0,
                "{width}: the title is {} px wide, narrower than its text",
                line.title.width(),
            );
            assert!(
                near(line.row.height(), Dim::Row.px_in(SizeClass::Compact)),
                "{width}: the row is {} px tall, not one line",
                line.row.height(),
            );
        }
    }

    /// A title with a word wider than its share cannot be set beside a field:
    /// the field goes under it, as wide as the row's content.
    #[test]
    fn a_title_with_a_wide_word_puts_the_field_under_it_at_the_full_width() {
        let line = line("Internationalisation", (320.0, 600.0));
        let gutter = GUTTER.px_in(SizeClass::Compact);
        assert!(
            line.field.min_y() >= line.title.max_y() - 0.5,
            "the field ({}) is not under the title ({})",
            line.field.min_y(),
            line.title.max_y(),
        );
        assert!(near(line.field.min_x(), line.row.min_x() + gutter), "{}", line.field.min_x());
        assert!(near(line.field.max_x(), line.row.max_x() - gutter), "{}", line.field.max_x());
    }

    /// A long title of short words keeps to its share and wraps; the field
    /// keeps at least `Dim::FieldMin`.
    #[test]
    fn a_long_title_of_short_words_wraps_in_its_share_and_leaves_the_field_its_least() {
        for width in [390.0, 320.0] {
            let line = line("Delay used when you test the keys", (width, 600.0));
            let inner = line.row.width() - 2.0 * GUTTER.px_in(SizeClass::Compact);
            assert!(
                line.title.width() <= crate::controls::adaptive::LABEL_SHARE * inner + 2.0,
                "{width}: the title is {} px of {inner}",
                line.title.width(),
            );
            assert!(line.field.width() >= Dim::FieldMin.px_in(SizeClass::Compact), "{width}");
            assert!(line.title.max_x() <= line.field.min_x(), "{width}: they overlap");
        }
    }

    /// On a desktop the field keeps its width: nothing changes there.
    #[test]
    fn a_field_on_a_desktop_keeps_its_width() {
        let line = line("Try it", (900.0, 400.0));
        assert!(
            near(line.field.width(), Dim::Field.px_in(SizeClass::Regular)),
            "the field is {} px wide",
            line.field.width(),
        );
    }
}
