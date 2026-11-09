//! Drag and drop, within a window, between windows and with other programs.
//!
//! - [`DropTarget`] takes drops: wrap a view in one (it draws the highlight)
//!   or call [`use_drop_target`] from a view that draws its own (a list that
//!   marks the row under the pointer). It names the [`Format`]s it takes,
//!   the [`DragAction`]s it allows and what to do with the data.
//! - [`DragSource`] starts drags: wrap a view in one, or call
//!   [`use_drag`] and [`Dragger::start`] from a view with its own gesture.
//!   It offers a [`Payload`], made when the drag starts and, for lazy parts,
//!   only when someone drops.
//! - The same targets take drags from this window, from another window of
//!   the program and from any other program, and a drag from here can land in
//!   any of them. A drop from this program hands over the payload in memory,
//!   with its [`Payload::local`] value.
//!
//! On Wayland this is the `wl_data_device` protocol, run beside winit on its
//! connection (see `wayland`). With no Wayland (a test, X11) a drag stays in
//! its window and reaches that window's targets only; touch drags do too.
//!
//! Positions are window pixels (Freya's logical pixels) unless said
//! otherwise; a target's [`DragInfo::position`] is from its own top-left.

mod payload;
#[cfg(target_os = "linux")]
pub(crate) mod wayland;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use futures_util::StreamExt;

use payload::LOCAL_MARKER;
pub use payload::{
    Format, GNOME_COPIED_FILES, KDE_CUT_SELECTION, Payload, Received, TEXT_TYPES, URI_LIST,
    uri_list,
};

use crate::component::ComponentName;
use crate::drag::{Point, is_touch, point_of};
use crate::style::{Line, Paint, Radius, Tw, bg, border, rounded};
use crate::surface::use_tokens;

/// How far the pointer moves with a button down before a press becomes a
/// drag: the slop every pointer drag in bingux-ui uses.
pub const DRAG_SLOP: f32 = 6.0;

/// How long a drag must rest over a spring-loaded target before it opens:
/// Finder's medium setting.
pub const SPRING_DELAY: Duration = Duration::from_millis(700);

/// How far the pointer may wander while a spring-loaded target counts the
/// rest; beyond it the count starts again.
const SPRING_WANDER: f32 = 6.0;

/// What a drop does to the dragged thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DragAction {
    /// The target gets a copy; the source keeps its own.
    Copy,
    /// The target takes it; the source removes its own.
    Move,
    /// The target asks the person (a menu at the drop). Before calling
    /// [`DropTarget::on_drop`], bingux-ui settles it on the target's
    /// preferred action of the other two.
    Ask,
}

/// A set of [`DragAction`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DragActions(u8);

impl DragActions {
    pub const NONE: Self = Self(0);
    pub const COPY: Self = Self(1);
    pub const MOVE: Self = Self(2);
    pub const ASK: Self = Self(4);
    pub const COPY_MOVE: Self = Self(1 | 2);
    pub const ALL: Self = Self(1 | 2 | 4);

    pub const fn of(action: DragAction) -> Self {
        match action {
            DragAction::Copy => Self::COPY,
            DragAction::Move => Self::MOVE,
            DragAction::Ask => Self::ASK,
        }
    }

    pub const fn contains(self, action: DragAction) -> bool {
        self.0 & Self::of(action).0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The first of Copy, Move and Ask in the set.
    pub fn first(self) -> Option<DragAction> {
        [DragAction::Copy, DragAction::Move, DragAction::Ask]
            .into_iter()
            .find(|action| self.contains(*action))
    }
}

impl std::ops::BitOr for DragActions {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<DragAction> for DragActions {
    fn from(action: DragAction) -> Self {
        Self::of(action)
    }
}

/// What a target knows about a drag over it, before the drop.
#[derive(Clone, Debug, PartialEq)]
pub struct DragInfo {
    /// The pointer, from the target's top-left.
    pub position: Point,
    /// What the drag offers.
    pub formats: Vec<Format>,
    /// The actions the source allows.
    pub source_actions: DragActions,
    /// The action a drop here would have now: what to show on a badge.
    /// `None` while the target is choosing.
    pub action: Option<DragAction>,
    /// Whether the drag came from a window of this program.
    pub local: bool,
}

/// A drop on a target.
#[derive(Clone, Debug)]
pub struct Dropped {
    /// The data, in the first format of [`DropTarget::accepts`] the drag had.
    pub data: Received,
    /// Where it was dropped, from the target's top-left.
    pub position: Point,
    /// Copy or Move (never Ask: see [`DragAction::Ask`]).
    pub action: DragAction,
}

/// How a drag that this program started ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragEnd {
    /// A target took it with this action. After a `Move` the source removes
    /// its own copy, unless the data was files: a file manager that takes a
    /// move of files moves them itself.
    Dropped(DragAction),
    /// Let go over nothing that takes it, or Escape.
    Cancelled,
}

/// The picture that follows the pointer while dragging out of the window.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum DragImage {
    /// The compositor's plain cursor only.
    None,
    /// A picture of the source as it is drawn (for [`DragSource`], its own
    /// box; for [`Dragger::start`], [`DragRequest::area`]).
    #[default]
    Snapshot,
    /// A picture of this part of the window.
    Area(Area),
}

// ---------------------------------------------------------------------------
// Targets

