//! The one floating surface. Menus, dropdown lists, popovers, tooltips,
//! action sheets and popups all place themselves through the same geometry
//! here: a [`PanelTrigger`] says what opened the panel, and [`place`] puts
//! a panel of a given size beside that trigger, flipping and clamping it
//! inside the window.

use freya::animation::*;
use freya::prelude::*;

use super::menu_query::Pill;
use super::sheet_gesture::{Claim, Metrics, Outcome, Seconds, SheetGesture};
use crate::component::ComponentName;
use crate::nav::ModalKind;
use crate::nav::router::use_modal;
use crate::motion::{PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::focus::{FocusCore, claim_press, release_press, use_focus_core};
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink};

/// Space kept between a floating panel and the window edges.
pub(crate) const PANEL_EDGE: f32 = S2.fixed_px();
/// The gap between a panel and the anchor it opens against.
pub(crate) const PANEL_GAP: f32 = S1.fixed_px();

/// How long a sheet takes to ease its pull back to rest, and popups take to fade.
pub(crate) const MOTION_MS: u64 = 150;
/// How long a dialog takes to fade and scale in or out.
const DIALOG_MOTION_MS: u64 = 120;

/// The grabber row at the top of a sheet, and the pill drawn in it. Popups
/// and dropdown sheets share it.
pub(crate) const GRABBER_ROW: f32 = 20.0;
pub(crate) const GRABBER_WIDTH: f32 = 36.0;
pub(crate) const GRABBER_HEIGHT: f32 = 4.0;
/// The tallest a phone sheet grows, as a share of the window height.
pub(crate) const SHEET_MAX_HEIGHT_PERCENT: f32 = 92.0;
/// The tallest a choice sheet grows, so the dimmed page shows above it.
pub(crate) const CHOICE_SHEET_MAX_HEIGHT_PERCENT: f32 = 70.0;
/// How tall the fade at the bottom of a sheet's rows is.
pub(crate) const SHEET_FADE: f32 = 28.0;
/// How far a sheet is dragged down before it dismisses on release, or half
/// its height when that is less (see [`Metrics::new`]).
pub(crate) const SHEET_DISMISS_DISTANCE: f32 = 96.0;

/// How far a sheet extends past the bottom edge, so its bottom corners sit
/// off screen and only its top corners show.
pub(crate) fn sheet_overhang() -> f32 {
    Radius::Overlay.px()
}

/// The padding below a popup sheet's content: the card's overhang under the
/// window edge and the window's bottom inset, so the content ends clear of
/// both.
pub(crate) fn sheet_foot() -> f32 {
    sheet_overhang() + Dim::SafeBottom.px()
}

/// The padding that ends a dropdown sheet's scroll content: the sheet's own
/// bottom space and the window's bottom inset. A list ends with it, so its
/// last row can scroll clear of the window edge and is never cut by it.
pub(crate) fn sheet_bottom() -> f32 {
    SHEET_BOTTOM.px() + Dim::SafeBottom.px()
}

/// The grabber row at the top of a sheet: a short pill. A press calls
/// `on_press` with the pointer's y, so the caller can start a drag. A touch
/// press does not stop propagating: Freya drops the global release of a
/// touch whose press was stopped, and the sheet needs it to end the drag.
pub(crate) fn grabber(tokens: &Tokens, on_press: impl FnMut(f32) + 'static) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(GRABBER_ROW))
        .tw(
            tokens,
            &[
                flex_col(),
                items(crate::style::Align::Center),
                justify(crate::style::Align::Center),
            ],
        )
        .on_pointer_down(grab_handler(on_press))
        .child(
            rect()
                .width(Size::px(GRABBER_WIDTH))
                .height(Size::px(GRABBER_HEIGHT))
                .tw(tokens, &[bg(Paint::Control), rounded(Radius::Pill)]),
        )
}

/// The pointer-down handler that starts a sheet drag: calls `on_press` with
/// the pointer's y. Used by the grabber and by a sheet's header strip.
pub(crate) fn grab_handler(
    mut on_press: impl FnMut(f32) + 'static,
) -> impl FnMut(Event<PointerEventData>) + 'static {
    move |event: Event<PointerEventData>| {
        if !matches!(event.data(), PointerEventData::Touch(_)) {
            event.stop_propagation();
        }
        on_press(event.global_location().y as f32);
    }
}

/// Seconds on the sheet gesture's clock: monotonic, from the first read.
fn gesture_clock() -> Seconds {
    thread_local! {
        static EPOCH: std::time::Instant = std::time::Instant::now();
    }
    EPOCH.with(|epoch| epoch.elapsed().as_secs_f64())
}

/// The sheet's pull, eased. It follows the pointer exactly while `dragging`,
/// and eases to its new value otherwise: the spring back to rest uses the same
/// curve and time as the sheet opening. A dismiss keeps the pull where it was.
pub(crate) fn use_sheet_pull(pull: State<f32>, dragging: bool, motion: u64) -> f32 {
    use_animation_transition(*pull.read(), move |from: f32, to: f32| {
        let time = if dragging { 0 } else { motion };
        AnimNum::new(from, to)
            .time(time)
            .function(Function::Quad)
            .ease(Ease::Out)
    })
    .get()
    .value()
}

