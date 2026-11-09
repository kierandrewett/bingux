//! Markdown drawn with the type ramp: a preview beside an editor, release
//! notes, a help page. The source is parsed by `bingux_text::markdown`
//! (CommonMark with tables, strikethrough and task lists); this draws the
//! blocks with the same styles every page uses, so a preview looks like
//! Bingux, not like a web page.
//!
//! | Markdown | Drawn as |
//! | -------- | -------- |
//! | `#` and `##` | `PAGE_TITLE` and `HEADING`; deeper levels `SECTION` and `EMPHASIS` |
//! | text, `**strong**`, `*emphasis*`, `` `code` ``, `~~strike~~` | `BODY` runs, semibold, italic, monospace, struck |
//! | links | `LINK` runs; a press opens the address through `on_link` |
//! | fenced code | a box in the monospace face, highlighted in its language |
//! | lists, task lists, quotes, tables, rules | indented blocks, a check mark, a bar at the leading edge, a grid, a hairline |
//!
//! With `follow(line)` the view scrolls so the block holding that source
//! line is in sight: a preview that keeps up with the caret.

use std::rc::Rc;

use freya::prelude::*;
use freya_core::elements::paragraph::ParagraphCursorExt;

use crate::component::ComponentName;

use bingux_text::markdown::{self, Align as ColumnAlign, Block, Inline, ListItem, Located};
use bingux_text::{Highlighter, Rope, TokenKind};

use super::palette::{SyntaxPalette, code_row, code_span};
use crate::icons::{self, Icon};
use crate::layout::divider;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, CODE, EMPHASIS, HEADING, Ink, LINK, PAGE_TITLE, SECTION, TextStyle};

/// A Markdown document drawn as a page. Build it with [`markdown_view`].
#[derive(Clone, PartialEq)]
pub struct MarkdownView {
    source: Rc<str>,
    follow: Option<usize>,
    on_link: Option<EventHandler<String>>,
}

/// `source` drawn as a page.
pub fn markdown_view(source: impl Into<Rc<str>>) -> MarkdownView {
    MarkdownView {
        source: source.into(),
        follow: None,
        on_link: None,
    }
}

impl MarkdownView {
    /// Keep the block that holds source line `line` (from 0) in sight.
    pub fn follow(mut self, line: usize) -> Self {
        self.follow = Some(line);
        self
    }

    /// Called with a link's address when it is pressed.
    pub fn on_link(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_link = Some(handler.into());
        self
    }
}

impl Component for MarkdownView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let source = self.source.clone();
        let blocks = use_memo_cached(&source);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut tops = use_state(Vec::<(usize, f32)>::new);
        let mut followed = use_state(|| None::<usize>);
        let palette = SyntaxPalette::new(&tokens);

        // Scroll to the block that holds the followed line, once per line.
        if let Some(line) = self.follow
            && *followed.peek() != Some(line)
        {
            // Block tops are window positions, which move as the page
            // scrolls; measured from the first block they are page offsets.
            let list = tops.peek().clone();
            let first = list.first().map_or(0.0, |(_, top)| *top);
            let target = list
                .iter()
                .take_while(|(start, _)| *start <= line)
                .last()
                .map(|(_, top)| *top - first);
            if let Some(offset) = target {
                followed.set(Some(line));
                let y = -offset.max(0.0);
                spawn(async move {
                    scroll.scroll_to_y(y as i32);
                });
            }
        }

        let on_link = self.on_link.clone();
        let count = blocks.len();
        let children = blocks.iter().enumerate().map(|(index, located)| {
            let start = located.line;
            rect()
                .key(index)
                .tw(&tokens, &[w_full()])
                .on_sized(move |event: Event<SizedEventData>| {
                    let top = event.area.min_y();
                    let mut list = tops.peek().clone();
                    list.resize(count, (0, 0.0));
                    if list[index] != (start, top) {
                        list[index] = (start, top);
                        tops.set(list);
                    }
                })
                .child(block(&tokens, &palette, &located.block, on_link.clone(), 0))
                .into_element()
        });

        rect()
            .component("MarkdownView")
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .child(
                ScrollView::new_controlled(scroll).child(
                    rect()
                        .tw(&tokens, &[w_full(), items(Align::Center), p(S6)])
                        .child(
                            rect()
                                .tw(&tokens, &[w_full(), max_w(Dim::Page), flex_col(), gap(S4)])
                                .a11y_role(AccessibilityRole::Document)
                                .children(children),
                        ),
                ),
            )
    }
}

/// The blocks of `source`, parsed again only when it changes.
fn use_memo_cached(source: &Rc<str>) -> Rc<Vec<Located>> {
    let cache = use_hook(|| {
        Rc::new(std::cell::RefCell::new((
            Rc::<str>::from(""),
            Rc::new(Vec::new()),
        )))
    });
    let mut cache = cache.borrow_mut();
    if !Rc::ptr_eq(&cache.0, source) && *cache.0 != **source {
        *cache = (source.clone(), Rc::new(markdown::parse(source)));
    }
    cache.1.clone()
}

