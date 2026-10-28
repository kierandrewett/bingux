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

pub mod owner;
pub mod ring_check;
#[cfg(all(test, feature = "testing"))]
mod ring_layout_tests;

use std::{cell::RefCell, rc::Rc};

use freya::prelude::*;
use freya_core::accessibility::tree::ACCESSIBILITY_ROOT_ID;
use owner::ScopeStack;

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
#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    /// The start of a region. Keyboard focus passes through it.
    Region(RegionKind),
    /// The start of a popup trap, with the id of the trap's end sentinel.
    TrapStart(AccessibilityId),
    /// The end of a popup trap, with the id of the trap's start sentinel.
    TrapEnd(AccessibilityId),
}

/// What a keyboard landing on a sentinel does next.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Send Tab in this direction, to move on from the sentinel.
    Bounce(Dir),
    /// Focus this sentinel instead, which keeps focus inside a trap.
    Enter(AccessibilityId),
}

/// What landing on the sentinel `edge` does when focus was moving in `dir`.
/// This is the wrap order of every modal scope: a Tab off the last control
/// lands on the end sentinel and re-enters at the start, a Shift+Tab off the
/// first lands on the start sentinel and re-enters at the end, so focus
/// cycles inside the scope and never reaches the page behind it. A region's
/// sentinel just passes focus on.
fn edge_step(edge: Edge, dir: Dir) -> Step {
    match (edge, dir) {
        (Edge::Region(_), dir) => Step::Bounce(dir),
        (Edge::TrapStart(_), Dir::Forward) => Step::Bounce(Dir::Forward),
        (Edge::TrapStart(end), Dir::Backward) => Step::Enter(end),
        (Edge::TrapEnd(start), Dir::Forward) => Step::Enter(start),
        (Edge::TrapEnd(_), Dir::Backward) => Step::Bounce(Dir::Backward),
    }
}

#[derive(Default)]
struct Core {
    dir: Dir,
    budget: u8,
    current: Option<RegionKind>,
    claimed: bool,
    /// The open modal scopes (dialogs, popups, sheets), the live one on top.
    scopes: ScopeStack<AccessibilityId>,
    /// How many scopes are open, as a state the rings read, so they hide and
    /// show as scopes come and go. Only the window host's core has one.
    modal_depth: Option<State<usize>>,
    starts: Vec<(RegionKind, AccessibilityId)>,
    /// The control pressed last and when, for a popup it opens to return
    /// focus to. A press older than [`OPENER_PRESS_MS`] is not an opener.
    pressed: Option<(AccessibilityId, std::time::Instant)>,
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

/// Marks the content of a modal scope, so a focus ring knows it is not behind
/// one. Provided by [`Trap`].
#[derive(Clone, Copy)]
pub(crate) struct InsideScope;

/// Provides the window's [`FocusCore`] to the tree below. The window host and
/// the overlay host both call it once: whichever is outermost creates the core
/// and the other uses it, so the page and the menus, dialogs and sheets drawn
/// beside it in the overlay layers share one focus state.
pub fn provide_focus_core() -> FocusCore {
    use_hook(|| {
        try_consume_context::<FocusCore>().unwrap_or_else(|| {
            let core = FocusCore::default();
            core.0.borrow_mut().modal_depth = Some(State::create(0));
            provide_context(core.clone());
            core
        })
    })
}

impl FocusCore {
    /// Whether a modal scope is up that the caller is not inside, so what it
    /// draws belongs to the inert page behind. A focus ring that is inert is
    /// not drawn. Reading it subscribes the caller to scopes opening and
    /// closing.
    pub(crate) fn page_inert(&self, inside_scope: bool) -> bool {
        let depth = self.0.borrow().modal_depth;
        if let Some(depth) = depth {
            let _ = *depth.read();
        }
        !inside_scope && self.0.borrow().scopes.is_modal()
    }

