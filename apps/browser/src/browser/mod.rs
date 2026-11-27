//! The browser's state and what changes it: the tabs, the downloads, the
//! permissions, and the engine behind them.
//!
//! `Browser` is plain data with methods. The UI reads it and calls its
//! commands; the engine's events go through [`Browser::apply`]. It calls the
//! backend itself, through [`Engine`] (the UI never does), so every rule about what a
//! command asks of the engine is here and testable over the test double.

mod commands;
mod kept;
mod lifecycle;
mod outbox;
mod prompts;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

pub use outbox::{DuplicatesAsk, Notice, Outbox, UiRequest};

use bingux_ui::web_view::requests::ContextTarget;
use bingux_ui::web_view::{PageEnvironment, RequestId, TabConfig, TabId};

use crate::address::SearchEngine;
use crate::downloads::Downloads;
use crate::engine::Engine;
use crate::permissions::Permissions;
use crate::settings::Settings;
use crate::store::Store;
use crate::store::{Keeping, NotSaved};
use crate::tabs::TabList;
use crate::tabs::{Tab, TabKind};

/// How a window is set up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Every tab is private: cookies and storage live in memory, and nothing
    /// is added to the history.
    pub private: bool,
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

/// Which profile a window belongs to.
///
/// Why an enum: a window used to carry the registry id and the name as two
/// `Option`s, and a private window had the id and no name. The pair was then
/// read as "no profile", so the window could not say which profile it was
/// started from; a new private window found it again only through the
/// environment of the process.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Belonging {
    /// No registered profile: a test, a snapshot, or a folder named by the
    /// environment.
    #[default]
    Unregistered,
    /// A window of the profile with registry id `id`, called `name`.
    Registered { id: String, name: String },
    /// A private window. It has no profile of its own and keeps nothing; `from`
    /// is the profile whose settings it started with, and so the one a new
    /// private window from this one starts from.
    PrivateFrom(Option<String>),
}

/// The profile a window opens in: who it is, what it starts with, and the
/// writer that keeps what changes.
#[derive(Debug)]
pub struct Profile {
    /// Which profile this window belongs to.
    pub belonging: Belonging,
    pub settings: Settings,
    /// Whether the settings are kept: how reading them at open went.
    pub settings_keeping: Keeping,
    /// The profile's writer; `None` for a private window and a window
    /// without a profile, which keep nothing.
    pub store: Option<Store>,
    /// Every profile of the registry as (id, name), oldest first: what the
    /// main menu's Profile row lists. Read when the browser starts, off the
    /// UI thread.
    pub known: Vec<(String, String)>,
}

impl Default for Profile {
    /// No profile: a test or a snapshot. Nothing is kept.
    fn default() -> Self {
        Self {
            belonging: Belonging::Unregistered,
            settings: Settings::default(),
            settings_keeping: Keeping::NotSaved(NotSaved::NoProfile),
            store: None,
            known: Vec::new(),
        }
    }
}

impl Profile {
    /// A private window of profile `id`: its settings, read without writing,
    /// and no store, so nothing it does is written.
    pub fn private(id: Option<String>, settings: Settings) -> Self {
        Self {
            belonging: Belonging::PrivateFrom(id),
            settings,
            settings_keeping: Keeping::NotSaved(NotSaved::Private),
            store: None,
            known: Vec::new(),
        }
    }
}

