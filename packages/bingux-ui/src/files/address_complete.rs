//! The rows under the address field while a path is typed: what the model
//! answered, put in the order a person reads it, with the part that matches
//! marked. Plain data and functions, so the ordering and the text a pick
//! produces are tested without a window.

use std::ffi::OsStr;
use std::ops::Range;
use std::path::PathBuf;

use bingux_files::location::Completion;
use bingux_files::places::{SpecialFolder, special_folder_in};

/// One suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Row {
    pub name: String,
    pub is_dir: bool,
    /// The bytes of `name` that match what was typed, to draw in bold.
    pub matched: Range<usize>,
    /// Which user folder this is, by its path as shown.
    pub special: Option<SpecialFolder>,
}

/// The answer to one typed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Suggestions {
    /// The folder that was searched.
    pub dir: PathBuf,
    /// The text this answers.
    pub typed: String,
    pub rows: Vec<Row>,
    /// What Tab fills in: the text with the longest shared start added.
    pub completed: String,
    /// The model left candidates out.
    pub truncated: bool,
    /// The folder has not answered yet (a slow share): ask again later.
    pub incomplete: bool,
}

impl Suggestions {
    /// The rows of `completion`, folders before files within the names that
    /// start with what was typed and again within those that only contain
    /// it. Only folders when `folders_only`.
    pub(super) fn of(typed: &str, completion: &Completion, folders_only: bool) -> Suggestions {
        let mut near: Vec<&bingux_files::location::Candidate> = Vec::new();
        let mut far: Vec<&bingux_files::location::Candidate> = Vec::new();
        for (index, candidate) in completion.candidates.iter().enumerate() {
            if folders_only && !candidate.is_dir {
                continue;
            }
            if index < completion.prefix_matches {
                near.push(candidate);
            } else {
                far.push(candidate);
            }
        }
        // `sort_by_key` is stable: the model's order holds inside each kind.
        near.sort_by_key(|candidate| !candidate.is_dir);
        far.sort_by_key(|candidate| !candidate.is_dir);
        let rows = near
            .into_iter()
            .chain(far)
            .map(|candidate| Row {
                matched: find_match(&candidate.name, &completion.prefix),
                special: candidate
                    .is_dir
                    .then(|| special_folder_in(&completion.dir, OsStr::new(&candidate.name)))
                    .flatten(),
                name: candidate.name.clone(),
                is_dir: candidate.is_dir,
            })
            .collect();
        Suggestions {
            dir: completion.dir.clone(),
            typed: typed.to_string(),
            rows,
            completed: completion.apply(typed),
            truncated: completion.truncated,
            incomplete: completion.incomplete,
        }
    }

    /// The text after picking row `index`: the typed folder part, the name,
    /// and a slash after a folder so typing goes on inside it.
    pub(super) fn pick(&self, index: usize) -> Option<String> {
        let row = self.rows.get(index)?;
        let head = &self.typed[..self.typed.rfind('/').map_or(0, |at| at + 1)];
        let mut text = format!("{head}{}", row.name);
        if row.is_dir {
            text.push('/');
        }
        Some(text)
    }
}

/// The bytes of `name` that match `typed`: the start when it begins with it,
/// ignoring case, else the first place it occurs. Empty when it does not.
pub(super) fn find_match(name: &str, typed: &str) -> Range<usize> {
    if typed.is_empty() {
        return 0..0;
    }
    let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    for (start, _) in name.char_indices() {
        let mut end = start;
        let mut chars = name[start..].chars();
        let mut all = true;
        for wanted in typed.chars() {
            match chars.next() {
                Some(got) if same(got, wanted) => end += got.len_utf8(),
                _ => {
                    all = false;
                    break;
                }
            }
        }
        if all {
            return start..end;
        }
    }
    0..0
}

/// The highlighted row after Down (`forward`) or Up, wrapping round the
/// ends. With no highlight Down takes the first row and Up the last.
pub(super) fn step(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, forward) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(at), true) => (at + 1) % count,
        (Some(at), false) => (at + count - 1) % count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_files::location::Candidate;

    fn candidate(name: &str, is_dir: bool) -> Candidate {
        Candidate {
            name: name.into(),
            is_dir,
        }
    }

    fn completion(candidates: Vec<Candidate>, prefix: &str, prefix_matches: usize) -> Completion {
        Completion {
            dir: PathBuf::from("/tmp/x"),
            prefix: prefix.into(),
            common: prefix.into(),
            candidates,
            prefix_matches,
            truncated: false,
            incomplete: false,
        }
    }

    #[test]
    fn folders_come_before_files_inside_each_kind_of_match() {
        let found = completion(
            vec![
                candidate("Pad.txt", false),
                candidate("Pictures", true),
                candidate("Photos", true),
                candidate("a-pdf", false),
                candidate("Escape", true),
            ],
            "p",
            3,
        );
        let rows = Suggestions::of("/tmp/x/p", &found, false).rows;
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        // Names that start with p first, folders first; then the rest.
        assert_eq!(names, ["Pictures", "Photos", "Pad.txt", "Escape", "a-pdf"]);
    }

    #[test]
    fn folders_only_drops_files() {
        let found = completion(vec![candidate("a", false), candidate("b", true)], "", 2);
        let rows = Suggestions::of("/tmp/x/", &found, true).rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "b");
    }

    #[test]
    fn a_pick_keeps_what_was_typed_before_the_last_slash() {
        let found = completion(
            vec![candidate("Pictures", true), candidate("pad.txt", false)],
            "p",
            2,
        );
        let suggestions = Suggestions::of("~/Docs/p", &found, false);
        assert_eq!(suggestions.pick(0).as_deref(), Some("~/Docs/Pictures/"));
        assert_eq!(suggestions.pick(1).as_deref(), Some("~/Docs/pad.txt"));
        assert_eq!(suggestions.pick(2), None);
        let root = Suggestions::of("p", &found, false);
        assert_eq!(root.pick(0).as_deref(), Some("Pictures/"));
    }

    #[test]
    fn the_match_is_marked_ignoring_case() {
        assert_eq!(find_match("Pictures", "pic"), 0..3);
        assert_eq!(find_match("My Pictures", "pic"), 3..6);
        assert_eq!(find_match("Pictures", "xyz"), 0..0);
        assert_eq!(find_match("Pictures", ""), 0..0);
        // Bytes, not characters: é takes two.
        assert_eq!(find_match("Écoute", "éco"), 0..4);
    }

    #[test]
    fn the_highlight_wraps() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), Some(0));
        assert_eq!(step(Some(0), 3, false), Some(2));
        assert_eq!(step(Some(1), 3, true), Some(2));
        assert_eq!(step(None, 0, true), None);
    }
}