    /// Tells everything that reads [`page_inert`](Self::page_inert) that the
    /// open scopes changed.
    fn publish_depth(&self) {
        let (depth, now) = {
            let core = self.0.borrow();
            (core.modal_depth, core.scopes.depth())
        };
        if let Some(mut depth) = depth {
            depth.set_if_modified(now);
        }
    }
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

/// Moves keyboard focus to the next focus stop (`back` for the previous), in
/// keyboard mode with the ring, as Tab does. A list answers its Up and Down
/// keys with it so the arrows walk the rows. Unlike [`FocusCore::tab`] it
/// needs no window host, so it also works in the inspector window.
///
/// Call it from a key handler. The move is sent a tick later: Freya's own
/// handler for the arrow keys asks, in the same event, to move inside the
/// focused accessibility group, and the last request in an event wins, so this
/// one is queued behind it.
pub fn step_focus(back: bool) {
    set_keyboard_mode();
    let dir = if back { Dir::Backward } else { Dir::Forward };
    spawn(async move { send_dir(dir) });
}

/// Moves keyboard focus to the stop `id`, in keyboard mode with the ring.
/// A list uses it for Home, End and type-ahead, where [`step_focus`] only
/// walks to a neighbour. It is sent a tick later, for the reason
/// [`step_focus`] is.
pub fn focus_node(id: AccessibilityId) {
    set_keyboard_mode();
    spawn(async move { send(AccessibilityFocusStrategy::Node(id)) });
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
            if core.scopes.is_modal() {
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
        let step = edge_step(edge, dir);
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

/// Takes back a claim from [`claim_press`] that no region press will ever
/// collect. A shield that closes a menu on the pointer going down claims the
/// press, so the release that follows (which lands on the page once the shield
/// has gone) does not park focus on the region and blur the control the menu
/// belongs to; when the shield still hears the release itself, it takes the
/// claim back here so the next press on empty space is not swallowed.
pub fn release_press(core: &FocusCore) {
    core.take_claim();
}

/// Whether a [`claim_press`] is waiting for its press to finish: a control or a
/// popup's shield has taken the press now going on. A text field reads it when
/// the pointer is released, to tell the release that ends a menu's dismissing
/// press from a click outside the field.
pub fn press_claimed(core: &FocusCore) -> bool {
    core.0.borrow().claimed
}

/// A press that opens something on `id`: records `id` as the pressed control
/// and requests its focus. A popup opened by the press returns focus to `id`
/// when it closes. Call from a control's press handler, after its action.
pub fn press_focus(core: &FocusCore, id: AccessibilityId) {
    note_press(core, id);
    id.request_focus();
}

/// How long after a press a modal scope it opened still counts it as its
/// opener, in milliseconds.
const OPENER_PRESS_MS: u128 = 1000;

/// Records `id` as the control pressed last, so a modal scope opened by the
/// press returns focus to it. A pointer press does not focus a button, so
/// without this the opener would be whatever had focus before. Every
/// focusable control's wrapper does it; callers never need to.
pub(crate) fn note_press(core: &FocusCore, id: AccessibilityId) {
    core.0.borrow_mut().pressed = Some((id, std::time::Instant::now()));
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
            // A key only reaches an element that listens to it, or one whose
            // descendant has the focus. A sentinel that has the focus listens
            // (and does nothing with the key), so a scope's own handler, such as
            // a dialog's Enter, hears it as it bubbles.
            .on_key_down(|_: Event<KeyboardEventData>| {})
    }
}

/// A modal focus scope's trap. Tab and Shift+Tab stay inside it, and the page
/// behind is inert. Build one with [`trap`] or [`trap_inline`].
#[derive(Clone, PartialEq)]
pub struct Trap {
    child: Element,
    /// Whether the trap's box fills its parent (a popup's content) or is as
    /// large as `child` (a dialog's card, centred by its panel).
    fill: bool,
    /// Whether the trap focuses its start when it opens. A scope whose own
    /// content picks the first focus (a dialog's default action) turns it off.
    focus_start: bool,
}

/// Traps focus inside `child` while it is mounted. On close, focus returns to
/// the element that had it when the trap opened. The trap fills its parent.
pub fn trap(child: impl Into<Element>) -> Trap {
    Trap {
        child: child.into(),
        fill: true,
        focus_start: true,
    }
}

impl Trap {
    /// The scope's content takes the first focus itself (a default action,
    /// an autofocused field), so the trap does not move it to its start.
    pub fn keep_initial_focus(mut self) -> Self {
        self.focus_start = false;
        self
    }
}

/// [`trap`] for a card that sizes itself and that its panel places: the trap
/// adds only two zero-size sentinels around it. Every modal layer (dialog,
/// alert, framed dialog, sheet) installs it, so the page behind never takes
/// the keyboard.
pub fn trap_inline(child: impl Into<Element>) -> Trap {
    Trap {
        child: child.into(),
        fill: false,
        focus_start: true,
    }
}

impl Component for Trap {
    fn render(&self) -> impl IntoElement {
        let start = use_a11y();
        let scope = use_a11y();
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
                let pressed = core.0.borrow_mut().pressed.take();
                pressed
                    .filter(|(_, at)| at.elapsed().as_millis() <= OPENER_PRESS_MS)
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| *window.focused_accessibility_id.peek())
            }
        });

