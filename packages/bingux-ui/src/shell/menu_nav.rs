//! The state machine of an open menu: which row is highlighted in each panel,
//! which submenus are open, and how the pointer and the keyboard move that.
//! It reads no clock and draws nothing: callers pass the time of each event,
//! so the open delay, the safe triangle and the type-ahead timeout are tested
//! without a window. [`super::menu`] draws it and schedules the wake-ups.
//!
//! # Submenus and the pointer
//!
//! - A row with a submenu opens it after [`SUBMENU_OPEN_DELAY_MS`] of the
//!   pointer resting on it, and at once when a sibling's submenu is already
//!   open, so moving down a list of submenu rows never waits.
//! - Moving to another row of the same panel closes the open submenu.
//! - A pointer leaving the open row toward its submenu starts the safe
//!   triangle: the corner of the pointer and the submenu's near edge. While
//!   the pointer stays in it, entering a sibling row does nothing, so a
//!   diagonal move reaches the submenu. Leaving the triangle closes the
//!   submenu; resting in it for [`CONE_REST_MS`] ends it, and the row under
//!   the pointer then takes effect as if it had just been entered.
//! - A disabled row takes no highlight and opens nothing, but it still
//!   counts as a move to a sibling, so it is never a dead zone.
//!
//! # Keys
//!
//! Keys act on the active panel. Down, Up, Home and End move the highlight
//! over the rows that can be picked. Right, Enter and Space open a submenu;
//! Left and Escape close it; Escape and Tab at the top level close the menu.
//! Typing builds a [`TypeAhead`] query, which takes Down, Up, Tab, Shift+Tab,
//! Enter, Backspace, Space and Escape while it is active (see
//! [`super::menu_query`]).

use super::menu_query::{self, Candidate, Ms, Reaction, TypeAhead};
use super::panel::PanelRect;

/// How long a pointer must rest on a row with a submenu before it opens.
pub(crate) const SUBMENU_OPEN_DELAY_MS: Ms = 150;

/// How long a pointer may rest inside the safe triangle before the triangle
/// is dropped, so the pointer can then pick another row of the parent menu.
pub(crate) const CONE_REST_MS: Ms = 300;

/// A row of a menu, as the navigation sees it.
pub(crate) trait Node: Sized {
    fn label(&self) -> &str;
    /// Whether the row can be highlighted and picked: enabled, and not a
    /// separator.
    fn selectable(&self) -> bool;
    /// The rows of the row's submenu.
    fn children(&self) -> Option<&[Self]>;
}

/// What a row does when the pointer enters it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Plain,
    /// A disabled row: no highlight and no submenu.
    Disabled,
    Submenu,
}

/// A key the menu understands, after the caller read modifiers and layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavKey {
    Escape,
    Left,
    Right,
    Down,
    Up,
    Home,
    End,
    Tab,
    ShiftTab,
    CtrlN,
    CtrlP,
    Enter,
    Space,
    Backspace,
    /// A printable character other than Space.
    Char(char),
}

impl NavKey {
    /// The key as the type-ahead query reads it.
    pub(crate) fn query(self) -> Option<menu_query::Key> {
        use menu_query::Key;
        Some(match self {
            NavKey::Char(c) => Key::Char(c),
            NavKey::Space => Key::Char(' '),
            NavKey::Backspace => Key::Backspace,
            NavKey::Down | NavKey::Tab | NavKey::CtrlN => Key::Next,
            NavKey::Up | NavKey::ShiftTab | NavKey::CtrlP => Key::Previous,
            NavKey::Enter => Key::Enter,
            NavKey::Escape => Key::Escape,
            NavKey::Left | NavKey::Right | NavKey::Home | NavKey::End => return None,
        })
    }
}

/// What the menu does after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The key was not for the menu: leave it to others.
    Ignored,
    /// The menu took the key: stop it here.
    Handled,
    /// Close the menu, and stop the key.
    Dismiss,
    /// Close the menu and let the key go on, so focus moves on from it.
    Leave,
    /// Close the menu and run the action of row `index` of panel `level`.
    Activate { level: usize, index: usize },
}

/// The safe triangle of an open row. A pointer that leaves the row toward its
/// submenu stays inside the triangle from `origin` to the submenu's near edge,
/// so the submenu stays open while the pointer travels to it.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Cone {
    /// The panel holding the open row.
    level: usize,
    origin: (f32, f32),
    sub: PanelRect,
    /// When the pointer last moved inside the triangle, for the rest check.
    last_move: Ms,
}

