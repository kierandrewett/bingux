//! The browser's state and what changes it: the tabs, the downloads, the
//! permissions, and the engine behind them.
//!
//! `Browser` is plain data with methods. The UI reads it and calls its
//! commands; the engine's events go through [`Browser::apply`]. It calls the
//! [`BrowserBackend`] itself (the UI never does), so every rule about what a
//! command asks of the engine is here and testable over the test double.

mod commands;
mod events;
mod lifecycle;
mod prompts;

use std::sync::Arc;

use bingux_ui::web_view::requests::ContextTarget;
use bingux_ui::web_view::{BackendError, BrowserBackend, Capabilities, FailureCause, PageEnvironment, RequestId, TabConfig, TabId};

use super::address::ENGINES;
use super::downloads::Downloads;
use super::history::History;
use super::permissions::Permissions;
use super::shortcuts::Command;
use super::tab::{Tab, TabKind};
use super::tabs::TabList;

/// Where the engine is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    Starting,
    Ready,
    /// The engine is gone, and every session with it. The tabs stay (without a
    /// session) until a restart opens them again, or the window is closed.
    Failed {
        message: String,
        /// The engine's exit code, or 128 plus the signal; 0 when it never started.
        code: u32,
        cause: FailureCause,
        /// A new engine on the same profile may be started (`Browser::restart_engine`).
        restartable: bool,
    },
}

/// How a window is set up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Every tab is private: cookies and storage live in memory, and nothing
    /// is added to the history.
    pub private: bool,
}

/// A message for the person that is not part of a page: a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub error: bool,
    pub text: String,
    /// A button on the toast: its label and what it runs.
    pub action: Option<(String, Command)>,
}

/// Something only the window can do. The UI takes these after a command and
/// does them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    /// Put the cursor in the address bar, with its text selected.
    FocusAddress,
    /// Open the find bar and focus its field.
    OpenFind,
    OpenDownloads,
    OpenPageInfo,
    OpenMainMenu,
    /// Make the window fullscreen, or take it out of it.
    Fullscreen(bool),
    /// Start another browser window whose tabs are all private.
    NewPrivateWindow,
    /// Close the window: its last tab is gone.
    CloseWindow,
}

/// A tab that was closed, for Reopen closed tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub kind: TabKind,
}

/// The page's context menu, waiting for the window to open it.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingMenu {
    pub tab: u64,
    /// The engine's request, or `None` for a menu the browser opens itself
    /// because the engine cannot say what is under the pointer.
    pub request: Option<RequestId>,
    pub target: ContextTarget,
}

/// The size the page area has, in logical pixels, and the display's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSize {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

pub struct Browser {
    backend: Arc<dyn BrowserBackend>,
    capabilities: Capabilities,
    pub options: Options,
    /// The appearance pages see: the colour scheme, accent, motion and text
    /// scale of the window, set by the UI whenever they change.
    pub environment: PageEnvironment,
    pub tabs: TabList,
    pub closed: Vec<ClosedTab>,
    pub downloads: Downloads,
    pub history: History,
    pub permissions: Permissions,
    pub engine: EngineStatus,
    /// Which of `address::ENGINES` the address bar searches with.
    pub search_engine: usize,
    pub view: ViewSize,
    pub notices: Vec<Notice>,
    pub ui: Vec<UiRequest>,
    pub menu: Option<PendingMenu>,
    pub fullscreen: bool,
    /// The tabs of an engine that failed, with the session each had: a restart
    /// opens them again from what the engine had saved of them.
    lost: Vec<(u64, TabId)>,
    /// What the window was to start on when the engine was not there to start
    /// it: the first restart opens it.
    start: Option<Vec<String>>,
}

impl Browser {
    /// A browser with no tab yet. `backend` is the engine.
    pub fn new(backend: Arc<dyn BrowserBackend>, options: Options) -> Self {
        let capabilities = backend.capabilities();
        Self {
            backend,
            capabilities,
            options,
            environment: PageEnvironment::default(),
            tabs: TabList::new(),
            closed: Vec::new(),
            downloads: Downloads::new(),
            history: History::new(),
            permissions: Permissions::new(),
            engine: EngineStatus::Starting,
            search_engine: 0,
            view: ViewSize {
                width: 1024,
                height: 640,
                scale: 1.0,
            },
            notices: Vec::new(),
            ui: Vec::new(),
            menu: None,
            fullscreen: false,
            lost: Vec::new(),
            start: None,
        }
    }

    /// The engine, for the parts of the UI that draw its frames and send its
    /// input directly (the frame view).
    pub fn backend(&self) -> &Arc<dyn BrowserBackend> {
        &self.backend
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn search(&self) -> &'static super::address::SearchEngine {
        &ENGINES[self.search_engine.min(ENGINES.len() - 1)]
    }

    /// The engine session of tab `key`.
    pub fn session_of(&self, key: u64) -> Option<TabId> {
        self.tabs.get(key).and_then(|tab| tab.session)
    }

    pub(crate) fn config(&self, url: &str, opener: Option<TabId>, hidden: bool) -> TabConfig {
        TabConfig {
            url: url.to_string(),
            private: self.options.private,
            width: self.view.width,
            height: self.view.height,
            scale: self.view.scale,
            zoom: 1.0,
            dark: self.environment.dark(),
            container: 0,
            opener,
            hidden,
            restore_from: None,
        }
    }

    /// Tell the person something.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.notices.push(Notice {
            error: false,
            text: text.into(),
            action: None,
        });
    }

    /// Tell the person something went wrong, unless the same was said last.
    pub(crate) fn fail(&mut self, error: &BackendError) {
        let text = error.to_string();
        if self.notices.last().is_some_and(|last| last.error && last.text == text) {
            return;
        }
        self.notices.push(Notice {
            error: true,
            text,
            action: None,
        });
    }

    /// Run an engine call; what goes wrong becomes a notice.
    pub(crate) fn engine_call(&mut self, call: impl FnOnce(&dyn BrowserBackend) -> bingux_ui::web_view::Done) {
        if let Err(error) = call(self.backend.as_ref()) {
            // A tab that is already closed is not the person's business.
            if !matches!(error, BackendError::NoSuchTab(_)) {
                self.fail(&error);
            }
        }
    }

    /// The window's appearance changed: every page sees the new one. The same
    /// value again does nothing.
    pub fn set_environment(&mut self, environment: PageEnvironment) {
        if self.environment == environment {
            return;
        }
        self.environment = environment;
        let environment = self.environment.clone();
        self.engine_call(|backend| backend.set_environment(&environment));
    }

    /// The notices not yet shown.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// What only the window can do, not yet done.
    pub fn take_ui(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.ui)
    }

    /// The tab on show.
    pub fn active(&self) -> Option<&Tab> {
        self.tabs.active()
    }
}
