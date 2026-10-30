//! The live walk: reading folders and testing every name against a
//! [`Matcher`], breadth first so that shallow matches arrive first, handing
//! the matches over in batches.
//!
//! It never follows a symbolic link (a link is a result, never a way in),
//! stays on the filesystem of the folder it started in, never enters a
//! folder twice, skips hidden entries unless [`WalkOptions::show_hidden`]
//! and, when [`WalkOptions::include_noise`] is off, skips the folders of
//! [`NOISE_FOLDERS`]. It checks its [`Cancel`] for every 32 entries, so a
//! changed query or a new folder stops it within one directory read.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::matcher::Matcher;
use crate::entry::Entry;
use crate::filter::Filter;
use crate::listing::{Cancel, hidden_names};
use crate::mime;

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

/// Look for what `matcher` accepts below each of `roots`, handing matches to
/// `emit` as they are found; `emit` returns false to stop. Paths in `known`
/// are skipped (they were reported already). Blocks: call it from a worker.
#[allow(unused_assignments)]
pub fn walk(
    roots: &[PathBuf],
    matcher: &Matcher,
    options: &WalkOptions,
    known: &HashSet<PathBuf>,
    cancel: &Cancel,
    emit: &mut dyn FnMut(Walked) -> bool,
) -> Summary {
    let mut summary = Summary::default();
    let chooser = options.filter.as_ref().map(Filter::matcher);
    let mut batch: Vec<Entry> = Vec::new();
    let mut last_flush = Instant::now();
    let mut seen_entries = 0_usize;
    let mut seen: HashSet<(u64, u64)> = HashSet::new();

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

    'roots: for root in roots {
        let Ok(root_meta) = fs::metadata(root) else {
            summary.unreadable += 1;
            continue;
        };
        let device = root_meta.dev();
        if !seen.insert((root_meta.dev(), root_meta.ino())) {
            continue;
        }
        let mut queue: VecDeque<PathBuf> = VecDeque::new();
        queue.push_back(root.clone());
        'folders: while let Some(folder) = queue.pop_front() {
            if cancel.is_cancelled() {
                summary.cancelled = true;
                break 'roots;
            }
            let Ok(reader) = fs::read_dir(&folder) else {
                summary.unreadable += 1;
                continue;
            };
            summary.folders += 1;
            let listed_hidden = if options.show_hidden {
                std::collections::HashSet::new()
            } else {
                hidden_names(&folder)
            };
            for item in reader {
                seen_entries += 1;
                if seen_entries % 32 == 0 && cancel.is_cancelled() {
                    summary.cancelled = true;
                    break 'roots;
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
                    || chooser
                        .as_ref()
                        .is_none_or(|chooser| chooser.accepts(&name, mime::for_name(&name).mime));
                if passes
                    && matcher.pre_accepts(&name, is_folder, file_type.is_symlink())
                    && matcher.name_hit(&name).is_some()
                    && !known.contains(&path)
                    && let Ok(mut entry) = Entry::from_path(&path, hidden)
                {
                    entry.hidden = hidden;
                    // A link is let through the name tests by kind and read
                    // here: its target decides.
                    if matcher.accepts(&entry) {
                        batch.push(entry);
                        summary.hits += 1;
                        if summary.hits >= options.max_hits {
                            summary.truncated = true;
                            flush!();
                            break 'roots;
                        }
                    }
                }
                // A symbolic link is a result but never a way in, so a link
                // back up the tree cannot make the walk circle.
                if is_folder
                    && let Ok(meta) = fs::symlink_metadata(&path)
                    && meta.dev() == device
                    && seen.insert((meta.dev(), meta.ino()))
                {
                    queue.push_back(path);
                }
                if batch.len() >= BATCH_SIZE || (!batch.is_empty() && last_flush.elapsed() >= BATCH_AGE) {
                    if !flush!() {
                        break 'roots;
                    }
                }
            }
            if last_flush.elapsed() >= BATCH_AGE && !flush!() {
                break 'folders;
            }
            if summary.cancelled {
                break 'roots;
            }
        }
    }
    if !summary.cancelled && !summary.truncated {
        flush!();
    }
    summary
}
