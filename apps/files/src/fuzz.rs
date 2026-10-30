//! Random walks over the windows, tabs and histories of Files, checked after
//! every step against a model of what each tab's history must be.
//!
//! The real [`TabSet`] and [`TabMemory`] run, with the same functions the
//! window uses to move a history (`bingux_ui::files::history`). Beside them a
//! reference keeps, for every tab, a browser's list of places and an index,
//! and what *Reopen closed tab* must bring back. A walk makes ordinary
//! moves in the tab on show and the things around it that must never touch a
//! history: opening, duplicating, closing and reopening tabs, switching,
//! reordering, and tearing tabs off into another window, or into a new one
//! and back. After every step the tabs' histories must be exactly what the
//! reference says, the selection must be a tab that exists, a tab's title must
//! be its folder, and the closed tabs must stay bounded.
//! (`bingux_ui::files::history_fuzz` walks one history alone.)

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use bingux_ui::files::history::{self, Trail};
use bingux_ui::files::{ClosedTabs, TabMemory, Where};
use bingux_ui::nav::HISTORY_LIMIT;

use crate::tabset::{Closed, TabSet};

/// How many random walks are made.
const WALKS: u64 = 10_000;
/// How many operations each makes.
const STEPS: usize = 80;
/// The tabs a walk may have in one window at most.
const MOST_TABS: usize = 8;

/// What the walks did, so walks that did nothing cannot pass: one counter
/// for each thing a tab's history must survive.
const CLOSED: usize = 0;
const REOPENED: usize = 1;
const DUPLICATED: usize = 2;
const CLOSED_MANY: usize = 3;
const NEW_WINDOW: usize = 4;
const ACROSS: usize = 5;
const MERGED: usize = 6;
const SPLIT: usize = 7;
static SEEN: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];

fn seen(kind: usize) {
    SEEN[kind].fetch_add(1, Ordering::Relaxed);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn places() -> Vec<Where> {
    let mut all: Vec<Where> = ["/", "/home/me", "/home/me/Documents", "/home/me/Documents/a", "/tmp", "/tmp/x y", "/tmp/-x", "/var"]
        .iter()
        .map(|path| Where::Folder(PathBuf::from(path)))
        .collect();
    all.push(Where::Recent);
    all
}

/// What a person or a command line might type that is not a place.
const NOT_PLACES: [&str; 8] = ["", "--snapshot", "-h", "rel", ".", "..", "~", "~/x"];

/// A browser's history: places and where in them it is.
#[derive(Clone, Debug)]
struct Reference {
    chain: Vec<Where>,
    at: usize,
}

impl Reference {
    fn new(start: Where) -> Self {
        Self { chain: vec![start], at: 0 }
    }

    fn visit(&mut self, to: &Where) {
        if !matches!(to, Where::Recent) && !matches!(to, Where::Folder(path) if path.is_absolute()) {
            return;
        }
        if *to == self.chain[self.at] {
            return;
        }
        self.chain.truncate(self.at + 1);
        self.chain.push(to.clone());
        if self.chain.len() > HISTORY_LIMIT + 1 {
            self.chain.remove(0);
        }
        self.at = self.chain.len() - 1;
    }

    fn up(&mut self) {
        if let Where::Folder(path) = &self.chain[self.at]
            && let Some(parent) = path.parent()
        {
            let parent = Where::Folder(parent.to_path_buf());
            self.visit(&parent);
        }
    }

    fn trail(&self) -> Trail {
        Trail {
            back: self.chain[..self.at].to_vec(),
            current: self.chain[self.at].clone(),
            forward: self.chain[self.at + 1..].to_vec(),
        }
    }
}

/// One window: its tabs, the reference history of each, and what the
/// reference says Reopen closed tab has.
struct Window {
    set: TabSet,
    refs: Vec<(u64, Reference)>,
    closed: Vec<(usize, Reference)>,
}

impl Window {
    fn new(at: Where) -> Self {
        let set = TabSet::new(at.clone());
        let id = set.tabs()[0].id;
        Window { set, refs: vec![(id, Reference::new(at))], closed: Vec::new() }
    }

    fn reference(&mut self, id: u64) -> &mut Reference {
        &mut self.refs.iter_mut().find(|(other, _)| *other == id).expect("a reference for the tab").1
    }

    fn take_reference(&mut self, id: u64) -> Reference {
        let index = self.refs.iter().position(|(other, _)| *other == id).expect("a reference for the tab");
        self.refs.remove(index).1
    }

    fn remember_closed(&mut self, index: usize, reference: Reference) {
        self.closed.push((index, reference));
        if self.closed.len() > ClosedTabs::LIMIT {
            self.closed.remove(0);
        }
    }

    /// Run `edit` on the history of the tab on show, as the file view does:
    /// on a copy that is then kept, with the tab's title following.
    fn edit(&mut self, edit: impl FnOnce(&mut bingux_ui::nav::NavModel)) {
        let tab = self.set.current().expect("a tab on show").clone();
        let mut model = tab.memory.model();
        edit(&mut model);
        let at = bingux_ui::files::place_of(model.location());
        tab.memory.set(model);
        self.set.moved(tab.id, at);
    }
}

/// Every window's tabs have exactly the histories the references have.
fn check(windows: &[Window], what: &str) {
    for (number, window) in windows.iter().enumerate() {
        let tabs = window.set.tabs();
        assert!(!tabs.is_empty(), "window {number} has a tab, after {what}");
        let ids = window.set.ids();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "tab ids are unique, after {what}");
        assert!(ids.contains(&window.set.active()), "the selected tab exists, after {what}");
        assert!(tabs.len() <= 40, "tab count stays sane, after {what}");
        assert_eq!(window.refs.len(), tabs.len(), "a reference for every tab, after {what}");
        assert_eq!(window.set.can_reopen(), !window.closed.is_empty(), "Reopen is offered when a tab is closed, after {what}");
        assert!(window.closed.len() <= ClosedTabs::LIMIT, "closed tabs are bounded, after {what}");
        for tab in tabs {
            let reference = &window.refs.iter().find(|(id, _)| *id == tab.id).expect("a reference").1;
            let trail = Trail::of(&tab.memory.model());
            assert_eq!(trail, reference.trail(), "the history of tab {} in window {number}, after {what}", tab.id);
            assert_eq!(tab.at, trail.current, "the tab's folder is its history's, after {what}");
        }
        // Tabs never share a history: two memories are two lists.
        for (index, one) in tabs.iter().enumerate() {
            for other in &tabs[index + 1..] {
                assert!(one.memory != other.memory, "two tabs share one memory, after {what}");
            }
        }
    }
}

