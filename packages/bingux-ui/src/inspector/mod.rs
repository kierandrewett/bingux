//!
//!
//! [`plugin`] is registered by [`crate::app::Application`]. Ctrl+Alt+Shift+I
//! opens or closes the inspector, a separate window. While it is open the
//! plugin takes a [`snapshot::Snapshot`] of each application window's element
//! tree after layout, and draws the hovered node's box on the application
//! canvas. `BINGUX_INSPECT_OPEN=1` opens it when the first window appears, for
//! tests.

mod a11y;
mod a11y_checks;
mod box_model;
mod dom_tree;
mod editor;
mod highlight;
mod nav;
mod overrides;
mod perf;
mod perf_sample;
mod shot;
mod snapshot;
mod styles;
mod toolbar;
mod window;

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use freya::prelude::*;

use crate::style::{PointerKind, SizeClass, force_density};
use crate::theme::{PHONE_PREVIEW_SIZE, Tokens};
use freya::winit::window::{Window, WindowId};
use freya_winit::plugins::{FreyaPlugin, Key, Modifiers, PluginEvent, PluginHandle};
use freya_winit::renderer::{NativeEvent, NativeWindowEvent, NativeWindowEventAction};

use snapshot::Snapshot;

/// The plugin that carries the inspector in every application.
pub fn plugin() -> InspectorPlugin {
    InspectorPlugin
}

/// The inspector's plugin hook. It holds no state: the state lives in the
/// thread-local store, which the window and the hook share on the UI thread.
pub struct InspectorPlugin;

impl FreyaPlugin for InspectorPlugin {
    fn plugin_id(&self) -> &'static str {
        "bingux-ui.inspector"
    }

    fn on_event(&mut self, event: &mut PluginEvent, handle: PluginHandle) {
        remember_handle(&handle);
        let inspector = match event {
            PluginEvent::AfterRedraw { window, .. }
            | PluginEvent::BeforeRender { window, .. }
            | PluginEvent::BeforeMeasuringLayout { window, .. } => is_inspector(window.id()),
            _ => false,
        };
        STORE.with(|store| perf::on_event(&*event, inspector, &mut store.borrow_mut().history));
        match event {
            PluginEvent::WindowCreated { window, .. } => {
                let id = window.id();
                if !is_inspector(id) && open_at_start() {
                    STORE.with(|store| store.borrow_mut().auto_opened = true);
                    toggle_window();
                }
            }
            PluginEvent::KeyboardInput {
                window,
                key,
                modifiers,
                is_pressed,
                ..
            } => {
                STORE.with(|store| {
                    store.borrow_mut().shift_held = modifiers.contains(Modifiers::SHIFT);
                });
                if *is_pressed && is_toggle_key(key, *modifiers) && !is_inspector(window.id()) {
                    toggle_window();
                }
                if *is_pressed && is_inspect_key(key, *modifiers) {
                    if !is_open() {
                        toggle_window();
                    }
                    set_picking(!picking());
                }
                if !*is_pressed && matches!(key, Key::Named(NamedKey::Escape)) && picking() {
                    set_picking(false);
                }
                if *is_pressed && is_open() {
                    STORE.with(|store| {
                        store
                            .borrow_mut()
                            .history
                            .record_input(std::time::Instant::now())
                    });
                }
            }
            PluginEvent::PointerInput {
                window,
                position,
                pressed,
                consumed,
            } => {
                if is_open() && !is_inspector(window.id()) {
                    STORE.with(|store| {
                        store
                            .borrow_mut()
                            .history
                            .record_input(std::time::Instant::now())
                    });
                    **consumed = pick_at(window, *position, *pressed);
                }
            }
            PluginEvent::BeforeMeasuringLayout { window, tree } => {
                let scale = window.scale_factor() as f32;
                STORE.with(|store| store.borrow().overrides.apply(window.id(), tree, scale));
            }
            PluginEvent::FinishedMeasuringLayout { window, tree } => {
                poll_pending();
                if is_open() && !is_inspector(window.id()) && !paused() {
                    let snapshot = Snapshot::capture(tree);
                    STORE.with(|store| {
                        let mut store = store.borrow_mut();
                        store.history.observe(&snapshot, std::time::Instant::now());
                        store.app_window = Some(window.id());
                        store.snapshot = Some(Rc::new(snapshot));
                        store.version += 1;
                    });
                }
            }
            PluginEvent::AfterRender { window, canvas, .. } => {
                if is_inspector(window.id()) {
                    shot::after_render(true, canvas);
                } else if is_open() {
                    highlight::draw(canvas);
                    shot::after_render(false, canvas);
                }
            }
            PluginEvent::WindowClosed { window, .. } => {
                if is_inspector(window.id()) {
                    STORE.with(|store| {
                        let mut store = store.borrow_mut();
                        store.inspector = None;
                        store.hover = None;
                        store.picking = false;
                    });
                    set_phone_preview(false);
                }
            }
            _ => {}
        }
    }
}

