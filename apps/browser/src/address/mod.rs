//! What was typed in the address bar: an address to go to, or words to search
//! for. Pure functions, so every rule has a test.

mod field;
mod search_engine;
mod suggest;
mod typed;
mod url;

pub use field::{AddressBar, FieldState};
pub use search_engine::SearchEngine;
pub use suggest::{OpenTab, Source, Suggestion, Visited};
pub use typed::{Internal, Typed};
pub use url::Address;

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
