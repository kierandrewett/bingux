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
use crate::search_highlight;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{self, BODY, CAPTION, Ink, SECTION, TITLE};

/// A form's action row: an optional muted note at the left and the action
/// at the right, on the row height and the gutter. Put it inside a group
/// as the last row, or under the last group on a page.
pub fn action_row(note: Option<&str>, action: impl IntoElement) -> Element {
    rect()
        .tw_layout(ROW)
        .child(rect().tw_layout(&[w_flex()]).maybe_child(note.map(caption)))
        .child(action)
        .into()
}

/// A form row for a popup or page: the label at the left and a control at the
/// right, on the row height and the gutter. Put rows in a [`Group`].
pub fn form_row(label: impl Into<String>, control: impl Into<Element>) -> Element {
    FormRow {
        label: label.into(),
        control: control.into(),
    }
    .into()
}

#[derive(PartialEq)]
struct FormRow {
    label: String,
    control: Element,
}

impl Component for FormRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .tw(&tokens, ROW)
            .child(
                rect()
                    .tw(&tokens, &[w_flex()])
                    .child(BODY.label(&tokens, self.label.clone())),
            )
            .child(self.control.clone())
    }
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
        CAPTION
            .selectable(&tokens, self.text.clone())
            .width(Size::fill())
            .text_align(TextAlign::Center)
    }
}

#[derive(PartialEq)]
struct CaptionText {
    text: String,
}

impl Component for CaptionText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        search_highlight::paragraph(CAPTION, &tokens, &self.text)
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
    filtered: bool,
    rows: Vec<Element>,
}

impl Group {
    pub fn new(title: impl Into<Option<String>>) -> Self {
        Self {
            title: title.into(),
            description: None,
            trailing: None,
            filtered: true,
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

    /// Shows every row whatever the search, for a group whose rows matched
    /// the search by other text than their own, such as an index entry.
    pub fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self
    }
}

impl Component for Group {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let lit = use_state(Vec::<usize>::new);
        let matched = use_state(Vec::<usize>::new);
        let query = search_highlight::current();
        let searching = self.filtered && query.is_some();
        let group_hit = query.as_deref().is_some_and(|query| {
            [self.title.as_deref(), self.description.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| search_highlight::hits(query, text))
        });
        search_highlight::use_match(group_hit);
        let filter = SlotFilter {
            matched,
            group_hit,
            searching,
        };
        let children = card_rows(self.rows.clone(), lit, filter);
        let heading = (self.title.is_some() || self.description.is_some()).then(|| {
            rect()
                .tw(&tokens, GROUP_HEAD)
                // A title alone centres on the trailing controls, as a meter
                // and a button sit on the Sound headings. With a description,
                // the controls line up with the bottom of the text.
                .maybe(self.description.is_none(), |element| {
                    element.tw(&tokens, &[items(Align::Center)])
                })
                .child(
                    rect()
                        .tw(&tokens, GROUP_HEAD_TEXT)
                        .maybe_child(
                            self.title
                                .as_ref()
                                .map(|title| section_label(title.clone())),
                        )
                        .maybe_child(self.description.as_ref().map(|text| caption(text.clone()))),
                )
                .maybe_child(self.trailing.clone())
        });
        let shown = !searching || group_hit || !matched.read().is_empty();
        // The page column owns the space between groups.
        search_highlight::collapse(
            &tokens,
            shown,
            rect()
                .tw(&tokens, GROUP)
                .maybe_child(heading)
                .child(rect().tw(&tokens, BOX).children(children)),
        )
    }
}

/// The row a [`Slot`] holds, given to the `Row` inside it so the row can
/// tell the group when it is lit.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowSlot {
    pub index: usize,
    /// The indexes of the rows that are hovered, pressed or selected.
    pub lit: State<Vec<usize>>,
    /// The indexes of the rows that match the search, in this group.
    pub matched: State<Vec<usize>>,
}