/// One sheet's drag: the pure gesture ([`SheetGesture`]) and the pull target
/// the sheet is eased to. Both are state, so a change redraws the sheet.
#[derive(Clone, Copy)]
pub(crate) struct SheetDrag {
    gesture: State<SheetGesture>,
    pull: State<f32>,
}

/// The drag state of the calling sheet. `pull` is where the sheet's pull
/// target lives: a popup's lives on the window, so the window can inset with it.
pub(crate) fn use_sheet_drag(pull: State<f32>) -> SheetDrag {
    SheetDrag {
        gesture: use_state(SheetGesture::default),
        pull,
    }
}

impl SheetDrag {
    /// The pointer went down on the grabber at `y`.
    pub(crate) fn grab(mut self, y: f32) {
        self.gesture.write().grab(y, gesture_clock());
    }

    /// A finger went down in the list at `y`.
    pub(crate) fn press_content(mut self, y: f32) {
        self.gesture.write().press_content(y, gesture_clock());
    }

    /// Whether the sheet follows the pointer. Reading it subscribes.
    pub(crate) fn dragging(self) -> bool {
        self.gesture.read().is_dragging()
    }

    /// The pointer moved to `y`. Returns who moves for it.
    pub(crate) fn moved(mut self, y: f32, at_top: bool, metrics: &Metrics) -> Claim {
        if !self.gesture.peek().is_active() {
            return Claim::Content;
        }
        let claim = self
            .gesture
            .write()
            .move_to(y, gesture_clock(), at_top, metrics);
        let pull = self.gesture.peek().pull();
        self.pull.set_if_modified(pull);
        claim
    }

    /// The pointer went up. A sheet that is let go springs back; one that
    /// is to be dismissed keeps its pull while it leaves.
    pub(crate) fn released(mut self, metrics: &Metrics) -> Outcome {
        if !self.gesture.peek().is_active() {
            return Outcome::None;
        }
        let outcome = self.gesture.write().release(gesture_clock(), metrics);
        self.settle(outcome);
        outcome
    }

    /// The pointer was cancelled: a dragged sheet springs back.
    pub(crate) fn cancelled(mut self) -> Outcome {
        if !self.gesture.peek().is_active() {
            return Outcome::None;
        }
        let outcome = self.gesture.write().cancel();
        self.settle(outcome);
        outcome
    }

    fn settle(mut self, outcome: Outcome) {
        if outcome == Outcome::Rest {
            self.gesture.write().settle_to(0.0);
            self.pull.set_if_modified(0.0);
        }
    }

    /// Forgets the pull, for a sheet that has left.
    pub(crate) fn reset(mut self) {
        self.gesture.write().reset();
        self.pull.set_if_modified(0.0);
    }
}

/// How far a sheet of `travel` px has left the window, from its presence
/// `progress` (1 when open) and `pull` (how far it is dragged down). Rest is
/// zero and gone is `travel`; a sheet dismissed from a pull continues from
/// where it is, so it spends no travel off screen.
pub(crate) fn sheet_offset(progress: f32, pull: f32, travel: f32) -> f32 {
    pull + (1.0 - progress) * (travel - pull).max(0.0)
}

/// How much of a sheet's scrim shows: all of it at rest, none once the
/// sheet is gone, following the sheet's drag.
pub(crate) fn scrim_amount(offset: f32, travel: f32) -> f32 {
    if travel <= 0.0 {
        return 1.0;
    }
    (1.0 - offset / travel).clamp(0.0, 1.0)
}

/// A rectangle in window coordinates: its top-left corner and its size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct PanelRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl PanelRect {
    /// A zero-sized rectangle at `point`.
    pub(crate) fn at_point(point: (f32, f32)) -> Self {
        Self {
            x: point.0,
            y: point.1,
            width: 0.0,
            height: 0.0,
        }
    }

    pub(crate) fn right(self) -> f32 {
        self.x + self.width
    }

    pub(crate) fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub(crate) fn contains(self, point: (f32, f32)) -> bool {
        point.0 >= self.x
            && point.0 <= self.right()
            && point.1 >= self.y
            && point.1 <= self.bottom()
    }

    /// The rectangle an area of the layout covers.
    pub(crate) fn from_area(area: &Area) -> Self {
        Self {
            x: area.min_x(),
            y: area.min_y(),
            width: area.max_x() - area.min_x(),
            height: area.max_y() - area.min_y(),
        }
    }
}

/// What opened a panel. Its anchor is the spot the panel is placed against,
/// and its preferred side decides which way it opens first.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PanelTrigger {
    /// A right click or a long press, at a point.
    Pointer((f32, f32)),
    /// A button or field the panel belongs to.
    Control(PanelRect),
    /// A list row. Submenus open beside it, on the given side when there is
    /// room: a submenu goes the way its parent went, so it never covers the
    /// panels it came from.
    Row(PanelRect, Side),
    /// A text field. The panel sits against the caret or selection, not the
    /// whole field.
    Text { field: PanelRect, caret: PanelRect },
    /// Opened from the keyboard, against the focused control.
    Keyboard(PanelRect),
}

impl PanelTrigger {
    /// The rectangle the panel is placed against.
    pub(crate) fn anchor(self) -> PanelRect {
        match self {
            Self::Pointer(point) => PanelRect::at_point(point),
            Self::Control(rect) | Self::Row(rect, _) | Self::Keyboard(rect) => rect,
            Self::Text { caret, .. } => caret,
        }
    }

