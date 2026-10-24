//! Popup content: a modal panel over the window with a header (Cancel, title,
//! primary action), an optional tab rail, and a scrolling body.
//!
//! Open one with [`Window::open`](super::Window::open). The panel has a fixed
//! size that [`Window::resize_to`](super::Window::resize_to) can animate, and
//! it stays inside the window with a margin. Cancel and Escape close it. Enter
//! runs the primary action when it is enabled. The primary action does not close
//! the popup on its own: the caller closes it once the work succeeds.
//!
//! The body component declares its primary action with [`use_popup_actions`],
//! so the header follows the body's own state.

use freya::animation::*;
use freya::prelude::*;

use super::{Window, WindowManager};
use crate::address::Address;
use crate::controls::button::{action_button, primary_button};
use crate::layout::divider;
use crate::shell::overlays::{OverlayLayers, OverlayStack, Overlays, use_presence};
use crate::shell::window::HeaderBar;
use crate::style::*;
use crate::surface::use_tokens;

/// How long the popup takes to fade and scale in or out, and the resize takes.
const MOTION_MS: u64 = 150;
/// The scale the panel starts from when it opens.
const OPEN_SCALE: f32 = 0.94;
/// The widest a popup gets, as a share of the window width, so it keeps a
/// margin on a phone.
const POPUP_MAX_WIDTH_PERCENT: f32 = 94.0;
/// The tallest a popup grows, as a share of the window height.
const POPUP_MAX_HEIGHT_PERCENT: f32 = 80.0;
/// The smallest a popup gets when an edge is dragged, or its starting size
/// when that is smaller.
const POPUP_MIN_WIDTH: f32 = 320.0;
const POPUP_MIN_HEIGHT: f32 = 240.0;
/// The width of the band along each edge and in each corner that resizes the
/// popup when dragged.
const RESIZE_BAND: f32 = 8.0;
/// The height of the tab rail under the header, when a popup has tabs.
fn tabs_height() -> f32 {
    Dim::Control.px() + S3.px()
}

/// The primary action in a popup header.
#[derive(Clone, PartialEq)]
pub struct PopupAction {
    pub label: String,
    pub enabled: bool,
    pub on_press: EventHandler<()>,
}

/// The actions a popup's header shows, set by its body with
/// [`use_popup_actions`].
#[derive(Clone, PartialEq, Default)]
pub struct PopupActions {
    pub primary: Option<PopupAction>,
}

/// A popup: a title, the content and its optional tab rail.
#[derive(Clone, PartialEq)]
pub struct Popup {
    title: String,
    content: Element,
    tabs: Option<Element>,
    dimensions: (f32, f32),
    address: Option<Address>,
}

/// A popup titled `title` that shows `content`. It is 520 px wide and 600 px
/// tall until [`Popup::size`] changes that. It is centred in the window.
pub fn popup(title: impl Into<String>, content: impl Into<Element>) -> Popup {
    Popup {
        title: title.into(),
        content: content.into(),
        tabs: None,
        dimensions: (Dim::PopupWidth.px(), Dim::PopupHeight.px()),
        address: None,
    }
}

impl Popup {
    /// Set the requested size in pixels. The panel is clamped to the window
    /// with a margin, and its body scrolls when it is taller than the panel.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.dimensions = (width, height);
        self
    }

    /// Tie the popup to the address it shows, so [`Window::address`] can
    /// answer.
    pub fn address(mut self, address: Address) -> Self {
        self.address = Some(address);
        self
    }

    /// A tab bar under the header, above the body. It stays put while the body
    /// scrolls, so use it to switch between pages of one form.
    pub fn tabs(mut self, tabs: impl Into<Element>) -> Self {
        self.tabs = Some(tabs.into());
        self
    }
}

/// Set the header's primary action for the popup this component is part of.
/// Call it from the body with the action for its current state. The header
/// updates when the actions change. Outside a popup it does nothing.
pub fn use_popup_actions(actions: PopupActions) {
    let window = super::use_window();
    let Some(entry) = window.entry() else {
        return;
    };
    let mut state = entry.actions;
    use_side_effect_with_deps(&actions, move |actions: &PopupActions| {
        state.set_if_modified(actions.clone());
    });
}

/// A popup's state as the window manager holds it. The states live in the
/// manager's scope, so they outlive the component that opened the popup.
#[derive(Clone, PartialEq)]
pub(crate) struct PopupEntry {
    pub id: u64,
    pub parent: Option<u64>,
    pub title: State<String>,
    pub size: State<(f32, f32)>,
    /// The smallest size a resize can reach.
    pub min: (f32, f32),
    pub closing: State<bool>,
    pub actions: State<PopupActions>,
    pub address: Option<Address>,
    pub content: Element,
    pub tabs: Option<Element>,
    pub overlays: Overlays,
}

