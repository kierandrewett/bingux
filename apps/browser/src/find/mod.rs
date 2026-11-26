//! Find in page.

mod bar;
mod state;

pub use bar::FindBar;
pub use state::BarState;

use bingux_ui::prelude::tr;
use bingux_ui::web_view::{FindQuery, TabId};

use crate::browser::{Outbox, UiRequest};
use crate::engine::Engine;

/// The search of the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Find {
    open: bool,
    text: String,
    match_case: bool,
    whole_words: bool,
    highlight_all: bool,
    /// `(current, total)` of the last search; `None` before one.
    result: Option<(u32, u32)>,
}

impl Find {
    /// Whether the find bar is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// What the bar searches for.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// `(match case, whole words, highlight all)`.
    pub fn options(&self) -> (bool, bool, bool) {
        (self.match_case, self.whole_words, self.highlight_all)
    }

    /// `(current, total)` of the last search; `None` before one.
    pub fn result(&self) -> Option<(u32, u32)> {
        self.result
    }

    /// The search found nothing.
    pub fn not_found(&self) -> bool {
        !self.text.is_empty() && self.result.is_some_and(|(_, total)| total == 0)
    }

    /// The page changed under the search: what it counted is gone.
    pub fn clear_result(&mut self) {
        self.result = None;
    }

    /// The engine counted the matches of the last search.
    pub fn on_result(&mut self, current: u32, total: u32) {
        self.result = Some((current, total));
    }

    /// Open the find bar, if the engine can search.
    pub fn open(&mut self, engine: &mut Engine, out: &mut Outbox) {
        // The engine took the call, or did not, at its first session: after
        // the window read what the engine can do, when it opened with none.
        engine.refresh();
        if !engine.capabilities().find {
            out.notify(tr!("browser-find-unavailable"));
            return;
        }
        self.open = true;
        out.request(UiRequest::OpenFind);
    }

    /// Close the find bar and clear what it highlighted.
    pub fn close(&mut self, session: Option<TabId>, engine: &Engine, out: &mut Outbox) {
        let Some(session) = session else {
            return;
        };
        self.open = false;
        self.result = None;
        engine.call(out, |backend| backend.find_stop(session));
    }

    /// The text of the find bar changed, or Enter asked for the next match.
    pub fn search(
        &mut self,
        session: Option<TabId>,
        text: &str,
        forward: bool,
        restart: bool,
        engine: &Engine,
        out: &mut Outbox,
    ) {
        let Some(session) = session else {
            return;
        };
        self.text = text.to_string();
        if text.is_empty() {
            self.result = None;
            engine.call(out, |backend| backend.find_stop(session));
            return;
        }
        let query = FindQuery {
            text: text.to_string(),
            forward,
            match_case: self.match_case,
            whole_words: self.whole_words,
            highlight_all: self.highlight_all,
            restart,
        };
        engine.call(out, |backend| backend.find(session, &query));
    }

    /// Match case, whole words or highlight all changed.
    pub fn set_options(
        &mut self,
        session: Option<TabId>,
        options: (bool, bool, bool),
        engine: &Engine,
        out: &mut Outbox,
    ) {
        (self.match_case, self.whole_words, self.highlight_all) = options;
        let text = self.text.clone();
        self.search(session, &text, true, true, engine, out);
    }

    /// The next or previous match; the bar opens when nothing is searched for.
    pub fn step(
        &mut self,
        session: Option<TabId>,
        forward: bool,
        engine: &mut Engine,
        out: &mut Outbox,
    ) {
        if self.text.is_empty() {
            self.open(engine, out);
            return;
        }
        let text = self.text.clone();
        self.search(session, &text, forward, false, engine, out);
    }
}

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