    /// The side the panel opens on when there is room.
    pub(crate) fn preferred_side(self) -> Side {
        match self {
            Self::Row(_, side) => side,
            Self::Pointer(_) | Self::Control(_) | Self::Text { .. } | Self::Keyboard(_) => {
                Side::Below
            }
        }
    }
}

/// The side of an anchor a panel opens on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Side {
    Below,
    Above,
    Right,
    Left,
}

impl Side {
    /// The side opposite this one, used when the preferred side has no room.
    fn opposite(self) -> Self {
        match self {
            Self::Below => Self::Above,
            Self::Above => Self::Below,
            Self::Right => Self::Left,
            Self::Left => Self::Right,
        }
    }

    /// The side that runs across this one: right for a panel above or below.
    fn perpendicular(self) -> Self {
        match self {
            Self::Below | Self::Above => Self::Right,
            Self::Right | Self::Left => Self::Below,
        }
    }
}

/// How a panel lines up with its anchor on the axis along the anchor's edge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum EdgeAlign {
    /// The start edges line up, or the end edges when the start has no room.
    Start,
    /// The centres line up on both axes, so a bubble sits on the anchor's middle.
    Centre,
}

/// Where a panel of `size` sits beside `anchor` in a window of `window`, with
/// start alignment. See [`place_aligned`].
pub(crate) fn place(
    anchor: PanelRect,
    size: (f32, f32),
    window: (f32, f32),
    prefer: Side,
    gap: f32,
) -> (f32, f32) {
    place_aligned(anchor, size, window, prefer, gap, EdgeAlign::Start)
}

/// Where a panel of `size` sits beside `anchor` in a window of `window`. It
/// opens on `prefer`, or on the opposite side when `prefer` has no room. With
/// [`EdgeAlign::Centre`], when neither of those fits, the two perpendicular sides
/// are tried in turn (right then left, or below then above) before falling
/// back to `prefer`. With [`EdgeAlign::Start`] it falls back to `prefer` directly.
/// The result is held inside the window on both axes. With no window size yet,
/// the panel opens on `prefer` unclamped.
pub(crate) fn place_aligned(
    anchor: PanelRect,
    size: (f32, f32),
    window: (f32, f32),
    prefer: Side,
    gap: f32,
    align: EdgeAlign,
) -> (f32, f32) {
    let (width, height) = size;
    let (window_width, window_height) = window;
    let fits = |side: Side| match side {
        Side::Below => anchor.bottom() + gap + height + PANEL_EDGE <= window_height,
        Side::Above => anchor.y - gap - height >= PANEL_EDGE,
        Side::Right => anchor.right() + gap + width + PANEL_EDGE <= window_width,
        Side::Left => anchor.x - gap - width >= PANEL_EDGE,
    };
    let unknown = window_width <= 0.0 || window_height <= 0.0;
    let side = if unknown || fits(prefer) {
        prefer
    } else if fits(prefer.opposite()) {
        prefer.opposite()
    } else if align == EdgeAlign::Centre {
        let perpendicular = prefer.perpendicular();
        [perpendicular, perpendicular.opposite()]
            .into_iter()
            .find(|&side| fits(side))
            .unwrap_or(prefer)
    } else {
        prefer
    };
    let (x, y) = match side {
        Side::Below | Side::Above => {
            let y = if side == Side::Below {
                anchor.bottom() + gap
            } else {
                anchor.y - gap - height
            };
            let x = match align {
                EdgeAlign::Centre => anchor.x + anchor.width / 2.0 - width / 2.0,
                EdgeAlign::Start => {
                    let start_fits = unknown || anchor.x + width + PANEL_EDGE <= window_width;
                    if start_fits {
                        anchor.x
                    } else {
                        anchor.right() - width
                    }
                }
            };
            (x, y)
        }
        Side::Right | Side::Left => {
            let x = if side == Side::Right {
                anchor.right() + gap
            } else {
                anchor.x - gap - width
            };
            let y = match align {
                EdgeAlign::Centre => anchor.y + anchor.height / 2.0 - height / 2.0,
                EdgeAlign::Start => anchor.y,
            };
            (x, y)
        }
    };
    if unknown {
        return (x, y);
    }
    (
        clamp_inside(x, width, window_width),
        clamp_inside(y, height, window_height),
    )
}

/// Holds a panel of `size` on one axis inside `limit`, keeping the edge margin.
fn clamp_inside(value: f32, size: f32, limit: f32) -> f32 {
    value.min(limit - size - PANEL_EDGE).max(PANEL_EDGE)
}

/// Where a menu of `size` opens against its trigger in a window of `window`.
/// A [`Panel`] of [`PanelKind::Menu`] is placed with this, so a caller that
/// needs a menu's rectangle before it draws gets the same one.
pub(crate) fn menu_rect(trigger: PanelTrigger, size: (f32, f32), window: (f32, f32)) -> PanelRect {
    let (x, y) = place(
        trigger.anchor(),
        size,
        window,
        trigger.preferred_side(),
        PANEL_GAP,
    );
    PanelRect {
        x,
        y,
        width: size.0,
        height: size.1,
    }
}

