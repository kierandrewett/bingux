//! Preference groups, captions, headings and status blocks.
//!
//! A group is a card, as a `ControlCentreDetails.qml` section: a muted name
//! above, an optional muted description, then rows set flush against one
//! card surface with hairlines between them.

use freya::prelude::*;

use crate::component::{ComponentName, Prop, Published};
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
///
/// The row is an [`AdaptiveLine`](crate::controls::adaptive::AdaptiveLine):
/// the label keeps its natural width, a text field or dropdown shrinks to what
/// is left, and a row that cannot fit both stacks the label above the control.
pub fn form_row(label: impl Into<String>, control: impl Into<Element>) -> Element {
    crate::controls::adaptive::AdaptiveLine::new("FormRow", label, control).into()
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
            .paragraph(&tokens, self.text.clone())
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
        // In a row that takes presses the caption is plain text, so the row's
        // press and pointer stay those of a button, not a text field.
        if crate::rows::use_in_pressable_row() {
            return match search_highlight::marked(CAPTION, &tokens, &self.text) {
                Some(marked) => marked.into(),
                None => CAPTION.paragraph(&tokens, self.text.clone()).into(),
            };
        }
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
    below_title: Option<Element>,
    footer: Option<String>,
    filtered: bool,
    rows: Vec<Element>,
}