/// A view that takes drops. A component (wrap the view with
/// [`drop_target`]) and the configuration [`use_drop_target`] takes.
///
/// ```ignore
/// drop_target(status_page(..))
///     .accepts([Format::Uris])
///     .on_drop(move |dropped: Dropped| open(dropped.data.files()))
/// ```
#[derive(Clone, PartialEq)]
pub struct DropTarget {
    child: Option<Element>,
    accepts: Vec<Format>,
    actions: DragActions,
    preferred: Option<Callback<DragInfo, DragAction>>,
    accept_if: Option<Callback<DragInfo, bool>>,
    on_drop: Option<EventHandler<Dropped>>,
    spring: Option<(Duration, EventHandler<DragInfo>)>,
    enabled: bool,
    highlight: bool,
}

/// A drop target around `child`. Give it [`accepts`](DropTarget::accepts)
/// and [`on_drop`](DropTarget::on_drop).
pub fn drop_target(child: impl Into<Element>) -> DropTarget {
    DropTarget {
        child: Some(child.into()),
        ..DropTarget::new()
    }
}

impl Default for DropTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl DropTarget {
    /// A target with nothing around it: for [`use_drop_target`].
    pub fn new() -> Self {
        Self {
            child: None,
            accepts: Vec::new(),
            actions: DragActions::COPY_MOVE,
            preferred: None,
            accept_if: None,
            on_drop: None,
            spring: None,
            enabled: true,
            highlight: true,
        }
    }

    /// The formats it takes, best first: a drop is read in the first one
    /// the drag offers.
    pub fn accepts(mut self, formats: impl IntoIterator<Item = Format>) -> Self {
        self.accepts = formats.into_iter().collect();
        self
    }

    /// The actions it allows. Copy and Move unless set.
    pub fn actions(mut self, actions: DragActions) -> Self {
        self.actions = actions;
        self
    }

    /// The action it would rather have, of those both sides allow. Without
    /// one: Copy, Move with Shift held, Copy with Ctrl held.
    pub fn preferred(mut self, choose: impl FnMut(DragInfo) -> DragAction + 'static) -> Self {
        self.preferred = Some(Callback::new(choose));
        self
    }

    /// Take only some drags: a folder that refuses itself, a list that takes
    /// drops on its folders only. Asked as the pointer moves.
    pub fn accept_if(mut self, accept: impl FnMut(DragInfo) -> bool + 'static) -> Self {
        self.accept_if = Some(Callback::new(accept));
        self
    }

    pub fn on_drop(mut self, on_drop: impl Into<EventHandler<Dropped>>) -> Self {
        self.on_drop = Some(on_drop.into());
        self
    }

    /// Open on a rested drag: `open` runs once a drag has stayed still over
    /// the target for `delay` ([`SPRING_DELAY`] is the usual), so the person
    /// can drop into a folder, tab or page that was closed.
    pub fn spring_loaded(
        mut self,
        delay: Duration,
        open: impl Into<EventHandler<DragInfo>>,
    ) -> Self {
        self.spring = Some((delay, open.into()));
        self
    }

    /// A disabled target takes nothing. Enabled unless set.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Whether [`drop_target`] draws the accent ring and tint while a drag
    /// it takes is over it. On unless set; turn it off to draw your own from
    /// the hover state.
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// The format it would read `offered` as, and the MIME type for it.
    fn choose(&self, offered: &[String]) -> Option<(Format, String)> {
        self.accepts.iter().find_map(|format| {
            format
                .best_mime(offered)
                .map(|mime| (format.clone(), mime.to_string()))
        })
    }
}

impl Component for DropTarget {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let handle = use_drop_target(self.clone());
        let lit = self.highlight && handle.over().is_some();
        let child = self.child.clone().unwrap_or_else(|| rect().into());
        handle.track(
            rect()
                .component("DropTarget")
                .maybe(lit, |rect| {
                    rect.tw(
                        &tokens,
                        &[
                            bg(Paint::AccentSoft),
                            border(Line::Ring(Paint::Accent)),
                            rounded(Radius::Card),
                        ],
                    )
                })
                .child(child),
        )
    }
}

/// A registered target: its box and its latest configuration.
struct Slot {
    id: u64,
    area: Cell<Option<Area>>,
    config: RefCell<DropTarget>,
    over: State<Option<DragInfo>>,
}

/// A drop target in a view that draws its own: a list that marks the row
/// under the pointer. `Copy`.
#[derive(Clone, Copy, PartialEq)]
pub struct DropHandle {
    over: State<Option<DragInfo>>,
    id: u64,
}

/// Make the current component a drop target with `target`'s settings
/// (refreshed on each render; its child is ignored). Give the handle the
/// view's box with [`DropHandle::track`] or [`DropHandle::set_area`].
pub fn use_drop_target(target: DropTarget) -> DropHandle {
    let over = use_state(|| None::<DragInfo>);
    let hub = use_try_consume::<DropHub>();
    let slot = use_hook(|| {
        let slot = Rc::new(Slot {
            id: hub.as_ref().map_or(0, DropHub::next_id),
            area: Cell::new(None),
            config: RefCell::new(DropTarget::new()),
            over,
        });
        if let Some(hub) = &hub {
            hub.0.targets.borrow_mut().push(slot.clone());
        }
        slot
    });
    *slot.config.borrow_mut() = DropTarget {
        child: None,
        ..target
    };
    let id = slot.id;
    use_drop({
        let hub = hub.clone();
        move || {
            if let Some(hub) = hub {
                hub.0.targets.borrow_mut().retain(|slot| slot.id != id);
            }
        }
    });
    DropHandle { over, id }
}

impl DropHandle {
    /// The drag over the target, if one it takes is. Reactive.
    pub fn over(&self) -> Option<DragInfo> {
        self.over.read().clone()
    }