/// Where a text edit bar of `size` sits against a caret or selection: centred
/// over it, above when there is room under the window header, below otherwise,
/// held inside the window. The bar never covers the header.
pub(crate) fn text_bar_rect(anchor: PanelRect, size: (f32, f32), window: (f32, f32)) -> PanelRect {
    let room_above = anchor.y - PANEL_GAP - size.1 >= Dim::Header.px();
    let prefer = if room_above { Side::Above } else { Side::Below };
    let (x, y) = place_aligned(anchor, size, window, prefer, PANEL_GAP, EdgeAlign::Centre);
    PanelRect {
        x,
        y,
        width: size.0,
        height: size.1,
    }
}

/// The shield under a panel: a clear layer over the window that takes every
/// press, wheel turn and hover outside the panel. A press on it, or Escape
/// anywhere in the window, runs `on_dismiss`. Its children draw over it.
pub(crate) fn shield(
    tokens: &Tokens,
    core: FocusCore,
    on_dismiss: EventHandler<()>,
    on_escape: EventHandler<()>,
) -> Rect {
    let press = on_dismiss;
    let release = core.clone();
    rect()
        .position(Position::new_global().top(0.0).left(0.0))
        .tw(tokens, &[w_screen(), h_screen()])
        .on_global_key_down(move |event: Event<KeyboardEventData>| {
            if event.key == Key::Named(NamedKey::Escape) {
                event.stop_propagation();
                on_escape.call(());
            }
        })
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            event.prevent_default();
            // Closing the panel takes the shield away, so the release lands on
            // the page: claim the press, or the page's region would take it as
            // a press on empty space and move focus off the control that opened
            // the panel.
            claim_press(&core);
            press.call(());
        })
        .on_press(move |event: Event<PressEventData>| {
            event.stop_propagation();
            release_press(&release);
        })
        .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
}

/// What a panel is. The same panel is a floating menu on every size, or a
/// choice that is anchored on a desktop and a sheet on a phone.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PanelKind {
    /// Actions or a popover: anchored to its trigger on every size, over a
    /// clear shield. Outside presses dismiss it.
    Menu,
    /// Choosing one value from a set. Anchored over a clear shield on a
    /// desktop; on a phone a sheet that rises from the bottom edge over a
    /// dimmed scrim, with a grab handle, the title as its header and a height
    /// cap.
    Choice,
    /// A centred dialog over a scrim on every size. It has no trigger; its
    /// card is the only child, and a press or Escape outside it dismisses.
    Dialog,
    /// The text edit bar over a caret or selection. It has no shield, so the
    /// text under it stays editable; it is placed once its content is measured.
    Bar,
}

/// The one floating surface and its backdrop. The surface opens against its
/// trigger, or on a phone a choice rises as a sheet. A press on the shield
/// runs `on_dismiss`; a sheet dragged down past [`SHEET_DISMISS_DISTANCE`]
/// also dismisses. The exit motion plays, then `on_closed` runs.
#[derive(Clone, PartialEq)]
pub(crate) struct Panel {
    kind: PanelKind,
    /// Whether the panel is wanted. Closing plays the exit motion first.
    open: bool,
    trigger: PanelTrigger,
    /// The layer the backdrop paints on.
    layer: Layer,
    /// An anchored panel's outer width.
    width: f32,
    /// The full height of the rows, before any cap.
    content_height: f32,
    /// The sheet's header, which is the control's label.
    title: Option<String>,
    /// The scroll position of a body that scrolls.
    scroll: Option<ScrollController>,
    on_dismiss: EventHandler<()>,
    on_closed: EventHandler<()>,
    children: Vec<Element>,
    /// Draws only the surface, with no shield of its own. A submenu sits
    /// inside the shield of the menu it belongs to.
    nested: bool,
    /// Rendered inside the shield after the surface. A menu's submenus go here.
    overlaid: Vec<Element>,
    /// The menu root's keyboard focus, key handler and global pointer-move handler.
    focus: Option<AccessibilityId>,
    on_key: Option<EventHandler<Event<KeyboardEventData>>>,
    on_move: Option<EventHandler<(f32, f32)>>,
    /// The bar's measured size, read once it is laid out. The bar is placed from it.
    measure: Option<State<Option<(f32, f32)>>>,
    /// The type-ahead query, drawn as a pill beside the surface.
    pill: Option<Pill>,
    /// What Escape does, when it is more than dismissing.
    on_escape: Option<EventHandler<()>>,
}

impl Panel {
    pub(crate) fn new(
        kind: PanelKind,
        open: bool,
        trigger: PanelTrigger,
        children: Vec<Element>,
    ) -> Self {
        Self {
            kind,
            open,
            trigger,
            layer: Layer::Overlay,
            width: Dim::Menu.px(),
            content_height: 0.0,
            title: None,
            scroll: None,
            on_dismiss: EventHandler::new(|_: ()| {}),
            on_closed: EventHandler::new(|_: ()| {}),
            children,
            nested: false,
            overlaid: Vec::new(),
            focus: None,
            on_key: None,
            on_move: None,
            measure: None,
            pill: None,
            on_escape: None,
        }
    }

