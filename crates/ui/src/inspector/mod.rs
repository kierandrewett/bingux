//! The inspector: a devtools window that watches one other window.
//!
//! [`plugin`] is installed by [`crate::app`]'s single launch function, so every
//! bingux-ui window has it and no application writes any code for it.
//! Ctrl+Alt+Shift+I, pressed in any window (the main window, an inspector,
//! whichever has keyboard focus) opens an inspector over that window, or
//! closes the one that is open. The plugin sees the key before the window's
//! content does, so a focused text field cannot swallow it.
//!
//! Each inspector is bound to one target window ([`target`]): its element
//! tree, inspect mode, Properties, Performance, Accessibility and phone
//! preview all act on that window. An inspector's own window can be the
//! target of another inspector, to any depth up to [`target::MAX_DEPTH`].
//! While no inspector exists the plugin only looks up empty tables: nothing
//! is captured, timed or sampled.
//!
//! `BINGUX_INSPECT_OPEN=1` opens an inspector over the first window that
//! appears, for tests.

mod a11y;
mod a11y_checks;
mod box_model;
mod computed;
mod details;
mod dom_tree;
mod errors;
mod editor;
mod highlight;
pub(crate) mod layout_checks;
mod layout_tab;
mod nav;
mod overrides;
mod perf;
mod perf_log;
mod perf_sample;
mod watchdog;
mod props;
mod route;
mod shot;
#[cfg(feature = "testing")]
mod probe_checks;
pub(crate) mod snapshot;
#[cfg(feature = "testing")]
pub(crate) mod state_checks;
mod target;
mod toolbar;
mod window;

use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use freya::prelude::*;

use crate::style::{PointerKind, SizeClass, force_density};
use crate::theme::{PHONE_PREVIEW_SIZE, Tokens};
use freya::winit::window::WindowId;
use freya_core::tree::Tree;
use freya_winit::plugins::{FreyaPlugin, Key, Modifiers, PluginEvent, PluginHandle};
use freya_winit::renderer::{NativeEvent, NativeWindowEvent, NativeWindowEventAction};

use snapshot::Snapshot;
use target::{InstanceId, Registry, Throttle, Toggle};

/// The plugin that carries the inspector in every application. Only
/// [`crate::app`] installs it.
pub(crate) fn plugin() -> InspectorPlugin {
    InspectorPlugin
}

/// The inspector's plugin hook. It holds no state: the state lives in the
/// thread-local store, which the windows and the hook share on the UI thread.
pub struct InspectorPlugin;