    /// Whether a drag it takes is over it. Reactive.
    pub fn is_targeted(&self) -> bool {
        self.over.read().is_some()
    }

    /// Track `rect`'s box as the target's area.
    pub fn track(self, rect: Rect) -> Rect {
        rect.on_sized(move |event: Event<SizedEventData>| self.set_area(event.visible_area))
    }

    /// Set the target's area in window pixels, from the view's own sizing.
    pub fn set_area(&self, area: Area) {
        if let Some(hub) = try_consume_context::<DropHub>()
            && let Some(slot) = hub.slot(self.id)
        {
            slot.area.set(Some(area));
        }
    }
}

// ---------------------------------------------------------------------------
// The window's hub

/// Where a drag over the window comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Origin {
    /// A Wayland drag; the serial of its enter event names it.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Wayland(u32),
    /// A drag of this window that never left it (no Wayland, or touch).
    Window,
}

/// The drag over the window.
struct Over {
    origin: Origin,
    mime_types: Vec<String>,
    formats: Vec<Format>,
    source_actions: DragActions,
    /// The drag this program started, when it is one.
    local: Option<u64>,
    at: Point,
    /// The target under the pointer that takes the drag, the format and
    /// type it would read, and the actions it allows and prefers.
    target: Option<Choice>,
    /// The action the compositor chose (Wayland) or the preferred (window).
    action: Option<DragAction>,
    spring: Option<(TaskHandle, Point)>,
}

#[derive(Clone, PartialEq)]
struct Choice {
    slot: u64,
    format: Format,
    mime: String,
    allowed: DragActions,
    preferred: DragAction,
}

struct HubInner {
    scope: ScopeId,
    next: Cell<u64>,
    targets: RefCell<Vec<Rc<Slot>>>,
    over: RefCell<Option<Over>>,
    /// This window's `wl_surface*`, once known.
    surface: Cell<Option<usize>>,
}

/// One per window: the drop targets in it and the drag over it. Provided
/// by [`WindowHost`](crate::window::WindowHost) with [`provide_drop_hub`].
#[derive(Clone)]
pub(crate) struct DropHub(Rc<HubInner>);

impl PartialEq for DropHub {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Provide the window's drop hub and connect it to Wayland. Called once per
/// window by `WindowHost`.
pub(crate) fn provide_drop_hub() {
    let scope = current_scope_id();
    let hub = use_provide_context(|| {
        DropHub(Rc::new(HubInner {
            scope,
            next: Cell::new(1),
            targets: RefCell::new(Vec::new()),
            over: RefCell::new(None),
            surface: Cell::new(None),
        }))
    });
    #[cfg(target_os = "linux")]
    {
        use_hook({
            let hub = hub.clone();
            move || connect(hub)
        });
        use_drop(move || {
            if let (Some(worker), Some(surface)) = (wayland::worker(), hub.0.surface.get()) {
                worker.send(wayland::Command::Unregister { surface });
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = hub;
}

/// Start the Wayland worker if it is not running, register this window's
/// surface and route its drag events to the hub.
#[cfg(target_os = "linux")]
fn connect(hub: DropHub) {
    use freya::winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };

    let (events, mut receiver) = unbounded::<wayland::WindowEvent>();
    let task_hub = hub.clone();
    spawn(async move {
        while let Some(event) = receiver.next().await {
            task_hub.wayland_event(event).await;
        }
    });
    let registered = hub;
    crate::shell::with_window(move |window| {
        let display = window
            .display_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                RawDisplayHandle::Wayland(display) => Some(display.display.as_ptr()),
                _ => None,
            });
        let surface = window
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Wayland(window) => Some(window.surface.as_ptr() as usize),
                _ => None,
            });
        let (Some(display), Some(surface)) = (display, surface) else {
            return;
        };
        // SAFETY: winit's display lives until its event loop ends, which
        // ends the program.
        let Some(worker) = (unsafe { wayland::start(display) }) else {
            return;
        };
        crate::clipboard::install_freya_bridge();
        registered.0.surface.set(Some(surface));
        worker.send(wayland::Command::Register { surface, events });
    });
}

#[cfg(target_os = "linux")]
fn dnd_actions(actions: DragActions) -> wayland_dnd::DndAction {
    let mut out = wayland_dnd::DndAction::empty();
    if actions.contains(DragAction::Copy) {
        out |= wayland_dnd::DndAction::Copy;
    }
    if actions.contains(DragAction::Move) {
        out |= wayland_dnd::DndAction::Move;
    }
    if actions.contains(DragAction::Ask) {
        out |= wayland_dnd::DndAction::Ask;
    }
    out
}

#[cfg(target_os = "linux")]
fn from_dnd(actions: wayland_dnd::DndAction) -> DragActions {
    let mut out = DragActions::NONE;
    if actions.contains(wayland_dnd::DndAction::Copy) {
        out = out | DragActions::COPY;
    }
    if actions.contains(wayland_dnd::DndAction::Move) {
        out = out | DragActions::MOVE;
    }
    if actions.contains(wayland_dnd::DndAction::Ask) {
        out = out | DragActions::ASK;
    }
    out
}

#[cfg(target_os = "linux")]
mod wayland_dnd {
    pub use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
}

/// The id of a drag this program started, from the marker type it offers.
fn local_id(mime_types: &[String]) -> Option<u64> {
    let marker = mime_types
        .iter()
        .find(|mime| mime.starts_with(LOCAL_MARKER))?;
    let mut pid = None;
    let mut id = None;
    for part in marker.split(';').skip(1) {
        match part.split_once('=') {
            Some(("pid", value)) => pid = value.parse::<u32>().ok(),
            Some(("id", value)) => id = value.parse::<u64>().ok(),
            _ => {}
        }
    }
    (pid == Some(std::process::id())).then_some(id).flatten()
}

