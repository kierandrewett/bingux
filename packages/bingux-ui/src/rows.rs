//! Rows: the building block of every preference list.
//!
//! Mirrors `ControlRow.qml`: a leading icon, a title, an optional subtitle
//! and trailing content. Convenience constructors pair a row with a
//! switch, slider, choice or text field.

use freya::animation::*;
use freya::prelude::*;

use crate::controls::dropdown::dropdown;
use crate::controls::segmented::segmented;
use crate::controls::slider::slider;
use crate::controls::switch::switch;
use crate::controls::text_field::text_field;
use crate::icons::{self, Icon};
use crate::surface::{
    FONT_SIZE, FONT_SMALL, ROW_HEIGHT, ROW_PADDING, TALL_ROW_HEIGHT, focus_border, surface_color,
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
    badge: Option<String>,
    destructive: bool,
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
        label()
            .text(self.text.clone())
            .color(tokens.muted)
            .font_size(FONT_SIZE)
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
            false,
        );
        let text_color = if !self.enabled {
            tokens.muted
        } else if self.destructive {
            tokens.danger
        } else {
            tokens.text
        };
        let icon_color = tokens.muted;

        let title = label()
            .text(self.title.clone())
            .color(text_color)
            .font_size(FONT_SIZE)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        let title_line: Element = match &self.badge {
            None => title.into(),
            Some(badge) => rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(8.0)
                .child(title)
                .child(
                    rect()
                        .height(Size::px(18.0))
                        .padding((0.0, 7.0))
                        .corner_radius(9.0)
                        .background(tokens.accent.with_af32(0.18))
                        .center()
                        .child(
                            label()
                                .text(badge.clone())
                                .color(tokens.accent)
                                .font_size(11.0)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .max_lines(1),
                        ),
                )
                .into(),
        };
        // Freya centres children against the content height, not against
        // `min_height`, so the column carries the row's inner height itself.
        let text_column = rect()
            .width(Size::flex(1.0))
            .height(Size::px(height - 16.0))
            .main_align(Alignment::Center)
            .spacing(1.0)
            .child(title_line)
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
            .min_height(Size::px(height))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((8.0, ROW_PADDING))
            .spacing(12.0)
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
    // One option is a fact, not a choice.
    if let [(_, only)] = options.as_slice() {
        return row(title).value(only.clone());
    }
    let short = options.len() <= 3 && options.iter().all(|(_, label)| label.chars().count() <= 12);
    if short {
        return row(title).trailing(segmented(options, selected, on_select));
    }
    row(title).trailing(dropdown(options, selected, on_select))
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
        paragraph()
            .width(Size::percent(55.0))
            .color(tokens.muted)
            .font_size(FONT_SIZE)
            .text_align(TextAlign::End)
            .child(self.text.clone())
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
            .width(260.0)
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
                .child(icons::icon(Icon::GoNext, 12.0, tokens.muted));
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
                        .margin((0.0, 0.0, 0.0, 12.0))
                        .background(tokens.text.with_af32(0.07))
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
                    .padding((0.0, 0.0, 0.0, 16.0))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.0))
                            .background(tokens.text.with_af32(0.07)),
                    )
                    .children(children)
            }))
    }
}
