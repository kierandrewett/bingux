//! [`FrameView`]: the element that shows a [`FrameSource`] and hands input to it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use freya::prelude::*;
use freya_winit::extensions::WinitPlatformExt;

use super::geometry::{Report, SizeReporter};
use super::input::{
    CaretRect, ClickCounter, CursorKind, FocusEscape, FrameFit, KeyInput, KeyRoute, KeyTracker,
    PointerAction, PointerButtons, PointerInput, TextInput, TouchInput, ViewSize, WheelInput,
    WheelScaling, relative, wheel_delta,
};
use super::present::{Look, Presenter, Pump};
use super::source::{FrameSource, WakeSignal};
use crate::component::ComponentName;
use crate::controls::StopRing;
use crate::focus::{FocusCore, press_focus, use_focus_core};
use crate::group::caption;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};

/// The callbacks an app gave the view, replaced on every render so the
/// handlers the view registered once always call the current ones.
#[derive(Clone, Default)]
struct Handlers {
    resize: Option<EventHandler<ViewSize>>,
    pointer: Option<EventHandler<PointerInput>>,
    wheel: Option<EventHandler<WheelInput>>,
    key: Option<EventHandler<KeyInput>>,
    text: Option<EventHandler<TextInput>>,
    touch: Option<EventHandler<TouchInput>>,
    focus: Option<EventHandler<bool>>,
}

/// Everything a mounted view keeps between renders and events, in cells: an
/// event changes none of it through a state, so a pointer move re-renders
/// nothing.
struct Live {
    source: Arc<dyn FrameSource>,
    presenter: RefCell<Presenter>,
    window: PlatformWindow,
    handlers: RefCell<Handlers>,
    reporter: RefCell<SizeReporter>,
    clicks: RefCell<ClickCounter>,
    keys: RefCell<KeyTracker>,
    buttons: Cell<PointerButtons>,
    /// Where the pointer was last told to the producer.
    last_at: Cell<Option<(f32, f32)>>,
    /// The view's top left in the window, and its size, in logical pixels.
    origin: Cell<(f32, f32)>,
    size: Cell<Option<(f32, f32)>>,
    focused: Cell<bool>,
    escape: Cell<FocusEscape>,
    /// The input method area last sent to the window.
    ime: Cell<Option<Area>>,
}

impl Live {
    fn pointer(
        &self,
        action: PointerAction,
        at: (f32, f32),
        button: Option<MouseButton>,
        count: u32,
    ) {
        let handler = self.handlers.borrow().pointer.clone();
        if let Some(handler) = handler {
            super::stats::input();
            handler.call(PointerInput {
                action,
                position: at,
                button,
                buttons: self.buttons.get(),
                modifiers: freya_winit::modifiers::live(),
                click_count: count,
            });
        }
    }

    /// A pointer press, from a mouse button or a finger standing in for one.
    fn down(&self, button: MouseButton, at: (f32, f32)) {
        let count = self.clicks.borrow_mut().press(Instant::now(), button, at);
        self.buttons.set(self.buttons.get().with(button));
        self.last_at.set(Some(at));
        self.pointer(PointerAction::Down, at, Some(button), count);
    }

    /// A release, if the press began in the view.
    fn up(&self, button: MouseButton, at: (f32, f32)) {
        if !self.buttons.get().contains(button) {
            return;
        }
        self.buttons.set(self.buttons.get().without(button));
        self.last_at.set(Some(at));
        let count = self.clicks.borrow().current();
        self.pointer(PointerAction::Up, at, Some(button), count);
    }

    /// A move; the same place twice is one move.
    fn moved(&self, at: (f32, f32)) {
        if self
            .last_at
            .get()
            .is_some_and(|last| (last.0 - at.0).abs() < 0.01 && (last.1 - at.1).abs() < 0.01)
        {
            return;
        }
        self.last_at.set(Some(at));
        self.pointer(PointerAction::Move, at, None, 0);
    }

    /// The view's box changed: tell the producer, at most once a frame.
    fn sized(self: &Rc<Self>, area: &Area) {
        self.origin.set((area.min_x(), area.min_y()));
        self.size.set(Some((area.width(), area.height())));
        self.offer_size();
    }

    fn offer_size(self: &Rc<Self>) {
        let Some((width, height)) = self.size.get() else {
            return;
        };
        let size = ViewSize {
            width,
            height,
            scale: *self.window.scale_factor.peek(),
        };
        let report = self.reporter.borrow_mut().offer(size, Instant::now());
        match report {
            Report::Now(size) => self.report(size),
            Report::After(delay) => {
                let live = self.clone();
                spawn(async move {
                    timer(delay).await;
                    let due = live.reporter.borrow_mut().fire(Instant::now());
                    if let Some(size) = due {
                        live.report(size);
                    }
                });
            }
            Report::Nothing => {}
        }
    }