fn walk(seed: u64) {
    let all = places();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let start = all[rng.below(all.len())].clone();
    let mut windows = vec![Window::new(start)];
    for step in 0..STEPS {
        let which = rng.below(windows.len());
        let operation = rng.below(24);
        let tab_count = windows[which].set.len();
        let pick = |rng: &mut Rng, window: &Window| window.set.tabs()[rng.below(window.set.len())].id;
        let what = format!("step {step} (op {operation}) of seed {seed}");
        let window = &mut windows[which];
        match operation {
            // Moves in the tab on show.
            0..=4 => {
                let to = all[rng.below(all.len())].clone();
                let id = window.set.active();
                window.reference(id).visit(&to);
                window.edit(|model| {
                    history::visit(model, &to);
                });
            }
            5 => {
                let to = Where::Folder(PathBuf::from(NOT_PLACES[rng.below(NOT_PLACES.len())]));
                window.edit(|model| {
                    history::visit(model, &to);
                });
            }
            6 => {
                let id = window.set.active();
                window.reference(id).up();
                window.edit(|model| {
                    history::up(model);
                });
            }
            7 | 8 => {
                let id = window.set.active();
                let reference = window.reference(id);
                reference.at = reference.at.saturating_sub(1);
                window.edit(|model| {
                    model.back();
                });
            }
            9 => {
                let id = window.set.active();
                let reference = window.reference(id);
                reference.at = (reference.at + 1).min(reference.chain.len() - 1);
                window.edit(|model| {
                    model.forward();
                });
            }
            10 => {
                let id = window.set.active();
                let menu = window.set.current().expect("a tab").memory.model().back_menu();
                if let Some(row) = menu.get(rng.below(menu.len().max(1))) {
                    let reference = window.reference(id);
                    reference.at -= row.steps;
                    let entry = row.entry;
                    window.edit(|model| {
                        model.pop_to(entry);
                    });
                }
            }
            11 => {
                let id = window.set.active();
                let menu = window.set.current().expect("a tab").memory.model().forward_menu();
                if let Some(row) = menu.get(rng.below(menu.len().max(1))) {
                    let reference = window.reference(id);
                    reference.at += row.steps;
                    let entry = row.entry;
                    window.edit(|model| {
                        model.forward_to(entry);
                    });
                }
            }
            // The tabs around the histories.
            12 if tab_count < MOST_TABS => {
                let at = all[rng.below(all.len())].clone();
                let id = window.set.open(at.clone());
                window.refs.push((id, Reference::new(at)));
            }
            13 | 14 => {
                let id = pick(&mut rng, window);
                let index = window.set.tabs().iter().position(|tab| tab.id == id).expect("a tab");
                if window.set.close(id) == Closed::Tab {
                    let reference = window.take_reference(id);
                    window.remember_closed(index, reference);
                    seen(CLOSED);
                }
            }
            15 if tab_count < MOST_TABS => {
                let id = pick(&mut rng, window);
                if let Some(new) = window.set.duplicate(id) {
                    let copy = window.reference(id).clone();
                    window.refs.push((new, copy));
                    seen(DUPLICATED);
                }
            }
            16 | 17 => {
                if let Some(new) = window.set.reopen() {
                    let (_, reference) = window.closed.pop().expect("the reference has the closed tab");
                    window.refs.push((new, reference));
                    seen(REOPENED);
                }
            }
            18 => {
                let id = pick(&mut rng, window);
                window.set.select(id);
            }
            19 => window.set.step(if rng.below(2) == 0 { 1 } else { -1 }),
            20 => window.set.nth(rng.below(10)),
            21 => {
                let id = pick(&mut rng, window);
                window.set.reorder(id, rng.below(tab_count + 1));
            }
            22 => {
                // Close several: the menu's Close other tabs, or those to the right.
                let all_ids = window.set.ids();
                let chosen: Vec<u64> = all_ids.iter().copied().filter(|_| rng.below(2) == 0).collect();
                let before: Vec<(u64, usize)> = all_ids.iter().copied().enumerate().map(|(index, id)| (id, index)).collect();
                window.set.close_many(&chosen);
                let remaining = window.set.ids();
                // The tabs that went, the rightmost first, are what Reopen brings back
                // left to right.
                for (id, index) in before.iter().rev() {
                    if !remaining.contains(id) {
                        let reference = window.take_reference(*id);
                        window.remember_closed(*index, reference);
                        seen(CLOSED_MANY);
                    }
                }
            }
            // Tearing a tab off: into another window, or into one of its own.
            _ => {
                if tab_count < 2 {
                    continue;
                }
                let id = pick(&mut rng, window);
                let Some(moved) = window.set.detach(id) else { continue };
                let reference = window.take_reference(id);
                let target = rng.below(windows.len() + 1);
                if target == windows.len() || target == which || windows.len() == 1 && rng.below(2) == 0 {
                    // A window of its own.
                    let fresh = TabSet::adopt(vec![moved], 0).expect("a window with the tab");
                    let new = fresh.tabs()[0].id;
                    windows.push(Window { set: fresh, refs: vec![(new, reference)], closed: Vec::new() });
                    seen(NEW_WINDOW);
                } else {
                    let other = &mut windows[target];
                    let index = rng.below(other.set.len() + 1);
                    let new = other.set.insert_moved(index, vec![moved]);
                    other.refs.push((new[0], reference));
                    seen(ACROSS);
                }
            }
        }
        // A window of one tab can also merge into another, so histories travel
        // as whole windows too.
        if windows.len() > 1 && rng.below(10) == 0 {
            let from = rng.below(windows.len());
            let into = (from + 1) % windows.len();
            let mut gone = windows.remove(from);
            let into = if into > from { into - 1 } else { into };
            let ids = gone.set.ids();
            let moved = gone.set.detach_all();
            let refs: Vec<Reference> = ids.iter().map(|id| gone.take_reference(*id)).collect();
            let target = &mut windows[into];
            let index = rng.below(target.set.len() + 1);
            let new = target.set.insert_moved(index, moved);
            for (id, reference) in new.into_iter().zip(refs) {
                target.refs.push((id, reference));
            }
            seen(MERGED);
        }
        // Two windows with a few tabs each are the common case: split one
        // window's other tabs off now and then.
        if windows.len() < 3 && windows.last().is_some_and(|window| window.set.len() > 2) && rng.below(15) == 0 {
            let last = windows.len() - 1;
            let window = &mut windows[last];
            let keep = window.set.active();
            let before: Vec<u64> = window.set.ids().into_iter().filter(|id| *id != keep).collect();
            if let Some((moved, on_show)) = window.set.detach_others(keep) {
                let refs: Vec<Reference> = before.iter().map(|id| window.take_reference(*id)).collect();
                let fresh = TabSet::adopt(moved, on_show).expect("the other tabs");
                let ids = fresh.ids();
                windows.push(Window { set: fresh, refs: ids.into_iter().zip(refs).collect(), closed: Vec::new() });
                seen(SPLIT);
            }
        }
        check(&windows, &what);
    }
}