        let drop_core = core.clone();
        use_hook({
            let core = core.clone();
            move || core.0.borrow_mut().scopes.push(start, Some(opener))
        });
        use_drop({
            let window = window.clone();
            move || {
                let restore = drop_core.0.borrow_mut().scopes.remove(start);
                // The window may already be gone at shutdown; then there is
                // nothing to restore into. A scope closed under another one
                // restores nothing: the live scope keeps its focus.
                if Platform::get().try_window(window.id).is_none() {
                    return;
                }
                drop_core.publish_depth();
                if let Some(opener) = restore {
                    let mut mode = window.navigation_mode;
                    mode.set(NavigationMode::Keyboard);
                    window.send(UserEvent::FocusAccessibilityNode(
                        AccessibilityFocusStrategy::Node(opener),
                    ));
                }
            }
        });
        // Rings behind the scope hide while it is open.
        use_provide_context(|| InsideScope);
        let publish_core = core.clone();
        use_side_effect(move || publish_core.publish_depth());

        // The focused node was removed (Freya resets focus to the window's
        // root): focus falls back to the live scope's start, so Escape and
        // Tab still work and nothing behind the scope takes the keyboard.
        let focused = window.focused_accessibility_id;
        let fallback_core = core.clone();
        // Focus that never reached anything is not a removed node: the
        // fallback waits until the focus has been somewhere other than the
        // window's root, so a scope that opens with its card taking the focus
        // is not pulled onto its start first.
        let held = use_hook(|| Rc::new(std::cell::Cell::new(false)));
        use_side_effect(move || {
            if *focused.read() != ACCESSIBILITY_ROOT_ID {
                held.set(true);
            } else if held.get() && fallback_core.0.borrow().scopes.is_live(start) {
                send(AccessibilityFocusStrategy::Node(start));
            }
        });

