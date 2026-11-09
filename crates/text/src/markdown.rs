//! Markdown as blocks and styled runs, for a preview to draw with its own
//! type styles. CommonMark with tables, strikethrough and task lists, read
//! by pulldown-cmark. Each top-level block knows the source line it starts
//! on, so a preview can follow the caret.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// How a run of text is set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
    pub strike: bool,
    /// The address of the link the run is part of.
    pub link: Option<String>,
}

/// A run of text in one style. A line break inside a paragraph is a run
/// with the text `"\n"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Centre,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// `Some(done)` for a task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        text: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    Code {
        language: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    /// `start` is the first number of an ordered list.
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Rule,
    Table {
        align: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// Raw HTML, shown as code.
    Html(String),
}

/// A top-level block and the source line (from 0) it starts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub line: usize,
    pub block: Block,
}

/// What is being built.
enum Frame {
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item {
        task: Option<bool>,
        blocks: Vec<Block>,
    },
    Table {
        align: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        row: Vec<Vec<Inline>>,
        in_head: bool,
    },
}

/// Parses `source` into blocks.
pub fn parse(source: &str) -> Vec<Located> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options).into_offset_iter();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect();
    let line_of = |offset: usize| {
        line_starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    };

    let mut out: Vec<Located> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut style = Style::default();
    let mut code: Option<(Option<String>, String)> = None;
    let mut html = String::new();
    let mut block_start = 0usize;
    let mut heading: Option<u8> = None;

    fn emit(out: &mut Vec<Located>, stack: &mut [Frame], line: usize, block: Block) {
        match stack.last_mut() {
            Some(Frame::Quote(blocks)) | Some(Frame::Item { blocks, .. }) => blocks.push(block),
            _ => out.push(Located { line, block }),
        }
    }

    /// The text of a tight list item comes with no paragraph around it; a
    /// block that starts inside the item ends that text.
    fn flush_tight(stack: &mut [Frame], inlines: &mut Vec<Inline>) {
        if let Some(Frame::Item { blocks, .. }) = stack.last_mut()
            && !inlines.is_empty()
        {
            blocks.push(Block::Paragraph(std::mem::take(inlines)));
        }
    }

    for (event, range) in parser {
        if stack.is_empty() && inlines.is_empty() && code.is_none() {
            block_start = line_of(range.start);
        }
        let starts_block = matches!(
            &event,
            Event::Rule
                | Event::Start(
                    Tag::Heading { .. }
                        | Tag::Paragraph
                        | Tag::CodeBlock(_)
                        | Tag::BlockQuote(_)
                        | Tag::List(_)
                        | Tag::Table(_)
                        | Tag::HtmlBlock
                )
        );
        if starts_block {
            flush_tight(&mut stack, &mut inlines);
        }
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    });
                }
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .map(str::to_owned)
                            .filter(|name| !name.is_empty()),
                        CodeBlockKind::Indented => None,
                    };
                    code = Some((language, String::new()));
                }
                Tag::BlockQuote(_) => stack.push(Frame::Quote(Vec::new())),
                Tag::List(start) => stack.push(Frame::List {
                    start,
                    items: Vec::new(),
                }),
                Tag::Item => stack.push(Frame::Item {
                    task: None,
                    blocks: Vec::new(),
                }),
                Tag::Table(align) => stack.push(Frame::Table {
                    align: align
                        .iter()
                        .map(|align| match align {
                            Alignment::None => Align::None,
                            Alignment::Left => Align::Left,
                            Alignment::Center => Align::Centre,
                            Alignment::Right => Align::Right,
                        })
                        .collect(),
                    header: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                    in_head: false,
                }),
                Tag::TableHead => {
                    if let Some(Frame::Table { in_head, .. }) = stack.last_mut() {
                        *in_head = true;
                    }
                }
                Tag::Emphasis => style.emphasis = true,
                Tag::Strong => style.strong = true,
                Tag::Strikethrough => style.strike = true,
                Tag::Link { dest_url, .. } => style.link = Some(dest_url.into_string()),
                Tag::Image { .. } => inlines.push(Inline {
                    text: "🖼 ".into(),
                    style: style.clone(),
                }),
                Tag::HtmlBlock => html.clear(),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    let level = heading.take().unwrap_or(1);
                    emit(
                        &mut out,
                        &mut stack,
                        block_start,
                        Block::Heading {
                            level,
                            text: std::mem::take(&mut inlines),
                        },
                    );
                }
                TagEnd::Paragraph => {
                    let paragraph = Block::Paragraph(std::mem::take(&mut inlines));
                    emit(&mut out, &mut stack, block_start, paragraph);
                }
                TagEnd::CodeBlock => {
                    if let Some((language, mut text)) = code.take() {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        emit(
                            &mut out,
                            &mut stack,
                            block_start,
                            Block::Code { language, text },
                        );
                    }
                }
                TagEnd::HtmlBlock => {
                    let text = std::mem::take(&mut html);
                    emit(
                        &mut out,
                        &mut stack,
                        block_start,
                        Block::Html(text.trim_end().to_owned()),
                    );
                }
                TagEnd::BlockQuote(_) => {
                    if let Some(Frame::Quote(blocks)) = stack.pop() {
                        emit(&mut out, &mut stack, block_start, Block::Quote(blocks));
                    }
                }
                TagEnd::Item => {
                    if let Some(Frame::Item { task, mut blocks }) = stack.pop() {
                        if !inlines.is_empty() {
                            blocks.push(Block::Paragraph(std::mem::take(&mut inlines)));
                        }
                        if let Some(Frame::List { items, .. }) = stack.last_mut() {
                            items.push(ListItem { task, blocks });
                        }
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List { start, items }) = stack.pop() {
                        emit(
                            &mut out,
                            &mut stack,
                            block_start,
                            Block::List { start, items },
                        );
                    }
                }
                TagEnd::TableCell => {
                    if let Some(Frame::Table { row, .. }) = stack.last_mut() {
                        row.push(std::mem::take(&mut inlines));
                    }
                }
                TagEnd::TableHead => {
                    if let Some(Frame::Table {
                        header,
                        row,
                        in_head,
                        ..
                    }) = stack.last_mut()
                    {
                        *header = std::mem::take(row);
                        *in_head = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(Frame::Table { rows, row, .. }) = stack.last_mut() {
                        rows.push(std::mem::take(row));
                    }
                }
                TagEnd::Table => {
                    if let Some(Frame::Table {
                        align,
                        header,
                        rows,
                        ..
                    }) = stack.pop()
                    {
                        emit(
                            &mut out,
                            &mut stack,
                            block_start,
                            Block::Table {
                                align,
                                header,
                                rows,
                            },
                        );
                    }
                }
                TagEnd::Emphasis => style.emphasis = false,
                TagEnd::Strong => style.strong = false,
                TagEnd::Strikethrough => style.strike = false,
                TagEnd::Link => style.link = None,
                _ => {}
            },
            Event::Text(text) => {
                if let Some((_, code_text)) = code.as_mut() {
                    code_text.push_str(&text);
                } else {
                    push_inline(&mut inlines, &text, &style);
                }
            }
            Event::Code(text) => {
                let code_style = Style {
                    code: true,
                    ..style.clone()
                };
                push_inline(&mut inlines, &text, &code_style);
            }
            Event::Html(text) => html.push_str(&text),
            Event::InlineHtml(text) => push_inline(
                &mut inlines,
                &text,
                &Style {
                    code: true,
                    ..style.clone()
                },
            ),
            Event::SoftBreak => push_inline(&mut inlines, " ", &style),
            Event::HardBreak => push_inline(&mut inlines, "\n", &style),
            Event::Rule => emit(&mut out, &mut stack, block_start, Block::Rule),
            Event::TaskListMarker(done) => {
                if let Some(Frame::Item { task, .. }) = stack.last_mut() {
                    *task = Some(done);
                }
            }
            Event::FootnoteReference(name) => {
                push_inline(&mut inlines, &format!("[{name}]"), &style)
            }
            _ => {}
        }
    }
    out
}

