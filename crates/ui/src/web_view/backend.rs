//! The one interface the browser's UI has to the engine: [`BrowserBackend`].
//!
//! It names exactly the operations the UI uses and nothing of Vivarium: the
//! real implementation (`engine.rs`, over the `vivarium` crate) maps each onto
//! an ABI call, and a test double implements it in memory. Operations the ABI
//! does not have yet default to [`BackendError::Unsupported`], and
//! [`Capabilities`] says which those are, so the UI greys the control out and
//! says why instead of pretending.
//!
//! Every method takes `&self` and returns at once: the engine is on another
//! thread and the UI thread never waits (see "Nothing blocks the UI thread" in
//! `docs/bingux-ui.md`).

use std::path::PathBuf;
use std::sync::Arc;
use std::task::Waker;

use crate::web_view::events::BrowserEvent;
use crate::web_view::ids::{DownloadId, RequestId, TabId};
use crate::web_view::input::PageInput;
use crate::web_view::report::SecurityReport;
use crate::web_view::requests::Response;
use crate::web_view::surface::PageSurface;

/// Why an operation did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    /// The engine build or the ABI does not have it yet. The text names it.
    Unsupported(&'static str),
    /// The tab is closed (or never was).
    NoSuchTab(TabId),
    /// The engine process is gone.
    EngineGone,
    /// The engine refused, with its message.
    Engine(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "{what} is not available in this engine yet"),
            Self::NoSuchTab(tab) => write!(f, "tab {} is closed", tab.0),
            Self::EngineGone => f.write_str("the engine is not running"),
            Self::Engine(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BackendError {}

/// The result of an operation.
pub type Done = Result<(), BackendError>;

/// What the engine can do today. The browser draws a control for a feature
/// the engine lacks as disabled, with the reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Search in the page, with a count of matches.
    pub find: bool,
    /// Permission prompts (camera, location, notifications ...).
    pub permissions: bool,
    /// `alert`, `confirm`, `prompt`, `beforeunload` and HTTP authentication.
    pub dialogs: bool,
    /// The details of a certificate error, and the choice to proceed.
    pub cert_errors: bool,
    /// The page's context menu: what is under the pointer.
    pub context_menus: bool,
    /// Downloads, with progress.
    pub downloads: bool,
    /// The address of the link under the pointer.
    pub hover_links: bool,
    /// Whether a page is making sound, and muting it.
    pub audio: bool,
    /// Fingers as touch events (else the browser turns them into a click, a
    /// scroll and a held press).
    pub touch: bool,
    /// Input method composition with a preedit string.
    pub composition: bool,
    /// `<select>` lists and `<input type=file>`.
    pub form_pickers: bool,
    /// Mute, pin and the like need no engine; this is only for the icons of
    /// pages: the engine sends the address of each page's icon.
    pub favicons: bool,
}

impl Capabilities {
    /// Everything works. For a test double.
    pub const fn all() -> Self {
        Self {
            find: true,
            permissions: true,
            dialogs: true,
            cert_errors: true,
            context_menus: true,
            downloads: true,
            hover_links: true,
            audio: true,
            touch: true,
            composition: true,
            form_pickers: true,
            favicons: true,
        }
    }
}

/// How the host's Remote Settings choice reads (`VV_REMOTE_SETTINGS_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemoteSettings {
    /// No traffic, no CRLite: revocation is OCSP soft-fail only.
    Off,
    /// The security collections, from the host's mirror.
    #[default]
    SecurityOnly,
    /// The same collections from Mozilla.
    MozillaDirect,
}

/// What the host told the engine at start. The engine reads nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineSettings {
    pub profile_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub download_dir: PathBuf,
    /// Where minidumps of a crashed process go. They are never uploaded.
    pub crash_dir: PathBuf,
    pub remote_settings: RemoteSettings,
    pub remote_settings_server: Option<String>,
    pub content_process_limit: u32,
    pub locales: String,
    /// What a page's WebRTC connection may reveal about the machine, in words:
    /// the host's choice when it made one, else the engine's own fixed policy.
    pub webrtc_ip_policy: String,
    /// The sandbox the engine must reach, or it does not start.
    pub sandbox_floor: String,
}

/// Where the engine is in its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineState {
    Starting,
    Ready,
    Failed,
}

/// Who the engine is, how the host configured it, and what it says about
/// itself. `security` is `None` until the engine has produced its report
/// (`NOT_READY`); the settings page says so rather than show an empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineInfo {
    pub state: EngineState,
    /// "Gecko 159.0", empty until the engine is ready.
    pub version: String,
    pub build_id: String,
    /// The user agent string pages see, as the engine reports it. Empty until
    /// the engine is ready, or while it does not report one.
    pub user_agent: String,
    pub settings: EngineSettings,
    pub security: Option<SecurityReport>,
}

/// What a new tab starts with.
#[derive(Clone, Debug, PartialEq)]
pub struct TabConfig {
    /// The first address, or empty for a blank tab.
    pub url: String,
    /// Cookies and storage live in memory and go with the tab.
    pub private: bool,
    /// Logical pixels.
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub zoom: f64,
    /// The colour scheme pages see (`prefers-color-scheme`).
    pub dark: bool,
    /// Tabs with different containers share no cookies or storage.
    pub container: u32,
    /// The tab this one belongs to.
    pub opener: Option<TabId>,
    /// No frames until it is shown.
    pub hidden: bool,
    /// The tab of the engine that failed whose page this one takes up again:
    /// its history, scroll position and form data, from the state the engine
    /// last reported. Only `BrowserBackend::restart` makes such a tab
    /// possible; an id the backend does not know is ignored. The Vivarium
    /// engine (ABI 2.0) reports no such state, so there the tab opens at
    /// `url`.
    pub restore_from: Option<TabId>,
}

