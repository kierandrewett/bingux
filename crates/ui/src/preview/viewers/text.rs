//! Text, code, markup and data: line numbers, colour, wrapping and a window
//! that grows.
//!
//! The viewer lays the text out itself as *visual lines* of equal height: a
//! long line is cut into as many rows as it needs when wrapping is on, and
//! only its first row carries the line number. Equal rows are what lets a
//! 4 MiB log scroll through a virtual list, so only the rows on screen are
//! ever built. The monospace face has one width for every character, so the
//! cut is arithmetic on the width of the box.
//!
//! The first window is read with the viewer (64 KiB). "Show more" reads the
//! next window off the UI thread and appends it, up to
//! [`TOTAL_LIMIT`](bingux_preview_core::text::TOTAL_LIMIT).
//!
//! | Key | Does |
//! | --- | ---- |
//! | `W` | wrap long lines, or not |

use std::sync::Arc;

use bingux_preview_core::highlight::{Language, Lexer, Token};
use bingux_preview_core::text::{TextWindow, read_window};
use bingux_preview_core::{Cancel, Loaded};
use freya::prelude::*;

use super::super::subject::Resolved;
use crate::component::ComponentName;
use crate::controls::action_button;
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::spawn_named;
use crate::typography::{CAPTION, CODE, Ink};

/// A piece of the text the size of a screenful: how many lines at most one
/// "Show more" reads.
const MORE: usize = 256 * 1024;

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Row {
    /// The line's number, on the first row of a wrapped line only.
    pub number: Option<usize>,
    pub text: String,
    /// Coloured pieces of `text`, by byte offsets.
    pub spans: Vec<(std::ops::Range<usize>, Token)>,
}

/// The ink a token of the text is drawn in.
pub(super) fn ink_of(token: Token) -> Ink {
    match token {
        Token::Keyword | Token::Key => Ink::Accent,
        Token::String => Ink::Success,
        Token::Comment => Ink::Muted,
        Token::Number => Ink::Warning,
        Token::Heading => Ink::Accent,
        Token::Type => Ink::Text,
    }
}

/// The rows of `text`: tabs become four spaces, a line break ends a line, and
/// with `cols` a line longer than that many characters goes on over more rows.
/// `first_line` numbers the first line.
pub(super) fn rows(text: &str, language: Language, cols: Option<usize>, first_line: usize, lexer: &mut Lexer) -> Vec<Row> {
    let mut rows = Vec::new();
    let _ = language;
    // A text that ends with a line break has no last, empty line.
    let body = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        return rows;
    }
    for (index, raw) in body.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r').replace('\t', "    ");
        let spans = lexer.line(&line);
        let number = first_line + index;
        let Some(cols) = cols.filter(|cols| *cols > 0) else {
            rows.push(Row {
                number: Some(number),
                spans: spans.into_iter().map(|span| (span.range, span.token)).collect(),
                text: line,
            });
            continue;
        };
        // Cut at character boundaries, `cols` characters a row.
        let mut starts: Vec<usize> = line.char_indices().step_by(cols).map(|(at, _)| at).collect();
        if starts.is_empty() {
            starts.push(0);
        }
        for (part, start) in starts.iter().enumerate() {
            let end = starts.get(part + 1).copied().unwrap_or(line.len());
            rows.push(Row {
                number: (part == 0).then_some(number),
                text: line[*start..end].to_owned(),
                spans: spans
                    .iter()
                    .filter(|span| span.range.start < end && span.range.end > *start)
                    .map(|span| (span.range.start.max(*start) - start..span.range.end.min(end) - start, span.token))
                    .collect(),
            });
        }
    }
    rows
}

/// How many lines `text` has.
pub(super) fn line_count(text: &str) -> usize {
    if text.is_empty() { 0 } else { text.strip_suffix('\n').unwrap_or(text).split('\n').count() }
}

/// The runs of a row for a paragraph: coloured pieces and the plain text
/// between them.
pub(super) fn runs(row: &Row) -> Vec<(Ink, String)> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, token) in &row.spans {
        if range.start > at {
            runs.push((Ink::Text, row.text[at..range.start].to_owned()));
        }
        if range.end > range.start.max(at) {
            runs.push((ink_of(*token), row.text[range.start.max(at)..range.end].to_owned()));
        }
        at = at.max(range.end);
    }
    if at < row.text.len() {
        runs.push((Ink::Text, row.text[at..].to_owned()));
    }
    runs
}