impl FreyaPlugin for InspectorPlugin {
    fn plugin_id(&self) -> &'static str {
        "bingux-ui.inspector"
    }

    fn on_event(&mut self, event: &mut PluginEvent, handle: PluginHandle) {
        remember_handle(&handle);
        // The launch request resolves on the event loop, which may be idle (an
        // application at rest never measures layout), so every event checks it.
        poll_pending();
        let event_window = match &*event {
            PluginEvent::AfterRedraw { window, .. }
            | PluginEvent::BeforeRender { window, .. }
            | PluginEvent::BeforeMeasuringLayout { window, .. } => Some(window.id()),
            _ => None,
        };
        if let Some(id) = event_window {
            // Frames are timed only for a window an inspector watches.
            for instance in watching(id) {
                with(instance, |instance| {
                    perf::on_event(instance.id, &*event, &mut instance.history)
                });
            }
        }
        // The frame log and the watchdog follow application windows, not the
        // inspector's own.
        if !event_window.is_some_and(is_inspector_window) {
            perf_log::on_event(&*event);
            watchdog::on_event(&*event);
        }
        match event {
            PluginEvent::WindowCreated { window, .. } => {
                let first = STORE.with(|store| !std::mem::replace(&mut store.borrow_mut().seen_window, true));
                if first && open_at_start() {
                    let title = window.title();
                    let _ = open_inspector(window.id(), &title);
                }
            }
            PluginEvent::KeyboardInput {
                window,
                key,
                code,
                modifiers,
                is_pressed,
            } => {
                let id = window.id();
                // The modifiers the window reported with the key, and those the
                // process saw held: a window that just got focus can lag behind.
                let modifiers = *modifiers | freya_winit::modifiers::live();
                if key_log() {
                    eprintln!(
                        "[inspector-key] window {id:?} {} key={key:?} code={code:?} mods={modifiers:?}",
                        if *is_pressed { "down" } else { "up" },
                    );
                }
                STORE.with(|store| {
                    store.borrow_mut().shift_held = modifiers.contains(Modifiers::SHIFT);
                });
                if !*is_pressed && is_letter(key, *code, Code::KeyI, "i") {
                    STORE.with(|store| store.borrow_mut().toggle_down = false);
                }
                // One toggle per physical press: a held key repeats.
                if *is_pressed
                    && is_toggle_key(key, *code, modifiers)
                    && !STORE.with(|store| std::mem::replace(&mut store.borrow_mut().toggle_down, true))
                {
                    toggle_in(id, &window.title());
                }
                if *is_pressed && is_inspect_key(key, *code, modifiers) {
                    toggle_picking_in(id, &window.title());
                }
                if !*is_pressed && matches!(key, Key::Named(NamedKey::Escape)) {
                    cancel_picking_in(id);
                }
                if *is_pressed {
                    for instance in watching(id) {
                        with(instance, |instance| {
                            instance.history.record_input(Instant::now())
                        });
                    }
                }
            }
            PluginEvent::PointerInput {
                window,
                position,
                pressed,
                consumed,
            } => {
                for instance in watching(window.id()) {
                    with(instance, |instance| {
                        instance.history.record_input(Instant::now())
                    });
                    **consumed |= pick_at(instance, effective_scale(window.id()), *position, *pressed);
                }
            }
            PluginEvent::BeforeMeasuringLayout { window, tree } => {
                let scale = window.scale_factor() as f32;
                STORE.with(|store| store.borrow().overrides.apply(window.id(), tree, scale));
            }
            PluginEvent::FinishedMeasuringLayout { window, tree } => {
                for instance in watching(window.id()) {
                    capture(instance, window.id(), tree);
                }
            }
            PluginEvent::AfterRender { window, canvas, .. } => {
                let id = window.id();
                let depth = STORE.with(|store| store.borrow().registry.depth_of(id));
                if depth > 0 {
                    shot::after_render(depth, canvas);
                }
                let mut watched = false;
                for instance in watching(id) {
                    watched = true;
                    highlight::draw(canvas, instance);
                }
                if watched && depth == 0 {
                    shot::after_render(0, canvas);
                }
            }
            PluginEvent::WindowClosed { window, .. } => window_closed(window.id()),
            _ => {}
        }
    }
}

/// Whether the key is the letter `letter` (lower case): by the physical key
/// (`code`, whatever the layout) or by the character it produced. With Ctrl
/// held some layouts report a control character, so the character alone is
/// not enough. Lock keys are not modifiers here, and the order the keys went
/// down in does not matter: only which are held.
fn is_letter(key: &Key, code: Code, want: Code, letter: &str) -> bool {
    code == want || matches!(key, Key::Character(text) if text.eq_ignore_ascii_case(letter))
}

/// Ctrl+Alt+Shift+I. Extra modifiers (Super) do not matter.
fn is_toggle_key(key: &Key, code: Code, modifiers: Modifiers) -> bool {
    let chord = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
    modifiers.contains(chord) && is_letter(key, code, Code::KeyI, "i")
}

/// Ctrl+Shift+C: turns inspect mode on or off. Not with Alt, which is the
/// toggle chord's.
fn is_inspect_key(key: &Key, code: Code, modifiers: Modifiers) -> bool {
    let chord = Modifiers::CONTROL | Modifiers::SHIFT;
    modifiers.contains(chord)
        && !modifiers.contains(Modifiers::ALT)
        && is_letter(key, code, Code::KeyC, "c")
}

