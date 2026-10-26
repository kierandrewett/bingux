//! The one floating surface. Menus, dropdown lists, popovers, tooltips,
//! action sheets and popups all place themselves through the same geometry
//! here: a [`PanelTrigger`] says what opened the panel, and [`place`] puts
//! a panel of a given size beside that trigger, flipping and clamping it
//! inside the window.

use freya::animation::*;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::motion::{PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{EMPHASIS, Ink};

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
pub(crate) const CHOICE_SHEET_MAX_HEIGHT_PERCENT: f32 = 60.0;
/// How tall the fade at the bottom of a sheet's rows is.
pub(crate) const SHEET_FADE: f32 = 28.0;
/// How far a sheet is dragged down before it dismisses on release.
pub(crate) const SHEET_DISMISS_DISTANCE: f32 = 96.0;

/// How far a sheet extends past the bottom edge, so its bottom corners sit
/// off screen and only its top corners show.
pub(crate) fn sheet_overhang() -> f32 {
    Radius::Overlay.px()
}

/// The grabber row at the top of a sheet: a short pill. A press calls
/// `on_press` with the pointer's y, so the caller can start a drag.
pub(crate) fn grabber(tokens: &Tokens, mut on_press: impl FnMut(f32) + 'static) -> Rect {
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
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            on_press(event.global_location().y as f32);
        })
        .child(
            rect()
                .width(Size::px(GRABBER_WIDTH))
                .height(Size::px(GRABBER_HEIGHT))
                .tw(tokens, &[bg(Paint::Control), rounded(Radius::Pill)]),
        )
}

/// The sheet's pull, eased. It follows the pointer while the sheet is dragged
/// and springs back to rest on release; a dismiss keeps it where it was.
pub(crate) fn use_sheet_pull(pull: State<f32>, motion: u64) -> f32 {
    use_animation_transition(*pull.read(), move |from: f32, to: f32| {
        // Only the spring back eases: a drag follows the pointer exactly.
        let time = if to <= 0.0 { motion } else { 0 };
        AnimNum::new(from, to)
            .time(time)
            .function(Function::Quad)
            .ease(Ease::Out)
    })
    .get()
    .value()
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
    /// A list row. Submenus open beside it.
    Row(PanelRect),
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
            Self::Control(rect) | Self::Row(rect) | Self::Keyboard(rect) => rect,
            Self::Text { caret, .. } => caret,
        }
    }

    /// The side the panel opens on when there is room.
    pub(crate) fn preferred_side(self) -> Side {
        match self {
            Self::Row(_) => Side::Right,
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
/// over it, above when there is room, below otherwise, held inside the window.
pub(crate) fn text_bar_rect(anchor: PanelRect, size: (f32, f32), window: (f32, f32)) -> PanelRect {
    let (x, y) = place_aligned(anchor, size, window, Side::Above, PANEL_GAP, EdgeAlign::Centre);
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
pub(crate) fn shield(tokens: &Tokens, on_dismiss: EventHandler<()>) -> Rect {
    let press = on_dismiss.clone();
    rect()
        .position(Position::new_global().top(0.0).left(0.0))
        .tw(tokens, &[w_screen(), h_screen()])
        .on_global_key_down(move |event: Event<KeyboardEventData>| {
            if event.key == Key::Named(NamedKey::Escape) {
                event.stop_propagation();
                on_dismiss.call(());
            }
        })
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            event.prevent_default();
            press.call(());
        })
        .on_press(|event: Event<PressEventData>| event.stop_propagation())
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
        let on_closed = self.on_closed.clone();
        let motion = if self.kind == PanelKind::Dialog {
            DIALOG_MOTION_MS
        } else {
            90
        };
        let progress = use_presence(self.open, tokens.motion(motion), move || on_closed.call(()));
        let visible = self.open || progress > 0.0;
        let root = *Platform::get().current_window().root_size.read();
        let window = (root.width, root.height);
        let sheet = self.kind == PanelKind::Choice && is_compact();
        let dismiss = self.on_dismiss.clone();
        let grab = use_state(|| None::<f32>);
        let pull = use_state(|| 0.0_f32);
        let eased = use_sheet_pull(pull, tokens.motion(MOTION_MS));

        let backdrop = if self.kind == PanelKind::Dialog {
            self.centred(&tokens, dismiss, progress)
        } else if sheet {
            shield(&tokens, dismiss.clone())
                .layer(self.layer)
                .child(
                    rect()
                        .position(Position::new_global().top(0.0).left(0.0))
                        .tw(&tokens, &[w_screen(), h_screen(), bg(Paint::Scrim)])
                        .opacity(progress),
                )
                .child(self.sheet(
                    &tokens,
                    window,
                    progress,
                    grab,
                    pull,
                    eased,
                    dismiss.clone(),
                ))
        } else if self.kind == PanelKind::Bar {
            self.edit_bar(&tokens, window, progress)
        } else if self.nested {
            self.menu(&tokens, window, progress)
        } else {
            shield(&tokens, dismiss.clone())
                .layer(self.layer)
                .child(self.menu(&tokens, window, progress))
                .children(self.overlaid.clone())
        };

        // The bar is sized by its content, so its wrapper is as wide as the window:
        // a bar inside a zero-width box would wrap every label to one letter.
        let wrapper_width = if self.kind == PanelKind::Bar { window.0 } else { 0.0 };
        rect()
            .component("Panel")
            .width(Size::px(wrapper_width))
            .height(Size::px(0.0))
            .maybe_child(visible.then_some(backdrop))
    }
}