impl PopupEntry {
    pub(crate) fn new(scope: ScopeId, id: u64, parent: Option<u64>, popup: Popup) -> Self {
        let (width, height) = popup.dimensions;
        Self {
            id,
            parent,
            title: State::create_in_scope(popup.title, scope),
            size: State::create_in_scope(popup.dimensions, scope),
            min: (width.min(POPUP_MIN_WIDTH), height.min(POPUP_MIN_HEIGHT)),
            closing: State::create_in_scope(false, scope),
            actions: State::create_in_scope(PopupActions::default(), scope),
            address: popup.address,
            content: popup.content,
            tabs: popup.tabs,
            overlays: Overlays::in_scope(scope),
        }
    }

    /// `size` limited to the popup's minimum and to the window less its
    /// margin. Code-driven and dragged resizes both go through this.
    pub(crate) fn clamp(&self, (width, height): (f32, f32)) -> (f32, f32) {
        let window = *Platform::get().current_window().root_size.peek();
        let max_width = (window.width * POPUP_MAX_WIDTH_PERCENT / 100.0).max(self.min.0);
        let max_height = (window.height * POPUP_MAX_HEIGHT_PERCENT / 100.0).max(self.min.1);
        (
            width.clamp(self.min.0, max_width),
            height.clamp(self.min.1, max_height),
        )
    }
}

/// A drag on a resize handle: the edges it moves, where the pointer began and
/// the size the popup had then.
#[derive(Clone, Copy, PartialEq)]
struct ResizeDrag {
    /// Which edges move: -1 for left or top, 1 for right or bottom, 0 for
    /// neither. The popup grows by twice the drag, so it stays centred.
    edges: (f32, f32),
    start: (f32, f32),
    size: (f32, f32),
}

/// One of the eight handles on the panel's edges and corners.
fn resize_handle(
    mut resize: State<Option<ResizeDrag>>,
    size: State<(f32, f32)>,
    edges: (f32, f32),
    position: Position,
    width: Size,
    height: Size,
    cursor: CursorIcon,
) -> Rect {
    rect()
        .position(position)
        .layer(Layer::Relative(2))
        .width(width)
        .height(height)
        .cursor(cursor)
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            let at = event.global_location();
            resize.set(Some(ResizeDrag {
                edges,
                start: (at.x as f32, at.y as f32),
                size: *size.peek(),
            }));
        })
}

/// The eight handles on the panel's edges and corners. Each is one band wide
/// and sits inside the edge, so the panel's clip does not cut it off.
fn resize_handles(
    resize: State<Option<ResizeDrag>>,
    size: State<(f32, f32)>,
    (width, height): (f32, f32),
) -> Vec<Rect> {
    let band = RESIZE_BAND;
    let inner_width = (width - 2.0 * band).max(0.0);
    let inner_height = (height - 2.0 * band).max(0.0);
    let at = |top: f32, left: f32| Position::new_absolute().top(top).left(left);
    vec![
        resize_handle(
            resize,
            size,
            (0.0, -1.0),
            at(0.0, band),
            Size::px(inner_width),
            Size::px(band),
            CursorIcon::NsResize,
        ),
        resize_handle(
            resize,
            size,
            (0.0, 1.0),
            at(height - band, band),
            Size::px(inner_width),
            Size::px(band),
            CursorIcon::NsResize,
        ),
        resize_handle(
            resize,
            size,
            (-1.0, 0.0),
            at(band, 0.0),
            Size::px(band),
            Size::px(inner_height),
            CursorIcon::EwResize,
        ),
        resize_handle(
            resize,
            size,
            (1.0, 0.0),
            at(band, width - band),
            Size::px(band),
            Size::px(inner_height),
            CursorIcon::EwResize,
        ),
        resize_handle(
            resize,
            size,
            (-1.0, -1.0),
            at(0.0, 0.0),
            Size::px(band),
            Size::px(band),
            CursorIcon::NwseResize,
        ),
        resize_handle(
            resize,
            size,
            (1.0, -1.0),
            at(0.0, width - band),
            Size::px(band),
            Size::px(band),
            CursorIcon::NeswResize,
        ),
        resize_handle(
            resize,
            size,
            (-1.0, 1.0),
            at(height - band, 0.0),
            Size::px(band),
            Size::px(band),
            CursorIcon::NeswResize,
        ),
        resize_handle(
            resize,
            size,
            (1.0, 1.0),
            at(height - band, width - band),
            Size::px(band),
            Size::px(band),
            CursorIcon::NwseResize,
        ),
    ]
}

/// Draws one popup: its scrim, its panel, and the `Window` it provides to its
/// content. Rendered by [`WindowHost`](super::WindowHost).
#[derive(PartialEq)]
pub(crate) struct PopupFrame {
    pub manager: WindowManager,
    pub entry: PopupEntry,
}

impl Component for PopupFrame {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.entry.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let manager = self.manager;
        let entry = self.entry.clone();
        let overlays = entry.overlays;
        let id = entry.id;
        let window = Window {
            manager,
            id: Some(id),
        };
        use_provide_context(move || window);
        use_provide_context(move || overlays);
        let a11y_id = use_a11y();

