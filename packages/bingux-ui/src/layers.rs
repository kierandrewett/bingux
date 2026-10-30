//! What is above what: one table of paint layers and one model of window stacking.
//!
//! # Paint layers (inside a window)
//!
//! Freya paints a node at its parent's layer plus its own `Layer::Relative` plus its depth, so a
//! deep node of a shallow page can outrank a shallow sibling. Every layer number the library uses
//! is named here, in one order:
//!
//! content < docked panes < tab strip < slide header < back veil < header bar < slide sheet <
//! slide controls < back shield < (overlay levels) menu < popup sheet < dialog < toast <
//! menu over a dialog.
//!
//! The overlay levels (`Layer::OverlayLevel`) are absolute and win over every relative layer; the
//! relative ones order the page itself.
//!
//! # Windows
//!
//! [`WindowStack`] is the model of how windows stack: a tree (a popup has a parent, a dialog has a
//! parent) kept as one bottom-to-top order. The rules:
//!
//! 1. A window opens directly above the last window of its parent's family (the parent and
//!    everything already open from it), so a popup opened from a sub-window is above that
//!    sub-window and a dialog opened from a popup is above that popup.
//! 2. A window with no parent (an app window, the inspector, a tear-off carry window) opens on top
//!    of everything. It is an independent toplevel: its own popups sit above it, not above others.
//! 3. A menu or popup is above every non-popup window of its family: nothing of the app covers it.
//! 4. Closing a window closes its family below it; the window below it in the order is on top again.
//! 5. Focusing or raising a window raises its family root with everything open from it, keeping
//!    their order, so a child is never behind its parent.
//! 6. A modal window blocks its ancestors until it closes.
//!
//! `BINGUX_DEBUG_LAYERS=1` prints the stack on every change ([`WindowStack::dump`]).

use freya::prelude::Layer;

// Relative layers, lowest first.
/// A tab being dragged in a strip: above the tabs it slides across.
pub(crate) const TAB_HELD: i16 = 32;
/// Where a dragged-in tab would land, above all of them.
pub(crate) const TAB_MARKER: i16 = TAB_HELD + 1;
/// A moving sheet's own header strip.
pub(crate) const SLIDE_HEADER: i16 = 500;
/// The veil over the screen beneath a predictive-back drag.
pub(crate) const BACK_VEIL: i16 = 1000;
/// A popup's outline, drawn over everything the popup holds.
pub(crate) const POPUP_OUTLINE: i16 = 1000;
/// The floating header bar: above the deepest page content, below overlays.
pub(crate) const HEADER: i16 = 1500;

// Floating cards: `Layer::Overlay` is a big jump above the parent's layer, so a card painted on it
// is above everything its parent holds. Each use has its own name, so a change to one is a change
// to one thing.
/// A menu, dropdown list or popover surface, its shield and its query pill, drawn in its window.
pub(crate) const PANEL: Layer = Layer::Overlay;
/// A tooltip.
pub(crate) const TOOLTIP: Layer = Layer::Overlay;
/// The Files hover card.
pub(crate) const HOVER_CARD: Layer = Layer::Overlay;
/// An editor that floats over its content: the rename field, the text edit bar.
pub(crate) const INLINE_EDITOR: Layer = Layer::Overlay;
/// A status pill over a view.
pub(crate) const STATUS_PILL: Layer = Layer::Overlay;
/// A floating card of a view (drag badge, banner) over its rows.
pub(crate) const VIEW_CARD: Layer = Layer::Overlay;
/// The controls a viewer floats over its picture or player.
pub(crate) const VIEWER_CONTROLS: Layer = Layer::Overlay;
/// What a drag carries: above the list it is dragged over.
pub(crate) const DRAG_CARRY: Layer = Layer::Overlay;
/// The sheet that slides over the window header.
pub(crate) const SLIDE_SHEET: i16 = 1600;
/// The window buttons lifted over a moving sheet.
pub(crate) const SLIDE_CONTROLS: i16 = 2600;
/// The shield that takes the touch while a back drag runs.
pub(crate) const BACK_SHIELD: i16 = 3000;

// Overlay levels, lowest first (`Layer::OverlayLevel`).
/// A menu, dropdown list or popover drawn in its window.
pub(crate) const LEVEL_MENU: u8 = 8;
/// An in-window popup (sub-window sheet).
pub(crate) const LEVEL_POPUP: u8 = 10;
/// A dialog.
pub(crate) const LEVEL_DIALOG: u8 = 12;
/// A toast.
pub(crate) const LEVEL_TOAST: u8 = 13;
/// A menu opened from inside a dialog: above it.
pub(crate) const LEVEL_MENU_OVER_DIALOG: u8 = 14;

pub(crate) const fn level(level: u8) -> Layer {
    Layer::OverlayLevel(level)
}