/// `BINGUX_INSPECT_KEYLOG=1` prints every key event the plugin sees.
fn key_log() -> bool {
    static ON: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var_os("BINGUX_INSPECT_KEYLOG").is_some_and(|v| v == "1"));
    *ON
}

/// `BINGUX_INSPECT_OPEN=1` opens an inspector over the first window.
fn open_at_start() -> bool {
    std::env::var_os("BINGUX_INSPECT_OPEN").is_some_and(|value| value == "1")
}

/// A window launch waiting for the event loop to create the window.
type Pending = Pin<Box<dyn Future<Output = WindowId>>>;

/// One inspector's state. Its window and target are in the [`Registry`].
struct Instance {
    id: InstanceId,
    /// The launch request sent to the event loop, until its window exists.
    pending: Option<Pending>,
    /// The window's title, naming the target. Set once the window exists.
    title: String,
    snapshot: Option<Rc<Snapshot>>,
    /// Bumped on every new snapshot, so the window knows to re-read it.
    version: u64,
    /// The node the pointer rests on in the tree view.
    hover: Option<u64>,
    /// Pick mode is on: a move previews the node under the cursor, a press
    /// chooses it.
    picking: bool,
    /// A node chosen by a press in pick mode, until the inspector takes it.
    picked: Option<u64>,
    /// Snapshots are frozen: the tree keeps the last one measured.
    paused: bool,
    /// The inspector window's tokens, for the hover box colour.
    tokens: Option<Tokens>,
    /// Input and frame timings of the target the accessibility checks read.
    history: a11y::FrameHistory,
    /// Holds snapshots of a target that redraws every frame to a few a second.
    throttle: Throttle,
    /// A press in pick mode was swallowed, so its release is swallowed too.
    swallow_release: bool,
    /// What the user should be told, shown once by the window.
    notice: Option<String>,
    /// Whether the target is shown at phone size, and the size to go back to.
    phone: Option<State<bool>>,
    phone_saved: Option<freya::winit::dpi::LogicalSize<f64>>,
}

impl Instance {
    fn new(id: InstanceId, title: String) -> Self {
        Instance {
            id,
            title,
            pending: None,
            snapshot: None,
            version: 0,
            hover: None,
            picking: false,
            picked: None,
            paused: false,
            tokens: None,
            history: a11y::FrameHistory::default(),
            throttle: Throttle::default(),
            swallow_release: false,
            notice: None,
            phone: None,
            phone_saved: None,
        }
    }
}

#[derive(Default)]
struct Store {
    /// Which window each inspector watches.
    registry: Registry<WindowId>,
    instances: HashMap<InstanceId, Instance>,
    /// A window has been created, so the start-up request has been honoured.
    seen_window: bool,
    /// Live property edits, re-applied before every layout pass.
    overrides: overrides::Overrides,
    /// The plugin handle, used to ask a window to lay out again.
    handle: Option<PluginHandle>,
    /// Shift is held: a press in pick mode keeps the mode on for the next pick.
    shift_held: bool,
    /// The toggle's key is down, so its auto-repeat does not toggle again.
    toggle_down: bool,
    /// How to read each window's navigation stack, registered by the window's
    /// router. A reader, not a copy: the pane shows the stack as it is when
    /// it renders.
    navigation: HashMap<WindowId, Rc<dyn Fn() -> crate::nav::Snapshot>>,
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::new(Store::default());
}

/// Runs `f` on one instance, if it still exists.
fn with<R>(id: InstanceId, f: impl FnOnce(&mut Instance) -> R) -> Option<R> {
    STORE.with(|store| store.borrow_mut().instances.get_mut(&id).map(f))
}

/// The inspectors watching `window`: none for nearly every window.
fn watching(window: WindowId) -> Vec<InstanceId> {
    STORE.with(|store| store.borrow().registry.watching(window).collect())
}

fn is_inspector_window(window: WindowId) -> bool {
    STORE.with(|store| store.borrow().registry.is_inspector_window(window))
}