fn local_marker(id: u64) -> String {
    format!("{LOCAL_MARKER};pid={};id={id}", std::process::id())
}

fn inside(area: &Area, at: Point) -> bool {
    at.0 >= area.min_x() && at.0 < area.max_x() && at.1 >= area.min_y() && at.1 < area.max_y()
}

impl DropHub {
    fn next_id(&self) -> u64 {
        let id = self.0.next.get();
        self.0.next.set(id + 1);
        id
    }

    fn slot(&self, id: u64) -> Option<Rc<Slot>> {
        self.0
            .targets
            .borrow()
            .iter()
            .find(|slot| slot.id == id)
            .cloned()
    }

    #[cfg(target_os = "linux")]
    async fn wayland_event(&self, event: wayland::WindowEvent) {
        use wayland::WindowEvent;
        match event {
            WindowEvent::Enter {
                serial,
                x,
                y,
                mime_types,
                source_actions,
            } => self.enter(
                Origin::Wayland(serial),
                mime_types,
                from_dnd(source_actions),
                (x as f32, y as f32),
            ),
            WindowEvent::Motion { x, y } => self.motion((x as f32, y as f32)),
            WindowEvent::Action(action) => {
                let action = from_dnd(action).first();
                if let Some(over) = self.0.over.borrow_mut().as_mut() {
                    over.action = action;
                }
                self.publish();
            }
            WindowEvent::Leave => self.leave(),
            WindowEvent::Drop {
                serial,
                x,
                y,
                action,
            } => {
                self.motion((x as f32, y as f32));
                let action = from_dnd(action).first();
                self.drop(Origin::Wayland(serial), action).await;
            }
        }
    }

    fn enter(
        &self,
        origin: Origin,
        mime_types: Vec<String>,
        source_actions: DragActions,
        at: Point,
    ) {
        self.clear();
        let formats = Format::list(&mime_types);
        let local = local_id(&mime_types).filter(|id| outgoing_is(*id));
        *self.0.over.borrow_mut() = Some(Over {
            origin,
            mime_types,
            formats,
            source_actions,
            local,
            at,
            target: None,
            action: None,
            spring: None,
        });
        self.retarget(true);
    }

    fn motion(&self, at: Point) {
        if let Some(over) = self.0.over.borrow_mut().as_mut() {
            over.at = at;
        } else {
            return;
        }
        self.retarget(false);
    }

    fn leave(&self) {
        self.clear();
    }

    /// Forget the drag: no target is lit and no spring is counting.
    fn clear(&self) {
        let over = self.0.over.borrow_mut().take();
        if let Some(over) = over {
            if let Some((task, _)) = over.spring {
                task.cancel();
            }
            if let Some(choice) = over.target
                && let Some(slot) = self.slot(choice.slot)
            {
                let mut state = slot.over;
                state.set_if_modified(None);
            }
        }
    }

    /// The info a target sees: the drag at `at`, from the target's corner.
    fn info(over: &Over, area: &Area, action: Option<DragAction>) -> DragInfo {
        DragInfo {
            position: (over.at.0 - area.min_x(), over.at.1 - area.min_y()),
            formats: over.formats.clone(),
            source_actions: over.source_actions,
            action,
            local: over.local.is_some(),
        }
    }

    /// Find the target under the pointer and tell it, the old one and the
    /// source. `entered` forces an answer to the source (a new enter needs one).
    fn retarget(&self, entered: bool) {
        let slots: Vec<Rc<Slot>> = self.0.targets.borrow().clone();
        let (choice, previous) = {
            let over_ref = self.0.over.borrow();
            let Some(over) = over_ref.as_ref() else {
                return;
            };
            let mut best: Option<(f32, Choice)> = None;
            for slot in &slots {
                let Some(area) = slot.area.get() else {
                    continue;
                };
                if !inside(&area, over.at) {
                    continue;
                }
                let config = slot.config.borrow().clone();
                if !config.enabled {
                    continue;
                }
                let Some((format, mime)) = config.choose(&over.mime_types) else {
                    continue;
                };
                let info = Self::info(over, &area, None);
                if let Some(accept) = &config.accept_if
                    && !accept.call(info.clone())
                {
                    continue;
                }
                let allowed = config.actions.intersect(over.source_actions);
                // A source that names no actions (an old client) allows Copy.
                let allowed = if over.source_actions.is_empty() {
                    config.actions.intersect(DragActions::COPY)
                } else {
                    allowed
                };
                let Some(fallback) = allowed.first() else {
                    continue;
                };
                let preferred = match &config.preferred {
                    Some(choose) => choose.call(info),
                    None => default_preferred(),
                };
                let preferred = if allowed.contains(preferred) {
                    preferred
                } else {
                    fallback
                };
                let size = area.width() * area.height();
                if best.as_ref().is_none_or(|(best, ..)| size < *best) {
                    best = Some((
                        size,
                        Choice {
                            slot: slot.id,
                            format,
                            mime,
                            allowed,
                            preferred,
                        },
                    ));
                }
            }
            (best.map(|(_, choice)| choice), over.target.clone())
        };

        let changed = choice != previous;
        if changed && let Some(old) = previous.as_ref().and_then(|old| self.slot(old.slot)) {
            let mut state = old.over;
            state.set_if_modified(None);
        }
        let spring = {
            let mut over_ref = self.0.over.borrow_mut();
            let Some(over) = over_ref.as_mut() else {
                return;
            };
            if changed || entered {
                over.target = choice.clone();
                if over.origin == Origin::Window {
                    over.action = choice.as_ref().map(|choice| choice.preferred);
                }
                #[cfg(target_os = "linux")]
                if let Origin::Wayland(serial) = over.origin
                    && let Some(worker) = wayland::worker()
                {
                    worker.send(wayland::Command::Respond {
                        serial,
                        mime: choice.as_ref().map(|choice| choice.mime.clone()),
                        actions: dnd_actions(
                            choice
                                .as_ref()
                                .map_or(DragActions::NONE, |choice| choice.allowed),
                        ),
                        preferred: dnd_actions(
                            choice
                                .as_ref()
                                .map_or(DragActions::NONE, |choice| choice.preferred.into()),
                        ),
                    });
                }
            }
            // The spring counts from where the pointer rests; a new target
            // or a wander starts it again.
            let wandered = over.spring.as_ref().is_some_and(|(_, from)| {
                (over.at.0 - from.0).hypot(over.at.1 - from.1) > SPRING_WANDER
            });
            if changed || wandered || choice.is_none() {
                if let Some((task, _)) = over.spring.take() {
                    task.cancel();
                }
            }
            (over.spring.is_none())
                .then_some(())
                .and(choice.as_ref())
                .and_then(|choice| self.slot(choice.slot))
                .and_then(|slot| slot.config.borrow().spring.clone())
                .map(|(delay, _)| (delay, over.at))
        };
        if let (Some((delay, at)), Some(choice)) = (spring, choice.as_ref()) {
            let hub = self.clone();
            let id = choice.slot;
            let task = spawn_in_scope(
                async move {
                    timer(delay).await;
                    hub.spring(id);
                },
                self.0.scope,
            );
            if let Some(over) = self.0.over.borrow_mut().as_mut() {
                over.spring = Some((task, at));
            }
        }
        self.publish();
    }