/// How far each level of a menu's cascade sits above the one before it, in relative layers.
/// The cards of a root menu and its submenus are siblings, and a card's rows sit a few layers
/// *above* the card itself (one per level of depth), so two cards on one layer would let the
/// earlier card's labels paint over the later card's fill wherever they overlap. A step wider
/// than the deepest card keeps every level wholly above the one before it.
pub(crate) const CASCADE_STEP: i16 = 256;

/// The most cascade levels that get a layer of their own; a deeper one shares the last.
const CASCADE_LEVELS: usize = 24;

/// The layer a submenu at `level` of its cascade paints on, relative to the shield it sits in
/// (the root menu is level 0, its first submenu 1).
pub(crate) fn cascade(level: usize) -> Layer {
    Layer::Relative(level.min(CASCADE_LEVELS) as i16 * CASCADE_STEP)
}

/// What kind of window an entry is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowKind {
    /// An app window, a second window, a tear-off carry window.
    Toplevel,
    /// A dialog or chooser window: above its parent, usually modal.
    Dialog,
    /// A tool window (the inspector, a palette): above its parent, never modal.
    Tool,
    /// A popup window: a menu, dropdown list, popover.
    Popup,
}

impl WindowKind {
    fn is_popup(self) -> bool {
        self == Self::Popup
    }
}

/// One window in the stack.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StackEntry {
    pub id: u64,
    pub kind: WindowKind,
    pub parent: Option<u64>,
    pub modal: bool,
    pub name: String,
}

/// The windows of a process, bottom to top.
#[derive(Clone, Default, Debug)]
pub struct WindowStack {
    order: Vec<StackEntry>,
}

impl WindowStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// The ids bottom to top.
    pub fn ids(&self) -> Vec<u64> {
        self.order.iter().map(|entry| entry.id).collect()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.index(id).is_some()
    }

    pub fn top(&self) -> Option<u64> {
        self.order.last().map(|entry| entry.id)
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.order.iter().position(|entry| entry.id == id)
    }

    /// `id` and every window open from it, in stacking order.
    fn family(&self, id: u64) -> Vec<u64> {
        let mut family = vec![id];
        let mut grew = true;
        while grew {
            grew = false;
            for entry in &self.order {
                if entry.parent.is_some_and(|parent| family.contains(&parent))
                    && !family.contains(&entry.id)
                {
                    family.push(entry.id);
                    grew = true;
                }
            }
        }
        family.sort_by_key(|member| self.index(*member));
        family
    }

    /// The window a family hangs from: follow parents to a window with none.
    fn root_of(&self, mut id: u64) -> u64 {
        while let Some(parent) = self
            .index(id)
            .and_then(|index| self.order[index].parent)
            .filter(|parent| self.index(*parent).is_some())
        {
            id = parent;
        }
        id
    }

    /// Opens a window. It goes above its parent's family (rule 1), on top of everything without
    /// a parent (rule 2), and a popup above every non-popup of its family (rule 3).
    pub fn open(&mut self, id: u64, kind: WindowKind, parent: Option<u64>, modal: bool, name: &str) {
        self.order.retain(|entry| entry.id != id);
        let parent = parent.filter(|parent| self.index(*parent).is_some());
        let entry = StackEntry { id, kind, parent, modal, name: name.to_string() };
        let at = match parent {
            None => self.order.len(),
            Some(parent) => {
                let family = self.family(parent);
                // The family's last window; a non-popup stops below the family's popups.
                let last = family.iter().filter_map(|member| self.index(*member)).max().unwrap_or(0);
                if kind.is_popup() {
                    last + 1
                } else {
                    let first_popup = family
                        .iter()
                        .filter_map(|member| self.index(*member))
                        .filter(|index| self.order[*index].kind.is_popup())
                        .min();
                    first_popup.unwrap_or(last + 1)
                }
            }
        };
        self.order.insert(at, entry);
        self.trace();
    }

    /// Opens a tool window (the inspector, a palette) as a child of `owner`: above it, closed
    /// with it, raised with it. An owner that is not in the stack yet (the application's first
    /// window is never registered by anything else) enters it as a toplevel first.
    pub fn open_tool(&mut self, id: u64, owner: u64, name: &str) {
        if !self.contains(owner) {
            self.open(owner, WindowKind::Toplevel, None, false, "window");
        }
        self.open(id, WindowKind::Tool, Some(owner), false, name);
    }

    /// Closes `id` and everything open from it (rule 4). Returns the ids closed, newest first.
    pub fn close(&mut self, id: u64) -> Vec<u64> {
        let mut doomed = self.family(id);
        doomed.reverse();
        self.order.retain(|entry| !doomed.contains(&entry.id));
        self.trace();
        doomed
    }

    /// Raises the family `id` belongs to (rule 5).
    pub fn raise(&mut self, id: u64) {
        let root = self.root_of(id);
        let family = self.family(root);
        let (mut moved, kept): (Vec<_>, Vec<_>) =
            self.order.drain(..).partition(|entry| family.contains(&entry.id));
        self.order = kept;
        self.order.append(&mut moved);
        self.trace();
    }

    /// Whether `id` is blocked by a modal window opened from it, directly or not (rule 6).
    pub fn blocked(&self, id: u64) -> bool {
        self.family(id)
            .iter()
            .any(|member| *member != id && self.index(*member).is_some_and(|i| self.order[i].modal))
    }

    /// One line per window, top first: `id kind parent modal name`.
    pub fn dump(&self) -> String {
        let mut lines = Vec::new();
        for (depth, entry) in self.order.iter().enumerate().rev() {
            lines.push(format!(
                "{depth:>2} #{} {:?} parent={} modal={} {}",
                entry.id,
                entry.kind,
                entry.parent.map_or("-".to_string(), |parent| format!("#{parent}")),
                entry.modal,
                entry.name
            ));
        }
        lines.join("\n")
    }

    fn trace(&self) {
        if std::env::var_os("BINGUX_DEBUG_LAYERS").is_some() {
            eprintln!("[layers]\n{}", self.dump());
        }
    }
}