/// The inspector the calling component, event handler or task belongs to.
/// Only the inspector's own windows call this.
fn me() -> Option<InstanceId> {
    let window = Platform::window_id();
    STORE.with(|store| store.borrow().registry.instance_of_window(window))
}

/// Runs `f` on the calling window's instance. `None` before the window has
/// been bound to one.
fn with_me<R>(f: impl FnOnce(&mut Instance) -> R) -> Option<R> {
    me().and_then(|id| with(id, f))
}

/// Called by a window's router: the inspector reads its stack with `reader`.
/// Registering again replaces the reader.
pub(crate) fn register_navigation(window: WindowId, reader: Rc<dyn Fn() -> crate::nav::Snapshot>) {
    STORE.with(|store| store.borrow_mut().navigation.insert(window, reader));
}

/// Called when a window's router goes away.
pub(crate) fn unregister_navigation(window: WindowId) {
    STORE.with(|store| store.borrow_mut().navigation.remove(&window));
}

/// The navigation stack of `window`, if its router registered one.
pub(crate) fn navigation(window: WindowId) -> Option<crate::nav::Snapshot> {
    // Cloned out first: the reader reads a Freya state and must not run
    // with the store borrowed.
    let reader = STORE.with(|store| store.borrow().navigation.get(&window).cloned())?;
    Some(reader())
}

/// The window the calling inspector watches.
pub(crate) fn app_window() -> Option<WindowId> {
    let id = me()?;
    STORE.with(|store| store.borrow().registry.target_of(id))
}

/// Binds an inspector window to its instance. An instance whose target
/// closed meanwhile no longer exists: its window is closed at once.
fn bind_window(id: InstanceId, window: WindowId) {
    let (known, bound_to) = STORE.with(|store| {
        let store = store.borrow();
        (store.registry.contains(id), store.registry.window_of(id))
    });
    if bound_to == Some(window) {
        return;
    }
    if !known {
        Platform::get().close_window(window);
        return;
    }
    if !STORE.with(|store| store.borrow_mut().registry.bind(id, window)) {
        return;
    }
    // Logged so a launch can be confirmed from stderr without a screenshot.
    let (target, depth) = STORE.with(|store| {
        let store = store.borrow();
        (store.registry.target_of(id), store.registry.depth(id))
    });
    if let Some(title) = with(id, |instance| instance.title.clone()) {
        Platform::get().set_window_title(window, title);
    }
    register_in_stack(id, window);
    // A new window may not get keyboard focus from the compositor; ask, so the
    // shortcut works in it without a click first.
    Platform::get().focus_window(window);
    eprintln!(
        "[inspector] window open (depth {}, target {:?})",
        depth.unwrap_or(0),
        target
    );
}

/// Called by an inspector window's root on render, so its components find
/// their instance before the plugin has seen the window created.
fn bind_current(id: InstanceId) {
    let window = Platform::window_id();
    if STORE.with(|store| store.borrow().registry.window_of(id)) != Some(window) {
        bind_window(id, window);
    }
}

/// An inspector is a tool window of its target: above it in the window stack
/// (`layers::WindowStack`), whatever else the application opens over the target.
fn register_in_stack(id: InstanceId, window: WindowId) {
    let Some(target) = STORE.with(|store| store.borrow().registry.target_of(id)) else {
        return;
    };
    crate::layers::with_stack(|stack| stack.open_tool(u64::from(window), u64::from(target), "inspector"));
}

