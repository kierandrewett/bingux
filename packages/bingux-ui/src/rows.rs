//! Rows: the building block of every preference list.
//!
//! Mirrors `ControlRow.qml`: a leading icon, a title, an optional subtitle
//! and trailing content. Convenience constructors pair a row with a
//! switch, slider, choice or text field.

use freya::animation::*;
use freya::prelude::*;

use crate::app_icon::app_icon;
use crate::controls::dropdown::dropdown;
use crate::controls::segmented::segmented;
use crate::controls::slider::slider;
use crate::controls::switch::switch;
use crate::controls::text_field::text_field;
use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{focus_border, provide_enabled, surface_color, use_tokens};
use crate::typography::{BADGE, BODY, CAPTION, Ink, VALUE};

/// The widest a read-only value gets before it is cut with an ellipsis.
const VALUE_MAX_WIDTH: f32 = 340.0;

/// One row: optional leading icon, title, optional subtitle, and trailing
/// content. The whole row is the target when `on_press` is set.
#[derive(PartialEq)]
pub struct Row {
    title: String,
    subtitle: Option<String>,
    trailing: Option<Element>,
    leading: Option<Icon>,
    leading_app: Option<String>,
    leading_signal: Option<(u8, bool)>,
    on_press: Option<EventHandler<()>>,
    enabled: bool,
    navigates: bool,
    selected: bool,
    badge: Option<String>,
    destructive: bool,
}