    fn report(&self, size: ViewSize) {
        let handler = self.handlers.borrow().resize.clone();
        if let Some(handler) = handler {
            handler.call(size);
        }
    }

    /// Focus came or went: tell the producer, and let go of what it held.
    fn focus_changed(&self, focused: bool) {
        if self.focused.replace(focused) == focused {
            return;
        }
        if !focused {
            self.keys.borrow_mut().clear();
        }
        let handler = self.handlers.borrow().focus.clone();
        if let Some(handler) = handler {
            handler.call(focused);
        }
    }

    /// Where the input method's popup goes: by the producer's caret while the
    /// view has focus, and back to the whole view otherwise.
    fn place_ime(&self, focused: bool, caret: Option<CaretRect>) {
        let area = match (focused, caret) {
            (true, Some(caret)) => {
                let (x, y) = self.origin.get();
                Some(Area::new(
                    (x + caret.x, y + caret.y).into(),
                    (caret.width, caret.height).into(),
                ))
            }
            _ => None,
        };
        if self.ime.replace(area) != area {
            Platform::get().set_ime_cursor_area(self.window.id.into(), area);
        }
    }

    /// Looks for a new frame and reads the producer's cursor and caret.
    fn pump(
        &self,
        mut has_frame: State<bool>,
        mut cursor: State<CursorKind>,
        mut caret: State<Option<CaretRect>>,
    ) {
        let outcome = self.presenter.borrow_mut().pump();
        if outcome == Pump::Shown {
            super::stats::shown();
            has_frame.set_if_modified(true);
            self.window.request_redraw();
        }
        cursor.set_if_modified(self.source.cursor());
        caret.set_if_modified(self.source.caret());
    }
}

/// An element that shows frames from a [`FrameSource`] and forwards input to
/// it. See the [module documentation](super) for the contract.
#[derive(Clone)]
pub struct FrameView {
    source: Arc<dyn FrameSource>,
    label: String,
    role: AccessibilityRole,
    fit: FrameFit,
    escape: FocusEscape,
    ring: Option<StopRing>,
    radius: Radius,
    placeholder: Option<String>,
    natural: bool,
    enabled: bool,
    handlers: Handlers,
}

impl PartialEq for FrameView {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
            && self.label == other.label
            && self.role == other.role
            && self.fit == other.fit
            && self.escape == other.escape
            && self.ring == other.ring
            && self.radius == other.radius
            && self.placeholder == other.placeholder
            && self.natural == other.natural
            && self.enabled == other.enabled
            && self.handlers.resize == other.handlers.resize
            && self.handlers.pointer == other.handlers.pointer
            && self.handlers.wheel == other.handlers.wheel
            && self.handlers.key == other.handlers.key
            && self.handlers.text == other.handlers.text
            && self.handlers.touch == other.handlers.touch
            && self.handlers.focus == other.handlers.focus
    }
}

/// A view of `source`. `label` is its accessible name: what the content is
/// ("Web page", "Remote desktop", "Camera preview"), which a screen reader
/// says when focus arrives.
pub fn frame_view(source: Arc<dyn FrameSource>, label: impl Into<String>) -> FrameView {
    FrameView {
        source,
        label: label.into(),
        role: AccessibilityRole::Canvas,
        fit: FrameFit::default(),
        escape: FocusEscape::default(),
        ring: Some(StopRing::Outside),
        radius: Radius::Control,
        placeholder: None,
        natural: false,
        enabled: true,
        handlers: Handlers::default(),
    }
}

impl FrameView {
    /// The accessible role of the content. The default is `Canvas`: a surface
    /// of pixels that takes the keyboard and the pointer. Use `Video`,
    /// `Document` or `Image` when that is what the producer shows. While the
    /// producer reports a [caret](FrameSource::caret) the role is a text
    /// input, so a screen reader and the input method know.
    pub fn role(mut self, role: AccessibilityRole) -> Self {
        self.role = role;
        self
    }

    /// How a frame of another size sits in the view. Default: stretched.
    pub fn fit(mut self, fit: FrameFit) -> Self {
        self.fit = fit;
        self
    }

    /// What Tab does while the view has focus. Default: it leaves the view.
    /// See [`FocusEscape`].
    pub fn focus_escape(mut self, escape: FocusEscape) -> Self {
        self.escape = escape;
        self
    }

