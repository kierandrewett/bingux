//! The detail card of the row the pointer rests on: name, type and size, date.
//!
//! The card belongs to the view, not to a row. A row comes and goes as the list
//! scrolls (only the rows in view exist), and a card tied to one would go with
//! it; so rows only *report* where the pointer is, and the view's [`HoverFeed`]
//! decides what the card shows.
//!
//! - **One decision, [`Track`], pure.** It knows the row under the pointer, the
//!   row the card is up for, when the pointer began to rest and whether the
//!   window is scrolling, and answers from those and a clock handed in. Its
//!   rules, which `tests` below run without a window:
//!   - *Scrolling hides the card at once and keeps it down.* Rows move under a
//!     resting pointer, so a card that opened for one is wrong a frame later.
//!     It comes back [`CARD_DELAY`] after the content last moved (the 200 ms of
//!     stillness the window's scroll signal needs, and the rest of the delay
//!     here). The row it is for is then looked up again from the geometry (the
//!     lattice, the viewport, the scroll position): Freya sends a pointer move
//!     after the layout of a wheel notch, but none during the glide that
//!     follows, so the last row an event named is from the middle of a scroll.
//!   - *Moving to another row while the card is up swaps it in place*: the same
//!     card, the same element, new text, at the new place. No wait, no remount.
//!   - *Leaving every row hides it*, a frame later, so that leaving one row and
//!     entering the next in the same frame is a swap and not a hide.
//! - **A write a frame.** Rows call [`HoverFeed::enter`] and [`HoverFeed::leave`]
//!   for every row the content moves under the pointer. Those only update
//!   [`Track`]; the one State the card reads is written by a task that runs at
//!   most once a frame, and only when the card it would draw is a different
//!   one. A call for the row the pointer is already on does nothing, and none
//!   starts a timer while the window scrolls.
//! - **No disk, no waiting.** The card is made from the entry the listing
//!   already holds. A size and a date that the listing does not have yet (a
//!   remote folder is statted in the background, as the rows do) read as an
//!   ellipsis in a line of the same height, and the card re-draws when the
//!   listing arrives, so it never changes shape.

use std::time::{Duration, Instant, SystemTime};

use bingux_files::{Entry, display};
use freya::prelude::*;

use super::columns::{Column, DateStyle, cell_text_styled};
use super::view::{Rows, Shared};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// The pointer rests this long on a row before its detail card shows.
pub(super) const CARD_DELAY: Duration = Duration::from_millis(700);
/// How long the card waits to write what it shows: one frame, so a leave and
/// the enter that follows it in the same frame are one swap.
const FRAME: Duration = Duration::from_millis(16);
/// What the window's scroll signal has already waited when it clears
/// (`SCROLL_STILL`): the card comes back [`CARD_DELAY`] after the content last
/// moved, not after the signal cleared.
const STILL: Duration = Duration::from_millis(200);

/// The row the pointer rests on, and where it entered it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Hover {
    pub row: usize,
    pub at: (f32, f32),
}

/// What the card does, as a function of what the pointer and the scroll did.
/// Holds no State and reads no clock: see the module documentation.
#[derive(Debug, Default)]
pub(super) struct Track {
    /// The row under the pointer.
    target: Option<Hover>,
    /// The row the card is up for.
    shown: Option<Hover>,
    /// When the pointer began to rest on `target`.
    since: Option<Instant>,
    /// Where the pointer was last seen, in the window.
    pointer: Option<(f32, f32)>,
    scrolling: bool,
}

impl Track {
    /// The pointer is on `hover.row`. Returns whether that is another row than
    /// the one it was on: the same row again changes nothing.
    pub fn enter(&mut self, hover: Hover, now: Instant) -> bool {
        self.pointer = Some(hover.at);
        if self.target.is_some_and(|target| target.row == hover.row) {
            return false;
        }
        self.target = Some(hover);
        self.since = Some(now);
        true
    }

    /// The pointer left `row`. Returns whether it was the row it was on.
    pub fn leave(&mut self, row: usize) -> bool {
        if self.target.is_none_or(|target| target.row != row) {
            return false;
        }
        self.target = None;
        self.since = None;
        true
    }

