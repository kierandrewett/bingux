//! What the engine says about a page: how it is loading, how it was
//! delivered, where it can go back to.

/// How the page was delivered (`VV_SECURITY_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SecurityState {
    /// Plain HTTP, or no page yet.
    #[default]
    Insecure,
    /// HTTPS with a certificate that checks out.
    Secure,
    /// HTTPS that is not trustworthy: an exception was made, or the page
    /// holds active mixed content.
    Broken,
    /// A file or another local page.
    Local,
    /// A page the browser draws itself (`bingux:` pages).
    Internal,
}

/// The connection of the page on show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Security {
    pub state: SecurityState,
    /// Scripts or frames loaded over plain HTTP (`VV_SECURITY_MIXED_ACTIVE`).
    pub mixed_active: bool,
    /// Images or media loaded over plain HTTP (`VV_SECURITY_MIXED_DISPLAY`).
    pub mixed_passive: bool,
    /// An extended validation certificate (`VV_SECURITY_EV`).
    pub extended_validation: bool,
}

/// Why a load stopped without a page: the engine's `nsresult` and its name
/// (`NS_ERROR_UNKNOWN_HOST`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    pub code: u32,
    pub name: String,
}

/// Where a load is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadState {
    /// A request is under way.
    pub loading: bool,
    /// How much has arrived, 0 to 1. Only meaningful while `loading`.
    pub progress: f32,
    /// The last load failed with this. Cleared by the next load.
    pub error: Option<LoadError>,
}

/// One entry of a tab's session history, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
}

/// Why the content process of a tab went away (`VV_PROCESS_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoneReason {
    Crashed,
    Killed,
    CleanExit,
}

/// Whether the page makes sound, and whether the host muted it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Audio {
    pub playing: bool,
    pub muted: bool,
}
