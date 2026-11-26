//! What the person can ask for. Each command changes the model and asks the
//! engine for what it needs; none of them waits for the engine's answer, which
//! comes back as an event.

use bingux_ui::web_view::input::{EditCommand, PageInput};
use bingux_ui::web_view::requests::DownloadState;
use bingux_ui::web_view::{DownloadId, FindQuery, TabId};

use super::{Browser, ClosedTab, Notice, UiRequest, ViewSize};
use crate::address::{Internal, Typed};
use crate::commands::Command;
use crate::model::tab::{Tab, TabKind};
use crate::model::tabs::Place;
use crate::zoom::Zoom;

/// How many closed tabs Reopen closed tab remembers.
const CLOSED_LIMIT: usize = 25;

impl Browser {
    // ---- Tabs ------------------------------------------------------------------

    /// Open what was typed in a new tab. `typed` is anything the address bar
    /// takes; empty is the new tab page. Returns the tab's key.
    pub fn open_tab(&mut self, typed: &str, place: Place, select: bool) -> Option<u64> {
        let opener = self.tabs.active().map(|tab| tab.key);
        match Typed::internal(typed) {
            Some(Internal::Settings) => return Some(self.open_settings()),
            Some(Internal::NewTab) => return self.create_web("", place, select, opener),
            None => {}
        }
        let url = match Typed::of(typed) {
            Typed::Nothing => String::new(),
            Typed::Address(url) => url,
            Typed::Search(words) => self.search().url_for(&words),
        };
        self.create_web(&url, place, select, opener)
    }

    /// A new tab for a page that was opened from tab `opener`.
    fn create_web(
        &mut self,
        url: &str,
        place: Place,
        select: bool,
        opener: Option<u64>,
    ) -> Option<u64> {
        let key = self.tabs.next_key();
        let mut tab = Tab::blank(key, TabKind::Web);
        tab.url = url.to_string();
        tab.private = self.options.private;
        tab.opener = opener;
        let opener_session = opener.and_then(|key| self.session_of(key));
        let config = self.config(url, opener_session, !select && !self.tabs.is_empty());
        match self.engine.backend().create_tab(&config) {
            Ok(session) => tab.session = Some(session),
            Err(error) => {
                self.out.fail(&error);
                return None;
            }
        }
        if !url.is_empty() {
            tab.load.loading = true;
        }
        let was = self.tabs.active_key();
        self.tabs.insert(tab, place, select);
        if self.tabs.active_key() != was {
            self.shown(was);
        }
        Some(key)
    }

    /// Open the settings page, or go to it when it is already open.
    pub fn open_settings(&mut self) -> u64 {
        if let Some(key) = self
            .tabs
            .all()
            .iter()
            .find(|tab| tab.kind == TabKind::Settings)
            .map(|tab| tab.key)
        {
            self.select(key);
            return key;
        }
        let key = self.tabs.next_key();
        let was = self.tabs.active_key();
        self.tabs
            .insert(Tab::blank(key, TabKind::Settings), Place::AfterActive, true);
        self.shown(was);
        key
    }

    /// Show tab `key`.
    pub fn select(&mut self, key: u64) {
        let was = self.tabs.active_key();
        if was != key && self.tabs.select(key) {
            self.shown(was);
        }
    }

    /// The tab on show changed from `was`: the engine hides the old page and
    /// shows the new one at the size of the view.
    fn shown(&mut self, was: u64) {
        let now = self.tabs.active_key();
        if was == now {
            return;
        }
        if let Some(session) = self.session_of(was) {
            self.engine_call(|backend| backend.set_visible(session, false, false));
        }
        if let Some(session) = self.session_of(now) {
            let ViewSize {
                width,
                height,
                scale,
            } = self.view;
            self.engine_call(|backend| {
                backend.set_size(session, width, height, scale)?;
                backend.set_visible(session, true, true)
            });
        }
        // A menu belongs to the page it was asked on.
        if self.menu.as_ref().is_some_and(|menu| menu.tab != now) {
            self.dismiss_menu();
        }
        if self.fullscreen && self.tabs.active().is_some_and(|tab| !tab.fullscreen) {
            self.fullscreen = false;
            self.out.request(UiRequest::Fullscreen(false));
        }
    }