/// Opens an inspector over `target`, whose title is `target_title`. The window is a child of
/// its target (`xdg_toplevel.set_parent`): the compositor keeps it above the target, so it is
/// never lost behind the window it inspects, and it goes with it.
fn open_inspector(target: WindowId, target_title: &str) -> Result<InstanceId, target::OpenError> {
    let id = STORE.with(|store| store.borrow_mut().registry.open(target))?;
    // Launching is only a request to the event loop, and `spawn` needs a
    // Freya scope that plugin callbacks do not have. The request is sent
    // now; `poll_pending` collects the window id on later events.
    let pending: Pending = Box::pin(async move {
        Platform::get()
            .launch_window(
                WindowConfig::new({
                    // The inspector is a window like any other: it fails into
                    // its own error card, never the target's.
                    let root: Rc<dyn Fn() -> Element> = Rc::new(move || window::root(id));
                    move || crate::resilience::window_boundary("The inspector", root.clone())
                })
                    .with_title("Inspector")
                    .with_parent(target)
                    .with_size(1000.0, 680.0)
                    .with_min_size(640.0, 420.0)
                    // Its own header bar draws the window controls.
                    .with_decorations(false),
            )
            .await
    });
    let mut instance = Instance::new(id, target::window_title(target_title));
    instance.pending = Some(pending);
    STORE.with(|store| store.borrow_mut().instances.insert(id, instance));
    poll_pending();
    // An idle target never measures layout on its own, so without this the
    // inspector would wait for an unrelated repaint before its first tree.
    request_layout(target);
    Ok(id)
}

/// Ctrl+Alt+Shift+I in `window`: opens an inspector over it, or closes the
/// one that is open. In an inspector's window that is an inspector of the
/// inspector.
fn toggle_in(window: WindowId, title: &str) {
    let toggle = STORE.with(|store| store.borrow().registry.toggle_for(window));
    match toggle {
        Toggle::Open => {
            let _ = open_inspector(window, title);
        }
        Toggle::Close(id) => {
            // An inspector still waiting for its window is left to open.
            let own = STORE.with(|store| store.borrow().registry.window_of(id));
            if let Some(own) = own {
                Platform::get().close_window(own);
            }
        }
        Toggle::Refuse(error) => notify(window, error.message()),
    }
}

/// Tells the user something in `window`, which is an inspector's.
fn notify(window: WindowId, message: String) {
    eprintln!("[inspector] {message}");
    let instance = STORE.with(|store| store.borrow().registry.instance_of_window(window));
    if let Some(instance) = instance {
        with(instance, |instance| instance.notice = Some(message));
    }
}

/// What the calling inspector window should tell its user. Returns it once.
pub(crate) fn take_notice() -> Option<String> {
    with_me(|instance| instance.notice.take()).flatten()
}

/// Ctrl+Shift+C in `window`. In an inspector's window it is that inspector's
/// own inspect button. In any other window it turns inspect mode on in the
/// inspector watching it, opening one first when there is none.
fn toggle_picking_in(window: WindowId, title: &str) {
    let own = STORE.with(|store| store.borrow().registry.instance_of_window(window));
    let instance = own
        .or_else(|| STORE.with(|store| store.borrow().registry.inspector_of(window)))
        .or_else(|| open_inspector(window, title).ok());
    if let Some(instance) = instance {
        let on = with(instance, |instance| instance.picking).unwrap_or(false);
        set_picking_of(instance, !on);
    }
}

/// Escape cancels inspect mode, whichever of the two windows has focus.
fn cancel_picking_in(window: WindowId) {
    let picking: Vec<InstanceId> = STORE.with(|store| {
        let store = store.borrow();
        store
            .instances
            .values()
            .filter(|instance| {
                instance.picking
                    && (store.registry.target_of(instance.id) == Some(window)
                        || store.registry.window_of(instance.id) == Some(window))
            })
            .map(|instance| instance.id)
            .collect()
    });
    for instance in picking {
        set_picking_of(instance, false);
    }
}

/// Polls the launch requests, if any are waiting, and binds each window once
/// the event loop has created it.
fn poll_pending() {
    let waiting: Vec<InstanceId> = STORE.with(|store| {
        let store = store.borrow();
        if store.instances.is_empty() {
            return Vec::new();
        }
        store
            .instances
            .values()
            .filter(|instance| instance.pending.is_some())
            .map(|instance| instance.id)
            .collect()
    });
    for id in waiting {
        let Some(mut pending) = with(id, |instance| instance.pending.take()).flatten() else {
            continue;
        };
        let mut cx = Context::from_waker(Waker::noop());
        match pending.as_mut().poll(&mut cx) {
            Poll::Ready(window) => bind_window(id, window),
            Poll::Pending => {
                with(id, |instance| instance.pending = Some(pending));
            }
        }
    }
}

