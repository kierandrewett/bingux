//! Where each character of a line is drawn: display columns and soft wrap.
//!
//! The editor draws text in a monospace face on a grid of cells. A character
//! takes the cells `unicode-width` gives it (two for most CJK and emoji), a
//! tab runs to the next tab stop, and a control character is shown as its
//! control picture (one cell). With wrap on, a line is cut into rows at the
//! last space that fits, or mid-word when a word is wider than the row.
//!
//! A line longer than [`LONG_LINE`] characters (a minified file, a log line
//! with a dump in it) is laid out as one cell a character with no tab stops,
//! so nothing about it costs more than the part on screen.

use unicode_width::UnicodeWidthChar;

/// Lines longer than this are laid out a cell a character.
pub const LONG_LINE: usize = 10_000;

/// One visual row of a line: chars `start..end` of the line, the first at
/// display column `start_col` of the unwrapped line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub start_col: usize,
}

/// How one line is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineLayout {
    /// The column at each char boundary, `len + 1` entries, or empty for a
    /// long line, whose column is its char index.
    cols: Vec<u32>,
    len: usize,
    pub rows: Vec<Row>,
}

/// The width of `ch` in cells when it starts at column `col`.
pub fn char_cells(ch: char, col: usize, tab_width: usize) -> usize {
    match ch {
        '\t' => tab_width - (col % tab_width.max(1)),
        ch if ch.is_control() => 1,
        ch => ch.width().unwrap_or(0),
    }
}

impl LineLayout {
    /// Lays out `line` (without its line break). `wrap` is the row width in
    /// cells, or `None` for one row however long.
    pub fn new(line: &str, tab_width: usize, wrap: Option<usize>) -> Self {
        let tab_width = tab_width.max(1);
        let len = line.chars().count();
        if len > LONG_LINE {
            let rows = match wrap {
                Some(width) if width > 0 => (0..len.div_ceil(width).max(1))
                    .map(|index| Row {
                        start: index * width,
                        end: ((index + 1) * width).min(len),
                        start_col: index * width,
                    })
                    .collect(),
                _ => vec![Row {
                    start: 0,
                    end: len,
                    start_col: 0,
                }],
            };
            return Self {
                cols: Vec::new(),
                len,
                rows,
            };
        }
        let mut cols = Vec::with_capacity(len + 1);
        let mut col = 0usize;
        cols.push(0);
        for ch in line.chars() {
            col += char_cells(ch, col, tab_width);
            cols.push(col as u32);
        }
        let rows = match wrap {
            Some(width) if width > 0 => wrap_rows(line, &cols, width),
            _ => vec![Row {
                start: 0,
                end: len,
                start_col: 0,
            }],
        };
        Self { cols, len, rows }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The column of char boundary `index` in the unwrapped line.
    pub fn col(&self, index: usize) -> usize {
        let index = index.min(self.len);
        if self.cols.is_empty() {
            index
        } else {
            self.cols[index] as usize
        }
    }

    /// The width of the whole line in cells.
    pub fn width(&self) -> usize {
        self.col(self.len)
    }

    /// The row that holds char boundary `index`. The boundary at the end of
    /// a wrapped row belongs to the next row, except at the end of the line.
    pub fn row_of(&self, index: usize) -> usize {
        let index = index.min(self.len);
        match self.rows.iter().position(|row| index < row.end) {
            Some(row) => row,
            None => self.rows.len() - 1,
        }
    }

    /// The column of char boundary `index` within its row.
    pub fn col_in_row(&self, index: usize) -> (usize, usize) {
        let row = self.row_of(index);
        (row, self.col(index) - self.rows[row].start_col)
    }

    /// The char boundary in `row` nearest to column `col` of that row. A
    /// column past the row's end gives the row's end, except on a wrapped
    /// row, whose last boundary belongs to the next row: there it is the
    /// boundary before the last char.
    pub fn index_at(&self, row: usize, col: usize) -> usize {
        let row_index = row.min(self.rows.len() - 1);
        let row = self.rows[row_index];
        let target = row.start_col + col;
        let wrapped = row_index + 1 < self.rows.len();
        let last = if wrapped && row.end > row.start {
            row.end - 1
        } else {
            row.end
        };
        if self.cols.is_empty() {
            return target.clamp(row.start, last);
        }
        let mut best = row.start;
        for index in row.start..=last {
            let here = self.cols[index] as usize;
            if here > target {
                // Nearer to this boundary or the one before?
                let before = self.cols[index - 1] as usize;
                if index > row.start && target - before >= here - target {
                    best = index;
                }
                return best.min(last);
            }
            best = index;
        }
        best
    }
}

/// Greedy wrap at the last whitespace that fits.
fn wrap_rows(line: &str, cols: &[u32], width: usize) -> Vec<Row> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut rows = Vec::new();
    let mut start = 0usize;
    while start < len {
        let start_col = cols[start] as usize;
        let mut end = start;
        let mut last_break = None;
        while end < len && (cols[end + 1] as usize) - start_col <= width {
            if chars[end].is_whitespace() {
                last_break = Some(end + 1);
            }
            end += 1;
        }
        if end == len {
            rows.push(Row {
                start,
                end,
                start_col,
            });
            break;
        }
        // The space that ends a row stays on it, so a row never starts with
        // the space the line broke at.
        while end < len && chars[end] == ' ' {
            end += 1;
            last_break = None;
        }
        let cut = match last_break {
            Some(cut) if cut > start => cut,
            _ if end == start => start + 1,
            _ => end,
        };
        rows.push(Row {
            start,
            end: cut,
            start_col,
        });
        start = cut;
    }
    if rows.is_empty() {
        rows.push(Row {
            start: 0,
            end: 0,
            start_col: 0,
        });
    }
    rows
}