    /// A centred dialog over a scrim, on every size. `children` is the card.
    pub(crate) fn dialog(open: bool, layer: Layer, children: Vec<Element>) -> Self {
        let mut panel = Self::new(
            PanelKind::Dialog,
            open,
            PanelTrigger::Pointer((0.0, 0.0)),
            children,
        );
        panel.layer = layer;
        panel
    }

    /// The text edit bar, opened over `trigger`'s caret. `measure` holds its
    /// size once it is laid out, and it is placed from there.
    pub(crate) fn bar(
        open: bool,
        trigger: PanelTrigger,
        measure: State<Option<(f32, f32)>>,
        children: Vec<Element>,
    ) -> Self {
        let mut panel = Self::new(PanelKind::Bar, open, trigger, children);
        panel.measure = Some(measure);
        panel
    }

    /// The anchored width and the rows' full height.
    pub(crate) fn size(mut self, width: f32, content_height: f32) -> Self {
        self.width = width;
        self.content_height = content_height;
        self
    }

    pub(crate) fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Scrolls the rows with `scroll`, showing edge hairlines where they
    /// continue past the body.
    pub(crate) fn scroll(mut self, scroll: ScrollController) -> Self {
        self.scroll = Some(scroll);
        self
    }

    pub(crate) fn on_dismiss(mut self, on_dismiss: impl Into<EventHandler<()>>) -> Self {
        self.on_dismiss = on_dismiss.into();
        self
    }

    pub(crate) fn on_closed(mut self, on_closed: impl Into<EventHandler<()>>) -> Self {
        self.on_closed = on_closed.into();
        self
    }

    /// The layer the backdrop paints on.
    pub(crate) fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    /// Draws only the surface, with no shield of its own.
    pub(crate) fn nested(mut self) -> Self {
        self.nested = true;
        self
    }

    /// Renders `overlaid` inside the shield, after the surface.
    pub(crate) fn overlaid(mut self, overlaid: Vec<Element>) -> Self {
        self.overlaid = overlaid;
        self
    }

    /// Shows the query of type-ahead as a pill beside the surface.
    pub(crate) fn pill(mut self, pill: Option<Pill>) -> Self {
        self.pill = pill;
        self
    }

    /// What Escape does, anywhere in the window, instead of dismissing: a
    /// panel that takes back a typed query or a submenu first uses it. The
    /// handler dismisses the panel itself when there is nothing to take back.
    /// The panel's own key handler must leave Escape alone, since both hear it.
    pub(crate) fn on_escape(mut self, on_escape: impl Into<EventHandler<()>>) -> Self {
        self.on_escape = Some(on_escape.into());
        self
    }

    /// Makes this the root of a menu: its surface takes `focus`, key presses
    /// go to `on_key`, and every pointer move in the window goes to `on_move`.
    pub(crate) fn navigation(
        mut self,
        focus: AccessibilityId,
        on_key: impl Into<EventHandler<Event<KeyboardEventData>>>,
        on_move: impl Into<EventHandler<(f32, f32)>>,
    ) -> Self {
        self.focus = Some(focus);
        self.on_key = Some(on_key.into());
        self.on_move = Some(on_move.into());
        self
    }
}

impl Component for Panel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let core = use_focus_core();
        // An open menu, choice or dialog is part of the navigation stack, so
        // Back (Alt+Left, the mouse button, a swipe) dismisses it as Escape
        // does. A submenu is part of its menu, and the text bar is no modal.
        use_modal(
            self.open && !self.nested && self.kind != PanelKind::Bar,
            match self.kind {
                PanelKind::Dialog => ModalKind::Dialog,
                PanelKind::Choice => ModalKind::Choice,
                PanelKind::Menu | PanelKind::Bar => ModalKind::Menu,
            },
            self.title.as_deref().unwrap_or_default(),
            self.on_dismiss.clone(),
        );
        let on_closed = self.on_closed.clone();
        let motion = if self.kind == PanelKind::Dialog {
            DIALOG_MOTION_MS
        } else {
            90
        };
        let pull = use_state(|| 0.0_f32);
        let drag = use_sheet_drag(pull);
        // A sheet that has left forgets its pull, so it opens at rest again.
        let progress = use_presence(self.open, tokens.motion(motion), move || {
            on_closed.call(());
            drag.reset();
        });
        let visible = self.open || progress > 0.0;
        let root = *Platform::get().current_window().root_size.read();
        let window = (root.width, root.height);
        let sheet = self.kind == PanelKind::Choice && is_compact();
        let dismiss = self.on_dismiss.clone();
        // Escape closes the panel, unless the panel has something to take back
        // first (a typed query, an open submenu).
        let escape = self
            .on_escape
            .clone()
            .unwrap_or_else(|| dismiss.clone());
        let eased = use_sheet_pull(pull, drag.dragging(), tokens.motion(MOTION_MS));

        let backdrop = if self.kind == PanelKind::Dialog {
            self.centred(&tokens, core, dismiss, progress)
        } else if sheet {
            let layout = self.sheet_layout(window);
            let offset = sheet_offset(progress, eased, layout.metrics.travel);
            // The scrim is a sibling of the sheet, not its parent: a press on
            // the sheet never reaches it, so the sheet needs no handler that
            // stops a touch (which would drop the touch's release).
            sheet_shield(&tokens, escape.clone())
                .layer(self.layer)
                .child(sheet_scrim(
                    &tokens,
                    core.clone(),
                    scrim_amount(offset, layout.metrics.travel),
                    dismiss.clone(),
                ))
                .child(self.sheet(&tokens, &layout, offset, drag, dismiss.clone()))
        } else if self.kind == PanelKind::Bar {
            self.edit_bar(&tokens, window, progress)
        } else if self.nested {
            self.menu(&tokens, window, progress)
        } else {
            shield(&tokens, core, dismiss.clone(), escape.clone())
                .layer(self.layer)
                .child(self.menu(&tokens, window, progress))
                .children(self.overlaid.clone())
        };

        // The bar is sized by its content, so its wrapper is as wide as the window:
        // a bar inside a zero-width box would wrap every label to one letter.
        let wrapper_width = if self.kind == PanelKind::Bar {
            window.0
        } else {
            0.0
        };
        let pill = if visible && !sheet {
            self.query_pill(&tokens, window, progress)
        } else {
            None
        };
        rect()
            .component("Panel")
            .width(Size::px(wrapper_width))
            .height(Size::px(0.0))
            .maybe_child(visible.then_some(backdrop))
            .maybe_child(pill)
    }
}

