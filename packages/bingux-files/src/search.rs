//! Searching file names: what a typed text means, how names are matched and
//! ranked, and the walker that looks in the subfolders.
//!
//! Search is not the file type [`Filter`](crate::filter::Filter). A filter is
//! part of the request (`*.png`, `image/*`) and says which kinds of file a
//! chooser may return; a search is a [`Query`] a person types, and it only
//! narrows what they see. Both apply: results of a search respect the filter.
//! The two never share a type, a field or a count.
//!
//! # What a typed text is
//!
//! The address bar is one text field for paths and for searches.
//! [`classify`] decides, without touching the disk:
//!
//! | Text | Is |
//! | ---- | -- |
//! | empty | a path (nothing typed) |
//! | starts with `/`, `~`, `./` or `../`, or is `.`, `..`, `~` | a path |
//! | starts with a scheme: `smb://`, `sftp://`, `file://`, ... | a path |
//! | its first component names an entry of the folder (`Documents`, `Documents/Sub`) | a path |
//! | anything else | a [`Query`] |
//!
//! "An entry of the folder" is decided from the names the caller hands over
//! (the listing already in memory), never by asking the disk.
//!
//! # Matching, and the order of results
//!
//! Matching ignores case and looks at the file name only (not its content;
//! content search is not offered). [`Query::matches`] says how well a name
//! matches, best first:
//!
//! 1. [`Tier::Exact`]: the name is the query.
//! 2. [`Tier::Prefix`]: the name starts with it.
//! 3. [`Tier::Word`]: it appears at the start of a word (after a space, `-`,
//!    `_`, `.` or another non-letter).
//! 4. [`Tier::Substring`]: it appears somewhere in the name.
//! 5. [`Tier::Words`]: each space-separated word of the query appears, in any
//!    order.
//! 6. [`Tier::Fuzzy`]: the letters appear in order with others between them
//!    (`rpt` finds `my_report.txt`). Only for queries of three letters or
//!    more, and only when they sit close together.
//!
//! [`rank`] keeps the entries that match and orders them by tier; inside a
//! tier the fuzzy matches with the tightest fit come first and everything
//! else keeps the order it was given (the sort the person chose).
//!
//! # Looking in the subfolders
//!
//! [`walk`] reads a folder tree on the calling thread (a view runs it on a
//! worker of its own), breadth first so that shallow matches arrive first,
//! and hands the matches over in batches. It never follows a symbolic link,
//! stays on the filesystem of the folder it started in, never enters a folder
//! twice, skips hidden entries unless [`WalkOptions::show_hidden`], and, when
//! [`WalkOptions::include_noise`] is off, skips the folders of
//! [`NOISE_FOLDERS`]. The default includes everything the person can see.
//! A chooser's file type filter goes in [`WalkOptions::filter`], so the walk
//! reports (and counts toward its limit) only the files the chooser accepts.
//! It checks its [`Cancel`] for every entry, so a changed query or a new
//! folder stops it within one directory read.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::ops::Range;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::entry::Entry;
use crate::filter::Filter;
use crate::mime;
use crate::listing::{Cancel, hidden_names};

// ---------------------------------------------------------------------------
// Path or query
// ---------------------------------------------------------------------------

/// What a typed text is; see [`classify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed<'a> {
    /// A location to go to. Includes the empty text.
    Path,
    /// A search for these words, trimmed.
    Query(&'a str),
}

/// Whether `text` starts with a URI scheme (`smb://`, `sftp://`).
fn has_scheme(text: &str) -> bool {
    let Some(at) = text.find("://") else {
        return false;
    };
    let scheme = &text[..at];
    let mut chars = scheme.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Decide whether `text` is a path or a search. `is_child` says whether a
/// name is an entry of the folder on show; it must not read the disk.
pub fn classify<'a>(text: &'a str, is_child: impl Fn(&str) -> bool) -> Typed<'a> {
    let text = text.trim();
    if text.is_empty() {
        return Typed::Path;
    }
    if text.starts_with(['/', '~'])
        || text.starts_with("./")
        || text.starts_with("../")
        || text == "."
        || text == ".."
        || has_scheme(text)
    {
        return Typed::Path;
    }
    let first = text.split('/').next().unwrap_or(text);
    if !first.is_empty() && is_child(first) {
        return Typed::Path;
    }
    Typed::Query(text)
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// How well a name matches a query; earlier is better.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Exact,
    Prefix,
    Word,
    Substring,
    Words,
    Fuzzy,
}