    /// Give the target under the pointer its current info.
    fn publish(&self) {
        let info = {
            let over_ref = self.0.over.borrow();
            let Some(over) = over_ref.as_ref() else {
                return;
            };
            let Some(choice) = &over.target else {
                return;
            };
            let Some(slot) = self.slot(choice.slot) else {
                return;
            };
            let Some(area) = slot.area.get() else {
                return;
            };
            (slot, Self::info(over, &area, over.action))
        };
        let (slot, info) = info;
        let mut state = slot.over;
        state.set_if_modified(Some(info));
    }

    fn spring(&self, id: u64) {
        let info = {
            let mut over_ref = self.0.over.borrow_mut();
            let Some(over) = over_ref.as_mut() else {
                return;
            };
            if over.target.as_ref().map(|choice| choice.slot) != Some(id) {
                return;
            }
            let Some(slot) = self.slot(id) else {
                return;
            };
            let Some(area) = slot.area.get() else {
                return;
            };
            let open = slot.config.borrow().spring.clone();
            open.map(|(_, open)| (open, Self::info(over, &area, over.action)))
        };
        if let Some((open, info)) = info {
            open.call(info);
        }
    }

    /// The person let go. Read the data in the target's format and hand it
    /// over, then tell the source.
    async fn drop(&self, origin: Origin, compositor_action: Option<DragAction>) {
        let taken = {
            let over_ref = self.0.over.borrow();
            over_ref.as_ref().and_then(|over| {
                let choice = over.target.clone()?;
                let slot = self.slot(choice.slot)?;
                let area = slot.area.get()?;
                let info = Self::info(over, &area, over.action);
                Some((choice, slot, info, over.local))
            })
        };
        self.clear();
        let Some((choice, slot, info, local)) = taken else {
            finish(origin, false, None);
            return;
        };
        let action = match compositor_action.or(info.action) {
            Some(DragAction::Ask) | None => choice.preferred,
            Some(action) => action,
        };
        let action = if action == DragAction::Ask {
            choice
                .allowed
                .intersect(DragActions::COPY_MOVE)
                .first()
                .unwrap_or(DragAction::Copy)
        } else {
            action
        };
        let data = match local {
            Some(id) => with_outgoing(id, |payload| payload.received(&choice.format)).flatten(),
            None => receive(origin, choice.mime.clone()).await,
        };
        let Some(data) = data else {
            finish(origin, false, None);
            return;
        };
        let on_drop = slot.config.borrow().on_drop.clone();
        finish(
            origin,
            true,
            (compositor_action == Some(DragAction::Ask)).then_some(action),
        );
        if let Some(on_drop) = on_drop {
            on_drop.call(Dropped {
                data,
                position: info.position,
                action,
            });
        }
        if let Some(id) = local {
            local_dropped(id, action);
        }
    }
}

/// The action a target without its own choice prefers: Copy, Move with
/// Shift, Copy with Ctrl.
fn default_preferred() -> DragAction {
    let held = freya_winit::modifiers::live();
    if held.contains(Modifiers::SHIFT) && !held.contains(Modifiers::CONTROL) {
        DragAction::Move
    } else {
        DragAction::Copy
    }
}

/// Read the drop as `mime`.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
async fn receive(origin: Origin, mime: String) -> Option<Received> {
    match origin {
        #[cfg(target_os = "linux")]
        Origin::Wayland(serial) => {
            let worker = wayland::worker()?;
            let (reply, answer) = futures_channel::oneshot::channel();
            worker.send(wayland::Command::Receive {
                serial,
                mime: mime.clone(),
                reply,
            });
            match answer.await {
                Ok(Ok(bytes)) => Some(Received::new(mime, bytes)),
                Ok(Err(error)) => {
                    eprintln!("bingux-ui: could not read the drop: {error}");
                    None
                }
                Err(_) => None,
            }
        }
        _ => None,
    }
}