fn heading_style(level: u8) -> TextStyle {
    match level {
        1 => PAGE_TITLE,
        2 => HEADING,
        3 => SECTION,
        _ => EMPHASIS,
    }
}

fn block(
    tokens: &Tokens,
    palette: &SyntaxPalette,
    block_: &Block,
    on_link: Option<EventHandler<String>>,
    depth: usize,
) -> Element {
    match block_ {
        Block::Heading { level, text } => {
            let style = heading_style(*level);
            let paragraph = runs(tokens, style, text, on_link, TextAlign::Start);
            let top = if *level <= 2 { S2 } else { S1 };
            rect()
                .tw(tokens, &[w_full(), flex_col(), gap(S2), pt(top)])
                .a11y_role(AccessibilityRole::Heading)
                .child(paragraph)
                .maybe_child((*level <= 2).then(|| divider(tokens)))
                .into()
        }
        Block::Paragraph(text) => runs(tokens, BODY, text, on_link, TextAlign::Start),
        Block::Code { language, text } => code_block(tokens, palette, language.as_deref(), text),
        Block::Html(text) => code_block(tokens, palette, Some("html"), text),
        Block::Rule => divider(tokens).into(),
        // A quote is a recessed box of muted text.
        Block::Quote(blocks) => rect()
            .tw(tokens, BOX)
            .tw(
                tokens,
                &[
                    w_full(),
                    flex_col(),
                    gap(S3),
                    p(S3),
                    bg(Paint::Recess),
                    ink(Paint::Muted),
                ],
            )
            .children(
                blocks
                    .iter()
                    .map(|inner| block(tokens, palette, inner, on_link.clone(), depth + 1)),
            )
            .into(),
        Block::List { start, items } => list(tokens, palette, *start, items, on_link, depth),
        Block::Table {
            align,
            header,
            rows,
        } => table(tokens, align, header, rows, on_link),
    }
}

fn list(
    tokens: &Tokens,
    palette: &SyntaxPalette,
    start: Option<u64>,
    items: &[ListItem],
    on_link: Option<EventHandler<String>>,
    depth: usize,
) -> Element {
    let bullet = ["•", "◦", "▪"][depth.min(2)];
    rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S2)])
        .a11y_role(AccessibilityRole::List)
        .children(items.iter().enumerate().map(|(index, item)| {
            let marker: Element = match (item.task, start) {
                (Some(true), _) => icons::icon(
                    Icon::CheckboxChecked,
                    IconSize::Control,
                    Paint::Accent.color(tokens),
                )
                .into(),
                (Some(false), _) => rect()
                    .tw(tokens, &[w(Dim::Box5), h(Dim::Box5), center()])
                    .child(rect().tw(
                        tokens,
                        &[
                            w(Dim::IconControl),
                            h(Dim::IconControl),
                            border(Line::Hairline),
                            rounded(Radius::Mark),
                        ],
                    ))
                    .into(),
                (None, Some(first)) => BODY
                    .in_ink(Ink::Muted)
                    .label(tokens, format!("{}.", first + index as u64))
                    .font_tabular()
                    .into(),
                (None, None) => BODY.in_ink(Ink::Muted).label(tokens, bullet).into(),
            };
            rect()
                .tw(tokens, &[w_full(), flex_row(), grow_children(), gap(S2)])
                .a11y_role(AccessibilityRole::ListItem)
                .child(
                    rect()
                        .tw(tokens, &[min_w(Dim::Box5), flex_row(), justify(Align::End)])
                        .child(marker),
                )
                .child(
                    rect()
                        .tw(tokens, &[w_flex(), flex_col(), gap(S2)])
                        .children(item.blocks.iter().map(|inner| {
                            block(tokens, palette, inner, on_link.clone(), depth + 1)
                        })),
                )
                .into_element()
        }))
        .into()
}

fn table(
    tokens: &Tokens,
    align: &[ColumnAlign],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    on_link: Option<EventHandler<String>>,
) -> Element {
    let cell = |inlines: &[Inline], column: usize, style: TextStyle| {
        let text_align = match align.get(column) {
            Some(ColumnAlign::Right) => TextAlign::End,
            Some(ColumnAlign::Centre) => TextAlign::Center,
            _ => TextAlign::Start,
        };
        rect()
            .tw(tokens, &[w_flex(), px(S3), py(S2)])
            .child(runs(tokens, style, inlines, on_link.clone(), text_align))
            .into_element()
    };
    let head = rect()
        .tw(
            tokens,
            &[w_full(), flex_row(), grow_children(), bg(Paint::Control)],
        )
        .children(
            header
                .iter()
                .enumerate()
                .map(|(column, inlines)| cell(inlines, column, EMPHASIS)),
        );
    rect()
        .tw(tokens, BOX)
        .tw(tokens, &[w_full(), flex_col(), clip()])
        .a11y_role(AccessibilityRole::Table)
        .child(head)
        .children(rows.iter().map(|row| {
            rect()
                .tw(tokens, &[w_full(), flex_col()])
                .child(divider(tokens))
                .child(
                    rect()
                        .tw(tokens, &[w_full(), flex_row(), grow_children()])
                        .children(
                            row.iter()
                                .enumerate()
                                .map(|(column, inlines)| cell(inlines, column, BODY)),
                        ),
                )
                .into_element()
        }))
        .into()
}