/// How a name matches: its tier, how loose a fuzzy match is, and the bytes
/// of the name to draw heavier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub tier: Tier,
    /// Zero except for [`Tier::Fuzzy`]: larger is a looser fit.
    pub penalty: u32,
    /// Byte ranges of the name, sorted, not overlapping, on character
    /// boundaries.
    pub ranges: Vec<Range<usize>>,
}

/// The letters of a text compared without case.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Fuzzy matching starts at this many letters; fewer would match nearly
/// everything.
const FUZZY_MIN: usize = 3;

/// What a person is looking for in names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    text: String,
    folded: Vec<char>,
    terms: Vec<Vec<char>>,
}

impl Query {
    /// A query for `text`, or `None` when it is blank.
    pub fn new(text: &str) -> Option<Query> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let folded: Vec<char> = text.chars().map(fold).collect();
        let terms = text
            .split_whitespace()
            .map(|term| term.chars().map(fold).collect())
            .collect();
        Some(Query {
            text: text.to_string(),
            folded,
            terms,
        })
    }

    /// The text, trimmed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How `name` matches, or `None` when it does not.
    pub fn matches(&self, name: &str) -> Option<Hit> {
        let chars: Vec<(usize, char)> = name.char_indices().map(|(at, c)| (at, fold(c))).collect();
        let name_len = name.len();
        let byte_at = |index: usize| chars.get(index).map_or(name_len, |(at, _)| *at);
        let range_of = |from: usize, count: usize| byte_at(from)..byte_at(from + count);
        let hit = |tier, ranges| {
            Some(Hit {
                tier,
                penalty: 0,
                ranges,
            })
        };
        let query = &self.folded;
        let find = |needle: &[char], from: usize, at_word: bool| -> Option<usize> {
            if needle.is_empty() || chars.len() < needle.len() {
                return None;
            }
            (from..=chars.len() - needle.len()).find(|&start| {
                let starts = start == 0 || !chars[start - 1].1.is_alphanumeric();
                (!at_word || starts)
                    && needle
                        .iter()
                        .enumerate()
                        .all(|(offset, c)| chars[start + offset].1 == *c)
            })
        };
        if chars.len() == query.len() && find(query, 0, false) == Some(0) {
            return hit(Tier::Exact, vec![0..name_len]);
        }
        if find(query, 0, false) == Some(0) {
            return hit(Tier::Prefix, vec![range_of(0, query.len())]);
        }
        if let Some(start) = find(query, 0, true) {
            return hit(Tier::Word, vec![range_of(start, query.len())]);
        }
        if let Some(start) = find(query, 0, false) {
            return hit(Tier::Substring, vec![range_of(start, query.len())]);
        }
        if self.terms.len() > 1 {
            let mut ranges = Vec::new();
            for term in &self.terms {
                let start = find(term, 0, true).or_else(|| find(term, 0, false))?;
                ranges.push(range_of(start, term.len()));
            }
            return hit(Tier::Words, merged(ranges));
        }
        self.fuzzy(&chars, &byte_at)
    }

    /// The letters in order, as tightly as they fit.
    fn fuzzy(&self, chars: &[(usize, char)], byte_at: &dyn Fn(usize) -> usize) -> Option<Hit> {
        let letters: Vec<char> = self.folded.iter().copied().filter(|c| !c.is_whitespace()).collect();
        if letters.len() < FUZZY_MIN || letters.len() > chars.len() {
            return None;
        }
        // Forward to where the last letter lands, then back to the latest
        // start that still fits: the tightest window ending there.
        let mut next = 0;
        let mut end = None;
        for (index, (_, c)) in chars.iter().enumerate() {
            if *c == letters[next] {
                next += 1;
                if next == letters.len() {
                    end = Some(index);
                    break;
                }
            }
        }
        let end = end?;
        let mut positions = vec![end];
        let mut wanted = letters.len() - 1;
        let mut index = end;
        while wanted > 0 {
            index -= 1;
            if chars[index].1 == letters[wanted - 1] {
                wanted -= 1;
                positions.push(index);
            }
        }
        positions.reverse();
        let start = positions[0];
        let window = end - start + 1;
        if window > letters.len() * 2 + 4 {
            return None;
        }
        let ranges = merged(
            positions
                .iter()
                .map(|position| byte_at(*position)..byte_at(*position + 1))
                .collect(),
        );
        let penalty = ((window - letters.len()) * 4 + start) as u32;
        Some(Hit {
            tier: Tier::Fuzzy,
            penalty,
            ranges,
        })
    }

    /// The bytes of `name` to draw heavier; empty when it does not match.
    pub fn highlight(&self, name: &str) -> Vec<Range<usize>> {
        self.matches(name).map(|hit| hit.ranges).unwrap_or_default()
    }
}