        // The frame removes itself from the manager once its exit has played.
        let closing = *entry.closing.read();
        let progress = use_presence(id, closing, tokens.motion(MOTION_MS), move |_| {
            manager.remove(id)
        });

        // A drag on an edge follows the pointer, so it does not animate; the
        // code-driven resizes still ease to their new size.
        let mut resize = use_state(|| None::<ResizeDrag>);
        let motion = tokens.motion(MOTION_MS);
        let target = *entry.size.read();
        let animated = use_animation_transition(target, move |from: (f32, f32), to: (f32, f32)| {
            let time = if resize.peek().is_some() { 0 } else { motion };
            (
                AnimNum::new(from.0, to.0)
                    .time(time)
                    .function(Function::Quad)
                    .ease(Ease::Out),
                AnimNum::new(from.1, to.1)
                    .time(time)
                    .function(Function::Quad)
                    .ease(Ease::Out),
            )
        })
        .get()
        .value();
        let (width, height) = if resize.read().is_some() {
            target
        } else {
            animated
        };

        let title = entry.title.read().clone();
        let actions = entry.actions.read().clone();
        let primary_keys = actions.primary.clone();
        let primary_element = actions.primary.map(|action| {
            let on_press = action.on_press.clone();
            Element::from(
                primary_button(action.label, move |_: ()| on_press.call(()))
                    .compact()
                    .enabled(action.enabled),
            )
        });
        let cancel = action_button("Cancel", move |_: ()| window.close()).compact();

        // The same header bar a window has: its height, inset and centred
        // title, so a popup reads as a small window and not a custom panel.
        let header = HeaderBar::new(title.clone())
            .fill(Paint::Bg)
            .leading(cancel);
        let header = match primary_element {
            Some(primary) => header.trailing(primary),
            None => header,
        };

        // A drag sets the size from where the pointer began. Each dragged side
        // moves by the drag, so the popup grows by twice it and stays centred.
        let on_drag = move |event: Event<PointerEventData>| {
            let Some(drag) = *resize.peek() else {
                return;
            };
            let at = event.global_location();
            let dx = at.x as f32 - drag.start.0;
            let dy = at.y as f32 - drag.start.1;
            window.resize_to(
                drag.size.0 + 2.0 * drag.edges.0 * dx,
                drag.size.1 + 2.0 * drag.edges.1 * dy,
            );
        };

        // The panel takes focus when the popup opens. A text field marked
        // `autofocus` is created after it, and freya keeps the last request.
        let panel = rect()
            .a11y_id(a11y_id)
            .a11y_auto_focus(true)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt(title)
            .tw(
                &tokens,
                &[
                    flex_col(),
                    rounded(Radius::Overlay),
                    // The popup is a window of its own: the page colour,
                    // with the groups raised on it as cards.
                    bg(Paint::Bg),
                    border(Line::Hairline),
                    shadow(Elevation::Floating),
                    clip(),
                ],
            )
            .width(Size::px(width))
            .height(Size::px(height))
            .max_width(Size::window_percent(POPUP_MAX_WIDTH_PERCENT))
            .max_height(Size::window_percent(POPUP_MAX_HEIGHT_PERCENT))
            .on_press(|event: Event<PressEventData>| event.stop_propagation())
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape) {
                    event.stop_propagation();
                    window.close();
                } else if event.key == Key::Named(NamedKey::Enter)
                    && let Some(action) = primary_keys.as_ref()
                    && action.enabled
                {
                    event.stop_propagation();
                    action.on_press.call(());
                }
            })
            .on_capture_global_pointer_move(on_drag)
            .on_global_pointer_up(move |_| resize.set_if_modified(None))
            .child(header)
            .maybe_child(entry.tabs.clone().map(|tabs| {
                rect()
                    .tw(&tokens, &[w_full(), px(S4), pb(S3)])
                    .height(Size::px(tabs_height()))
                    .child(tabs)
            }))
            .child(divider(&tokens))
            .child(
                ScrollView::new()
                    .drag_scrolling(false)
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(
                        rect()
                            .tw(&tokens, &[w_full(), p(S4)])
                            .child(entry.content.clone()),
                    ),
            )
            // The handles come last so they sit above the content.
            .children(resize_handles(resize, entry.size, (width, height)));

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .layer(Layer::OverlayLevel(10))
            .tw(
                &tokens,
                &[
                    w_screen(),
                    h_screen(),
                    justify(Align::Center),
                    items(Align::Center),
                    bg(Paint::Scrim),
                ],
            )
            // The scrim fades in with the panel, which also scales up from a
            // little smaller, about the panel's centre.
            .opacity(progress)
            .child(
                rect()
                    .scale(OPEN_SCALE + (1.0 - OPEN_SCALE) * progress)
                    .child(panel),
            )
            .child(OverlayLayers {
                overlays,
                stack: OverlayStack::Popup,
            })
    }
}