fn push_inline(inlines: &mut Vec<Inline>, text: &str, style: &Style) {
    if let Some(last) = inlines.last_mut()
        && last.style == *style
        && text != "\n"
        && last.text != "\n"
    {
        last.text.push_str(text);
        return;
    }
    inlines.push(Inline {
        text: text.to_owned(),
        style: style.clone(),
    });
}

/// The text of runs, without styles: a heading's name for an outline.
pub fn plain(inlines: &[Inline]) -> String {
    inlines.iter().map(|inline| inline.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_paragraphs_and_lines() {
        let blocks = parse("# Title\n\nSome *soft* **bold** `code`.\n\n---\n");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].line, 0);
        assert!(
            matches!(&blocks[0].block, Block::Heading { level: 1, text } if plain(text) == "Title")
        );
        assert_eq!(blocks[1].line, 2);
        let Block::Paragraph(runs) = &blocks[1].block else {
            panic!()
        };
        assert!(
            runs.iter()
                .any(|run| run.text == "soft" && run.style.emphasis)
        );
        assert!(
            runs.iter()
                .any(|run| run.text == "bold" && run.style.strong)
        );
        assert!(runs.iter().any(|run| run.text == "code" && run.style.code));
        assert_eq!(blocks[2].block, Block::Rule);
    }

    #[test]
    fn tight_and_task_lists() {
        let blocks = parse("- [x] done\n- [ ] to do\n- plain\n");
        let Block::List { start: None, items } = &blocks[0].block else {
            panic!("{blocks:?}")
        };
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].task, Some(true));
        assert_eq!(items[1].task, Some(false));
        assert!(matches!(&items[2].blocks[0], Block::Paragraph(runs) if plain(runs) == "plain"));
    }

    #[test]
    fn nested_lists_and_quotes() {
        let blocks = parse("1. one\n   - inner\n2. two\n\n> quoted\n");
        let Block::List {
            start: Some(1),
            items,
        } = &blocks[0].block
        else {
            panic!("{blocks:?}")
        };
        assert!(
            matches!(&items[0].blocks[1], Block::List { .. }),
            "{items:?}"
        );
        assert!(
            matches!(&blocks[1].block, Block::Quote(inner) if matches!(&inner[0], Block::Paragraph(_)))
        );
    }

    #[test]
    fn code_blocks_and_tables() {
        let blocks = parse("```rust\nfn main() {}\n```\n\n| a | b |\n|:--|--:|\n| 1 | 2 |\n");
        assert_eq!(
            blocks[0].block,
            Block::Code {
                language: Some("rust".into()),
                text: "fn main() {}".into()
            }
        );
        let Block::Table {
            align,
            header,
            rows,
        } = &blocks[1].block
        else {
            panic!()
        };
        assert_eq!(align, &vec![Align::Left, Align::Right]);
        assert_eq!(plain(&header[1]), "b");
        assert_eq!(plain(&rows[0][0]), "1");
    }

    #[test]
    fn links_keep_their_address() {
        let blocks = parse("See [the site](https://example.com).\n");
        let Block::Paragraph(runs) = &blocks[0].block else {
            panic!()
        };
        assert!(runs.iter().any(|run| run.text == "the site"
            && run.style.link.as_deref() == Some("https://example.com")));
    }
}
