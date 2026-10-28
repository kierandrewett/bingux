//! Data usage: totals, a chart over time, limits.

use bingux_ui::prelude::Location;

mod chart;
mod format;
mod limits;
mod model;
mod page;
mod summary;

pub use page::UsagePage;
pub use summary::{connection_usage_row, scope_usage_row, usage_row};

/// What a data usage page is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageScope {
    All,
    Interface(String),
    Connection(String),
}

impl UsageScope {
    pub fn title(&self) -> String {
        match self {
            Self::All => "Data usage".to_string(),
            Self::Interface(name) | Self::Connection(name) => name.clone(),
        }
    }

    /// The location of this scope below the usage page at `base`.
    pub fn location(&self, base: Location) -> Location {
        match self {
            Self::All => base,
            Self::Interface(name) => base.join("interface").join(name),
            Self::Connection(name) => base.join("connection").join(name),
        }
    }
}
