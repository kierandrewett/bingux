//! The window's focus model: regions, F6 cycling and popup focus traps.
//!
//! Tab and Shift+Tab move through every focusable control in visual order and
//! wrap; Freya does that natively over the accessibility tree. [`WindowHost`]
//! sends those moves, so Tab works in every window.
//!
//! Each region (sidebar, header, page content) starts with a zero-size
//! *sentinel*: a focusable node that sits before the region's first control.
//! Tab reaching a sentinel moves on, so the focus lands on the region's first
//! control. F6 and Shift+F6 focus the next or previous region's sentinel, in
//! keyboard mode, which bounces to the first control with the ring shown. A
//! click on empty space in a region focuses its sentinel in pointer mode, so
//! the next Tab continues from there and no ring shows.
//!
//! A popup trap ([`trap`]) has a sentinel at each end. Tab and Shift+Tab
//! cannot leave the popup, and when it closes, focus returns to the element
//! that opened it.
//!
//! Bounces are budgeted per key press, so a region with no controls cannot
//! loop forever.
//!
//! [`WindowHost`]: crate::window::WindowHost

use std::{cell::RefCell, rc::Rc};

use freya::prelude::*;

/// The regions of a Bingux window, in F6 order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    Sidebar,
    Header,
    Content,
}

/// F6 order, the same order as the regions appear down the window.
const ORDER: [RegionKind; 3] = [RegionKind::Sidebar, RegionKind::Header, RegionKind::Content];

/// How many sentinel bounces one key press may cause.
const BOUNCE_BUDGET: u8 = 8;

/// The direction the last Tab or F6 moved focus in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Dir {
    #[default]
    Forward,
    Backward,
}

/// What a sentinel is for.
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    /// The start of a region. Keyboard focus passes through it.
    Region(RegionKind),
    /// The start of a popup trap, with the id of the trap's end sentinel.
    TrapStart(AccessibilityId),
    /// The end of a popup trap, with the id of the trap's start sentinel.
    TrapEnd(AccessibilityId),
}

/// What a keyboard landing on a sentinel does next.
enum Step {
    /// Send Tab in this direction, to move on from the sentinel.
    Bounce(Dir),
    /// Focus this sentinel instead, which keeps focus inside a trap.
    Enter(AccessibilityId),
}

#[derive(Default)]
struct Core {
    dir: Dir,
    budget: u8,
    current: Option<RegionKind>,
    claimed: bool,
    open_traps: usize,
    starts: Vec<(RegionKind, AccessibilityId)>,
    /// The control pressed last, for a popup it opens to return focus to.
    pressed: Option<AccessibilityId>,
    /// The start a pointer press parked focus on. Freya switches to keyboard
    /// mode on Tab, so the parked start can get a keyboard state while the Tab
    /// that moves focus off it is still in flight; that arrival must not
    /// bounce a second time.
    parked: Option<AccessibilityId>,
}

/// The window's focus state, shared by every region and trap. [`WindowHost`]
/// provides it; components get it with [`use_focus_core`].
///
/// [`WindowHost`]: crate::window::WindowHost
#[derive(Clone, Default)]
pub struct FocusCore(Rc<RefCell<Core>>);

/// Provides the window's [`FocusCore`] to the tree below. Call once, from the
/// window host.
pub fn provide_focus_core() -> FocusCore {
    use_provide_context(FocusCore::default)
}

/// The window's [`FocusCore`], from the window host above. Without a host (the
/// gallery, the snapshot runner, an inspector window) it is a detached core:
/// focus moves do nothing there, and controls keep working.
pub fn use_focus_core() -> FocusCore {
    try_consume_context::<FocusCore>().unwrap_or_default()
}

fn send(strategy: AccessibilityFocusStrategy) {
    Platform::get()
        .current_window()
        .send(UserEvent::FocusAccessibilityNode(strategy));
}

fn send_dir(dir: Dir) {
    send(match dir {
        Dir::Forward => {
            AccessibilityFocusStrategy::Forward(AccessibilityFocusMovement::OutsideGroup)
        }
        Dir::Backward => {
            AccessibilityFocusStrategy::Backward(AccessibilityFocusMovement::OutsideGroup)
        }
    });
}

fn set_keyboard_mode() {
    let mut mode = Platform::get().current_window().navigation_mode;
    mode.set(NavigationMode::Keyboard);
}

impl FocusCore {
    /// Tab (`back` for Shift+Tab): move on in that direction, with a fresh
    /// bounce budget.
    pub fn tab(&self, back: bool) {
        let dir = if back { Dir::Backward } else { Dir::Forward };
        {
            let mut core = self.0.borrow_mut();
            core.dir = dir;
            core.budget = BOUNCE_BUDGET;
        }
        set_keyboard_mode();
        send_dir(dir);
    }