/// How many characters the number column needs for `lines` lines.
pub(super) fn gutter_digits(lines: usize) -> usize {
    lines.max(1).to_string().len()
}

#[derive(PartialEq)]
pub(super) struct TextView {
    pub resolved: Arc<Resolved>,
}

/// The data of the list: the rows and what the gutter needs.
#[derive(Clone)]
struct ListData {
    rows: Arc<Vec<Row>>,
    gutter: f32,
    /// The list's width: the virtual list gives its items none of their own.
    width: f32,
}

impl PartialEq for ListData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.rows, &other.rows) && self.gutter == other.gutter && self.width == other.width
    }
}

#[derive(PartialEq)]
struct RowView {
    data: ListData,
    index: usize,
}

impl Component for RowView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let row = &self.data.rows[self.index];
        let number = row
            .number
            .map(|number| CODE.in_ink(Ink::Muted).label(&tokens, number.to_string()).max_lines(1).text_align(TextAlign::Right))
            .map(Element::from);
        rect()
            .width(Size::px(self.data.width))
            .tw(&tokens, &[flex_row(), items(Align::Start), gap(S3), px(S3)])
            .child(rect().width(Size::px(self.data.gutter)).maybe_child(number))
            .child(rect().tw(&tokens, &[w_flex()]).child(CODE.runs(&tokens, runs(row)).max_lines(1)))
    }
}

