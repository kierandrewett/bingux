//! Preference groups, captions, headings and status blocks.

use freya::prelude::*;

use crate::controls::button::action_button;
use crate::surface::{CORNER, FONT_HEADING, FONT_SIZE, FONT_SMALL, TALL_ROW_HEIGHT, use_tokens};

/// A short muted explanation under a title or group.
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
            .font_size(FONT_SMALL)
            .margin((0.0, 16.0, 8.0, 16.0))
            .child(self.text.clone())
    }
}

/// A group of rows on one card with an optional name and description
/// above it, like a libadwaita preferences group with macOS inset
/// separators between rows.
#[derive(PartialEq)]
pub struct Group {
    title: Option<String>,
    description: Option<String>,
    rows: Vec<Element>,
}

impl Group {
    pub fn new(title: impl Into<Option<String>>) -> Self {
        Self {
            title: title.into(),
            description: None,
            rows: Vec::new(),
        }
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.description = (!text.is_empty()).then_some(text);
        self
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
                        .margin((0.0, 0.0, 0.0, 12.0))
                        .background(tokens.text.with_af32(0.07))
                        .into(),
                );
            }
            children.push(row.clone());
        }
        rect()
            .width(Size::fill())
            .margin((0.0, 0.0, 20.0, 0.0))
            .maybe_child(
                self.title
                    .as_ref()
                    .map(|title| section_label(title.clone())),
            )
            .maybe_child(self.description.as_ref().map(|text| caption(text.clone())))
            .child(
                rect()
                    .width(Size::fill())
                    .background(tokens.card)
                    .corner_radius(CORNER)
                    .padding(4.0)
                    .children(children),
            )
    }
}

/// A group name: small, muted, aligned with the row text.
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
            .font_size(13.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .margin((0.0, 0.0, 6.0, 16.0))
    }
}

/// The kind of status block to show in a page.
#[derive(Clone, PartialEq)]
pub enum Status {
    Loading(String),
    Empty(String),
    Error(String),
    Info(String),
}

/// A short status on a group surface: loading, empty, error or informational.
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
    /// A button beside the message, for retries and similar.
    pub fn action(mut self, label: &str, on_press: impl Into<EventHandler<()>>) -> Self {
        self.action = Some((label.to_string(), on_press.into()));
        self
    }
}

impl Component for StatusBlock {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (text, color) = match &self.status {
            Status::Loading(text) | Status::Empty(text) => (text.clone(), tokens.muted),
            Status::Error(text) => (text.clone(), tokens.danger),
            Status::Info(text) => (text.clone(), tokens.text),
        };
        rect()
            .width(Size::fill())
            .min_height(Size::px(TALL_ROW_HEIGHT))
            .padding((8.0, 16.0))
            .margin((0.0, 0.0, 16.0, 0.0))
            .spacing(12.0)
            .background(tokens.card)
            .corner_radius(CORNER)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .maybe_child(
                matches!(self.status, Status::Loading(_)).then(|| CircularLoader::new().size(16.0)),
            )
            .child(
                paragraph()
                    .width(Size::flex(1.0))
                    .color(color)
                    .font_size(FONT_SIZE)
                    .child(text),
            )
            .maybe_child(self.action.as_ref().map(|(label, on_press)| {
                let on_press = on_press.clone();
                action_button(label.clone(), move |_| on_press.call(()))
            }))
    }
}

/// A heading-sized label for in-page headers such as a details view.
pub fn heading(text: impl Into<String>) -> Element {
    let text = text.into();
    label()
        .text(text)
        .font_size(FONT_HEADING)
        .font_weight(FontWeight::SEMI_BOLD)
        .into()
}