impl Panel {
    /// The query pill of a menu or list that has type-ahead: a small pill
    /// under the surface (above it when there is no room below), aligned to
    /// its left edge. It sits outside the rows, so nothing in the list
    /// shifts, and it is announced to a screen reader as a polite status.
    fn query_pill(&self, tokens: &Tokens, window: (f32, f32), progress: f32) -> Option<Rect> {
        let pill = self.pill.as_ref()?;
        let viewport = if self.scroll.is_some() {
            self.content_height.min(Dim::MenuList.px())
        } else {
            self.content_height
        };
        let placed = menu_rect(
            self.trigger,
            (self.width, viewport + 2.0 * S2.px()),
            window,
        );
        let height = Dim::Box7.px();
        let below = placed.bottom() + PANEL_GAP;
        let y = if below + height + PANEL_EDGE <= window.1 {
            below
        } else {
            (placed.y - PANEL_GAP - height).max(PANEL_EDGE)
        };
        Some(
            rect()
                .component("QueryPill")
                .position(Position::new_global().top(y).left(placed.x))
                .layer(Layer::Overlay)
                .opacity(progress)
                .a11y_role(AccessibilityRole::Status)
                .a11y_alt(pill.announcement())
                .a11y_builder(|builder| builder.set_live(accesskit::Live::Polite))
                .tw(
                    tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        gap(S2),
                        h(Dim::Box7),
                        px(S3),
                        rounded(Radius::Pill),
                        bg(Paint::Popover),
                        border(Line::Hairline),
                        shadow(Elevation::Raised),
                    ],
                )
                .child(BODY.in_ink(Ink::Text).label(tokens, pill.query.clone()).max_lines(1))
                .child(
                    CAPTION
                        .in_ink(Ink::Muted)
                        .label(tokens, pill.count_text())
                        .max_lines(1),
                ),
        )
    }
}

impl Panel {
    /// The centred dialog: a scrim over the window and the card centred on
    /// it. The scrim and the card fade and scale in separately. A press on the
    /// scrim, or Escape, dismisses; the card stops its own presses.
    fn centred(&self, tokens: &Tokens, core: FocusCore, dismiss: EventHandler<()>, progress: f32) -> Rect {
        shield(tokens, core, dismiss.clone(), dismiss)
            .layer(self.layer)
            .child(
                rect()
                    .position(Position::new_global().top(0.0).left(0.0))
                    .tw(tokens, &[w_screen(), h_screen(), bg(Paint::Scrim)])
                    .opacity(progress),
            )
            .child(
                rect()
                    .position(Position::new_global().top(0.0).left(0.0))
                    .tw(tokens, &[w_screen(), h_screen(), center()])
                    .child(
                        rect()
                            .opacity(progress)
                            .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
                            .children(self.children.clone()),
                    ),
            )
    }

    /// A fade from the sheet's surface at the bottom of the rows, showing that
    /// the list continues below the sheet's edge.
    fn sheet_fade(tokens: &Tokens) -> Rect {
        let solid = Paint::Popover.color(tokens);
        let gradient = LinearGradient::new()
            .angle(180.0)
            .stop(GradientStop::new(solid.with_a(255), 0.0))
            .stop(GradientStop::new(solid.with_a(0), 100.0));
        gradient_fill(
            rect()
                .position(Position::new_absolute().bottom(0.0).left(0.0))
                .layer(Layer::Relative(1))
                .width(Size::fill())
                .height(Size::px(SHEET_FADE)),
            gradient,
        )
    }