        // Opening: focus the start, so the first Tab enters the popup. The
        // sentinel is in the tree only after this render, so the focus waits
        // for the first effect.
        let effect_core = core.clone();
        let focus_start = self.focus_start;
        use_side_effect(move || {
            let mut opened = opened;
            if !*opened.peek() && focus_start {
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
            .a11y_id(scope)
            .a11y_focusable(false)
            .a11y_role(AccessibilityRole::GenericContainer)
            // The scope is modal: assistive technology treats the page behind
            // as inert, and the inspector marks it so.
            .a11y_builder(|builder| builder.set_modal())
            .maybe(self.fill, |root| root.width(Size::fill()).height(Size::fill()))
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

/// The wrap order of every scope: how a landing on a sentinel moves focus on.
#[cfg(test)]
mod edge_tests {
    use super::*;

    const START: AccessibilityId = AccessibilityId(1);
    const END: AccessibilityId = AccessibilityId(2);

    #[test]
    fn a_region_sentinel_always_passes_focus_on() {
        for kind in ORDER {
            for dir in [Dir::Forward, Dir::Backward] {
                assert_eq!(edge_step(Edge::Region(kind), dir), Step::Bounce(dir));
            }
        }
    }

    #[test]
    fn tab_into_a_traps_start_moves_on_to_its_first_control() {
        assert_eq!(
            edge_step(Edge::TrapStart(END), Dir::Forward),
            Step::Bounce(Dir::Forward)
        );
    }

    #[test]
    fn tab_off_a_traps_last_control_wraps_to_its_start() {
        assert_eq!(edge_step(Edge::TrapEnd(START), Dir::Forward), Step::Enter(START));
    }

    #[test]
    fn shift_tab_off_a_traps_first_control_wraps_to_its_end() {
        assert_eq!(edge_step(Edge::TrapStart(END), Dir::Backward), Step::Enter(END));
    }

    #[test]
    fn shift_tab_into_a_traps_end_moves_on_to_its_last_control() {
        assert_eq!(
            edge_step(Edge::TrapEnd(START), Dir::Backward),
            Step::Bounce(Dir::Backward)
        );
    }
}

/// One Tab press moves focus exactly one control, across a page of every kind
/// of control, forward and back.
#[cfg(all(test, feature = "testing"))]
mod tab_tests {
    use std::time::Duration;

    use freya::prelude::*;

    use crate::controls::*;
    use crate::snapshot::Snapshot;
    use crate::style::*;
    use crate::surface::use_tokens;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    /// How many controls the page holds, each one a single Tab stop.
    const STOPS: usize = 9;

    /// The window root: it provides the tokens the window host reads.
    #[derive(PartialEq)]
    struct Root;

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(Page)
        }
    }

    #[derive(PartialEq)]
    struct Page;

    impl Component for Page {
        fn render(&self) -> impl IntoElement {
            let tokens = use_tokens();
            let name = use_state(String::new);
            let options = || {
                vec![
                    ("a".to_string(), "One".to_string()),
                    ("b".to_string(), "Two".to_string()),
                ]
            };
            rect()
                .expanded()
                .tw(&tokens, &[flex_col(), p(S4), gap(S2)])
                .child(action_button("First", |_: ()| {}))
                .child(switch(false, |_: bool| {}))
                .child(checkbox(false, |_: bool| {}).text("Check"))
                .child(dropdown(options(), "a", |_: String| {}))
                .child(segmented(options(), "a", |_: String| {}))
                .child(slider(0.5, |_: f64| {}))
                .child(text_field(name, |_: String| {}))
                .child(stepper(3, |_: i64| {}))
                .child(primary_button("Last", |_: ()| {}))
        }
    }

    fn focused(shot: &mut Snapshot) -> AccessibilityId {
        shot.runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek())
    }

    #[test]
    fn each_tab_press_moves_focus_one_control_and_shift_tab_walks_back() {
        let mut shot = Snapshot::new(
            || Root.into_element(),
            (520.0, 520.0),
        );
        shot.settle(Duration::from_millis(300));
        let tab = || Key::Named(NamedKey::Tab);

        // A lap forward: every press lands on a control the walk has not
        // visited, and the press after the last control wraps to the first.
        let mut walk = Vec::new();
        for press in 1..=STOPS {
            shot.key(tab());
            let now = focused(&mut shot);
            assert!(
                !walk.contains(&now),
                "Tab press {press} stayed on or revisited a control (walk so far: {})",
                walk.len()
            );
            walk.push(now);
        }
        shot.key(tab());
        assert_eq!(focused(&mut shot), walk[0], "Tab {} wraps to the first control", STOPS + 1);

        // Shift+Tab retraces the lap one control per press.
        for (back, want) in walk.iter().rev().enumerate() {
            shot.key_with(tab(), Modifiers::SHIFT);
            assert_eq!(
                focused(&mut shot),
                *want,
                "Shift+Tab press {} lands on control {}",
                back + 1,
                STOPS - 1 - back
            );
        }
    }
}
