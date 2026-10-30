//! The window side of type-ahead (see [`super::menu_query`] for the rules):
//! reading keys, keeping the query of a list, and drawing rows with their
//! matched letters marked. Every menu, dropdown list and popover list that
//! has rows to pick from uses this one set of pieces.
//!
//! A list that keeps its own highlight, like the dropdown, holds a
//! [`ListQuery`] and asks it about each key:
//!
//! 1. `let ahead = use_list_query();` in `render`.
//! 2. In the key handler, build a [`Candidate`] per row and call
//!    [`ListQuery::key`] (saying whether the list is open). On `Focus`
//!    highlight the row and scroll it into view, on `Activate` pick it, on
//!    `Handled` stop the event, and on `Ignored` carry on with the list's own
//!    keys.
//! 3. Draw each row's label with [`marked_label`] from [`ListQuery::mark`], and
//!    dim it by [`dimmed`].
//! 4. Give the panel [`ListQuery::pill`] with `Panel::pill`.
//! 5. Call [`ListQuery::clear`] when the list opens, closes or picks a row.
//!
//! The menu keeps its query inside its own state ([`super::menu_nav`]) and
//! uses the same row and pill helpers.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::controls::direction::{Direction, use_direction};
use super::menu_nav::{Ms, NavKey};
use super::menu_query::{self, Candidate, Mark, Pill, Reaction, TypeAhead};
use crate::search_highlight;
use crate::theme::Tokens;
use crate::typography::{BODY, Ink};

/// A row that does not match the query is drawn at this opacity.
pub(crate) const DIMMED_OPACITY: f32 = 0.4;

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// The time for [`menu_query`](super::menu_query): milliseconds since the
/// process first asked.
pub(crate) fn now_ms() -> Ms {
    START.elapsed().as_millis() as Ms
}

/// The menu key of a key press, or `None` when the menu does not read it: a
/// letter with Ctrl, Alt or the logo key held is a shortcut for someone else,
/// except the few that edit the query (Ctrl+A, Ctrl+Backspace). `direction` is
/// the one the list reads in (`use_direction()` where it is drawn).
pub(crate) fn nav_key(event: &Event<KeyboardEventData>, direction: Direction) -> Option<NavKey> {
    let modifiers = event.modifiers;
    let rtl = direction.is_rtl();
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let other = modifiers.contains(Modifiers::ALT) || modifiers.contains(Modifiers::META);
    match &event.key {
        Key::Named(named) => {
            if other {
                return None;
            }
            if ctrl {
                return (*named == NamedKey::Backspace).then_some(NavKey::CtrlBackspace);
            }
            Some(match named {
                NamedKey::Escape => NavKey::Escape,
                // The menu's Right is towards the end: it opens a submenu, which a window that
                // reads from the right opens on the left, so there the key is Left.
                NamedKey::ArrowLeft => if rtl { NavKey::Right } else { NavKey::Left },
                NamedKey::ArrowRight => if rtl { NavKey::Left } else { NavKey::Right },
                NamedKey::ArrowDown => NavKey::Down,
                NamedKey::ArrowUp => NavKey::Up,
                NamedKey::Home => NavKey::Home,
                NamedKey::End => NavKey::End,
                NamedKey::Backspace => NavKey::Backspace,
                NamedKey::Delete => NavKey::Delete,
                NamedKey::Enter => NavKey::Enter,
                NamedKey::Tab if modifiers.contains(Modifiers::SHIFT) => NavKey::ShiftTab,
                NamedKey::Tab => NavKey::Tab,
                _ => return None,
            })
        }
        Key::Character(text) => {
            if other {
                return None;
            }
            if ctrl {
                return match text.to_lowercase().as_str() {
                    "n" => Some(NavKey::CtrlN),
                    "p" => Some(NavKey::CtrlP),
                    "a" => Some(NavKey::CtrlA),
                    _ => None,
                };
            }
            if text == " " {
                return Some(NavKey::Space);
            }
            text.chars().next().map(NavKey::Char)
        }
        // `Key` has no other variants.
    }
}

/// Runs `wake` with the time once `after` milliseconds have passed. The task
/// goes with the component that started it.
pub(crate) fn wake_after(after: Ms, wake: impl FnOnce(Ms) + 'static) {
    spawn(async move {
        timer(Duration::from_millis(after)).await;
        wake(now_ms());
    });
}