/// Sort `ranges` and join those that touch.
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match out.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
    out
}

/// The members of `order` (indices into `entries`) whose names match
/// `query`, best match first; see the module documentation for the order.
pub fn rank(entries: &[Entry], order: &[usize], query: &Query) -> Vec<usize> {
    let mut found: Vec<(Tier, u32, usize)> = order
        .iter()
        .enumerate()
        .filter_map(|(place, &index)| {
            let hit = query.matches(&entries[index].name)?;
            Some((hit.tier, hit.penalty, place))
        })
        .collect();
    found.sort_unstable();
    found.into_iter().map(|(_, _, place)| order[place]).collect()
}

/// The folder of `path` as a person reads it below `root`: `""` for a file
/// directly in `root`, `Photos/2024` for one two folders down.
pub fn relative_folder(path: &Path, root: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    match parent.strip_prefix(root) {
        Ok(rest) => rest.to_string_lossy().into_owned(),
        Err(_) => parent.to_string_lossy().into_owned(),
    }
}

// ---------------------------------------------------------------------------
// Walking subfolders
// ---------------------------------------------------------------------------

/// Folders that hold what other tools generate and a person rarely means to
/// find: skipped when [`WalkOptions::include_noise`] is off.
pub const NOISE_FOLDERS: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    "target",
    "__pycache__",
    ".venv",
    ".tox",
    ".gradle",
    ".cache",
];

/// How [`walk`] looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkOptions {
    /// Look in hidden folders and report hidden entries.
    pub show_hidden: bool,
    /// Look in [`NOISE_FOLDERS`]. On by default: the search finds everything
    /// the person could see by browsing.
    pub include_noise: bool,
    /// The file type filter of a chooser: files it does not accept are not
    /// results and do not count toward `max_hits` (folders always are).
    pub filter: Option<Filter>,
    /// Stop after this many matches and say so.
    pub max_hits: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        WalkOptions {
            show_hidden: false,
            include_noise: true,
            filter: None,
            max_hits: 10_000,
        }
    }
}

/// What [`walk`] sends to its caller.
#[derive(Clone, Debug, PartialEq)]
pub enum Walked {
    /// More matches, each with its size and dates.
    Hits(Vec<Entry>),
    /// How many folders have been read so far.
    Progress { folders: usize },
}

/// How a walk ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub hits: usize,
    pub folders: usize,
    /// Folders that could not be read (no permission, gone, not answering).
    pub unreadable: usize,
    /// `max_hits` was reached.
    pub truncated: bool,
    /// The caller cancelled, or `emit` said stop.
    pub cancelled: bool,
}

/// The longest a match waits for others before it is sent.
const BATCH_AGE: Duration = Duration::from_millis(50);
/// The most matches in one batch.
const BATCH_SIZE: usize = 64;

