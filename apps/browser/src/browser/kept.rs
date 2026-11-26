//! What the profile's store says about keeping, turned into state: the
//! settings page reads [`Browser::settings_keeping`] and
//! [`Browser::history_keeping`], and a part that stops being kept is told
//! once in a toast. History itself is the store's and the bingux-ui
//! searchable `history`; the window only sends it what was visited.

use bingux_ui::prelude::tr;

use super::Browser;
use crate::store::Event;
use crate::store::{Keeping, Part};

impl Browser {
    /// Take in what the store has said since the last call.
    pub(crate) fn take_store_events(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        for event in store.take_events() {
            match event {
                Event::Settings(keeping) => self.kept_settings(keeping),
                Event::History(keeping) => self.kept_history(keeping),
            }
        }
    }

    /// How keeping the settings goes now. A change into a state that keeps
    /// nothing is told once; a window that keeps nothing by design (private,
    /// no profile) says so on its settings page only.
    pub(super) fn kept_settings(&mut self, keeping: Keeping) {
        if keeping == self.settings_keeping {
            return;
        }
        if !keeping.is_by_design()
            && let Some(text) = keeping.text(Part::Settings)
        {
            self.out.notify(text);
        }
        self.settings_keeping = keeping;
    }

    /// How keeping the history goes now, told as the settings are.
    fn kept_history(&mut self, keeping: Keeping) {
        if keeping == self.history_keeping {
            return;
        }
        if !keeping.is_by_design()
            && let Some(text) = keeping.text(Part::History)
        {
            self.out.notify(text);
        }
        self.history_keeping = keeping;
    }

    /// Whether this window has the `history` searchable: a window of a
    /// profile. A private window, and one without a profile, have none.
    pub fn has_history(&self) -> bool {
        self.store.is_some()
    }

    /// How many pages the history holds.
    pub fn history_len(&self) -> usize {
        self.store.as_ref().map_or(0, |store| store.history_len())
    }

    /// Forget the history: the browser's file and searchable, then the
    /// engine's own record of visits (Places), which is what colours visited
    /// links. The engine answers when it is done (`HistoryCleared`); only a
    /// failure is news then.
    pub fn clear_history(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        store.clear_history();
        self.engine
            .call(&mut self.out, |backend| backend.clear_history());
        self.out.notify(tr!("browser-history-cleared"));
    }

    /// The history file could not be read: move it aside and keep history
    /// in a new one. Only offered for a damaged file.
    pub fn start_over_history(&mut self) {
        if !self.history_keeping.can_start_over() {
            return;
        }
        if let Some(store) = &self.store {
            store.start_over_history();
        }
    }

    /// Wake `waker` when the store has something to say.
    pub fn set_store_waker(&self, waker: std::task::Waker) {
        if let Some(store) = &self.store {
            store.set_waker(waker);
        }
    }

    /// The settings file could not be read: move it aside and keep the
    /// settings in use from now on. Only offered for a damaged file.
    pub fn start_over_settings(&mut self) {
        if !self.settings_keeping.can_start_over() {
            return;
        }
        if let Some(store) = &self.store {
            store.start_over_settings(&self.settings);
        }
    }

    /// Waits until the store has written everything sent so far. For tests
    /// and the window's last moment, never for a frame.
    pub fn flush_store(&mut self) {
        if let Some(store) = &self.store {
            store.flush();
        }
        self.take_store_events();
    }
}