/// A piece of a row as drawn: text and the highlight kind of the source it
/// came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment<K> {
    pub text: String,
    pub kind: Option<K>,
    /// A shown whitespace or control character: drawn faint.
    pub faint: bool,
}

/// The pieces of row `row` of `line` with tabs expanded to spaces, control
/// characters as their pictures and, with `show_whitespace`, spaces as `·`
/// and tabs as `→`. `spans` are highlight runs in byte offsets of `line`,
/// sorted and not overlapping.
pub fn row_segments<K: Copy + PartialEq>(
    line: &str,
    layout: &LineLayout,
    row: usize,
    spans: &[(std::ops::Range<usize>, K)],
    tab_width: usize,
    show_whitespace: bool,
) -> Vec<Segment<K>> {
    let Some(row) = layout.rows.get(row).copied() else {
        return Vec::new();
    };
    let tab_width = tab_width.max(1);
    let mut out: Vec<Segment<K>> = Vec::new();
    let mut span_index = 0usize;
    let push = |out: &mut Vec<Segment<K>>, text: &str, kind: Option<K>, faint: bool| {
        if let Some(last) = out.last_mut()
            && last.kind == kind
            && last.faint == faint
        {
            last.text.push_str(text);
            return;
        }
        out.push(Segment {
            text: text.to_owned(),
            kind,
            faint,
        });
    };
    let long = layout.cols.is_empty();
    for (index, (byte, ch)) in line
        .char_indices()
        .enumerate()
        .skip(row.start)
        .take(row.end - row.start)
    {
        while span_index < spans.len() && spans[span_index].0.end <= byte {
            span_index += 1;
        }
        let kind = spans
            .get(span_index)
            .filter(|(range, _)| range.start <= byte)
            .map(|(_, kind)| *kind);
        let col = layout.col(index);
        match ch {
            '\t' if !long => {
                let cells = tab_width - (col % tab_width);
                if show_whitespace {
                    push(&mut out, "→", kind, true);
                    push(&mut out, &" ".repeat(cells.saturating_sub(1)), kind, false);
                } else {
                    push(&mut out, &" ".repeat(cells), kind, false);
                }
            }
            ' ' if show_whitespace => push(&mut out, "·", kind, true),
            ch if ch.is_control() => {
                let picture = control_picture(ch);
                push(&mut out, picture.encode_utf8(&mut [0; 4]), kind, true);
            }
            ch => push(&mut out, ch.encode_utf8(&mut [0; 4]), kind, false),
        }
    }
    out
}

