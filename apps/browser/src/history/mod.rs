//! A profile's history: `browser.sqlite`, and the bingux-ui searchable
//! `history` that the address field and the new tab page ask.
//!
//! Only the store's thread opens the database. The index is created by the
//! store with the search service's `create_index` and filled from the file
//! when the store opens it; after that the store keeps the two in step, so
//! nothing reads the file twice and no second connection exists.
//!
//! History is private data: errors name the file and the SQLite code, never
//! an address.

mod database;
mod page;
mod search;

#[cfg(test)]
mod tests;

pub use database::Database;
pub use page::{Change, Page};
pub use search::History;

/// The name of the searchable: one per process, and a process has one profile.
pub const INDEX: &str = "history";

/// The most pages kept: older ones are trimmed when the store opens.
pub const CAP: usize = 50_000;
