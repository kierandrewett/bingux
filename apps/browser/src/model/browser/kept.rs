//! What the profile's store says about keeping, turned into state: the
//! settings page reads [`Browser::settings_keeping`], and a part that stops
//! being kept is told once in a toast.

use super::Browser;
use crate::model::keeping::{Keeping, NotSaved};
use crate::profile::store::Event;

impl Browser {
    /// Take in what the store has said since the last call.
    pub(super) fn take_store_events(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        for event in store.take_events() {
            match event {
                Event::Settings(keeping) => self.kept_settings(keeping),
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
        if !by_design(&keeping)
            && let Some(text) = keeping.text("Settings")
        {
            self.notify(text);
        }
        self.settings_keeping = keeping;
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

/// A state the window is in by how it was opened, not by a failure.
fn by_design(keeping: &Keeping) -> bool {
    matches!(
        keeping,
        Keeping::NotSaved(NotSaved::Private | NotSaved::NoProfile)
    )
}
