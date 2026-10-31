//! Running a search: the names index first, then the live walk, merged.
//!
//! [`run`] is the one entry point. It takes a [`Plan`] and sends
//! [`Event`]s to its caller, on the calling thread (a view runs it on a
//! worker of its own):
//!
//! 1. The query becomes a [`Matcher`]. One that cannot be run (a bad regular
//!    expression) ends the search at once with [`Summary::invalid`].
//! 2. For every folder to look in that the [`NameIndex`] covers, the index
//!    answers first. Its hits are read from the disk (a name that has gone
//!    is dropped; size, date and kind are checked), put in relevance order
//!    and sent as one batch. This is what makes a search under the home
//!    folder show results at once.
//! 3. The walker reads every folder to look in and tests every name,
//!    skipping paths the index already sent. It finds what the index cannot
//!    (the middle of a word, letters in order, a regular expression, hidden
//!    files, files made since the crawl) and is the authority on what is
//!    there.
//!
//! Results are sent in the order found; a consumer that wants relevance
//! order sorts with [`Matcher::rank`] when the search is done (see
//! `design/files-search.md` for why nothing is reordered while it runs).
//! Nothing is sent twice. Cancelling stops the walk within 32 entries and
//! the index phase between two hits.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::index::NameIndex;
use super::matcher::{Invalid, Matcher};
use super::query::{Hidden, Query};
use super::walk::{WalkOptions, Walked, walk};
use crate::entry::Entry;
use crate::filter::Filter;
use crate::listing::Cancel;

/// Everything a search needs besides the query.
#[derive(Clone)]
pub struct Plan {
    pub query: Query,
    /// The folders to look in, with everything below them.
    pub roots: Vec<PathBuf>,
    /// The home folder: where `in:~/x` starts.
    pub home: PathBuf,
    /// The view's "Show hidden files", which `hidden:` may override.
    pub view_hidden: bool,
    /// Look in build and dependency folders ([`NOISE_FOLDERS`](super::NOISE_FOLDERS)).
    pub include_noise: bool,
    /// A chooser's file type filter.
    pub chooser_filter: Option<Filter>,
    /// Stop after this many results.
    pub max_hits: usize,
    /// The names index to ask first, if any.
    pub index: Option<Arc<NameIndex>>,
}

/// What [`run`] tells its caller.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// More results, none seen before.
    Found(Vec<Entry>),
    /// How many folders the walk has read.
    Progress { folders: usize },
}

/// How a search ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub hits: usize,
    /// Of `hits`, how many the index answered before the walk began.
    pub indexed: usize,
    pub folders: usize,
    pub unreadable: usize,
    /// The limit was reached.
    pub truncated: bool,
    pub cancelled: bool,
    /// The query could not be run.
    pub invalid: Option<Invalid>,
    /// `in:` names a folder that is not there.
    pub missing: Option<String>,
}

/// The folder `within` names, looked for from `root`.
pub fn resolve_within(within: &str, root: &Path, home: &Path) -> PathBuf {
    if within == "~" {
        home.to_path_buf()
    } else if let Some(rest) = within.strip_prefix("~/") {
        home.join(rest)
    } else {
        let path = Path::new(within);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    }
}

/// Run `plan`, sending results to `emit` as they are found. `emit` returns
/// false to stop. Blocks: call it from a worker.
pub fn run(plan: &Plan, cancel: &Cancel, emit: &mut dyn FnMut(Event) -> bool) -> Summary {
    let mut summary = Summary::default();
    let matcher = match Matcher::new(&plan.query) {
        Ok(matcher) => matcher,
        Err(invalid) => {
            summary.invalid = Some(invalid);
            return summary;
        }
    };
    // `in:` narrows every root.
    let mut roots: Vec<PathBuf> = Vec::new();
    for root in &plan.roots {
        match &plan.query.filters.within {
            Some(within) => {
                let folder = resolve_within(within, root, &plan.home);
                if folder.is_dir() {
                    if !roots.contains(&folder) {
                        roots.push(folder);
                    }
                } else {
                    summary.missing = Some(within.clone());
                }
            }
            None => roots.push(root.clone()),
        }
    }
    if roots.is_empty() {
        return summary;
    }
    summary.missing = None;

    let mut known: HashSet<PathBuf> = HashSet::new();
    if let Some(index) = &plan.index {
        if roots.iter().any(|root| index.covers(root)) {
            index.refresh();
        }
        for root in roots.iter().filter(|root| index.covers(root)) {
            if cancel.is_cancelled() {
                summary.cancelled = true;
                return summary;
            }
            let mut found: Vec<Entry> = Vec::new();
            for path in index.candidates(&matcher, root, cancel) {
                if cancel.is_cancelled() {
                    summary.cancelled = true;
                    return summary;
                }
                if known.contains(&path) {
                    continue;
                }
                let Ok(entry) = Entry::from_path(&path, false) else {
                    continue;
                };
                let chooser_ok = entry.is_dir()
                    || plan
                        .chooser_filter
                        .as_ref()
                        .is_none_or(|filter| filter.accepts_entry(&entry));
                if chooser_ok
                    && matcher.accepts(&entry)
                    && (matcher.name_hit(&entry.name).is_some()
                        || matcher.typo_accepts(&entry.name))
                {
                    known.insert(path);
                    found.push(entry);
                }
            }
            found.truncate(plan.max_hits.saturating_sub(summary.hits));
            // Best match first; the index's own order breaks ties.
            found.sort_by_cached_key(|entry| matcher.rank(entry, root));
            summary.hits += found.len();
            summary.indexed += found.len();
            if !found.is_empty() && !emit(Event::Found(found)) {
                summary.cancelled = true;
                return summary;
            }
        }
    }
    if summary.hits >= plan.max_hits {
        summary.truncated = true;
        return summary;
    }

    let show_hidden = match plan.query.filters.hidden {
        Hidden::Follow => plan.view_hidden,
        Hidden::Include | Hidden::Only => true,
        Hidden::Exclude => false,
    };
    let options = WalkOptions {
        show_hidden,
        include_noise: plan.include_noise,
        filter: plan.chooser_filter.clone(),
        max_hits: plan.max_hits - summary.hits,
    };
    let walked = walk(
        &roots,
        &matcher,
        &options,
        &known,
        cancel,
        &mut |walked| match walked {
            Walked::Hits(entries) => emit(Event::Found(entries)),
            Walked::Progress { folders } => emit(Event::Progress { folders }),
        },
    );
    summary.hits += walked.hits;
    summary.folders = walked.folders;
    summary.unreadable = walked.unreadable;
    summary.truncated = walked.truncated;
    summary.cancelled = walked.cancelled;
    summary
}
