//! Preference groups, captions, headings and status blocks.

use freya::prelude::*;

use crate::controls::button::action_button;
use crate::controls::progress::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::use_tokens;

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
            .font_size(TEXT_SM)
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
    trailing: Option<Element>,
    rows: Vec<Element>,
}

impl Group {
    pub fn new(title: impl Into<Option<String>>) -> Self {
        Self {
            title: title.into(),
            description: None,
            trailing: None,
            rows: Vec::new(),
        }
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.description = (!text.is_empty()).then_some(text);
        self
    }

    /// An element at the right end of the title line, such as a refresh
    /// button.
    pub fn trailing(mut self, element: impl Into<Element>) -> Self {
        self.trailing = Some(element.into());
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
                        .background(tokens.separator.with_af32(0.045))
                        .into(),
                );
            }
            children.push(row.clone());
        }
        // A flat section: a bold title, an optional muted description, a
        // hairline, then rows with no surface behind them. The page column
        // owns the space between sections.
        let heading = (self.title.is_some() || self.description.is_some()).then(|| {
            rect()
                .width(Size::fill())
                .padding((0.0, GUTTER))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .spacing(S1)
                        .maybe_child(
                            self.title
                                .as_ref()
                                .map(|title| section_label(title.clone())),
                        )
                        .maybe_child(self.description.as_ref().map(|text| caption(text.clone()))),
                )
                .maybe_child(self.trailing.clone())
        });
        rect()
            .width(Size::fill())
            .maybe_child(heading)
            .maybe_child(
                (self.title.is_some() || self.description.is_some()).then(|| {
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin((S3, 0.0, S1, 0.0))
                        .background(tokens.separator)
                }),
            )
            .child(rect().width(Size::fill()).children(children))
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
            .font_size(TEXT_LG)
            .font_weight(FontWeight::SEMI_BOLD)
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
        let (text, color, glyph) = match &self.status {
            Status::Loading(text) => (text.clone(), tokens.muted, None),
            Status::Empty(text) => (text.clone(), tokens.muted, None),
            Status::Error(text) => (text.clone(), tokens.danger, Some(Icon::DialogError)),
            Status::Info(text) => (text.clone(), tokens.text, None),
        };
        let short = text.chars().count() <= 70;
        // A status is a row with the height of every other row. Only an
        // error is tinted. A long message wraps and grows the row.
        rect()
            .width(Size::fill())
            .maybe(short, |card| card.height(Size::px(ROW_HEIGHT)))
            .maybe(!short, |card| card.min_height(Size::px(ROW_HEIGHT)))
            .padding((S2, GUTTER))
            .spacing(S3)
            .map(
                matches!(self.status, Status::Error(_)).then_some(tokens.danger.with_af32(0.12)),
                |card, tint| card.background(tint),
            )
            .corner_radius(RADIUS_LG)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .maybe_child(
                matches!(self.status, Status::Loading(_)).then(|| spinner(SpinnerSize::Small)),
            )
            .maybe_child(glyph.map(|glyph| icons::icon(glyph, ICON_LG, color)))
            .child(
                paragraph()
                    .width(Size::flex(1.0))
                    .color(color)
                    .font_size(TEXT_MD)
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
        .font_size(TEXT_LG)
        .font_weight(FontWeight::SEMI_BOLD)
        .into()
}