impl Component for TextView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let Loaded::Text(first) = &self.resolved.loaded else {
            return super::Card::of(&self.resolved, &crate::preview::PreviewNav::new(|_| {})).into_element();
        };
        let mut wrap = use_state(|| true);
        let mut more = use_state(Vec::<Arc<TextWindow>>::new);
        let mut reading = use_state(|| false);
        let mut width = use_state(|| 0.0f32);

        // Plain monospace: a preview is for reading what a file is, not for highlighting it.
        let language = Language::Plain;
        let char_width = (crate::controls::path_label::measure_text("0000000000", CODE) / 10.0).max(1.0);
        let extra: Vec<Arc<TextWindow>> = more.read().clone();
        let all: Vec<&TextWindow> = std::iter::once(first).chain(extra.iter().map(|window| &**window)).collect();
        let last = all.last().copied().unwrap_or(first).clone();

        // The rows of every window read so far, numbered on from one another.
        let mut lexer = Lexer::new(language);
        let mut built = Vec::new();
        let digits_hint = 6;
        let cols_for = |gutter: f32| {
            let room = (*width.read() - gutter - 2.0 * S3.px() - S3.px()).max(char_width);
            (*wrap.read()).then(|| (room / char_width).floor() as usize)
        };
        let gutter = char_width * digits_hint as f32;
        let cols = cols_for(gutter);
        let mut line = 1;
        for window in &all {
            let chunk = rows(&window.text, language, cols, line, &mut lexer);
            line += line_count(&window.text);
            built.extend(chunk);
        }
        let digits = gutter_digits(line.saturating_sub(1));
        let gutter = char_width * digits.max(2) as f32;
        let data = ListData {
            rows: Arc::new(built),
            gutter,
            width: *width.read(),
        };
        let row_height = CODE.size.px() * CODE.leading;

        let source = self.resolved.source.clone();
        let end = last.end;
        let has_more = last.has_more();
        let capped = last.is_capped();
        let read_more = move |_: ()| {
            if *reading.peek() {
                return;
            }
            reading.set(true);
            let source = source.clone();
            spawn(async move {
                let window = spawn_named("preview-text-more", move |token| {
                    let cancel = Cancel::new();
                    if token.is_cancelled() {
                        cancel.cancel();
                    }
                    read_window(&source, end, MORE, &cancel).ok()
                })
                .await;
                if let Some(window) = window {
                    more.write().push(Arc::new(window));
                }
                reading.set(false);
            });
        };

        let total_note = match (last.total, capped, has_more) {
            (_, true, _) => Some("The preview stops here. Open the file to read the rest.".to_owned()),
            (Some(total), _, true) => Some(format!(
                "Showing {} of {}.",
                bingux_files::display::size(last.end),
                bingux_files::display::size(total)
            )),
            _ => None,
        };
        let encoding_note = first.encoding.note().map(str::to_owned);

        let on_key = move |event: Event<KeyboardEventData>| {
            if let Key::Character(text) = &event.key
                && text.eq_ignore_ascii_case("w")
                && !event.modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
            {
                event.stop_propagation();
                let next = !*wrap.peek();
                wrap.set(next);
            }
        };

        let list = VirtualScrollView::new_with_data(data.clone(), |item, data: &ListData| {
            Element::from(RowView {
                data: data.clone(),
                index: item.index,
            })
        })
        .length(data.rows.len())
        .item_size(row_height)
        .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()));

        // The text is the whole viewer: no language bar, no wrap switch (W wraps
        // or unwraps). A line only appears below it when the preview stopped
        // early, or the file is not UTF-8.
        let notice = total_note.is_some() || encoding_note.is_some() || has_more;
        let note = total_note.or(encoding_note).unwrap_or_default();
        rect()
            .component("PreviewText")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .on_global_key_down(on_key)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(list),
            )
            .maybe_child(notice.then(|| {
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), px(S4), py(S2), flex_row(), items(Align::Center), gap(S3), bg(Paint::Control)])
                        .child(rect().tw(&tokens, &[w_flex()]).child(
                            CAPTION
                                .in_ink(Ink::Muted)
                                .label(&tokens, note)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ))
                        .maybe_child(
                            (has_more).then(|| Element::from(action_button(if *reading.read() { "Reading…" } else { "Show more" }, read_more.clone())).clone()),
                        ),
                )
            }))
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str, cols: Option<usize>) -> Vec<(Option<usize>, String)> {
        rows(text, Language::Plain, cols, 1, &mut Lexer::new(Language::Plain))
            .into_iter()
            .map(|row| (row.number, row.text))
            .collect()
    }

    #[test]
    fn lines_are_numbered_and_the_last_break_makes_no_empty_line() {
        assert_eq!(plain("a\nb\n", None), [(Some(1), "a".into()), (Some(2), "b".into())]);
        assert_eq!(plain("a\n\nb", None), [(Some(1), "a".into()), (Some(2), String::new()), (Some(3), "b".into())]);
    }

    #[test]
    fn tabs_and_carriage_returns_are_tidied() {
        assert_eq!(plain("a\tb\r\n", None), [(Some(1), "a    b".into())]);
    }

    #[test]
    fn a_long_line_wraps_and_only_its_first_row_has_a_number() {
        assert_eq!(
            plain("abcdefgh\nxy\n", Some(3)),
            [
                (Some(1), "abc".into()),
                (None, "def".into()),
                (None, "gh".into()),
                (Some(2), "xy".into())
            ]
        );
    }

    #[test]
    fn wrapping_never_cuts_a_character() {
        assert_eq!(plain("éééé", Some(3)), [(Some(1), "ééé".into()), (None, "é".into())]);
    }

    #[test]
    fn colour_spans_follow_the_cut() {
        let mut lexer = Lexer::new(Language::Rust);
        let got = rows("let x = 42;\n", Language::Rust, Some(8), 1, &mut lexer);
        assert_eq!(got[0].text, "let x = ");
        assert_eq!(got[0].spans, [(0..3, Token::Keyword)]);
        assert_eq!(got[1].text, "42;");
        assert_eq!(got[1].spans, [(0..2, Token::Number)]);
    }

    #[test]
    fn runs_fill_the_gaps_with_plain_text() {
        let row = Row {
            number: Some(1),
            text: "let x".into(),
            spans: vec![(0..3, Token::Keyword)],
        };
        assert_eq!(runs(&row), [(Ink::Accent, "let".to_owned()), (Ink::Text, " x".to_owned())]);
    }

    #[test]
    fn the_gutter_fits_the_last_number() {
        assert_eq!(gutter_digits(9), 1);
        assert_eq!(gutter_digits(1000), 4);
        assert_eq!(gutter_digits(0), 1);
    }

}