    /// Close tab `key`. The last one closes the window.
    pub fn close_tab(&mut self, key: u64) {
        let was = self.tabs.active_key();
        let Some((tab, _)) = self.tabs.remove(key) else {
            return;
        };
        if let Some(session) = tab.session {
            self.engine_call(|backend| backend.close_tab(session));
        }
        // What the tab was waiting for will never be answered by anyone.
        if !(tab.is_blank() && tab.kind == TabKind::Web) {
            self.closed.push(ClosedTab {
                url: tab.url.clone(),
                title: tab.display_title(),
                pinned: tab.pinned,
                kind: tab.kind,
            });
            if self.closed.len() > CLOSED_LIMIT {
                self.closed.remove(0);
            }
        }
        if self.menu.as_ref().is_some_and(|menu| menu.tab == key) {
            self.menu = None;
        }
        if self.tabs.is_empty() {
            self.out.request(UiRequest::CloseWindow);
            return;
        }
        if self.tabs.active_key() != was {
            // The tab on show went away: its replacement is shown, and there
            // is no page to hide.
            let now = self.tabs.active_key();
            if let Some(session) = self.session_of(now) {
                let ViewSize {
                    width,
                    height,
                    scale,
                } = self.view;
                self.engine_call(|backend| {
                    backend.set_size(session, width, height, scale)?;
                    backend.set_visible(session, true, true)
                });
            }
            if self.fullscreen {
                self.fullscreen = false;
                self.out.request(UiRequest::Fullscreen(false));
            }
        }
    }

    /// Open the tab closed last, where it was.
    pub fn reopen_closed(&mut self) {
        let Some(closed) = self.closed.pop() else {
            self.out.notify("There is no closed tab to reopen.");
            return;
        };
        let key = match closed.kind {
            TabKind::Settings => Some(self.open_settings()),
            TabKind::Web => self.create_web(&closed.url, Place::End, true, None),
        };
        if let (Some(key), true) = (key, closed.pinned) {
            self.tabs.set_pinned(key, true);
        }
    }

    /// A second tab at the same address, next to this one.
    pub fn duplicate_tab(&mut self, key: u64) {
        let Some(tab) = self.tabs.get(key) else {
            return;
        };
        let (url, kind) = (tab.url.clone(), tab.kind);
        match kind {
            TabKind::Settings => {
                self.open_settings();
            }
            TabKind::Web => {
                self.create_web(&url, Place::After(key), true, Some(key));
            }
        }
    }

    pub fn set_pinned(&mut self, key: u64, pinned: bool) {
        self.tabs.set_pinned(key, pinned);
    }

    /// The strip moved tab `key` to place `to`.
    pub fn move_tab(&mut self, key: u64, to: usize) {
        self.tabs.move_to(key, to);
    }

    /// Close every tab but `key` (not the pinned ones).
    pub fn close_others(&mut self, key: u64) {
        let others: Vec<u64> = self
            .tabs
            .all()
            .iter()
            .filter(|tab| tab.key != key && !tab.pinned)
            .map(|tab| tab.key)
            .collect();
        for other in others {
            self.close_tab(other);
        }
    }

    /// Close the tabs after `key` (not the pinned ones).
    pub fn close_after(&mut self, key: u64) {
        let Some(at) = self.tabs.position(key) else {
            return;
        };
        let later: Vec<u64> = self.tabs.all()[at + 1..]
            .iter()
            .filter(|tab| !tab.pinned)
            .map(|tab| tab.key)
            .collect();
        for other in later {
            self.close_tab(other);
        }
    }