pub struct Browser {
    /// The engine: its handle, what it can do, where it is.
    pub engine: Engine,
    pub options: Options,
    /// The appearance pages see: the colour scheme, accent, motion and text
    /// scale of the window, set by the UI whenever they change.
    pub environment: PageEnvironment,
    pub tabs: TabList,
    pub closed: Vec<ClosedTab>,
    pub downloads: Downloads,
    /// Whether the history is kept.
    pub history_keeping: Keeping,
    pub permissions: Permissions,
    /// The settings of the window's profile.
    pub settings: Settings,
    /// Whether a change of the settings is kept.
    pub settings_keeping: Keeping,
    /// The writer of the profile, when this window keeps anything.
    pub(crate) store: Option<Store>,
    /// The profile the window belongs to: its registry id and name, `None`
    /// for a window opened without one and for a private window.
    pub profile: Option<(String, String)>,
    /// For a private window: the profile whose settings it started with.
    /// `None` outside a private window. Set with `profile` from one
    /// [`Belonging`], so the two are never both set.
    pub(crate) private_from: Option<String>,
    /// Every profile as (id, name), for the main menu's Profile row.
    pub profiles: Vec<(String, String)>,
    pub view: ViewSize,
    /// What the browser has said and the window has not yet shown or done.
    pub(crate) out: Outbox,
    pub menu: Option<PendingMenu>,
    pub fullscreen: bool,
    /// The share of the content area a page keeps when a toolbox is docked
    /// under it. The window remembers the last one the splitter was dragged to.
    pub devtools_page_share: f32,
    /// The tabs of an engine that failed, with the session each had: a restart
    /// opens them again from what the engine had saved of them.
    pub(crate) lost: Vec<(u64, TabId)>,
    /// What the window was to start on when the engine was not there to start
    /// it: the first restart opens it.
    pub(crate) start: Option<Vec<String>>,
}

impl Browser {
    /// A browser with no tab yet and no profile.
    pub fn new(engine: impl Into<Engine>, options: Options) -> Self {
        Self::with_profile(engine, options, Profile::default())
    }

    /// A browser with no tab yet, in `profile`.
    pub fn with_profile(engine: impl Into<Engine>, options: Options, profile: Profile) -> Self {
        let mut browser = Self {
            engine: engine.into(),
            options,
            environment: PageEnvironment::default(),
            tabs: TabList::new(),
            closed: Vec::new(),
            downloads: Downloads::new(),
            history_keeping: if profile.store.is_some() {
                Keeping::Loading
            } else {
                profile.settings_keeping.clone()
            },
            permissions: Permissions::new(),
            settings: profile.settings,
            settings_keeping: Keeping::Loading,
            store: profile.store,
            profile: match &profile.belonging {
                Belonging::Registered { id, name } => Some((id.clone(), name.clone())),
                Belonging::Unregistered | Belonging::PrivateFrom(_) => None,
            },
            private_from: match &profile.belonging {
                Belonging::PrivateFrom(from) => from.clone(),
                Belonging::Unregistered | Belonging::Registered { .. } => None,
            },
            profiles: profile.known,
            view: ViewSize {
                width: 1024,
                height: 640,
                scale: 1.0,
            },
            out: Outbox::default(),
            menu: None,
            fullscreen: false,
            devtools_page_share: crate::devtools::DEFAULT_PAGE_SHARE,
            lost: Vec::new(),
            start: None,
        };
        browser.kept_settings(profile.settings_keeping);
        browser
    }

    pub fn search(&self) -> &'static SearchEngine {
        &SearchEngine::ALL[self.settings.engine_index()]
    }

    /// The address bar searches with `SearchEngine::ALL[at]` from now on, and the
    /// profile keeps the choice.
    pub fn set_search_engine(&mut self, at: usize) {
        let Some(engine) = SearchEngine::ALL.get(at) else {
            return;
        };
        self.settings.search_engine = engine.id.to_string();
        if let Some(store) = &self.store {
            store.save_settings(&self.settings);
        }
    }

    /// The registry id of the profile a new private window starts from: this
    /// window's own, or for a private window the one it started from.
    pub fn private_window_profile(&self) -> Option<String> {
        match &self.profile {
            Some((id, _)) => Some(id.clone()),
            None => self.private_from.clone(),
        }
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

    /// The window's appearance changed: every page sees the new one. The same
    /// value again does nothing.
    pub fn set_environment(&mut self, environment: PageEnvironment) {
        if self.environment == environment {
            return;
        }
        self.environment = environment;
        let environment = self.environment.clone();
        self.engine.call(&mut self.out, |backend| {
            backend.set_environment(&environment)
        });
    }

    /// The notices not yet shown.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.out.take_notices()
    }

    /// What only the window can do, not yet done.
    pub fn take_ui(&mut self) -> Vec<UiRequest> {
        self.out.take_requests()
    }

    /// The tab on show.
    pub fn active(&self) -> Option<&Tab> {
        self.tabs.active()
    }
}