    /// Draws the focus ring just inside the view's box instead of outside it,
    /// for a view that fills its window's edge, where an outside ring is cut off.
    pub fn ring_inside(mut self) -> Self {
        self.ring = Some(StopRing::Inside);
        self
    }

    /// Draws no focus ring. The view is still a Tab stop and takes the
    /// keyboard. For a producer that shows its own focus (a game, a remote
    /// desktop whose guest draws its focus, a page with its own caret and
    /// outlines): a ring around all of it says nothing the picture does not.
    /// The app must be sure the producer does show it: a keyboard user who
    /// cannot see where focus is cannot use the page.
    pub fn producer_draws_focus(mut self) -> Self {
        self.ring = None;
        self
    }

    /// The corner radius of the view and its focus ring. Default
    /// [`Radius::Control`].
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    /// A line of text centred in the view until the first frame arrives
    /// ("Starting", "Connecting to the desktop").
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Sizes the view to the producer's [`size_hint`](FrameSource::size_hint)
    /// instead of the room it is given: a video at its natural size.
    pub fn natural_size(mut self) -> Self {
        self.natural = true;
        self
    }

    /// Whether the view takes focus and input. A disabled view still shows
    /// frames. Also disabled while a container above it is.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The view's size or scale changed. Called when the view first has a
    /// size, then at most once per frame while it changes, with the last size
    /// always delivered. Render at [`ViewSize::device_size`] and the frame is
    /// drawn 1:1. The view keeps showing the last frame meanwhile, at its own size
    /// in the top left corner (see [`FrameFit::Anchored`]).
    pub fn on_resize(mut self, handler: impl Into<EventHandler<ViewSize>>) -> Self {
        self.handlers.resize = Some(handler.into());
        self
    }

    /// Pointer: move, press, release, enter, leave. A touch is reported here
    /// as the left button unless [`on_touch`](FrameView::on_touch) is set.
    pub fn on_pointer(mut self, handler: impl Into<EventHandler<PointerInput>>) -> Self {
        self.handlers.pointer = Some(handler.into());
        self
    }

    /// The wheel or a touchpad scroll over the view. Setting this keeps the
    /// scroll from also reaching a scroll view the view sits in.
    pub fn on_wheel(mut self, handler: impl Into<EventHandler<WheelInput>>) -> Self {
        self.handlers.wheel = Some(handler.into());
        self
    }

    /// Keys, down and up, while the view has focus. Every key goes here
    /// except F6 (the window's region cycling) and, with the default
    /// [`FocusEscape`], Tab and Shift+Tab. Setting this keeps the keys from
    /// reaching the rest of the window.
    pub fn on_key(mut self, handler: impl Into<EventHandler<KeyInput>>) -> Self {
        self.handlers.key = Some(handler.into());
        self
    }

    /// Text from an input method (preedit and commit), and text typed with no
    /// physical key, such as an on-screen keyboard's.
    pub fn on_text(mut self, handler: impl Into<EventHandler<TextInput>>) -> Self {
        self.handlers.text = Some(handler.into());
        self
    }

    /// Touches, one event per finger and phase.
    pub fn on_touch(mut self, handler: impl Into<EventHandler<TouchInput>>) -> Self {
        self.handlers.touch = Some(handler.into());
        self
    }

    /// The view gained (`true`) or lost (`false`) keyboard focus.
    pub fn on_focus(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.handlers.focus = Some(handler.into());
        self
    }
}

impl Component for FrameView {
    /// A different source is a different view: it remounts, with its own
    /// presenter and waker.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(Arc::as_ptr(&self.source) as *const () as usize as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let core = use_focus_core();
        let usable = use_enabled(self.enabled);
        let window = use_hook(|| Platform::get().current_window());
        let has_frame = use_state(|| false);
        let cursor = use_state(|| CursorKind::Default);
        let caret = use_state(|| None::<CaretRect>);

        let live = use_hook({
            let source = self.source.clone();
            move || {
                Rc::new(Live {
                    presenter: RefCell::new(Presenter::new(source.clone())),
                    source,
                    window,
                    handlers: RefCell::new(Handlers::default()),
                    reporter: RefCell::new(SizeReporter::default()),
                    clicks: RefCell::new(ClickCounter::default()),
                    keys: RefCell::new(KeyTracker::default()),
                    buttons: Cell::new(PointerButtons::default()),
                    last_at: Cell::new(None),
                    origin: Cell::new((0.0, 0.0)),
                    size: Cell::new(None),
                    focused: Cell::new(false),
                    escape: Cell::new(FocusEscape::default()),
                    ime: Cell::new(None),
                })
            }
        });
        live.handlers.replace(self.handlers.clone());
        live.escape.set(self.escape);

