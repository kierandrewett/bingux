//! Data usage: totals, a chart over time, limits.

use bingux_ui::prelude::Address;

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

    /// The address of this scope below the usage page at `base`.
    pub fn address(&self, base: Address) -> Address {
        match self {
            Self::All => base,
            Self::Interface(name) => base.join("interface").join(name),
            Self::Connection(name) => base.join("connection").join(name),
        }
    }

    /// The scope named by the segments below `network/usage`.
    pub fn from_segments(segments: &[&str]) -> Option<Self> {
        Some(match segments {
            [] => Self::All,
            ["interface", name] => Self::Interface(name.to_string()),
            ["connection", name] => Self::Connection(name.to_string()),
            _ => return None,
        })
    }
}
