//! A zoomable, pannable, turnable view of anything with a size: a picture, a
//! map tile, a page, album artwork.
//!
//! [`ZoomView`] owns the *placement* of content in a box and nothing about what
//! the content is. The owner says how big the content is ([`ZoomView::new`]'s
//! `natural` size, in its own pixels, or `None` when the content fits itself)
//! and gives a function that draws it at a size; the view decides that size,
//! where the content sits, and how it is turned. A second use of the same
//! component: Quick Look's picture, the Preview app's window, a cover shown
//! large from a player, a map that has a rectangle to look at.
//!
//! ```ignore
//! use bingux_ui::zoom_view::{ZoomView, use_zoom_handle};
//!
//! let zoom = use_zoom_handle();
//! // `zoom` can be driven from anywhere in the window: zoom.rotate(true).
//! ZoomView::new(Some((4000.0, 3000.0)), |width, height| {
//!     ImageViewer::new(path.clone()).width(Size::px(width)).height(Size::px(height)).into()
//! })
//! .handle(zoom)
//! ```
//!
//! # What a person can do
//!
//! | Input | Effect |
//! | ----- | ------ |
//! | `+`, `=`, `-` | zoom a quarter of a step about the middle |
//! | `0`, `1` | fit the box, actual size (one picture pixel on one device pixel) |
//! | `R`, `Shift+R` | turn a quarter right, left |
//! | `H`, `V` | flip as it is seen, left to right, top to bottom |
//! | Shift with an arrow | pan a zoomed view (a bare arrow is the owner's) |
//! | Ctrl and wheel | zoom about the pointer |
//! | wheel, touchpad | pan a zoomed view |
//! | drag | pan; letting go while moving glides on and slows down |
//! | double click | actual size at the pointer, then back to fit |
//! | two fingers | zoom about the middle between them, and pan with it |
//!
//! Zooming, turning and flipping animate. A resize keeps what was last asked
//! for: fitted content stays fitted, and content at actual size stays at actual
//! size. The hover pill (zoom out, the percentage, zoom in, fit and actual
//! size) shows while the pointer is over the view or the content is zoomed.
//! Reduced motion makes every move instant and turns the glide off.
//!
//! The arithmetic is [`View`]; [`ZoomHandle`] is the same state reachable from
//! outside the view (a toolbar's buttons, a Save as copy that needs to know
//! how the picture was turned).

mod glide;
mod view;

use std::rc::Rc;
use std::time::Instant;

use freya::prelude::*;

pub use view::{
    Action, FitMode, MAX_PIXEL_SCALE, MAX_ZOOM, MIN_ZOOM, PAN_STEP, PinchStep, STEP, View,
    fit_scale, pinch_step, route, turned,
};

use crate::component::ComponentName;
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::motion::{ROLL_MS, use_tween};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// Draws the content at `width` by `height` logical pixels, before it is turned.
type Draw = Rc<dyn Fn(f32, f32) -> Element>;
/// Whether the view takes the keys right now.
type Gate = Rc<dyn Fn() -> bool>;

/// The state of a [`ZoomView`], reachable from outside it. Make one with
/// [`use_zoom_handle`], give it to the view with [`ZoomView::handle`], and call
/// its methods from a toolbar, a menu or a key handler. It is `Copy`.
#[derive(Clone, Copy, PartialEq)]
pub struct ZoomHandle {
    view: State<View>,
    sticky: State<Option<FitMode>>,
    viewport: State<(f32, f32)>,
    natural: State<Option<(f32, f32)>>,
    display: State<f32>,
    /// The angle of the turn on screen, in degrees, kept across full turns so
    /// the content never spins the long way round.
    spin: State<f32>,
    /// Moves are instant: a drag, a glide, a wheel pan, and the first sizing.
    instant: State<bool>,
    /// A number that changes when something other than a glide moves the view,
    /// which ends the glide.
    epoch: State<u64>,
    /// How the content came: the turn and mirror [`ZoomHandle::reset`] goes back to.
    home: State<(u8, bool)>,
}