/// Ctrl+Alt+Shift+I, in either letter case.
fn is_toggle_key(key: &Key, modifiers: Modifiers) -> bool {
    let chord = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
    modifiers == chord && matches!(key, Key::Character(text) if text.eq_ignore_ascii_case("i"))
}

/// Ctrl+Shift+C, in either letter case: turns inspect mode on or off.
fn is_inspect_key(key: &Key, modifiers: Modifiers) -> bool {
    let chord = Modifiers::CONTROL | Modifiers::SHIFT;
    modifiers == chord && matches!(key, Key::Character(text) if text.eq_ignore_ascii_case("c"))
}

/// `BINGUX_INSPECT_OPEN=1` opens the inspector with the first application window.
fn open_at_start() -> bool {
    std::env::var_os("BINGUX_INSPECT_OPEN").is_some_and(|value| value == "1")
}

/// A window launch waiting for the event loop to create the window.
type Pending = Pin<Box<dyn Future<Output = WindowId>>>;

#[derive(Default)]
struct Store {
    /// The window the inspector is in, once it has opened.
    inspector: Option<WindowId>,
    /// The inspector is launching and its id is not known yet.
    opening: bool,
    /// The environment asked for the inspector at start-up, and it has opened.
    auto_opened: bool,
    /// The application window that last measured a tree, and its tree.
    app_window: Option<WindowId>,
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
    /// The launch request sent to the event loop, until its window exists.
    pending: Option<Pending>,
    /// Live property edits, re-applied before every layout pass.
    overrides: overrides::Overrides,
    /// Input and frame timings the accessibility checks read.
    history: a11y::FrameHistory,
    /// The plugin handle, used to ask the application window to lay out again.
    handle: Option<PluginHandle>,
    /// Shift is held: a press in pick mode keeps the mode on for the next pick.
    shift_held: bool,
    /// A press in pick mode was swallowed, so its release is swallowed too.
    swallow_release: bool,
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::new(Store::default());
}

fn is_open() -> bool {
    STORE.with(|store| {
        let store = store.borrow();
        store.inspector.is_some() || store.opening
    })
}

/// The accessibility issues of the application tree last measured.
pub(crate) fn issues() -> Vec<a11y::Issue> {
    STORE.with(|store| {
        let store = store.borrow();
        match &store.snapshot {
            Some(snapshot) => a11y::check(snapshot, &store.history),
            None => Vec::new(),
        }
    })
}

fn is_inspector(id: WindowId) -> bool {
    STORE.with(|store| store.borrow().inspector == Some(id))
}

/// Closes the inspector window if it is open. The toolbar's close button calls
/// this. Unlike [`toggle_window`], it never opens the window.
pub(crate) fn close_inspector() {
    let inspector = STORE.with(|store| store.borrow().inspector);
    if let Some(id) = inspector {
        Platform::get().close_window(id);
    }
}

/// Opens the inspector if it is closed, closes it if it is open.
fn toggle_window() {
    let (inspector, opening) = STORE.with(|store| {
        let store = store.borrow();
        (store.inspector, store.opening)
    });
    if let Some(id) = inspector {
        Platform::get().close_window(id);
    } else if !opening {
        STORE.with(|store| store.borrow_mut().opening = true);
        // Launching is only a request to the event loop, and `spawn` needs a
        // Freya scope that plugin callbacks do not have. The request is sent
        // now; `poll_pending` collects the window id on later layout passes.
        let pending: Pending = Box::pin(async move {
            Platform::get()
                .launch_window(
                    WindowConfig::new(window::root)
                        .with_title("Inspector")
                        .with_size(1000.0, 680.0)
                        .with_min_size(640.0, 420.0),
                )
                .await
        });
        STORE.with(|store| store.borrow_mut().pending = Some(pending));
        poll_pending();
    }
}