    /// The pointer is at `at`.
    pub fn point(&mut self, at: (f32, f32)) {
        self.pointer = Some(at);
    }

    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.pointer
    }

    /// The rows are gone or the view is busy: nothing is hovered, nothing shown.
    pub fn clear(&mut self) {
        self.target = None;
        self.since = None;
        self.shown = None;
    }

    pub fn is_scrolling(&self) -> bool {
        self.scrolling
    }

    /// The window began to scroll: the card goes at once, and stays down.
    pub fn begin_scroll(&mut self) {
        self.scrolling = true;
        self.shown = None;
    }

    /// The window stopped scrolling, with the pointer on `row` (`None`: on no
    /// row). That row is what the pointer rests on now, whatever it was on
    /// before, and the wait is what is left of [`CARD_DELAY`] after the scroll
    /// signal's own stillness.
    pub fn end_scroll(&mut self, row: Option<usize>, now: Instant) {
        self.scrolling = false;
        self.shown = None;
        let at = self.pointer.unwrap_or_default();
        self.target = row.map(|row| Hover { row, at });
        self.since = self.target.map(|_| now.checked_sub(STILL).unwrap_or(now));
    }

    /// What the card shows now, the decision the others only prepare.
    pub fn commit(&mut self, now: Instant) -> Option<Hover> {
        self.shown = match (self.scrolling, self.target, self.shown) {
            (true, ..) | (_, None, _) => None,
            // Up already: the card follows the pointer to the new row.
            (_, Some(target), Some(_)) => Some(target),
            (_, Some(target), None) => self
                .since
                .filter(|since| now.saturating_duration_since(*since) >= CARD_DELAY)
                .map(|_| target),
        };
        self.shown
    }

    /// When [`Track::commit`] next has something to decide, from `now`. `None`
    /// while it has nothing: nothing under the pointer and nothing shown, the
    /// card up for the row the pointer is on, or content moving.
    pub fn wake(&self, now: Instant) -> Option<Duration> {
        if self.scrolling {
            return None;
        }
        match (self.target, self.shown) {
            (None, None) => None,
            (None, Some(_)) => Some(FRAME),
            (Some(target), Some(shown)) => (target != shown).then_some(FRAME),
            (Some(_), None) => self
                .since
                .map(|since| CARD_DELAY.saturating_sub(now.saturating_duration_since(since))),
        }
    }
}

/// What the view's rows and its scroll tell the card, and the one State the
/// card reads. `Copy`, like the rest of the view's gestures.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct HoverFeed {
    track: State<Track>,
    /// What the card draws. Written by [`HoverFeed::fired`] only.
    shown: State<Option<Hover>>,
    /// The task that will commit, and when.
    pending: State<Option<(Instant, TaskHandle)>>,
    /// The view's scope: the task lives as long as the view, not as long as
    /// the row whose handler started it.
    scope: ScopeId,
    scroll: Option<ScrollActivity>,
}

/// The view's feed. A hook.
pub(super) fn use_hover_feed() -> HoverFeed {
    HoverFeed {
        track: use_state(Track::default),
        shown: use_state(|| None),
        pending: use_state(|| None),
        scope: use_hook(current_scope_id),
        scroll: scroll_activity(),
    }
}

impl HoverFeed {
    /// The pointer entered `hover.row`.
    pub fn enter(mut self, hover: Hover) {
        let now = Instant::now();
        if self.track.write().enter(hover, now) && !self.scrolling_now() {
            self.settle(now);
        }
    }

    /// The pointer left `row`.
    pub fn leave(mut self, row: usize) {
        let now = Instant::now();
        if self.track.write().leave(row) && !self.scrolling_now() {
            self.settle(now);
        }
    }

    /// The pointer is at a window point. Nothing draws for it: it is what the
    /// end of a scroll looks for a row at.
    pub fn pointer(mut self, at: (f32, f32)) {
        if self.track.peek().pointer() != Some(at) {
            self.track.write().point(at);
        }
    }