/// A [`ZoomHandle`] for this component. Call it from `render`.
pub fn use_zoom_handle() -> ZoomHandle {
    ZoomHandle {
        view: use_state(View::default),
        sticky: use_state(|| Some(FitMode::Fit)),
        viewport: use_state(|| (0.0, 0.0)),
        natural: use_state(|| None),
        display: use_state(|| 1.0),
        spin: use_state(|| 0.0),
        instant: use_state(|| true),
        epoch: use_state(|| 0),
        home: use_state(|| (0, false)),
    }
}

/// The angle equal to `turns` quarter turns, in degrees, nearest to `near`.
fn nearest_angle(turns: u8, near: f32) -> f32 {
    let base = f32::from(turns % 4) * 90.0;
    let laps = ((near - base) / 360.0).round();
    base + laps * 360.0
}

impl ZoomHandle {
    /// How the content is placed now.
    pub fn view(&self) -> View {
        *self.view.read()
    }

    /// Quarter turns clockwise, 0 to 3.
    pub fn turns(&self) -> u8 {
        self.view().turns
    }

    /// Whether the content is mirrored (flipped an odd number of times).
    pub fn mirrored(&self) -> bool {
        self.view().mirrored
    }

    /// Whether the content has been turned or flipped from how it came.
    pub fn is_edited(&self) -> bool {
        let view = self.view();
        (view.turns, view.mirrored) != *self.home.read()
    }

    /// How the content came: its turn (quarters clockwise) and mirror, which
    /// the owner gave the view with [`ZoomView::orientation`].
    pub fn home(&self) -> (u8, bool) {
        *self.home.read()
    }

    /// "100%" for the content's scale in device pixels, when its size is known.
    pub fn percent(&self) -> Option<String> {
        let view = self.view();
        view.percent(
            *self.natural.read(),
            *self.viewport.read(),
            *self.display.read(),
        )
    }

    /// Whether the whole content is in the box.
    pub fn is_fit(&self) -> bool {
        self.view().is_fit()
    }

    /// What the person last asked for, if they have not zoomed or panned since.
    pub fn fit_mode(&self) -> Option<FitMode> {
        *self.sticky.read()
    }

    fn put(&self, next: View, sticky: Option<FitMode>, instant: bool) {
        let (mut view, mut mode, mut state, mut epoch) =
            (self.view, self.sticky, self.instant, self.epoch);
        state.set_if_modified(instant);
        mode.set_if_modified(sticky);
        view.set_if_modified(next);
        if !instant {
            *epoch.write() += 1;
        }
    }

    fn follow_spin(&self, turns: u8) {
        let mut spin = self.spin;
        let near = *spin.peek();
        spin.set_if_modified(nearest_angle(turns, near));
    }

    /// Do what a key or a button asks, about `at` (relative to the box's middle).
    pub fn apply(&self, action: Action, at: (f32, f32)) {
        let now = *self.view.peek();
        let natural = *self.natural.peek();
        let viewport = *self.viewport.peek();
        let display = *self.display.peek();
        let (next, sticky) = match action {
            Action::ZoomIn => (now.zoom_at(STEP, at, natural, viewport), None),
            Action::ZoomOut => (now.zoom_at(1.0 / STEP, at, natural, viewport), None),
            Action::Fit => (now.fitted(), Some(FitMode::Fit)),
            Action::Actual => match View::actual_zoom(natural, now.turns, viewport, display) {
                Some(_) => (
                    now.at_actual(natural, viewport, display),
                    Some(FitMode::Actual),
                ),
                None => (now, *self.sticky.peek()),
            },
            Action::Rotate(right) => (now.rotated(right, natural, viewport), Some(FitMode::Fit)),
            Action::FlipHorizontal => (now.flipped_horizontally(), *self.sticky.peek()),
            Action::FlipVertical => (now.flipped_vertically(), *self.sticky.peek()),
            Action::Pan(dx, dy) => (now.panned((dx, dy), natural, viewport), None),
        };
        match action {
            Action::Rotate(_) => {
                let mut spin = self.spin;
                let angle = *spin.peek()
                    + if matches!(action, Action::Rotate(true)) {
                        90.0
                    } else {
                        -90.0
                    };
                spin.set_if_modified(angle);
            }
            Action::FlipHorizontal | Action::FlipVertical => self.follow_spin(next.turns),
            _ => {}
        }
        self.put(next, sticky, false);
    }

