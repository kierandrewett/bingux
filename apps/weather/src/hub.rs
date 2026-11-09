//! What the parts of the window change together: the settings (the saved
//! places, the units, the place on show), the count of refreshes asked for and
//! of location lookups, and whether a phone shows the list or the page.
//!
//! The handles are `State`s, which are `Copy`, so a `Hub` is passed to the
//! lists, the menus and the search sheet and used in their handlers. Every
//! change to the settings is written to disk from a worker.

use bingux_weather::store::HERE;
use bingux_weather::{Place, Settings, Units};
use freya::prelude::*;

use crate::load;

#[derive(Clone, Copy, PartialEq)]
pub struct Hub {
    pub settings: State<Settings>,
    /// The times a refresh was asked for. Part of every forecast's key.
    pub refresh: State<u64>,
    /// The times the position was asked for again.
    pub attempt: State<u64>,
    /// On a phone: whether the list of places is the screen.
    pub list: State<bool>,
}

impl Hub {
    /// Changes the settings and saves them.
    pub fn change(self, edit: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings;
        edit(&mut settings.write());
        let saved: Settings = (*settings.peek()).clone();
        let keep: Vec<String> = saved.saved.iter().map(Place::key).collect();
        load::save(saved);
        load::prune(keep);
    }

    /// The place with `key` (or [`HERE`]) is the one on show. On a phone that
    /// leaves the list for the page.
    pub fn select(self, key: &str) {
        let key = key.to_string();
        self.change(move |settings| settings.selected = Some(key));
        let mut list = self.list;
        list.set_if_modified(false);
    }

    /// Adds `place` to the saved places, unless it is there, and shows it.
    pub fn add(self, place: Place) {
        let key = place.key();
        self.change(move |settings| {
            if !settings.saved.iter().any(|saved| saved.key() == key) {
                settings.saved.push(place);
            }
            settings.selected = Some(key);
        });
        let mut list = self.list;
        list.set_if_modified(false);
    }

    /// Takes a place off the list.
    pub fn remove(self, key: &str) {
        let key = key.to_string();
        self.change(move |settings| {
            settings.saved.retain(|place| place.key() != key);
            if settings.selected.as_deref() == Some(key.as_str()) {
                settings.selected = None;
            }
        });
    }

    /// Moves a saved place; `from` and `to` are places in the saved list.
    pub fn reorder(self, from: usize, to: usize) {
        self.change(move |settings| {
            if from < settings.saved.len() && to < settings.saved.len() {
                let place = settings.saved.remove(from);
                settings.saved.insert(to, place);
            }
        });
    }

    /// Whether "My location" is a place of the list.
    pub fn use_location(self, on: bool) {
        self.change(move |settings| {
            settings.use_location = on;
            if !on && settings.selected.as_deref() == Some(HERE) {
                settings.selected = None;
            }
        });
        if on {
            let mut attempt = self.attempt;
            *attempt.write() += 1;
        }
    }

    /// Fetches every place again: the forecasts are too old to reuse once half
    /// a minute has passed.
    pub fn refresh_all(self) {
        let mut refresh = self.refresh;
        *refresh.write() += 1;
    }

    /// Looks for the computer's position again.
    pub fn locate_again(self) {
        let mut attempt = self.attempt;
        *attempt.write() += 1;
    }

    /// Chooses the units. `None` follows the locale.
    pub fn units(self, units: Option<Units>) {
        self.change(move |settings| settings.units = units);
    }
}