    /// F6 (`back` for Shift+F6): focus the first control of the next or
    /// previous region, in keyboard mode. Does nothing while a popup traps
    /// focus.
    pub fn cycle(&self, back: bool) {
        let target = {
            let core = self.0.borrow();
            if core.open_traps > 0 {
                return;
            }
            let count = ORDER.len();
            let current = core
                .current
                .and_then(|kind| ORDER.iter().position(|region| *region == kind));
            (1..=count).find_map(|step| {
                let index = match (current, back) {
                    (Some(index), false) => (index + step) % count,
                    (Some(index), true) => (index + count - step % count) % count,
                    (None, false) => (step - 1) % count,
                    (None, true) => (count - step % count) % count,
                };
                let kind = ORDER[index];
                core.starts
                    .iter()
                    .find(|(registered, _)| *registered == kind)
                    .map(|(_, id)| *id)
            })
        };
        let Some(target) = target else {
            return;
        };
        {
            let mut core = self.0.borrow_mut();
            core.dir = Dir::Forward;
            core.budget = BOUNCE_BUDGET;
        }
        self.0.borrow_mut().parked = None;
        set_keyboard_mode();
        send(AccessibilityFocusStrategy::Node(target));
    }

    /// Focuses the start of `kind`. `keyboard` moves through the start into
    /// the first control, with the ring; otherwise focus parks on the start in
    /// pointer mode and the next Tab continues from there.
    pub fn focus_region(&self, kind: RegionKind, keyboard: bool) {
        let target = self
            .0
            .borrow()
            .starts
            .iter()
            .find(|(registered, _)| *registered == kind)
            .map(|(_, id)| *id);
        let Some(target) = target else {
            return;
        };
        self.0.borrow_mut().parked = (!keyboard).then_some(target);
        if keyboard {
            {
                let mut core = self.0.borrow_mut();
                core.dir = Dir::Forward;
                core.budget = BOUNCE_BUDGET;
            }
            set_keyboard_mode();
        }
        send(AccessibilityFocusStrategy::Node(target));
    }

    /// A press that a focusable control took. Its region must not also take
    /// the press as empty space.
    fn claim_press(&self) {
        self.0.borrow_mut().claimed = true;
    }

    /// Takes the claim a control left on this press, returning whether there
    /// was one.
    fn take_claim(&self) -> bool {
        std::mem::replace(&mut self.0.borrow_mut().claimed, false)
    }

    fn set_current(&self, kind: RegionKind) {
        self.0.borrow_mut().current = Some(kind);
    }

    fn register(&self, kind: RegionKind, id: AccessibilityId) {
        let mut core = self.0.borrow_mut();
        core.starts.retain(|(registered, _)| *registered != kind);
        core.starts.push((kind, id));
    }

    /// Spends one bounce. `false` once the budget is gone.
    fn spend(&self) -> bool {
        let mut core = self.0.borrow_mut();
        if core.budget == 0 {
            return false;
        }
        core.budget -= 1;
        true
    }

    /// Forgets a park on `id` once focus has left it.
    fn forget_park(&self, id: AccessibilityId) {
        let mut core = self.0.borrow_mut();
        if core.parked == Some(id) {
            core.parked = None;
        }
    }

    /// Keyboard focus landed on a sentinel: move on from it.
    fn keyboard_on(&self, id: AccessibilityId, edge: Edge) {
        // A pointer-parked start that gets keyboard focus is the Tab that is
        // moving focus off it, not a landing: that Tab already steps on.
        if matches!(edge, Edge::Region(_)) && self.0.borrow().parked == Some(id) {
            return;
        }
        let dir = self.0.borrow().dir;
        let step = match (edge, dir) {
            (Edge::Region(_), dir) => Step::Bounce(dir),
            (Edge::TrapStart(_), Dir::Forward) => Step::Bounce(Dir::Forward),
            (Edge::TrapStart(end), Dir::Backward) => Step::Enter(end),
            (Edge::TrapEnd(start), Dir::Forward) => Step::Enter(start),
            (Edge::TrapEnd(_), Dir::Backward) => Step::Bounce(Dir::Backward),
        };
        if !self.spend() {
            return;
        }
        match step {
            Step::Bounce(dir) => send_dir(dir),
            Step::Enter(id) => send(AccessibilityFocusStrategy::Node(id)),
        }
    }
}

/// A region of the window, from [`use_region`]. Its start sentinel goes first
/// in the region, and its root handles presses.
#[derive(Clone)]
pub struct Region {
    kind: RegionKind,
    start: AccessibilityId,
    core: FocusCore,
}

/// The region of `kind` in this component. Call once per region and keep the
/// handle for the region's element.
pub fn use_region(kind: RegionKind) -> Region {
    let start = use_a11y();
    let core = use_focus_core();
    Region { kind, start, core }
}