/// A fenced code block, highlighted in its language when the highlighter
/// knows it.
fn code_block(
    tokens: &Tokens,
    palette: &SyntaxPalette,
    language: Option<&str>,
    text: &str,
) -> Element {
    let font = CODE.size.px();
    let row = (font * 1.5).round();
    let language = language.and_then(bingux_text::language::by_name);
    let rope = Rope::from_str(text);
    let lines = rope.len_lines();
    let spans = language.map(|language| {
        let mut highlighter = Highlighter::new(&language);
        highlighter.spans(&rope, 0, lines, lines + 1).lines
    });
    let rows = (0..lines).map(|line| {
        let content = rope.line(line).to_string();
        let content = content.trim_end_matches('\n');
        let layout = bingux_text::layout::LineLayout::new(content, 4, None);
        let line_spans = spans
            .as_ref()
            .and_then(|spans| spans.get(line).cloned().flatten());
        let segments = bingux_text::layout::row_segments(
            content,
            &layout,
            0,
            line_spans.as_deref().unwrap_or(&[]),
            4,
            false,
        );
        segments
            .into_iter()
            .fold(code_row(font, row), |paragraph, segment| {
                let ink = segment.kind.map_or_else(
                    || palette.ink(TokenKind::Operator),
                    |kind| palette.ink(kind),
                );
                paragraph.span(code_span(segment.text, ink, font))
            })
            .into_element()
    });
    rect()
        .tw(tokens, BOX)
        .tw(tokens, &[w_full(), p(S3), bg(Paint::Recess)])
        .child(
            crate::style::scroll_x()
                .height(Size::auto())
                .child(rect().tw(tokens, &[flex_col()]).children(rows)),
        )
        .into()
}

/// A paragraph of runs in `style`. One with links is a component of its
/// own, which keeps the laid-out paragraph between frames to find which run
/// a press landed on.
fn runs(
    tokens: &Tokens,
    style: TextStyle,
    inlines: &[Inline],
    on_link: Option<EventHandler<String>>,
    align: TextAlign,
) -> Element {
    let has_links = inlines.iter().any(|inline| inline.style.link.is_some());
    match on_link {
        Some(on_link) if has_links => LinkText {
            style,
            inlines: inlines.to_vec(),
            on_link,
            align,
        }
        .into_element(),
        _ => styled_runs(tokens, style, inlines, align).0.into(),
    }
}

/// The paragraph of `inlines` and where each link is, in UTF-16 units.
fn styled_runs(
    tokens: &Tokens,
    style: TextStyle,
    inlines: &[Inline],
    align: TextAlign,
) -> (Paragraph, Vec<(usize, usize, String)>) {
    let mut links: Vec<(usize, usize, String)> = Vec::new();
    let mut at = 0usize;
    let mut paragraph = style
        .paragraph(tokens, "")
        .width(Size::fill())
        .text_align(align);
    for inline in inlines {
        let length = inline.text.encode_utf16().count();
        if let Some(url) = &inline.style.link {
            links.push((at, at + length, url.clone()));
        }
        at += length;
        let run_style = if inline.style.link.is_some() {
            LINK
        } else if inline.style.code {
            CODE.in_ink(Ink::Text)
        } else if inline.style.strong {
            TextStyle {
                weight: EMPHASIS.weight,
                ..style
            }
        } else {
            style
        };
        let mut span = run_style.span(tokens, inline.text.clone());
        if inline.style.emphasis {
            span = span.font_slant(FontSlant::Italic);
        }
        if inline.style.strike {
            span = span.text_decoration(TextDecoration::LineThrough);
        }
        if inline.style.link.is_some() {
            span = span.text_decoration(TextDecoration::Underline);
        }
        paragraph = paragraph.span(span);
    }
    (paragraph, links)
}

#[derive(Clone, PartialEq)]
struct LinkText {
    style: TextStyle,
    inlines: Vec<Inline>,
    on_link: EventHandler<String>,
    align: TextAlign,
}

impl Component for LinkText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let holder = use_hook(ParagraphHolder::default);
        let (paragraph, links) = styled_runs(&tokens, self.style, &self.inlines, self.align);
        let on_link = self.on_link.clone();
        let press_holder = holder.clone();
        paragraph
            .holder(holder)
            .on_pointer_press(move |event: Event<PointerEventData>| {
                let point = event.element_location();
                let inner = press_holder.0.borrow();
                let Some(inner) = inner.as_ref() else { return };
                let local = press_holder.visible_location(point);
                let index = inner.paragraph.cursor_index_at_point((
                    local.x * inner.scale_factor,
                    local.y * inner.scale_factor,
                ));
                if let Some((_, _, url)) = links
                    .iter()
                    .find(|(start, end, _)| index >= *start && index < *end)
                {
                    on_link.call(url.clone());
                }
            })
    }
}