        // The producer wakes this task; it takes the newest frame, asks for a
        // redraw and reads the cursor and caret. Nothing here polls.
        use_hook({
            let live = live.clone();
            move || {
                let signal = WakeSignal::new();
                live.source.set_waker(signal.waker());
                // The producer may have a frame ready already.
                signal.mark();
                spawn(async move {
                    loop {
                        signal.notified().await;
                        live.pump(has_frame, cursor, caret);
                    }
                });
            }
        });

        // A change of scale is a change of size to the producer, though the
        // box in logical pixels did not move.
        use_side_effect({
            let live = live.clone();
            move || {
                let _scale = *live.window.scale_factor.read();
                live.offer_size();
            }
        });
        // Focus: tell the producer, and put the input method by its caret.
        use_side_effect({
            let live = live.clone();
            move || {
                let focused = focus.read().is_focused();
                live.focus_changed(focused);
                live.place_ime(focused, caret());
            }
        });

        let look = Look {
            backdrop: Paint::Backdrop.color(&tokens),
            placeholder: Paint::Control.color(&tokens),
        };
        let fit = self.fit;
        let draw = {
            let live = live.clone();
            move |context: &mut FillRenderContext| {
                live.presenter.borrow().paint(
                    context.canvas,
                    (context.size.width, context.size.height),
                    fit,
                    &look,
                );
            }
        };
        let role = if caret().is_some() {
            AccessibilityRole::MultilineTextInput
        } else {
            self.role
        };
        let natural = self.natural.then(|| self.source.size_hint()).flatten();

        let mut face = painted_on_pixels(rect(), draw)
            .component("FrameView")
            .tw(&tokens, &[rounded(self.radius), clip()])
            .a11y_id(a11y_id)
            .a11y_role(role)
            .a11y_alt(self.label.clone())
            .a11y_enabled(usable)
            .a11y_focusable(usable)
            .cursor(cursor().icon())
            .maybe(cursor() == CursorKind::Hidden, |face| {
                face.cursor_visible(false)
            })
            .on_sized({
                let live = live.clone();
                move |event: Event<SizedEventData>| {
                    live.sized(&event.area);
                    live.place_ime(live.focused.get(), *caret.peek());
                }
            });
        face = match natural {
            Some((width, height)) => face.width(Size::px(width)).height(Size::px(height)),
            None => face.tw(&tokens, &[w_full(), h_full()]),
        };
        if !has_frame()
            && let Some(text) = &self.placeholder
        {
            face = face
                .tw(&tokens, &[flex_col(), center()])
                .child(caption(text.clone()));
        }
        if usable {
            face = attach_input(face, &live, &core, a11y_id);
        }
        let wrapped = match self.ring {
            Some(StopRing::Outside) => face.focusable(focus()),
            Some(StopRing::Inside) => face.focusable_inside(focus()),
            None => rect().child(face),
        };
        match natural {
            Some(_) => wrapped,
            None => wrapped.tw(&tokens, &[w_full(), h_full()]),
        }
    }
}