    pub fn zoom_in(&self) {
        self.apply(Action::ZoomIn, (0.0, 0.0));
    }

    pub fn zoom_out(&self) {
        self.apply(Action::ZoomOut, (0.0, 0.0));
    }

    pub fn fit(&self) {
        self.apply(Action::Fit, (0.0, 0.0));
    }

    pub fn actual_size(&self) {
        self.apply(Action::Actual, (0.0, 0.0));
    }

    pub fn rotate(&self, right: bool) {
        self.apply(Action::Rotate(right), (0.0, 0.0));
    }

    pub fn flip_horizontal(&self) {
        self.apply(Action::FlipHorizontal, (0.0, 0.0));
    }

    pub fn flip_vertical(&self) {
        self.apply(Action::FlipVertical, (0.0, 0.0));
    }

    /// Put the content back as it came: fitted, with its own turn and mirror.
    pub fn reset(&self) {
        let (turns, mirrored) = *self.home.peek();
        let mut spin = self.spin;
        let near = *spin.peek();
        spin.set_if_modified(nearest_angle(turns, near));
        self.put(
            View {
                turns,
                mirrored,
                ..View::default()
            },
            Some(FitMode::Fit),
            false,
        );
    }

    /// Hold on to what the person last asked for across a resize.
    fn settle(&self) {
        let Some(mode) = *self.sticky.peek() else {
            return;
        };
        let now = *self.view.peek();
        let natural = *self.natural.peek();
        let viewport = *self.viewport.peek();
        let display = *self.display.peek();
        let next = match mode {
            FitMode::Fit => now.fitted(),
            FitMode::Actual => now.at_actual(natural, viewport, display),
        }
        .clamped(natural, viewport);
        let mut view = self.view;
        view.set_if_modified(next);
    }
}

/// The content, its size, and how it may be driven.
pub struct ZoomView {
    natural: Option<(f32, f32)>,
    draw: Draw,
    handle: Option<ZoomHandle>,
    mode: FitMode,
    orientation: (u8, bool),
    on_mode: Option<EventHandler<FitMode>>,
    controls: bool,
    gate: Option<Gate>,
    label: Option<String>,
    paint: Paint,
}

impl ZoomView {
    /// A view of content that is `natural` pixels (width, height) before any
    /// turn, or `None` when it has no size of its own and fits the box.
    /// `draw(width, height)` makes the content at that many logical pixels.
    pub fn new(natural: Option<(f32, f32)>, draw: impl Fn(f32, f32) -> Element + 'static) -> Self {
        ZoomView {
            natural,
            draw: Rc::new(draw),
            handle: None,
            mode: FitMode::Fit,
            orientation: (0, false),
            on_mode: None,
            controls: true,
            gate: None,
            label: None,
            paint: Paint::Recess,
        }
    }