/// Polls the launch request, if one is waiting, and records the window once
/// the event loop has created it.
fn poll_pending() {
    let Some(mut pending) = STORE.with(|store| store.borrow_mut().pending.take()) else {
        return;
    };
    let mut cx = Context::from_waker(Waker::noop());
    match pending.as_mut().poll(&mut cx) {
        Poll::Ready(id) => STORE.with(|store| {
            // Logged so a launch can be confirmed from stderr without a screenshot.
            eprintln!("[inspector] window open");
            let mut store = store.borrow_mut();
            store.opening = false;
            store.inspector = Some(id);
        }),
        Poll::Pending => STORE.with(|store| store.borrow_mut().pending = Some(pending)),
    }
}

/// The current snapshot and its version.
fn snapshot() -> (u64, Option<Rc<Snapshot>>) {
    STORE.with(|store| {
        let store = store.borrow();
        (store.version, store.snapshot.clone())
    })
}

/// The hovered node of the latest snapshot, with that snapshot.
fn hovered() -> Option<(Rc<Snapshot>, u64)> {
    STORE.with(|store| {
        let store = store.borrow();
        Some((store.snapshot.clone()?, store.hover?))
    })
}

/// Sets the hovered node and redraws the application window, so the box shows
/// on the canvas.
fn set_hover(node: Option<u64>) {
    let app = STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.hover == node {
            return None;
        }
        store.hover = node;
        store.app_window
    });
    if let Some(app) = app {
        Platform::get().with_window(app, |window| window.request_redraw());
    }
}

/// Turns pick mode on or off. Either way the hover box is cleared, and the
/// application window is redrawn at once so the box leaves the canvas.
pub(crate) fn set_picking(picking: bool) {
    let (inspector, app) = STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.picking = picking;
        store.picked = None;
        store.hover = None;
        (store.inspector, store.app_window)
    });
    if let Some(app) = app {
        Platform::get().with_window(app, |window| window.request_redraw());
    }
    if let Some(window) = inspector {
        Platform::get().with_window(window, |w| w.request_redraw());
    }
}

/// Whether pick mode is on.
pub(crate) fn picking() -> bool {
    STORE.with(|store| store.borrow().picking)
}

/// The node a press chose in pick mode. Returns it once.
pub(crate) fn take_pick() -> Option<u64> {
    STORE.with(|store| store.borrow_mut().picked.take())
}

/// Pick mode: a move under the cursor previews the node there, and a press on a
/// node chooses it and ends pick mode. While pick mode is on, a press and its
/// release are swallowed, so the click never reaches the application. Returns
/// whether this event was swallowed. `position` is in physical pixels.
fn pick_at(window: &Window, position: (f64, f64), pressed: Option<bool>) -> bool {
    if pressed == Some(false) {
        // The release of a swallowed press is swallowed too, even if the press
        // ended pick mode.
        return STORE.with(|store| std::mem::take(&mut store.borrow_mut().swallow_release));
    }
    let (picking, snapshot) = STORE.with(|store| {
        let store = store.borrow();
        (store.picking, store.snapshot.clone())
    });
    if !picking {
        return false;
    }
    let scale = window.scale_factor() as f32;
    let hit = snapshot
        .and_then(|snapshot| snapshot.hit(position.0 as f32 / scale, position.1 as f32 / scale));
    let press = pressed == Some(true);
    // A press on a node ends pick mode unless shift is held. No hover box may
    // outlive that: later moves return early and would never clear it.
    let stays_picking = if press {
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            store.swallow_release = true;
            if let Some(node) = hit {
                store.picking = store.shift_held;
                store.picked = Some(node);
            }
            store.picking
        })
    } else {
        true
    };
    set_hover(if stays_picking { hit } else { None });
    press
}

/// The inspector window's tokens, if it has rendered.
fn tokens() -> Option<Tokens> {
    STORE.with(|store| store.borrow().tokens)
}