/// Tell the source the drop is handled (`ok`) or refused.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn finish(origin: Origin, ok: bool, chosen: Option<DragAction>) {
    #[cfg(target_os = "linux")]
    if let Origin::Wayland(serial) = origin
        && let Some(worker) = wayland::worker()
    {
        worker.send(wayland::Command::Finish {
            serial,
            ok,
            action: chosen.map(|action| dnd_actions(action.into())),
        });
    }
}

// ---------------------------------------------------------------------------
// Sources

/// The drag this program started: its payload, for targets of this program
/// and for data requests, and what to tell the source when it ends.
struct Outgoing {
    id: u64,
    payload: Payload,
    on_end: Option<EventHandler<DragEnd>>,
    /// Set by a target of this program, which knows the action first.
    ended: Option<DragAction>,
}

thread_local! {
    static OUTGOING: RefCell<Option<Outgoing>> = const { RefCell::new(None) };
    static NEXT_DRAG: Cell<u64> = const { Cell::new(1) };
}

fn outgoing_is(id: u64) -> bool {
    OUTGOING.with_borrow(|outgoing| outgoing.as_ref().is_some_and(|drag| drag.id == id))
}

fn with_outgoing<T>(id: u64, read: impl FnOnce(&Payload) -> T) -> Option<T> {
    OUTGOING.with_borrow(|outgoing| {
        outgoing
            .as_ref()
            .filter(|drag| drag.id == id)
            .map(|drag| read(&drag.payload))
    })
}

fn local_dropped(id: u64, action: DragAction) {
    OUTGOING.with_borrow_mut(|outgoing| {
        if let Some(drag) = outgoing.as_mut().filter(|drag| drag.id == id) {
            drag.ended = Some(action);
        }
    });
}

/// End drag `id`: tell its source how it went.
fn end_drag(id: u64, end: DragEnd) {
    let drag = OUTGOING.with_borrow_mut(|outgoing| outgoing.take_if(|drag| drag.id == id));
    if let Some(drag) = drag {
        let end = match (end, drag.ended) {
            (DragEnd::Cancelled, Some(action)) | (DragEnd::Dropped(_), Some(action)) => {
                DragEnd::Dropped(action)
            }
            (end, None) => end,
        };
        if let Some(on_end) = drag.on_end {
            on_end.call(end);
        }
    }
}

/// A drag to start: what it carries and how it looks.
#[derive(Clone)]
pub struct DragRequest {
    payload: Payload,
    actions: DragActions,
    image: DragImage,
    area: Option<Area>,
    pointer: Point,
    touch: bool,
    on_end: Option<EventHandler<DragEnd>>,
}

impl DragRequest {
    /// A drag of `payload`, held at `pointer` (window pixels): where the
    /// press that started it was.
    pub fn new(payload: Payload, pointer: Point) -> Self {
        Self {
            payload,
            actions: DragActions::COPY,
            image: DragImage::Snapshot,
            area: None,
            pointer,
            touch: false,
            on_end: None,
        }
    }

    /// The actions the drag allows. Copy unless set.
    pub fn actions(mut self, actions: DragActions) -> Self {
        self.actions = actions;
        self
    }

    pub fn image(mut self, image: DragImage) -> Self {
        self.image = image;
        self
    }

    /// The box of what is dragged, in window pixels: the picture for
    /// [`DragImage::Snapshot`].
    pub fn area(mut self, area: Area) -> Self {
        self.area = Some(area);
        self
    }

    /// The drag is a finger's: it stays in the window (Wayland drags
    /// follow a pointer button).
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }

    pub fn on_end(mut self, on_end: impl Into<EventHandler<DragEnd>>) -> Self {
        self.on_end = Some(on_end.into());
        self
    }
}

/// Starts drags from a component with its own gesture. Cheap to clone.
#[derive(Clone, PartialEq)]
pub struct Dragger {
    hub: Option<DropHub>,
}

/// A [`Dragger`] for this window.
pub fn use_drag() -> Dragger {
    let hub = use_try_consume::<DropHub>();
    Dragger { hub }
}

impl Dragger {
    /// The [`Dragger`] of the window an event handler runs in: for a view
    /// whose handlers are not where its hooks are.
    pub fn here() -> Self {
        Self {
            hub: try_consume_context::<DropHub>(),
        }
    }

    /// Whether a drag from this window can reach other windows and
    /// programs: a Wayland connection with drag and drop is up. A view that
    /// keeps its own drag inside the window hands it over (with
    /// [`start`](Self::start)) only when this is true, as the pointer leaves
    /// the window.
    pub fn reaches_other_programs(&self) -> bool {
        #[cfg(target_os = "linux")]
        if let Some(hub) = &self.hub {
            return wayland::worker().is_some() && hub.0.surface.get().is_some();
        }
        false
    }

