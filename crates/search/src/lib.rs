//! Bingux search: documents, schema, an inverted index with prefix and typo
//! tolerant search, OpenSearch-shaped requests and responses, a process-wide
//! service, on-disk snapshots and the app search socket.
//!
//! No UI and no async runtime. `bingux-ui` re-exports it as
//! `bingux_ui::search`; Spotlight (`bingux-searchd`) links it directly.
//! Design and rationale: `design/search.md`.

pub mod aggregate;
mod bitmap;
pub mod document;
pub mod error;
pub mod filter;
mod highlight;
mod index;
pub mod ipc;
pub mod request;
pub mod response;
pub mod schema;
mod search;
pub mod service;
pub mod snapshot;
mod text;
pub mod tokenizer;
mod typo;

pub use document::{Document, Value};
pub use error::{Error, Result};
pub use filter::{Filter, Lit};
pub use index::{Index, Limits};
pub use request::{
    AggSpec, BoolQuery, HighlightSpec, PrefixMode, Query, Search, SortKey, SortSpec, SourceSpec,
    Strategy, TextQuery,
};
pub use response::{Aggregation, Bucket, Hit, MatchRange, Relation, SearchResponse};
pub use schema::{RankingRule, Schema, SearchableField, TypoSettings};
pub use service::{Change, SearchService, Searchable};
pub use text::Cancel;
