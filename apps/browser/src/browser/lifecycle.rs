//! The engine's life as the window sees it: it fails, and a restart opens the
//! tabs again; a page asks for a window, and the browser says yes or no.

use bingux_ui::web_view::{Disposition, FailureCause, NewWindow, RequestId, TabId};

use super::{Browser, EngineStatus, Notice};
use crate::model::tab::{Tab, TabKind};
use crate::model::tabs::Place;

impl Browser {
    /// The engine is gone: every session with it. The tabs stay as they were,
    /// without a session, so a restart can open them again; what each was
    /// waiting for will never be answered by anyone.
    pub(super) fn engine_failed(
        &mut self,
        message: String,
        code: u32,
        cause: FailureCause,
        restartable: bool,
    ) {
        for tab in self.tabs.all_mut() {
            let Some(session) = tab.session.take() else {
                continue;
            };
            self.lost.push((tab.key, session));
            tab.ready = false;
            tab.load.loading = false;
            tab.audio.playing = false;
            tab.hover = None;
            tab.prompts.clear();
            tab.cert = None;
        }
        self.menu = None;
        self.engine = EngineStatus::Failed {
            message,
            code,
            cause,
            restartable,
        };
    }

    /// The engine is up (again): it takes the window's look, and what the
    /// controls may offer is what it says it can do.
    pub(super) fn engine_ready(&mut self) {
        self.engine = EngineStatus::Ready;
        self.capabilities = self.backend().capabilities();
        let environment = self.environment.clone();
        self.engine_call(|backend| backend.set_environment(&environment));
    }

    /// Open what the window starts on: the addresses it was launched with, or
    /// one blank tab. While the engine is not there the window opens on its
    /// page, with no tab; the first restart opens them.
    pub fn open_first(&mut self, first: &[String]) {
        if matches!(self.engine, EngineStatus::Failed { .. }) {
            self.start = Some(first.to_vec());
            return;
        }
        if first.is_empty() {
            self.open_tab("", Place::End, true);
        }
        for url in first {
            self.open_tab(url, Place::End, false);
        }
        if let Some(key) = self.tabs.all().first().map(|tab| tab.key) {
            self.select(key);
        }
    }

    /// Start a new engine on the same profile and open every tab again where
    /// it was. Only after a failure that says it is safe. When the engine
    /// still does not start the window keeps saying why (the backend says it
    /// again, with the new words).
    pub fn restart_engine(&mut self) {
        let EngineStatus::Failed {
            restartable: true, ..
        } = self.engine
        else {
            return;
        };
        if let Err(error) = self.backend().restart() {
            self.fail(&error);
            return;
        }
        self.engine = EngineStatus::Starting;
        self.capabilities = self.backend().capabilities();
        let active = self.tabs.active_key();
        for (key, old) in std::mem::take(&mut self.lost) {
            let Some(tab) = self.tabs.get(key).filter(|tab| tab.kind == TabKind::Web) else {
                continue;
            };
            let mut config = self.config(&tab.url.clone(), None, key != active);
            config.restore_from = Some(old);
            match self.backend().create_tab(&config) {
                Ok(session) => {
                    if let Some(tab) = self.tabs.get_mut(key) {
                        tab.session = Some(session);
                        tab.gone = None;
                        tab.load.loading = !tab.url.is_empty();
                        tab.load.progress = 0.0;
                        tab.load.error = None;
                    }
                }
                Err(error) => self.fail(&error),
            }
        }
        self.shown_again(active);
        if let Some(first) = self.start.take() {
            self.open_first(&first);
        }
    }

    /// The engine made a session for `window.open`: it becomes a tab next to
    /// the page that opened it, in front of it or behind it as the page asked,
    /// unless the page opened it as it loaded, with no action of the person:
    /// that is a pop-up, and the browser blocks it.
    ///
    /// Only what the engine says is a reason. A request that says nothing
    /// about a gesture (the engine of today does not yet) is not blocked for
    /// that: not knowing is not a verdict.
    pub(super) fn accept_new_tab(
        &mut self,
        opener: TabId,
        new: TabId,
        url: &str,
        window: &NewWindow,
    ) {
        if window.unrequested && !window.user_gesture {
            if let Err(error) = self.backend().close_tab(new) {
                self.fail(&error);
            }
            let from = if window.origin.is_empty() {
                "a page".to_string()
            } else {
                window.origin.clone()
            };
            self.notices.push(Notice {
                error: false,
                text: format!("Blocked a pop-up from {from}"),
                action: None,
            });
            return;
        }
        let opener_key = self.key_of(opener);
        let background = window.disposition == Disposition::BackgroundTab;
        let config = self.config("", Some(opener), background);
        if let Err(error) = self.backend().accept_tab(new, &config) {
            self.fail(&error);
            return;
        }
        let key = self.tabs.next_key();
        let mut tab = Tab::blank(key, TabKind::Web);
        tab.session = Some(new);
        tab.url = url.to_string();
        tab.private = self.options.private;
        tab.opener = opener_key;
        tab.load.loading = !url.is_empty();
        let place = opener_key.map_or(Place::End, Place::After);
        let was = self.tabs.active_key();
        self.tabs.insert(tab, place, !background);
        if was != self.tabs.active_key()
            && let Some(session) = self.session_of(was)
        {
            self.engine_call(|backend| backend.set_visible(session, false, false));
        }
    }

    /// The engine ended a question itself: what the window showed for it
    /// goes, since nothing is left to answer.
    pub(super) fn request_closed(&mut self, session: TabId, id: RequestId) {
        if let Some(key) = self.key_of(session)
            && let Some(tab) = self.tabs.get_mut(key)
        {
            tab.prompts.retain(|prompt| prompt.id() != id);
            if tab.cert.as_ref().is_some_and(|(asked, _)| *asked == id) {
                tab.cert = None;
            }
        }
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.request == Some(id))
        {
            self.menu = None;
        }
    }
}