/// A window closed. Its own inspector goes, and so do the inspectors over it,
/// and over those: their windows are closed, so the process ends when the
/// application's window does.
fn window_closed(window: WindowId) {
    // An inspector closing while the target is at phone size gives the target
    // its own size back.
    if let Some(id) = STORE.with(|store| store.borrow().registry.instance_of_window(window))
        && with(id, |instance| instance.phone.is_some_and(|phone| *phone.peek())).unwrap_or(false)
    {
        apply_phone(id, false);
    }
    let closed = STORE.with(|store| {
        let mut store = store.borrow_mut();
        let closed = store.registry.close(window);
        for id in &closed.removed {
            store.instances.remove(id);
            perf::forget(*id);
        }
        closed
    });
    for own in closed.close {
        Platform::get().close_window(own);
    }
    // Out of the stack with whatever grew from it (an inspector goes with its target).
    crate::layers::with_stack(|stack| stack.close(u64::from(window)));
}

/// Copies the target's tree for the inspector watching it, at most once per
/// [`target::CAPTURE_GAP`]. A refused capture is remembered, and the
/// inspector asks for one more layout when its window next polls.
fn capture(id: InstanceId, window: WindowId, tree: &Tree) {
    let due = with(id, |instance| !instance.paused && instance.throttle.due(Instant::now()));
    if due != Some(true) {
        return;
    }
    let mut snapshot = Snapshot::capture(tree, effective_scale(window));
    if let Some(focus) = Platform::get().try_window(u64::from(window)) {
        snapshot.mark_focused(*focus.focused_accessibility_id.peek());
    }
    with(id, |instance| {
        instance.history.observe(&snapshot, Instant::now());
        instance.snapshot = Some(Rc::new(snapshot));
        instance.version += 1;
    });
}

/// The scale factor `window` lays out and renders at.
fn effective_scale(window: WindowId) -> f32 {
    Platform::get()
        .try_window(u64::from(window))
        .map(|window| *window.scale_factor.peek() as f32)
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0)
}

/// The calling window's current snapshot and its version.
fn snapshot() -> (u64, Option<Rc<Snapshot>>) {
    with_me(|instance| (instance.version, instance.snapshot.clone())).unwrap_or((0, None))
}

/// The accessibility issues of the target tree last measured.
pub(crate) fn issues() -> Vec<a11y::Issue> {
    with_me(|instance| match &instance.snapshot {
        Some(snapshot) => a11y::check(snapshot, &instance.history),
        None => Vec::new(),
    })
    .unwrap_or_default()
}

