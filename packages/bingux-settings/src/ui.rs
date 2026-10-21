//! Design-system components: boxed groups, rows, status blocks and buttons.
//!
//! Sizes follow `shell/bingux/Theme.qml` and the Control Centre: 48 px rows,
//! 64 px rows with a subtitle, 12 px corners, 1 px outlines. Colours come
//! from [`Tokens`], provided as context by the window root.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::theme::{SharedTokens, Tokens};

pub use crate::theme::{
    HEADER_HEIGHT, RADIUS as CORNER, SIDEBAR_OVERLAY_BELOW as NARROW_BREAKPOINT, SIDEBAR_WIDTH,
};

pub const PAGE_WIDTH: f32 = 600.0;
pub const ROW_HEIGHT: f32 = 48.0;
pub const TALL_ROW_HEIGHT: f32 = 64.0;
pub const PAGE_PADDING: f32 = 24.0;

/// The live palette the window root provided. Reading it subscribes the
/// component to accent and colour-scheme changes.
pub fn use_tokens() -> Tokens {
    *use_consume::<SharedTokens>().0.read()
}

fn outline(tokens: &Tokens) -> Border {
    Border::new()
        .width(1.0)
        .alignment(BorderAlignment::Inner)
        .fill(tokens.outline)
}

/// Large page heading at the top of the page column.
pub fn page_title(text: impl Into<String>) -> Element {
    let text = text.into();
    label()
        .text(text)
        .font_size(26.0)
        .font_weight(FontWeight::BOLD)
        .margin((0.0, 0.0, 6.0, 0.0))
        .into()
}

/// A short explanation under a page title or group.
pub fn caption(text: impl Into<String>) -> Element {
    let text = text.into();
    CaptionText { text }.into()
}

#[derive(PartialEq)]
struct CaptionText {
    text: String,
}

impl Component for CaptionText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        paragraph()
            .color(tokens.muted)
            .font_size(13.0)
            .margin((0.0, 0.0, 12.0, 0.0))
            .child(self.text.clone())
    }
}

/// A boxed group of rows with an optional title above it.
#[derive(PartialEq)]
pub struct Group {
    title: Option<String>,
    rows: Vec<Element>,
}

impl Group {
    pub fn new(title: impl Into<Option<String>>) -> Self {
        Self {
            title: title.into(),
            rows: Vec::new(),
        }
    }

    pub fn titled(title: &str) -> Self {
        Self::new(Some(title.to_string()))
    }

    pub fn row(mut self, row: impl Into<Element>) -> Self {
        self.rows.push(row.into());
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = Element>) -> Self {
        self.rows.extend(rows);
        self
    }
}