    /// Nothing is hovered any more (the rows changed).
    pub fn clear(mut self) {
        self.track.write().clear();
        self.shown.set_if_modified(None);
    }

    /// The window's scroll signal changed. A scroll hides the card at once.
    /// When it ends the rows have moved under a pointer that gave no event for
    /// most of that (a glide has no pointer move; the one after the last
    /// notch is from the middle of it), so the row at the pointer is looked
    /// up again from the geometry by `row_at`, and the card waits for it.
    pub fn refresh(mut self, row_at: impl FnOnce((f32, f32)) -> Option<usize>) {
        let now = Instant::now();
        let scrolling = self.scrolling_now();
        let (was, pointer) = {
            let track = self.track.peek();
            (track.is_scrolling(), track.pointer())
        };
        if scrolling && !was {
            self.track.write().begin_scroll();
            self.shown.set_if_modified(None);
        } else if !scrolling && was {
            let row = pointer.and_then(row_at);
            self.track.write().end_scroll(row, now);
            self.settle(now);
        }
    }

    /// Whether the window is scrolling. Reading it subscribes the reader.
    pub fn scrolling(&self) -> bool {
        self.scroll.is_some_and(|scroll| scroll.scrolling())
    }

    /// Whether the window is scrolling, without subscribing.
    fn scrolling_now(&self) -> bool {
        self.scroll.is_some_and(|scroll| scroll.is_scrolling())
    }

    /// What the card draws. Reading it subscribes the reader.
    pub fn shown(&self) -> Option<Hover> {
        *self.shown.read()
    }

    /// Makes sure a commit is coming when [`Track::wake`] says one is due.
    /// A commit that is already on its way and no later is left alone, so a
    /// stream of events moves no commit later and starts no second task.
    fn settle(mut self, now: Instant) {
        let Some(wake) = self.track.peek().wake(now) else {
            return;
        };
        let due = now + wake;
        if self
            .pending
            .peek()
            .as_ref()
            .is_some_and(|(pending, _)| *pending <= due)
        {
            return;
        }
        if let Some((_, task)) = self.pending.take() {
            task.cancel();
        }
        let task = spawn_in_scope(
            async move {
                timer(wake).await;
                self.fired();
            },
            self.scope,
        );
        self.pending.set(Some((due, task)));
    }

    /// The commit's time: the only place the card's State is written.
    fn fired(mut self) {
        // The task is the one that is running: let it go without cancelling.
        let _ = self.pending.take();
        let now = Instant::now();
        let shown = self.track.write().commit(now);
        self.shown.set_if_modified(shown);
        self.settle(now);
    }
}

/// The lines of the card for `entry`. `modified` is the date as the Modified
/// column writes it, if the entry has one. `place` is a line of its own where
/// the entry is (Recent, search results); `reserve_place` keeps that line for
/// the entries of the view that have none, so that no card is shorter than
/// another.
///
/// An entry the listing has not statted yet has no size and no date: the type
/// line stands without a size, and the date reads as an ellipsis, in the same
/// number of lines as the finished card.
pub(super) fn card_lines(
    entry: &Entry,
    modified: Option<&str>,
    place: Option<String>,
    reserve_place: bool,
    link: Option<&std::path::Path>,
) -> Vec<String> {
    let mut kind = if entry.is_dir() || !entry.sized {
        entry.mime.description.to_string()
    } else {
        format!("{} · {}", entry.mime.description, display::size(entry.size))
    };
    if let Some(target) = link {
        kind.push_str(&format!(" · Link to {}", target.display()));
    }
    let date = match (modified, entry.sized) {
        (Some(modified), _) => format!("Modified {modified}"),
        (None, false) => "Modified …".to_string(),
        (None, true) => "Modified unknown".to_string(),
    };
    let mut lines = vec![entry.name.clone(), kind, date];
    match place {
        Some(place) => lines.push(place),
        // A no-break space: an empty label has no height.
        None if reserve_place => lines.push("\u{a0}".to_string()),
        None => {}
    }
    lines
}

