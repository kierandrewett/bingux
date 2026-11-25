//! What the engine tells the host. One enum, so the browser has one place that
//! turns the engine's news into state.

use crate::web_view::ids::{RequestId, TabId};
use crate::web_view::page::{Audio, GoneReason, HistoryEntry, LoadError, Security};
use crate::web_view::requests::{DownloadInfo, Request};

/// News about one tab.
#[derive(Clone, Debug, PartialEq)]
pub enum TabEvent {
    /// The session exists and accepts commands.
    Ready,
    /// The session is gone. `error` is why, when it was not asked for.
    Closed { error: Option<String> },
    /// The top document's address changed.
    Location {
        url: String,
        can_back: bool,
        can_forward: bool,
    },
    Title(String),
    /// A load began (`started`), made progress, or ended.
    Load {
        started: bool,
        loading: bool,
        progress: f32,
        error: Option<LoadError>,
    },
    Security(Security),
    /// The address of the page's icon.
    Favicon(String),
    /// The session history, oldest first, and where the tab is in it.
    History {
        index: usize,
        entries: Vec<HistoryEntry>,
    },
    Fullscreen(bool),
    /// The content process of the tab went away. A new load recovers it.
    ProcessGone(GoneReason),
    /// `window.open` or `target=_blank`. `new` is the session the engine made
    /// for it; accept it with `BrowserBackend::accept_tab`, or close it.
    NewTab {
        new: TabId,
        url: String,
        name: String,
        /// `noopener`: the new page has no `window.opener`.
        no_opener: bool,
        /// What the page asked for besides a tab.
        window: NewWindow,
    },
    /// The page called `window.close()`.
    CloseRequested,
    /// The page zoom changed because of the page's own input (Ctrl+wheel).
    Zoom(f64),
    /// The pointer is over a link, or left it.
    HoverLink(Option<String>),
    Audio(Audio),
    /// A search of the page found `total` matches; `current` (1-based, 0 for
    /// none) is the one on show.
    FindResult { current: u32, total: u32 },
}

/// How a new window asks to be shown and what the engine knows about who
/// opened it. Every field is what the engine said; one it does not report
/// yet (the origin, the disposition and the flags of CR-8) reads as its
/// neutral value, so a rule that depends on it must treat the neutral value
/// as "not known", never as "the person did not act".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewWindow {
    pub disposition: Disposition,
    /// The opener's origin (`https://example.org`), empty when not reported.
    pub origin: String,
    /// The person acted (`VV_NEW_WINDOW_USER_GESTURE`).
    pub user_gesture: bool,
    /// Opened by a script as the page loaded, with no action of the person:
    /// the pop-up blocker's case (`VV_NEW_WINDOW_FROM_SCRIPT_ON_LOAD`).
    pub unrequested: bool,
    /// `noreferrer`.
    pub no_referrer: bool,
    /// The size the page asked for in `window.open` features, logical pixels.
    pub size: Option<(u32, u32)>,
}

/// Where a new window wants to go (`VV_DISPOSITION_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Disposition {
    /// A tab, in front.
    #[default]
    Tab,
    /// A tab behind the one on show.
    BackgroundTab,
    /// A window of its own. A browser window has no such thing: it is a tab.
    Window,
    /// A small window, as `window.open` with features asks for.
    Popup,
    /// The opener's own place.
    Current,
}

/// Why the engine is gone (`vv_engine_failed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCause {
    /// The engine process died.
    Crashed,
    /// The engine broke the protocol and the library killed it: its output
    /// cannot be trusted, and the message says what it did.
    ProtocolViolation,
    /// Another engine has the profile open.
    ProfileLocked,
    /// The engine could not enforce the sandbox the host requires.
    SandboxFloor,
    /// The profile folder is not safe to use (`VV_ERR_PROFILE_UNSAFE`): the
    /// engine did not start, and the message names the folder and the rule.
    ProfileUnsafe,
    /// The engine did not start for another reason (a library that is not
    /// there, a configuration it refused).
    StartFailed,
}

/// News about the engine as a whole.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    /// Gecko is up: sessions run.
    Ready,
    /// The engine is gone and every session with it. `code` is its exit code,
    /// or 128 plus the signal (0 when it never started). `restartable` says a
    /// new engine on the same profile is safe: `BrowserBackend::restart`.
    Failed {
        message: String,
        code: u32,
        cause: FailureCause,
        restartable: bool,
    },
    /// The engine's account of its security changed. Read it again with
    /// `BrowserBackend::info`.
    SecurityChanged,
}

/// One thing that happened, in the order the engine produced it.
#[derive(Clone, Debug, PartialEq)]
pub enum BrowserEvent {
    Engine(EngineEvent),
    Tab { tab: TabId, event: TabEvent },
    /// The engine asks a question and waits for the answer.
    Request { tab: TabId, request: Request },
    /// The engine ended a question itself (the deadline passed, the page
    /// went on, the tab closed): there is nothing left to answer, and what it
    /// did instead is the safe default.
    RequestClosed { tab: TabId, request: RequestId },
    /// A download started or changed.
    Download(DownloadInfo),
}

impl BrowserEvent {
    /// A tab's news, shorter to write.
    pub fn tab(tab: TabId, event: TabEvent) -> Self {
        Self::Tab { tab, event }
    }
}