/// Marks the calling row as lit or not in its group, if it is in one. A lit
/// row has no hairline above or below it, so its highlight reads as one
/// block; the row calls this on every render.
pub(crate) fn use_row_lit(lit: bool) {
    let slot = use_try_consume::<RowSlot>();
    use_side_effect_with_deps(&lit, move |lit| {
        let Some(slot) = slot else { return };
        let mut rows = slot.lit;
        let listed = rows.peek().contains(&slot.index);
        if *lit && !listed {
            rows.write().push(slot.index);
        } else if !*lit && listed {
            rows.write().retain(|index| *index != slot.index);
        }
    });
    use_drop(move || {
        if let Some(slot) = slot {
            let mut rows = slot.lit;
            rows.write().retain(|index| *index != slot.index);
        }
    });
}

/// Records the calling row as matching the search in its group, if it is in
/// one, and counts it into the search section above. The row calls this on
/// every render, with whether its own text matches.
pub(crate) fn use_row_match(matched: bool) {
    let slot = use_try_consume::<RowSlot>();
    use_side_effect_with_deps(&matched, move |matched| {
        let Some(slot) = slot else { return };
        let mut rows = slot.matched;
        let listed = rows.peek().contains(&slot.index);
        if *matched && !listed {
            rows.write().push(slot.index);
        } else if !*matched && listed {
            rows.write().retain(|index| *index != slot.index);
        }
    });
    use_drop(move || {
        if let Some(slot) = slot {
            let mut rows = slot.matched;
            rows.write().retain(|index| *index != slot.index);
        }
    });
    search_highlight::use_match(matched);
}

/// How a group's rows show during a search: a row shows when it matches or
/// when the whole group does, and a group with no shown rows hides.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct SlotFilter {
    pub(crate) matched: State<Vec<usize>>,
    pub(crate) group_hit: bool,
    pub(crate) searching: bool,
}

impl SlotFilter {
    /// Whether the row at `index` shows, and whether it has a hairline above.
    fn placement(self, index: usize) -> (bool, bool) {
        if !self.searching || self.group_hit {
            return (true, index > 0);
        }
        let matched = self.matched.read();
        let shown = matched.contains(&index);
        (shown, shown && matched.iter().any(|&above| above < index))
    }
}

/// One row of a group with the hairline above it. The hairline is hidden
/// while this row or the one above it is lit.
#[derive(PartialEq)]
struct Slot {
    index: usize,
    lit: State<Vec<usize>>,
    filter: SlotFilter,
    row: Element,
}

impl Component for Slot {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (index, lit, matched) = (self.index, self.lit, self.filter.matched);
        use_provide_context(move || RowSlot {
            index,
            lit,
            matched,
        });
        let hide = {
            let lit = self.lit.read();
            lit.contains(&index)
                || index
                    .checked_sub(1)
                    .is_some_and(|above| lit.contains(&above))
        };
        let (shown, divided) = self.filter.placement(index);
        let content = rect()
            .tw(&tokens, &[flex_col(), w_full()])
            .maybe_child(divided.then(|| hairline(&tokens, hide)))
            .child(self.row.clone());
        search_highlight::collapse(&tokens, shown, content)
    }
}

/// Rows with a hairline between each pair, inset by the gutter on both
/// sides, that goes while either neighbour is lit.
pub(crate) fn card_rows(
    rows: Vec<Element>,
    lit: State<Vec<usize>>,
    filter: SlotFilter,
) -> Vec<Element> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            Element::from(Slot {
                index,
                lit,
                filter,
                row,
            })
        })
        .collect()
}

/// The hairline above a row, laid over the row's top edge so it takes no room
/// in the flow. It is inset by the gutter on both sides, and goes transparent
/// while the row is hidden by a neighbour's highlight.
fn hairline(tokens: &Tokens, hide: bool) -> Rect {
    let line = rect().tw(
        tokens,
        &[w_full(), h(Dim::Hairline), mx(GUTTER), bg(Paint::Separator)],
    );
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .tw(tokens, &[w_full(), h(Dim::Hairline)])
        .maybe_child((!hide).then_some(line))
}

/// The same hairlines for a group that has no interactive rows.
fn plain_rows(tokens: &Tokens, rows: Vec<Element>) -> Vec<Element> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            rect()
                .tw(tokens, &[flex_col(), w_full()])
                .maybe_child((index > 0).then(|| hairline(tokens, false)))
                .child(row)
                .into()
        })
        .collect()
}

