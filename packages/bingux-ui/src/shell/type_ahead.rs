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
//!    [`ListQuery::key`]. On `Focus` highlight the row and scroll it into
//!    view, on `Activate` pick it, on `Handled` stop the event, and on
//!    `Ignored` carry on with the list's own keys.
//! 3. Draw each row's label with [`marked_label`] from [`ListQuery::mark`], and
//!    dim it by [`dimmed`].
//! 4. Give the panel [`ListQuery::pill`] with `Panel::pill`.
//!
//! The menu keeps its query inside its own state ([`super::menu_nav`]) and
//! uses the same row and pill helpers.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::menu_nav::NavKey;
use super::menu_query::{self, Candidate, Mark, Ms, Pill, Reaction, TypeAhead};
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
/// letter with Ctrl, Alt or the logo key held is a shortcut for someone else.
pub(crate) fn nav_key(event: &Event<KeyboardEventData>) -> Option<NavKey> {
    let modifiers = event.modifiers;
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let other = modifiers.contains(Modifiers::ALT) || modifiers.contains(Modifiers::META);
    match &event.key {
        Key::Named(named) => {
            if ctrl || other {
                return None;
            }
            Some(match named {
                NamedKey::Escape => NavKey::Escape,
                NamedKey::ArrowLeft => NavKey::Left,
                NamedKey::ArrowRight => NavKey::Right,
                NamedKey::ArrowDown => NavKey::Down,
                NamedKey::ArrowUp => NavKey::Up,
                NamedKey::Home => NavKey::Home,
                NamedKey::End => NavKey::End,
                NamedKey::Backspace => NavKey::Backspace,
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
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ListQuery(State<TypeAhead>);

/// The query of a list. Call it from `render`.
pub(crate) fn use_list_query() -> ListQuery {
    ListQuery(use_state(TypeAhead::default))
}

impl ListQuery {
    /// What the query makes of `event`, for a list of `items` whose row
    /// `focused` has the keyboard. It also starts the timer that ends the
    /// query, so a stale one goes away without another key.
    pub(crate) fn key(
        self,
        event: &Event<KeyboardEventData>,
        items: &[Candidate],
        focused: Option<usize>,
    ) -> Reaction {
        let Some(key) = nav_key(event) else {
            return Reaction::Ignored;
        };
        // Escape is the panel's, which hears it anywhere in the window.
        let Some(key) = key.query().filter(|key| *key != menu_query::Key::Escape) else {
            return Reaction::Ignored;
        };
        let mut state = self.0;
        let Some(mut ahead) = state.try_write() else {
            return Reaction::Ignored;
        };
        let reaction = ahead.key(key, now_ms(), items, focused);
        let ends = ahead.expires_at();
        drop(ahead);
        if let Some(end) = ends {
            let after = end.saturating_sub(now_ms()) + 20;
            wake_after(after, move |now| self.expire(now));
        }
        reaction
    }

    fn expire(self, now: Ms) {
        let mut state = self.0;
        let Some(mut ahead) = state.try_write() else {
            return;
        };
        ahead.expire(now);
    }

    /// How row `index` shows. Reading it in `render` subscribes the list.
    pub(crate) fn mark(self, index: usize) -> Mark {
        self.0.read().mark(index)
    }

    /// The pill to give the panel, or `None` without a query.
    pub(crate) fn pill(self) -> Option<Pill> {
        self.0.read().pill()
    }

    /// Forgets the query: when the list closes.
    pub(crate) fn clear(self) {
        let mut state = self.0;
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
        let mut state = self.0;
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
/// has its letters marked.
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