impl Cone {
    fn contains(self, point: (f32, f32)) -> bool {
        // The submenu opens to one side of the row; its near edge is the one
        // facing the pointer, so a flipped menu works the same way.
        let near = if self.sub.x >= self.origin.0 {
            self.sub.x
        } else {
            self.sub.x + self.sub.width
        };
        let a = self.origin;
        let b = (near, self.sub.y);
        let c = (near, self.sub.y + self.sub.height);
        let d1 = side(point, a, b);
        let d2 = side(point, b, c);
        let d3 = side(point, c, a);
        let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(negative && positive)
    }
}

/// The sign of the cross product of `point` against the edge `a` to `b`.
fn side(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (point.0 - b.0) * (a.1 - b.1) - (a.0 - b.0) * (point.1 - b.1)
}

/// A submenu waiting for the pointer to rest on its row.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Pending {
    level: usize,
    index: usize,
    due: Ms,
}

/// A row the pointer entered while the safe triangle held it back.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Held {
    level: usize,
    index: usize,
    kind: RowKind,
    point: (f32, f32),
}

/// The keyboard and pointer state of an open menu.
///
/// `open[level]` is the row of panel `level` whose submenu is open, so panel
/// `level + 1` exists for each entry. `highlight[level]` is the highlighted row
/// of panel `level`, and `active` is the panel that keys act on.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct NavState {
    open: Vec<usize>,
    highlight: Vec<Option<usize>>,
    active: usize,
    cone: Option<Cone>,
    pending: Option<Pending>,
    held: Option<Held>,
    ahead: TypeAhead,
}

/// The rows shown in panel `level`: the root rows, then each open submenu.
pub(crate) fn panel_items<'a, T: Node>(root: &'a [T], open: &[usize], level: usize) -> &'a [T] {
    let mut items = root;
    for &index in open.iter().take(level) {
        match items.get(index).and_then(Node::children) {
            Some(sub) => items = sub,
            None => return &[],
        }
    }
    items
}

pub(crate) fn first_selectable<T: Node>(items: &[T]) -> Option<usize> {
    items.iter().position(Node::selectable)
}

fn last_selectable<T: Node>(items: &[T]) -> Option<usize> {
    items.iter().rposition(Node::selectable)
}

/// The next selectable row after `from` in `forward` order, wrapping around.
fn step_selectable<T: Node>(items: &[T], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = items.len();
    let mut current = from;
    for _ in 0..count {
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        if items[next].selectable() {
            return Some(next);
        }
        current = Some(next);
    }
    None
}

fn candidates<T: Node>(items: &[T]) -> Vec<Candidate<'_>> {
    items
        .iter()
        .map(|item| Candidate {
            label: item.label(),
            selectable: item.selectable(),
        })
        .collect()
}

impl NavState {
    /// The state of a new menu. From the keyboard, the first row is
    /// highlighted.
    pub(crate) fn new<T: Node>(items: &[T], keyboard: bool) -> Self {
        let mut nav = Self::default();
        nav.sync();
        if keyboard {
            nav.highlight[0] = first_selectable(items);
        }
        nav
    }

    /// The open submenus: the row of each panel whose submenu is open.
    pub(crate) fn open(&self) -> &[usize] {
        &self.open
    }

    /// The row of panel `level` whose submenu is open.
    pub(crate) fn open_row(&self, level: usize) -> Option<usize> {
        self.open.get(level).copied()
    }

    /// The highlighted row of panel `level`.
    pub(crate) fn highlighted(&self, level: usize) -> Option<usize> {
        self.highlight.get(level).copied().flatten()
    }

    /// The panel the keys act on.
    pub(crate) fn active(&self) -> usize {
        self.active
    }

    /// The type-ahead query of the active panel.
    pub(crate) fn ahead(&self) -> &TypeAhead {
        &self.ahead
    }

    /// Whether the pointer is crossing toward an open submenu, so its moves
    /// matter.
    pub(crate) fn crossing(&self) -> bool {
        self.cone.is_some()
    }

    /// Keeps one highlight slot per panel and the active panel in range.
    fn sync(&mut self) {
        self.highlight.resize(self.open.len() + 1, None);
        self.active = self.active.min(self.open.len());
    }

    /// Makes `level` the panel the keys act on. A query belongs to one panel,
    /// so a change of panel ends it.
    fn set_active(&mut self, level: usize) {
        if self.active != level {
            self.ahead.clear();
        }
        self.active = level;
    }