    /// Reach this view's state from outside ([`use_zoom_handle`]).
    pub fn handle(mut self, handle: ZoomHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// How the content starts: fitted, or at actual size.
    pub fn start(mut self, mode: FitMode) -> Self {
        self.mode = mode;
        self
    }

    /// How the content comes: turned `turns` quarters clockwise after being
    /// mirrored left to right when `mirrored`. A photograph whose file says it
    /// is stored on its side starts upright this way, and a person's turn is
    /// counted from it ([`ZoomHandle::is_edited`]).
    pub fn orientation(mut self, turns: u8, mirrored: bool) -> Self {
        self.orientation = (turns % 4, mirrored);
        self
    }

    /// Called when the person chooses fit or actual size (not for the start
    /// mode): an owner that remembers the choice per kind of content saves it.
    pub fn on_mode(mut self, handler: impl Into<EventHandler<FitMode>>) -> Self {
        self.on_mode = Some(handler.into());
        self
    }

    /// Show the hover pill (on by default).
    pub fn controls(mut self, controls: bool) -> Self {
        self.controls = controls;
        self
    }

    /// The keys are taken only while `gate` says so: a docked pane leaves them
    /// with its list until it is clicked.
    pub fn keys_when(mut self, gate: impl Fn() -> bool + 'static) -> Self {
        self.gate = Some(Rc::new(gate));
        self
    }

    /// What a screen reader says about the content.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The paint behind the content (the recess by default).
    pub fn backdrop(mut self, paint: Paint) -> Self {
        self.paint = paint;
        self
    }
}

impl PartialEq for ZoomView {
    /// Never equal: the draw function is a closure, so the view is drawn again
    /// whenever its owner is.
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

/// The time since the process's first call, in seconds.
fn now_secs() -> f32 {
    static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    START.elapsed().as_secs_f32()
}

/// Two presses this close in time and place are a double click.
const DOUBLE_CLICK: f32 = 0.35;
const DOUBLE_CLICK_SLOP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
struct Drag {
    from: (f32, f32),
    pan: (f32, f32),
}

impl Component for ZoomView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let own = use_zoom_handle();
        let handle = self.handle.unwrap_or(own);
        let drag = use_state(|| None::<Drag>);
        let mut hovering = use_state(|| false);
        let samples = use_hook(|| Rc::new(std::cell::RefCell::new(glide::Samples::default())));
        let last_press =
            use_hook(|| Rc::new(std::cell::Cell::new((f32::NEG_INFINITY, (0.0f32, 0.0f32)))));
        let fingers = use_hook(|| Rc::new(std::cell::RefCell::new([None::<(u64, (f32, f32))>; 2])));
        let started = use_hook(|| Rc::new(std::cell::Cell::new(false)));
        let reported = use_hook(|| Rc::new(std::cell::Cell::new(false)));

        let display = *Platform::get().current_window().scale_factor.read() as f32;
        let natural = self.natural;

        // The handle learns what only this render knows.
        {
            let (mut handle_natural, mut handle_display) = (handle.natural, handle.display);
            use_side_effect_with_deps(
                &(
                    natural.map(|(w, h)| (w.to_bits(), h.to_bits())),
                    display.to_bits(),
                ),
                move |_| {
                    handle_natural.set_if_modified(natural);
                    handle_display.set_if_modified(display);
                    handle.settle();
                },
            );
        }
        // A new view starts as the owner says; the handle may be older than the view.
        {
            let (mode, (turns, mirrored)) = (self.mode, self.orientation);
            let started = started.clone();
            use_hook(move || {
                if !started.replace(true) {
                    let (mut spin, mut home) = (handle.spin, handle.home);
                    spin.set(f32::from(turns) * 90.0);
                    home.set((turns, mirrored));
                    handle.put(
                        View {
                            turns,
                            mirrored,
                            ..View::default()
                        },
                        Some(mode),
                        true,
                    );
                }
            });
        }
        // The person's choice of fit or actual size goes to the owner.
        {
            let on_mode = self.on_mode.clone();
            let reported = reported.clone();
            let sticky = *handle.sticky.read();
            use_side_effect_with_deps(&sticky, move |mode: &Option<FitMode>| {
                // The first run is the start mode, not a choice.
                if !reported.replace(true) {
                    return;
                }
                if let (Some(mode), Some(on_mode)) = (*mode, &on_mode) {
                    on_mode.call(mode);
                }
            });
        }

        let view = *handle.view.read();
        let (vw, vh) = *handle.viewport.read();
        let ms = if *handle.instant.read() {
            0
        } else {
            tokens.motion(ROLL_MS)
        };

        // Where the content is: what the view says, tweened.
        let scale_target = match natural {
            Some(natural) => view.scale(natural, (vw, vh)),
            None => view.zoom,
        };
        let scale = use_tween(scale_target, ms);
        let pan_x = use_tween(view.pan.0, ms);
        let pan_y = use_tween(view.pan.1, ms);
        let angle = use_tween(*handle.spin.read(), ms);
        let mirror = use_tween(if view.mirrored { -1.0 } else { 1.0 }, ms);

        let (width, height) = match natural {
            Some(natural) => (natural.0 * scale, natural.1 * scale),
            None => {
                let (w, h) = turned((vw * scale, vh * scale), view.turns);
                (w, h)
            }
        };
        let left = (vw - width) / 2.0 + pan_x;
        let top = (vh - height) / 2.0 + pan_y;