impl Panel {
    /// The centred dialog: a scrim over the window and the card centred on
    /// it. The scrim and the card fade and scale in separately. A press on the
    /// scrim, or Escape, dismisses; the card stops its own presses.
    fn centred(&self, tokens: &Tokens, dismiss: EventHandler<()>, progress: f32) -> Rect {
        shield(tokens, dismiss)
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
    /// hairline at each edge the rows continue past.
    fn body(&self, tokens: &Tokens, viewport: f32, inset: bool) -> Rect {
        let column = rect().width(Size::fill()).children(self.children.clone());
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
        let above = offset < -0.5;
        let below = -offset + viewport < self.content_height - 0.5;
        let edge = |bottom: bool| {
            rect()
                .position(if bottom {
                    Position::new_absolute().bottom(0.0).left(0.0)
                } else {
                    Position::new_absolute().top(0.0).left(0.0)
                })
                .layer(Layer::Relative(1))
                .tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)])
        };
        rect()
            .width(Size::fill())
            .height(Size::px(viewport))
            .child(
                ScrollView::new_controlled(scroll)
                    .drag_scrolling(false)
                    .child(column),
            )
            .maybe_child(above.then(|| edge(false)))
            .maybe_child((below && inset).then(|| Self::sheet_fade(tokens)))
            .maybe_child((below && !inset).then(|| edge(true)))
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
            .child(self.body(tokens, viewport, false));
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

    /// The sheet at the bottom edge: a grabber, the title and the rows. Its
    /// bottom corners sit below the window, so only the top corners show. It
    /// slides up with `progress` and follows a drag. Releasing a drag past
    /// [`SHEET_DISMISS_DISTANCE`] dismisses; otherwise it springs back.
    fn sheet(
        &self,
        tokens: &Tokens,
        window: (f32, f32),
        progress: f32,
        mut grab: State<Option<f32>>,
        mut pull: State<f32>,
        eased: f32,
        dismiss: EventHandler<()>,
    ) -> Rect {
        let overhang = sheet_overhang();
        let header = if self.title.is_some() {
            Dim::Control.px()
        } else {
            0.0
        };
        let chrome = GRABBER_ROW + header + overhang;
        let percent = if self.kind == PanelKind::Choice {
            CHOICE_SHEET_MAX_HEIGHT_PERCENT
        } else {
            SHEET_MAX_HEIGHT_PERCENT
        };
        let cap = (window.1 * percent / 100.0 - chrome).max(0.0);
        let viewport = self.content_height.min(cap);
        let slide = (1.0 - progress) * (chrome + viewport + overhang) + eased;
        rect()
            .position(Position::new_absolute().bottom(-overhang).left(0.0))
            .layer(Layer::Overlay)
            .width(Size::fill())
            .offset_y(slide)
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
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                if let Some(start) = *grab.peek() {
                    pull.set_if_modified((event.global_location().y as f32 - start).max(0.0));
                }
            })
            .on_global_pointer_up(move |_| {
                if grab.peek().is_none() {
                    return;
                }
                grab.set(None);
                if *pull.peek() > SHEET_DISMISS_DISTANCE {
                    dismiss.call(());
                } else {
                    pull.set_if_modified(0.0);
                }
            })
            .child(grabber(tokens, move |y| grab.set(Some(y))))
            .maybe_child(self.title.clone().map(|title| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(header))
                    .tw(
                        tokens,
                        &[flex_row(), items(crate::style::Align::Center), px(S4)],
                    )
                    .child(
                        EMPHASIS
                            .in_ink(Ink::Text)
                            .label(tokens, title)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
            }))
            .child(self.body(tokens, viewport, true))
            .child(rect().height(Size::px(overhang)))
    }
}