    /// Highlights row `index` of panel `level`. Moving off the row whose
    /// submenu is open closes that submenu.
    pub(crate) fn set_highlight(&mut self, level: usize, index: Option<usize>) {
        if self.open.get(level).copied() != index {
            self.open.truncate(level);
        }
        self.cone = None;
        self.pending = None;
        self.held = None;
        self.set_active(level);
        self.sync();
        self.highlight[level] = index;
    }

    /// Opens the submenu of row `index` of panel `level`. `first` is the row
    /// to highlight in the submenu; without one the submenu is only shown, and
    /// the parent panel stays active.
    pub(crate) fn open_sub(&mut self, level: usize, index: usize, first: Option<usize>) {
        self.open.truncate(level);
        self.open.push(index);
        self.cone = None;
        self.pending = None;
        self.held = None;
        self.set_active(if first.is_some() { level + 1 } else { level });
        self.sync();
        self.highlight[level] = Some(index);
        self.highlight[level + 1] = first;
    }

    /// Closes the deepest open submenu. Its parent row stays highlighted.
    /// It returns whether there was one.
    pub(crate) fn close_deepest(&mut self) -> bool {
        if self.open.pop().is_none() {
            return false;
        }
        self.cone = None;
        self.pending = None;
        self.held = None;
        let level = self.open.len();
        self.set_active(level);
        self.sync();
        true
    }

    // -----------------------------------------------------------------
    // The pointer

    /// A pointer entered row `index` of panel `level` at `point`.
    pub(crate) fn enter(
        &mut self,
        level: usize,
        index: usize,
        kind: RowKind,
        point: (f32, f32),
        now: Ms,
    ) {
        if let Some(cone) = self.cone
            && cone.level == level
            && cone.contains(point)
        {
            // Crossing toward the open submenu: keep it open, and remember
            // the row for when the crossing is over.
            self.held = Some(Held {
                level,
                index,
                kind,
                point,
            });
            return;
        }
        let sibling_open = self.open.len() > level;
        let already = self.open.get(level) == Some(&index);
        match kind {
            // The row takes no highlight, but the move still leaves the row
            // before it, and closes that row's submenu.
            RowKind::Disabled => self.set_highlight(level, None),
            RowKind::Plain => self.set_highlight(level, Some(index)),
            RowKind::Submenu => {
                self.set_highlight(level, Some(index));
                if already {
                    return;
                }
                if sibling_open {
                    self.open_sub(level, index, None);
                } else {
                    self.pending = Some(Pending {
                        level,
                        index,
                        due: now + SUBMENU_OPEN_DELAY_MS,
                    });
                }
            }
        }
    }

    /// A pointer left row `index` of panel `level` at `point`. `sub` is the
    /// rectangle of the row's submenu when it is open: the pointer is then
    /// probably crossing toward it, so the safe triangle starts.
    pub(crate) fn leave(
        &mut self,
        level: usize,
        index: usize,
        point: (f32, f32),
        sub: Option<PanelRect>,
        now: Ms,
    ) {
        if self
            .pending
            .is_some_and(|pending| (pending.level, pending.index) == (level, index))
        {
            self.pending = None;
        }
        if self
            .held
            .is_some_and(|held| (held.level, held.index) == (level, index))
        {
            self.held = None;
        }
        if let Some(sub) = sub {
            self.cone = Some(Cone {
                level,
                origin: point,
                sub,
                last_move: now,
            });
        }
    }

    /// The pointer moved anywhere over the menu. Inside the submenu the safe
    /// triangle ends; inside the triangle it waits for a rest; anywhere else
    /// the open submenu closes.
    pub(crate) fn moved(&mut self, point: (f32, f32), now: Ms) {
        let Some(cone) = self.cone else {
            return;
        };
        if cone.sub.contains(point) {
            self.cone = None;
            self.held = None;
        } else if cone.contains(point) {
            self.cone = Some(Cone {
                last_move: now,
                ..cone
            });
        } else {
            self.open.truncate(cone.level);
            self.cone = None;
            self.sync();
            self.highlight[cone.level] = None;
            self.replay_held(now);
        }
    }

    /// Applies the row that was held back by the safe triangle.
    fn replay_held(&mut self, now: Ms) {
        if let Some(held) = self.held.take() {
            self.enter(held.level, held.index, held.kind, held.point, now);
        }
    }