        let gate = self.gate.clone();
        let on_key = move |event: Event<KeyboardEventData>| {
            if gate.as_ref().is_some_and(|gate| !gate()) {
                return;
            }
            if let Some(action) = route(&event.key, event.modifiers) {
                event.stop_propagation();
                handle.apply(action, (0.0, 0.0));
            }
        };

        let on_wheel = move |event: Event<WheelEventData>| {
            let (vw, vh) = *handle.viewport.peek();
            let now = *handle.view.peek();
            let natural = *handle.natural.peek();
            let delta = event.delta_y as f32;
            if freya_winit::modifiers::live().contains(Modifiers::CONTROL) {
                event.stop_propagation();
                let at = (
                    event.element_location.x as f32 - vw / 2.0,
                    event.element_location.y as f32 - vh / 2.0,
                );
                handle.put(
                    now.zoom_at((-delta * 0.002).exp(), at, natural, (vw, vh)),
                    None,
                    false,
                );
            } else if !now.is_fit() {
                event.stop_propagation();
                let moved = now.panned((-(event.delta_x as f32), -delta), natural, (vw, vh));
                handle.put(moved, None, true);
            }
        };

        let samples_down = samples.clone();
        let samples_move = samples.clone();
        let samples_up = samples.clone();
        let last_press_down = last_press.clone();
        let reduced = tokens.reduced_motion;
        let fingers_start = fingers.clone();
        let fingers_move = fingers.clone();
        let fingers_end = fingers.clone();
        let fingers_cancel = fingers.clone();

        let on_down = move |event: Event<PointerEventData>| {
            if !event.data().is_primary() {
                return;
            }
            let mut drag = drag;
            let at = event.global_location();
            let here = (at.x as f32, at.y as f32);
            let (when, was) = last_press_down.get();
            let now = now_secs();
            if now - when < DOUBLE_CLICK
                && (here.0 - was.0).hypot(here.1 - was.1) < DOUBLE_CLICK_SLOP
            {
                // Double click: actual size at the pointer, or back to fit.
                last_press_down.set((f32::NEG_INFINITY, here));
                let (vw, vh) = *handle.viewport.peek();
                let local = event.element_location();
                let point = (local.x as f32 - vw / 2.0, local.y as f32 - vh / 2.0);
                let current = *handle.view.peek();
                if current.is_fit() {
                    let natural = *handle.natural.peek();
                    let display = *handle.display.peek();
                    if let Some(actual) =
                        View::actual_zoom(natural, current.turns, (vw, vh), display)
                    {
                        let target =
                            current.zoom_at(actual / current.zoom, point, natural, (vw, vh));
                        handle.put(target, Some(FitMode::Actual), false);
                    }
                } else {
                    handle.put(current.fitted(), Some(FitMode::Fit), false);
                }
                drag.set(None);
                return;
            }
            last_press_down.set((now, here));
            samples_down.borrow_mut().clear();
            samples_down.borrow_mut().push(now, here);
            // A glide in flight stops under the pointer.
            let mut epoch = handle.epoch;
            *epoch.write() += 1;
            drag.set(Some(Drag {
                from: here,
                pan: handle.view.peek().pan,
            }));
        };
        let on_move = move |event: Event<PointerEventData>| {
            let Some(held) = *drag.peek() else { return };
            let (vw, vh) = *handle.viewport.peek();
            let at = event.global_location();
            let here = (at.x as f32, at.y as f32);
            samples_move.borrow_mut().push(now_secs(), here);
            let now = *handle.view.peek();
            let natural = *handle.natural.peek();
            let target = View {
                pan: (
                    held.pan.0 + here.0 - held.from.0,
                    held.pan.1 + here.1 - held.from.1,
                ),
                ..now
            };
            handle.put(target.clamped(natural, (vw, vh)), None, true);
        };
        let on_up = move |_: Event<PointerEventData>| {
            let mut drag = drag;
            if drag.peek().is_none() {
                return;
            }
            drag.set(None);
            let velocity = samples_up.borrow().velocity(now_secs());
            if reduced || glide::is_resting(velocity) {
                return;
            }
            let generation = {
                let mut epoch = handle.epoch;
                *epoch.write() += 1;
                *epoch.peek()
            };
            spawn(async move {
                let ticker = RenderingTicker::get();
                let window = Platform::get().current_window();
                let mut velocity = velocity;
                let mut last = Instant::now();
                loop {
                    window.request_redraw();
                    ticker.tick().await;
                    if *handle.epoch.peek() != generation {
                        return;
                    }
                    let now = Instant::now();
                    let dt = (now - last).as_secs_f32().min(0.05);
                    last = now;
                    let (by, next) = glide::step(velocity, dt);
                    velocity = next;
                    let (vw, vh) = *handle.viewport.peek();
                    let current = *handle.view.peek();
                    let natural = *handle.natural.peek();
                    let moved = current.panned(by, natural, (vw, vh));
                    // Stop on an axis that ran into the edge.
                    if (moved.pan.0 - current.pan.0).abs() < by.0.abs() * 0.5 {
                        velocity.0 = 0.0;
                    }
                    if (moved.pan.1 - current.pan.1).abs() < by.1.abs() * 0.5 {
                        velocity.1 = 0.0;
                    }
                    handle.put(moved, None, true);
                    if glide::is_resting(velocity) {
                        return;
                    }
                }
            });
        };