#[test]
fn ten_thousand_random_windows_never_lose_a_tabs_history() {
    for seed in 1..=WALKS {
        walk(seed);
    }
    let counts: Vec<usize> = SEEN.iter().map(|count| count.load(Ordering::Relaxed)).collect();
    eprintln!(
        "{WALKS} walks: closed {}, reopened {}, duplicated {}, closed several {}, torn into a new window {}, torn into another {}, windows merged {}, tabs split off {}",
        counts[CLOSED], counts[REOPENED], counts[DUPLICATED], counts[CLOSED_MANY], counts[NEW_WINDOW], counts[ACROSS], counts[MERGED], counts[SPLIT]
    );
    for (name, count) in [
        ("closed", CLOSED),
        ("reopened", REOPENED),
        ("duplicated", DUPLICATED),
        ("closed several", CLOSED_MANY),
        ("torn into a new window", NEW_WINDOW),
        ("torn into another window", ACROSS),
        ("windows merged", MERGED),
        ("tabs split off", SPLIT),
    ] {
        assert!(counts[count] > 1_000, "the walks {name} tabs {} times: too few to mean anything", counts[count]);
    }
}

#[test]
fn a_tab_that_was_never_navigated_has_one_place_and_no_menus() {
    let memory = TabMemory::new(Where::Folder("/home/me".into()));
    let model = memory.model();
    assert!(!model.can_go_back() && !model.can_forward());
    assert!(model.back_menu().is_empty() && model.forward_menu().is_empty());
}
