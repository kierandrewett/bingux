//! Preference groups, captions, headings and status blocks.
//!
//! A group is a card, as a `ControlCentreDetails.qml` section: a muted name
//! above, an optional muted description, then rows set flush against one
//! card surface with hairlines between them.

use freya::prelude::*;

use crate::controls::button::action_button;
use crate::controls::progress::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::layout::page_column;
use crate::scale::*;
use crate::surface::{over, use_tokens};
use crate::theme::Tokens;

/// A form's action row: an optional muted note at the left and the action
/// at the right, on the row height and the gutter. Put it inside a group
/// as the last row, or under the last group on a page.
pub fn action_row(note: Option<&str>, action: impl IntoElement) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(ROW_HEIGHT))
        .padding((0.0, GUTTER))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .content(Content::Flex)
        .child(rect().width(Size::flex(1.0)).maybe_child(note.map(caption)))
        .child(action)
        .into()
}

/// A short muted explanation under a title or group.
pub fn caption(text: impl Into<String>) -> Element {
    let text = text.into();
    CaptionText { text }.into()
}

/// A caption centred under an icon and a name, at the head of a subpage.
pub fn centered_caption(text: impl Into<String>) -> Element {
    let text = text.into();
    CenteredCaption { text }.into()
}

#[derive(PartialEq)]
struct CenteredCaption {
    text: String,
}

impl Component for CenteredCaption {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        paragraph()
            .width(Size::fill())
            .text_align(TextAlign::Center)
            .color(tokens.muted)
            .font_size(TEXT_SM)
            .child(self.text.clone())
    }
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

/// A group of rows on one card, with an optional name and description above
/// it. Rows sit flush against the card, with no inset around their hover
/// layer; a hairline inset by the gutter divides them.
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
        let children = card_rows(&tokens, self.rows.clone());
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
                        .spacing(S0)
                        .maybe_child(
                            self.title
                                .as_ref()
                                .map(|title| section_label(title.clone())),
                        )
                        .maybe_child(self.description.as_ref().map(|text| caption(text.clone()))),
                )
                .maybe_child(self.trailing.clone())
        });
        // The page column owns the space between groups; the name sits
        // a small step above its card.
        rect()
            .width(Size::fill())
            .spacing(S1)
            .maybe_child(heading)
            .child(card_surface(&tokens).children(children))
    }
}

/// The card every group and skeleton section sits on: the card colour, a
/// 12 px radius, and content clipped to that radius so flush rows follow it.
fn card_surface(tokens: &Tokens) -> Rect {
    rect()
        .width(Size::fill())
        .corner_radius(RADIUS_LG)
        .background(tokens.card)
        .overflow(Overflow::Clip)
}

/// Rows with a hairline between each pair, inset by the gutter on the left.
fn card_rows(tokens: &Tokens, rows: Vec<Element>) -> Vec<Element> {
    let mut children = Vec::with_capacity(rows.len() * 2);
    for (index, row) in rows.into_iter().enumerate() {
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
        children.push(row);
    }
    children
}

/// A group name: small and muted, as the section titles in
/// `ControlCentreDetails.qml`, aligned with the row text.
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
            .color(tokens.muted)
            .font_size(TEXT_SM)
            .font_weight(FontWeight::MEDIUM)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
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

/// A short status on a card: loading, empty, error or informational.
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
        // Only an error is tinted; it is a danger layer over the card.
        let background = if matches!(self.status, Status::Error(_)) {
            over(tokens.danger.with_af32(0.12), tokens.card)
        } else {
            tokens.card
        };
        let short = text.chars().count() <= 70;
        // A status has the metrics of a row. A long message wraps and grows
        // the block.
        rect()
            .width(Size::fill())
            .maybe(short, |card| card.height(Size::px(ROW_HEIGHT)))
            .maybe(!short, |card| card.min_height(Size::px(ROW_HEIGHT)))
            .padding((S2, GUTTER))
            .spacing(S3)
            .background(background)
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

/// The shape of a page before its data has arrived: two sections of quiet
/// bars where the titles and rows will be. It carries no text, so nothing
/// reads as "loading", and the real page replaces it in place.
pub fn skeleton() -> Element {
    Skeleton.into()
}

const SKELETON_TITLE_WIDTH: f32 = 120.0;
const SKELETON_LABEL_WIDTH: f32 = 180.0;
const SKELETON_VALUE_WIDTH: f32 = 96.0;
const SKELETON_ROWS: usize = 3;

#[derive(PartialEq)]
struct Skeleton;

impl Component for Skeleton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let bar = |width: f32| {
            rect()
                .width(Size::px(width))
                .height(Size::px(TEXT_SM))
                .corner_radius(RADIUS_SM)
                .background(tokens.elevated)
        };
        let section = || {
            let rows = (0..SKELETON_ROWS).map(|_| {
                Element::from(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(ROW_HEIGHT))
                        .padding((0.0, GUTTER))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .child(bar(SKELETON_LABEL_WIDTH)),
                        )
                        .child(bar(SKELETON_VALUE_WIDTH)),
                )
            });
            rect()
                .width(Size::fill())
                .spacing(S1)
                .child(
                    rect()
                        .padding((0.0, GUTTER))
                        .child(bar(SKELETON_TITLE_WIDTH)),
                )
                .child(card_surface(&tokens).children(card_rows(&tokens, rows.collect())))
        };
        page_column().child(section()).child(section())
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