impl Group {
    pub fn new(title: impl Into<Option<String>>) -> Self {
        Self {
            title: title.into(),
            description: None,
            trailing: None,
            below_title: None,
            footer: None,
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

    /// An element between the title line and the card, such as a search field
    /// that belongs to the group's list.
    pub fn below_title(mut self, element: impl Into<Element>) -> Self {
        self.below_title = Some(element.into());
        self
    }

    /// A caption under the card, at the card's edge and the group's gap from
    /// it: the note that belongs to this group, such as the kind of network
    /// under its password. A caption set as a sibling of the group sits a
    /// whole page gap away and reads as detached.
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.footer = (!text.is_empty()).then_some(text);
        self
    }

    pub fn titled(title: &str) -> Self {
        Self::new(Some(title.to_string()))
    }

    pub fn row(mut self, row: impl Into<Element>) -> Self {
        self.rows.push(row.into());
        self
    }

    /// Free content on the card, such as a chart or a preview, 16 px from
    /// the card's edge on every side. A row sits flush against the card, so
    /// content that is not a row goes here.
    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.rows
            .push(rect().tw_layout(BOX_BODY).child(content.into()).into());
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
        let (mounted, whole) = crate::stage::use_staged_rows(self.rows.len(), query.is_some());
        let children = card_rows(
            self.rows.iter().take(mounted).cloned().collect(),
            lit,
            filter,
        );
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
                .component("Group")
                .published(|| {
                    let call = match &self.title {
                        Some(title) => {
                            Published::call("Group::titled").arg(Prop::text("title", title.clone()))
                        }
                        None => Published::call("Group::new"),
                    };
                    call.maybe_method(
                        self.description
                            .as_ref()
                            .map(|text| Prop::text("description", text.clone())),
                    )
                })
                .tw(&tokens, GROUP)
                .maybe_child(heading)
                .maybe_child(self.below_title.clone())
                .child(rect().tw(&tokens, BOX).children(children))
                // The footer waits for the last row: it sits under the card.
                .maybe_child(
                    self.footer
                        .as_ref()
                        .filter(|_| whole)
                        .map(|text| caption(text.clone())),
                ),
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

/// Puts `index` in or out of `rows`, writing only when that changes it.
fn list(mut rows: State<Vec<usize>>, index: usize, on: bool) {
    let listed = rows.peek().contains(&index);
    if on && !listed {
        rows.write().push(index);
    } else if !on && listed {
        rows.write().retain(|listed| *listed != index);
    }
}

/// What a row tells the group it is in, and the search section above it, in one
/// hook (a row is mounted forty at a time, and each hook of a row is paid
/// forty times): whether it is `lit` (hovered, pressed or selected: a lit row
/// has no hairline above or below it, so its highlight reads as one block) and
/// whether it `matched` the search (it is counted into the section above, so
/// the section shows only when something under it matches). The row calls this
/// on every render, and takes itself out of both when it goes away.
pub(crate) fn use_row_slot(lit: bool, matched: bool) {
    let slot = use_try_consume::<RowSlot>();
    let sink = use_try_consume::<search_highlight::MatchSink>().map(|sink| sink.0);
    let counted = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(false)));
    let counted_here = counted.clone();
    use_side_effect_with_deps(&(lit, matched), move |&(lit, matched)| {
        if let Some(slot) = slot {
            list(slot.lit, slot.index, lit);
            list(slot.matched, slot.index, matched);
        }
        if matched != counted_here.get() {
            counted_here.set(matched);
            if let Some(mut sink) = sink {
                if matched {
                    *sink.write() += 1;
                } else {
                    *sink.write() -= 1;
                }
            }
        }
    });
    use_drop(move || {
        if let Some(slot) = slot {
            list(slot.lit, slot.index, false);
            list(slot.matched, slot.index, false);
        }
        if counted.get() {
            if let Some(mut sink) = sink {
                *sink.write() -= 1;
            }
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
        rect()
            .component("Section")
            .child(search_highlight::label(SECTION, &tokens, &self.text))
    }
}

/// The title block of a group, with `trailing` controls at its right, and no
/// card. A page that sets several cards under one name uses it for that name
/// and its toolbar, then lists the cards below.
pub fn group_head(title: impl Into<String>, trailing: impl Into<Element>) -> Element {
    GroupHead {
        title: title.into(),
        trailing: trailing.into(),
    }
    .into()
}

#[derive(PartialEq)]
struct GroupHead {
    title: String,
    trailing: Element,
}

impl Component for GroupHead {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .tw(&tokens, GROUP_HEAD)
            .tw(&tokens, &[items(Align::Center)])
            .child(
                rect()
                    .tw(&tokens, GROUP_HEAD_TEXT)
                    .child(section_label(self.title.clone())),
            )
            .child(self.trailing.clone())
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
            .component("StatusBlock")
            .tw(&tokens, BOX)
            .tw(&tokens, if short { ROW } else { ROW_TALL })
            .tw(&tokens, &[bg(paint)])
            .maybe_child(
                matches!(self.status, Status::Loading(_)).then(|| spinner(SpinnerSize::Small)),
            )
            .maybe_child(glyph.map(|glyph| icons::icon(glyph, IconSize::Row, color)))
            .child(
                BODY.in_ink(ink)
                    .paragraph(&tokens, text)
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
        sections: sections.iter().map(|&rows| "-".repeat(rows)).collect(),
    }
    .into()
}

/// Skeleton sections spelled row by row, for a page whose rows differ in
/// height. Each string is one section and each character one row: `-` a
/// one-line row, `=` a row with a second line of text, `#` a block the height
/// of the arrangement canvas. A leading `~TEXT|` gives the section's title a
/// description under it, set in the real text so that it wraps as the real one
/// will.
pub fn skeleton_rows(sections: &[&str]) -> Element {
    Skeleton {
        sections: sections.iter().map(|section| section.to_string()).collect(),
    }
    .into()
}

const SKELETON_ROWS: usize = 3;

#[derive(PartialEq)]
struct Skeleton {
    sections: Vec<String>,
}

/// A bar standing in for a group title, in a box as tall as the real title,
/// so the card below it starts where the real card will.
fn skeleton_title(tokens: &Tokens, description: Option<&str>) -> Element {
    rect()
        .tw(tokens, &[w_full()])
        .child(
            rect()
                .opacity(0.0)
                .tw(tokens, GROUP_HEAD_TEXT)
                .child(section_label("Mg"))
                .maybe_child(description.map(|text| caption(text.to_string()))),
        )
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .tw(
                    tokens,
                    &[w_full(), h_full(), flex_col(), justify(Align::Center)],
                )
                .child(skeleton_bar(tokens, Dim::SkeletonTitle)),
        )
        .into()
}

/// `text` laid out invisibly to give a box its line height, with `bar` drawn
/// over it, centred on the line.
fn ghost(tokens: &Tokens, text: Element, bar: Element) -> Element {
    rect()
        .tw(tokens, &[w_full()])
        .child(rect().opacity(0.0).child(text))
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .tw(
                    tokens,
                    &[w_full(), h_full(), flex_col(), justify(Align::Center)],
                )
                .child(bar),
        )
        .into()
}