    /// The rows, scrolled inside `viewport` when the panel scrolls, with a
    /// hairline at each edge the rows continue past. `tail` is padding that
    /// ends the rows, inside the scroll content. A sheet passes its `drag`: a
    /// finger drags the list, and a press in it can become the sheet's drag.
    fn body(
        &self,
        tokens: &Tokens,
        viewport: f32,
        inset: bool,
        tail: f32,
        drag: Option<SheetDrag>,
    ) -> Rect {
        let column = rect()
            .width(Size::fill())
            .children(self.children.clone())
            .maybe_child((tail > 0.0).then(|| rect().height(Size::px(tail))));
        let padding = [px(S4)];
        let column = if inset {
            column.tw(tokens, &padding)
        } else {
            column
        };
        let Some(scroll) = self.scroll.clone() else {
            return column;
        };
        let offset = <(i32, i32)>::from(scroll.clone()).1 as f32;
        let below = -offset + viewport < self.content_height + tail - 0.5;
        // A floating list shows that it continues with its scrollbar alone.
        // An edge hairline under the last visible row read as a stray line
        // at the bottom of the panel. A sheet fades its rows out instead.
        let mut region = rect().width(Size::fill()).height(Size::px(viewport));
        if let Some(drag) = drag {
            // Only a finger drags the list: a mouse scrolls it with the wheel.
            region = region.on_pointer_down(move |event: Event<PointerEventData>| {
                if matches!(event.data(), PointerEventData::Touch(_)) {
                    drag.press_content(event.global_location().y as f32);
                }
            });
        }
        region
            .child(
                ScrollView::new_controlled(scroll)
                    .drag_scrolling(drag.is_some())
                    .child(column),
            )
            .maybe_child((below && inset).then(|| Self::sheet_fade(tokens)))
    }

    /// The anchored surface: a menu or popover placed against its trigger.
    /// A menu's root also takes the keyboard and the pointer's moves.
    fn menu(&self, tokens: &Tokens, window: (f32, f32), progress: f32) -> Rect {
        let viewport = if self.scroll.is_some() {
            self.content_height.min(Dim::MenuList.px())
        } else {
            self.content_height
        };
        let size = (self.width, viewport + 2.0 * S2.px());
        let anchor = self.trigger.anchor();
        let placed = menu_rect(self.trigger, size, window);
        // The surface grows from the corner nearest its anchor.
        let origin = match (placed.y < anchor.y, placed.x < anchor.x) {
            (true, true) => TransformOrigin::bottom_right(),
            (true, false) => TransformOrigin::bottom_left(),
            (false, true) => TransformOrigin::top_right(),
            (false, false) => TransformOrigin::top_left(),
        };
        let mut surface = rect()
            .position(Position::new_global().top(placed.y).left(placed.x))
            .layer(Layer::Overlay)
            .width(Size::px(self.width))
            .opacity(progress)
            .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
            .transform_origin(origin)
            .tw(tokens, MENU)
            // A press or wheel turn inside the panel never reaches the shield.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
            .child(self.body(tokens, viewport, false, 0.0, None));
        if let Some(focus) = self.focus {
            surface = surface.a11y_id(focus).a11y_focusable(true);
        }
        if let Some(on_key) = self.on_key.clone() {
            surface =
                surface.on_key_down(move |event: Event<KeyboardEventData>| on_key.call(event));
        }
        if let Some(on_move) = self.on_move.clone() {
            surface = surface.on_global_pointer_move(move |event: Event<PointerEventData>| {
                let point = event.global_location();
                on_move.call((point.x as f32, point.y as f32));
            });
        }
        surface
    }

