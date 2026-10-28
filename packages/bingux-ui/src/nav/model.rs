//! The navigation model: a stack of locations and the operations on it.
//!
//! Pure data and logic, no UI: everything the person sees about navigation is
//! derived from a [`NavModel`] and the [`RouteTable`] it holds. The model is
//! what the tests drive; [`super::router::Router`] wraps it in a Freya state.
//! See `design/navigation.md`, section 3.3, for the rules.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use super::location::Location;
use super::table::{Mode, RouteKind, RouteTable, Transition};
use crate::icons::Icon;

/// One place in the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Unique for the life of the model. Retained state (scroll, UI state) is
    /// keyed by it, so it survives a query change that replaces the entry in
    /// place and is dropped when the entry leaves every stack.
    pub id: u64,
    pub location: Location,
    /// A title the page or the row that opened it supplied.
    pub title: Option<String>,
}

/// What a floating surface is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    Menu,
    Choice,
    Dialog,
    Sheet,
}

/// An open floating surface: part of the stack so Back dismisses it first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modal {
    pub id: u64,
    pub kind: ModalKind,
    pub title: String,
}

/// What a page says when the stack is about to leave it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PopGuard {
    /// Leave.
    #[default]
    Allow,
    /// Stay, silently.
    Block,
    /// Ask first; the move happens only if the person discards.
    Confirm(Confirmation),
}

/// The question a [`PopGuard::Confirm`] asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirmation {
    pub title: String,
    pub body: String,
    /// The label of the button that leaves anyway.
    pub discard: String,
}

/// A guard function a page registers for its entry.
pub type Guard = Rc<dyn Fn() -> PopGuard>;

/// An operation on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Push { to: Location, title: Option<String> },
    Pop,
    PopTo(u64),
    Up,
    Replace(Location),
    SwitchRoot(Location),
    Reset(Location),
    Navigate(Location),
    /// The Back of Escape, Alt+Left, the Back button and the swipe: the top
    /// modal, else a pop.
    Back,
    Forward,
    ForwardTo(u64),
}

/// A change of the current entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub transition: Transition,
    pub from: Entry,
    pub to: Entry,
    /// Entries that left the back and forward stacks, for dropping their state.
    pub removed: Vec<Entry>,
    /// How many entries a pop crossed: a menu choice two levels up is one
    /// change with `steps == 2`.
    pub steps: usize,
}

/// What an operation did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Changed(Change),
    /// Only the fragment changed: scroll to the anchor, nothing moved.
    Anchored(Option<String>),
    /// A modal was asked to close. The caller runs its dismissal.
    DismissModal(u64),
    /// A page vetoed leaving. `Block` is final; `Confirm` carries the op to
    /// run again, forced, if the person discards.
    Vetoed(PopGuard, Box<Op>),
    /// Nothing to do: already there, or nowhere to go.
    Unchanged,
}

/// One row of a history menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryItem {
    pub entry: u64,
    pub title: String,
    pub icon: Option<Icon>,
    pub location: Location,
    /// How many entries choosing it crosses.
    pub steps: usize,
}

/// A debug view of the model, for the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub back: Vec<EntryInfo>,
    pub current: EntryInfo,
    pub forward: Vec<EntryInfo>,
    pub modals: Vec<Modal>,
    pub mode: Mode,
}

/// One entry as the inspector lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryInfo {
    pub id: u64,
    pub uri: String,
    pub route: String,
    pub params: Vec<(String, String)>,
    pub title: String,
}

/// The stack.
#[derive(Clone)]
pub struct NavModel {
    table: Arc<RouteTable>,
    back: Vec<Entry>,
    current: Entry,
    /// The nearest entry last.
    forward: Vec<Entry>,
    modals: Vec<Modal>,
    guards: Rc<RefCell<HashMap<u64, Guard>>>,
    next_id: u64,
    wide: bool,
}

impl NavModel {
    /// A model starting at `start` (not canonical yet). In hierarchical mode
    /// the stack is synthesised from the table, so a window opened on
    /// `settings://network/proxy` can go Back to Network.
    pub fn new(table: Arc<RouteTable>, start: &Location) -> Self {
        let start = table.canonical(start);
        let mut model = Self {
            current: Entry {
                id: 0,
                location: start.clone(),
                title: None,
            },
            table,
            back: Vec::new(),
            forward: Vec::new(),
            modals: Vec::new(),
            guards: Rc::new(RefCell::new(HashMap::new())),
            next_id: 1,
            wide: true,
        };
        model.reset_to(&start);
        model
    }

    fn make(&mut self, location: Location, title: Option<String>) -> Entry {
        let id = self.next_id;
        self.next_id += 1;
        Entry {
            id,
            location,
            title,
        }
    }

    fn reset_to(&mut self, to: &Location) {
        let chain = if self.table.mode == Mode::Hierarchical {
            self.table.chain(to)
        } else {
            vec![to.clone()]
        };
        let last = chain.len().saturating_sub(1);
        let mut entries: Vec<Entry> = Vec::new();
        for (index, place) in chain.into_iter().enumerate() {
            // The query and fragment belong to the destination, not its parents.
            let place = if index == last { to.clone() } else { place };
            entries.push(self.make(place, None));
        }
        self.current = entries.pop().expect("a chain has its destination");
        self.back = entries;
        self.forward.clear();
    }

    // ---- reading ----------------------------------------------------------

    pub fn table(&self) -> &Arc<RouteTable> {
        &self.table
    }

    pub fn current(&self) -> &Entry {
        &self.current
    }

    pub fn location(&self) -> &Location {
        &self.current.location
    }

    /// The entries below the current one, oldest first.
    pub fn back_entries(&self) -> &[Entry] {
        &self.back
    }