impl Component for Group {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut children = Vec::with_capacity(self.rows.len() * 2);
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                children.push(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .background(tokens.outline)
                        .into(),
                );
            }
            children.push(row.clone());
        }
        rect()
            .width(Size::fill())
            .margin((0.0, 0.0, 20.0, 0.0))
            .maybe_child(self.title.as_ref().map(|title| {
                label()
                    .text(title.clone())
                    .font_size(15.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .margin((0.0, 0.0, 8.0, 4.0))
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .background(tokens.elevated)
                    .border(outline(&tokens))
                    .corner_radius(CORNER)
                    .overflow(Overflow::Clip)
                    .children(children),
            )
    }
}

/// One row: a title, an optional subtitle, and a trailing control.
///
/// Rows with a subtitle are 64 px tall, others 48 px. A row with `on_press`
/// highlights on hover and acts like a button; the whole row is the target.
#[derive(PartialEq)]
pub struct Row {
    title: String,
    subtitle: Option<String>,
    trailing: Option<Element>,
    leading: Option<Icon>,
    on_press: Option<EventHandler<()>>,
    enabled: bool,
    navigates: bool,
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

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A muted value on the right, for read-only rows.
    pub fn value(self, value: impl Into<String>) -> Self {
        let value = value.into();
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
            .font_size(14.0)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .max_width(Size::px(360.0))
    }
}

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let interactive = self.on_press.is_some() && self.enabled;
        let height = if self.subtitle.is_some() {
            TALL_ROW_HEIGHT
        } else {
            ROW_HEIGHT
        };
        let background = if interactive && hovering() {
            tokens.hover
        } else {
            Color::TRANSPARENT
        };
        let text_color = tokens.text.mul_if(!self.enabled, 0.6);

        let text_column = rect()
            .width(Size::flex(1.0))
            .main_align(Alignment::Center)
            .spacing(2.0)
            .child(
                label()
                    .text(self.title.clone())
                    .color(text_color)
                    .font_size(15.0)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(self.subtitle.as_ref().map(|subtitle| {
                label()
                    .text(subtitle.clone())
                    .color(tokens.muted.mul_if(!self.enabled, 0.6))
                    .font_size(13.0)
                    .max_lines(2)
                    .text_overflow(TextOverflow::Ellipsis)
            }));

        let on_press = self.on_press.clone();
        rect()
            .width(Size::fill())
            .height(Size::px(height))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((0.0, 16.0))
            .spacing(12.0)
            .background(background)
            .a11y_role(if interactive {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::GenericContainer
            })
            .maybe(interactive, |element| {
                element
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| hovering.set(false))
                    .on_press(move |_: Event<PressEventData>| {
                        if let Some(on_press) = &on_press {
                            on_press.call(());
                        }
                    })
            })
            .maybe_child(
                self.leading
                    .map(|leading| icons::icon(leading, 18.0, text_color)),
            )
            .child(text_column)
            .maybe_child(self.trailing.clone())
            .maybe_child(
                self.navigates
                    .then(|| icons::icon(Icon::GoNext, 16.0, tokens.muted)),
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
        .trailing(
            Switch::new()
                .toggled(on)
                .on_toggle(move |_| on_toggle.call(!on)),
        )
        .on_press(move |_| press.call(!on))
}

/// A row with a slider on the right. `value` and the callback use 0 to 100.
pub fn slider_row(
    title: impl Into<String>,
    value: f64,
    on_moved: impl Into<EventHandler<f64>>,
) -> Row {
    let on_moved = on_moved.into();
    row(title).trailing(
        rect().width(Size::px(220.0)).child(
            Slider::new(move |value: f64| on_moved.call(value))
                .value(value)
                .size(Size::fill()),
        ),
    )
}

/// A row with a dropdown. `options` are `(key, label)` pairs.
pub fn choice_row(
    title: impl Into<String>,
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Row {
    let on_select = on_select.into();
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

/// The kind of status block to show in a page.
#[derive(Clone, PartialEq)]
pub enum Status {
    Loading(String),
    Empty(String),
    Error(String),
    Info(String),
}

/// A full-width status block: loading, empty, error or informational.
#[derive(PartialEq)]
pub struct StatusBlock {
    status: Status,
    action: Option<(String, EventHandler<()>)>,
}

pub fn status(status: Status) -> StatusBlock {
    StatusBlock {
        status,
        action: None,
    }
}

impl StatusBlock {
    /// A button under the message, for retries and similar.
    pub fn action(mut self, label: &str, on_press: impl Into<EventHandler<()>>) -> Self {
        self.action = Some((label.to_string(), on_press.into()));
        self
    }
}

impl Component for StatusBlock {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (text, color) = match &self.status {
            Status::Loading(text) => (text.clone(), tokens.muted),
            Status::Empty(text) => (text.clone(), tokens.muted),
            Status::Error(text) => (text.clone(), tokens.danger),
            Status::Info(text) => (text.clone(), tokens.text),
        };
        let border_color = match &self.status {
            Status::Error(_) => tokens.danger.with_a(110),
            _ => tokens.outline,
        };
        rect()
            .width(Size::fill())
            .padding(16.0)
            .margin((0.0, 0.0, 20.0, 0.0))
            .spacing(12.0)
            .background(tokens.elevated)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .corner_radius(CORNER)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .maybe_child(
                matches!(self.status, Status::Loading(_)).then(|| CircularLoader::new().size(18.0)),
            )
            .child(
                paragraph()
                    .width(Size::flex(1.0))
                    .color(color)
                    .font_size(14.0)
                    .child(text),
            )
            .maybe_child(self.action.as_ref().map(|(label, on_press)| {
                let on_press = on_press.clone();
                Button::new()
                    .on_press(move |_| on_press.call(()))
                    .child(label.clone())
            }))
    }
}

/// An accent-filled button for the page's primary action.
pub fn primary_button(text: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> Button {
    let on_press = on_press.into();
    let text = text.into();
    Button::new()
        .style_variant(ButtonStyleVariant::Filled)
        .on_press(move |_| on_press.call(()))
        .child(text)
}

/// A plain button for secondary actions inside rows.
pub fn row_button(text: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> Button {
    let on_press = on_press.into();
    let text = text.into();
    Button::new()
        .compact()
        .on_press(move |_| on_press.call(()))
        .child(text)
}

/// A square icon button for header bars and row actions.
#[derive(PartialEq)]
pub struct IconButton {
    icon: Icon,
    size: f32,
    tooltip: Option<String>,
    on_press: EventHandler<()>,
    danger: bool,
}

pub fn icon_button(icon: Icon, on_press: impl Into<EventHandler<()>>) -> IconButton {
    IconButton {
        icon,
        size: 32.0,
        tooltip: None,
        on_press: on_press.into(),
        danger: false,
    }
}

impl IconButton {
    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_string());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Hover fills with the danger colour, for close buttons.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

impl Component for IconButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();
        let background = if hovering() {
            if self.danger {
                tokens.danger.with_a(200)
            } else {
                tokens.hover
            }
        } else {
            Color::TRANSPARENT
        };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(2.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.accent)
        } else {
            Border::new()
        };
        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .maybe(self.tooltip.is_some(), |element| {
                element.a11y_alt(self.tooltip.clone().unwrap_or_default())
            })
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .center()
            .corner_radius(self.size / 2.0)
            .background(background)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
                a11y_id.request_focus();
            })
            .child(icons::icon(self.icon, self.size * 0.5, tokens.text));
        match &self.tooltip {
            Some(text) => TooltipContainer::new(Tooltip::new_text(text.clone()))
                .child(button)
                .into(),
            None => Element::from(button),
        }
    }
}

/// A group-style heading for content that is not a boxed group, such as a
/// status block. Looks the same as a `Group` title.
pub fn section_label(text: impl Into<String>) -> Element {
    let text = text.into();
    SectionLabel { text }.into()
}

#[derive(PartialEq)]
struct SectionLabel {
    text: String,
}

impl Component for SectionLabel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        label()
            .text(self.text.clone())
            .color(tokens.text)
            .font_size(15.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .margin((0.0, 0.0, 8.0, 4.0))
    }
}
