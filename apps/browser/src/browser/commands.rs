//! What the person can ask for. Each command changes the model and asks the
//! engine for what it needs; none of them waits for the engine's answer, which
//! comes back as an event.

use bingux_ui::prelude::tr;
use bingux_ui::web_view::input::{EditCommand, PageInput};
use bingux_ui::web_view::requests::DownloadState;
use bingux_ui::web_view::{DownloadId, TabId};

use super::{Browser, ClosedTab, DuplicatesAsk, UiRequest, ViewSize};
use crate::address::{Internal, Typed};
use crate::tabs::Place;
use crate::tabs::{Tab, TabKind};
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
            self.engine.call(&mut self.out, |backend| {
                backend.set_visible(session, false, false)
            });
        }
        self.set_toolbox_visible(was, false);
        if let Some(session) = self.session_of(now) {
            let ViewSize {
                width,
                height,
                scale,
            } = self.view;
            self.engine.call(&mut self.out, |backend| {
                backend.set_size(session, width, height, scale)?;
                backend.set_visible(session, true, true)
            });
        }
        self.set_toolbox_visible(now, true);
        // A menu belongs to the page it was asked on.
        if self.menu.as_ref().is_some_and(|menu| menu.tab != now) {
            self.dismiss_menu();
        }
        if self.fullscreen && self.tabs.active().is_some_and(|tab| !tab.fullscreen) {
            self.fullscreen = false;
            self.out.request(UiRequest::Fullscreen(false));
        }
    }

    /// Escape belongs to the browser when the window is fullscreen by its own
    /// doing (F11). A page that asked for fullscreen takes Escape itself: the
    /// engine ends it and says so (the tab's fullscreen flag), and the window
    /// follows. Forwarding is the only way out the engine offers, and it
    /// leaves the page's own state right.
    pub fn escape_leaves_window_fullscreen(&self) -> bool {
        self.fullscreen && !self.tabs.active().is_some_and(|tab| tab.fullscreen)
    }

    /// Close tab `key`. The last one closes the window.
    pub fn close_tab(&mut self, key: u64) {
        let was = self.tabs.active_key();
        let Some((tab, _)) = self.tabs.remove(key) else {
            return;
        };
        if let Some(session) = tab.session {
            self.engine
                .call(&mut self.out, |backend| backend.close_tab(session));
        }
        // The engine closes a toolbox after its target; letting go of ours now
        // frees its surface without waiting for that.
        if let Some(docked) = tab.toolbox {
            self.engine.call(&mut self.out, |backend| {
                backend.close_devtools(docked.handle)
            });
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
                self.engine.call(&mut self.out, |backend| {
                    backend.set_size(session, width, height, scale)?;
                    backend.set_visible(session, true, true)
                });
            }
            self.set_toolbox_visible(now, true);
            if self.fullscreen {
                self.fullscreen = false;
                self.out.request(UiRequest::Fullscreen(false));
            }
        }
    }

    /// Open the tab closed last, where it was.
    pub fn reopen_closed(&mut self) {
        if self.closed.is_empty() {
            self.out
                .notify(tr!("browser-tabs-none-to-reopen-notification"));
            return;
        }
        self.reopen_closed_at(self.closed.len() - 1);
    }

    /// Open the closed tab at `index` of `closed` (the newest is last), where
    /// it was, and take it off the list.
    pub fn reopen_closed_at(&mut self, index: usize) {
        if index >= self.closed.len() {
            return;
        }
        let closed = self.closed.remove(index);
        let key = match closed.kind {
            TabKind::Settings => Some(self.open_settings()),
            TabKind::Web => self.create_web(&closed.url, Place::End, true, None),
        };
        if let (Some(key), true) = (key, closed.pinned) {
            self.tabs.set_pinned(key, true);
        }
    }

    /// Open the window of the profile with registry id `id`. This window's own
    /// profile is already open here, so that asks for nothing.
    pub fn open_profile(&mut self, id: &str) {
        if self.profile.as_ref().is_some_and(|(own, _)| own == id) {
            return;
        }
        self.out.request(UiRequest::OpenProfile(id.to_string()));
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

    /// Close the tabs that only repeat a page another tab has
    /// ([`crate::tabs::TabList::duplicates`]), as Firefox's `removeAllDuplicateTabs`
    /// does: nothing when there are none, the question of `warnAboutClosingTabs`
    /// first when it applies ([`Browser::duplicates_ask`]), then the tabs close and the
    /// confirmation hint says how many.
    pub fn close_duplicate_tabs(&mut self) {
        let keys = self.tabs.duplicates();
        if keys.is_empty() {
            return;
        }
        match self.duplicates_ask(keys.len()) {
            None => self.close_tabs_as_duplicates(keys),
            Some(ask) => self
                .out
                .request(UiRequest::ConfirmCloseDuplicates { keys, ask }),
        }
    }

    /// What, if anything, to ask before `count` duplicate tabs close: the order of
    /// Firefox's `warnAboutClosingTabs` for `ALL_DUPLICATES`. The first time in a
    /// profile it asks whatever the count, and has the profile remember that it
    /// did before it asks, so Cancel does not bring it back. Then
    /// `browser.tabs.warnOnCloseOtherTabs` (on, and with no setting here to turn it
    /// off) asks only when more tabs would go than Reopen closed tab keeps
    /// (`browser.sessionstore.max_tabs_undo`, [`CLOSED_LIMIT`]); Firefox's own check
    /// that one tab is never asked about is that limit's lower bound.
    fn duplicates_ask(&mut self, count: usize) -> Option<DuplicatesAsk> {
        if !self.settings.have_shown_close_all_duplicate_tabs_warning {
            self.settings.have_shown_close_all_duplicate_tabs_warning = true;
            if let Some(store) = &self.store {
                store.save_settings(&self.settings);
            }
            return Some(DuplicatesAsk::FirstTime);
        }
        (count > CLOSED_LIMIT).then_some(DuplicatesAsk::Many(count))
    }

    /// Close `keys`, which Close duplicate tabs chose (and the person agreed to
    /// close, if it asked), and say how many went: Firefox's confirmation hint
    /// (`confirmation-hint-duplicate-tabs-closed`). The window has no anchored hint, so it is a toast.
    pub fn close_tabs_as_duplicates(&mut self, keys: Vec<u64>) {
        let count = keys.len();
        for key in keys {
            self.close_tab(key);
        }
        self.out.notify(tr!(
            "browser-confirmation-hint-duplicate-tabs-closed",
            count = count
        ));
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
            self.engine.call(&mut self.out, |backend| {
                backend.load(session, "about:blank")
            });
        } else {
            self.engine
                .call(&mut self.out, |backend| backend.load(session, url));
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
    pub(crate) fn shown_again(&mut self, key: u64) {
        if key != self.tabs.active_key() {
            return;
        }
        if let Some(session) = self.session_of(key) {
            let ViewSize {
                width,
                height,
                scale,
            } = self.view;
            self.engine.call(&mut self.out, |backend| {
                backend.set_size(session, width, height, scale)?;
                backend.set_visible(session, true, true)
            });
        }
    }

    pub fn back(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine
                .call(&mut self.out, |backend| backend.back(session));
        }
    }

    pub fn forward(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine
                .call(&mut self.out, |backend| backend.forward(session));
        }
    }

    pub fn go_to_history(&mut self, key: u64, index: usize) {
        if let Some(session) = self.session_of(key) {
            self.engine.call(&mut self.out, |backend| {
                backend.go_to_history(session, index)
            });
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
        self.engine.call(&mut self.out, |backend| {
            backend.reload(session, bypass_cache)
        });
    }

    pub fn stop(&mut self, key: u64) {
        if let Some(session) = self.session_of(key) {
            self.engine
                .call(&mut self.out, |backend| backend.stop(session));
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
            self.engine.call(&mut self.out, |backend| {
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
            self.engine
                .call(&mut self.out, |backend| backend.send(session, input));
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
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.open(&mut self.engine, &mut self.out);
        }
    }

    /// Close the find bar and clear what it highlighted.
    pub fn close_find(&mut self, key: u64) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.close(tab.session, &self.engine, &mut self.out);
        }
    }

    /// The text of the find bar changed, or Enter asked for the next match.
    pub fn find(&mut self, key: u64, text: &str, forward: bool, restart: bool) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.search(
                tab.session,
                text,
                forward,
                restart,
                &self.engine,
                &mut self.out,
            );
        }
    }

    /// Match case, whole words or highlight all changed.
    pub fn set_find_options(
        &mut self,
        key: u64,
        match_case: bool,
        whole_words: bool,
        highlight_all: bool,
    ) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find.set_options(
                tab.session,
                (match_case, whole_words, highlight_all),
                &self.engine,
                &mut self.out,
            );
        }
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
                self.engine
                    .call(&mut self.out, |backend| backend.remove_download(id));
            }
        }
        self.downloads.clear_finished();
    }

    pub(crate) fn send_edit(&mut self, key: u64, edit: EditCommand) {
        self.send(key, PageInput::Edit(edit));
    }

    pub(crate) fn step(&mut self, by: isize) {
        if let Some(target) = self.tabs.step(by) {
            self.select(target);
        }
    }

    pub(crate) fn shift_tab(&mut self, key: u64, by: isize) {
        let Some(at) = self.tabs.position(key) else {
            return;
        };
        let to = (at as isize + by).max(0) as usize;
        self.tabs.move_to(key, to);
    }

    pub(crate) fn zoom_by(&mut self, key: u64, step: fn(Zoom) -> Zoom) {
        let Some(current) = self.tabs.get(key).map(|tab| tab.zoom) else {
            return;
        };
        self.set_zoom(key, step(current));
    }

    pub(crate) fn find_step(&mut self, key: u64, forward: bool) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.find
                .step(tab.session, forward, &mut self.engine, &mut self.out);
        }
    }

    /// The tab whose engine session is `session`.
    pub fn key_of(&self, session: TabId) -> Option<u64> {
        self.tabs.by_session(session).map(|tab| tab.key)
    }
}
