//! Rows: the building block of every preference list.
//!
//! Mirrors `ControlRow.qml`: a leading icon, a title, an optional subtitle
//! and trailing content. Convenience constructors pair a row with a
//! switch, slider, choice or text field.

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::Toggled;
use freya::prelude::*;

use crate::app_icon::app_icon;
use crate::controls::dropdown::dropdown;
use crate::controls::radio::{RadioMark, next_enabled};
use crate::controls::segmented::segmented;
use crate::controls::slider::slider;
use crate::controls::switch::switch;
use crate::controls::text_field::text_field;
use crate::group::{SlotFilter, card_rows, use_row_lit, use_row_match};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS, use_tween};
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
    /// Whether the trailing content is a read-only value. The title and the
    /// value then share the row evenly, and a long value is cut to its half.
    value_trailing: bool,
    /// Whether the subtitle is kept to one line, cut with an ellipsis, and the
    /// row takes the subtitle height. See [`Row::subtitle_one_line`].
    subtitle_one_line: bool,
    /// The radio of a radio row, set by [`radio_row`].
    radio: Option<RadioChoice>,
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
        radio: None,
    }
}

impl Row {
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

    pub fn trailing(mut self, trailing: impl Into<Element>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    /// Trailing content when there is any; a row without it has none.
    pub fn maybe_trailing(mut self, trailing: Option<impl Into<Element>>) -> Self {
        self.trailing = trailing.map(Into::into);
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
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

    /// The row is pressed as a whole. Such a row also gets the hover fill and
    /// the pointer cursor, as it opens something.
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
        self.trailing(ValueText { text: value })
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

/// A read-only value on the right of a row. It is selectable: Ctrl+C copies
/// it, and a right click offers Copy. It stays on one line. It fills the
/// half of the row that `Row` gives a value, aligned to its end, and a long
/// value is cut there with an ellipsis.
#[derive(PartialEq)]
struct ValueText {
    text: String,
}

impl Component for ValueText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let pressable = use_in_pressable_row();
        // A selection would take the press that opens a pressable row, so
        // there the value is plain text; elsewhere it can be selected.
        let value: Element = if pressable {
            VALUE
                .label(&tokens, self.text.clone())
                .max_lines(1)
                .line_height(VALUE.leading)
                .text_align(TextAlign::End)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Size::fill())
                .into()
        } else {
            SelectableText::new()
                .span(VALUE.span(&tokens, self.text.clone()))
                .max_lines(1)
                .line_height(VALUE.leading)
                .text_align(TextAlign::End)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Size::fill())
                .into()
        };
        let text = self.text.clone();
        rect()
            .tw(&tokens, &[w_flex()])
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let text = text.clone();
                let items = vec![menu_item("Copy", move |_| {
                    if let Some(overlays) = overlays {
                        copy_text(overlays, text.clone());
                    }
                })];
                open_row_menu(overlays, &items, &event);
            })
            .child(value)
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
        let generated_id = use_a11y();
        let radio = self.radio.as_ref();
        let a11y_id = radio
            .and_then(|choice| choice.a11y_id)
            .unwrap_or(generated_id);
        let focus = use_focus(a11y_id);
        provide_enabled(self.enabled);
        let interactive = self.on_press.is_some() && self.enabled;
        let focusable = interactive && radio.is_none_or(|choice| choice.tab_stop);
        let mut pressable = use_provide_context(|| RowPressable(State::create(interactive)));
        pressable.0.set_if_modified(interactive);
        // Only a row that opens something, such as one with a chevron, lights
        // on hover and press. A switch, slider or value row does not.
        let activatable = interactive && self.activatable;
        let overlays = try_consume_context::<Overlays>();
        let menu = self.context_menu.clone();
        use_row_lit((activatable && (hovering() || pressing())) || self.selected);
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
        let reveal = use_tween(if nested_shown { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        let nested_hit = !nested_matched.read().is_empty();
        use_row_match(row_hit || nested_hit);
        let single_line = self.subtitle.is_none();
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
            None => search_highlight::label(BODY.in_ink(ink), &tokens, &self.title),
        };
        // On a phone a long title wraps onto a second line rather than
        // being cut, as a narrow row has little room beside its control.
        let wrapped = touch() && self.badge.is_none() && self.font.is_none();
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
                None => match search_highlight::marked(CAPTION, &tokens, subtitle) {
                    Some(marked) => marked
                        .max_lines(subtitle_lines)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .into(),
                    // A selectable text takes the pointer-down that a press
                    // starts from, so a pressable row keeps its text plain.
                    None if !interactive => CAPTION
                        .selectable(&tokens, subtitle.clone())
                        .max_lines(subtitle_lines)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .into(),
                    None => CAPTION
                        .paragraph(&tokens, subtitle.clone())
                        .max_lines(subtitle_lines)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .into(),
                },
            });
        let text_column = rect()
            .tw(&tokens, ROW_TEXT)
            .child(title_line)
            .maybe_child(subtitle_line);
        let trailing = (self.trailing.is_some() || self.selected || self.navigates).then(|| {
            rect()
                .tw(&tokens, ROW_TRAILING)
                // A value takes the same share of the row as the title and
                // fills it, so a long value is cut before the chevron. Any
                // other trailing control keeps its own width. Freya's layout
                // cannot shrink an overflowing child, so a cap on this area
                // would push a chevron or a dropdown past the box edge.
                .maybe(self.value_trailing, |element| {
                    element.tw(&tokens, &[w_flex(), grow_children(), justify(Align::End)])
                })
                .maybe_child(self.trailing.clone())
                .maybe_child((self.selected && !self.navigates).then(|| {
                    icons::icon(Icon::Check, IconSize::Mark, Paint::Accent.color(&tokens))
                }))
                .maybe_child(self.navigates.then(|| {
                    icons::icon(Icon::GoNext, IconSize::Mark, Paint::Muted.color(&tokens))
                }))
        });

        let on_press = self.on_press.clone();
        let face = rect()
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
            .tw(&tokens, if single_line { ROW } else { ROW_TALL })
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    activatable && hovering(),
                    activatable && pressing(),
                    self.selected,
                )],
            )
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if open_row_menu(overlays, &menu, &event) {
                    return;
                }
                if activatable {
                    pressing.set(true);
                }
            })
            .maybe(interactive, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |_: Event<PressEventData>| {
                        if let Some(on_press) = &on_press {
                            on_press.call(());
                        }
                        a11y_id.request_focus();
                    })
            })
            .maybe_child(radio.map(|choice| {
                rect()
                    .opacity(if self.enabled { 1.0 } else { DISABLED_OPACITY })
                    .child(RadioMark {
                        selected: choice.selected,
                        hover: hovering() && self.enabled,
                        press: pressing() && self.enabled,
                    })
            }))
            .maybe_child({
                let leading: Option<Element> = match self.leading {
                    Some(leading) => {
                        let glyph = icons::icon(leading, IconSize::Row, icon_paint.color(&tokens));
                        // With a subtitle, the icon sits on the title line at
                        // the top instead of between the two lines. The column
                        // spans the row's height so its first child is at the top.
                        Some(if single_line {
                            glyph.into()
                        } else {
                            rect()
                                .tw(&tokens, &[h_full(), flex_col(), pt(S0_5)])
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
                leading
            })
            .maybe_child(
                self.leading_signal
                    .map(|(level, secured)| crate::controls::signal_bars(level, secured)),
            )
            .maybe_child(
                self.leading_avatar
                    .as_ref()
                    .map(|user| crate::controls::avatar(user.clone(), Dim::Box9)),
            )
            .maybe_child(
                self.leading_app
                    .as_ref()
                    .map(|name| app_icon(name.clone(), IconSize::App)),
            )
            .child(text_column)
            .maybe_child(trailing)
            .focusable(focus(), Radius::Control)
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
            .tw(&tokens, &[flex_col(), w_full()])
            .child(face)
            .maybe_child((reveal > 0.0).then(|| {
                Element::from(NestedRows {
                    rows: rows.clone(),
                    lit: nested_lit,
                    filter,
                    opacity: reveal,
                })
            }))
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
    opacity: f32,
}

impl Component for NestedRows {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = use_state(|| 0usize);
        use_provide_context(move || MatchSink(count));
        rect()
            .tw(&tokens, &[flex_col(), w_full(), pl(GUTTER)])
            .opacity(self.opacity)
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
    /// boost. It shows only with a mark, and once the track is measured.
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
        let compact = touch();
        // The track's measured width places the boost label under its tick.
        let mut track_width = use_state(|| 0.0_f32);

        let title = search_highlight::label(BODY.in_ink(ink), &tokens, &self.title);
        let subtitle: Option<Element> = self.subtitle.as_ref().map(|subtitle| -> Element {
            match search_highlight::marked(CAPTION, &tokens, subtitle) {
                Some(marked) => marked
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into(),
                // A level row has no press, so its subtitle can be selected.
                None => CAPTION
                    .selectable(&tokens, subtitle.clone())
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
            (Some(point), Some(text)) if measured > 0.0 => {
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
            .tw(&tokens, &[w_full(), px(GUTTER), py(S3)])
            .on_pointer_down(move |event: Event<PointerEventData>| {
                open_row_menu(overlays, &menu, &event);
            });
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
    row.trailing(ValueText { text: value.into() })
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
            .tw(&tokens, &[flex_col(), w_full()])
            .child(header)
            .maybe_child((progress > 0.0).then(|| {
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), pl(GUTTER)])
                    .opacity(progress)
                    .children(children)
            }))
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