/// A group name: a heading above a card, larger and heavier than the row
/// text it introduces.
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
        search_highlight::label(SECTION, &tokens, &self.text)
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
        let (text, ink, glyph) = match &self.status {
            Status::Loading(text) => (text.clone(), Ink::Muted, None),
            Status::Empty(text) => (text.clone(), Ink::Muted, None),
            Status::Error(text) => (text.clone(), Ink::Danger, Some(Icon::DialogError)),
            Status::Info(text) => (text.clone(), Ink::Text, None),
        };
        let color = ink.color(&tokens);
        // Only an error is tinted; it is a danger layer over the card.
        let paint = if matches!(self.status, Status::Error(_)) {
            Paint::DangerSoft
        } else {
            Paint::Card
        };
        // A status is a card with the metrics of a row. A short message is
        // one row high, centred; a long one wraps and grows the block (Freya
        // centres against the content, not a minimum height, so the short
        // case needs the fixed row height).
        let short = text.chars().count() <= 70;
        rect()
            .tw(&tokens, BOX)
            .tw(&tokens, if short { ROW } else { ROW_TALL })
            .tw(&tokens, &[bg(paint)])
            .maybe_child(
                matches!(self.status, Status::Loading(_)).then(|| spinner(SpinnerSize::Small)),
            )
            .maybe_child(glyph.map(|glyph| icons::icon(glyph, IconSize::Row, color)))
            .child(
                BODY.in_ink(ink)
                    .selectable(&tokens, text)
                    .width(Size::flex(1.0)),
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
    skeleton_shape(&[SKELETON_ROWS, SKELETON_ROWS])
}

/// The same quiet bars as [`skeleton`], one section per entry of `sections`
/// with that many rows. A page whose loaded shape is known uses it, so the
/// rows it will show are already in place when its data arrives.
pub fn skeleton_shape(sections: &[usize]) -> Element {
    Skeleton {
        sections: sections.to_vec(),
    }
    .into()
}

const SKELETON_ROWS: usize = 3;

#[derive(PartialEq)]
struct Skeleton {
    sections: Vec<usize>,
}

impl Component for Skeleton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let bar = |width: Dim| skeleton_bar(&tokens, width);
        let section = |rows: usize| {
            let rows = (0..rows).map(|_| {
                Element::from(
                    rect()
                        .tw(&tokens, ROW)
                        .child(
                            rect()
                                .tw(&tokens, &[w_flex()])
                                .child(bar(Dim::SkeletonLabel)),
                        )
                        .child(bar(Dim::SkeletonValue)),
                )
            });
            rect()
                .tw(&tokens, GROUP)
                .child(bar(Dim::SkeletonTitle))
                .child(
                    rect()
                        .tw(&tokens, BOX)
                        .children(plain_rows(&tokens, rows.collect())),
                )
        };
        let sections: Vec<Element> = self
            .sections
            .iter()
            .map(|&rows| Element::from(section(rows)))
            .collect();
        page_column().children(sections)
    }
}

/// One row-sized placeholder for a group whose rows are not known yet, so the
/// group keeps its place while its data loads.
pub fn skeleton_row() -> Element {
    SkeletonRow.into()
}

#[derive(PartialEq)]
struct SkeletonRow;

impl Component for SkeletonRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .tw(&tokens, ROW)
            .child(
                rect()
                    .tw(&tokens, &[w_flex()])
                    .child(skeleton_bar(&tokens, Dim::SkeletonLabel)),
            )
            .child(skeleton_bar(&tokens, Dim::SkeletonValue))
    }
}

fn skeleton_bar(tokens: &Tokens, width: Dim) -> impl IntoElement {
    rect().tw(
        tokens,
        &[
            w(width),
            h(Dim::SkeletonLine),
            rounded(Radius::Mark),
            bg(Paint::Control),
        ],
    )
}

/// A title for in-page headers such as a details view.
pub fn heading(text: impl Into<String>) -> Element {
    typography::styled_label(TITLE, text)
}