    /// The entries Forward would visit, nearest last.
    pub fn forward_entries(&self) -> &[Entry] {
        &self.forward
    }

    /// The entry below the current one: what Back reveals.
    pub fn previous(&self) -> Option<&Entry> {
        self.back.last()
    }

    pub fn modals(&self) -> &[Modal] {
        &self.modals
    }

    /// Tell the model whether the window shows the sidebar beside the page.
    /// A wide window hides the index entry: the sidebar is that list.
    pub fn set_wide(&mut self, wide: bool) {
        self.wide = wide;
    }

    pub fn is_wide(&self) -> bool {
        self.wide
    }

    fn is_index(&self, entry: &Entry) -> bool {
        self.table.kind(&entry.location) == RouteKind::Index
    }

    /// The back entries a person can see: all of them on a compact window, all
    /// but the index on a wide one.
    pub fn visible_back(&self) -> Vec<&Entry> {
        self.back
            .iter()
            .filter(|entry| !(self.wide && self.is_index(entry)))
            .collect()
    }

    /// Whether Back would do something: close a modal or leave a page.
    pub fn can_back(&self) -> bool {
        !self.modals.is_empty() || !self.visible_back().is_empty()
    }

    /// Whether there is a page to go back to. This, and not [`NavModel::can_back`],
    /// decides whether the Back button shows, the history menu has rows and a
    /// back swipe may start: an open menu or dialog never changes the header.
    pub fn can_go_back(&self) -> bool {
        !self.visible_back().is_empty()
    }