    /// The time the next scheduled change is due, so the caller can wake up
    /// then and call [`Self::poll`].
    pub(crate) fn wake_at(&self) -> Option<Ms> {
        [
            self.pending.map(|pending| pending.due),
            self.cone.map(|cone| cone.last_move + CONE_REST_MS),
            self.ahead.expires_at(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Runs what is due at `now`: a submenu waiting for its delay, a rest in
    /// the safe triangle, a query that ran out. It returns whether anything
    /// changed.
    pub(crate) fn poll(&mut self, now: Ms) -> bool {
        let mut changed = false;
        if let Some(pending) = self.pending
            && now >= pending.due
        {
            self.pending = None;
            // The pointer must still be on the row.
            if self.highlighted(pending.level) == Some(pending.index)
                && self.open.get(pending.level) != Some(&pending.index)
            {
                self.open_sub(pending.level, pending.index, None);
                changed = true;
            }
        }
        if let Some(cone) = self.cone
            && now >= cone.last_move + CONE_REST_MS
        {
            self.cone = None;
            self.replay_held(now);
            changed = true;
        }
        if self.ahead.expire(now) {
            changed = true;
        }
        changed
    }

    // -----------------------------------------------------------------
    // The keyboard

    /// A key was pressed at `now` while the menu was open. `root` is the
    /// menu's rows; the keys act on the active panel.
    pub(crate) fn press<T: Node>(&mut self, root: &[T], key: NavKey, now: Ms) -> Outcome {
        let level = self.active;
        let items = panel_items(root, &self.open, level);
        let focused = self.highlighted(level);

        // An active query takes its keys first; a first character starts one.
        if let Some(query_key) = key.query()
            && (self.ahead.is_active() || matches!(key, NavKey::Char(_)))
        {
            let rows = candidates(items);
            match self.ahead.key(query_key, now, &rows, focused) {
                Reaction::Ignored => {}
                Reaction::Handled => return Outcome::Handled,
                Reaction::Focus(index) => {
                    self.set_highlight(level, Some(index));
                    return Outcome::Handled;
                }
                Reaction::Activate(index) => return self.activate(items, level, index),
            }
        }

        match key {
            NavKey::Escape => {
                if self.close_deepest() {
                    Outcome::Handled
                } else {
                    Outcome::Dismiss
                }
            }
            NavKey::Left => {
                if level > 0 && self.close_deepest() {
                    Outcome::Handled
                } else {
                    Outcome::Ignored
                }
            }
            NavKey::Right => {
                let Some(index) = focused else {
                    return Outcome::Ignored;
                };
                let Some(item) = items.get(index) else {
                    return Outcome::Ignored;
                };
                match item.children() {
                    Some(sub) if item.selectable() => {
                        self.open_sub(level, index, first_selectable(sub));
                        Outcome::Handled
                    }
                    _ => Outcome::Ignored,
                }
            }
            NavKey::Down | NavKey::Up => {
                let forward = key == NavKey::Down;
                if let Some(next) = step_selectable(items, focused, forward) {
                    self.set_highlight(level, Some(next));
                }
                Outcome::Handled
            }
            NavKey::Home => {
                if let Some(first) = first_selectable(items) {
                    self.set_highlight(level, Some(first));
                }
                Outcome::Handled
            }
            NavKey::End => {
                if let Some(last) = last_selectable(items) {
                    self.set_highlight(level, Some(last));
                }
                Outcome::Handled
            }
            NavKey::Tab | NavKey::ShiftTab => Outcome::Leave,
            NavKey::Enter | NavKey::Space => match focused {
                Some(index) => self.activate(items, level, index),
                None => Outcome::Handled,
            },
            NavKey::CtrlN | NavKey::CtrlP | NavKey::Backspace | NavKey::Char(_) => Outcome::Ignored,
        }
    }

    /// Picks row `index` of the panel showing `items`: a submenu opens, a row
    /// with an action runs it.
    fn activate<T: Node>(&mut self, items: &[T], level: usize, index: usize) -> Outcome {
        let Some(item) = items.get(index) else {
            return Outcome::Handled;
        };
        if !item.selectable() {
            return Outcome::Handled;
        }
        match item.children() {
            Some(sub) => {
                self.open_sub(level, index, first_selectable(sub));
                Outcome::Handled
            }
            None => Outcome::Activate { level, index },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Row {
        label: &'static str,
        enabled: bool,
        sub: Option<Vec<Row>>,
    }

    impl Node for Row {
        fn label(&self) -> &str {
            self.label
        }
        fn selectable(&self) -> bool {
            self.enabled && !self.label.is_empty()
        }
        fn children(&self) -> Option<&[Self]> {
            self.sub.as_deref()
        }
    }

    fn row(label: &'static str) -> Row {
        Row {
            label,
            enabled: true,
            sub: None,
        }
    }

    fn sub(label: &'static str, rows: Vec<Row>) -> Row {
        Row {
            label,
            enabled: true,
            sub: Some(rows),
        }
    }

    fn separator() -> Row {
        row("")
    }

    /// New folder, New file, ---, Sort by >, View as >, Disabled, Refresh.
    fn menu() -> Vec<Row> {
        vec![
            row("New folder"),
            row("New file"),
            separator(),
            sub("Sort by", vec![row("Name"), row("Size"), separator(), row("Modified")]),
            sub("View as", vec![row("List"), row("Grid")]),
            Row {
                label: "Paste",
                enabled: false,
                sub: None,
            },
            row("Refresh"),
        ]
    }

    /// The submenu of "Sort by" sits to the right of the root menu, level 1.
    fn sort_rect() -> PanelRect {
        PanelRect {
            x: 220.0,
            y: 100.0,
            width: 200.0,
            height: 140.0,
        }
    }

    const SORT: usize = 3;
    const VIEW: usize = 4;
    const PASTE: usize = 5;
    const REFRESH: usize = 6;

    fn hover(nav: &mut NavState, index: usize, kind: RowKind, now: Ms) {
        nav.enter(0, index, kind, (100.0, 100.0), now);
    }

    #[test]
    fn a_submenu_row_highlights_at_once_and_opens_after_the_delay() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 1000);
        assert_eq!(nav.highlighted(0), Some(SORT));
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.wake_at(), Some(1000 + SUBMENU_OPEN_DELAY_MS));
        assert!(!nav.poll(1000 + SUBMENU_OPEN_DELAY_MS - 1));
        assert_eq!(nav.open_row(0), None);
        assert!(nav.poll(1000 + SUBMENU_OPEN_DELAY_MS));
        assert_eq!(nav.open_row(0), Some(SORT));
        // Opened by hover: the keys still act on the parent.
        assert_eq!(nav.active(), 0);
        assert_eq!(nav.wake_at(), None);
    }

    #[test]
    fn leaving_before_the_delay_opens_nothing() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        nav.leave(0, SORT, (100.0, 120.0), None, 100);
        hover(&mut nav, REFRESH, RowKind::Plain, 100);
        assert!(!nav.poll(500));
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.highlighted(0), Some(REFRESH));
    }

