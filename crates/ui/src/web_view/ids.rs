//! The names the engine gives things. All of them are numbers that are never
//! reused for the life of an engine, so a stale one is an error, not another
//! tab.

/// One browsing surface: a Vivarium session. The browser's tab strip keys its
/// tabs by [`TabId::get`], so a tab keeps its identity whatever its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u32);

impl TabId {
    /// The id the tab strip knows the tab by.
    pub const fn get(self) -> u64 {
        self.0 as u64
    }

    /// The tab a strip id names.
    pub const fn from_strip(id: u64) -> Self {
        Self(id as u32)
    }
}

/// One question the engine asked the host (a permission, a dialog, a
/// certificate error). It is answered once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u32);

/// One download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DownloadId(pub u64);