        // Two fingers zoom and pan together.
        let on_touch_start = move |event: Event<TouchEventData>| {
            let mut slots = fingers_start.borrow_mut();
            let finger = (
                event.finger_id,
                (
                    event.element_location.x as f32,
                    event.element_location.y as f32,
                ),
            );
            if let Some(slot) = slots.iter_mut().flatten().find(|slot| slot.0 == finger.0) {
                *slot = finger;
            } else if let Some(empty) = slots.iter_mut().find(|slot| slot.is_none()) {
                *empty = Some(finger);
            }
        };
        let on_touch_move = move |event: Event<TouchEventData>| {
            let mut slots = fingers_move.borrow_mut();
            let at = (
                event.element_location.x as f32,
                event.element_location.y as f32,
            );
            let before = *slots;
            let Some(slot) = slots
                .iter_mut()
                .flatten()
                .find(|slot| slot.0 == event.finger_id)
            else {
                return;
            };
            slot.1 = at;
            if let ([Some(a0), Some(b0)], [Some(a1), Some(b1)]) = (before, *slots)
                && let Some(step) = pinch_step([a0.1, b0.1], [a1.1, b1.1])
            {
                drop(slots);
                // Two fingers own the touch: the one-finger drag stops.
                let mut drag = drag;
                drag.set_if_modified(None);
                let (vw, vh) = *handle.viewport.peek();
                let now = *handle.view.peek();
                let natural = *handle.natural.peek();
                let about = (step.middle.0 - vw / 2.0, step.middle.1 - vh / 2.0);
                let zoomed = now.zoom_at(step.factor, about, natural, (vw, vh));
                handle.put(zoomed.panned(step.moved, natural, (vw, vh)), None, true);
            }
        };
        let finger_lifted =
            move |slots: &Rc<std::cell::RefCell<[Option<(u64, (f32, f32))>; 2]>>,
                  event: &TouchEventData| {
                for slot in slots.borrow_mut().iter_mut() {
                    if slot.is_some_and(|slot| slot.0 == event.finger_id) {
                        *slot = None;
                    }
                }
            };