    /// The text edit bar: a pill of commands at the caret or selection. It is
    /// sized by its content, read through `measure`, and placed once known.
    fn edit_bar(&self, tokens: &Tokens, window: (f32, f32), progress: f32) -> Rect {
        let measure = self.measure;
        let size = measure.and_then(|measure| *measure.read());
        let at = size.map(|size| text_bar_rect(self.trigger.anchor(), size, window));
        let (x, y) = at.map_or((0.0, 0.0), |rect| (rect.x, rect.y));
        rect()
            .position(Position::new_global().top(y).left(x))
            .layer(Layer::Overlay)
            .width(Size::Inner)
            .opacity(if at.is_some() { progress } else { 0.0 })
            .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
            .tw(
                tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    h(Dim::Control),
                    rounded(Radius::Pill),
                    bg(Paint::Floating),
                    border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
                    shadow(Elevation::Raised),
                ],
            )
            // A press on the bar never reaches the field under it.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_sized(move |event: Event<SizedEventData>| {
                if let Some(mut measure) = measure {
                    measure.set_if_modified(Some((event.area.width(), event.area.height())));
                }
            })
            .children(self.children.clone())
    }

    /// The sheet's geometry in a window of `window`: it is as tall as its
    /// rows, the grabber and the title, and the bottom padding that ends its
    /// scroll content, up to the share of the window it may take. The rows
    /// scroll inside what is left.
    fn sheet_layout(&self, window: (f32, f32)) -> SheetLayout {
        let overhang = sheet_overhang();
        let header = if self.title.is_some() {
            Dim::Control.px()
        } else {
            0.0
        };
        let tail = sheet_bottom();
        let chrome = GRABBER_ROW + header;
        let percent = if self.kind == PanelKind::Choice {
            CHOICE_SHEET_MAX_HEIGHT_PERCENT
        } else {
            SHEET_MAX_HEIGHT_PERCENT
        };
        let cap = (window.1 * percent / 100.0 - chrome).max(0.0);
        let viewport = (self.content_height + tail).min(cap);
        // A device preview holds the page to a phone-wide column in the middle
        // of the window: the sheet rises from the bottom of that column, as the
        // page's own sheets do, not across the whole window.
        let column = page_column(window.0);
        SheetLayout {
            header,
            tail,
            viewport,
            column: (column < window.0).then(|| ((window.0 - column) / 2.0, column)),
            metrics: Metrics::new(chrome + viewport, overhang, SHEET_DISMISS_DISTANCE),
        }
    }

    /// The sheet at the bottom edge: a grabber, the title and the rows. Its
    /// bottom corners sit below the window, so only the top corners show.
    ///
    /// The slot is positioned once and carries the motion: `offset` shifts
    /// the card inside it, background and rows together, so the sheet moves
    /// as one piece and nothing inside it reflows. (An offset on the card
    /// itself would shift its rows and leave its background behind.) The
    /// grabber, the title strip and a finger dragging the list down from its
    /// top pull the sheet through `drag`; a release past the threshold, or a
    /// fling, calls `dismiss`, and anything else springs back.
    fn sheet(
        &self,
        tokens: &Tokens,
        layout: &SheetLayout,
        offset: f32,
        drag: SheetDrag,
        dismiss: EventHandler<()>,
    ) -> Rect {
        let overhang = sheet_overhang();
        let metrics = layout.metrics;
        let scroll = self.scroll;
        let card = rect()
            .component("Sheet")
            .tw(
                tokens,
                &[
                    w_full(),
                    flex_col(),
                    bg(Paint::Popover),
                    rounded(Radius::Overlay),
                    clip(),
                ],
            )
            .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let at_top = scroll.is_none_or(scrolled_to_top);
                let y = event.global_location().y as f32;
                if drag.moved(y, at_top, &metrics) == Claim::Sheet
                    && let Some(mut scroll) = scroll
                {
                    // The sheet has the gesture, so the list holds its top.
                    scroll.scroll_to_y(0);
                }
            })
            .on_global_pointer_up(move |event: Event<PointerEventData>| {
                let cancelled = matches!(
                    event.data(),
                    PointerEventData::Touch(touch) if touch.phase == TouchPhase::Cancelled
                );
                let outcome = if cancelled {
                    drag.cancelled()
                } else {
                    drag.released(&metrics)
                };
                if outcome == Outcome::Dismiss {
                    dismiss.call(());
                }
            })
            .on_touch_cancel(move |_: Event<TouchEventData>| {
                drag.cancelled();
            })
            .child(grabber(tokens, move |y| drag.grab(y)))
            .maybe_child(self.title.clone().map(|title| {
                // The title strip pulls the sheet as the grabber does: a
                // pill alone is a small thing to hit with a finger.
                rect()
                    .width(Size::fill())
                    .height(Size::px(layout.header))
                    .tw(
                        tokens,
                        &[flex_row(), items(crate::style::Align::Center), px(S4)],
                    )
                    .on_pointer_down(grab_handler(move |y| drag.grab(y)))
                    .child(
                        EMPHASIS
                            .in_ink(Ink::Text)
                            .label(tokens, title)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
            }))
            .child(self.body(tokens, layout.viewport, true, layout.tail, Some(drag)))
            .child(rect().height(Size::px(overhang)));
        let (left, width) = layout.column.map_or((0.0, Size::fill()), |(left, width)| {
            (left, Size::px(width))
        });
        rect()
            .component("SheetSlot")
            .position(Position::new_absolute().bottom(-overhang).left(left))
            .layer(Layer::Overlay)
            .width(width)
            .offset_y(offset)
            .child(card)
    }
}

/// What a sheet's size is made of.
struct SheetLayout {
    /// The title strip's height, or zero without a title.
    header: f32,
    /// The padding that ends the scroll content, below the last row.
    tail: f32,
    /// The visible height of the rows.
    viewport: f32,
    /// The page column's left edge and width when the page is narrower than
    /// the window (a device preview); `None` when the page is the window.
    column: Option<(f32, f32)>,
    metrics: Metrics,
}

/// Whether the list is scrolled to its top.
fn scrolled_to_top(scroll: ScrollController) -> bool {
    <(i32, i32)>::from(scroll).1 >= 0
}

/// The clear layer under a sheet: it takes the wheel and presses over the
/// window and hears Escape, and dismisses nothing itself. The scrim beside
/// the sheet does that.
fn sheet_shield(tokens: &Tokens, on_escape: EventHandler<()>) -> Rect {
    rect()
        .position(Position::new_global().top(0.0).left(0.0))
        .tw(tokens, &[w_screen(), h_screen()])
        .on_global_key_down(move |event: Event<KeyboardEventData>| {
            if event.key == Key::Named(NamedKey::Escape) {
                event.stop_propagation();
                on_escape.call(());
            }
        })
        .on_press(|event: Event<PressEventData>| event.stop_propagation())
        .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
}

/// The dim layer behind a sheet, `amount` of it showing. A press on it dismisses.
fn sheet_scrim(tokens: &Tokens, core: FocusCore, amount: f32, on_dismiss: EventHandler<()>) -> Rect {
    let release = core.clone();
    rect()
        .position(Position::new_global().top(0.0).left(0.0))
        .tw(tokens, &[w_screen(), h_screen(), bg(Paint::Scrim)])
        .opacity(amount)
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            event.prevent_default();
            // The release that follows lands on the page once the sheet has
            // gone: it must not take focus off the control the sheet belongs to.
            claim_press(&core);
            on_dismiss.call(());
        })
        .on_press(move |_: Event<PressEventData>| release_press(&release))
}