thread_local! {
    static STACK: std::cell::RefCell<WindowStack> = std::cell::RefCell::new(WindowStack::new());
}

/// Runs `change` on this process's window stack. The windows register themselves here
/// (see `popup_windows::backend` and `app::open_window`).
pub fn with_stack<T>(change: impl FnOnce(&mut WindowStack) -> T) -> T {
    STACK.with(|stack| change(&mut stack.borrow_mut()))
}

/// The process's window stack as text, for the inspector and `BINGUX_DEBUG_LAYERS`.
pub fn dump_stack() -> String {
    STACK.with(|stack| stack.borrow().dump())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> WindowStack {
        let mut stack = WindowStack::new();
        stack.open(1, WindowKind::Toplevel, None, false, "Files");
        stack
    }

    #[test]
    fn overlay_levels_are_ordered() {
        assert!(LEVEL_MENU < LEVEL_POPUP && LEVEL_POPUP < LEVEL_DIALOG);
        assert!(LEVEL_DIALOG < LEVEL_TOAST && LEVEL_TOAST < LEVEL_MENU_OVER_DIALOG);
        assert!(TAB_HELD < TAB_MARKER);
        assert!(SLIDE_HEADER < BACK_VEIL && BACK_VEIL <= POPUP_OUTLINE && POPUP_OUTLINE < HEADER);
        assert!(HEADER < SLIDE_SHEET && SLIDE_SHEET < SLIDE_CONTROLS && SLIDE_CONTROLS < BACK_SHIELD);
    }

    #[test]
    fn a_popup_opens_above_its_parent_and_closing_returns_to_it() {
        let mut stack = app();
        stack.open(2, WindowKind::Popup, Some(1), false, "menu");
        stack.open(3, WindowKind::Popup, Some(2), false, "submenu");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
        assert_eq!(stack.close(3), vec![3]);
        assert_eq!(stack.top(), Some(2));
    }

    #[test]
    fn closing_a_popup_closes_what_grew_from_it_newest_first() {
        let mut stack = app();
        stack.open(2, WindowKind::Popup, Some(1), false, "menu");
        stack.open(3, WindowKind::Popup, Some(2), false, "submenu");
        assert_eq!(stack.close(2), vec![3, 2]);
        assert_eq!(stack.ids(), vec![1]);
    }

    #[test]
    fn a_popup_opened_from_a_sub_window_is_above_that_sub_window() {
        let mut stack = app();
        stack.open(2, WindowKind::Dialog, Some(1), false, "chooser");
        stack.open(3, WindowKind::Popup, Some(2), false, "dropdown");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
    }

    #[test]
    fn a_dialog_opened_over_a_popup_family_stays_below_the_popups() {
        // Menus are above everything of their app: a dialog the app opens from the window
        // goes above the window but not above its open menu.
        let mut stack = app();
        stack.open(2, WindowKind::Popup, Some(1), false, "menu");
        stack.open(3, WindowKind::Dialog, Some(1), true, "About");
        assert_eq!(stack.ids(), vec![1, 3, 2]);
    }

    #[test]
    fn a_chooser_over_a_dialog_is_above_the_dialog() {
        let mut stack = app();
        stack.open(2, WindowKind::Dialog, Some(1), true, "Preferences");
        stack.open(3, WindowKind::Dialog, Some(2), true, "Choose a file");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
        assert!(stack.blocked(1));
        assert!(stack.blocked(2));
        assert!(!stack.blocked(3));
        stack.close(3);
        assert!(!stack.blocked(2));
        assert!(stack.blocked(1));
    }

    #[test]
    fn independent_toplevels_open_on_top_and_their_popups_sit_above_them() {
        let mut stack = app();
        stack.open(2, WindowKind::Toplevel, None, false, "Second window");
        stack.open(3, WindowKind::Popup, Some(1), false, "files menu");
        stack.open(4, WindowKind::Popup, Some(2), false, "second menu");
        // Each popup is above its own window; the second window was raised first.
        let ids = stack.ids();
        assert!(ids.iter().position(|id| *id == 3) > ids.iter().position(|id| *id == 1));
        assert!(ids.iter().position(|id| *id == 4) > ids.iter().position(|id| *id == 2));
    }

    #[test]
    fn an_inspector_is_a_tool_window_above_its_target() {
        let mut stack = WindowStack::new();
        // The target is not registered by anything else: the inspector brings it in.
        stack.open_tool(2, 1, "inspector");
        assert_eq!(stack.ids(), vec![1, 2]);
        assert_eq!(stack.order[1].kind, WindowKind::Tool);
        assert_eq!(stack.order[1].parent, Some(1));
        assert!(!stack.order[1].modal && !stack.blocked(1), "an inspector never blocks its target");
        // A dialog the target opens afterwards goes above the target and its inspector's family.
        stack.open(3, WindowKind::Dialog, Some(1), true, "About");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
    }

    #[test]
    fn menus_of_the_target_and_of_the_inspector_are_above_both() {
        let mut stack = app();
        stack.open_tool(2, 1, "inspector");
        stack.open(3, WindowKind::Popup, Some(1), false, "files menu");
        stack.open(4, WindowKind::Popup, Some(2), false, "inspector menu");
        let ids = stack.ids();
        let at = |id: u64| ids.iter().position(|member| *member == id).expect("in the stack");
        // A popup is above every non-popup of its family: a menu of the target is above the
        // inspector that watches it, and the inspector's own is above both windows.
        assert!(at(3) > at(2) && at(3) > at(1));
        assert!(at(4) > at(2) && at(4) > at(1));
        // Closing the target's menu leaves the inspector and its menu where they were.
        stack.close(3);
        assert_eq!(stack.ids(), vec![1, 2, 4]);
    }

    #[test]
    fn raising_the_target_or_the_inspector_raises_both_and_closing_the_target_closes_it() {
        let mut stack = app();
        stack.open_tool(2, 1, "inspector");
        stack.open(3, WindowKind::Toplevel, None, false, "Tear-off");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
        stack.raise(2);
        assert_eq!(stack.ids(), vec![3, 1, 2], "the inspector is never behind its target");
        stack.raise(3);
        stack.raise(1);
        assert_eq!(stack.ids(), vec![3, 1, 2]);
        // The inspector of the inspector hangs from the inspector.
        stack.open_tool(4, 2, "inspector of inspector");
        assert_eq!(stack.close(1), vec![4, 2, 1], "newest first, the whole family");
        assert_eq!(stack.ids(), vec![3]);
    }

    #[test]
    fn focusing_the_parent_raises_its_children_with_it() {
        let mut stack = app();
        stack.open(2, WindowKind::Dialog, Some(1), false, "About");
        stack.open(3, WindowKind::Toplevel, None, false, "Tear-off");
        assert_eq!(stack.ids(), vec![1, 2, 3]);
        stack.raise(1);
        assert_eq!(stack.ids(), vec![3, 1, 2]);
        // Raising the child raises the whole family: the child is never behind its parent.
        stack.raise(3);
        stack.raise(2);
        assert_eq!(stack.ids(), vec![3, 1, 2]);
    }

    #[test]
    fn opening_the_same_id_again_moves_it() {
        let mut stack = app();
        stack.open(2, WindowKind::Popup, Some(1), false, "a");
        stack.open(2, WindowKind::Popup, Some(1), false, "a");
        assert_eq!(stack.ids(), vec![1, 2]);
    }

    #[test]
    fn a_parent_that_is_gone_makes_a_toplevel() {
        let mut stack = app();
        stack.open(2, WindowKind::Dialog, Some(99), false, "orphan");
        assert_eq!(stack.ids(), vec![1, 2]);
        assert_eq!(stack.order[1].parent, None);
    }

    #[test]
    fn the_dump_lists_the_stack_top_first() {
        let mut stack = app();
        stack.open(2, WindowKind::Popup, Some(1), false, "menu");
        let dump = stack.dump();
        let lines: Vec<&str> = dump.lines().collect();
        assert!(lines[0].contains("#2") && lines[0].contains("Popup") && lines[0].contains("parent=#1"));
        assert!(lines[1].contains("#1") && lines[1].contains("Files"));
    }
}