    #[test]
    fn resting_on_another_row_before_the_delay_cancels_it_too() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        hover(&mut nav, 0, RowKind::Plain, 50);
        nav.poll(10_000);
        assert_eq!(nav.open_row(0), None);
    }

    #[test]
    fn plain_rows_highlight_on_hover_without_waiting() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, 1, RowKind::Plain, 0);
        assert_eq!(nav.highlighted(0), Some(1));
        assert_eq!(nav.wake_at(), None);
    }

    #[test]
    fn a_second_submenu_row_opens_at_once_while_another_is_open() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        nav.poll(SUBMENU_OPEN_DELAY_MS);
        assert_eq!(nav.open_row(0), Some(SORT));
        nav.leave(0, SORT, (100.0, 190.0), None, 400);
        hover(&mut nav, VIEW, RowKind::Submenu, 400);
        assert_eq!(nav.open_row(0), Some(VIEW), "switching is instant");
        assert_eq!(nav.wake_at(), None);
    }

    #[test]
    fn moving_to_a_sibling_row_closes_the_open_submenu() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        nav.poll(SUBMENU_OPEN_DELAY_MS);
        nav.leave(0, SORT, (100.0, 110.0), None, 300);
        hover(&mut nav, 0, RowKind::Plain, 300);
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.open().len(), 0);
    }

    #[test]
    fn a_disabled_row_is_not_a_dead_zone() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        nav.poll(SUBMENU_OPEN_DELAY_MS);
        nav.leave(0, SORT, (100.0, 110.0), None, 300);
        hover(&mut nav, PASTE, RowKind::Disabled, 300);
        assert_eq!(nav.open_row(0), None, "the submenu closes");
        assert_eq!(nav.highlighted(0), None, "and the disabled row is not lit");
        nav.poll(10_000);
        assert_eq!(nav.open_row(0), None);
    }

    #[test]
    fn a_disabled_row_cancels_a_pending_submenu() {
        let mut nav = NavState::new(&menu(), false);
        hover(&mut nav, SORT, RowKind::Submenu, 0);
        hover(&mut nav, PASTE, RowKind::Disabled, 20);
        nav.poll(1_000);
        assert_eq!(nav.open_row(0), None);
    }

    /// Opens "Sort by" and leaves its row toward the submenu.
    fn crossing() -> NavState {
        let mut nav = NavState::new(&menu(), false);
        nav.enter(0, SORT, RowKind::Submenu, (150.0, 95.0), 0);
        nav.poll(SUBMENU_OPEN_DELAY_MS);
        nav.leave(0, SORT, (210.0, 105.0), Some(sort_rect()), 200);
        nav
    }

    #[test]
    fn a_diagonal_move_toward_the_submenu_keeps_it_open() {
        let mut nav = crossing();
        // Down and to the right, still inside the triangle, over "View as".
        nav.moved((215.0, 140.0), 210);
        nav.enter(0, VIEW, RowKind::Submenu, (215.0, 140.0), 210);
        assert_eq!(nav.open_row(0), Some(SORT));
        assert_eq!(nav.highlighted(0), Some(SORT));
        // Reaching the submenu ends the crossing.
        nav.moved((230.0, 150.0), 220);
        nav.enter(1, 1, RowKind::Plain, (230.0, 150.0), 220);
        assert_eq!(nav.open_row(0), Some(SORT));
        assert_eq!(nav.highlighted(1), Some(1));
        assert_eq!(nav.active(), 1);
        // The held row is forgotten: nothing opens later.
        nav.poll(10_000);
        assert_eq!(nav.open_row(0), Some(SORT));
    }

    #[test]
    fn leaving_the_triangle_closes_the_submenu() {
        let mut nav = crossing();
        // Straight down, away from the submenu.
        nav.moved((150.0, 250.0), 210);
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.highlighted(0), None);
        // The row now under the pointer then takes effect.
        hover(&mut nav, REFRESH, RowKind::Plain, 211);
        assert_eq!(nav.highlighted(0), Some(REFRESH));
    }

    #[test]
    fn resting_in_the_triangle_drops_it_and_the_row_under_the_pointer_takes_over() {
        let mut nav = crossing();
        nav.moved((214.0, 130.0), 210);
        nav.enter(0, VIEW, RowKind::Submenu, (214.0, 130.0), 210);
        assert_eq!(nav.open_row(0), Some(SORT));
        assert_eq!(nav.wake_at(), Some(210 + CONE_REST_MS));
        assert!(!nav.poll(210 + CONE_REST_MS - 1));
        assert!(nav.poll(210 + CONE_REST_MS));
        // The held row was entered after all: switching is instant.
        assert_eq!(nav.highlighted(0), Some(VIEW));
        assert_eq!(nav.open_row(0), Some(VIEW));
    }

    #[test]
    fn a_pointer_that_slips_off_the_held_row_forgets_it() {
        let mut nav = crossing();
        nav.moved((214.0, 130.0), 210);
        nav.enter(0, VIEW, RowKind::Submenu, (214.0, 130.0), 210);
        nav.leave(0, VIEW, (219.0, 150.0), None, 220);
        nav.poll(10_000);
        assert_eq!(nav.highlighted(0), Some(SORT));
        assert_eq!(nav.open_row(0), Some(SORT));
    }

    #[test]
    fn every_move_in_the_triangle_extends_the_rest() {
        let mut nav = crossing();
        nav.moved((214.0, 130.0), 400);
        nav.moved((216.0, 135.0), 600);
        assert_eq!(nav.wake_at(), Some(600 + CONE_REST_MS));
    }

    #[test]
    fn a_submenu_that_flipped_to_the_left_has_its_triangle_on_that_side() {
        let mut nav = NavState::new(&menu(), false);
        let left = PanelRect {
            x: 0.0,
            y: 100.0,
            width: 200.0,
            height: 140.0,
        };
        nav.enter(0, SORT, RowKind::Submenu, (250.0, 95.0), 0);
        nav.poll(SUBMENU_OPEN_DELAY_MS);
        nav.leave(0, SORT, (222.0, 105.0), Some(left), 200);
        // Toward the left submenu: kept.
        nav.moved((205.0, 140.0), 210);
        assert_eq!(nav.open_row(0), Some(SORT));
        // Away to the right: closed.
        nav.moved((300.0, 140.0), 220);
        assert_eq!(nav.open_row(0), None);
    }

    #[test]
    fn a_tap_opens_the_submenu_without_any_hover() {
        let mut nav = NavState::new(&menu(), false);
        let rows = menu();
        let first = first_selectable(rows[SORT].children().expect("a submenu"));
        nav.open_sub(0, SORT, first);
        assert_eq!(nav.open_row(0), Some(SORT));
        assert_eq!(nav.highlighted(1), Some(0));
        assert_eq!(nav.active(), 1);
    }

    // -- keyboard ---------------------------------------------------------

    #[test]
    fn down_and_up_skip_separators_and_disabled_rows() {
        let rows = menu();
        let mut nav = NavState::new(&rows, true);
        assert_eq!(nav.highlighted(0), Some(0));
        nav.press(&rows, NavKey::Down, 0);
        assert_eq!(nav.highlighted(0), Some(1));
        nav.press(&rows, NavKey::Down, 0);
        assert_eq!(nav.highlighted(0), Some(SORT));
        nav.press(&rows, NavKey::Down, 0);
        nav.press(&rows, NavKey::Down, 0);
        assert_eq!(nav.highlighted(0), Some(REFRESH), "Paste is disabled");
        nav.press(&rows, NavKey::Down, 0);
        assert_eq!(nav.highlighted(0), Some(0), "wraps");
        nav.press(&rows, NavKey::Up, 0);
        assert_eq!(nav.highlighted(0), Some(REFRESH));
        nav.press(&rows, NavKey::Home, 0);
        assert_eq!(nav.highlighted(0), Some(0));
        nav.press(&rows, NavKey::End, 0);
        assert_eq!(nav.highlighted(0), Some(REFRESH));
    }

    #[test]
    fn right_opens_a_submenu_with_its_first_row_and_left_closes_it() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.set_highlight(0, Some(SORT));
        assert_eq!(nav.press(&rows, NavKey::Right, 0), Outcome::Handled);
        assert_eq!(nav.open_row(0), Some(SORT));
        assert_eq!(nav.active(), 1);
        assert_eq!(nav.highlighted(1), Some(0));
        nav.press(&rows, NavKey::Down, 0);
        assert_eq!(nav.highlighted(1), Some(1));
        assert_eq!(nav.press(&rows, NavKey::Left, 0), Outcome::Handled);
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.active(), 0);
        assert_eq!(nav.highlighted(0), Some(SORT), "the parent row stays lit");
        // Left at the top level is not the menu's.
        assert_eq!(nav.press(&rows, NavKey::Left, 0), Outcome::Ignored);
    }

    #[test]
    fn right_on_a_plain_row_does_nothing() {
        let rows = menu();
        let mut nav = NavState::new(&rows, true);
        assert_eq!(nav.press(&rows, NavKey::Right, 0), Outcome::Ignored);
    }

    #[test]
    fn enter_and_space_open_a_submenu_or_pick_a_row() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.set_highlight(0, Some(VIEW));
        assert_eq!(nav.press(&rows, NavKey::Enter, 0), Outcome::Handled);
        assert_eq!(nav.open_row(0), Some(VIEW));
        assert_eq!(
            nav.press(&rows, NavKey::Space, 0),
            Outcome::Activate { level: 1, index: 0 }
        );
        nav.close_deepest();
        nav.set_highlight(0, Some(1));
        assert_eq!(
            nav.press(&rows, NavKey::Enter, 0),
            Outcome::Activate { level: 0, index: 1 }
        );
        // Nothing highlighted: the key is taken and does nothing.
        let mut idle = NavState::new(&rows, false);
        assert_eq!(idle.press(&rows, NavKey::Enter, 0), Outcome::Handled);
    }

    #[test]
    fn escape_steps_back_one_level_then_closes_the_menu() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.open_sub(0, SORT, Some(0));
        assert_eq!(nav.press(&rows, NavKey::Escape, 0), Outcome::Handled);
        assert_eq!(nav.open_row(0), None);
        assert_eq!(nav.press(&rows, NavKey::Escape, 0), Outcome::Dismiss);
    }

    #[test]
    fn tab_leaves_the_menu_and_goes_on() {
        let rows = menu();
        let mut nav = NavState::new(&rows, true);
        assert_eq!(nav.press(&rows, NavKey::Tab, 0), Outcome::Leave);
        assert_eq!(nav.press(&rows, NavKey::ShiftTab, 0), Outcome::Leave);
    }

    // -- type-ahead in a menu ---------------------------------------------

    #[test]
    fn typing_focuses_the_best_match_and_marks_the_panel() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        assert_eq!(nav.press(&rows, NavKey::Char('r'), 0), Outcome::Handled);
        assert_eq!(nav.highlighted(0), Some(REFRESH));
        assert!(nav.ahead().is_active());
        assert_eq!(
            nav.ahead().mark(REFRESH),
            menu_query::Mark::Hit(0..1)
        );
        assert_eq!(nav.ahead().mark(1), menu_query::Mark::Dim);
    }

    #[test]
    fn with_a_query_the_arrows_walk_the_matches_and_enter_picks() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('n'), 0);
        assert_eq!(nav.highlighted(0), Some(0));
        // Matches: New folder, New file, Sort by? no; "n" in "Sort by"? no.
        nav.press(&rows, NavKey::Down, 10);
        assert_eq!(nav.highlighted(0), Some(1));
        nav.press(&rows, NavKey::Down, 20);
        assert_eq!(nav.highlighted(0), Some(0), "wraps over the matches only");
        nav.press(&rows, NavKey::CtrlP, 30);
        assert_eq!(nav.highlighted(0), Some(1));
        nav.press(&rows, NavKey::ShiftTab, 40);
        assert_eq!(nav.highlighted(0), Some(0));
        nav.press(&rows, NavKey::Tab, 50);
        assert_eq!(nav.highlighted(0), Some(1));
        assert_eq!(
            nav.press(&rows, NavKey::Enter, 60),
            Outcome::Activate { level: 0, index: 1 }
        );
    }

    #[test]
    fn a_query_that_finds_a_submenu_enters_it_on_enter() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('v'), 0);
        assert_eq!(nav.highlighted(0), Some(VIEW));
        assert_eq!(nav.press(&rows, NavKey::Enter, 5), Outcome::Handled);
        assert_eq!(nav.open_row(0), Some(VIEW));
        assert_eq!(nav.active(), 1);
        // The query belonged to the root panel.
        assert!(!nav.ahead().is_active());
    }

    #[test]
    fn escape_clears_the_query_first_and_only_then_closes() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('r'), 0);
        assert_eq!(nav.press(&rows, NavKey::Escape, 10), Outcome::Handled);
        assert!(!nav.ahead().is_active());
        assert_eq!(nav.press(&rows, NavKey::Escape, 20), Outcome::Dismiss);
    }

    #[test]
    fn space_types_inside_a_query_and_picks_otherwise() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('n'), 0);
        nav.press(&rows, NavKey::Char('e'), 1);
        nav.press(&rows, NavKey::Char('w'), 2);
        assert_eq!(nav.press(&rows, NavKey::Space, 3), Outcome::Handled);
        assert_eq!(nav.ahead().query(), "new ");
        nav.press(&rows, NavKey::Char('f'), 4);
        assert_eq!(nav.ahead().query(), "new f");
        assert_eq!(nav.highlighted(0), Some(0));
        assert_eq!(nav.ahead().matches().count(), 2);
    }

    #[test]
    fn the_query_runs_out_on_the_clock_and_the_menu_stays_where_it_was() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('r'), 100);
        assert_eq!(nav.wake_at(), Some(100 + menu_query::RESET_MS));
        assert!(!nav.poll(100 + menu_query::RESET_MS - 1));
        assert!(nav.poll(100 + menu_query::RESET_MS));
        assert!(!nav.ahead().is_active());
        assert_eq!(nav.highlighted(0), Some(REFRESH));
        // Space now picks again.
        assert_eq!(
            nav.press(&rows, NavKey::Space, 2000),
            Outcome::Activate { level: 0, index: REFRESH }
        );
    }

    #[test]
    fn a_query_is_per_panel() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.set_highlight(0, Some(SORT));
        nav.press(&rows, NavKey::Right, 0);
        nav.press(&rows, NavKey::Char('s'), 1);
        assert_eq!(nav.highlighted(1), Some(1), "Size in the submenu");
        nav.press(&rows, NavKey::Escape, 2);
        assert!(!nav.ahead().is_active());
        assert_eq!(nav.active(), 1, "Escape cleared the query only");
        nav.press(&rows, NavKey::Char('m'), 3);
        assert_eq!(nav.highlighted(1), Some(3));
        nav.press(&rows, NavKey::Left, 4);
        assert!(!nav.ahead().is_active());
        assert_eq!(nav.active(), 0);
    }

    #[test]
    fn disabled_rows_are_never_found() {
        let rows = menu();
        let mut nav = NavState::new(&rows, false);
        nav.press(&rows, NavKey::Char('p'), 0);
        assert_eq!(nav.highlighted(0), None, "Paste is disabled");
        assert_eq!(nav.ahead().matches().count(), 0);
        assert_eq!(nav.ahead().mark(PASTE), menu_query::Mark::Plain);
    }
}