    /// Start a drag. Call it while the pointer button that pressed is still
    /// held (from a pointer move past [`DRAG_SLOP`]). Returns whether the
    /// drag went to the system; when it did, this window receives no more
    /// pointer events until the drag ends, and `on_end` says how it ended.
    /// Otherwise it is a drag in the window, and the caller feeds it with
    /// [`moved`](Self::moved) and [`released`](Self::released).
    pub fn start(&self, request: DragRequest) -> bool {
        let id = NEXT_DRAG.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        // A drag still going (its end never came) gives way.
        if let Some(previous) = OUTGOING.with_borrow(|outgoing| outgoing.as_ref().map(|d| d.id)) {
            end_drag(previous, DragEnd::Cancelled);
        }
        OUTGOING.with_borrow_mut(|outgoing| {
            *outgoing = Some(Outgoing {
                id,
                payload: request.payload.clone(),
                on_end: request.on_end.clone(),
                ended: None,
            });
        });
        #[cfg(target_os = "linux")]
        if !request.touch
            && let Some(hub) = &self.hub
            && let (Some(worker), Some(surface)) = (wayland::worker(), hub.0.surface.get())
        {
            start_wayland_drag(worker, surface, id, request);
            return true;
        }
        if let Some(hub) = &self.hub {
            let mut mime_types = request.payload.mime_types();
            mime_types.push(local_marker(id));
            hub.enter(Origin::Window, mime_types, request.actions, request.pointer);
        }
        false
    }

    /// The pointer moved during a drag in the window.
    pub fn moved(&self, at: Point) {
        if let Some(hub) = &self.hub
            && hub
                .0
                .over
                .borrow()
                .as_ref()
                .is_some_and(|over| over.origin == Origin::Window)
        {
            hub.motion(at);
        }
    }

    /// The pointer went up during a drag in the window: drop it.
    pub fn released(&self, at: Point) {
        let Some(id) = OUTGOING.with_borrow(|outgoing| outgoing.as_ref().map(|drag| drag.id))
        else {
            return;
        };
        let Some(hub) = self.hub.clone() else {
            end_drag(id, DragEnd::Cancelled);
            return;
        };
        hub.motion(at);
        spawn_in_scope(
            async move {
                hub.drop(Origin::Window, None).await;
                end_drag(id, DragEnd::Cancelled);
            },
            self.hub.as_ref().map(|hub| hub.0.scope),
        );
    }

    /// Escape: cancel the drag in hand.
    pub fn cancel(&self) {
        if let Some(hub) = &self.hub {
            hub.clear();
        }
        #[cfg(target_os = "linux")]
        if let Some(worker) = wayland::worker() {
            worker.send(wayland::Command::CancelDrag);
        }
        if let Some(id) = OUTGOING.with_borrow(|outgoing| outgoing.as_ref().map(|drag| drag.id)) {
            end_drag(id, DragEnd::Cancelled);
        }
    }
}

#[cfg(target_os = "linux")]
fn start_wayland_drag(
    worker: &'static wayland::Worker,
    surface: usize,
    id: u64,
    request: DragRequest,
) {
    let (events, mut receiver): (_, UnboundedReceiver<wayland::SourceEvent>) = unbounded();
    let mut mime_types = request.payload.mime_types();
    mime_types.push(local_marker(id));
    let area = match &request.image {
        DragImage::None => None,
        DragImage::Snapshot => request.area,
        DragImage::Area(area) => Some(*area),
    };
    let actions = dnd_actions(request.actions);
    let pointer = request.pointer;
    let window = Platform::get().current_window();
    spawn_in_window(async move {
        let icon = match area {
            Some(area) => snapshot(&window, area, pointer).await,
            None => None,
        };
        worker.send(wayland::Command::StartDrag {
            surface,
            mime_types,
            actions,
            icon,
            events,
        });
        let mut action = None;
        while let Some(event) = receiver.next().await {
            match event {
                wayland::SourceEvent::Send { mime, pipe } => {
                    if mime.starts_with(LOCAL_MARKER) {
                        drop(pipe);
                        continue;
                    }
                    if let Some(bytes) =
                        with_outgoing(id, |payload| payload.bytes_for(&mime)).flatten()
                    {
                        wayland::write_off_thread(pipe, bytes);
                    }
                }
                wayland::SourceEvent::Action(chosen) => action = from_dnd(chosen).first(),
                wayland::SourceEvent::Dropped => {}
                wayland::SourceEvent::Finished => {
                    end_drag(id, DragEnd::Dropped(action.unwrap_or(DragAction::Copy)));
                    return;
                }
                wayland::SourceEvent::Cancelled => {
                    end_drag(id, DragEnd::Cancelled);
                    return;
                }
            }
        }
        end_drag(id, DragEnd::Cancelled);
    });
}

/// A picture of `area` of the window as it is drawn now, faded a little as
/// a lifted thing is, held at `pointer`.
#[cfg(target_os = "linux")]
async fn snapshot(window: &PlatformWindow, area: Area, pointer: Point) -> Option<wayland::Icon> {
    use freya::engine::prelude::{AlphaType, ColorType, ImageInfo};

    let scale = (*window.scale_factor.peek() as f32).max(1.0);
    // A whole number of buffer pixels a logical pixel: Wayland's buffer
    // scale is an integer. A fractional scale draws the picture a little
    // smaller than the source.
    let buffer_scale = scale.round().max(1.0) as i32;
    let x = (area.min_x() * scale).round() as i32;
    let y = (area.min_y() * scale).round() as i32;
    let width = (area.width() * scale).round() as i32;
    let height = (area.height() * scale).round() as i32;
    if width <= 0 || height <= 0 || width > 4096 || height > 4096 {
        return None;
    }
    let pixels = window
        .post_render_callback(move |surface| {
            let info = ImageInfo::new(
                (width, height),
                ColorType::BGRA8888,
                AlphaType::Premul,
                None,
            );
            let row = width as usize * 4;
            let mut pixels = vec![0u8; row * height as usize];
            surface
                .canvas()
                .read_pixels(&info, &mut pixels, row, (x, y))
                .then_some(pixels)
        })
        .await
        .ok()
        .flatten()?;
    // Lifted: 85% opaque. Premultiplied, so every channel scales.
    let pixels = pixels
        .into_iter()
        .map(|channel| ((u16::from(channel) * 217) / 255) as u8)
        .collect();
    let hotspot = (
        ((pointer.0 - area.min_x()) * scale / buffer_scale as f32).round() as i32,
        ((pointer.1 - area.min_y()) * scale / buffer_scale as f32).round() as i32,
    );
    Some(wayland::Icon {
        width: width as u32,
        height: height as u32,
        scale: buffer_scale,
        pixels,
        hotspot,
    })
}

