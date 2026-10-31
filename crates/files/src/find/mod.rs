//! Finding files by name: the query a person types, how names are matched
//! and ranked, and the search that reads folders.
//!
//! Search is not the file type [`Filter`](crate::filter::Filter). A filter is
//! part of a chooser's request (`*.png`, `image/*`) and says which kinds of
//! file it may return; a search is a [`Query`] a person types, and it only
//! narrows what they see. Both apply: results respect the filter.
//!
//! | Module | What it holds |
//! | ------ | ------------- |
//! | [`query`] | The typed [`Query`]: words, filters, the text they are written as |
//! | [`matcher`] | A query made ready to test names and entries, and the relevance order |
//! | [`scope`] | Where to look: the folder and below, home, everywhere, a place |
//! | [`walk`] | The live walk of folders, streaming, cancellable |
//! | [`index`] | The names index in the shared search engine, for the home folder |
//! | [`engine`] | [`run`]: index first, then the walk, merged, nothing sent twice |
//! | [`history`] | Recent searches |
//!
//! `design/files-search.md` says how the pieces fit and what is not done.

mod civil;
pub mod engine;
pub mod history;
pub mod index;
pub mod matcher;
pub mod query;
pub mod scope;
pub mod walk;

#[cfg(test)]
mod tests;

pub use civil::{Clock, Date, Moment, day_text, parse_day, shift_days};
pub use engine::{Event, Plan, Summary, resolve_within, run};
pub use history::Recents;
pub use index::NameIndex;
pub use matcher::{Hit, Invalid, Matcher, Rank, Tier, depth_below};
pub use query::{
    FilterKey, Filters, Hidden, Kind, Modified, NameMode, Problem, Query, SizeCond, SortBy, Term,
    format_bytes, parse_bytes, split_pending,
};
pub use scope::Scope;
pub use walk::{NOISE_FOLDERS, WalkOptions, Walked};

use std::path::Path;

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