/// Look for `query` in the names below `root`, handing matches to `emit` as
/// they are found. `emit` returns false to stop. Blocks: call it from a
/// worker. See the module documentation for what it skips.
#[allow(unused_assignments)]
pub fn walk(
    root: &Path,
    query: &Query,
    options: &WalkOptions,
    cancel: &Cancel,
    emit: &mut dyn FnMut(Walked) -> bool,
) -> Summary {
    let mut summary = Summary::default();
    let Ok(root_meta) = fs::metadata(root) else {
        summary.unreadable = 1;
        return summary;
    };
    let device = root_meta.dev();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    seen.insert((root_meta.dev(), root_meta.ino()));
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    let matcher = options.filter.as_ref().map(Filter::matcher);
    let mut batch: Vec<Entry> = Vec::new();
    let mut last_flush = Instant::now();
    let mut seen_entries = 0_usize;

    macro_rules! flush {
        () => {{
            let mut keep_going = true;
            if !batch.is_empty() {
                keep_going = emit(Walked::Hits(std::mem::take(&mut batch)));
            }
            if keep_going {
                keep_going = emit(Walked::Progress {
                    folders: summary.folders,
                });
            }
            last_flush = Instant::now();
            if !keep_going {
                summary.cancelled = true;
            }
            keep_going
        }};
    }

    'folders: while let Some(folder) = queue.pop_front() {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let Ok(reader) = fs::read_dir(&folder) else {
            summary.unreadable += 1;
            continue;
        };
        summary.folders += 1;
        let listed_hidden = if options.show_hidden {
            HashSet::new()
        } else {
            hidden_names(&folder)
        };
        for item in reader {
            seen_entries += 1;
            if seen_entries % 32 == 0 && cancel.is_cancelled() {
                summary.cancelled = true;
                break 'folders;
            }
            let Ok(item) = item else { continue };
            let file_name = item.file_name();
            let name = file_name.to_string_lossy().into_owned();
            let hidden = name.starts_with('.') || listed_hidden.contains(name.as_str());
            if hidden && !options.show_hidden {
                continue;
            }
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            let is_folder = file_type.is_dir();
            if is_folder && !options.include_noise && NOISE_FOLDERS.contains(&name.as_str()) {
                continue;
            }
            let path = folder.join(&file_name);
            let passes = is_folder
                || matcher
                    .as_ref()
                    .is_none_or(|matcher| matcher.accepts(&name, mime::for_name(&name).mime));
            if passes && query.matches(&name).is_some() {
                if let Ok(entry) = Entry::from_path(&path, hidden) {
                    batch.push(entry);
                    summary.hits += 1;
                    if summary.hits >= options.max_hits {
                        summary.truncated = true;
                        flush!();
                        break 'folders;
                    }
                }
            }
            // A symbolic link is a result but never a way in, so a link back
            // up the tree cannot make the walk circle.
            if is_folder
                && let Ok(meta) = fs::symlink_metadata(&path)
                && meta.dev() == device
                && seen.insert((meta.dev(), meta.ino()))
            {
                queue.push_back(path);
            }
            if batch.len() >= BATCH_SIZE || (!batch.is_empty() && last_flush.elapsed() >= BATCH_AGE)
            {
                if !flush!() {
                    break 'folders;
                }
            }
        }
        if last_flush.elapsed() >= BATCH_AGE && !flush!() {
            break;
        }
    }
    if !summary.cancelled && !summary.truncated {
        flush!();
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn names(text: &str) -> impl Fn(&str) -> bool + '_ {
        move |name| text.split_whitespace().any(|known| known == name)
    }

    #[test]
    fn tells_paths_from_queries() {
        let known = names("Documents Pictures notes.txt");
        let kind = |text| classify(text, &known);
        for path in [
            "", "   ", "/", "/home", "~", "~/x", "./a", "../a", ".", "..", "smb://host/share",
            "sftp://u@h/", "file:///tmp", "Documents", "Documents/", "Documents/Sub",
            "notes.txt",
        ] {
            assert_eq!(kind(path), Typed::Path, "{path:?}");
        }
        assert_eq!(kind("rep"), Typed::Query("rep"));
        assert_eq!(kind("  report 2024 "), Typed::Query("report 2024"));
        assert_eq!(kind("Docu"), Typed::Query("Docu"));
        assert_eq!(kind("documents"), Typed::Query("documents"));
        assert_eq!(kind("a:b"), Typed::Query("a:b"));
        assert_eq!(kind("1http://x"), Typed::Query("1http://x"));
        assert_eq!(kind(".hidden"), Typed::Query(".hidden"));
    }

    fn tier(query: &str, name: &str) -> Option<Tier> {
        Query::new(query)?.matches(name).map(|hit| hit.tier)
    }

    #[test]
    fn ranks_matches_from_exact_to_fuzzy() {
        assert_eq!(tier("report", "Report"), Some(Tier::Exact));
        assert_eq!(tier("rep", "report.pdf"), Some(Tier::Prefix));
        assert_eq!(tier("pdf", "annual-report.pdf"), Some(Tier::Word));
        assert_eq!(tier("port", "report.pdf"), Some(Tier::Substring));
        assert_eq!(tier("2024 report", "report for 2024.txt"), Some(Tier::Words));
        assert_eq!(tier("rpt", "my_report.txt"), Some(Tier::Fuzzy));
        assert_eq!(tier("zzz", "report.pdf"), None);
        // Two letters never go fuzzy; far-apart letters do not match.
        assert_eq!(tier("rt", "report.pdf"), Some(Tier::Substring));
        assert_eq!(tier("rp", "report.pdf"), None);
        assert_eq!(tier("rxz", "report.txt"), None);
        assert_eq!(tier("abc", "a------------------b------------------c"), None);
        assert!(Query::new("  ").is_none());
    }

    #[test]
    fn highlights_by_byte_on_character_boundaries() {
        let query = Query::new("ÉCOLE").unwrap();
        let name = "mon école.txt";
        let ranges = query.highlight(name);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&name[ranges[0].clone()], "école");
        let fuzzy = Query::new("rpt").unwrap().highlight("my_report.txt");
        let picked: Vec<&str> = fuzzy.iter().map(|r| &"my_report.txt"[r.clone()]).collect();
        assert_eq!(picked.concat(), "rpt");
        assert!(Query::new("zz").unwrap().highlight("a").is_empty());
    }

    fn entry(name: &str) -> Entry {
        Entry::named(
            PathBuf::from("/x").join(name),
            name.to_string(),
            crate::entry::Kind::File,
            false,
            false,
        )
    }

    #[test]
    fn rank_orders_by_tier_then_keeps_the_given_order() {
        let entries: Vec<Entry> = ["z-notes.txt", "znotes.txt", "notes.txt", "my_n_o_t_e.txt", "notes", "a notes b", "n-o-t-e-s.md", "other"]
            .into_iter()
            .map(entry)
            .collect();
        let order: Vec<usize> = (0..entries.len()).collect();
        let ranked = rank(&entries, &order, &Query::new("notes").unwrap());
        let by_name: Vec<&str> = ranked.iter().map(|i| entries[*i].name.as_str()).collect();
        assert_eq!(
            by_name,
            ["notes", "notes.txt", "z-notes.txt", "a notes b", "znotes.txt", "n-o-t-e-s.md"]
        );
        // The caller's order decides within a tier.
        let reversed: Vec<usize> = order.iter().rev().copied().collect();
        let ranked = rank(&entries, &reversed, &Query::new("notes").unwrap());
        let by_name: Vec<&str> = ranked.iter().map(|i| entries[*i].name.as_str()).collect();
        assert_eq!(by_name[..2], ["notes", "notes.txt"]);
        assert_eq!(by_name[2..4], ["a notes b", "z-notes.txt"]);
        assert_eq!(by_name[4], "znotes.txt");
    }

    fn tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bingux-search-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn run(root: &Path, text: &str, options: &WalkOptions) -> (Vec<String>, Summary) {
        let query = Query::new(text).unwrap();
        let mut found = Vec::new();
        let summary = walk(root, &query, options, &Cancel::new(), &mut |walked| {
            if let Walked::Hits(entries) = walked {
                found.extend(entries.into_iter().map(|e| relative_folder(&e.path, root) + "|" + &e.name));
            }
            true
        });
        found.sort();
        (found, summary)
    }

    #[test]
    fn walks_subfolders_and_skips_what_it_should() {
        let root = tree("walk");
        for dir in ["a/b", ".secret", "node_modules/pkg", ".git", "target"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "needle.txt",
            "a/needle-a.txt",
            "a/b/needle-b.txt",
            ".secret/needle-s.txt",
            ".needle-hidden",
            "node_modules/pkg/needle-n.js",
            "target/needle-t.o",
            ".git/needle-g",
            "a/other.txt",
        ] {
            fs::write(root.join(file), b"x").unwrap();
        }
        // A link up the tree is reported when it matches but never entered.
        symlink(&root, root.join("a/needle-loop")).unwrap();

        let (found, summary) = run(&root, "needle", &WalkOptions::default());
        assert_eq!(
            found,
            [
                "a/b|needle-b.txt",
                "a|needle-a.txt",
                "a|needle-loop",
                "node_modules/pkg|needle-n.js",
                "target|needle-t.o",
                "|needle.txt",
            ]
        );
        assert!(!summary.truncated && !summary.cancelled);
        assert_eq!(summary.hits, 6);

        let (found, _) = run(&root, "needle", &WalkOptions { show_hidden: true, ..WalkOptions::default() });
        assert!(found.contains(&".secret|needle-s.txt".to_string()));
        assert!(found.contains(&"|.needle-hidden".to_string()));
        assert!(found.contains(&".git|needle-g".to_string()));

        let quiet = WalkOptions { include_noise: false, ..WalkOptions::default() };
        let (found, _) = run(&root, "needle", &quiet);
        assert!(!found.iter().any(|f| f.contains("needle-n.js") || f.contains("needle-t.o")));
        assert!(found.contains(&"a/b|needle-b.txt".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_file_type_filter_applies_before_the_limit() {
        let root = tree("filter");
        fs::create_dir_all(root.join("sub")).unwrap();
        for file in ["a.txt", "b.txt", "c.png", "sub/d.png", "sub/e.txt"] {
            fs::write(root.join(file), b"x").unwrap();
        }
        fs::create_dir_all(root.join("sub/find-folder")).unwrap();
        let only_png = WalkOptions {
            filter: Some(Filter::parse("Pictures", "png")),
            max_hits: 3,
            ..WalkOptions::default()
        };
        let query = Query::new(".").unwrap();
        let mut found = Vec::new();
        let summary = walk(&root, &query, &only_png, &Cancel::new(), &mut |walked| {
            if let Walked::Hits(entries) = walked {
                found.extend(entries.into_iter().map(|e| e.name));
            }
            true
        });
        found.sort();
        // Two pictures pass; the folder `sub` has no dot and does not match.
        assert_eq!(found, ["c.png", "d.png"]);
        assert!(!summary.truncated, "{summary:?}");
        let query = Query::new("s").unwrap();
        let mut folders = Vec::new();
        walk(&root, &query, &only_png, &Cancel::new(), &mut |walked| {
            if let Walked::Hits(entries) = walked {
                folders.extend(entries.into_iter().filter(Entry::is_dir).map(|e| e.name));
            }
            true
        });
        folders.sort();
        assert_eq!(folders, ["sub"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn does_not_cross_into_another_filesystem() {
        // `/dev/shm` is a tmpfs inside the devtmpfs `/dev`: a different
        // device, so a walk of `/dev` must not enter it.
        let (Ok(outer), Ok(inner)) = (fs::metadata("/dev"), fs::metadata("/dev/shm")) else {
            return;
        };
        if outer.dev() == inner.dev() {
            return;
        }
        let name = format!("bingux-needle-{}", std::process::id());
        let file = Path::new("/dev/shm").join(&name);
        if fs::write(&file, b"x").is_err() {
            return;
        }
        let (across, _) = run(Path::new("/dev"), &name, &WalkOptions::default());
        let (inside, _) = run(Path::new("/dev/shm"), &name, &WalkOptions::default());
        let _ = fs::remove_file(&file);
        assert!(across.is_empty(), "{across:?}");
        assert_eq!(inside.len(), 1);
    }

    #[test]
    fn streams_stops_when_cancelled_and_when_the_limit_is_reached() {
        let root = tree("cancel");
        for dir in 0..20 {
            fs::create_dir_all(root.join(format!("d{dir}"))).unwrap();
            for file in 0..10 {
                fs::write(root.join(format!("d{dir}/hit-{file}")), b"x").unwrap();
            }
        }
        let query = Query::new("hit").unwrap();

        let limited = WalkOptions { max_hits: 25, ..WalkOptions::default() };
        let mut count = 0;
        let summary = walk(&root, &query, &limited, &Cancel::new(), &mut |walked| {
            if let Walked::Hits(entries) = walked {
                count += entries.len();
            }
            true
        });
        assert!(summary.truncated);
        assert_eq!(count, 25);

        // The caller cancels while results stream: nothing more is read.
        let cancel = Cancel::new();
        let mut batches = 0;
        let stopper = cancel.clone();
        let summary = walk(&root, &query, &WalkOptions::default(), &cancel, &mut |walked| {
            if matches!(walked, Walked::Hits(_)) {
                batches += 1;
                stopper.cancel();
            }
            true
        });
        assert!(summary.cancelled);
        assert_eq!(batches, 1);
        assert!(summary.hits < 200);

        // A flag set before the walk starts means no work.
        let cancel = Cancel::new();
        cancel.cancel();
        let summary = walk(&root, &query, &WalkOptions::default(), &cancel, &mut |_| true);
        assert!(summary.cancelled);
        assert_eq!(summary.hits, 0);

        // `emit` returning false stops it too.
        let summary = walk(&root, &query, &WalkOptions::default(), &Cancel::new(), &mut |_| false);
        assert!(summary.cancelled);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn relative_folders_read_below_the_root() {
        let root = Path::new("/h/me");
        assert_eq!(relative_folder(Path::new("/h/me/a.txt"), root), "");
        assert_eq!(relative_folder(Path::new("/h/me/Photos/2024/a.jpg"), root), "Photos/2024");
        assert_eq!(relative_folder(Path::new("/elsewhere/a.jpg"), root), "/elsewhere");
    }
}