impl Region {
    /// The region's start sentinel. Put it as the region's first child.
    pub fn sentinel(&self) -> Sentinel {
        Sentinel {
            id: self.start,
            edge: Edge::Region(self.kind),
        }
    }

    /// The region's press handler. A press on a control the control claimed
    /// (see [`Focusable`]) does nothing more. A press on empty space focuses
    /// the region's start in pointer mode.
    pub fn press(&self, event: &Event<PressEventData>) {
        // The region takes the press as the outermost handler: an enclosing
        // region must not park focus again on top of it.
        event.stop_propagation();
        self.core.set_current(self.kind);
        if self.core.take_claim() {
            return;
        }
        self.core.focus_region(self.kind, false);
    }
}

/// Marks a press that a focusable control took, so its region does not treat
/// it as empty space. Call from the control's press handler, before the
/// region's handler runs.
pub fn claim_press(core: &FocusCore) {
    core.claim_press();
}

/// A press that opens something on `id`: records `id` as the pressed control
/// and requests its focus. A popup opened by the press returns focus to `id`
/// when it closes. Call from a control's press handler, after its action.
pub fn press_focus(core: &FocusCore, id: AccessibilityId) {
    core.0.borrow_mut().pressed = Some(id);
    id.request_focus();
}

/// A zero-size, always-focusable accessibility node at an edge of a region or a
/// popup trap. From [`Region::sentinel`] or [`trap`].
#[derive(Clone, PartialEq)]
pub struct Sentinel {
    id: AccessibilityId,
    edge: Edge,
}

impl Component for Sentinel {
    fn render(&self) -> impl IntoElement {
        let core = use_focus_core();
        let id = self.id;
        let edge = self.edge;
        let focus = use_focus(id);

        let hook_core = core.clone();
        use_hook(move || {
            if let Edge::Region(kind) = edge {
                hook_core.register(kind, id);
            }
        });

        use_side_effect(move || {
            let state = *focus.read();
            if let Edge::Region(kind) = edge
                && state.is_focused()
            {
                core.set_current(kind);
            }
            if !state.is_focused() {
                core.forget_park(id);
            }
            if state == Focus::Keyboard {
                core.keyboard_on(id, edge);
            }
        });

        rect()
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .a11y_id(id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::GenericContainer)
    }
}

/// A popup's focus trap. Tab and Shift+Tab stay inside the popup. Build one
/// with [`trap`].
#[derive(Clone, PartialEq)]
pub struct Trap {
    child: Element,
}

/// Traps focus inside `child` while it is mounted. On close, focus returns to
/// the element that had it when the trap opened.
pub fn trap(child: impl Into<Element>) -> Trap {
    Trap {
        child: child.into(),
    }
}

impl Component for Trap {
    fn render(&self) -> impl IntoElement {
        let start = use_a11y();
        let end = use_a11y();
        let core = use_focus_core();
        let opened = use_state(|| false);
        let window = use_hook(|| Platform::get().current_window());
        // The control that opened the popup took its press just before the
        // popup mounted, and freya's panel focus takes over after that.
        let opener = use_hook({
            let core = core.clone();
            let window = window.clone();
            move || {
                core.0
                    .borrow_mut()
                    .pressed
                    .take()
                    .unwrap_or_else(|| *window.focused_accessibility_id.peek())
            }
        });

        let drop_core = core.clone();
        use_hook({
            let core = core.clone();
            move || core.0.borrow_mut().open_traps += 1
        });
        use_drop({
            let window = window.clone();
            move || {
                drop_core.0.borrow_mut().open_traps -= 1;
                // The window may already be gone at shutdown; then there is
                // nothing to restore into.
                if Platform::get().try_window(window.id).is_some() {
                    let mut mode = window.navigation_mode;
                    mode.set(NavigationMode::Keyboard);
                    window.send(UserEvent::FocusAccessibilityNode(
                        AccessibilityFocusStrategy::Node(opener),
                    ));
                }
            }
        });

        // Opening: focus the start, so the first Tab enters the popup. The
        // sentinel is in the tree only after this render, so the focus waits
        // for the first effect.
        let effect_core = core.clone();
        use_side_effect(move || {
            let mut opened = opened;
            if !*opened.peek() {
                opened.set(true);
                if *Platform::get().current_window().navigation_mode.peek()
                    == NavigationMode::Keyboard
                {
                    effect_core.0.borrow_mut().dir = Dir::Forward;
                    effect_core.0.borrow_mut().budget = BOUNCE_BUDGET;
                }
                send(AccessibilityFocusStrategy::Node(start));
            }
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .child(Sentinel {
                id: start,
                edge: Edge::TrapStart(end),
            })
            .child(self.child.clone())
            .child(Sentinel {
                id: end,
                edge: Edge::TrapEnd(start),
            })
    }
}