/// The layout findings of the target tree last measured: visual defects, not
/// accessibility problems.
pub(crate) fn layout_findings() -> Vec<layout_checks::Finding> {
    with_me(|instance| {
        instance
            .snapshot
            .as_deref()
            .map(layout_checks::check)
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// The hovered node of the inspector's snapshot, with that snapshot.
fn hovered(id: InstanceId) -> Option<(Rc<Snapshot>, u64)> {
    with(id, |instance| Some((instance.snapshot.clone()?, instance.hover?))).flatten()
}

/// Sets the hovered node and redraws the target, so the box shows on its
/// canvas.
fn set_hover(node: Option<u64>) {
    if let Some(id) = me() {
        set_hover_of(id, node);
    }
}

fn set_hover_of(id: InstanceId, node: Option<u64>) {
    let changed = with(id, |instance| {
        if instance.hover == node {
            return false;
        }
        instance.hover = node;
        true
    });
    if changed == Some(true) {
        redraw_target(id);
    }
}

fn redraw_target(id: InstanceId) {
    let target = STORE.with(|store| store.borrow().registry.target_of(id));
    if let Some(target) = target {
        Platform::get().with_window(target, |window| window.request_redraw());
    }
}

/// Turns pick mode on or off for the calling inspector.
pub(crate) fn set_picking(picking: bool) {
    if let Some(id) = me() {
        set_picking_of(id, picking);
    }
}

/// Turns pick mode on or off. Either way the hover box is cleared, and the
/// target is redrawn at once so the box leaves its canvas.
fn set_picking_of(id: InstanceId, picking: bool) {
    with(id, |instance| {
        instance.picking = picking;
        instance.picked = None;
        instance.hover = None;
    });
    redraw_target(id);
    if let Some(own) = STORE.with(|store| store.borrow().registry.window_of(id)) {
        Platform::get().with_window(own, |window| window.request_redraw());
    }
}

/// Whether the calling inspector is in pick mode.
pub(crate) fn picking() -> bool {
    with_me(|instance| instance.picking).unwrap_or(false)
}

/// The node a press chose in pick mode. Returns it once.
pub(crate) fn take_pick() -> Option<u64> {
    with_me(|instance| instance.picked.take()).flatten()
}

/// Pick mode: a move under the cursor previews the node there, and a press on a
/// node chooses it and ends pick mode. While pick mode is on, a press and its
/// release are swallowed, so the click never reaches the target. Returns
/// whether this event was swallowed. `position` is in physical pixels.
fn pick_at(id: InstanceId, scale: f32, position: (f64, f64), pressed: Option<bool>) -> bool {
    if pressed == Some(false) {
        // The release of a swallowed press is swallowed too, even if the press
        // ended pick mode.
        return with(id, |instance| std::mem::take(&mut instance.swallow_release))
            .unwrap_or(false);
    }
    let Some((picking, snapshot)) =
        with(id, |instance| (instance.picking, instance.snapshot.clone()))
    else {
        return false;
    };
    if !picking {
        return false;
    }
    let hit = snapshot
        .and_then(|snapshot| snapshot.hit(position.0 as f32 / scale, position.1 as f32 / scale));
    let press = pressed == Some(true);
    let shift = STORE.with(|store| store.borrow().shift_held);
    // A press on a node ends pick mode unless shift is held. No hover box may
    // outlive that: later moves return early and would never clear it.
    let stays_picking = if press {
        with(id, |instance| {
            instance.swallow_release = true;
            if let Some(node) = hit {
                instance.picking = shift;
                instance.picked = Some(node);
            }
            instance.picking
        })
        .unwrap_or(false)
    } else {
        true
    };
    set_hover_of(id, if stays_picking { hit } else { None });
    if press && !stays_picking {
        // The inspector window shows the inspect button as on or off.
        if let Some(own) = STORE.with(|store| store.borrow().registry.window_of(id)) {
            Platform::get().with_window(own, |window| window.request_redraw());
        }
    }
    press
}

/// The calling inspector window's tokens, called by the window whenever it
/// renders, so the hover box follows the accent.
fn set_tokens(tokens: Tokens) {
    with_me(|instance| instance.tokens = Some(tokens));
}

/// The inspector window's tokens, if it has rendered.
fn tokens_of(id: InstanceId) -> Option<Tokens> {
    with(id, |instance| instance.tokens).flatten()
}

/// Remembers the plugin handle, the first time the hook runs.
fn remember_handle(handle: &PluginHandle) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.handle.is_none() {
            store.handle = Some(handle.clone());
        }
    });
}

/// Asks a window to run layout on its next frame. Overrides are applied in
/// that pass, so an edit shows at once.
fn request_layout(window: WindowId) {
    let handle = STORE.with(|store| store.borrow().handle.clone());
    if let Some(handle) = handle {
        handle.send_event_loop_event(NativeEvent::Window(NativeWindowEvent {
            window_id: window,
            action: NativeWindowEventAction::RequestLayout,
        }));
    }
}

/// Whether the calling inspector's snapshots are frozen.
pub(crate) fn paused() -> bool {
    with_me(|instance| instance.paused).unwrap_or(false)
}

