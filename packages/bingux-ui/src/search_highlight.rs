//! Search highlights: marks the words of a search inside titles and text,
//! the way a browser marks a find.
//!
//! A search results pane calls [`provide`] with its query. Text drawn below
//! it through [`label`] or [`paragraph`] shows each matched word on an accent
//! tint. Text outside such a pane, or text with no match, draws exactly as the
//! plain [`TextStyle`] would.

use std::ops::Range;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::shell::sections::use_search_block;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{PAGE_TITLE, TextStyle};

/// The query of the search pane above, shared with the text below it.
#[derive(Clone, Copy, PartialEq)]
struct SearchTerm(State<String>);

/// Marks the words of the query in the text drawn below this component. Call
/// it in the render of the component that shows the results.
pub fn provide(query: State<String>) {
    use_provide_context(|| SearchTerm(query));
}

/// One line of `text` in `style`, with the matched words marked. Long text is
/// cut with an ellipsis.
pub fn label(style: TextStyle, tokens: &Tokens, text: &str) -> Element {
    match marked(style, tokens, text) {
        None => style
            .label(tokens, text)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .into(),
        Some(paragraph) => paragraph
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .into(),
    }
}

/// Text in `style` that wraps and can be selected and copied. While a search
/// marks words in it, it is a paragraph with the marks instead: a selection
/// takes the paragraph's only highlight slot, so a selectable text cannot show
/// search marks.
pub fn paragraph(style: TextStyle, tokens: &Tokens, text: &str) -> Element {
    match marked(style, tokens, text) {
        None => SelectableText::new()
            .span(style.span(tokens, text))
            .line_height(style.leading)
            .into(),
        Some(paragraph) => paragraph.into(),
    }
}

/// Text in `style` as a paragraph with the matched words marked, or `None`
/// when no search is provided or nothing matches. A caller that adds its own
/// paragraph options chains them onto the result and falls back on `None`.
pub fn marked(style: TextStyle, tokens: &Tokens, text: &str) -> Option<Paragraph> {
    let ranges = marks(text)?;
    // A highlight change alone is a style-only diff: Freya keeps the old
    // layout and never re-measures the new element, so its paragraph is
    // missing at render (paragraph.rs `render` unwraps it). Keying by the
    // ranges makes each highlight set a new element that is measured.
    Some(
        style
            .paragraph(tokens, text)
            .highlights(ranges.clone())
            .highlight_color(mark_colour(tokens))
            .key(ranges),
    )
}

/// One line of `text` in `style` with the given byte `ranges` marked on the
/// same accent tint as a search: the way a list shows which letters of a name
/// matched a query. `ranges` are sorted, do not overlap and lie on character
/// boundaries. The line is cut with an ellipsis when it does not fit; chain
/// more paragraph options onto the result. With no ranges it is plain text in
/// the same element type, so a row that gains or loses its marks keeps its
/// place in the tree.
pub fn highlighted_label(
    style: TextStyle,
    tokens: &Tokens,
    text: &str,
    ranges: &[Range<usize>],
) -> Paragraph {
    let chars: Vec<(usize, usize)> = ranges
        .iter()
        .map(|range| {
            let start = text[..range.start].chars().count();
            (start, start + text[range.clone()].chars().count())
        })
        .collect();
    // See `marked`: the key makes each highlight set a measured element.
    style
        .paragraph(tokens, text)
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
        .highlights(chars.clone())
        .highlight_color(mark_colour(tokens))
        .key(chars)
}

/// [`highlighted_label`] with the font size scaled by `scale`, rounded to
/// the nearest quarter pixel, as `TextStyle::label_scaled` does.
pub fn highlighted_label_scaled(
    style: TextStyle,
    tokens: &Tokens,
    text: &str,
    ranges: &[Range<usize>],
    scale: f32,
) -> Paragraph {
    style.scale_paragraph(highlighted_label(style, tokens, text, ranges), scale)
}

/// The search text of the pane above, trimmed, or `None` when no search is
/// provided or the text is blank. It reads the context without a hook, so a
/// row may call it, or the text helpers that use it, from any branch.
pub fn current() -> Option<String> {
    let term = try_consume_context::<SearchTerm>()?;
    let query = term.0.read().trim().to_string();
    (!query.is_empty()).then_some(query)
}

/// Whether `text` contains every word of `query`, ignoring case: the rule the
/// settings index uses to pick a page. `query` must not be blank.
pub fn hits(query: &str, text: &str) -> bool {
    query
        .split_whitespace()
        .all(|word| !char_ranges(text, word).is_empty())
}

/// The number of matches under a search section, shared with the section above.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct MatchSink(pub State<usize>);

/// Counts the caller into the search section above while `matched` holds, so
/// the section shows only when something under it matches. Rows and groups
/// call it on every render.
pub(crate) fn use_match(matched: bool) {
    let sink = use_try_consume::<MatchSink>().map(|sink| sink.0);
    count_into(sink, matched);
}