/// Registers the input handlers of the view on its face.
fn attach_input(face: Rect, live: &Rc<Live>, core: &FocusCore, a11y_id: AccessibilityId) -> Rect {
    face.on_pointer_enter({
        let live = live.clone();
        move |event: Event<PointerEventData>| {
            if matches!(event.data(), PointerEventData::Mouse(_)) {
                let at = element_point(&event);
                live.last_at.set(Some(at));
                live.pointer(PointerAction::Enter, at, None, 0);
            }
        }
    })
    .on_pointer_leave({
        let live = live.clone();
        move |event: Event<PointerEventData>| {
            if matches!(event.data(), PointerEventData::Mouse(_)) {
                live.pointer(PointerAction::Leave, element_point(&event), None, 0);
            }
        }
    })
    .on_pointer_move({
        let live = live.clone();
        move |event: Event<PointerEventData>| {
            if matches!(event.data(), PointerEventData::Mouse(_)) {
                live.moved(element_point(&event));
            }
        }
    })
    // A press that began in the view keeps going to it wherever the pointer
    // is, until the button is up: a drag out of the view still selects. As
    // `controls::capture` does for a drag that makes a value, the moves and the
    // release are taken in the capture phase and consumed, so nothing under the
    // pointer shows hover or is pressed meanwhile. Unlike it, every move goes
    // through and every button counts: this view forwards pointer events.
    .on_capture_global_pointer_move({
        let live = live.clone();
        move |event: Event<PointerEventData>| {
            if live.buttons.get().0 == 0 {
                return;
            }
            let touch = matches!(event.data(), PointerEventData::Touch(_));
            if touch && live.handlers.borrow().touch.is_some() {
                return;
            }
            let global = event.global_location();
            live.moved(relative((global.x, global.y), live.origin.get()));
            event.prevent_default();
        }
    })
    .on_pointer_down({
        let live = live.clone();
        let core = core.clone();
        move |event: Event<PointerEventData>| {
            let touch = matches!(event.data(), PointerEventData::Touch(_));
            event.stop_propagation();
            let button = event.data().button().unwrap_or(MouseButton::Left);
            if button == MouseButton::Left {
                press_focus(&core, a11y_id);
            }
            let touches_are_separate = touch && live.handlers.borrow().touch.is_some();
            if !touches_are_separate {
                live.down(button, element_point(&event));
            }
        }
    })
    .on_capture_global_pointer_up({
        let live = live.clone();
        move |event: Event<PointerEventData>| {
            let button = event.data().button().unwrap_or(MouseButton::Left);
            if !live.buttons.get().contains(button) {
                return;
            }
            let global = event.global_location();
            live.up(button, relative((global.x, global.y), live.origin.get()));
            event.prevent_default();
        }
    })
    .on_wheel({
        let live = live.clone();
        move |event: Event<WheelEventData>| {
            let handler = live.handlers.borrow().wheel.clone();
            let Some(handler) = handler else {
                return;
            };
            event.stop_propagation();
            super::stats::input();
            let scaling = WheelScaling {
                step: freya_winit::bingux_scroll::wheel_step(),
                touchpad: freya_winit::bingux_scroll::touchpad_factor(),
                scale: *live.window.scale_factor.peek(),
            };
            handler.call(WheelInput {
                delta: wheel_delta(event.source, event.delta_x, event.delta_y, scaling),
                position: (
                    event.element_location.x as f32,
                    event.element_location.y as f32,
                ),
                modifiers: freya_winit::modifiers::live(),
                phase: event.phase,
            });
        }
    })
    .on_touch_start(touch_handler(live, a11y_id, core))
    .on_touch_move(touch_handler(live, a11y_id, core))
    .on_touch_end(touch_handler(live, a11y_id, core))
    .on_touch_cancel(touch_handler(live, a11y_id, core))
    .on_key_down(key_handler(live, true))
    .on_key_up(key_handler(live, false))
    .on_ime_preedit({
        let live = live.clone();
        move |event: Event<ImePreeditEventData>| {
            let handler = live.handlers.borrow().text.clone();
            if let Some(handler) = handler {
                handler.call(TextInput::Preedit {
                    text: event.text.clone(),
                    cursor: event.cursor,
                });
            }
        }
    })
}

fn element_point(event: &Event<PointerEventData>) -> (f32, f32) {
    let at = event.data().element_location();
    (at.x as f32, at.y as f32)
}

fn touch_handler(
    live: &Rc<Live>,
    a11y_id: AccessibilityId,
    core: &FocusCore,
) -> impl FnMut(Event<TouchEventData>) + 'static {
    let live = live.clone();
    let core = core.clone();
    move |event: Event<TouchEventData>| {
        let handler = live.handlers.borrow().touch.clone();
        let Some(handler) = handler else {
            return;
        };
        event.stop_propagation();
        if event.phase == TouchPhase::Started {
            press_focus(&core, a11y_id);
        }
        handler.call(TouchInput {
            phase: event.phase,
            id: event.finger_id,
            position: (
                event.element_location.x as f32,
                event.element_location.y as f32,
            ),
        });
    }
}

fn key_handler(live: &Rc<Live>, pressed: bool) -> impl FnMut(Event<KeyboardEventData>) + 'static {
    let live = live.clone();
    move |event: Event<KeyboardEventData>| {
        let route = live.keys.borrow_mut().route(
            &event.key,
            event.code,
            event.modifiers,
            pressed,
            live.escape.get(),
        );
        match route {
            KeyRoute::Leave => {}
            KeyRoute::Key(input) => {
                let handler = live.handlers.borrow().key.clone();
                if let Some(handler) = handler {
                    event.stop_propagation();
                    event.prevent_default();
                    super::stats::input();
                    handler.call(input);
                }
            }
            KeyRoute::Text(text) => {
                let handler = live.handlers.borrow().text.clone();
                if let Some(handler) = handler {
                    event.stop_propagation();
                    event.prevent_default();
                    super::stats::input();
                    handler.call(text);
                }
            }
        }
    }
}
