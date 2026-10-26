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
use crate::address::Address;
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
const POPUP_OUTLINE_LAYER: i16 = 1000;
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
const RESIZE_OUTSIDE: f32 = 4.0;
const RESIZE_INSIDE: f32 = 2.0;
/// The side of the square resize handle centred on each corner: a little
/// larger than an edge band, so a corner is easy to find without reaching
/// into the header buttons.
const RESIZE_CORNER: f32 = 12.0;

use crate::shell::panel::{
    MOTION_MS, SHEET_DISMISS_DISTANCE, SHEET_MAX_HEIGHT_PERCENT, grabber, sheet_overhang,
    use_sheet_pull,
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
    title: String,
    content: Element,
    tabs: Option<Element>,
    dimensions: (f32, f32),
    address: Option<Address>,
    flags: PopupFlags,
}

/// A popup titled `title` that shows `content`. It is 520 px wide and 600 px
/// tall until [`Popup::size`] changes that. It is centred in the window, or a
/// bottom sheet on a phone.
pub fn popup(title: impl Into<String>, content: impl Into<Element>) -> Popup {
    Popup {
        title: title.into(),
        content: content.into(),
        tabs: None,
        dimensions: (Dim::PopupWidth.px(), Dim::PopupHeight.px()),
        address: None,
        flags: PopupFlags::NONE,
    }
}

impl Popup {
    /// Set the requested size in pixels. The panel is clamped to the window
    /// with a margin, and its body scrolls when it is taller than the panel.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.dimensions = (width, height);
        self
    }

    /// Tie the popup to the address it shows, so [`WindowLike::address`] can
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
    pub size: State<(f32, f32)>,
    /// The smallest size a resize can reach.
    pub min: (f32, f32),
    pub closing: State<bool>,
    pub actions: State<PopupActions>,
    pub address: Option<Address>,
    pub content: Element,
    pub tabs: Option<Element>,
    pub flags: PopupFlags,
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
                size: *size.peek(),
            }));
        })
}

/// The eight handles around the panel. They are drawn beside the panel, not
/// in it, so its clip does not cut them off: each edge band straddles the
/// edge, mostly outside it, and each corner has a larger square centred on
/// the corner, so the corners are easy to grab.
fn resize_handles(
    resize: State<Option<ResizeDrag>>,
    size: State<(f32, f32)>,
    (width, height): (f32, f32),
) -> Vec<Rect> {
    let band = RESIZE_OUTSIDE + RESIZE_INSIDE;
    let half = RESIZE_CORNER / 2.0;
    let inner_width = (width - RESIZE_CORNER).max(0.0);
    let inner_height = (height - RESIZE_CORNER).max(0.0);
    let at = |top: f32, left: f32| Position::new_absolute().top(top).left(left);
    let handle = |edges, position, w, h, cursor| {
        resize_handle(
            resize,
            size,
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

        // The frame removes itself from the manager once its exit has played.
        let closing = *entry.closing.read();
        let progress = use_presence(!closing, tokens.motion(MOTION_MS), move || {
            manager.remove(id)
        });

        // A drag on an edge follows the pointer, so it does not animate; the
        // code-driven resizes still ease to their new size.
        let mut resize = use_state(|| None::<ResizeDrag>);
        // A sheet's grabber: where its drag began. How far a sheet is pulled
        // lives on the manager, so the window can inset with it.
        let mut grab = use_state(|| None::<f32>);
        // An outside press closes the popup only when it also began outside.
        // A drag that starts on a resize handle or in the panel and is let go
        // over the scrim is not a press on the scrim.
        let mut outside_down = use_state(|| false);
        let mut pull = manager.sheet_pull;
        let mut pulling = manager.sheet_pulling;
        let motion = tokens.motion(MOTION_MS);
        let eased = use_sheet_pull(pull, motion);
        let pulled = if *pulling.read() == Some(id) {
            eased
        } else {
            0.0
        };
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

        // A drag sets the size from where the pointer began. Each dragged side
        // moves by the drag, so the popup grows by twice it and stays centred.
        // On a sheet, the grabber's drag pulls the sheet down instead.
        let on_move = move |event: Event<PointerEventData>| {
            let at = event.global_location();
            if sheet {
                if let Some(start) = *grab.peek() {
                    pull.set_if_modified((at.y as f32 - start).max(0.0));
                }
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
        // Releasing a sheet past the dismiss distance closes it. Otherwise it
        // springs back, as a sheet that is not pulled far enough should.
        let on_release = move |_| {
            resize.set_if_modified(None);
            let grabbing = grab.peek().is_some();
            if grabbing {
                grab.set(None);
                if *pull.peek() > SHEET_DISMISS_DISTANCE {
                    window.close();
                } else {
                    pull.set_if_modified(0.0);
                }
            }
        };

        let (max_width, max_height) = if sheet {
            (100.0, SHEET_MAX_HEIGHT_PERCENT)
        } else {
            (POPUP_MAX_WIDTH_PERCENT, POPUP_MAX_HEIGHT_PERCENT)
        };
        // The size the panel is actually drawn at: its own size, clamped to
        // the window by the same caps as the panel. The edge hairline and the
        // resize handles use it, so they stay on the panel's edge when the
        // window is too small for the popup's own size.
        let root = *Platform::get().current_window().root_size.read();
        let shown_size = (
            width.min(root.width * max_width / 100.0),
            height.min(root.height * max_height / 100.0),
        );
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
            .height(Size::px(if sheet {
                height + sheet_overhang()
            } else {
                height
            }))
            .max_width(Size::window_percent(max_width))
            .max_height(Size::window_percent(max_height))
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
            .maybe_child(
                (sheet && flags.contains(PopupFlags::GRABBER))
                    .then(|| {
                        grabber(&tokens, move |y| {
                            grab.set(Some(y));
                            pulling.set(Some(id));
                        })
                    }),
            )
            .child(header)
            .maybe_child(entry.tabs.clone().map(|tabs| {
                rect()
                    // The rail's ends line up with the header buttons' outer
                    // edges, which the header insets by `S2`. The rail is one
                    // control tall at the top; the rest of the height is the
                    // gap below it.
                    .tw(&tokens, &[w_full(), px(S2)])
                    .height(Size::px(tabs_height()))
                    .child(tabs)
            }))
            .child(divider(&tokens))
            .child(if flags.contains(PopupFlags::FILL) {
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), h_flex(), clip()])
                        .child(crate::focus::trap(entry.content.clone())),
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
                                .child(crate::focus::trap(entry.content.clone())),
                        ),
                )
            });

        // A sheet rests with its overhang below the edge and slides up from
        // under it; a pulled sheet follows the pointer down.
        let slide = if sheet {
            sheet_overhang() + (1.0 - progress) * (height + sheet_overhang()) + pulled
        } else {
            0.0
        };
        let dismiss_outside = !flags.contains(PopupFlags::NO_DISMISS_OUTSIDE);

        let body = if sheet {
            rect()
                .tw(&tokens, &[w_screen()])
                .offset_y(slide)
                .child(panel)
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
                .children(resize_handles(resize, entry.size, shown_size))
        };

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .layer(Layer::OverlayLevel(10))
            .tw(
                &tokens,
                &[
                    w_screen(),
                    h_screen(),
                    justify(if sheet { Align::End } else { Align::Center }),
                    items(Align::Center),
                    bg(Paint::Scrim),
                ],
            )
            // The scrim fades in with the popup.
            .opacity(progress)
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
