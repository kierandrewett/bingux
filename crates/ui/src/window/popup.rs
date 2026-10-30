//! Popup content and its presentation.
//!
//! A popup is a modal over its parent window. On a wide window it is a panel
//! in the middle, with the header's Cancel and primary text buttons, and its
//! edges and corners resize. On a narrow window (a phone) it is a sheet that
//! rises from the bottom edge, with a close icon and a check icon in the
//! header, and a grabber that drags it down to dismiss.
//!
//! What the popup shows and how it is dismissed comes from [`PopupFlags`], set
//! with [`Popup::flags`]. Escape always closes a popup. An outside press closes
//! it unless [`PopupFlags::NO_DISMISS_OUTSIDE`] is set.
//!
//! The header's primary action is set by the body with [`use_popup_actions`],
//! so it follows the body's own state. The primary action does not close the
//! popup on its own: the caller closes it once the work succeeds.

use std::ops::{BitOr, BitOrAssign};

use freya::animation::*;
use freya::prelude::*;

use super::{PopupWindow, Window, WindowLike, WindowManager};
use crate::nav::Location;
use crate::controls::button::{action_button, icon_button, primary_button};
use crate::icons::Icon;
use crate::layout::divider;
use crate::motion::use_presence;
use crate::shell::overlays::{OverlayLayers, OverlayStack, Overlays};
use crate::shell::window::HeaderBar;
use crate::style::*;
use crate::surface::use_tokens;

/// The scale the panel starts from when it opens.
const OPEN_SCALE: f32 = 0.94;
/// The layer of the popup's edge hairline, relative to the popup. Higher
/// than any depth the panel's content reaches (the header bar's buttons sit
/// a few dozen levels down), so the outline is drawn over all of it.
const POPUP_OUTLINE_LAYER: i16 = crate::layers::POPUP_OUTLINE;
/// The widest a popup gets, as a share of the window width, so it keeps a
/// margin on a phone.
const POPUP_MAX_WIDTH_PERCENT: f32 = 94.0;
/// The tallest a popup grows, as a share of the window height.
const POPUP_MAX_HEIGHT_PERCENT: f32 = 80.0;
/// The smallest a popup gets when an edge is dragged, or its starting size
/// when that is smaller.
const POPUP_MIN_WIDTH: f32 = 320.0;
const POPUP_MIN_HEIGHT: f32 = 240.0;
/// How far an edge's resize band reaches outside the panel, and how far
/// inside it. Most of the band is outside, so the scrollbar and controls at
/// the panel's edge stay usable.
pub(crate) const RESIZE_OUTSIDE: f32 = 4.0;
pub(crate) const RESIZE_INSIDE: f32 = 2.0;
/// The side of the square resize handle centred on each corner: a little
/// larger than an edge band, so a corner is easy to find without reaching
/// into the header buttons.
pub(crate) const RESIZE_CORNER: f32 = 12.0;

use crate::shell::panel::{
    MOTION_MS, SHEET_DISMISS_DISTANCE, SHEET_MAX_HEIGHT_PERCENT, grabber, scrim_amount, sheet_foot,
    sheet_overhang, sheet_position, use_sheet_drag,
};
use crate::shell::sheet_gesture::{Metrics, Outcome, lift_limit};
use crate::shell::sheet_motion::{
    SHEET_MS, SHEET_SPRING_MS, openness, use_sheet_clock, use_sheet_spring,
};

/// The space between a header button and the header's top or bottom edge: the
/// header is `Dim::Header` tall and its buttons `Dim::Control`, centred.
fn header_gap() -> f32 {
    (Dim::Header.px() - Dim::Control.px()) / 2.0
}

/// The height of the tab rail under the header, when a popup has tabs: one
/// control, and below it the same gap the header keeps under its buttons, so
/// Cancel, the rail and the divider sit at an even rhythm.
fn tabs_height() -> f32 {
    Dim::Control.px() + header_gap()
}

/// What a popup shows and how it is dismissed. Combine flags with `|`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupFlags(u8);