/// The Unicode control picture for a C0 control character or DEL, else `�`.
fn control_picture(ch: char) -> char {
    match ch as u32 {
        code @ 0..=0x1f => char::from_u32(0x2400 + code).unwrap_or('\u{fffd}'),
        0x7f => '\u{2421}',
        _ => '\u{fffd}',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_run_to_the_next_stop() {
        let layout = LineLayout::new("a\tb", 4, None);
        assert_eq!(layout.col(1), 1);
        assert_eq!(layout.col(2), 4);
        assert_eq!(layout.width(), 5);
    }

    #[test]
    fn wide_characters_take_two_cells() {
        let layout = LineLayout::new("日本", 4, None);
        assert_eq!(layout.width(), 4);
        assert_eq!(layout.index_at(0, 3), 2);
        assert_eq!(layout.index_at(0, 2), 1);
    }

    #[test]
    fn wrap_breaks_after_spaces() {
        let layout = LineLayout::new("hello world again", 4, Some(8));
        let rows: Vec<(usize, usize)> =
            layout.rows.iter().map(|row| (row.start, row.end)).collect();
        assert_eq!(rows, vec![(0, 6), (6, 12), (12, 17)]);
        // The end of a wrapped row is the start of the next.
        assert_eq!(layout.row_of(6), 1);
        assert_eq!(layout.col_in_row(17), (2, 5));
    }

    #[test]
    fn a_word_wider_than_the_row_is_cut() {
        let layout = LineLayout::new("abcdefghij", 4, Some(4));
        assert_eq!(layout.rows.len(), 3);
        assert_eq!(layout.rows[2].start, 8);
    }

    #[test]
    fn index_at_snaps_to_the_nearest_boundary() {
        let layout = LineLayout::new("abc", 4, None);
        assert_eq!(layout.index_at(0, 0), 0);
        assert_eq!(layout.index_at(0, 2), 2);
        assert_eq!(layout.index_at(0, 99), 3);
    }

    #[test]
    fn long_lines_are_a_cell_a_char() {
        let line = "x".repeat(LONG_LINE + 10);
        let layout = LineLayout::new(&line, 4, Some(100));
        assert_eq!(layout.col(500), 500);
        assert_eq!(layout.rows.len(), (LONG_LINE + 10).div_ceil(100));
    }

    #[test]
    fn segments_expand_tabs_and_show_whitespace() {
        let layout = LineLayout::new("\tif x", 4, None);
        let spans = vec![(1..3, 1u8)];
        let plain = row_segments(
            " \tif x",
            &LineLayout::new(" \tif x", 4, None),
            0,
            &[] as &[(std::ops::Range<usize>, u8)],
            4,
            false,
        );
        assert_eq!(plain[0].text, "    if x");
        let shown = row_segments("\tif x", &layout, 0, &spans, 4, true);
        assert_eq!(shown[0].text, "→");
        assert!(shown[0].faint);
        assert_eq!(shown[1].text, "   ");
        assert_eq!(shown[2].text, "if");
        assert_eq!(shown[2].kind, Some(1));
        assert_eq!(shown[3].text, "·");
    }

    #[test]
    fn control_characters_are_pictures() {
        let line = "a\u{1b}[0m";
        let segments = row_segments(
            line,
            &LineLayout::new(line, 4, None),
            0,
            &[] as &[(std::ops::Range<usize>, u8)],
            4,
            false,
        );
        assert!(
            segments
                .iter()
                .any(|segment| segment.text == "\u{241b}" && segment.faint)
        );
    }
}