/// The query of one list that keeps its own highlight. It is `Copy`, so key
/// handlers capture it freely. Make it with [`use_list_query`].
///
/// # No timeout, except for a shut dropdown
///
/// While a list is open its query never times out: it stays until the person
/// edits it away, presses Escape, picks a row or the list closes. People who
/// type slowly, or through assistive technology, would lose it otherwise.
///
/// A dropdown whose list is shut is the one exception, and it follows how a
/// native select box behaves (and the WAI-ARIA listbox guidance): typing picks
/// the best match at once, and nothing on screen shows the query, so a prefix
/// left over from earlier would silently change what the next letter picks.
/// Such a query ends after [`CLOSED_RESET_MS`]. The delay is long on purpose,
/// so a slow typist still finishes a word, and the list opening or closing
/// ends the wait: an open list never has one.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ListQuery {
    ahead: State<TypeAhead>,
    /// Counts changes of mind: a pending shut-list reset only runs if the
    /// count is still the one it was made for.
    epoch: State<u64>,
    /// The direction the list reads in, for its horizontal keys.
    direction: Direction,
}

/// How long a shut dropdown keeps what was typed on it. Open lists never
/// reset (see [`ListQuery`]).
pub(crate) const CLOSED_RESET_MS: Ms = 2_000;

/// The query of a list. Call it from `render`.
pub(crate) fn use_list_query() -> ListQuery {
    ListQuery {
        ahead: use_state(TypeAhead::default),
        epoch: use_state(|| 0),
        direction: use_direction(),
    }
}

impl ListQuery {
    /// What the query makes of `event`, for a list of `items` whose row
    /// `focused` has the keyboard. `open` says whether the list is showing;
    /// a shut one forgets its query after [`CLOSED_RESET_MS`] of quiet, an
    /// open one never does.
    pub(crate) fn key(
        self,
        event: &Event<KeyboardEventData>,
        items: &[Candidate],
        focused: Option<usize>,
        open: bool,
    ) -> Reaction {
        let Some(key) = nav_key(event, self.direction) else {
            return Reaction::Ignored;
        };
        // Escape is the panel's, which hears it anywhere in the window.
        let Some(key) = key.query().filter(|key| *key != menu_query::Key::Escape) else {
            return Reaction::Ignored;
        };
        let mut state = self.ahead;
        let Some(mut ahead) = state.try_write() else {
            return Reaction::Ignored;
        };
        let reaction = ahead.key(key, items, focused);
        let waiting = !open && ahead.is_active();
        drop(ahead);
        let epoch = self.next_epoch();
        if waiting {
            wake_after(CLOSED_RESET_MS, move |_| self.reset_shut(epoch));
        }
        reaction
    }

    /// Ends any reset that is waiting, and returns the count for a new one.
    fn next_epoch(self) -> u64 {
        let mut epoch = self.epoch;
        let next = *epoch.peek() + 1;
        if let Some(mut count) = epoch.try_write() {
            *count = next;
        }
        next
    }

    /// The shut-list reset: runs only if nothing happened since it was set.
    fn reset_shut(self, epoch: u64) {
        if *self.epoch.peek() != epoch {
            return;
        }
        let mut state = self.ahead;
        if let Some(mut ahead) = state.try_write() {
            ahead.clear();
        }
    }

    /// How row `index` shows. Reading it in `render` subscribes the list.
    pub(crate) fn mark(self, index: usize) -> Mark {
        self.ahead.read().mark(index)
    }

    /// The pill to give the panel, or `None` without a query.
    pub(crate) fn pill(self) -> Option<Pill> {
        self.ahead.read().pill()
    }

    /// Forgets the query, and any reset waiting on it: when the list opens or
    /// closes, or a row is picked.
    pub(crate) fn clear(self) {
        self.next_epoch();
        let mut state = self.ahead;
        if state.peek().is_active()
            && let Some(mut ahead) = state.try_write()
        {
            ahead.clear();
        }
    }

    /// Escape, which the panel hears anywhere in the window (see
    /// `Panel::on_escape`): it takes back a typed query. It returns whether
    /// there was one; when not, the panel closes.
    pub(crate) fn escape(self) -> bool {
        self.next_epoch();
        let mut state = self.ahead;
        match state.try_write() {
            Some(mut ahead) => ahead.clear(),
            None => false,
        }
    }
}

/// The opacity of a row for its mark: dimmed when a query is active and the
/// row does not match, otherwise `None` (the row keeps its own).
pub(crate) fn dimmed(mark: &Mark) -> Option<f32> {
    (*mark == Mark::Dim).then_some(DIMMED_OPACITY)
}

/// A one-line label for a list row, filling the space it is given. A match
/// has its letters marked. The text starts at the row's start edge, which a
/// right-to-left interface has on the right.
pub(crate) fn marked_label(tokens: &Tokens, ink: Ink, text: &str, mark: &Mark) -> Element {
    match mark {
        Mark::Hit(range) => {
            search_highlight::highlighted_label(BODY.in_ink(ink), tokens, text, &[range.clone()])
                .width(Size::flex(1.0))
                .into()
        }
        Mark::Plain | Mark::Dim => BODY
            .in_ink(ink)
            .label(tokens, text)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Size::flex(1.0))
            .into(),
    }
}