/// Freezes or resumes the calling inspector's snapshots.
pub(crate) fn set_paused(paused: bool) {
    with_me(|instance| instance.paused = paused);
}

/// Called by the inspector window's poll: when a snapshot was refused by the
/// throttle, the target is laid out once more so the tree catches up.
pub(crate) fn catch_up() {
    let Some(id) = me() else { return };
    let stale = with(id, |instance| instance.throttle.stale() && !instance.paused);
    if stale == Some(true)
        && let Some(target) = app_window()
    {
        request_layout(target);
    }
}

/// Lays out the target again, applying edits and taking a new snapshot, even
/// when the throttle would hold one back.
pub(crate) fn refresh() {
    let Some(id) = me() else { return };
    if let Some(target) = app_window() {
        with(id, |instance| instance.throttle = Throttle::default());
        request_layout(target);
    }
}

/// Sets one property override on a node of the target. `original` is the text
/// the property had before, kept with the first override.
pub(crate) fn edit(
    window: WindowId,
    node: u64,
    property: &'static str,
    edit: overrides::Edit,
    original: String,
) {
    STORE.with(|store| {
        store
            .borrow_mut()
            .overrides
            .set(window, node, property, edit, original)
    });
    request_layout(window);
}

/// Drops one property override, or every override on the node when `property`
/// is `None`.
pub(crate) fn reset(window: WindowId, node: u64, property: Option<&'static str>) {
    STORE.with(|store| store.borrow_mut().overrides.reset(window, node, property));
    request_layout(window);
}

/// Whether a property of a node of the target is overridden.
pub(crate) fn has_override(window: WindowId, node: u64, property: &'static str) -> bool {
    STORE.with(|store| store.borrow().overrides.has(window, node, property))
}

/// Every override on a node of the target, sorted by property.
pub(crate) fn overrides_of(window: WindowId, node: u64) -> Vec<(&'static str, overrides::Entry)> {
    STORE.with(|store| store.borrow().overrides.list(window, node))
}

/// The density the target renders at, so the inspector reads sizes as the
/// tokens they are there, not at its own density.
pub(crate) fn app_density(window: WindowId) -> SizeClass {
    crate::style::window_density(u64::from(window)).class()
}

/// Whether the calling inspector's target is shown at phone size.
pub(crate) fn phone_preview() -> bool {
    let Some(state) = with_me(|instance| {
        *instance
            .phone
            .get_or_insert_with(|| State::create_global(false))
    }) else {
        return false;
    };
    *state.read()
}

/// Shows the calling inspector's target at phone size (compact, touch), or
/// releases it. Only the target changes; no other window does.
pub(crate) fn set_phone_preview(on: bool) {
    if let Some(id) = me() {
        apply_phone(id, on);
    }
}

fn apply_phone(id: InstanceId, on: bool) {
    let Some(state) = with(id, |instance| {
        *instance
            .phone
            .get_or_insert_with(|| State::create_global(false))
    }) else {
        return;
    };
    let mut state = state;
    state.set(on);
    let Some(target) = STORE.with(|store| store.borrow().registry.target_of(id)) else {
        return;
    };
    // An inspector window keeps its own fixed density when released.
    let base = STORE
        .with(|store| store.borrow().registry.is_inspector_window(target))
        .then_some((SizeClass::Regular, PointerKind::Mouse));
    force_density(
        target,
        if on {
            Some((SizeClass::Compact, PointerKind::Touch))
        } else {
            base
        },
    );
    let (width, height) = PHONE_PREVIEW_SIZE;
    if on {
        // Remember the size the user chose so turning the preview off restores it.
        Platform::get().with_window(target, move |window| {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            with(id, |instance| instance.phone_saved = Some(size));
            let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(
                f64::from(width),
                f64::from(height),
            ));
        });
    } else if let Some(size) = with(id, |instance| instance.phone_saved.take()).flatten() {
        Platform::get().with_window(target, move |window| {
            let _ = window.request_inner_size(size);
        });
    }
    request_layout(target);
}