impl Default for TabConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            private: false,
            width: 1024,
            height: 640,
            scale: 1.0,
            zoom: 1.0,
            dark: false,
            container: 0,
            opener: None,
            hidden: false,
            restore_from: None,
        }
    }
}

/// A search of the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindQuery {
    pub text: String,
    /// Next match, or the previous one.
    pub forward: bool,
    pub match_case: bool,
    pub whole_words: bool,
    pub highlight_all: bool,
    /// The text changed: start again from the top of the page. Without it a
    /// repeated query moves to the next match.
    pub restart: bool,
}

impl FindQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            forward: true,
            match_case: false,
            whole_words: false,
            highlight_all: true,
            restart: true,
        }
    }
}

/// The engine, as the browser's UI sees it.
pub trait BrowserBackend: Send + Sync + 'static {
    /// What works. Constant for the life of the engine.
    fn capabilities(&self) -> Capabilities;

    /// Who the engine is and how it was configured.
    fn info(&self) -> EngineInfo;

    /// Starts a new engine on the same profile, after an
    /// `EngineEvent::Failed` that says it is `restartable`, and returns once
    /// it is started (`EngineEvent::Ready` says when pages run). Every tab of
    /// the old engine is gone; the caller makes them again with
    /// `TabConfig::restore_from`.
    fn restart(&self) -> Done {
        Err(BackendError::Unsupported("restarting the engine"))
    }

    // ---- Events: pull, never push, and no timer ----------------------------

    /// The waker to call when there are events. Cheap, from any thread, and
    /// coalescing, as [`FrameSource::set_waker`] is.
    ///
    /// [`FrameSource::set_waker`]: crate::frame_view::FrameSource::set_waker
    fn set_waker(&self, waker: Waker);

    /// Appends every event since the last call, in order. Never blocks.
    fn poll_events(&self, out: &mut Vec<BrowserEvent>);

    // ---- Tabs ----------------------------------------------------------------

    /// A new tab. The id is valid at once; `TabEvent::Ready` follows.
    fn create_tab(&self, config: &TabConfig) -> Result<TabId, BackendError>;

    /// Takes the session the engine made for a `TabEvent::NewTab` request.
    /// The page keeps its `window.opener` unless the request said not to.
    fn accept_tab(&self, new: TabId, config: &TabConfig) -> Done;

    /// Closes a tab, or rejects a `TabEvent::NewTab` that was not accepted.
    fn close_tab(&self, tab: TabId) -> Done;

    /// The tab's pixels, pointer shape and caret. `None` for a tab that is
    /// closed.
    fn surface(&self, tab: TabId) -> Option<Arc<PageSurface>>;

    // ---- Navigation ----------------------------------------------------------

    /// Loads `url`, fixed up as an address bar does ("example.com" works).
    fn load(&self, tab: TabId, url: &str) -> Done;
    fn reload(&self, tab: TabId, bypass_cache: bool) -> Done;
    fn stop(&self, tab: TabId) -> Done;
    fn back(&self, tab: TabId) -> Done;
    fn forward(&self, tab: TabId) -> Done;
    /// An entry of the last `TabEvent::History`.
    fn go_to_history(&self, tab: TabId, index: usize) -> Done;

    // ---- The surface ----------------------------------------------------------

    /// The size in logical pixels and the scale of the view. A new size starts
    /// a new buffer set; the next frame is whole.
    fn set_size(&self, tab: TabId, width: u32, height: u32, scale: f32) -> Done;
    /// A hidden tab makes no frames and stops animating.
    fn set_visible(&self, tab: TabId, visible: bool, focused: bool) -> Done;
    fn set_zoom(&self, tab: TabId, zoom: f64) -> Done;
    /// The appearance pages see, for every tab, now and later (see
    /// [`PageEnvironment`]). An engine that cannot set all of it sets what it
    /// can: the colour scheme of the tabs there are.
    fn set_environment(&self, environment: &crate::web_view::environment::PageEnvironment) -> Done;

    // ---- Input -----------------------------------------------------------------

    /// Input for the page, in call order.
    fn send(&self, tab: TabId, input: PageInput) -> Done;

    // ---- Page features ---------------------------------------------------------

    fn find(&self, _tab: TabId, _query: &FindQuery) -> Done {
        Err(BackendError::Unsupported("find in page"))
    }

    fn find_stop(&self, _tab: TabId) -> Done {
        Err(BackendError::Unsupported("find in page"))
    }

    fn set_muted(&self, _tab: TabId, _muted: bool) -> Done {
        Err(BackendError::Unsupported("muting a tab"))
    }

    /// The window is fullscreen (the page asked, and the person agreed) or
    /// left it (`VV_EVENT_FULLSCREEN`, then `vv_session_fullscreen_result`).
    fn set_fullscreen(&self, _tab: TabId, _on: bool) -> Done {
        Err(BackendError::Unsupported("fullscreen answers"))
    }

    /// The answer to a `BrowserEvent::Request`.
    fn respond(&self, _tab: TabId, _request: RequestId, _response: Response) -> Done {
        Err(BackendError::Unsupported("answering the engine's requests"))
    }

    // ---- Downloads -------------------------------------------------------------

    fn cancel_download(&self, _download: DownloadId) -> Done {
        Err(BackendError::Unsupported("downloads"))
    }

    fn pause_download(&self, _download: DownloadId) -> Done {
        Err(BackendError::Unsupported("downloads"))
    }

    fn resume_download(&self, _download: DownloadId) -> Done {
        Err(BackendError::Unsupported("downloads"))
    }
}