pub fn row(title: impl Into<String>) -> Row {
    Row {
        title: title.into(),
        subtitle: None,
        trailing: None,
        leading: None,
        leading_app: None,
        leading_signal: None,
        on_press: None,
        enabled: true,
        navigates: false,
        selected: false,
        badge: None,
        destructive: false,
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

    /// Trailing content when there is any; a row without it has none.
    pub fn maybe_trailing(mut self, trailing: Option<impl Into<Element>>) -> Self {
        self.trailing = trailing.map(Into::into);
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// A full-colour application icon, by icon name, in place of a glyph.
    /// Four signal bars at the start, `level` 0..=4.
    pub fn leading_signal(mut self, level: u8, secured: bool) -> Self {
        self.leading_signal = Some((level, secured));
        self
    }

    pub fn leading_app(mut self, icon_name: impl Into<String>) -> Self {
        self.leading_app = Some(icon_name.into());
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
        VALUE
            .label(&tokens, self.text.clone())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .max_width(Size::px(VALUE_MAX_WIDTH))
    }
}

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        provide_enabled(self.enabled);
        let interactive = self.on_press.is_some() && self.enabled;
        let single_line = self.subtitle.is_none();
        let height = row_height();
        let background = surface_color(
            &tokens,
            Color::TRANSPARENT,
            interactive && hovering(),
            interactive && pressing(),
            self.selected,
        );
        let ink = if !self.enabled {
            Ink::Muted
        } else if self.destructive {
            Ink::Danger
        } else {
            Ink::Text
        };
        let icon_color = if self.selected {
            tokens.accent
        } else {
            tokens.muted
        };

        let title = BODY
            .in_ink(ink)
            .label(&tokens, self.title.clone())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        let title_line: Element = match &self.badge {
            None => title.into(),
            Some(badge) => rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(S2)
                .child(title)
                .child(
                    rect()
                        .height(Size::px(BOX_SM))
                        .padding((0.0, S2))
                        .corner_radius(RADIUS)
                        .background(tokens.accent.with_af32(0.18))
                        .center()
                        .child(BADGE.label(&tokens, badge.clone()).max_lines(1)),
                )
                .into(),
        };
        // A subtitle wraps onto a second or third line, so a row that has one
        // grows to fit it. A row without one has the row height, and its
        // text is centred by the row.
        let text_column = rect()
            .width(Size::flex(1.0))
            .spacing(S0)
            .child(title_line)
            .maybe_child(self.subtitle.as_ref().map(|subtitle| {
                CAPTION
                    .paragraph(&tokens, subtitle.clone())
                    .max_lines(3)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::fill())
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
            .maybe(single_line, |row| row.height(Size::px(height)))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((if single_line { 0.0 } else { S3 + S0 }, GUTTER))
            .spacing(S3)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .maybe(interactive, |element| {
                element
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
                    .map(|leading| icons::icon(leading, ICON_LG, icon_color)),
            )
            .maybe_child(
                self.leading_signal
                    .map(|(level, secured)| crate::controls::signal_bars(level, secured)),
            )
            .maybe_child(
                self.leading_app
                    .as_ref()
                    .map(|name| app_icon(name.clone(), ICON_APP)),
            )
            .child(text_column)
            .maybe_child(self.trailing.clone())
            .maybe_child(
                (self.selected && !self.navigates)
                    .then(|| icons::icon(Icon::Check, ICON_SM, tokens.accent)),
            )
            .maybe_child(
                self.navigates
                    .then(|| icons::icon(Icon::GoNext, ICON_SM, tokens.muted)),
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

/// A level row for a setting: the title and its value over a full-width
/// slider. `value` and the callback use 0 to 100.
pub fn slider_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> LevelRow {
    level_row(title, value, on_moved)
}

/// A level row: the title and its value on one line, with a slider as wide
/// as the row beneath. Use it where the slider is the point of the row, such
/// as a volume. An optional leading element (an icon or a mute button) sits
/// before the text and an optional trailing one after it.
#[derive(PartialEq)]
pub struct LevelRow {
    title: String,
    subtitle: Option<String>,
    leading: Option<Element>,
    trailing: Option<Element>,
    value: f64,
    value_text: Option<String>,
    enabled: bool,
    on_moved: EventHandler<f64>,
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
        trailing: None,
        value: value.clamp(0.0, 100.0),
        value_text: None,
        enabled: true,
        on_moved: on_moved.into(),
    }
}

impl LevelRow {
    /// The text at the right of the title line, such as "500 ms". Without
    /// it the row shows the value as a percentage.
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

    pub fn leading(mut self, leading: impl Into<Element>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<Element>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }
}

impl Component for LevelRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        provide_enabled(self.enabled);
        let ink = if self.enabled { Ink::Text } else { Ink::Muted };
        let heading = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .child(
                rect().width(Size::flex(1.0)).child(
                    BODY.in_ink(ink)
                        .label(&tokens, self.title.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
            .child(
                VALUE.label(
                    &tokens,
                    self.value_text
                        .clone()
                        .unwrap_or_else(|| format!("{}%", self.value.round() as i64)),
                ),
            );
        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((S1, GUTTER))
            .spacing(S4)
            .maybe_child(self.leading.clone())
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .child(heading)
                    .maybe_child(self.subtitle.as_ref().map(|subtitle| {
                        CAPTION
                            .label(&tokens, subtitle.clone())
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                    }))
                    .child(slider(self.value, self.on_moved.clone()).fill()),
            )
            .maybe_child(self.trailing.clone())
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
    row(title).trailing(dropdown(options, selected, on_select).min_width(FIELD_WIDTH))
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
/// pages. Long values wrap onto further lines instead of truncating.
pub fn property_row(title: impl Into<String>, value: impl Into<String>) -> Row {
    let value = value.into();
    row(title).trailing(PropertyValue { text: value })
}

#[derive(PartialEq)]
struct PropertyValue {
    text: String,
}

impl Component for PropertyValue {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        VALUE
            .paragraph(&tokens, self.text.clone())
            .width(Size::percent(55.0))
            .text_align(TextAlign::End)
    }
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
            .width(FIELD_WIDTH)
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
        let motion = tokens.motion(160);
        let reveal = use_animation_transition(if open() { 1.0 } else { 0.0 }, move |from, to| {
            AnimNum::new(from, to)
                .time(motion)
                .function(Function::Cubic)
                .ease(Ease::Out)
        });
        let progress = reveal.get().value();
        let chevron =
            rect()
                .rotate(progress * 90.0)
                .child(icons::icon(Icon::GoNext, ICON_SM, tokens.muted));
        let header = row(self.title.clone())
            .maybe_subtitle(self.subtitle.clone())
            .trailing(chevron)
            .on_press(move |_| open.set(!open()));
        let mut children = Vec::with_capacity(self.children.len() * 2);
        for (index, child) in self.children.iter().enumerate() {
            if index > 0 {
                children.push(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin((0.0, 0.0, 0.0, GUTTER))
                        .background(tokens.separator)
                        .into(),
                );
            }
            children.push(child.clone());
        }
        rect()
            .width(Size::fill())
            .child(header)
            .maybe_child((progress > 0.0).then(|| {
                rect()
                    .width(Size::fill())
                    .opacity(progress)
                    .padding((0.0, 0.0, 0.0, S4))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.0))
                            .background(tokens.separator),
                    )
                    .children(children)
            }))
    }
}

/// How many options a [`choice_list`] draws at once. A list of hundreds of
/// time zones or fonts is narrowed by typing, not drawn whole.
const CHOICE_LIST_LIMIT: usize = 60;

/// A filter field above a card of options with a tick on the chosen one,
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
        let rows = matching.iter().take(CHOICE_LIST_LIMIT).map(|(key, label)| {
            let on_select = self.on_select.clone();
            let chosen = key.clone();
            Element::from(
                row(label.clone())
                    .selected(*key == self.selected)
                    .on_press(move |_| on_select.call(chosen.clone())),
            )
        });
        let mut list = crate::group::Group::new(None::<String>).rows(rows);
        if matching.is_empty() {
            list = list.row(crate::group::status(crate::group::Status::Empty(
                "Nothing matches.".to_string(),
            )));
        }
        rect()
            .width(Size::fill())
            .spacing(S5)
            .child(
                crate::controls::search_field(query, |_| {})
                    .placeholder(self.placeholder.clone())
                    .fill_width(),
            )
            .child(list)
            .maybe_child((hidden > 0).then(|| {
                CAPTION
                    .label(
                        &tokens,
                        format!("{hidden} more. Keep typing to narrow the list."),
                    )
                    .margin((0.0, GUTTER))
            }))
    }
}