impl PopupFlags {
    /// No extra controls: a titled panel that closes on Escape or an outside
    /// press.
    pub const NONE: Self = Self(0);
    /// A phone sheet shows a grabber that drags down to dismiss.
    pub const GRABBER: Self = Self(1 << 0);
    /// Show the body's primary action in the header.
    pub const OK: Self = Self(1 << 1);
    /// Show a Cancel button (a text button on desktop, a close icon on a
    /// phone).
    pub const CANCEL: Self = Self(1 << 2);
    /// A press outside the popup does not close it.
    pub const NO_DISMISS_OUTSIDE: Self = Self(1 << 3);
    /// Shrink the window's content into a rounded card behind the popup, for a
    /// popup that is a whole task and not a small form.
    pub const INSET_VIEWPORT: Self = Self(1 << 4);
    /// The body lays itself out: it gets the whole panel under the header,
    /// with no padding and no scrolling of its own, so it can hold a sidebar
    /// and a list that each scroll. A file chooser or an app picker is this
    /// shape; a form is not.
    pub const FILL: Self = Self(1 << 5);
    /// The usual form: Cancel, OK and, on a phone, the grabber. Use it for a
    /// popup whose body declares a primary action.
    pub const FORM: Self = Self(Self::CANCEL.0 | Self::OK.0 | Self::GRABBER.0);

    /// Whether every flag in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for PopupFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PopupFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
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

/// A popup: a title, the content, its optional tab rail and its flags.
#[derive(Clone, PartialEq)]
pub struct Popup {
    pub(crate) title: String,
    content: Element,
    tabs: Option<Element>,
    dimensions: (f32, Option<f32>),
    location: Option<Location>,
    flags: PopupFlags,
}

/// A popup titled `title` that shows `content`. It is 520 px wide until
/// [`Popup::width`] changes that, and as tall as its content: the panel
/// measures the body, so there is no empty space under the last row, and the
/// body scrolls only when it is taller than the window allows. It is centred
/// in the window, or a bottom sheet on a phone.
pub fn popup(title: impl Into<String>, content: impl Into<Element>) -> Popup {
    let title = title.into();
    Popup {
        // A panic in the body shows an error card in the panel, with Retry; the
        // header, the other popups and the window stay up.
        content: crate::resilience::error_boundary(title.clone(), content).into(),
        title,
        tabs: None,
        dimensions: (Dim::PopupWidth.px(), None),
        location: None,
        flags: PopupFlags::NONE,
    }
}

impl Popup {
    /// Set the width, at most the window's less its margin. The height
    /// follows the content.
    pub fn width(mut self, width: Dim) -> Self {
        self.dimensions.0 = width.px();
        self
    }

    /// Set a width and a height in pixels, for a body that lays itself out
    /// and has no height of its own (see [`PopupFlags::FILL`]). Both are
    /// clamped to the window with a margin, and the body scrolls when it is
    /// taller than the panel. A form does not use this: it sizes to its
    /// content.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.dimensions = (width, Some(height));
        self
    }

    /// Tie the popup to the location it shows, so [`WindowLike::location`] can
    /// answer.
    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// A tab bar under the header, above the body. It stays put while the body
    /// scrolls, so use it to switch between pages of one form.
    pub fn tabs(mut self, tabs: impl Into<Element>) -> Self {
        self.tabs = Some(tabs.into());
        self
    }

    /// Set what the popup shows and how it dismisses. See [`PopupFlags`].
    pub fn flags(mut self, flags: PopupFlags) -> Self {
        self.flags = flags;
        self
    }
}

/// Set the header's primary action for the popup this component is part of.
/// Call it from the body with the action for its current state. The header
/// updates when the actions change. Outside a popup it does nothing.
pub fn use_popup_actions(actions: PopupActions) {
    let Window::Popup(window) = super::use_window() else {
        return;
    };
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
    /// The width, and the height when code or a drag has set one. `None`
    /// follows the content.
    pub size: State<(f32, Option<f32>)>,
    /// The size the panel is drawn at, as the layout resolved it.
    pub drawn: State<(f32, f32)>,
    /// The smallest size a resize can reach.
    pub min: (f32, f32),
    pub closing: State<bool>,
    pub actions: State<PopupActions>,
    pub location: Option<Location>,
    pub content: Element,
    pub tabs: Option<Element>,
    pub flags: PopupFlags,
    pub overlays: Overlays,
}