/// A view that can be dragged: a press and a move past [`DRAG_SLOP`]
/// starts a drag of the payload `make` returns.
///
/// ```ignore
/// drag_source(file_tile(..), move || Payload::files([path.clone()]))
///     .actions(DragActions::COPY_MOVE)
/// ```
#[derive(Clone, PartialEq)]
pub struct DragSource {
    child: Element,
    payload: Callback<(), Payload>,
    actions: DragActions,
    image: DragImage,
    on_end: Option<EventHandler<DragEnd>>,
    enabled: bool,
}

/// `child`, draggable; `payload` makes what the drag carries when it starts.
pub fn drag_source(
    child: impl Into<Element>,
    payload: impl FnMut(()) -> Payload + 'static,
) -> DragSource {
    DragSource {
        child: child.into(),
        payload: Callback::new(payload),
        actions: DragActions::COPY,
        image: DragImage::Snapshot,
        on_end: None,
        enabled: true,
    }
}

impl DragSource {
    /// The actions the drag allows. Copy unless set.
    pub fn actions(mut self, actions: DragActions) -> Self {
        self.actions = actions;
        self
    }

    /// The picture that follows the pointer: the view itself unless set.
    pub fn image(mut self, image: DragImage) -> Self {
        self.image = image;
        self
    }

    pub fn on_end(mut self, on_end: impl Into<EventHandler<DragEnd>>) -> Self {
        self.on_end = Some(on_end.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Gesture {
    Idle,
    Pressed {
        at: Point,
        touch: bool,
    },
    /// A drag in the window: this component feeds it.
    InWindow,
}

impl Component for DragSource {
    fn render(&self) -> impl IntoElement {
        let dragger = use_drag();
        let mut gesture = use_state(|| Gesture::Idle);
        let mut area = use_state(|| None::<Area>);
        let enabled = self.enabled;
        let payload = self.payload.clone();
        let actions = self.actions;
        let image = self.image.clone();
        let on_end = self.on_end.clone();
        let mover = dragger.clone();
        let releaser = dragger.clone();
        rect()
            .component("DragSource")
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(Some(event.area)))
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let touch = is_touch(&event);
                if !enabled || (!touch && event.button() != Some(MouseButton::Left)) {
                    return;
                }
                gesture.set(Gesture::Pressed {
                    at: point_of(&event),
                    touch,
                });
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let at = point_of(&event);
                let now = *gesture.peek();
                match now {
                    Gesture::Pressed { at: from, touch } => {
                        if (at.0 - from.0).hypot(at.1 - from.1) <= DRAG_SLOP {
                            return;
                        }
                        let mut request = DragRequest::new(payload.call(()), from)
                            .actions(actions)
                            .image(image.clone())
                            .touch(touch);
                        if let Some(area) = *area.peek() {
                            request = request.area(area);
                        }
                        if let Some(on_end) = on_end.clone() {
                            request = request.on_end(on_end);
                        }
                        let system = mover.start(request);
                        gesture.set(if system {
                            Gesture::Idle
                        } else {
                            Gesture::InWindow
                        });
                        if !system {
                            mover.moved(at);
                        }
                    }
                    Gesture::InWindow => mover.moved(at),
                    Gesture::Idle => {}
                }
            })
            .on_global_pointer_up(move |event: Event<PointerEventData>| {
                if *gesture.peek() == Gesture::InWindow {
                    releaser.released(point_of(&event));
                }
                gesture.set_if_modified(Gesture::Idle);
            })
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape) && *gesture.peek() == Gesture::InWindow
                {
                    dragger.cancel();
                    gesture.set(Gesture::Idle);
                }
            })
            .child(self.child.clone())
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes guarded: a drag of another program (or an old drag of
    //! this one) taken for this program's and read from memory, and actions
    //! that do not combine as Wayland combines them.
    use super::*;

    #[test]
    fn only_this_programs_marker_is_local() {
        let mine = vec![URI_LIST.to_string(), local_marker(7)];
        assert_eq!(local_id(&mine), Some(7));
        let theirs = vec![format!("{LOCAL_MARKER};pid=1;id=7")];
        assert_eq!(local_id(&theirs), None);
        assert_eq!(local_id(&[URI_LIST.to_string()]), None);
    }

    #[test]
    fn the_marker_is_not_a_format() {
        let offered = vec![URI_LIST.to_string(), local_marker(1)];
        assert_eq!(Format::list(&offered), vec![Format::Uris]);
    }

    #[test]
    fn actions_combine_as_sets() {
        let both = DragActions::COPY | DragActions::MOVE;
        assert_eq!(both, DragActions::COPY_MOVE);
        assert_eq!(
            both.intersect(DragActions::MOVE).first(),
            Some(DragAction::Move)
        );
        assert_eq!(DragActions::NONE.first(), None);
        assert!(DragActions::ALL.contains(DragAction::Ask));
    }
}