    /// Whether there is an entry to reveal under a back gesture: the page
    /// below, ignoring modals (a gesture dismisses those one at a time).
    pub fn has_previous(&self) -> bool {
        !self.visible_back().is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// The title of `entry`: the one it carries, else its route's.
    pub fn title(&self, entry: &Entry) -> String {
        entry
            .title
            .clone()
            .unwrap_or_else(|| self.table.title_of(&entry.location))
    }

    /// The window title: the page's, then the app's.
    pub fn window_title(&self, app: &str) -> String {
        if self.is_index(&self.current) {
            return app.to_string();
        }
        format!("{} \u{2013} {app}", self.title(&self.current))
    }

    fn item(&self, entry: &Entry, steps: usize) -> HistoryItem {
        HistoryItem {
            entry: entry.id,
            title: self.title(entry),
            icon: self.table.icon_of(&entry.location),
            location: entry.location.clone(),
            steps,
        }
    }

    /// The Back button's menu: the entries below the current one, nearest
    /// first. Choosing one is `pop_to`.
    pub fn back_menu(&self) -> Vec<HistoryItem> {
        self.back
            .iter()
            .rev()
            .enumerate()
            .filter(|(_, entry)| !(self.wide && self.is_index(entry)))
            .map(|(steps, entry)| self.item(entry, steps + 1))
            .collect()
    }

    /// The Forward button's menu: the entries Forward would visit, nearest
    /// first. Choosing one is `forward_to`.
    pub fn forward_menu(&self) -> Vec<HistoryItem> {
        self.forward
            .iter()
            .rev()
            .enumerate()
            .map(|(steps, entry)| self.item(entry, steps + 1))
            .collect()
    }

    /// The root route of the current entry. On the index, a wide window shows
    /// the first root.
    pub fn selected_root(&self) -> Option<usize> {
        self.table.root_of(&self.current.location).or_else(|| {
            (self.wide && self.is_index(&self.current))
                .then(|| self.table.default_root())
                .flatten()
        })
    }

    fn info(&self, entry: &Entry) -> EntryInfo {
        let resolved = self.table.resolve(&entry.location);
        EntryInfo {
            id: entry.id,
            uri: entry.location.to_string(),
            route: resolved.as_ref().map_or_else(
                || "(not found)".to_string(),
                |found| self.table.route_def(found.route).id().to_string(),
            ),
            params: resolved
                .map(|found| found.params.bindings().to_vec())
                .unwrap_or_default(),
            title: self.title(entry),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            back: self.back.iter().map(|entry| self.info(entry)).collect(),
            current: self.info(&self.current),
            forward: self.forward.iter().rev().map(|entry| self.info(entry)).collect(),
            modals: self.modals.clone(),
            mode: self.table.mode,
        }
    }

    // ---- state the pages register -----------------------------------------

    /// Set the title of entry `id`, as a page does once it knows the name.
    /// Returns whether it changed.
    pub fn set_title(&mut self, id: u64, title: &str) -> bool {
        let title = Some(title.to_string());
        for entry in self.back.iter_mut().chain(self.forward.iter_mut()) {
            if entry.id == id {
                let changed = entry.title != title;
                entry.title = title;
                return changed;
            }
        }
        if self.current.id == id {
            let changed = self.current.title != title;
            self.current.title = title;
            return changed;
        }
        false
    }

    /// Register or clear the pop guard of entry `id`.
    pub fn set_guard(&self, id: u64, guard: Option<Guard>) {
        match guard {
            Some(guard) => {
                self.guards.borrow_mut().insert(id, guard);
            }
            None => {
                self.guards.borrow_mut().remove(&id);
            }
        }
    }

    /// Record an open floating surface and return its id.
    pub fn open_modal(&mut self, kind: ModalKind, title: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.modals.push(Modal {
            id,
            kind,
            title: title.to_string(),
        });
        id
    }

    pub fn close_modal(&mut self, id: u64) {
        self.modals.retain(|modal| modal.id != id);
    }

    // ---- operations -------------------------------------------------------

    /// Push `to`: what a row that opens a subpage does.
    pub fn push(&mut self, to: &Location) -> Outcome {
        self.apply(
            Op::Push {
                to: to.clone(),
                title: None,
            },
            false,
        )
    }

    /// Push `to` with the title the caller already knows.
    pub fn push_titled(&mut self, to: &Location, title: &str) -> Outcome {
        self.apply(
            Op::Push {
                to: to.clone(),
                title: Some(title.to_string()),
            },
            false,
        )
    }

    pub fn pop(&mut self) -> Outcome {
        self.apply(Op::Pop, false)
    }

    pub fn pop_to(&mut self, entry: u64) -> Outcome {
        self.apply(Op::PopTo(entry), false)
    }

    pub fn up(&mut self) -> Outcome {
        self.apply(Op::Up, false)
    }

    pub fn replace(&mut self, to: &Location) -> Outcome {
        self.apply(Op::Replace(to.clone()), false)
    }

    pub fn switch_root(&mut self, to: &Location) -> Outcome {
        self.apply(Op::SwitchRoot(to.clone()), false)
    }

    pub fn reset(&mut self, to: &Location) -> Outcome {
        self.apply(Op::Reset(to.clone()), false)
    }

    pub fn navigate(&mut self, to: &Location) -> Outcome {
        self.apply(Op::Navigate(to.clone()), false)
    }

    pub fn back(&mut self) -> Outcome {
        self.apply(Op::Back, false)
    }

    pub fn forward(&mut self) -> Outcome {
        self.apply(Op::Forward, false)
    }

    pub fn forward_to(&mut self, entry: u64) -> Outcome {
        self.apply(Op::ForwardTo(entry), false)
    }

    /// The guard verdict for leaving `entries`, nearest the person first.
    fn verdict(&self, entries: &[&Entry]) -> PopGuard {
        for entry in entries {
            let guard = self.guards.borrow().get(&entry.id).cloned();
            if let Some(guard) = guard {
                match guard() {
                    PopGuard::Allow => {}
                    other => return other,
                }
            }
        }
        PopGuard::Allow
    }

    /// Run `op`. `force` skips the pop guards: it is how a confirmed discard
    /// runs the op again.
    pub fn apply(&mut self, op: Op, force: bool) -> Outcome {
        let table = self.table.clone();
        match op.clone() {
            Op::Back => match self.modals.last() {
                Some(modal) => Outcome::DismissModal(modal.id),
                None => self.apply(Op::Pop, force),
            },
            Op::Push { to, title } => self.apply_push(&op, to, title, force),
            Op::Pop => {
                if self.visible_back().is_empty() {
                    return Outcome::Unchanged;
                }
                let target = self.back.len() - 1;
                self.leave_to(&op, target, force, true)
            }
            Op::PopTo(id) => match self.back.iter().position(|entry| entry.id == id) {
                Some(target) => self.leave_to(&op, target, force, true),
                None => Outcome::Unchanged,
            },
            Op::Up => {
                let Some(up) = table.up(&self.current.location) else {
                    return Outcome::Unchanged;
                };
                match self.back.iter().rposition(|entry| entry.location.same_path(&up)) {
                    Some(target) if target + 1 == self.back.len() => {
                        self.leave_to(&op, target, force, true)
                    }
                    _ => self.apply_replace(&op, up, force, true),
                }
            }
            Op::Replace(to) => {
                let to = table.canonical(&to);
                self.apply_replace(&op, to, force, false)
            }
            Op::SwitchRoot(to) => self.apply_switch_root(&op, to, force),
            Op::Reset(to) => {
                let to = table.canonical(&to);
                let mut leaving: Vec<&Entry> = vec![&self.current];
                leaving.extend(self.back.iter().rev());
                if !force && let Some(stop) = self.stop(&op, self.verdict(&leaving)) {
                    return stop;
                }
                let from = self.current.clone();
                let mut removed = self.back.clone();
                removed.extend(self.forward.clone());
                self.reset_to(&to);
                Outcome::Changed(Change {
                    transition: Transition::None,
                    from,
                    to: self.current.clone(),
                    removed,
                    steps: 0,
                })
            }
            Op::Navigate(to) => self.apply_navigate(&op, to, force),
            Op::Forward => match self.forward.last() {
                Some(next) => {
                    let id = next.id;
                    self.apply(Op::ForwardTo(id), force)
                }
                None => Outcome::Unchanged,
            },
            Op::ForwardTo(id) => {
                let Some(target) = self.forward.iter().position(|entry| entry.id == id) else {
                    return Outcome::Unchanged;
                };
                if !force && let Some(stop) = self.stop(&op, self.verdict(&[&self.current])) {
                    return stop;
                }
                let from = self.current.clone();
                // The entries nearer than the target become back entries.
                let mut crossed: Vec<Entry> = self.forward.split_off(target);
                crossed.reverse();
                let steps = crossed.len();
                let to = crossed.pop().expect("the target is among the crossed");
                self.back.push(from.clone());
                self.back.extend(crossed);
                self.current = to.clone();
                Outcome::Changed(Change {
                    transition: self.transition_between(&from.location, &to.location, Some(Transition::Push)),
                    from,
                    to,
                    removed: Vec::new(),
                    steps,
                })
            }
        }
    }

    fn stop(&self, op: &Op, verdict: PopGuard) -> Option<Outcome> {
        match verdict {
            PopGuard::Allow => None,
            other => Some(Outcome::Vetoed(other, Box::new(op.clone()))),
        }
    }

    /// The transition for a move, with the table's rules applied, and the
    /// wide window's: the index is the sidebar there, so a move to or from it
    /// is a cross-fade of the page, or nothing when the same root shows.
    fn transition_between(
        &self,
        from: &Location,
        to: &Location,
        directed: Option<Transition>,
    ) -> Transition {
        let table = &self.table;
        if table.mode == Mode::Linear || from.same_path(to) {
            return Transition::None;
        }
        if self.wide
            && (table.kind(from) == RouteKind::Index || table.kind(to) == RouteKind::Index)
        {
            let shown = |place: &Location| {
                table
                    .root_of(place)
                    .or_else(|| (table.kind(place) == RouteKind::Index).then(|| table.default_root()).flatten())
            };
            return if shown(from) == shown(to) {
                Transition::None
            } else {
                Transition::Fade
            };
        }
        let classified = table.classify(from, to);
        match directed {
            // A pop or a forward keeps its direction; a change of root still fades.
            Some(direction) if classified != Transition::Fade => direction,
            _ => classified,
        }
    }

    /// Leave the current entry for `back[target]`, dropping everything above.
    fn leave_to(&mut self, op: &Op, target: usize, force: bool, keep_forward: bool) -> Outcome {
        let mut leaving: Vec<&Entry> = vec![&self.current];
        leaving.extend(self.back[target + 1..].iter().rev());
        if !force && let Some(stop) = self.stop(op, self.verdict(&leaving)) {
            return stop;
        }
        let from = self.current.clone();
        let mut crossed: Vec<Entry> = self.back.split_off(target + 1);
        let to = self.back.pop().expect("the target exists");
        let steps = crossed.len() + 1;
        // Linear apps keep what was left so Forward can come back to it.
        let mut removed = Vec::new();
        if self.table.mode == Mode::Linear && keep_forward {
            self.forward.push(from.clone());
            crossed.reverse();
            self.forward.extend(crossed);
        } else {
            removed.push(from.clone());
            removed.extend(crossed);
        }
        self.current = to.clone();
        Outcome::Changed(Change {
            transition: self.transition_between(&from.location, &to.location, Some(Transition::Pop)),
            from,
            to,
            removed,
            steps,
        })
    }

    fn apply_replace(&mut self, op: &Op, to: Location, force: bool, popping: bool) -> Outcome {
        if to.same_place(&self.current.location) {
            return self.anchored(&to);
        }
        if !force && let Some(stop) = self.stop(op, self.verdict(&[&self.current])) {
            return stop;
        }
        let from = self.current.clone();
        let entry = self.make(to, None);
        self.current = entry.clone();
        let directed = popping.then_some(Transition::Pop);
        Outcome::Changed(Change {
            transition: self.transition_between(&from.location, &entry.location, directed),
            from: from.clone(),
            to: entry,
            removed: vec![from],
            steps: 0,
        })
    }

    /// A move to the place the current entry already is: only the fragment
    /// may differ.
    fn anchored(&mut self, to: &Location) -> Outcome {
        if self.current.location.fragment() == to.fragment() {
            return Outcome::Unchanged;
        }
        self.current.location = to.clone();
        Outcome::Anchored(to.fragment().map(str::to_string))
    }

    fn apply_push(&mut self, op: &Op, to: Location, title: Option<String>, force: bool) -> Outcome {
        let table = self.table.clone();
        let to = table.canonical(&to);
        if to.same_place(&self.current.location) {
            return self.anchored(&to);
        }
        // A query-only change: replace the entry in place, or push when a
        // changed key is declared to push.
        if to.same_path(&self.current.location) {
            let pushes = self.query_pushes(&self.current.location, &to);
            if !pushes {
                let from = self.current.clone();
                self.current.location = to;
                if title.is_some() {
                    self.current.title = title;
                }
                return Outcome::Changed(Change {
                    transition: Transition::None,
                    from,
                    to: self.current.clone(),
                    removed: Vec::new(),
                    steps: 0,
                });
            }
        }
        let current_route = table.resolve(&self.current.location);
        let replaces = match (&current_route, table.resolve(&to)) {
            // An ephemeral page is never left behind.
            (Some(found), _) if table.route_def(found.route).ephemeral => true,
            // A tab replaces its sibling.
            (Some(here), Some(there)) => {
                table.route_def(here.route).kind == RouteKind::Tab
                    && table.route_def(there.route).kind == RouteKind::Tab
                    && table.up(&self.current.location) == table.up(&to)
            }
            _ => false,
        };
        if replaces {
            if !force && let Some(stop) = self.stop(op, self.verdict(&[&self.current])) {
                return stop;
            }
            let from = self.current.clone();
            let entry = self.make(to, title);
            self.current = entry.clone();
            self.forward.clear();
            let tab = table.kind(&from.location) == RouteKind::Tab;
            return Outcome::Changed(Change {
                transition: if tab {
                    Transition::Fade
                } else {
                    self.transition_between(&from.location, &entry.location, None)
                },
                from: from.clone(),
                to: entry,
                removed: vec![from],
                steps: 0,
            });
        }
        let from = self.current.clone();
        let entry = self.make(to, title);
        self.back.push(from.clone());
        self.current = entry.clone();
        let removed = std::mem::take(&mut self.forward);
        Outcome::Changed(Change {
            transition: self.transition_between(&from.location, &entry.location, None),
            from,
            to: entry,
            removed,
            steps: 0,
        })
    }

    /// Whether going from `old` to `new` (the same path) changes a query key
    /// that the route declares as pushing.
    fn query_pushes(&self, old: &Location, new: &Location) -> bool {
        let Some(found) = self.table.resolve(new) else {
            return false;
        };
        self.table.route_def(found.route).query.iter().any(|spec| {
            spec.pushes && old.query(spec.key) != new.query(spec.key)
        })
    }

    fn apply_switch_root(&mut self, op: &Op, to: Location, force: bool) -> Outcome {
        let table = self.table.clone();
        let to = table.canonical(&to);
        let wanted = table.root_of(&to);
        if wanted.is_some() && table.root_of(&self.current.location) == wanted {
            // Already in that root: choosing it again returns to its top.
            if let Some(target) = self
                .back
                .iter()
                .rposition(|entry| entry.location.same_path(&to))
            {
                return self.leave_to(op, target, force, false);
            }
            if self.current.location.same_path(&to) {
                return Outcome::Unchanged;
            }
        }
        let mut leaving: Vec<&Entry> = vec![&self.current];
        leaving.extend(self.back.iter().rev());
        if !force && let Some(stop) = self.stop(op, self.verdict(&leaving)) {
            return stop;
        }
        let from = self.current.clone();
        let mut removed = std::mem::take(&mut self.back);
        removed.extend(std::mem::take(&mut self.forward));
        // The index stays at the bottom, so the list is one Back away on a phone.
        let home = table.home();
        if table.resolve(&home).is_some() && !to.same_path(&home) {
            let index = self.make(home, None);
            self.back = vec![index];
        }
        let entry = self.make(to, None);
        self.current = entry.clone();
        removed.push(from.clone());
        Outcome::Changed(Change {
            transition: self.transition_between(&from.location, &entry.location, None),
            from,
            to: entry,
            removed,
            steps: 0,
        })
    }

    fn apply_navigate(&mut self, op: &Op, to: Location, force: bool) -> Outcome {
        let table = self.table.clone();
        let to = table.canonical(&to);
        if table.mode == Mode::Linear {
            return self.apply_push(op, to, None, force);
        }
        if to.same_place(&self.current.location) {
            return self.anchored(&to);
        }
        let chain = table.chain(&to);
        let stack: Vec<Entry> = self
            .back
            .iter()
            .cloned()
            .chain(std::iter::once(self.current.clone()))
            .collect();
        let shared = stack
            .iter()
            .zip(&chain)
            .take_while(|(entry, place)| entry.location.same_path(place))
            .count();
        if shared == 0 {
            return self.apply(Op::Reset(to), force);
        }
        // `to` is on the stack already: go back to it, taking its query along.
        if shared == chain.len() && chain.len() <= stack.len() {
            let target = chain.len() - 1;
            if target == stack.len() - 1 {
                // The current entry, with another query or fragment.
                return self.apply_push(op, to, None, force);
            }
            let mut outcome = self.leave_to(op, target, force, false);
            if let Outcome::Changed(change) = &mut outcome
                && change.to.location != to
            {
                self.current.location = to.clone();
                change.to.location = to;
            }
            return outcome;
        }
        // Keep the shared part, drop the rest, push what is missing.
        let leaving: Vec<&Entry> = stack[shared..].iter().rev().collect();
        if !force && let Some(stop) = self.stop(op, self.verdict(&leaving)) {
            return stop;
        }
        let from = self.current.clone();
        let mut removed: Vec<Entry> = stack[shared..].to_vec();
        removed.extend(std::mem::take(&mut self.forward));
        // Entries up to `shared` stay, including the current one when the whole
        // stack is the new location's ancestry; the rest of the chain is pushed.
        self.back = stack[..shared].to_vec();
        let last = chain.len() - 1;
        for (index, place) in chain.iter().enumerate().skip(shared) {
            let place = if index == last { to.clone() } else { place.clone() };
            let entry = self.make(place, None);
            if index == last {
                self.current = entry;
            } else {
                self.back.push(entry);
            }
        }
        let to = self.current.clone();
        Outcome::Changed(Change {
            transition: self.transition_between(&from.location, &to.location, None),
            from,
            to,
            removed,
            steps: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::table::tests::{loc, settings};
    use super::*;

    fn model(start: &str) -> NavModel {
        let mut model = NavModel::new(Arc::new(settings()), &loc(start));
        model.set_wide(false);
        model
    }

    fn uris(model: &NavModel) -> Vec<String> {
        model
            .back_entries()
            .iter()
            .chain(std::iter::once(model.current()))
            .map(|entry| entry.location.to_string())
            .collect()
    }

    fn changed(outcome: Outcome) -> Change {
        match outcome {
            Outcome::Changed(change) => change,
            other => panic!("expected a change, got {other:?}"),
        }
    }

    #[test]
    fn a_window_opened_on_a_deep_link_has_the_synthesised_stack() {
        let model = model("settings://network/wifi/saved/Home");
        assert_eq!(
            uris(&model),
            [
                "settings://",
                "settings://network",
                "settings://network/wifi",
                "settings://network/wifi/saved",
                "settings://network/wifi/saved/Home"
            ]
        );
    }

    #[test]
    fn push_appends_and_pop_returns() {
        let mut model = model("settings://network");
        let change = changed(model.push(&loc("settings://network/wifi")));
        assert_eq!(change.transition, Transition::Push);
        assert_eq!(uris(&model).last().unwrap(), "settings://network/wifi");
        let change = changed(model.pop());
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(change.steps, 1);
        assert_eq!(model.location(), &loc("settings://network"));
    }

    #[test]
    fn back_follows_where_you_came_from_not_the_path() {
        // Network status row opens the network directly; Back returns to
        // Network, not to Wi-Fi.
        let mut model = model("settings://network");
        model.push(&loc("settings://network/wifi/network/Home"));
        let change = changed(model.back());
        assert_eq!(change.to.location, loc("settings://network"));
    }

    #[test]
    fn pop_to_crosses_several_entries_in_one_transition() {
        let mut model = model("settings://network");
        model.push(&loc("settings://network/wifi"));
        model.push(&loc("settings://network/wifi/saved"));
        model.push(&loc("settings://network/wifi/saved/Home"));
        let target = model.back_entries()[1].id;
        assert_eq!(model.back_entries()[1].location, loc("settings://network"));
        let change = changed(model.pop_to(target));
        assert_eq!(change.steps, 3);
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(change.removed.len(), 3);
        assert_eq!(model.location(), &loc("settings://network"));
        assert_eq!(model.back_entries().len(), 1);
    }

    #[test]
    fn pop_at_the_bottom_does_nothing() {
        let mut model = model("settings://");
        assert_eq!(model.pop(), Outcome::Unchanged);
        assert!(!model.can_back());
    }

    #[test]
    fn a_wide_window_hides_the_index_so_a_root_has_no_back() {
        let mut model = model("settings://network");
        model.set_wide(true);
        assert!(!model.can_back());
        assert_eq!(model.pop(), Outcome::Unchanged);
        assert!(model.back_menu().is_empty());
        model.set_wide(false);
        assert!(model.can_back());
        assert_eq!(model.back_menu().len(), 1);
        assert_eq!(model.back_menu()[0].title, "Settings");
    }

    #[test]
    fn replace_swaps_the_current_entry() {
        let mut model = model("settings://network/wifi");
        let before = model.back_entries().len();
        let change = changed(model.replace(&loc("settings://network/proxy")));
        assert_eq!(model.back_entries().len(), before);
        assert_eq!(model.location(), &loc("settings://network/proxy"));
        assert_eq!(change.removed.len(), 1);
    }

    #[test]
    fn switch_root_clears_the_stack_with_a_fade() {
        let mut model = model("settings://network/wifi/saved");
        let change = changed(model.switch_root(&loc("settings://sound")));
        assert_eq!(change.transition, Transition::Fade);
        assert_eq!(uris(&model), ["settings://", "settings://sound"]);
        assert!(change.removed.len() >= 3);
    }

    #[test]
    fn switch_root_to_the_root_you_are_in_pops_to_it() {
        let mut model = model("settings://network/wifi");
        let change = changed(model.switch_root(&loc("settings://network")));
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(uris(&model), ["settings://", "settings://network"]);
        assert_eq!(model.switch_root(&loc("settings://network")), Outcome::Unchanged);
    }

    #[test]
    fn a_wide_window_fades_between_roots_and_from_the_index() {
        let mut model = model("settings://");
        model.set_wide(true);
        // The index shows the first root; choosing it changes nothing visible.
        let change = changed(model.switch_root(&loc("settings://network")));
        assert_eq!(change.transition, Transition::None);
        let change = changed(model.switch_root(&loc("settings://sound")));
        assert_eq!(change.transition, Transition::Fade);
    }

    #[test]
    fn reset_drops_everything() {
        let mut model = model("settings://network/wifi");
        model.reset(&loc("settings://sound"));
        assert_eq!(uris(&model), ["settings://", "settings://sound"]);
        assert!(!model.can_forward());
    }

    #[test]
    fn up_pops_when_the_parent_is_below_and_replaces_otherwise() {
        let mut model = model("settings://network");
        model.push(&loc("settings://network/wifi"));
        let change = changed(model.up());
        assert_eq!(model.location(), &loc("settings://network"));
        assert_eq!(change.steps, 1);
        // Opened straight to a grandchild: Up replaces with the parent page.
        model.push(&loc("settings://network/wifi/network/Home"));
        let change = changed(model.up());
        assert_eq!(model.location(), &loc("settings://network/wifi"));
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(change.steps, 0);
    }

    #[test]
    fn navigate_to_a_page_in_the_stack_pops_to_it() {
        let mut model = model("settings://network/wifi/saved/Home");
        let change = changed(model.navigate(&loc("settings://network/wifi")));
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(uris(&model), ["settings://", "settings://network", "settings://network/wifi"]);
    }

    #[test]
    fn navigate_keeps_the_shared_ancestors_and_pushes_the_rest() {
        let mut model = model("settings://network/wifi");
        let kept = model.back_entries()[1].id;
        let change = changed(model.navigate(&loc("settings://network/proxy")));
        assert_eq!(change.transition, Transition::Push);
        assert_eq!(uris(&model), ["settings://", "settings://network", "settings://network/proxy"]);
        assert_eq!(model.back_entries()[1].id, kept, "the shared entry keeps its state");
        let change = changed(model.navigate(&loc("settings://network/wifi/saved/Home")));
        assert_eq!(model.location(), &loc("settings://network/wifi/saved/Home"));
        assert_eq!(model.back_entries().len(), 4, "index, network, wifi, saved");
        assert!(change.removed.iter().any(|entry| entry.location == loc("settings://network/proxy")));
    }

    #[test]
    fn navigate_to_another_root_fades_and_builds_that_roots_stack() {
        let mut model = model("settings://network/wifi");
        let change = changed(model.navigate(&loc("settings://sound/applications")));
        assert_eq!(change.transition, Transition::Fade);
        assert_eq!(
            uris(&model),
            ["settings://", "settings://sound", "settings://sound/applications"]
        );
    }

    #[test]
    fn navigate_with_an_alias_scheme_or_a_fragment() {
        let table = Arc::new(settings().alias_scheme("bingux-settings"));
        let mut model = NavModel::new(table, &loc("bingux-settings://network"));
        model.set_wide(false);
        assert_eq!(model.location(), &loc("settings://network"));
        let change = changed(model.navigate(&loc("bingux-settings://network/proxy#method")));
        assert_eq!(change.to.location.fragment(), Some("method"));
        // The same place with another fragment is an anchor, not a move.
        assert_eq!(
            model.navigate(&loc("settings://network/proxy#other")),
            Outcome::Anchored(Some("other".into()))
        );
        assert_eq!(model.navigate(&loc("settings://network/proxy#other")), Outcome::Unchanged);
    }

    #[test]
    fn unknown_locations_open_a_not_found_page_you_can_leave() {
        let mut model = model("settings://network");
        let change = changed(model.navigate(&loc("settings://network/nowhere/deep")));
        assert_eq!(model.table().resolve(&change.to.location), None);
        assert_eq!(model.title(&change.to), "Not found");
        // The stack under it is what exists, so Back works.
        assert_eq!(uris(&model), ["settings://", "settings://network", "settings://network/nowhere/deep"]);
        assert!(matches!(model.back(), Outcome::Changed(_)));
        let mut model = self::model("settings://");
        assert!(matches!(model.navigate(&loc("settings://zzz")), Outcome::Changed(_)));
        assert_eq!(model.table().up(model.location()), Some(loc("settings://")));
    }

    #[test]
    fn query_only_changes_replace_in_place_and_never_slide() {
        let mut model = model("settings://sound/devices");
        let id = model.current().id;
        let depth = model.back_entries().len();
        let change = changed(model.push(&loc("settings://sound/devices?view=grid")));
        assert_eq!(change.transition, Transition::None);
        assert_eq!(model.current().id, id, "the entry and its state stay");
        assert_eq!(model.back_entries().len(), depth);
        assert_eq!(model.location().query("view"), Some("grid"));
        // Back to the default drops the key.
        model.push(&loc("settings://sound/devices?view=list"));
        assert_eq!(model.location(), &loc("settings://sound/devices"));
    }

    #[test]
    fn a_query_a_route_declares_as_pushing_adds_a_history_entry() {
        let table = Arc::new(settings().route("sound/log").query_pushes("page", "1"));
        let mut model = NavModel::new(table, &loc("settings://sound/log"));
        model.set_wide(false);
        let depth = model.back_entries().len();
        let change = changed(model.push(&loc("settings://sound/log?page=2")));
        assert_eq!(change.transition, Transition::None);
        assert_eq!(model.back_entries().len(), depth + 1);
        assert!(matches!(model.pop(), Outcome::Changed(_)));
        assert_eq!(model.location(), &loc("settings://sound/log"));
    }

    #[test]
    fn tabs_replace_their_siblings() {
        let table = Arc::new(settings().tab("sound/streams").title("Streams"));
        let mut model = NavModel::new(table, &loc("settings://sound/devices"));
        model.set_wide(false);
        let depth = model.back_entries().len();
        let change = changed(model.push(&loc("settings://sound/streams")));
        assert_eq!(change.transition, Transition::Fade);
        assert_eq!(model.back_entries().len(), depth);
    }

    #[test]
    fn an_ephemeral_page_is_skipped_by_back() {
        let mut model = model("settings://sound");
        model.push(&loc("settings://sound/applications"));
        let depth = model.back_entries().len();
        model.push(&loc("settings://sound/devices"));
        assert_eq!(model.back_entries().len(), depth, "the ephemeral entry was replaced");
        assert_eq!(uris(&model), ["settings://", "settings://sound", "settings://sound/devices"]);
    }

    #[test]
    fn forward_is_off_in_a_hierarchy() {
        let mut model = model("settings://network/wifi");
        model.pop();
        assert!(!model.can_forward());
        assert_eq!(model.forward(), Outcome::Unchanged);
    }

    fn linear(start: &str) -> NavModel {
        let table = RouteTable::new("file").rooted().mode(Mode::Linear).root("*path");
        NavModel::new(Arc::new(table), &loc(start))
    }

    #[test]
    fn a_linear_app_has_back_and_forward() {
        let mut model = linear("file:///home/me");
        assert!(!model.can_back());
        let change = changed(model.navigate(&loc("file:///home/me/Documents")));
        assert_eq!(change.transition, Transition::None);
        model.navigate(&loc("file:///etc"));
        model.navigate(&loc("file:///usr"));
        assert_eq!(model.back_entries().len(), 3);
        model.back();
        model.back();
        assert_eq!(model.location(), &loc("file:///home/me/Documents"));
        assert!(model.can_forward());
        assert_eq!(model.forward_menu().len(), 2);
        assert_eq!(model.forward_menu()[0].title, "etc");
        let change = changed(model.forward());
        assert_eq!(change.to.location, loc("file:///etc"));
        // A new place clears what was ahead.
        model.navigate(&loc("file:///var"));
        assert!(!model.can_forward());
    }

    #[test]
    fn forward_to_crosses_entries_in_one_step_and_keeps_the_rest() {
        let mut model = linear("file:///a");
        for next in ["file:///b", "file:///c", "file:///d"] {
            model.navigate(&loc(next));
        }
        let first = model.back_entries()[0].id;
        let change = changed(model.pop_to(first));
        assert_eq!(change.steps, 3);
        assert_eq!(model.forward_menu().len(), 3);
        let far = model.forward_menu()[2].entry;
        assert_eq!(model.forward_menu()[0].title, "b");
        let change = changed(model.forward_to(far));
        assert_eq!(change.steps, 3);
        assert_eq!(model.location(), &loc("file:///d"));
        let titles: Vec<_> = model.back_menu().into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["c", "b", "a"]);
        assert!(!model.can_forward());
    }

    #[test]
    fn navigating_to_the_place_you_are_at_does_nothing() {
        let mut model = linear("file:///a");
        assert_eq!(model.navigate(&loc("file:///a")), Outcome::Unchanged);
        assert!(!model.can_back());
    }

    #[test]
    fn the_history_menu_lists_the_path_back_nearest_first() {
        let mut model = model("settings://network");
        model.push(&loc("settings://network/wifi"));
        model.push_titled(&loc("settings://network/wifi/network/Home"), "Home (5 GHz)");
        let menu = model.back_menu();
        let titles: Vec<_> = menu.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["Wi-Fi", "Network", "Settings"]);
        assert_eq!(menu.iter().map(|item| item.steps).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(menu[1].icon, Some(Icon::Network));
        assert_eq!(model.title(model.current()), "Home (5 GHz)");
        // Choosing the second row is one pop of two entries.
        let change = changed(model.pop_to(menu[1].entry));
        assert_eq!(change.steps, 2);
        assert_eq!(change.transition, Transition::Pop);
        assert_eq!(model.location(), &loc("settings://network"));
    }

    #[test]
    fn a_one_entry_stack_shows_one_row() {
        let mut model = model("settings://sound");
        model.set_wide(true);
        model.push(&loc("settings://sound/applications"));
        let menu = model.back_menu();
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].title, "Sound");
    }

    #[test]
    fn a_page_can_set_its_own_title() {
        let mut model = model("settings://network/wifi/network/Home");
        let id = model.current().id;
        assert_eq!(model.title(model.current()), "Home");
        assert!(model.set_title(id, "Home 5 GHz"));
        assert!(!model.set_title(id, "Home 5 GHz"));
        assert_eq!(model.title(model.current()), "Home 5 GHz");
        assert_eq!(model.window_title("Settings"), "Home 5 GHz \u{2013} Settings");
        assert_eq!(model.title(&model.back_entries()[0]), "Settings");
    }

    #[test]
    fn modals_are_dismissed_before_pages_are_left() {
        let mut model = model("settings://network/wifi");
        let menu = model.open_modal(ModalKind::Menu, "");
        let dialog = model.open_modal(ModalKind::Dialog, "Forget?");
        assert!(model.can_back());
        assert_eq!(model.back(), Outcome::DismissModal(dialog));
        model.close_modal(dialog);
        assert_eq!(model.back(), Outcome::DismissModal(menu));
        model.close_modal(menu);
        assert!(matches!(model.back(), Outcome::Changed(_)));
        assert_eq!(model.location(), &loc("settings://network"));
    }

    #[test]
    fn a_modal_gives_a_root_a_back_even_on_a_wide_window() {
        let mut model = model("settings://network");
        model.set_wide(true);
        assert!(!model.can_back());
        let id = model.open_modal(ModalKind::Sheet, "");
        assert!(model.can_back());
        model.close_modal(id);
        assert!(!model.can_back());
    }

    #[test]
    fn an_open_modal_never_changes_what_the_header_shows() {
        // At a root: no Back button with or without a menu open.
        let mut model = model("settings://network");
        model.set_wide(true);
        let before = (model.can_go_back(), model.back_menu(), model.title(model.current()));
        let menu = model.open_modal(ModalKind::Menu, "");
        assert!(!model.can_go_back());
        assert_eq!(
            (model.can_go_back(), model.back_menu(), model.title(model.current())),
            before
        );
        // Back still dismisses the menu first.
        assert_eq!(model.back(), Outcome::DismissModal(menu));
        model.close_modal(menu);
        // At a subpage: the Back button stays visible, the same menu, same title.
        model.push(&loc("settings://network/wifi"));
        let before = (model.can_go_back(), model.back_menu(), model.title(model.current()), model.window_title("Settings"));
        assert!(before.0);
        let menu = model.open_modal(ModalKind::Menu, "");
        assert_eq!(
            (model.can_go_back(), model.back_menu(), model.title(model.current()), model.window_title("Settings")),
            before
        );
        model.close_modal(menu);
    }

    #[test]
    fn a_blocking_guard_keeps_the_page() {
        let mut model = model("settings://network/wifi");
        let id = model.current().id;
        model.set_guard(id, Some(Rc::new(|| PopGuard::Block)));
        for outcome in [model.pop(), model.back(), model.switch_root(&loc("settings://sound"))] {
            assert!(matches!(outcome, Outcome::Vetoed(PopGuard::Block, _)), "{outcome:?}");
        }
        assert_eq!(model.location(), &loc("settings://network/wifi"));
        model.set_guard(id, None);
        assert!(matches!(model.pop(), Outcome::Changed(_)));
    }

    #[test]
    fn a_confirming_guard_leaves_only_when_forced() {
        let mut model = model("settings://network/wifi");
        let id = model.current().id;
        let ask = Confirmation {
            title: "Discard changes?".into(),
            body: "Unsaved edits will be lost.".into(),
            discard: "Discard".into(),
        };
        let asked = ask.clone();
        model.set_guard(id, Some(Rc::new(move || PopGuard::Confirm(asked.clone()))));
        let Outcome::Vetoed(PopGuard::Confirm(question), op) = model.pop() else {
            panic!("a confirmation was expected");
        };
        assert_eq!(question, ask);
        assert_eq!(model.location(), &loc("settings://network/wifi"));
        assert!(matches!(model.apply(*op, true), Outcome::Changed(_)));
        assert_eq!(model.location(), &loc("settings://network"));
    }

    #[test]
    fn a_guard_deeper_in_a_multi_level_pop_still_asks() {
        let mut model = model("settings://network");
        model.push(&loc("settings://network/wifi"));
        let wifi = model.current().id;
        model.push(&loc("settings://network/wifi/network/Home"));
        model.set_guard(wifi, Some(Rc::new(|| PopGuard::Block)));
        let target = model.back_entries()[1].id;
        assert!(matches!(model.pop_to(target), Outcome::Vetoed(PopGuard::Block, _)));
        // Popping one level does not leave the guarded page.
        assert!(matches!(model.pop(), Outcome::Changed(_)));
    }

    #[test]
    fn the_snapshot_lists_route_ids_and_params() {
        let mut model = model("settings://network/wifi/network/Home?x=1");
        model.open_modal(ModalKind::Dialog, "Forget Home?");
        let snapshot = model.snapshot();
        assert_eq!(snapshot.back.len(), 3);
        assert_eq!(snapshot.current.route, "network/wifi/network/:ssid");
        assert_eq!(snapshot.current.params, vec![("ssid".to_string(), "Home".to_string())]);
        assert_eq!(snapshot.current.uri, "settings://network/wifi/network/Home?x=1");
        assert_eq!(snapshot.modals.len(), 1);
        assert_eq!(snapshot.back[0].route, "");
    }
}