        let zoomed = !view.is_fit();
        let dragging = drag.read().is_some();
        let label = handle
            .percent()
            .unwrap_or_else(|| format!("{}%", (view.zoom * 100.0).round() as u32));
        let label_width = crate::controls::path_label::measure_text("3200%", CAPTION) + 8.0;
        let pill = (self.controls && (*hovering.read() || zoomed)).then(|| {
            let at_fit = view.is_fit() && handle.fit_mode() == Some(FitMode::Fit);
            let toggle = move |_| {
                if at_fit && handle.natural.peek().is_some() {
                    handle.actual_size();
                } else {
                    handle.fit();
                }
            };
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        gap(S1),
                        p(S1),
                        rounded(Radius::Pill),
                        bg(Paint::Floating),
                        shadow(Elevation::Floating),
                    ],
                )
                .child(
                    icon_button(Icon::ZoomOut, move |_| handle.zoom_out())
                        .ink(Ink::OnFloating)
                        .tooltip("Zoom out (-)")
                        .enabled(view.zoom > MIN_ZOOM),
                )
                .child(
                    rect()
                        .width(Size::px(label_width))
                        .tw(&tokens, &[flex_row(), justify(Align::Center)])
                        .child(
                            CAPTION
                                .in_ink(Ink::OnFloating)
                                .label(&tokens, label)
                                .max_lines(1),
                        ),
                )
                .child(
                    icon_button(Icon::ZoomIn, move |_| handle.zoom_in())
                        .ink(Ink::OnFloating)
                        .tooltip("Zoom in (+)"),
                )
                .child(
                    icon_button(Icon::ZoomFitBest, toggle)
                        .ink(Ink::OnFloating)
                        .tooltip(if at_fit && natural.is_some() {
                            "Actual size (1)"
                        } else {
                            "Fit to window (0)"
                        }),
                )
        });

        let content = (self.draw)(width.max(1.0), height.max(1.0));
        rect()
            .component("ZoomView")
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(self.label.clone().unwrap_or_default())
            .tw(&tokens, &[w_full(), h_full(), clip(), bg(self.paint)])
            .cursor(if dragging {
                CursorIcon::Grabbing
            } else if zoomed {
                CursorIcon::Grab
            } else {
                CursorIcon::default()
            })
            .on_global_key_down(on_key)
            .on_pointer_enter(move |_| hovering.set_if_modified(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_sized(move |event: Event<SizedEventData>| {
                let size = (event.area.width(), event.area.height());
                let mut viewport = handle.viewport;
                if *viewport.peek() != size {
                    viewport.set(size);
                    handle.settle();
                    let mut instant = handle.instant;
                    // The first sizing places the content without a move; later
                    // ones (a window being resized) follow it live.
                    instant.set_if_modified(true);
                    spawn(async move {
                        let ticker = RenderingTicker::get();
                        Platform::get().current_window().request_redraw();
                        ticker.tick().await;
                        ticker.tick().await;
                        let mut instant = instant;
                        instant.set_if_modified(false);
                    });
                }
            })
            .on_wheel(on_wheel)
            .on_pointer_down(on_down)
            .on_global_pointer_move(on_move)
            .on_global_pointer_up(on_up)
            .on_touch_start(on_touch_start)
            .on_touch_move(on_touch_move)
            .on_touch_end({
                let fingers = fingers_end;
                move |event: Event<TouchEventData>| finger_lifted(&fingers, &event)
            })
            .on_touch_cancel({
                let fingers = fingers_cancel;
                move |event: Event<TouchEventData>| finger_lifted(&fingers, &event)
            })
            .child(
                rect()
                    .position(Position::new_absolute().left(left).top(top))
                    // ui-lint: allow(the content's box is the size the view computed for its zoom)
                    .width(Size::px(width.max(1.0)))
                    // ui-lint: allow(the content's box is the size the view computed for its zoom)
                    .height(Size::px(height.max(1.0)))
                    .rotate(angle)
                    .scale((mirror, 1.0))
                    .child(content),
            )
            .maybe_child(pill.map(|pill| {
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().bottom(S4.px()).left(0.0))
                    .width(Size::fill())
                    .tw(&tokens, &[flex_row(), justify(Align::Center)])
                    .child(pill)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_angle_after_a_flip_is_the_nearest_equivalent_one() {
        assert_eq!(nearest_angle(1, 0.0), 90.0);
        // A turn right from 270 (three quarters) to four quarters is 360, not 0.
        assert_eq!(nearest_angle(0, 270.0), 360.0);
        // 450 is a tie between 270 and 630; 480 is nearer 630.
        assert_eq!(nearest_angle(3, 480.0), 630.0);
        assert_eq!(nearest_angle(0, -90.0), 0.0);
        assert_eq!(nearest_angle(3, -90.0), -90.0);
    }
}