impl PopupEntry {
    pub(crate) fn new(scope: ScopeId, id: u64, parent: Option<u64>, popup: Popup) -> Self {
        let (width, height) = popup.dimensions;
        // The overflow checks open every popup at its narrowest.
        let width = crate::stress::popup_width().map_or(width, |least| least.min(width));
        // A body that lays itself out has no height of its own to follow.
        let height = height.or_else(|| {
            popup
                .flags
                .contains(PopupFlags::FILL)
                .then(|| Dim::PopupHeight.px())
        });
        let min_height = match height {
            Some(height) => height.min(POPUP_MIN_HEIGHT),
            // A content-sized popup can be dragged down to a header and one row.
            None => Dim::Header.px() + Dim::Row.px(),
        };
        Self {
            id,
            parent,
            title: State::create_in_scope(popup.title, scope),
            size: State::create_in_scope((width, height), scope),
            drawn: State::create_in_scope((width, 0.0), scope),
            min: (width.min(POPUP_MIN_WIDTH), min_height),
            closing: State::create_in_scope(false, scope),
            actions: State::create_in_scope(PopupActions::default(), scope),
            location: popup.location,
            content: popup.content,
            tabs: popup.tabs,
            flags: popup.flags,
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

/// The panel's height, from what the popup asked for and what its body
/// measured. Pure, so the rule can be tested without a window.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PanelHeight {
    /// The height of the panel, a sheet's overhang included.
    panel: f32,
    /// The part of it a person sees: the panel less a sheet's overhang.
    visible: f32,
}

/// `requested` is the visible height that code or a drag asked for; `None`
/// follows the content. `content` is the measured height of the scrolling
/// body (padding and a sheet's foot included), `None` until it is laid out.
/// `chrome` is everything above the body, `limit` the tallest the panel may
/// be, and `overhang` how far a sheet hangs below the window.
///
/// A popup is as tall as its content and no taller: whatever does not fit
/// under `limit` scrolls, and nothing is left empty under the last row.
fn panel_height(
    requested: Option<f32>,
    content: Option<f32>,
    chrome: f32,
    limit: f32,
    overhang: f32,
) -> PanelHeight {
    let wanted = match requested {
        Some(height) => height + overhang,
        // Until the body is measured it may take all there is, so it is laid
        // out unconstrained and its real height can be read.
        None => chrome + content.unwrap_or(limit),
    };
    let panel = wanted.min(limit).max(chrome);
    PanelHeight {
        panel,
        visible: panel - overhang,
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
    size: (f32, f32),
    edges: (f32, f32),
    position: Position,
    width: Size,
    height: Size,
    cursor: CursorIcon,
) -> Rect {
    rect()
        .position(position)
        // Above the outline: the inner part of each band overlaps the panel,
        // whose deep nodes would otherwise outrank the handle and take the press.
        .layer(Layer::Relative(POPUP_OUTLINE_LAYER + 1))
        .width(width)
        .height(height)
        .cursor(cursor)
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            let at = event.global_location();
            resize.set(Some(ResizeDrag {
                edges,
                start: (at.x as f32, at.y as f32),
                size,
            }));
        })
}

/// The eight handles around the panel. They are drawn beside the panel, not
/// in it, so its clip does not cut them off: each edge band straddles the
/// edge, mostly outside it, and each corner has a larger square centred on
/// the corner, so the corners are easy to grab.
fn resize_handles(resize: State<Option<ResizeDrag>>, (width, height): (f32, f32)) -> Vec<Rect> {
    let band = RESIZE_OUTSIDE + RESIZE_INSIDE;
    let half = RESIZE_CORNER / 2.0;
    let inner_width = (width - RESIZE_CORNER).max(0.0);
    let inner_height = (height - RESIZE_CORNER).max(0.0);
    let at = |top: f32, left: f32| Position::new_absolute().top(top).left(left);
    let handle = |edges, position, w, h, cursor| {
        resize_handle(
            resize,
            (width, height),
            edges,
            position,
            Size::px(w),
            Size::px(h),
            cursor,
        )
    };
    vec![
        // Edges, between the corner squares.
        handle(
            (0.0, -1.0),
            at(-RESIZE_OUTSIDE, half),
            inner_width,
            band,
            CursorIcon::NsResize,
        ),
        handle(
            (0.0, 1.0),
            at(height - RESIZE_INSIDE, half),
            inner_width,
            band,
            CursorIcon::NsResize,
        ),
        handle(
            (-1.0, 0.0),
            at(half, -RESIZE_OUTSIDE),
            band,
            inner_height,
            CursorIcon::EwResize,
        ),
        handle(
            (1.0, 0.0),
            at(half, width - RESIZE_INSIDE),
            band,
            inner_height,
            CursorIcon::EwResize,
        ),
        // Corners, centred on each corner of the panel.
        handle(
            (-1.0, -1.0),
            at(-half, -half),
            RESIZE_CORNER,
            RESIZE_CORNER,
            CursorIcon::NwseResize,
        ),
        handle(
            (1.0, -1.0),
            at(-half, width - half),
            RESIZE_CORNER,
            RESIZE_CORNER,
            CursorIcon::NeswResize,
        ),
        handle(
            (-1.0, 1.0),
            at(height - half, -half),
            RESIZE_CORNER,
            RESIZE_CORNER,
            CursorIcon::NeswResize,
        ),
        handle(
            (1.0, 1.0),
            at(height - half, width - half),
            RESIZE_CORNER,
            RESIZE_CORNER,
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
    /// The window is narrow, so the popup is a sheet from the bottom edge.
    pub sheet: bool,
}

impl Component for PopupFrame {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.entry.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let manager = self.manager;
        let entry = self.entry.clone();
        let flags = entry.flags;
        let sheet = self.sheet;
        let overlays = entry.overlays;
        let id = entry.id;
        let window = PopupWindow { manager, id };
        use_provide_context(move || Window::Popup(window));
        use_provide_context(move || overlays);
        let a11y_id = use_a11y();

        let closing = *entry.closing.read();

        // A drag on an edge follows the pointer, so it does not animate; the
        // code-driven resizes still ease to their new size.
        let mut resize = use_state(|| None::<ResizeDrag>);
        // A sheet's drag. How far a sheet is pulled, and the release that let
        // it go, live on the manager, so the window can inset with it.
        let drag = use_sheet_drag(manager.sheet_pull, manager.sheet_release);
        // An outside press closes the popup only when it also began outside.
        // A drag that starts on a resize handle or in the panel and is let go
        // over the scrim is not a press on the scrim.
        let mut outside_down = use_state(|| false);
        let mut pulling = manager.sheet_pulling;
        let mut dragging = manager.sheet_dragging;
        let motion = tokens.motion(MOTION_MS);
        let eased = use_sheet_spring(
            manager.sheet_pull,
            manager.sheet_release,
            drag.dragging(),
            tokens.motion(SHEET_SPRING_MS),
            lift_limit(sheet_overhang()),
        );
        let pulled = if *pulling.read() == Some(id) {
            eased
        } else {
            0.0
        };
        let drag = drag.showing(pulled);
        let (requested_width, requested_height) = *entry.size.read();
        // The width and a height that was asked for ease to their new values.
        // The content's own height does not animate: the panel is simply as
        // tall as what it holds.
        let target = (requested_width, requested_height.unwrap_or(0.0));
        let animated = use_animation_transition(target, move |from: (f32, f32), to: (f32, f32)| {
            let time = if resize.peek().is_some() { 0 } else { motion };
            // The first height asked for has nothing to ease from.
            let from_height = if from.1 <= 0.0 { to.1 } else { from.1 };
            (
                AnimNum::new(from.0, to.0)
                    .time(time)
                    .function(Function::Quad)
                    .ease(Ease::Out),
                AnimNum::new(from_height, to.1)
                    .time(time)
                    .function(Function::Quad)
                    .ease(Ease::Out),
            )
        })
        .get()
        .value();
        let (width, eased_height) = if resize.read().is_some() {
            target
        } else {
            animated
        };
        let requested = requested_height.map(|_| eased_height);
        // What the layout measured: the scrolling body's content, and the
        // header, tabs and rule above it.
        let mut content_height = use_state(|| None::<f32>);
        let mut chrome_height = use_state(|| Dim::Header.px() + Dim::Hairline.px());
        let mut drawn = entry.drawn;

        let title = entry.title.read().clone();
        let actions = entry.actions.read().clone();
        let primary_keys = actions.primary.clone();
        // A sheet's check is an icon button, so it stays on the bar even when
        // the action is not ready: it dims, as the desktop's text button does.
        let primary_element = actions
            .primary
            .filter(|_| flags.contains(PopupFlags::OK))
            .map(|action| {
                if sheet {
                    let on_press = action.on_press.clone();
                    Element::from(
                        icon_button(Icon::Check, move |_: ()| on_press.call(()))
                            .tooltip(&action.label)
                            .enabled(action.enabled),
                    )
                } else {
                    let on_press = action.on_press.clone();
                    Element::from(
                        primary_button(action.label, move |_: ()| on_press.call(()))
                            .compact()
                            .enabled(action.enabled),
                    )
                }
            });
        let cancel = flags.contains(PopupFlags::CANCEL).then(|| {
            if sheet {
                Element::from(
                    icon_button(Icon::WindowClose, move |_: ()| window.close()).tooltip("Close"),
                )
            } else {
                Element::from(action_button("Cancel", move |_: ()| window.close()).compact())
            }
        });

        // The same header bar a window has: its height, inset and centred
        // title, so a popup reads as a small window and not a custom panel.
        let mut header = HeaderBar::new(title.clone()).fill(Paint::Bg);
        if let Some(cancel) = cancel {
            header = header.leading(cancel);
        }
        if let Some(primary) = primary_element {
            header = header.trailing(primary);
        }

        let (max_width, max_height) = if sheet {
            (100.0, SHEET_MAX_HEIGHT_PERCENT)
        } else {
            (POPUP_MAX_WIDTH_PERCENT, POPUP_MAX_HEIGHT_PERCENT)
        };
        let root = *Platform::get().current_window().root_size.read();
        // The panel's height: what a drag or the code asked for, else its
        // content's, up to the share of the window it may take.
        let height = panel_height(
            requested,
            *content_height.read(),
            *chrome_height.read(),
            root.height * max_height / 100.0,
            if sheet { sheet_overhang() } else { 0.0 },
        );
        // The size the panel is actually drawn at, as the panel itself is
        // capped by the window. The edge hairline and the resize handles use
        // it, so they stay on the panel's edge when the window is too small
        // for the popup's own size.
        let shown_size = (width.min(root.width * max_width / 100.0), height.visible);
        let metrics = Metrics::new(shown_size.1, sheet_overhang(), SHEET_DISMISS_DISTANCE);

        // A drag sets the size from where the pointer began. Each dragged side
        // moves by the drag, so the popup grows by twice it and stays centred.
        // On a sheet, the grabber's drag pulls the sheet down instead.
        let on_move = move |event: Event<PointerEventData>| {
            let at = event.global_location();
            if sheet {
                drag.moved(at.y as f32, true, &metrics);
                return;
            }
            let Some(drag) = *resize.peek() else {
                return;
            };
            let dx = at.x as f32 - drag.start.0;
            let dy = at.y as f32 - drag.start.1;
            window.resize_to(
                drag.size.0 + 2.0 * drag.edges.0 * dx,
                drag.size.1 + 2.0 * drag.edges.1 * dy,
            );
        };
        // Releasing a sheet past the dismiss distance, or with a fling, closes
        // it. Otherwise it springs back, as a sheet that is not pulled far
        // enough should. A cancelled touch springs it back.
        let on_release = move |event: Event<PointerEventData>| {
            resize.set_if_modified(None);
            if !sheet {
                return;
            }
            let cancelled = matches!(
                event.data(),
                PointerEventData::Touch(touch) if touch.phase == TouchPhase::Cancelled
            );
            let outcome = if cancelled {
                drag.cancelled()
            } else {
                drag.released(&metrics)
            };
            dragging.set_if_modified(false);
            if outcome == Outcome::Dismiss {
                window.close();
            }
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
                    shadow(Elevation::Floating),
                    clip(),
                ],
            )
            .width(if sheet { Size::fill() } else { Size::px(width) })
            .height(Size::px(height.panel))
            .max_width(Size::window_percent(max_width))
            .max_height(Size::window_percent(max_height))
            .on_sized(move |event: Event<SizedEventData>| {
                drawn.set_if_modified((event.area.width(), event.area.height()));
            })
            .on_press(|event: Event<PressEventData>| event.stop_propagation())
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
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
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(on_release)
            .child(
                // Everything above the body: the panel measures it, so the
                // body gets the height that is left.
                rect()
                    .tw(&tokens, &[w_full(), flex_col()])
                    .on_sized(move |event: Event<SizedEventData>| {
                        chrome_height.set_if_modified(event.area.height());
                    })
                    .maybe_child((sheet && flags.contains(PopupFlags::GRABBER)).then(|| {
                        grabber(&tokens, move |y| {
                            drag.grab(y);
                            pulling.set(Some(id));
                            dragging.set_if_modified(true);
                        })
                    }))
                    .child(header)
                    .maybe_child(entry.tabs.clone().map(|tabs| {
                        rect()
                            // The rail's ends line up with the header buttons'
                            // outer edges, which the header insets by `S2`. The
                            // rail is one control tall at the top; the rest of
                            // the height is the gap below it.
                            .tw(&tokens, &[w_full(), px(S2)])
                            .height(Size::px(tabs_height()))
                            .child(tabs)
                    }))
                    .child(divider(&tokens)),
            )
            .child(if flags.contains(PopupFlags::FILL) {
                // A sheet's card hangs `sheet_overhang()` below the window,
                // so its content ends that far, plus the bottom inset, above
                // the card's own bottom edge.
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                        .child(
                            rect()
                                .tw(&tokens, &[w_full(), h_flex(), clip()])
                                .child(crate::focus::trap(entry.content.clone())),
                        )
                        .maybe_child(
                            sheet.then(|| rect().height(Size::px(sheet_foot()))),
                        ),
                )
            } else {
                Element::from(
                    ScrollView::new()
                        .drag_scrolling(false)
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .child(
                            rect()
                                .tw(&tokens, &[w_full(), p(S4)])
                                // The body's natural height, which the panel
                                // takes up to its limit. The trap is inline: one that
                                // fills its parent makes an auto-height body as tall as
                                // the room it is measured in, and the popup never shrinks
                                // to its content (SP-07).
                                .on_sized(move |event: Event<SizedEventData>| {
                                    content_height
                                        .set_if_modified(Some(event.area.height()));
                                })
                                .child(crate::focus::trap_inline(entry.content.clone()))
                                .maybe_child(sheet.then(|| {
                                    rect().height(Size::px(sheet_foot()))
                                })),
                        ),
                )
            });
        // The frame removes itself from the manager once its exit has played.
        let remove = move || {
            manager.remove(id);
            // The pull is the window's, so it outlives this popup: the next
            // sheet must open at rest.
            let mut pull = manager.sheet_pull;
            pull.set_if_modified(0.0);
        };
        let presence = use_presence(!closing, motion, move || {
            if !sheet {
                remove();
            }
        });
        // A sheet is moved by its own clock, and does not start until its body
        // is measured: it is laid out at its final size and only translated, so
        // until then there is nothing to translate.
        let measured = flags.contains(PopupFlags::FILL)
            || requested_height.is_some()
            || content_height.read().is_some();
        let clock = use_sheet_clock(
            sheet && !closing && measured,
            !closing,
            tokens.motion(SHEET_MS),
            move || {
                if sheet {
                    remove();
                }
            },
        );
        let progress = if sheet { openness(clock) } else { presence };
        // A sheet rests with its overhang below the edge and slides up from
        // under it; a pulled sheet follows the pointer down, and one let go
        // past the dismiss distance leaves at the speed it was let go at.
        let dismissed = closing && *pulling.peek() == Some(id) && *manager.sheet_pull.peek() > 0.0;
        let slide = if sheet {
            sheet_overhang()
                + sheet_position(
                    clock,
                    pulled,
                    dismissed.then(|| manager.sheet_release.peek().velocity),
                    metrics.travel,
                )
        } else {
            0.0
        };
        let dismiss_outside = !flags.contains(PopupFlags::NO_DISMISS_OUTSIDE);

        let body = if sheet {
            rect()
                .tw(&tokens, &[w_screen()])
                .offset_y(slide)
                .child(panel)
                // The sheet's edge, the same hairline the desktop popup has all round: without it a
                // sheet's top edge was the panel fill against the dimmed page, 1.07:1
                // (scripts/pixel-check.py, BR-03). The bottom hangs below the window.
                .child(
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .layer(Layer::Relative(POPUP_OUTLINE_LAYER))
                        .width(Size::fill())
                        .height(Size::px(height.panel))
                        .tw(&tokens, &[rounded(Radius::Overlay), border(Line::Hairline)]),
                )
        } else {
            rect()
                // The panel scales up from a little smaller, about its centre.
                .scale(OPEN_SCALE + (1.0 - OPEN_SCALE) * progress)
                .child(panel)
                // The window's edge, drawn over everything the panel holds, so
                // the hairline runs round the whole popup, header included.
                // Freya adds each element's depth to its layer, so the header
                // bar's nodes deep in the panel outrank a shallow sibling: the
                // outline takes a layer above any depth the panel reaches. It
                // has no fill, so presses reach the panel through it.
                .child(
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .layer(Layer::Relative(POPUP_OUTLINE_LAYER))
                        .width(Size::px(shown_size.0))
                        .height(Size::px(shown_size.1))
                        .tw(&tokens, &[rounded(Radius::Overlay), border(Line::Hairline)]),
                )
                // The handles sit beside the panel, above the scrim, so
                // they reach outside the panel's clipped edge.
                .children(resize_handles(resize, shown_size))
        };

        let root = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .layer(crate::layers::level(crate::layers::LEVEL_POPUP))
            .tw(
                &tokens,
                &[
                    w_screen(),
                    h_screen(),
                    justify(if sheet { Align::End } else { Align::Center }),
                    items(Align::Center),
                ],
            );
        // The scrim fades in with the popup. A sheet's scrim is its own box
        // under the sheet, so the sheet itself never fades: it follows the
        // sheet's position, and fades with exactly its timing.
        let root = if sheet {
            root.child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .tw(&tokens, &[w_screen(), h_screen(), bg(Paint::Scrim)])
                    .opacity(scrim_amount(slide - sheet_overhang(), metrics.travel)),
            )
        } else {
            root.tw(&tokens, &[bg(Paint::Scrim)]).opacity(progress)
        };
        root
            .on_pointer_down(move |_: Event<PointerEventData>| outside_down.set(true))
            .on_press(move |_: Event<PressEventData>| {
                let began_outside = *outside_down.peek();
                outside_down.set(false);
                if dismiss_outside && began_outside {
                    window.close();
                }
            })
            .child(body)
            .child(OverlayLayers {
                overlays,
                stack: OverlayStack::Popup,
                page: None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME: f32 = 49.0;
    const LIMIT: f32 = 480.0;

    #[test]
    fn a_popup_is_as_tall_as_its_content_and_no_taller() {
        let height = panel_height(None, Some(120.0), CHROME, LIMIT, 0.0);
        assert_eq!(height.panel, CHROME + 120.0);
        assert_eq!(height.visible, CHROME + 120.0);
    }

    #[test]
    fn content_over_the_limit_is_capped_and_scrolls() {
        let height = panel_height(None, Some(900.0), CHROME, LIMIT, 0.0);
        assert_eq!(height.panel, LIMIT);
    }

    #[test]
    fn an_unmeasured_body_may_take_the_limit_so_it_can_be_measured() {
        assert_eq!(panel_height(None, None, CHROME, LIMIT, 0.0).panel, LIMIT);
    }

    #[test]
    fn a_height_asked_for_wins_over_the_content() {
        let height = panel_height(Some(300.0), Some(120.0), CHROME, LIMIT, 0.0);
        assert_eq!(height.panel, 300.0);
        assert_eq!(panel_height(Some(700.0), Some(120.0), CHROME, LIMIT, 0.0).panel, LIMIT);
    }

    #[test]
    fn a_sheet_hangs_its_overhang_below_the_window() {
        // The body's measured height holds the sheet's foot, overhang included,
        // so the panel is chrome and body; the visible part is that less the
        // overhang.
        let height = panel_height(None, Some(200.0), CHROME, LIMIT, 20.0);
        assert_eq!(height.panel, CHROME + 200.0);
        assert_eq!(height.visible, CHROME + 180.0);
        // A height asked for is the visible one: the overhang comes on top.
        let asked = panel_height(Some(300.0), None, CHROME, LIMIT, 20.0);
        assert_eq!(asked.panel, 320.0);
        assert_eq!(asked.visible, 300.0);
    }

    #[test]
    fn a_tiny_limit_never_squeezes_the_panel_below_its_chrome() {
        assert_eq!(panel_height(None, Some(50.0), CHROME, 10.0, 0.0).panel, CHROME);
    }
}