fn skeleton_line(tokens: &Tokens, kind: char) -> Element {
    let bar = |width: Dim| skeleton_bar(tokens, width);
    match kind {
        '#' => rect()
            .tw(
                tokens,
                &[w_full(), h(Dim::ArrangementHeight), bg(Paint::Control)],
            )
            .into(),
        // The height of a row with a second line is its text's, not a number:
        // the title and the caption are laid out invisibly, as the real row lays
        // them out, and the bars are drawn over them.
        '=' => rect()
            .tw(tokens, ROW_TALL)
            .child(
                rect()
                    .tw(tokens, &[flex_col(), gap(S0_5), w_flex()])
                    .child(ghost(
                        tokens,
                        BODY.label(tokens, "Mg").into(),
                        bar(Dim::SkeletonLabel).into_element(),
                    ))
                    .child(ghost(
                        tokens,
                        CAPTION.label(tokens, "Mg").into(),
                        bar(Dim::SkeletonValue).into_element(),
                    )),
            )
            .child(bar(Dim::SkeletonValue))
            .into(),
        _ => rect()
            .tw(tokens, ROW)
            .child(
                rect()
                    .tw(tokens, &[w_flex()])
                    .child(bar(Dim::SkeletonLabel)),
            )
            .child(bar(Dim::SkeletonValue))
            .into(),
    }
}

impl Component for Skeleton {
    fn render(&self) -> impl IntoElement {
        crate::stage::use_provisional(true);
        let tokens = use_tokens();
        let sections: Vec<Element> = self
            .sections
            .iter()
            .map(|rows| {
                let (description, rows) =
                    match rows.strip_prefix('~').and_then(|rest| rest.split_once('|')) {
                        Some((text, rows)) => (Some(text), rows),
                        None => (None, rows.as_str()),
                    };
                let lines: Vec<Element> = rows
                    .chars()
                    .map(|kind| skeleton_line(&tokens, kind))
                    .collect();
                Element::from(
                    rect()
                        .tw(&tokens, GROUP)
                        .child(skeleton_title(&tokens, description))
                        .child(rect().tw(&tokens, BOX).children(plain_rows(&tokens, lines))),
                )
            })
            .collect();
        page_column().children(sections)
    }
}

/// One row-sized placeholder for a group whose rows are not known yet, so the
/// group keeps its place while its data loads.
pub fn skeleton_row() -> Element {
    SkeletonRow { subtitle: false }.into()
}

/// As [`skeleton_row`], as tall as a row with a second line of text, for a list
/// whose rows have a subtitle (a chosen zone's offset, a font's sample).
pub fn skeleton_row_subtitled() -> Element {
    SkeletonRow { subtitle: true }.into()
}

#[derive(PartialEq)]
struct SkeletonRow {
    subtitle: bool,
}

impl Component for SkeletonRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        crate::stage::use_provisional(true);
        skeleton_line(&tokens, if self.subtitle { '=' } else { '-' })
    }
}

/// A bar standing in for a caption line (a count, a hint under a list), in a
/// box as tall as the real caption, so what follows it keeps its place.
pub fn skeleton_caption() -> Element {
    SkeletonCaption.into()
}

#[derive(PartialEq)]
struct SkeletonCaption;

impl Component for SkeletonCaption {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        crate::stage::use_provisional(true);
        rect()
            .tw(&tokens, &[w_full()])
            .child(rect().opacity(0.0).child(caption("Mg")))
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .tw(
                        &tokens,
                        &[w_full(), h_full(), flex_col(), justify(Align::Center)],
                    )
                    .child(skeleton_bar(&tokens, Dim::SkeletonTitle)),
            )
    }
}

/// A placeholder bar of a set width that never grows past the box it is in, so
/// a narrow row squeezes it and not the bar beside it.
fn skeleton_bar(tokens: &Tokens, width: Dim) -> impl IntoElement {
    rect()
        .tw(
            tokens,
            &[
                w(width),
                h(Dim::SkeletonLine),
                rounded(Radius::Mark),
                bg(Paint::Control),
            ],
        )
        .max_width(Size::percent(100.0))
}

/// A title for in-page headers such as a details view.
pub fn heading(text: impl Into<String>) -> Element {
    typography::styled_label(TITLE, text)
}
