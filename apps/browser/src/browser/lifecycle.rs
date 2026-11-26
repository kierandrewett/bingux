//! The engine's life as the window sees it: it fails, and a restart opens the
//! tabs again; a page asks for a window, and the browser says yes or no.

use bingux_ui::prelude::tr;
use bingux_ui::web_view::{Disposition, NewWindow, RequestId, TabId};

use super::{Browser, Notice};
use crate::engine;
use crate::tabs::Place;
use crate::tabs::{Tab, TabKind};

impl Browser {
    /// Open what the window starts on: the addresses it was launched with, or
    /// one blank tab. While the engine is not there the window opens on its
    /// page, with no tab; the first restart opens them.
    pub fn open_first(&mut self, first: &[String]) {
        if matches!(self.engine.status, engine::Status::Failed { .. }) {
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

    /// The engine made a session for `window.open`: it becomes a tab next to
    /// the page that opened it, in front of it or behind it as the page asked,
    /// unless the page opened it as it loaded, with no action of the person:
    /// that is a pop-up, and the browser blocks it.
    ///
    /// Only what the engine says is a reason. A request that says nothing
    /// about a gesture (the engine of today does not yet) is not blocked for
    /// that: not knowing is not a verdict.
    pub(crate) fn accept_new_tab(
        &mut self,
        opener: TabId,
        new: TabId,
        url: &str,
        window: &NewWindow,
    ) {
        if window.unrequested && !window.user_gesture {
            if let Err(error) = self.engine.backend().close_tab(new) {
                self.out.fail(&error);
            }
            let from = if window.origin.is_empty() {
                tr!("browser-tabs-popup-origin-unknown")
            } else {
                window.origin.clone()
            };
            self.out.notice(Notice {
                error: false,
                text: tr!("browser-tabs-popup-blocked-notification", site = from),
                action: None,
            });
            return;
        }
        let opener_key = self.key_of(opener);
        let background = window.disposition == Disposition::BackgroundTab;
        let config = self.config("", Some(opener), background);
        if let Err(error) = self.engine.backend().accept_tab(new, &config) {
            self.out.fail(&error);
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
            self.engine.call(&mut self.out, |backend| {
                backend.set_visible(session, false, false)
            });
        }
    }

    /// Open the developer tools of tab `key` in a tab of their own, next to
    /// it and in front. The engine makes the toolbox a session and closes it
    /// after its target; a private tab is refused by the engine.
    pub fn open_devtools(&mut self, key: u64) {
        let page = self
            .tabs
            .get(key)
            .filter(|tab| tab.kind == TabKind::Web && !tab.is_blank())
            .map(|tab| tab.private);
        let (Some(private), Some(session)) = (page, self.session_of(key)) else {
            self.out.notice(Notice {
                error: true,
                text: tr!("browser-devtools-no-page"),
                action: None,
            });
            return;
        };
        if !self.engine.capabilities().devtools || private {
            self.out.notice(Notice {
                error: true,
                text: if private {
                    tr!("browser-devtools-private")
                } else {
                    tr!("browser-devtools-unavailable")
                },
                action: None,
            });
            return;
        }
        let view = self.view;
        let toolbox =
            match self
                .engine
                .backend()
                .open_devtools(session, view.width, view.height, view.scale)
            {
                Ok(toolbox) => toolbox,
                Err(error) => {
                    self.out.fail(&error);
                    return;
                }
            };
        let new = self.tabs.next_key();
        let mut tab = Tab::blank(new, TabKind::Web);
        tab.session = Some(toolbox);
        tab.url = "about:devtools-toolbox".to_string();
        tab.opener = Some(key);
        tab.load.loading = true;
        let was = self.tabs.active_key();
        self.tabs.insert(tab, Place::After(key), true);
        if was != self.tabs.active_key()
            && let Some(session) = self.session_of(was)
        {
            self.engine.call(&mut self.out, |backend| {
                backend.set_visible(session, false, false)
            });
        }
    }

    /// The engine ended a question itself: what the window showed for it
    /// goes, since nothing is left to answer.
    pub(crate) fn request_closed(&mut self, session: TabId, id: RequestId) {
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
