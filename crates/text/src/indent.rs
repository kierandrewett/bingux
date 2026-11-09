//! Indentation: tabs or spaces, and how many spaces a level is.

use ropey::Rope;

/// How a document is indented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Indent {
    Tabs,
    Spaces(u8),
}

impl Default for Indent {
    fn default() -> Self {
        Indent::Spaces(4)
    }
}

impl Indent {
    /// The text of one level.
    pub fn unit(self) -> String {
        match self {
            Indent::Tabs => "\t".into(),
            Indent::Spaces(width) => " ".repeat(width.max(1) as usize),
        }
    }

    /// The width of one level in cells, with tabs `tab_width` wide.
    pub fn width(self, tab_width: usize) -> usize {
        match self {
            Indent::Tabs => tab_width,
            Indent::Spaces(width) => width.max(1) as usize,
        }
    }

    /// The status bar's words: `Tabs`, `Spaces: 4`.
    pub fn label(self) -> String {
        match self {
            Indent::Tabs => "Tabs".into(),
            Indent::Spaces(width) => format!("Spaces: {width}"),
        }
    }
}

/// How many lines detection reads.
const SAMPLE: usize = 5000;

/// The indentation of `rope`, or `None` when it has no indented line to tell.
///
/// Counts lines that start with a tab against lines that start with spaces,
/// and for spaces takes the most common step between one line's indent and
/// the next one's (the way the indentation grows), preferring 2, 4 and 8.
pub fn detect(rope: &Rope) -> Option<Indent> {
    let mut tabs = 0usize;
    let mut spaces = 0usize;
    let mut steps = [0usize; 9];
    let mut previous = 0usize;
    for line in rope.lines().take(SAMPLE) {
        let mut chars = line.chars();
        let first = chars.next();
        match first {
            Some('\t') => {
                tabs += 1;
                previous = 0;
            }
            Some(' ') => {
                let width = 1 + line.chars().skip(1).take_while(|ch| *ch == ' ').count();
                // A line of spaces only, or a block comment's ` *`, says nothing.
                let rest = line.chars().nth(width);
                if matches!(rest, None | Some('\n') | Some('*')) {
                    continue;
                }
                spaces += 1;
                let step = width.abs_diff(previous);
                if (1..=8).contains(&step) {
                    steps[step] += 1;
                }
                previous = width;
            }
            Some('\n') | None => {}
            Some(_) => previous = 0,
        }
    }
    if tabs == 0 && spaces == 0 {
        return None;
    }
    if tabs > spaces {
        return Some(Indent::Tabs);
    }
    let best = (1..=8)
        .max_by_key(|step| {
            // Ties go to the usual widths.
            let bonus = match step {
                4 => 3,
                2 => 2,
                8 => 1,
                _ => 0,
            };
            steps[*step] * 4 + bonus
        })
        .unwrap_or(4);
    if steps[best] == 0 {
        return Some(Indent::Spaces(4));
    }
    Some(Indent::Spaces(best as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_spaces() {
        let rope = Rope::from_str("a:\n  b:\n    c\n  d\ne\n");
        assert_eq!(detect(&rope), Some(Indent::Spaces(2)));
    }

    #[test]
    fn four_spaces() {
        let rope = Rope::from_str("fn a() {\n    if x {\n        y\n    }\n}\n");
        assert_eq!(detect(&rope), Some(Indent::Spaces(4)));
    }

    #[test]
    fn tabs() {
        let rope = Rope::from_str("a:\n\tb\n\t\tc\n\td\n");
        assert_eq!(detect(&rope), Some(Indent::Tabs));
    }

    #[test]
    fn nothing_indented() {
        assert_eq!(detect(&Rope::from_str("one\ntwo\n")), None);
    }

    #[test]
    fn block_comments_do_not_count() {
        let rope = Rope::from_str("/**\n * doc\n */\nfn a() {\n    b\n}\n");
        assert_eq!(detect(&rope), Some(Indent::Spaces(4)));
    }
}