/// Called by the inspector window whenever it renders, so the hover box
/// follows the accent.
fn set_tokens(tokens: Tokens) {
    STORE.with(|store| store.borrow_mut().tokens = Some(tokens));
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

/// Asks the application window to run layout on its next frame. Overrides are
/// applied in that pass, so an edit shows at once.
fn request_layout(window: WindowId) {
    let handle = STORE.with(|store| store.borrow().handle.clone());
    if let Some(handle) = handle {
        handle.send_event_loop_event(NativeEvent::Window(NativeWindowEvent {
            window_id: window,
            action: NativeWindowEventAction::RequestLayout,
        }));
    }
}

/// Whether snapshots are frozen.
pub(crate) fn paused() -> bool {
    STORE.with(|store| store.borrow().paused)
}

/// Freezes or resumes snapshots of the application window.
pub(crate) fn set_paused(paused: bool) {
    STORE.with(|store| store.borrow_mut().paused = paused);
}

/// Lays out the application window again, applying edits and taking a new
/// snapshot even while paused.
pub(crate) fn refresh() {
    let app = STORE.with(|store| store.borrow().app_window);
    if let Some(app) = app {
        request_layout(app);
    }
}

/// Sets one property override on a node of the application window. `value` is
/// the text the editor shows for it and `original` the text it replaced.
pub(crate) fn edit(
    window: WindowId,
    node: u64,
    property: &'static str,
    value: String,
    edit: overrides::Edit,
    original: String,
) {
    STORE.with(|store| {
        store
            .borrow_mut()
            .overrides
            .set(window, node, property, value, edit, original)
    });
    request_layout(window);
}

/// Drops one property override, or every override on the node when `property`
/// is `None`.
pub(crate) fn reset(window: WindowId, node: u64, property: Option<&'static str>) {
    STORE.with(|store| store.borrow_mut().overrides.reset(window, node, property));
    request_layout(window);
}

/// Turns one override of a node on or off, keeping its edit.
pub(crate) fn toggle_override(window: WindowId, node: u64, property: &'static str) {
    STORE.with(|store| store.borrow_mut().overrides.toggle(window, node, property));
    request_layout(window);
}

/// Whether a property of a node of the application window is overridden and on.
pub(crate) fn has_override(window: WindowId, node: u64, property: &'static str) -> bool {
    STORE.with(|store| store.borrow().overrides.has(window, node, property))
}

/// Every override on a node of the application window, sorted by property.
pub(crate) fn overrides_of(window: WindowId, node: u64) -> Vec<(&'static str, overrides::Entry)> {
    STORE.with(|store| store.borrow().overrides.list(window, node))
}

/// The application window the inspector is showing.
pub(crate) fn app_window() -> Option<WindowId> {
    STORE.with(|store| store.borrow().app_window)
}

thread_local! {
    static PHONE: State<bool> = State::create_global(false);
    /// The application window's logical size before the phone preview, so
    /// turning the preview off can restore it.
    static PHONE_SAVED_SIZE: RefCell<Option<freya::winit::dpi::LogicalSize<f64>>> =
        const { RefCell::new(None) };
}

/// Whether the application window is shown at phone size.
pub(crate) fn phone_preview() -> bool {
    PHONE.with(|phone| *phone.read())
}

/// Shows the application window at phone size (compact, touch), or releases
/// it. Only the application window changes; the inspector keeps its own
/// density.
pub(crate) fn set_phone_preview(on: bool) {
    PHONE.with(|phone| {
        let mut phone = *phone;
        phone.set(on);
    });
    let Some(app) = app_window() else {
        return;
    };
    force_density(app, on.then_some((SizeClass::Compact, PointerKind::Touch)));
    let (width, height) = PHONE_PREVIEW_SIZE;
    if on {
        // Remember the size the user chose so turning the preview off restores it.
        Platform::get().with_window(app, move |window| {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            PHONE_SAVED_SIZE.with(|slot| *slot.borrow_mut() = Some(size));
            let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(
                f64::from(width),
                f64::from(height),
            ));
        });
    } else if let Some(size) = PHONE_SAVED_SIZE.with(|slot| slot.borrow_mut().take()) {
        Platform::get().with_window(app, move |window| {
            let _ = window.request_inner_size(size);
        });
    }
    request_layout(app);
}