/// Adds `matched` to `sink` while it holds, and takes it back out when the
/// caller goes away.
fn count_into(sink: Option<State<usize>>, matched: bool) {
    let mut counted = use_state(|| false);
    use_side_effect_with_deps(&matched, move |matched| {
        if *matched == *counted.peek() {
            return;
        }
        counted.set(*matched);
        if let Some(mut sink) = sink {
            if *matched {
                *sink.write() += 1;
            } else {
                *sink.write() -= 1;
            }
        }
    });
    use_drop(move || {
        if *counted.peek() {
            if let Some(mut sink) = sink {
                *sink.write() -= 1;
            }
        }
    });
}

/// A page's part of the search results: its heading (the page icon and name,
/// marked) above the page's real controls. It shows only while something under
/// it matches. Hidden controls stay mounted, so they can still report their
/// matches.
pub fn section(title: impl Into<String>, icon: Icon, body: Element, sink: State<usize>) -> Element {
    Section {
        title: title.into(),
        icon,
        body,
        sink,
    }
    .into()
}

/// `content` as it is when `shown`. Otherwise it is taken out of the flow at
/// zero height and clipped, so the matches under it still count and the page
/// column's gap does not open a space where it would have been.
pub(crate) fn collapse(tokens: &Tokens, shown: bool, content: impl Into<Element>) -> Element {
    let content: Element = content.into();
    if shown {
        content
    } else {
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::px(0.0))
            .tw(tokens, &[clip()])
            .child(content)
            .into()
    }
}

/// A page's block in the search results: its heading, the page icon and name,
/// above its body. The sidebar lists the block while it is `shown`.
#[derive(PartialEq)]
struct PageBlock {
    title: String,
    icon: Icon,
    body: Element,
    shown: bool,
}

impl Component for PageBlock {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let on_sized = use_search_block(&self.title, self.icon, self.shown);
        // The heading is the block's top, so a jump to the block lands on it.
        let heading = rect()
            .tw(
                &tokens,
                &[flex_row(), items(Align::Center), gap(S2), w_full()],
            )
            .on_sized(on_sized)
            .child(icons::icon(
                self.icon,
                IconSize::Row,
                Paint::Text.color(&tokens),
            ))
            .child(label(PAGE_TITLE, &tokens, &self.title));
        // A clear gap above each page, and a closer one to its groups.
        let content = rect()
            .tw(&tokens, &[flex_col(), gap(S2), w_full(), pt(S6)])
            .child(heading)
            .child(self.body.clone());
        collapse(&tokens, self.shown, content)
    }
}

/// A page's block that always shows, with `body` as its controls: for pages
/// listed by their index entries.
pub fn page_block(title: impl Into<String>, icon: Icon, body: Element) -> Element {
    PageBlock {
        title: title.into(),
        icon,
        body,
        shown: true,
    }
    .into()
}

#[derive(PartialEq)]
struct Section {
    title: String,
    icon: Icon,
    body: Element,
    sink: State<usize>,
}

impl Component for Section {
    fn render(&self) -> impl IntoElement {
        let matches = use_state(|| 0usize);
        use_provide_context(move || MatchSink(matches));
        let shown = current().is_none() || *matches.read() > 0;
        count_into(Some(self.sink), shown);
        Element::from(PageBlock {
            title: self.title.clone(),
            icon: self.icon,
            body: self.body.clone(),
            shown,
        })
    }
}

/// Text in `style` as a paragraph with the matched words marked. Used by the
/// `label` and `paragraph` helpers above, and by callers that need more.
fn marks(text: &str) -> Option<Vec<(usize, usize)>> {
    let term = try_consume_context::<SearchTerm>()?;
    let query = term.0.read().clone();
    let ranges = char_ranges(text, &query);
    (!ranges.is_empty()).then_some(ranges)
}

/// The tint behind a match: the accent, translucent so the text stays legible.
fn mark_colour(tokens: &Tokens) -> Color {
    Paint::Accent.color(tokens).with_af32(0.4)
}

/// Each run of `text` that matches a word of `query`, ignoring case, as
/// `(start, end)` character indices, which is what the paragraph highlights
/// take. Overlapping runs are merged and the result is sorted.
fn char_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let lower: Vec<char> = text.chars().map(lowercase).collect();
    let mut found = Vec::new();
    for word in query.split_whitespace() {
        let needle: Vec<char> = word.chars().map(lowercase).collect();
        if needle.is_empty() || needle.len() > lower.len() {
            continue;
        }
        for start in 0..=lower.len() - needle.len() {
            if lower[start..start + needle.len()] == needle[..] {
                found.push((start, start + needle.len()));
            }
        }
    }
    found.sort_unstable();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in found {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// The lower case form of `c` when it is one character, else `c` itself, so
/// that both texts keep one entry per character.
fn lowercase(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}