/// How often the view and its list have rendered on this thread, for the
/// tests that check a scroll with a card armed draws neither again.
#[cfg(test)]
pub(super) mod renders {
    use std::cell::Cell;

    thread_local! {
        static VIEW: Cell<usize> = const { Cell::new(0) };
        static LIST: Cell<usize> = const { Cell::new(0) };
    }

    pub fn view() {
        VIEW.with(|count| count.set(count.get() + 1));
    }

    pub fn list() {
        LIST.with(|count| count.set(count.get() + 1));
    }

    /// The renders of the view and of the list so far.
    pub fn counts() -> (usize, usize) {
        (VIEW.with(Cell::get), LIST.with(Cell::get))
    }
}

/// The card, drawn over the view at the point the pointer entered the row.
#[derive(Clone, PartialEq)]
pub(super) struct HoverCard {
    pub shared: Shared,
    /// The listing: the card draws again when it changes, as a row does.
    pub rows: Rows,
    pub origin: State<Area>,
}

impl Component for HoverCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.shared.get();
        let feed = ctx.gestures.hover;
        // The scroll signal changes a frame before the pointer says anything,
        // and the end of a scroll is told by nothing else. The refresh reads
        // the scroll position, so it runs in a task: an effect that reads it
        // would run again for every notch.
        let scrolling = feed.scrolling();
        let shared = self.shared.clone();
        use_side_effect_with_deps(&scrolling, move |_| {
            let shared = shared.clone();
            spawn(async move {
                let ctx = shared.get();
                feed.refresh(|at| ctx.row_at(at));
            });
        });
        // The card is made once and keeps its size between shows: a new row
        // changes its text and place, and nothing else.
        let mut size = use_state(|| None::<(f32, f32)>);
        let busy = ctx.gestures.drag.read().is_some()
            || ctx.gestures.band.read().is_some()
            || ctx.env.ui.rename.read().is_some()
            // A preview or a dialog is over the view; the pointer rests under it.
            || ctx.env.overlays.dialog_open()
            // A menu holds the pointer; the card behind it is stale.
            || ctx.env.overlays.menu_open();
        let Some(hover) = feed.shown().filter(|_| !busy && !scrolling) else {
            return rect().interactive(false).into_element();
        };
        if hover.row >= self.rows.len() {
            return rect().interactive(false).into_element();
        }
        let entry = self.rows.entry(hover.row);
        let modified = entry.modified.map(|_| {
            cell_text_styled(
                Column::Modified,
                entry,
                SystemTime::now(),
                &ctx.home,
                DateStyle::Absolute,
            )
        });
        let place = if ctx.recent {
            entry
                .path
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
        } else {
            None
        };
        let reserve_place = ctx.recent;
        // Where a link leads is what the link itself says: `readlink` does
        // not touch the target, so a stalled mount cannot hold the card.
        let link = entry
            .symlink
            .then(|| std::fs::read_link(&entry.path).ok())
            .flatten();
        let lines = card_lines(
            entry,
            modified.as_deref(),
            place,
            reserve_place,
            link.as_deref(),
        );

        let origin = *self.origin.read();
        let (width, height) = (*size.read()).unwrap_or((0.0, 0.0));
        let measured = width > 0.0;
        // Below and right of the pointer; flipped when it would leave the view.
        let mut left = hover.at.0 - origin.min_x() + S4.px();
        let mut top = hover.at.1 - origin.min_y() + S5.px();
        if left + width > origin.width() {
            left = (hover.at.0 - origin.min_x() - width - S2.px()).max(0.0);
        }
        if top + height > origin.height() {
            top = (hover.at.1 - origin.min_y() - height - S2.px()).max(0.0);
        }
        let mut card = rect()
            .interactive(false)
            .layer(crate::layers::HOVER_CARD)
            .position(Position::new_absolute().left(left).top(top))
            .opacity(if measured { 1.0 } else { 0.0 })
            .on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified(Some((event.area.width(), event.area.height())));
            })
            .tw(&tokens, TOOLTIP)
            .tw(
                &tokens,
                &[
                    flex_col(),
                    border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
                ],
            );
        for (index, line) in lines.into_iter().enumerate() {
            let style = if index == 0 { BODY } else { CAPTION }.in_ink(Ink::OnFloating);
            card = card.child(style.label(&tokens, line).max_lines(1));
        }
        card.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(row: usize) -> Hover {
        Hover {
            row,
            at: (10.0 * row as f32, 5.0),
        }
    }

    fn after(from: Instant, ms: u64) -> Instant {
        from + Duration::from_millis(ms)
    }

    #[test]
    fn the_card_waits_for_the_pointer_to_rest() {
        let t0 = Instant::now();
        let mut track = Track::default();
        assert!(track.enter(at(3), t0));
        assert_eq!(track.commit(after(t0, 100)), None, "not before the delay");
        assert_eq!(track.wake(after(t0, 100)), Some(Duration::from_millis(600)));
        assert_eq!(track.commit(after(t0, 700)), Some(at(3)));
        assert_eq!(track.wake(after(t0, 700)), None, "nothing left to decide");
    }

    #[test]
    fn the_same_row_again_changes_nothing() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        let moved = Hover {
            row: 3,
            at: (99.0, 9.0),
        };
        assert!(
            !track.enter(moved, after(t0, 300)),
            "no change for the row it is on"
        );
        assert_eq!(
            track.commit(after(t0, 700)),
            Some(at(3)),
            "the delay was not restarted"
        );
    }

    #[test]
    fn a_card_that_is_up_swaps_to_the_next_row_in_place() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        assert_eq!(track.commit(after(t0, 700)), Some(at(3)));
        // Leave one row and enter the next in the same frame: one swap, no gap.
        track.leave(3);
        track.enter(at(4), after(t0, 800));
        assert_eq!(track.wake(after(t0, 800)), Some(FRAME));
        assert_eq!(
            track.commit(after(t0, 816)),
            Some(at(4)),
            "the new row at once, with no wait"
        );
        assert_eq!(track.wake(after(t0, 816)), None);
    }

    #[test]
    fn leaving_every_row_hides_the_card_a_frame_later() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        track.commit(after(t0, 700));
        assert!(track.leave(3));
        assert_eq!(track.wake(after(t0, 710)), Some(FRAME));
        assert_eq!(track.commit(after(t0, 726)), None);
        assert!(
            !track.leave(3),
            "a row that was not hovered is not a change"
        );
    }

    #[test]
    fn a_stale_leave_does_not_hide_the_row_the_pointer_is_on() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        track.commit(after(t0, 700));
        // The next row is entered first, then the old one reports its leave.
        track.enter(at(4), after(t0, 800));
        assert!(!track.leave(3));
        assert_eq!(track.commit(after(t0, 816)), Some(at(4)));
    }

    #[test]
    fn a_scroll_hides_the_card_and_keeps_it_down() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        assert_eq!(track.commit(after(t0, 700)), Some(at(3)));

        track.begin_scroll();
        assert_eq!(
            track.commit(after(t0, 800)),
            None,
            "hidden the moment the scroll starts"
        );
        // Rows go by under the pointer, a new row every frame, for a second.
        for frame in 0..60_u64 {
            let now = after(t0, 800 + 16 * frame);
            track.leave(frame as usize + 3);
            track.enter(at(frame as usize + 4), now);
            assert_eq!(
                track.commit(now),
                None,
                "frame {frame}: no card while it scrolls"
            );
            assert_eq!(
                track.wake(now),
                None,
                "frame {frame}: and no commit to wait for"
            );
        }
    }

    #[test]
    fn after_a_scroll_the_card_returns_for_the_row_under_the_pointer_once_still() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        track.commit(after(t0, 700));
        track.begin_scroll();
        track.leave(3);
        track.enter(at(9), after(t0, 900));
        // The scroll signal clears 200 ms after the last move, with the pointer
        // on a row that the geometry says is row 12: a glide gave no event.
        let still = after(t0, 1100);
        track.end_scroll(Some(12), still);
        assert_eq!(
            track.commit(still),
            None,
            "not at the instant the scroll ends"
        );
        assert_eq!(
            track.wake(still),
            Some(CARD_DELAY - STILL),
            "700 ms after the content last moved, 200 of them already waited"
        );
        assert_eq!(track.commit(after(t0, 1100 + 400)), None);
        let shown = track
            .commit(after(t0, 1100 + 500))
            .expect("the card is back");
        assert_eq!(
            shown.row, 12,
            "the row the pointer is on now, not the one an event named"
        );
        assert_eq!(
            shown.at,
            at(9).at,
            "at the pointer, where the last event saw it"
        );
    }

    #[test]
    fn a_scroll_with_no_row_under_the_pointer_brings_no_card() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        track.commit(after(t0, 700));
        track.begin_scroll();
        track.leave(3);
        track.end_scroll(None, after(t0, 1000));
        assert_eq!(track.commit(after(t0, 5000)), None);
        assert_eq!(track.wake(after(t0, 5000)), None);
    }

    #[test]
    fn clearing_forgets_the_row_and_the_card() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(3), t0);
        track.commit(after(t0, 700));
        track.clear();
        assert_eq!(track.commit(after(t0, 800)), None);
        assert!(
            track.enter(at(3), after(t0, 900)),
            "the same row is a new hover after a clear"
        );
    }

    #[test]
    fn a_hundred_row_changes_in_one_frame_cost_one_decision() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.enter(at(0), t0);
        track.commit(after(t0, 700));
        // A fast sweep: the pointer crosses many rows before the frame ends.
        let mut changes = 0;
        for row in 1..=100 {
            changes += usize::from(track.leave(row - 1));
            changes += usize::from(track.enter(at(row), after(t0, 710)));
        }
        assert_eq!(changes, 200, "every change is seen by the track");
        // ...but one commit, one frame later, shows only where it ended.
        assert_eq!(track.wake(after(t0, 710)), Some(FRAME));
        assert_eq!(track.commit(after(t0, 726)), Some(at(100)));
        assert_eq!(track.wake(after(t0, 726)), None);
    }

    fn entry(name: &str, size: Option<u64>) -> Entry {
        let mut entry = Entry::named(
            std::path::PathBuf::from("/x").join(name),
            name.to_string(),
            bingux_files::Kind::File,
            false,
            false,
        );
        if let Some(size) = size {
            entry.size = size;
            entry.sized = true;
        }
        entry
    }

    #[test]
    fn the_card_has_the_same_lines_before_and_after_the_listing_has_the_size() {
        let before = card_lines(&entry("a.txt", None), None, None, false, None);
        let after = card_lines(
            &entry("a.txt", Some(2)),
            Some("15 Nov 2023"),
            None,
            false,
            None,
        );
        assert_eq!(before.len(), after.len(), "{before:?} against {after:?}");
        assert_eq!(before[2], "Modified …");
        assert_eq!(after[1], "Text document · 2 bytes");
        assert_eq!(after[2], "Modified 15 Nov 2023");
    }

    #[test]
    fn a_link_says_where_it_leads_in_its_type_line() {
        let lines = card_lines(
            &entry("a.txt", Some(2)),
            Some("1 Jan"),
            None,
            false,
            Some(std::path::Path::new("/mnt/x/a.txt")),
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "Text document · 2 bytes · Link to /mnt/x/a.txt");
    }

    #[test]
    fn a_view_with_places_keeps_a_line_for_each_card() {
        let with = card_lines(
            &entry("a.txt", Some(2)),
            Some("1 Jan"),
            Some("In sub".into()),
            true,
            None,
        );
        let without = card_lines(&entry("b.txt", Some(2)), Some("1 Jan"), None, true, None);
        assert_eq!(with.len(), 4);
        assert_eq!(
            without.len(),
            4,
            "the line is kept, blank, so the cards are alike"
        );
        assert_eq!(
            card_lines(&entry("c.txt", Some(2)), Some("1 Jan"), None, false, None).len(),
            3
        );
    }
}
