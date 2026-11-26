//! Find in page: the bar of a tab, what was asked for, and how far the engine has got.
//!
//! The search itself is the engine's. This feature owns the bar's lifecycle
//! (closed, open and idle, searching, counted) so that the bar never shows a
//! count that belongs to other text. The window's focus request for the field
//! is `BarState`.

mod bar;
mod state;

pub use bar::FindBar;
pub use state::BarState;

use bingux_ui::prelude::tr;
use bingux_ui::web_view::{FindQuery, TabId};

use crate::browser::{Outbox, UiRequest};
use crate::engine::Engine;

/// What the person asked for. It survives closing the bar, so Find next
/// (F3) and reopening the bar start from the last text and options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Query {
    text: String,
    match_case: bool,
    whole_words: bool,
    highlight_all: bool,
}

/// How far the engine has got with the query of an open bar.
///
/// Why an enum: a count belongs to the text it was made for. As a separate
/// `Option` beside the text, an old count outlived an edit of the text and the
/// field showed "not found" for words nobody had searched for yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Progress {
    /// Nothing is being searched for: the text is empty, or the page changed
    /// under the last search and what it counted is gone.
    #[default]
    Idle,
    /// The query was sent. The engine has not said how many matches it has
    /// (`nsITypeAheadFind` calls this `FIND_PENDING`).
    Pending,
    /// The engine counted. `current` is 1-based, 0 for none; a `total` of 0 is
    /// "not found".
    Counted { current: u32, total: u32 },
}

/// The bar of one tab: shut, or open with the progress of its search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Bar {
    #[default]
    Closed,
    Open(Progress),
}

/// The search of the page.
///
/// The lifecycle is: closed; open and idle; searching; counted. Only an open
/// bar keeps a count, so a count that arrives after the bar closed is dropped,
/// and a new text is never shown with the count of an old one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Find {
    query: Query,
    bar: Bar,
}

impl Find {
    /// Whether the find bar is open.
    pub fn is_open(&self) -> bool {
        matches!(self.bar, Bar::Open(_))
    }

    /// What the bar searches for.
    pub fn text(&self) -> &str {
        &self.query.text
    }

    /// `(match case, whole words, highlight all)`.
    pub fn options(&self) -> (bool, bool, bool) {
        (
            self.query.match_case,
            self.query.whole_words,
            self.query.highlight_all,
        )
    }

    /// `(current, total)` of the last search; `None` until the engine has
    /// counted the query that is in the bar now.
    pub fn result(&self) -> Option<(u32, u32)> {
        match self.bar {
            Bar::Open(Progress::Counted { current, total }) => Some((current, total)),
            _ => None,
        }
    }

    /// The search found nothing.
    pub fn not_found(&self) -> bool {
        !self.query.text.is_empty() && self.result().is_some_and(|(_, total)| total == 0)
    }

    /// The page changed under the search: what it counted is gone.
    pub fn clear_result(&mut self) {
        if let Bar::Open(_) = self.bar {
            self.bar = Bar::Open(Progress::Idle);
        }
    }

    /// The engine counted the matches of the last search. A count for a bar
    /// that is shut, or for no search, is left over from before and is dropped.
    pub fn on_result(&mut self, current: u32, total: u32) {
        if let Bar::Open(Progress::Pending | Progress::Counted { .. }) = self.bar {
            self.bar = Bar::Open(Progress::Counted { current, total });
        }
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
        // Closed to open. The text is kept but nothing is highlighted any more,
        // so there is no count until the person searches again.
        if !self.is_open() {
            self.bar = Bar::Open(Progress::Idle);
        }
        out.request(UiRequest::OpenFind);
    }

    /// Close the find bar and clear what it highlighted. The bar closes
    /// whether or not there is an engine to tell: a tab with no session (the
    /// engine is gone) has nothing highlighted.
    pub fn close(&mut self, session: Option<TabId>, engine: &Engine, out: &mut Outbox) {
        self.bar = Bar::Closed;
        if let Some(session) = session {
            engine.call(out, |backend| backend.find_stop(session));
        }
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
        self.query.text = text.to_string();
        // A new query: whatever was counted was for other text.
        if let Bar::Open(_) = self.bar {
            self.bar = Bar::Open(if text.is_empty() {
                Progress::Idle
            } else {
                Progress::Pending
            });
        }
        let Some(session) = session else {
            return;
        };
        if text.is_empty() {
            engine.call(out, |backend| backend.find_stop(session));
            return;
        }
        let query = FindQuery {
            text: text.to_string(),
            forward,
            match_case: self.query.match_case,
            whole_words: self.query.whole_words,
            highlight_all: self.query.highlight_all,
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
        (
            self.query.match_case,
            self.query.whole_words,
            self.query.highlight_all,
        ) = options;
        let text = self.query.text.clone();
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
        if self.query.text.is_empty() {
            self.open(engine, out);
            return;
        }
        let text = self.query.text.clone();
        self.search(session, &text, forward, false, engine, out);
    }
}

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