    /// Mute or unmute the tab's sound.
    pub fn toggle_mute(&mut self, key: u64) {
        let (Some(session), Some(muted)) = (
            self.session_of(key),
            self.tabs.get(key).map(|tab| tab.audio.muted),
        ) else {
            return;
        };
        match self.engine.backend().set_muted(session, !muted) {
            Ok(()) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.audio.muted = !muted;
                }
            }
            Err(error) => self.out.fail(&error),
        }
    }

    // ---- Navigation ------------------------------------------------------------

    /// The person typed `typed` in the address bar of tab `key` and pressed
    /// Enter.
    pub fn navigate(&mut self, key: u64, typed: &str) {
        match Typed::internal(typed) {
            Some(Internal::Settings) => {
                self.open_settings();
                return;
            }
            Some(Internal::NewTab) => {
                self.go(key, "");
                return;
            }
            None => {}
        }
        let url = match Typed::of(typed) {
            Typed::Nothing => return,
            Typed::Address(url) => url,
            Typed::Search(words) => self.search().url_for(&words),
        };
        self.go(key, &url);
    }

    /// Load `url` in tab `key`. A settings tab becomes a web tab.
    pub fn go(&mut self, key: u64, url: &str) {
        if self
            .tabs
            .get(key)
            .is_some_and(|tab| tab.kind == TabKind::Settings)
        {
            let config = self.config(url, None, false);
            match self.engine.backend().create_tab(&config) {
                Ok(session) => {
                    if let Some(tab) = self.tabs.get_mut(key) {
                        tab.kind = TabKind::Web;
                        tab.session = Some(session);
                        tab.private = self.options.private;
                        tab.url = url.to_string();
                        tab.load.loading = !url.is_empty();
                    }
                    self.shown_again(key);
                }
                Err(error) => self.out.fail(&error),
            }
            return;
        }
        let Some(session) = self.session_of(key) else {
            return;
        };
        if url.is_empty() {
            // The new tab page: the engine's page is blank behind it.
            self.engine_call(|backend| backend.load(session, "about:blank"));
        } else {
            self.engine_call(|backend| backend.load(session, url));
        }
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.url = url.to_string();
            tab.title.clear();
            tab.favicon = None;
            tab.gone = None;
            tab.cert = None;
            tab.load.error = None;
            tab.load.loading = !url.is_empty();
            tab.load.progress = 0.0;
        }
    }

    /// A tab that had no session has one: show it as the engine's tab.
    pub(super) fn shown_again(&mut self, key: u64) {
        if key != self.tabs.active_key() {
            return;
        }
        if let Some(session) = self.session_of(key) {
            let ViewSize {
                width,
                height,
                scale,
            } = self.view;
            self.engine_call(|backend| {
                backend.set_size(session, width, height, scale)?;
                backend.set_visible(session, true, true)
            });
        }
    }

    pub fn back(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.back(session));
        }
    }

    pub fn forward(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.forward(session));
        }
    }

    pub fn go_to_history(&mut self, key: u64, index: usize) {
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.go_to_history(session, index));
        }
    }

    /// Reload, or load again the page a crash or an error replaced.
    pub fn reload(&mut self, key: u64, bypass_cache: bool) {
        let Some(tab) = self.tabs.get(key) else {
            return;
        };
        let (session, recover, url) = (
            tab.session,
            tab.gone.is_some() || tab.cert.is_some(),
            tab.url.clone(),
        );
        let Some(session) = session else {
            return;
        };
        if recover && !url.is_empty() {
            // A crashed tab has no page to reload: a new load makes one.
            self.go(key, &url);
            return;
        }
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.load.error = None;
            tab.load.loading = true;
            tab.load.progress = 0.0;
        }
        self.engine_call(|backend| backend.reload(session, bypass_cache));
    }

    pub fn stop(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.stop(session));
        }
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.load.loading = false;
        }
    }

    // ---- The view ---------------------------------------------------------------

    /// The page area has this size (logical pixels) at this scale.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
        let next = ViewSize {
            width: width.max(1),
            height: height.max(1),
            scale,
        };
        if next == self.view {
            return;
        }
        self.view = next;
        if let Some(session) = self.session_of(self.tabs.active_key()) {
            self.engine_call(|backend| {
                backend.set_size(session, next.width, next.height, next.scale)
            });
        }
    }

    /// Set the zoom of tab `key`, kept to what the engine allows.
    pub fn set_zoom(&mut self, key: u64, to: Zoom) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.zoom.set(tab.session, to, &self.engine, &mut self.out);
        }
    }

    /// Send input to the page of tab `key`.
    pub fn send(&mut self, key: u64, input: PageInput) {
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.send(session, input));
        }
    }

    /// The window is the active one or not: the page shows its caret.
    pub fn set_window_focus(&mut self, focused: bool) {
        let action = if focused {
            bingux_ui::web_view::input::FocusAction::Activate
        } else {
            bingux_ui::web_view::input::FocusAction::Deactivate
        };
        self.send(self.tabs.active_key(), PageInput::Focus(action));
    }

    // ---- Find in page ------------------------------------------------------------

    /// Open the find bar of tab `key`.
    pub fn open_find(&mut self, key: u64) {
        // The engine took the call, or did not, at its first session: after
        // the window read what the engine can do, when it opened with none.
        self.engine.refresh();
        if !self.engine.capabilities().find {
            self.out
                .notify("Find in page is not available in this engine yet.");
            return;
        }
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.open = true;
        }
        self.out.request(UiRequest::OpenFind);
    }

    /// Close the find bar and clear what it highlighted.
    pub fn close_find(&mut self, key: u64) {
        let Some(session) = self.session_of(key) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.open = false;
            tab.find.result = None;
        }
        self.engine_call(|backend| backend.find_stop(session));
    }

    /// The text of the find bar changed, or Enter asked for the next match.
    pub fn find(&mut self, key: u64, text: &str, forward: bool, restart: bool) {
        let Some(session) = self.session_of(key) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(key) else {
            return;
        };
        tab.find.text = text.to_string();
        if text.is_empty() {
            tab.find.result = None;
            self.engine_call(|backend| backend.find_stop(session));
            return;
        }
        let query = FindQuery {
            text: text.to_string(),
            forward,
            match_case: tab.find.match_case,
            whole_words: tab.find.whole_words,
            highlight_all: tab.find.highlight_all,
            restart,
        };
        self.engine_call(|backend| backend.find(session, &query));
    }

    /// Match case, whole words or highlight all changed.
    pub fn set_find_options(
        &mut self,
        key: u64,
        match_case: bool,
        whole_words: bool,
        highlight_all: bool,
    ) {
        let Some(tab) = self.tabs.get_mut(key) else {
            return;
        };
        tab.find.match_case = match_case;
        tab.find.whole_words = whole_words;
        tab.find.highlight_all = highlight_all;
        let text = tab.find.text.clone();
        self.find(key, &text, true, true);
    }

    // ---- Downloads ---------------------------------------------------------------

    /// Take finished, failed and cancelled downloads off the list, and have
    /// the engine forget them.
    pub fn clear_finished_downloads(&mut self) {
        let over: Vec<DownloadId> = self
            .downloads
            .list()
            .iter()
            .filter(|download| {
                !matches!(
                    download.state,
                    DownloadState::Running | DownloadState::Paused
                )
            })
            .map(|download| download.id)
            .collect();
        if self.engine.capabilities().download_control {
            for id in over {
                // The engine ignores an id it does not hold (a restart forgot it).
                self.engine_call(|backend| backend.remove_download(id));
            }
        }
        self.downloads.clear_finished();
    }

    // ---- Commands -----------------------------------------------------------------

    /// Run a shortcut or a menu command on the tab on show.
    pub fn run(&mut self, command: Command) {
        let key = self.tabs.active_key();
        match command {
            Command::NewTab => {
                self.open_tab("", Place::AfterActive, true);
                self.out.request(UiRequest::FocusAddress);
            }
            Command::NewPrivateWindow => self.out.request(UiRequest::NewPrivateWindow),
            Command::CloseTab => self.close_tab(key),
            Command::ReopenTab => self.reopen_closed(),
            Command::NextTab => self.step(1),
            Command::PreviousTab => self.step(-1),
            Command::GoToTab(n) => {
                if let Some(target) = self.tabs.nth(n) {
                    self.select(target);
                }
            }
            Command::LastTab => {
                if let Some(target) = self.tabs.last() {
                    self.select(target);
                }
            }
            Command::MoveTabLeft => self.shift_tab(key, -1),
            Command::MoveTabRight => self.shift_tab(key, 1),
            Command::FocusAddress => self.out.request(UiRequest::FocusAddress),
            Command::Find => self.open_find(key),
            Command::FindNext => self.find_step(key, true),
            Command::FindPrevious => self.find_step(key, false),
            Command::Reload => self.reload(key, false),
            Command::ReloadBypassCache => self.reload(key, true),
            Command::Back => self.back(key),
            Command::Forward => self.forward(key),
            Command::ZoomIn => self.zoom_by(key, Zoom::stepped_in),
            Command::ZoomOut => self.zoom_by(key, Zoom::stepped_out),
            Command::ZoomReset => self.set_zoom(key, Zoom::DEFAULT),
            Command::Fullscreen => {
                let on = !self.fullscreen;
                self.out.request(UiRequest::Fullscreen(on));
                self.fullscreen = on;
                if let Some(session) = self.session_of(key) {
                    self.engine_call(|backend| backend.set_fullscreen(session, on));
                }
            }
            Command::Downloads => self.out.request(UiRequest::OpenDownloads),
            Command::Settings => {
                self.open_settings();
            }
            Command::PageInfo => self.out.request(UiRequest::OpenPageInfo),
            Command::ViewSource => {
                let url = self
                    .tabs
                    .active()
                    .filter(|tab| tab.kind == TabKind::Web && !tab.is_blank())
                    .map(|tab| tab.url.clone());
                if let Some(url) = url.filter(|url| !url.starts_with("view-source:")) {
                    self.open_tab(&format!("view-source:{url}"), Place::AfterActive, true);
                }
            }
            Command::Print => self.out.notice(Notice {
                error: true,
                text: "Printing is not available in this engine yet.".to_string(),
                action: None,
            }),
            Command::MainMenu => self.out.request(UiRequest::OpenMainMenu),
            Command::Edit(edit) => self.send_edit(key, edit),
        }
    }

    fn send_edit(&mut self, key: u64, edit: EditCommand) {
        self.send(key, PageInput::Edit(edit));
    }

    fn step(&mut self, by: isize) {
        if let Some(target) = self.tabs.step(by) {
            self.select(target);
        }
    }

    fn shift_tab(&mut self, key: u64, by: isize) {
        let Some(at) = self.tabs.position(key) else {
            return;
        };
        let to = (at as isize + by).max(0) as usize;
        self.tabs.move_to(key, to);
    }

    fn zoom_by(&mut self, key: u64, step: fn(Zoom) -> Zoom) {
        let Some(current) = self.tabs.get(key).map(|tab| tab.zoom) else {
            return;
        };
        self.set_zoom(key, step(current));
    }

    fn find_step(&mut self, key: u64, forward: bool) {
        let Some(tab) = self.tabs.get(key) else {
            return;
        };
        if tab.find.text.is_empty() {
            self.open_find(key);
            return;
        }
        let text = tab.find.text.clone();
        self.find(key, &text, forward, false);
    }

    /// The tab whose engine session is `session`.
    pub fn key_of(&self, session: TabId) -> Option<u64> {
        self.tabs.by_session(session).map(|tab| tab.key)
    }
}
