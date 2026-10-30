//! The navigation split view: a sidebar with a search field and a flat list
//! of destinations beside the page column. The sidebar is the shell's
//! sidebar surface with a hairline on its right edge. Items are rows with a
//! muted icon and a normal-weight title; the selected row carries the accent
//! selection layer, and hover and press are token layers drawn over it.
//!
//! The view draws the window's navigation stack ([`crate::nav`]): the sidebar
//! lists the root routes of the app's route table, the page is the route of
//! the current location, the header's Back button, title and history menu come
//! from the stack, and a page change is animated from the change the router
//! recorded. Pages never wire Back themselves.
//!
//! Below [`NARROW_BELOW`] the window is a phone: the index (the list of pages)
//! is a screen of its own, a page opens over it with a Back button in the
//! header, the window buttons are hidden, and an edge swipe from the left goes
//! back: predictive back, with the previous screen live beneath the current
//! one (see `design/navigation.md`, section 4).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use freya::animation::*;
use freya::prelude::*;

use crate::component::{ComponentName, Prop, Published};
use crate::controls::{action_button, search_field};
use crate::controls::press::{is_touch, use_press};
use crate::focus::{RegionKind, claim_press, use_focus_core, use_region};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, crossfade};
use crate::nav::gesture::{
    BackGesture, DRAG_SHARE, Pose, Release, SLIDE_MS, Update, release_curve, release_slope, slide,
};
use crate::nav::router::EntryContext;
use crate::nav::{Change, Entry, Location, NavModel, RouteKind, RouteTable, Transition, use_router};
use crate::shell::back_button::BackButton;
use crate::shell::sections::{self, SearchHit};
use crate::shell::window::{HeaderBar, WindowControls};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::theme::{NARROW_BELOW, Tokens};
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink, PAGE_TITLE};

/// The split view. It reads the window's router: the sidebar lists the roots
/// of its route table and the content is the page of the current location.
#[derive(PartialEq)]
pub struct NavigationSplitView {
    pub app_title: String,
    pub page_width: Dim,
    /// An element in the header bar, before the window buttons.
    pub header_trailing: Option<Element>,
    pub profile: Option<Profile>,
    /// While the search has text, `results` replaces the page content, and
    /// Enter in the search field calls `on_submit`. See
    /// [`NavigationSplitView::search`].
    pub search: Option<(State<String>, Element, EventHandler<()>)>,
}

/// The signed-in person at the top of the sidebar: an avatar and the name,
/// laid out like a sidebar item. Pressing it switches to the root at `target`.
#[derive(Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub target: Location,
}

#[derive(Clone, PartialEq)]
pub enum SidebarMark {
    /// A page icon, in the text colour when the row is selected.
    Icon(Icon),
    /// The account avatar for a login name.
    Avatar(String),
}

/// A row of the sidebar: a page, the profile or a search result. Every sidebar
/// row is this component, so they share the icon column, the label start, the
/// neutral selection layer and the hover, press and focus states.
#[derive(Clone, PartialEq)]
pub struct SidebarRow {
    pub mark: SidebarMark,
    pub label: String,
    pub selected: bool,
    pub on_press: EventHandler<()>,
    /// Keys other than Enter and Space, such as the arrows. Enter and Space
    /// press the row.
    pub on_key: Option<EventHandler<Event<KeyboardEventData>>>,
}

impl Component for SidebarRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let press = use_press();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let selected = self.selected;
        let on_press = self.on_press.clone();
        let on_press_key = self.on_press.clone();
        let on_key = self.on_key.clone();
        let core = use_focus_core();
        let enter_core = core.clone();
        let icon_color = if selected {
            Paint::Text.color(&tokens)
        } else {
            Paint::Muted.color(&tokens)
        };
        let column = rect().tw(&tokens, NAV_ICON);
        let column = match &self.mark {
            SidebarMark::Icon(icon) => column.child(icons::icon(*icon, IconSize::Row, icon_color)),
            SidebarMark::Avatar(user) => {
                column.child(crate::controls::avatar(user.clone(), Dim::NavIcon))
            }
        };
        rect()
            .component("SidebarRow")
            .tw(&tokens, NAV_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering(),
                    pressing(),
                    selected,
                )],
            )
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(selected))
            // A finger that lands on a row is not hovering it: the highlight
            // is the pressed state only, so it cannot stay behind.
            .on_pointer_enter(move |event: Event<PointerEventData>| {
                if !is_touch(&event) {
                    hovering.set(true);
                }
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            // Press feedback on touch-down. It clears when the finger lifts,
            // leaves the row, is cancelled, or strays far enough to be
            // scrolling the list: that touch is not a tap.
            .on_pointer_down(move |event: Event<PointerEventData>| {
                press.begin(&event, None);
                pressing.set(true);
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                press.moved(&event);
                if is_touch(&event) && !press.pending() {
                    pressing.set_if_modified(false);
                }
            })
            .on_global_pointer_up(move |_| {
                press.release();
                pressing.set_if_modified(false);
            })
            .on_press(move |event: Event<PressEventData>| {
                // A touch that scrolled the list and lifted over this row
                // does not choose it.
                if !press.tap(&event) {
                    return;
                }
                claim_press(&core);
                on_press.call(());
                core.focus_region(RegionKind::Content, false);
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    event.stop_propagation();
                    on_press_key.call(());
                    enter_core.focus_region(RegionKind::Content, true);
                } else if let Some(on_key) = &on_key {
                    on_key.call(event);
                }
            })
            .child(column)
            .child(
                BODY.label(&tokens, self.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            ).focusable(focus())
            .tw(&tokens, &[w_full()])
    }
}

#[derive(PartialEq)]
struct ProfileCard {
    profile: Profile,
    selected: bool,
}

impl Component for ProfileCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let router = use_router();
        let target = self.profile.target.clone();
        let row = SidebarRow {
            mark: SidebarMark::Avatar(crate::controls::avatar::current_user()),
            label: self.profile.name.clone(),
            selected: self.selected,
            on_press: (move |_: ()| router.switch_root(&target)).into(),
            on_key: None,
        };
        rect()
            .component("ProfileCard")
            .tw(&tokens, &[w_full(), px(S2), py(S2)])
            .child(Element::from(row))
    }
}

/// The ids of the pages that are busy. The split view provides it, and a page
/// marks itself busy with [`use_page_busy`].
#[derive(Clone, Copy, PartialEq)]
struct BusyPages(State<Vec<u64>>);

/// Shows the header's busy spinner while `busy` is true. Use it instead of a
/// loading block in the page, so the content does not move.
pub fn use_page_busy(busy: bool) {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = use_hook(|| NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let pages = use_try_consume::<BusyPages>().map(|found| found.0);
    use_side_effect_with_deps(&busy, move |busy| {
        let Some(mut pages) = pages else { return };
        let listed = pages.read().contains(&id);
        if *busy && !listed {
            pages.write().push(id);
        } else if !*busy && listed {
            pages.write().retain(|other| *other != id);
        }
    });
    use_drop(move || {
        if let Some(mut pages) = pages {
            pages.write().retain(|other| *other != id);
        }
    });
}

/// A button in the page header, shown at the right of the title.
#[derive(Clone, PartialEq)]
pub struct PageAction {
    pub label: String,
    pub on_press: EventHandler<()>,
    /// When set, the action is a switch in this state and `on_press` is
    /// called with no value to flip it; the label is its tooltip.
    pub switch: Option<bool>,
}

/// The title block at the top of a page: the page's name large, the status
/// line under it when the page sets one, and an action at the right. A page
/// puts it first in its own content, so it draws and scrolls with the page and
/// always shows that page's own status. The name comes from the page frame the
/// split view draws around each page.
#[derive(Clone, Default, PartialEq)]
pub struct PageHead {
    pub subtitle: Option<String>,
    pub action: Option<PageAction>,
}

impl PageHead {
    pub fn new() -> Self {
        Self::default()
    }

    /// The sentence under the title, such as "Connected to Home".
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn action(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.action = Some(PageAction {
            label: label.into(),
            on_press: on_press.into(),
            switch: None,
        });
        self
    }

    /// A master switch at the right of the title, as GNOME puts Wi-Fi and
    /// Bluetooth. `label` names it for assistive tools; `on_toggle` receives
    /// the new state.
    pub fn switch(
        mut self,
        label: impl Into<String>,
        on: bool,
        on_toggle: impl Into<EventHandler<bool>>,
    ) -> Self {
        let on_toggle = on_toggle.into();
        self.action = Some(PageAction {
            label: label.into(),
            on_press: EventHandler::new(move |_: ()| on_toggle.call(!on)),
            switch: Some(on),
        });
        self
    }
}

impl Component for PageHead {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let title = try_consume_context::<CurrentPage>().map(|page| page.title);
        let action: Option<Element> = self.action.clone().map(|action| match action {
            PageAction {
                on_press,
                switch: Some(on),
                ..
            } => crate::controls::switch(on, move |_: bool| on_press.call(())).into(),
            action => action_button(action.label, action.on_press).into(),
        });
        let status = self.subtitle.clone().unwrap_or_default();
        // A compact page shrinks and fades its large title as it scrolls. A
        // desktop page keeps its title still.
        let large = try_consume_context::<PageScroll>()
            .filter(|page| page.compact)
            .map(|page| large_title_progress(page.scroll));
        title_row(&tokens, title, status, action, large)
    }
}

/// The page's name, provided by the frame the split view draws around the
/// page. A [`PageHead`] reads it, so its name is the page it is drawn in.
#[derive(Clone, PartialEq)]
struct CurrentPage {
    title: String,
}

/// The page as the split view draws it: its entry, its name, its scroll, and
/// whether a phone draws it, provided to the page content. The split view
/// keeps the frame of a page that is sliding out, so that page keeps its own
/// name and entry while it moves.
#[derive(Clone, PartialEq)]
struct PageFrame {
    entry: u64,
    title: String,
    content: Element,
    scroll: ScrollController,
    compact: bool,
}

impl Component for PageFrame {
    fn render(&self) -> impl IntoElement {
        provide_context(EntryContext { id: self.entry });
        provide_context(CurrentPage {
            title: self.title.clone(),
        });
        provide_context(PageScroll {
            scroll: self.scroll,
            compact: self.compact,
        });
        self.content.clone()
    }
}

/// The kind of move the view draws.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Motion {
    Push,
    Pop,
    Fade,
    /// A compact pop: the current screen shrinks and slides away while the
    /// previous one grows beneath it ([`Pose`]).
    Predictive,
}

impl Motion {
    /// How long the move takes. A subpage opening and closing take the same
    /// time: [`SLIDE_MS`], a compact close (predictive back) and a swipe
    /// released past its threshold included.
    fn ms(self) -> u64 {
        match self {
            Motion::Fade => 120,
            Motion::Push | Motion::Pop | Motion::Predictive => SLIDE_MS,
        }
    }
}

/// The motion for a transition the router recorded. A pop on a compact window
/// is the predictive-back animation; elsewhere it is the page sliding out.
fn motion_for(transition: Transition, compact: bool) -> Option<Motion> {
    match transition {
        Transition::None => None,
        Transition::Push => Some(Motion::Push),
        Transition::Fade => Some(Motion::Fade),
        Transition::Pop if compact => Some(Motion::Predictive),
        Transition::Pop => Some(Motion::Pop),
    }
}

/// Layers for the slide. Freya gives each node the layer of its parent plus
/// one, so the rows of a deep page sit several layers up and paint over a
/// shallow sheet. The moving sheet sits above the window header, which it
/// covers as it moves; its own header strip sits above the sheet's rows.
const SLIDE_SHEET_LAYER: i16 = crate::layers::SLIDE_SHEET;
const SLIDE_HEADER_LAYER: i16 = crate::layers::SLIDE_HEADER;
/// The layer of the veil over the screen beneath a predictive-back drag: above
/// that screen's rows, below the screen that moves.
const BACK_VEIL_LAYER: i16 = crate::layers::BACK_VEIL;
/// The layer of the shield that takes the touch while a back drag runs.
const BACK_SHIELD_LAYER: i16 = crate::layers::BACK_SHIELD;
/// The layer of the window buttons lifted over a moving sheet, above the
/// sheet's own header strip.
const SLIDE_CONTROLS_LAYER: i16 = crate::layers::SLIDE_CONTROLS;

/// A slide in progress: the page that moved away, its header strip when it was
/// a sheet, and where the moving page starts from. The value runs from the last
/// frame drawn, so a move that starts mid-slide continues from there.
#[derive(Clone)]
struct Slide {
    motion: Motion,
    /// The drag progress a predictive pop continues from: 0 for a pop that
    /// was not a swipe.
    d0: f32,
    /// How fast the finger was going as it let go, in drag progress per
    /// second: what a swipe's pop carries on at.
    speed: f32,
    outgoing: Element,
    outgoing_header: Option<Element>,
    /// A compact window moves whole screens: the one that went away, with its
    /// header.
    outgoing_screen: Option<Element>,
    start_x: f32,
    /// The outgoing page's own scroll, so it holds still as it slides.
    scroll: ScrollController,
}

impl Slide {
    /// A slide from what is on screen: `shown` is the last frame's slide, when
    /// one was running, so a move that interrupts it continues from there.
    fn start(
        motion: Motion,
        outgoing: Element,
        outgoing_header: Option<Element>,
        shown: Option<(Motion, f32)>,
        scroll: ScrollController,
    ) -> Self {
        let start_x = match (motion, shown) {
            (Motion::Push, Some((Motion::Pop, x))) => x,
            (Motion::Push, _) => 1.0,
            (Motion::Pop, Some((Motion::Push, x))) => x,
            (Motion::Pop, _) => 0.0,
            (Motion::Fade | Motion::Predictive, _) => 0.0,
        };
        Self {
            motion,
            d0: 0.0,
            speed: 0.0,
            outgoing,
            outgoing_header,
            outgoing_screen: None,
            start_x,
            scroll,
        }
    }

    /// How far along the move is, 0 to 1, a moment `time` (0 to 1) into it.
    fn eased(&self, time: f32) -> f32 {
        curve(self.motion, swipe_slope(self.d0, self.speed), time)
    }

    /// The moving page's offset, as a fraction of the width, `time` (0 to 1)
    /// into the move.
    fn at(&self, time: f32) -> f32 {
        offset(self.motion, self.start_x, self.eased(time))
    }
}

/// How far along a move is, 0 to 1, a moment `time` (0 to 1) into it. One
/// curve for every slide, [`slide`], which is point-symmetric: a page that
/// arrives and a page that leaves are the same motion, the close being the
/// open both played backwards and mirrored in space. A swipe released past
/// its threshold (`swipe`, the slope of its release) carries on from the
/// finger's speed instead, so it never stands still where the finger let go.
fn curve(motion: Motion, swipe: Option<f32>, time: f32) -> f32 {
    match (motion, swipe) {
        (Motion::Predictive, Some(slope)) => release_curve(time, slope),
        _ => slide(time),
    }
}

/// The start of the curve a swiped pop takes, as a multiple of its average
/// speed: the finger's `speed` (drag progress per second) as it let go at
/// drag progress `d0`, read in commit progress (the drag pose finishes over
/// [`DRAG_SHARE`] of it). `None` for a move that is not a released swipe.
fn swipe_slope(d0: f32, speed: f32) -> Option<f32> {
    (d0 > 0.0).then(|| release_slope(speed * DRAG_SHARE, 1.0 - d0))
}

/// Where the moving page is, as a fraction of the width, once the move is
/// `progress` of the way, for a slide that starts at `start_x`.
fn offset(motion: Motion, start_x: f32, progress: f32) -> f32 {
    match motion {
        Motion::Push => start_x * (1.0 - progress),
        Motion::Pop => start_x + (1.0 - start_x) * progress,
        Motion::Fade | Motion::Predictive => 0.0,
    }
}

/// How far a moving page is in its card look when its edge is `x` of the
/// width from its place: a card over the first 15 % of the way, flat at rest.
/// A function of where the page is, not of the time, so a page leaving and a
/// page arriving look the same at the same place.
fn card_amount(x: f32) -> f32 {
    const EDGE: f32 = 0.15;
    (x / EDGE).clamp(0.0, 1.0)
}


/// The page's scroll and size class, provided by the frame. A compact
/// [`PageHead`] reads the scroll to shrink and fade its large title.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PageScroll {
    pub(crate) scroll: ScrollController,
    pub(crate) compact: bool,
}

/// Makes a moving sheet a card: rounded on its leading edge and casting a
/// shadow to the left, both scaled by `amount` so they vanish at rest. The
/// sheet always takes the slide layer, so the rows of the page under it never
/// paint over it, even while it is still flat.
fn card_look(page: Rect, tokens: &Tokens, amount: f32) -> Rect {
    // The shadow is cast by this outer node, which must not clip: a clip on
    // the same node would cut off the shadow falling past its own edge. The
    // page is clipped by the inner node, to the same rounded corners.
    let sheet = rect()
        .width(Size::fill())
        .height(Size::fill())
        .layer(Layer::Relative(SLIDE_SHEET_LAYER))
        .tw(
            tokens,
            &[
                bg(Paint::Bg),
                rounded_left_scaled(Radius::Window, amount),
                sheet_edge_shadow(amount),
            ],
        );
    sheet.child(
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .tw(
                tokens,
                &[clip(), rounded_left_scaled(Radius::Window, amount)],
            )
            .child(page),
    )
}

/// A pointer's x as the back gesture reads it: the gesture is written for a
/// window that reads from the left (a press in the edge zone at the start, a
/// swipe towards the end), so in a window that reads from the right the pointer
/// is reflected and the swipe starts at the right edge.
fn swipe_x(x: f32, width: f32, direction: crate::controls::direction::Direction) -> f32 {
    if direction.is_rtl() { width - x } else { x }
}

/// Places a page in a slide. The offset lives on a frame around the page, not
/// on the sheet: torin applies an offset to a node's children only, so a sheet
/// that carried it would keep its background, layer and shadow where they were
/// and leave the page beneath exposed.
fn slide_frame(offset: f32, opacity: f32, sheet: Option<f32>, tokens: &Tokens, page: Rect) -> Rect {
    let page = match sheet {
        Some(amount) => card_look(page, tokens, amount),
        None => page,
    };
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
        .offset_x(offset)
        .opacity(opacity)
        .child(page)
}

/// The window buttons, drawn over a moving sheet. The sheet covers the window
/// header as it moves, but the buttons belong to the window, so they stay at
/// the top right above the sheet until the slide settles.
fn lifted_controls(tokens: &Tokens) -> Rect {
    rect()
        .position(Position::new_absolute().top(0.0).right(0.0))
        .layer(Layer::Relative(SLIDE_CONTROLS_LAYER))
        .tw(
            tokens,
            &[flex_row(), items(Align::Center), h(Dim::Header), px(S2)],
        )
        .child(WindowControls)
}

/// The header strip a sheet carries while it moves on a desktop window: its
/// Back button. The window buttons are not on it; they are lifted above the
/// sheet instead (see `lifted_controls`).
fn sheet_header(back: bool) -> Element {
    let header = HeaderBar::new(String::new()).fill(Paint::Bg);
    let header = if back {
        header.leading(BackButton)
    } else {
        header
    };
    Element::from(
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .layer(Layer::Relative(SLIDE_HEADER_LAYER))
            .child(header),
    )
}

/// What the split view remembers between renders to run page motion.
#[derive(Default)]
struct ViewMemory {
    /// The router's journal sequence this view has drawn.
    seen: u64,
    /// The page last drawn (desktop), kept so it can slide away.
    element: Option<Element>,
    /// The header strip the page on screen carries, when it is a sheet.
    element_header: Option<Element>,
    /// The whole screen last drawn on a compact window: header and page.
    screen: Option<Element>,
    pending: bool,
    duration: u64,
    /// The animation run count when the queued slide was set up.
    start_runs: Option<usize>,
    active: Option<Slide>,
    /// The moving page's offset in the last frame drawn, when that frame was a slide.
    shown: Option<(Motion, f32)>,
    /// Each page's scroll, kept while its entry is on the stack.
    scrolls: HashMap<u64, ScrollController>,
    /// The screen under a back swipe, built once when the swipe starts.
    preview: Option<(u64, Element)>,
    /// How far a back swipe had gone when it committed, and how fast the
    /// finger was going, for the pop's slide.
    commit: Option<(f32, f32)>,
}

impl ViewMemory {
    /// The scroll of entry `id`, made at the top when the page has none.
    fn scroll_for(&mut self, id: u64) -> ScrollController {
        *self
            .scrolls
            .entry(id)
            .or_insert_with(|| ScrollController::new(0, 0))
    }

    /// The change the router recorded since the last render, if any. Scroll
    /// controllers of entries that left every stack are dropped, except the
    /// page that is leaving, which holds still while it slides.
    fn enter(&mut self, journal: &crate::nav::router::Journal, model: &NavModel) -> Option<Change> {
        if journal.seq == self.seen {
            return None;
        }
        self.seen = journal.seq;
        let change = journal.change.clone()?;
        let alive: Vec<u64> = model
            .back_entries()
            .iter()
            .chain(model.forward_entries())
            .map(|entry| entry.id)
            .chain([model.current().id, change.from.id])
            .collect();
        self.scrolls.retain(|id, _| alive.contains(id));
        if let Some((id, _)) = &self.preview
            && !alive.contains(id)
        {
            self.preview = None;
        }
        Some(change)
    }
}

/// The sidebar's destinations: the root routes of the table.
#[derive(Clone, PartialEq)]
struct RootItem {
    route: usize,
    title: String,
    icon: Icon,
    group: &'static str,
    keywords: &'static str,
    location: Location,
}

fn root_items(table: &RouteTable) -> Vec<RootItem> {
    table
        .sidebar()
        .filter_map(|(route, def)| {
            Some(RootItem {
                route,
                title: table.title_of(&table.pattern_location(route)?),
                icon: def.icon.unwrap_or(Icon::Folder),
                group: def.group,
                keywords: def.keywords,
                location: table.pattern_location(route)?,
            })
        })
        .collect()
}

/// Rasterises the icons the window shows before any page does, in the colours
/// they are drawn in: the sidebar rows, the search field, the window controls
/// and the Back and Forward buttons.
fn warm_chrome(tokens: &Tokens, sidebar: &[Icon]) {
    let text = Paint::Text.color(tokens);
    let muted = Paint::Muted.color(tokens);
    for &icon in sidebar {
        icons::warm(icon, IconSize::Row, text);
        icons::warm(icon, IconSize::Row, muted);
    }
    icons::warm(Icon::Search, IconSize::Row, muted);
    for icon in [
        Icon::WindowMinimize,
        Icon::WindowMaximize,
        Icon::WindowRestore,
        Icon::WindowClose,
        Icon::GoPrevious,
        Icon::GoNext,
    ] {
        icons::warm(icon, IconSize::Control, text);
    }
    icons::warm(Icon::WindowClose, IconSize::Control, Paint::OnDanger.color(tokens));
}

/// A compact screen: its header over its content.
fn compact_screen(tokens: &Tokens, header: Element, content: Element) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .tw(tokens, &[flex_col(), bg(Paint::Bg)])
        .child(header)
        .child(content)
        .into()
}

/// A screen that predictive back is lifting: scaled, shifted toward the
/// finger, rounded and shadowed, over the screen beneath it.
fn lifted_screen(tokens: &Tokens, screen: Element, pose: &Pose, width: f32) -> Rect {
    let sheet = rect()
        .width(Size::fill())
        .height(Size::fill())
        .layer(Layer::Relative(SLIDE_SHEET_LAYER))
        .tw(
            tokens,
            &[
                bg(Paint::Bg),
                rounded_scaled(Radius::Window, pose.current_round),
                lift_shadow(pose.current_shadow),
            ],
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .tw(
                    tokens,
                    &[clip(), rounded_scaled(Radius::Window, pose.current_round)],
                )
                .child(screen),
        );
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
        .offset_x(pose.current_dx * width)
        .scale(pose.current_scale)
        .opacity(pose.current_opacity)
        .child(sheet)
}

/// The screen beneath a back gesture: grown from behind as the other leaves.
fn receding_screen(screen: Element, pose: &Pose) -> Rect {
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
        .scale(pose.previous_scale)
        .child(screen)
}

/// The black veil over the beneath screen, fading as it is revealed: the
/// opaque backdrop fill at the veil's strength.
fn back_veil(tokens: &Tokens, pose: &Pose) -> Rect {
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
        .layer(Layer::Relative(BACK_VEIL_LAYER))
        .tw(tokens, &[bg(Paint::Backdrop)])
        .opacity(BACK_VEIL_OPACITY * pose.previous_dim)
}

/// How dark the veil over the previous screen is at the start of a drag.
const BACK_VEIL_OPACITY: f32 = 0.12;

/// A clear layer over the window while a back swipe runs: it takes the touch,
/// so the control under the finger is not pressed when the swipe lifts.
fn back_shield() -> Rect {
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
        .layer(Layer::Relative(BACK_SHIELD_LAYER))
        .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
        .on_press(|event: Event<PressEventData>| event.stop_propagation())
        .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
}

/// The page column: centred, at most `page_width` wide, with the page margins.
/// A compact page ends with the bottom safe area (`Dim::SafeBottom`, nothing
/// on a desktop window), so its last row clears the window edge and the home
/// indicator.
fn page_column(content: Element, side: Space, page_width: Dim, inset: bool) -> impl IntoElement {
    rect()
        .component("Page")
        .tw_layout(&[w_full(), items(Align::Center), pt(S4), pb(S8), px(side)])
        // Under a floating header bar the page starts below it at rest.
        .maybe_child(inset.then(|| rect().tw_layout(&[h(Dim::Header)])))
        .child(
            rect()
                .component("PageColumn")
                .tw_layout(&[w(page_width)])
                .max_width(Size::fill())
                .child(content),
        )
        .child(rect().tw_layout(&[h(Dim::SafeBottom)]))
}

/// The title row at the top of a page: the page name large, the page's status
/// line under it when it sets one, and the page's action at the right. On a
/// compact page `large` is the scroll progress from 0 to 1: the row starts
/// lower, and the name fades as the page scrolls into the header bar.
fn title_row(
    tokens: &Tokens,
    title: Option<String>,
    status: String,
    action: Option<Element>,
    large: Option<f32>,
) -> Element {
    let name = |title: String| -> Element {
        let label = PAGE_TITLE
            .label(tokens, title)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        match large {
            None => label.into(),
            Some(progress) => rect()
                .width(Size::fill())
                .opacity(1.0 - progress)
                .child(label)
                .into(),
        }
    };
    let head = rect()
        .component("PageHead")
        .published(|| {
            let call = match &title {
                Some(title) => Published::call("page").arg(Prop::text("title", title.clone())),
                None => Published::call("page"),
            };
            call.method(Prop::text("status", status.clone()))
        })
        .tw(tokens, PAGE_HEAD);
    // A compact page gives its large title more room above it.
    let head = if large.is_some() {
        head.tw_layout(&[pt(S8)])
    } else {
        head
    };
    head.child(
        rect()
            .tw(tokens, PAGE_HEAD_TEXT)
            .maybe_child(title.map(name))
            .maybe_child((!status.is_empty()).then(|| status_line(tokens, &status))),
    )
    .maybe_child(action)
    .into()
}

/// The status line under a page title, when the page sets one.
fn status_line(tokens: &Tokens, status: &str) -> Element {
    BODY.in_ink(Ink::Muted)
        .paragraph(tokens, status)
        .width(Size::fill())
        .into()
}

impl NavigationSplitView {
    pub fn new(app_title: impl Into<String>) -> Self {
        Self {
            app_title: app_title.into(),
            page_width: Dim::Page,
            header_trailing: None,
            profile: None,
            search: None,
        }
    }

    pub fn page_width(mut self, width: Dim) -> Self {
        self.page_width = width;
        self
    }

    /// An element at the right of the header bar, before the window buttons:
    /// a tool that belongs to the window rather than to one page (the
    /// taxonomy's display switcher, a view menu).
    pub fn header_trailing(mut self, element: impl Into<Element>) -> Self {
        self.header_trailing = Some(element.into());
        self
    }

    /// The signed-in person, shown at the top of the sidebar. Pressing it
    /// switches to the root at `profile.target`.
    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Search the app. While `query` has text, `results` replaces the page
    /// content and a narrow window shows them instead of the sidebar list.
    /// Enter in the search field calls `on_submit` (open the best match).
    /// The caller owns `query`.
    pub fn search(
        mut self,
        query: State<String>,
        results: Element,
        on_submit: impl Into<EventHandler<()>>,
    ) -> Self {
        self.search = Some((query, results, on_submit.into()));
        self
    }
}

impl Component for NavigationSplitView {
    fn render(&self) -> impl IntoElement {
        let direction = crate::controls::direction::use_direction();
        let tokens = use_tokens();
        let router = use_router();
        let table = router.table();
        let fallback_query = use_state(String::new);
        let mut query = self
            .search
            .as_ref()
            .map_or(fallback_query, |(query, _, _)| *query);
        let search_id = use_a11y();
        let search_focus = use_focus(search_id);
        let busy_pages = use_provide_context(|| BusyPages(State::create(Vec::new())));
        let section_registry = sections::provide();
        let busy = !busy_pages.0.read().is_empty();
        // On a narrow window the index, the list of pages, is the screen.
        let width = Platform::get().current_window().root_size.read().width;
        let narrow = width < NARROW_BELOW || device_preview();
        use_side_effect_with_deps(&narrow, move |narrow| router.set_wide(!*narrow));
        let mut model = router.read();
        model.set_wide(!narrow);

        let side = if narrow { S4 } else { S8 };
        // A narrow window caps the column at the phone width, so cards stop
        // growing before a label and its value drift apart.
        let page_width = if narrow && self.page_width == Dim::Page {
            Dim::PagePhone
        } else {
            self.page_width
        };
        let header_region = use_region(RegionKind::Header);
        let content_region = use_region(RegionKind::Content);

        // The page on screen: the current entry. On the index a wide window
        // shows the first root and a narrow one the list.
        let current = model.current().clone();
        let at_index = table.kind(&current.location) == RouteKind::Index;
        let list_key = narrow && at_index;
        let roots = root_items(&table);
        // The sidebar and header icons are drawn on the first frame: rasterise
        // them on a worker now, so that frame finds them in the icon cache
        // and a mounting row rasterises nothing.
        use_hook({
            let tokens = tokens.clone();
            let sidebar: Vec<Icon> = roots.iter().map(|root| root.icon).collect();
            move || warm_chrome(&tokens, &sidebar)
        });
        let selected_root = model.selected_root();
        let shown_location = match table.default_root().and_then(|root| table.pattern_location(root)) {
            Some(first) if at_index && !narrow => first,
            _ => current.location.clone(),
        };
        let title = if at_index && !narrow {
            table.title_of(&shown_location)
        } else {
            model.title(&current)
        };
        let can_back = model.can_go_back();
        let window_title = model.window_title(&self.app_title);
        use_side_effect_with_deps(&window_title, |title| {
            let title = title.clone();
            crate::shell::window::with_window(move |window| window.set_title(&title));
        });

        // Each page owns its scroll controller, so a page reached by Back is
        // still where it was, and a page reached any other way starts at the
        // top. The page being replaced stays in `memory` while a motion runs.
        let memory = use_hook(|| Rc::new(RefCell::new(ViewMemory::default())));
        let journal = router.journal();
        let entered = memory.borrow_mut().enter(&journal, &model);
        let scroll = memory.borrow_mut().scroll_for(current.id);

        // The pages with results, when the split view searches.
        let search_hits = self
            .search
            .as_ref()
            .map(|_| sections::hits(&section_registry.0.read()));
        let sidebar = |full_page: bool, results: Option<Element>| Sidebar {
            roots: roots.clone(),
            selected: selected_root,
            query,
            search_id,
            full_page,
            profile: self.profile.clone(),
            hits: search_hits.clone(),
            results,
            scroll,
        };
        let results = self
            .search
            .as_ref()
            .map(|(_, results, on_submit)| (results.clone(), on_submit.clone()));
        let searching = results.is_some() && !query.read().trim().is_empty();
        let submit = results.as_ref().map(|(_, on_submit)| on_submit.clone());
        // A desktop header floats over the page and blurs what scrolls under it.
        let overlay = !narrow;
        // Results replace the page content while the search has text. Results
        // carry their own page head, as a page does.
        let shown: Element = match results {
            Some((results, _)) if searching => rect()
                .tw(&tokens, PAGE_BODY)
                .child(PageHead::new())
                .child(results)
                .into(),
            _ => table.render(&shown_location),
        };
        // The page as the split view draws it, framed with its name. The page
        // head reads that name, and a page sliding out keeps its own frame.
        let body: Element = PageFrame {
            entry: current.id,
            title: if searching { "Search results".to_string() } else { title.clone() },
            content: shown,
            scroll,
            compact: narrow,
        }
        .into();

        // Page changes move the content. A subpage slides in from the right, a
        // pop slides it out to the right (a compact pop is predictive back),
        // and a sidebar switch cross-fades.
        let mut generation = use_state(|| 0u64);
        let anim_memory = memory.clone();
        let anim = use_animation_transition(generation(), move |_, _| {
            AnimNum::new(0.0, 1.0)
                .time(anim_memory.borrow().duration)
                .function(Function::Linear)
                .ease(Ease::In)
        });
        let runs = anim.runs();
        let effect_memory = memory.clone();
        // The slide starts after the render that queued it.
        use_side_effect_with_deps(&journal.seq, move |_| {
            if effect_memory.borrow().pending {
                effect_memory.borrow_mut().pending = false;
                *generation.write() += 1;
            }
        });

        // The back swipe.
        let gesture = use_hook(|| Rc::new(RefCell::new(BackGesture::default())));
        let clock = use_hook(std::time::Instant::now);
        let mut redraw = use_state(|| 0u32);
        let _ = redraw();

        let mut changed = false;
        {
            let mut memory = memory.borrow_mut();
            if let Some(change) = entered {
                changed = true;
                let motion = motion_for(change.transition, narrow)
                    .filter(|motion| tokens.motion(motion.ms()) > 0);
                let shown = memory.shown.take();
                let outgoing_scroll = memory.scroll_for(change.from.id);
                let started = *runs.peek();
                memory.active = match (motion, memory.element.clone()) {
                    (Some(motion), Some(outgoing)) => {
                        memory.duration = tokens.motion(motion.ms());
                        memory.pending = true;
                        memory.start_runs = Some(started);
                        let mut slide = Slide::start(
                            motion,
                            outgoing,
                            memory.element_header.clone(),
                            shown,
                            outgoing_scroll,
                        );
                        if motion == Motion::Predictive {
                            (slide.d0, slide.speed) = memory.commit.unwrap_or((0.0, 0.0));
                        }
                        slide.outgoing_screen = memory.screen.clone();
                        Some(slide)
                    }
                    _ => {
                        memory.start_runs = None;
                        None
                    }
                };
                memory.commit = None;
            }
        }
        // The animation restarts after this render, so until its run count
        // moves the last run's value is stale and the slide holds at its start.
        let waiting = !changed
            && memory
                .borrow()
                .start_runs
                .is_some_and(|start| *runs.peek() <= start);
        // Read the value on every render, even when it is not used this frame:
        // the read is what subscribes this component to the animation's ticks.
        // Without it a slide that restarts while waiting never re-renders.
        let value = anim.get().value().clamp(0.0, 1.0);
        // How far into the move's time the slide is, 0 to 1. The curve each
        // move follows is `Slide::eased`.
        let time = if changed || waiting { 0.0 } else { value };
        let active = {
            let mut memory = memory.borrow_mut();
            if !changed && time >= 1.0 {
                memory.active = None;
                memory.shown = None;
            }
            memory.active.clone()
        };

        // The gesture's own clock: a cancel springs back, and a commit has
        // finished once the pop's slide has.
        let now = clock.elapsed();
        {
            let mut gesture = gesture.borrow_mut();
            if matches!(gesture.phase(), crate::nav::gesture::Phase::Settling { commit: true, .. })
                && active.is_none()
            {
                gesture.finish();
            }
            gesture.tick(now);
        }
        let (drag_phase, drag_value) = {
            let gesture = gesture.borrow();
            (gesture.phase(), gesture.value(now))
        };
        let swiping = narrow
            && active.is_none()
            && model.has_previous()
            && model.modals().is_empty()
            && matches!(
                drag_phase,
                crate::nav::gesture::Phase::Dragging
                    | crate::nav::gesture::Phase::Settling { commit: false, .. }
            );
        let reduced = tokens.motion(SLIDE_MS) == 0;
        crate::task::set_sliding(active.is_some() || swiping);

        // ---- the desktop page ------------------------------------------------
        let page_scroll = |scroll: ScrollController, content: Element, origin: bool, inset: bool| {
            ScrollView::new_controlled(scroll)
                .scrollbar_inset_top(if inset { Dim::Header.px() } else { 0.0 })
                .scrollbar_inset_bottom(Radius::Window.px() + 3.0)
                .drag_scrolling(false)
                .child(
                    rect()
                        .width(Size::fill())
                        .maybe(origin, |wrapper| {
                            wrapper.on_sized(sections::origin_handler(section_registry))
                        })
                        .child(page_column(content, side, page_width, inset)),
                )
        };

        let (page_offset, page_opacity, page_sheet, page_header, outgoing) = match active.clone() {
            Some(slide) if !narrow => {
                let x = slide.at(time);
                let moved = x * width;
                let progress = slide.eased(time);
                // The sheet is rounded and shadowed while it moves, and flat at rest.
                let card = card_amount(x);
                match slide.motion {
                    Motion::Fade | Motion::Predictive => (
                        0.0,
                        progress,
                        None,
                        None,
                        Some((0.0, 1.0 - progress, slide.outgoing, None, None, slide.scroll)),
                    ),
                    Motion::Push => {
                        memory.borrow_mut().shown = Some((slide.motion, x));
                        (
                            moved,
                            1.0,
                            Some(card),
                            overlay.then(|| sheet_header(can_back)),
                            Some((0.0, 1.0, slide.outgoing, None, None, slide.scroll)),
                        )
                    }
                    Motion::Pop => {
                        memory.borrow_mut().shown = Some((slide.motion, x));
                        (
                            0.0,
                            1.0,
                            None,
                            None,
                            Some((
                                moved,
                                1.0,
                                slide.outgoing,
                                Some(card),
                                slide.outgoing_header,
                                slide.scroll,
                            )),
                        )
                    }
                }
            }
            _ => (0.0, 1.0, None, None, None),
        };
        let wide_page = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::fill())
            .child(page_scroll(scroll, body.clone(), true, overlay))
            .maybe_child(page_header);
        let wide_page = slide_frame(page_offset, page_opacity, page_sheet, &tokens, wide_page);
        let wide_outgoing = outgoing.map(|(offset, opacity, element, sheet, header, scroll)| {
            let outgoing_page = rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .height(Size::fill())
                .child(page_scroll(scroll, element, false, overlay))
                .maybe_child(header);
            slide_frame(offset, opacity, sheet, &tokens, outgoing_page)
        });
        let edge_top = if overlay { Dim::Header.px() } else { 0.0 };
        let wide_content = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .on_press({
                let press = content_region.clone();
                move |event: Event<PressEventData>| press.press(&event)
            })
            .child(content_region.sentinel())
            .child(wide_page)
            .maybe_child(wide_outgoing)
            .child(ScrollEdge {
                scroll,
                top: edge_top,
            });

        // A page that slides in over a desktop header brings its own Back on the
        // sheet. The header's Back waits until the slide settles, so one chevron
        // shows at a time and it does not appear before the page arrives.
        let pushing = overlay
            && matches!(active.as_ref(), Some(slide) if matches!(slide.motion, Motion::Push));
        let show_back = can_back && !pushing;
        // A phone has no window buttons. A desktop window dragged narrow keeps
        // them, so a mouse user can still close it: the buttons go only while
        // a finger drives a narrow window.
        let show_controls = !(narrow && pointer_kind() == PointerKind::Touch);
        // While a sheet moves over the header, the window buttons are lifted
        // above it, so the header keeps none and the buttons stay put.
        let lifted = overlay
            && show_controls
            && matches!(active.as_ref(), Some(slide) if !matches!(slide.motion, Motion::Fade));

        // ---- the compact screens ----------------------------------------------
        let trailing = self.header_trailing.clone();
        let page_header_bar = |back: bool, busy: bool, title_slot: Option<Element>, controls: bool| {
            let header = HeaderBar::new(String::new()).busy(busy).fill(Paint::Bg);
            let header = if back { header.leading(BackButton) } else { header };
            let header = match &trailing {
                Some(trailing) => header.trailing(trailing.clone()),
                None => header,
            };
            let header = if controls { header.trailing(WindowControls) } else { header };
            match title_slot {
                Some(slot) => header.title_slot(slot),
                None => header,
            }
        };
        // One page's area under its header: the page and the hairline.
        let compact_area = |scroll: ScrollController, content: Element, current: bool| -> Element {
            let area = rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .maybe(current, |area| {
                    area.on_press({
                        let press = content_region.clone();
                        move |event: Event<PressEventData>| press.press(&event)
                    })
                    .child(content_region.sentinel())
                })
                .child(
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(page_scroll(scroll, content, current, false)),
                )
                .child(ScrollEdge { scroll, top: 0.0 });
            area.into()
        };
        // The screen of an entry other than the current one: what predictive
        // back reveals beneath the swipe.
        let preview_of = |entry: &Entry, back_below: bool| -> Element {
            if table.kind(&entry.location) == RouteKind::Index {
                let header = HeaderBar::new(self.app_title.clone()).fill(Paint::Bg);
                return compact_screen(&tokens, header.into(), sidebar(true, None).into());
            }
            let entry_scroll = memory.borrow_mut().scroll_for(entry.id);
            let entry_title = model.title(entry);
            let content: Element = PageFrame {
                entry: entry.id,
                title: entry_title.clone(),
                content: table.render(&entry.location),
                scroll: entry_scroll,
                compact: true,
            }
            .into();
            let header = page_header_bar(
                back_below,
                false,
                Some(
                    ScrolledTitle {
                        scroll: entry_scroll,
                        title: entry_title,
                    }
                    .into(),
                ),
                false,
            );
            compact_screen(&tokens, header.into(), compact_area(entry_scroll, content, false))
        };

        let current_screen: Option<Element> = narrow.then(|| {
            if list_key {
                let header = HeaderBar::new(self.app_title.clone()).fill(Paint::Bg);
                let header = match &trailing {
                    Some(trailing) => header.trailing(trailing.clone()),
                    None => header,
                };
                let header = if show_controls { header.trailing(WindowControls) } else { header };
                let list: Element = if searching {
                    sidebar(true, Some(compact_area(scroll, body.clone(), true))).into()
                } else {
                    sidebar(true, None).into()
                };
                compact_screen(&tokens, header.into(), list)
            } else {
                let header = page_header_bar(
                    show_back,
                    busy,
                    Some(
                        ScrolledTitle {
                            scroll,
                            title: if searching { "Search results".to_string() } else { title.clone() },
                        }
                        .into(),
                    ),
                    show_controls,
                );
                compact_screen(&tokens, header.into(), compact_area(scroll, body.clone(), true))
            }
        });

        // The stage: what the compact window draws under the window edge.
        let stage: Option<Element> = current_screen.clone().map(|screen| {
            let at_rest = |screen: Element| -> Rect { slide_frame(0.0, 1.0, None, &tokens, rect().width(Size::fill()).height(Size::fill()).child(screen)) };
            let stage = rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .tw(&tokens, &[clip(), bg(Paint::Bg)]);
            let width = width.max(1.0);
            let pose_of = |d: f32, k: f32| {
                if reduced { Pose::cross_fade(d, k) } else if k > 0.0 { Pose::committing(d, k) } else { Pose::at(d, 0.0) }
            };
            let built: Rect = match &active {
                Some(slide) if slide.motion == Motion::Predictive => {
                    let pose = pose_of(slide.d0, slide.eased(time));
                    let outgoing = slide
                        .outgoing_screen
                        .clone()
                        .unwrap_or_else(|| rect().into());
                    stage
                        .child(back_veil(&tokens, &pose))
                        .child(receding_screen(screen, &pose))
                        .child(lifted_screen(&tokens, outgoing, &pose, width))
                }
                Some(slide) if slide.motion == Motion::Push => {
                    let x = slide.at(time);
                    let moved = x * width;
                    let card = card_amount(x);
                    memory.borrow_mut().shown = Some((slide.motion, x));
                    let outgoing = slide.outgoing_screen.clone().unwrap_or_else(|| rect().into());
                    stage.child(at_rest(outgoing)).child(slide_frame(
                        moved,
                        1.0,
                        Some(card),
                        &tokens,
                        rect().width(Size::fill()).height(Size::fill()).child(screen),
                    ))
                }
                Some(slide) => {
                    let outgoing = slide.outgoing_screen.clone().unwrap_or_else(|| rect().into());
                    stage
                        .child(slide_frame(0.0, 1.0 - slide.eased(time), None, &tokens, rect().width(Size::fill()).height(Size::fill()).child(outgoing)))
                        .child(slide_frame(0.0, slide.eased(time), None, &tokens, rect().width(Size::fill()).height(Size::fill()).child(screen)))
                }
                None if swiping => {
                    let previous = model.previous().cloned();
                    let back_below = model.visible_back().len() >= 2;
                    match previous {
                        Some(previous) => {
                            let cached = memory
                                .borrow()
                                .preview
                                .as_ref()
                                .filter(|(id, _)| *id == previous.id)
                                .map(|(_, element)| element.clone());
                            let preview = cached.unwrap_or_else(|| {
                                let element = preview_of(&previous, back_below);
                                memory.borrow_mut().preview = Some((previous.id, element.clone()));
                                element
                            });
                            let pose = if reduced { Pose::cross_fade(drag_value, 0.0) } else { Pose::at(drag_value, 0.0) };
                            stage
                                .child(back_veil(&tokens, &pose))
                                .child(receding_screen(preview, &pose))
                                .child(lifted_screen(&tokens, screen, &pose, width))
                                .child(back_shield())
                        }
                        None => stage.child(at_rest(screen)),
                    }
                }
                None => stage.child(at_rest(screen)),
            };
            built.into()
        });
        if narrow {
            memory.borrow_mut().screen = current_screen.clone();
        }
        {
            let mut memory = memory.borrow_mut();
            memory.element = Some(body.clone());
            memory.element_header = overlay.then(|| sheet_header(can_back));
        }

        // ---- the window --------------------------------------------------------
        let header = HeaderBar::new(String::new())
            .busy(busy)
            .fill(if overlay { Paint::Frosted } else { Paint::Bg });
        let header = if show_back { header.leading(BackButton) } else { header };
        let header = match &self.header_trailing {
            Some(trailing) => header.trailing(trailing.clone()),
            None => header,
        };
        let header = if show_controls && !lifted {
            header.trailing(WindowControls)
        } else {
            header
        };
        let header = header.title_slot(crossfade(PageTitle {
            entry: current.id,
            scroll,
            title: if searching { "Search results".to_string() } else { title.clone() },
        }));

        let back_router = router;
        let back_gesture = gesture.clone();
        let back_memory = memory.clone();
        let move_gesture = gesture.clone();
        let up_gesture = gesture.clone();
        let up_memory = memory.clone();
        let settle_memory = memory.clone();
        let mut generation_tick = generation;
        let start_settle = move |ms: u64| {
            // Frames for a spring back: the animation only ticks the view, the
            // gesture's clock says where the screen is.
            settle_memory.borrow_mut().duration = ms;
            settle_memory.borrow_mut().start_runs = Some(*runs.peek());
            *generation_tick.write() += 1;
        };
        let mut up_settle = start_settle.clone();
        let mut move_settle = start_settle;
        let cancel_ms = gesture.borrow().config.cancel_ms;
        let reduced_cancel = tokens.motion(cancel_ms) == 0;
        let motion_cancel_ms = tokens.motion(cancel_ms);

        rect()
            .component("NavigationSplitView")
            .expanded()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    bg(if narrow { Paint::Bg } else { Paint::Sidebar }),
                ],
            )
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let control = event.modifiers == Modifiers::CONTROL;
                let letter = match &event.key {
                    Key::Character(text) if control => text.to_lowercase(),
                    _ => String::new(),
                };
                // Back is towards the start of the reading direction: Alt+Right in a window
                // that reads from the right.
                let towards = direction.reading_key(&event.key);
                if event.modifiers == Modifiers::ALT && towards == Key::Named(NamedKey::ArrowLeft) {
                    router.back();
                } else if event.modifiers == Modifiers::ALT && towards == Key::Named(NamedKey::ArrowRight) {
                    router.forward();
                } else if control && letter == "f" {
                    // Focus the search. A narrow window shows the list first.
                    if narrow {
                        router.switch_root(&router.table().home());
                    }
                    search_id.request_focus();
                } else if control && (letter == "q" || letter == "w") {
                    Platform::get().close_window(Platform::window_id());
                } else if event.key == Key::Named(NamedKey::Enter)
                    && searching
                    && search_focus.read().is_focused()
                    && let Some(submit) = &submit
                {
                    submit.call(());
                } else if event.key == Key::Named(NamedKey::Escape) && !query.read().is_empty() {
                    query.set(String::new());
                }
            })
            .on_global_pointer_down(move |event: Event<PointerEventData>| {
                if let PointerEventData::Mouse(mouse) = event.data() {
                    match mouse.button {
                        Some(MouseButton::Back) => return back_router.back(),
                        Some(MouseButton::Forward) => return back_router.forward(),
                        _ => {}
                    }
                }
                // An edge swipe from the left starts a back gesture on a phone
                // window, and for a finger on any window.
                if !event.data().is_primary() {
                    return;
                }
                let touch = matches!(event.data(), PointerEventData::Touch(_));
                if !(narrow || touch) {
                    return;
                }
                let at = event.global_location();
                let allowed = back_router.model().can_go_back() && back_memory.borrow().active.is_none();
                back_gesture.borrow_mut().down(
                    clock.elapsed(),
                    (swipe_x(at.x as f32, width, direction), at.y as f32),
                    width,
                    allowed,
                );
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let update = {
                    let mut gesture = move_gesture.borrow_mut();
                    if !gesture.is_pressed() {
                        return;
                    }
                    gesture.moved(clock.elapsed(), (swipe_x(at.x as f32, width, direction), at.y as f32))
                };
                match update {
                    Update::Started | Update::Progress(_) => *redraw.write() += 1,
                    Update::Cancelled => {
                        if reduced_cancel {
                            move_gesture.borrow_mut().finish();
                            *redraw.write() += 1;
                        } else {
                            move_settle(motion_cancel_ms);
                        }
                    }
                    Update::None | Update::Aborted => {}
                }
            })
            .on_global_pointer_up(move |_| {
                let release = up_gesture.borrow_mut().up(clock.elapsed());
                match release {
                    Release::Commit { from, speed } => {
                        up_memory.borrow_mut().commit = Some((from, speed));
                        let outcome = router.run(crate::nav::Op::Back, false);
                        if !matches!(outcome, crate::nav::Outcome::Changed(_)) {
                            // A modal closed, or the page vetoed leaving: nothing
                            // popped, so the screen springs back.
                            up_memory.borrow_mut().commit = None;
                            up_gesture.borrow_mut().abort();
                            *redraw.write() += 1;
                        }
                    }
                    Release::Cancel { .. } => {
                        if reduced_cancel {
                            up_gesture.borrow_mut().finish();
                            *redraw.write() += 1;
                        } else {
                            up_settle(motion_cancel_ms);
                        }
                    }
                    Release::Tap | Release::Ignored => {}
                }
            })
            .maybe_child((!narrow).then(|| sidebar(false, None)))
            .maybe_child((!narrow).then(|| rect().tw(&tokens, SPLIT_HAIRLINE)))
            .child(
                rect()
                    // Sizes follow the size class, which the root sets and
                    // which re-renders subscribers in place, so no remount.
                    .tw(&tokens, &[w_flex(), h_full(), bg(Paint::Bg)])
                    .on_press({
                        let press = header_region.clone();
                        move |event: Event<PressEventData>| press.press(&event)
                    })
                    .child(header_region.sentinel())
                    .maybe_child((!narrow).then(|| {
                        Element::from(
                            rect()
                                .position(Position::new_absolute().top(0.0).left(0.0))
                                .width(Size::fill())
                                .layer(Layer::Relative(HEADER_LAYER))
                                .backdrop_blur(HEADER_BLUR)
                                .child(header),
                        )
                    }))
                    .maybe_child((!narrow && lifted).then(|| lifted_controls(&tokens)))
                    .maybe_child((!narrow).then(|| Element::from(wide_content)))
                    .maybe_child(stage),
            )
    }
}


/// How much the content behind a floating header bar is blurred.
const HEADER_BLUR: f32 = 20.0;
/// The layer the floating header bar draws on. Layers add up with element
/// depth, so this sits above the deepest page content and below overlays.
const HEADER_LAYER: i16 = crate::layers::HEADER;

/// The scroll distance, in pixels, where the header title starts to appear.
/// Above it the title row at the top of the page is still fully visible.
const TITLE_FADE_FROM: f32 = 24.0;
/// The scroll distance where the header title is fully shown, and where the
/// large title has fully faded. The two never show together.
const TITLE_FADE_TO: f32 = 56.0;

/// The page's scroll distance in pixels, zero at the top and above.
fn scrolled_px(scroll: ScrollController) -> f32 {
    (-<(i32, i32)>::from(scroll).1).max(0) as f32
}

/// How far a compact page's large title has scrolled away, from 0 at the top
/// to 1 once it has fully faded.
fn large_title_progress(scroll: ScrollController) -> f32 {
    (scrolled_px(scroll) / TITLE_FADE_TO).clamp(0.0, 1.0)
}

/// The page title in the header bar, centred. It is a pure function of the
/// page's scroll offset: it fades in while it rises from half a line below
/// its place, as if half out of the bar, and is clipped to its line. Dragging
/// the page back up reverses it exactly, with no timed animation.
#[derive(PartialEq)]
struct ScrolledTitle {
    scroll: ScrollController,
    title: String,
}

impl Component for ScrolledTitle {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let progress = ((scrolled_px(self.scroll) - TITLE_FADE_FROM)
            / (TITLE_FADE_TO - TITLE_FADE_FROM))
            .clamp(0.0, 1.0);
        let line = EMPHASIS.size.px() * EMPHASIS.leading;
        let title = EMPHASIS
            .label(&tokens, self.title.clone())
            .width(Size::fill())
            .text_align(TextAlign::Center)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        // The line is reserved even before the title shows, so the bar does
        // not change height when the title starts to rise.
        rect()
            .tw_layout(&[w_full()])
            .tw(&tokens, &[clip()])
            .height(Size::px(line))
            .maybe_child((progress > 0.0).then(|| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(line))
                    .offset_y((1.0 - progress) * line / 2.0)
                    .opacity(progress)
                    .child(title)
            }))
    }
}

/// The page title in the header bar. Crossfade compares it by entry and
/// title, so the title fades when the page changes and not on every scroll.
#[derive(Clone, PartialEq)]
struct PageTitle {
    entry: u64,
    scroll: ScrollController,
    title: String,
}

impl Component for PageTitle {
    fn render(&self) -> impl IntoElement {
        ScrolledTitle {
            scroll: self.scroll,
            title: self.title.clone(),
        }
    }
}

#[derive(PartialEq)]
struct Sidebar {
    roots: Vec<RootItem>,
    /// The root route of the page on screen.
    selected: Option<usize>,
    query: State<String>,
    search_id: AccessibilityId,
    /// Drawn as the whole window below the header, not as a column.
    full_page: bool,
    profile: Option<Profile>,
    /// The pages with search results, when the split view searches. While
    /// the search has text, the sidebar lists these instead of its pages.
    hits: Option<Vec<SearchHit>>,
    /// The results screen, drawn in place of the list under the search field
    /// while a narrow window searches.
    results: Option<Element>,
    scroll: ScrollController,
}

impl Component for Sidebar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let router = use_router();
        let region = use_region(RegionKind::Sidebar);
        let selected = self.selected;
        let query = self.query;
        let needle = query.read().trim().to_lowercase();
        let typing = !needle.is_empty();
        let visible: Vec<RootItem> = self
            .roots
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || item.title.to_lowercase().contains(&needle)
                    || item
                        .keywords
                        .split_whitespace()
                        .any(|word| word.starts_with(&needle))
            })
            .cloned()
            .collect();
        // While searching, the list is the pages with results: the same set
        // the results pane shows, one block each.
        let hit_list = self.hits.as_ref().filter(|_| typing);
        let mut entries: Vec<Element> = Vec::new();
        let no_matches = typing && hit_list.map_or(visible.is_empty(), |hits| hits.is_empty());
        if let Some(hits) = hit_list {
            entries.extend(hits.iter().map(|hit| {
                Element::from(SearchHitRow {
                    hit: hit.clone(),
                    scroll: self.scroll,
                })
            }));
        }
        let mut last_group = "";
        for item in visible.iter().filter(|_| hit_list.is_none()) {
            let group = item.group;
            if group != last_group && !group.is_empty() {
                entries.push(group_label(&tokens, group));
            }
            last_group = group;
            entries.push(Element::from(NavEntry {
                item: item.clone(),
                siblings: visible.clone(),
                selected,
            }));
        }

        let shell = if self.full_page {
            rect()
                .component("Sidebar")
                .width(Size::fill())
                .height(Size::flex(1.0))
                .tw(&tokens, &[flex_col(), bg(Paint::Bg)])
        } else {
            rect().tw(&tokens, SIDEBAR).component("Sidebar")
        };
        let table = router.table();
        let card = self
            .profile
            .clone()
            .filter(|_| !typing && self.results.is_none())
            .map(|profile| {
                let on_profile = table
                    .root_of(&profile.target)
                    .is_some_and(|root| Some(root) == selected);
                Element::from(ProfileCard {
                    profile,
                    selected: on_profile,
                })
            });
        let list = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .min_height(Size::px(0.0))
            .child(
                ScrollView::new().drag_scrolling(false).child(
                    rect()
                        .tw(&tokens, SIDEBAR_LIST)
                        .tw(&tokens, &[pt(S2)])
                        .children(entries)
                        .maybe_child(no_matches.then(|| {
                            rect()
                                .tw(&tokens, &[pt(S3), px(S3)])
                                .child(BODY.in_ink(Ink::Muted).label(&tokens, "Nothing matches."))
                        })),
                ),
            );
        shell
            .on_press({
                let press = region.clone();
                move |event: Event<PressEventData>| press.press(&event)
            })
            .child(search_band(query, self.search_id))
            .child(region.sentinel())
            .maybe_child(card)
            .child(self.results.clone().unwrap_or_else(|| list.into()))
    }
}

#[derive(PartialEq)]
struct NavEntry {
    item: RootItem,
    /// The items currently listed, for Up and Down.
    siblings: Vec<RootItem>,
    selected: Option<usize>,
}

impl Component for NavEntry {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let item = self.item.clone();
        let siblings = self.siblings.clone();
        let selected = self.selected;
        let target = item.location.clone();
        Element::from(SidebarRow {
            mark: SidebarMark::Icon(item.icon),
            label: item.title.clone(),
            selected: selected == Some(item.route),
            on_press: (move |_: ()| router.switch_root(&target)).into(),
            on_key: Some(
                (move |event: Event<KeyboardEventData>| {
                    // Walk from the selected item, the page on screen.
                    let position = siblings.iter().position(|other| Some(other.route) == selected);
                    let target = match &event.key {
                        Key::Named(NamedKey::ArrowDown) => {
                            position.and_then(|index| siblings.get(index + 1))
                        }
                        Key::Named(NamedKey::ArrowUp) => position
                            .and_then(|index| index.checked_sub(1))
                            .and_then(|index| siblings.get(index)),
                        Key::Named(NamedKey::Home) => siblings.first(),
                        Key::Named(NamedKey::End) => siblings.last(),
                        _ => None,
                    };
                    if let Some(target) = target {
                        event.stop_propagation();
                        router.switch_root(&target.location);
                    }
                })
                .into(),
            ),
        })
    }
}

/// A hairline under the header bar while the page is scrolled.
#[derive(PartialEq)]
struct ScrollEdge {
    scroll: ScrollController,
    top: f32,
}

impl Component for ScrollEdge {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scrolled = <(i32, i32)>::from(self.scroll).1 < 0;
        rect()
            .position(Position::new_absolute().top(self.top).left(0.0))
            .layer(Layer::Relative(2))
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::Hairline),
                    bg(if scrolled {
                        Paint::Separator
                    } else {
                        Paint::Transparent
                    }),
                ],
            )
    }
}

/// The name of a group of pages, in small muted type above its first page.
fn group_label(tokens: &Tokens, group: &str) -> Element {
    let mut name = group.to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    rect()
        .tw_layout(NAV_LABEL)
        .child(CAPTION.label(tokens, name))
        .into()
}

/// A page with search results, listed in the sidebar while searching.
/// Pressing it scrolls the results to the page's block.
#[derive(Clone, PartialEq)]
struct SearchHitRow {
    hit: SearchHit,
    scroll: ScrollController,
}

impl Component for SearchHitRow {
    fn render(&self) -> impl IntoElement {
        let mut scroll = self.scroll;
        let offset = self.hit.offset;
        Element::from(SidebarRow {
            mark: SidebarMark::Icon(self.hit.icon),
            label: self.hit.title.clone(),
            selected: false,
            on_press: (move |_: ()| {
                scroll.scroll_to_y((section_top() - offset as f32).round() as i32);
            })
            .into(),
            on_key: None,
        })
    }
}

/// The scroll of the page the caller is on, when a split view draws it.
pub(crate) fn use_page_scroll() -> Option<ScrollController> {
    use_try_consume::<PageScroll>().map(|page| page.scroll)
}

/// Where a search hit's block top sits when it is opened: just under the frosted header.
pub(crate) fn section_top() -> f32 {
    Dim::Header.px() + S2.px()
}

/// The top band of the sidebar. It is the header height, like the page
/// header, so the search field sits on the window-control line.
fn search_band(query: State<String>, search_id: AccessibilityId) -> impl IntoElement {
    rect().tw_layout(SIDEBAR_SEARCH).child(
        search_field(query, |_: String| {})
            .a11y_id(search_id)
            .well()
            .fill_width(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Scheme;

    const SLIDES: [Motion; 3] = [Motion::Push, Motion::Pop, Motion::Predictive];

    #[test]
    fn opening_and_closing_take_one_duration() {
        for motion in SLIDES {
            assert_eq!(motion.ms(), SLIDE_MS, "{motion:?}");
        }
        // A cross-fade between pages is its own, shorter move.
        assert!(Motion::Fade.ms() < SLIDE_MS);
    }

    #[test]
    fn every_slide_is_instant_under_reduced_motion() {
        let reduced = Tokens::new("blue", Scheme::PreferDark, true);
        let normal = Tokens::new("blue", Scheme::PreferDark, false);
        for motion in SLIDES.into_iter().chain([Motion::Fade]) {
            assert_eq!(reduced.motion(motion.ms()), 0, "{motion:?}");
            assert_eq!(normal.motion(motion.ms()), motion.ms(), "{motion:?}");
        }
    }

    /// Where the page that moves is, as a fraction of the width, `time` into
    /// its move: an arriving page starts at the right edge, a leaving page at
    /// its place.
    fn page_at(motion: Motion, time: f32) -> f32 {
        let start = if motion == Motion::Push { 1.0 } else { 0.0 };
        offset(motion, start, curve(motion, None, time))
    }

    #[test]
    fn a_closing_page_retraces_the_path_of_an_opening_one() {
        for step in 0..=320 {
            let time = step as f32 / 320.0;
            let opening = page_at(Motion::Push, 1.0 - time);
            let closing = page_at(Motion::Pop, time);
            assert!((opening - closing).abs() < 1e-5, "{step} ms: {opening} against {closing}");
        }
        assert_eq!(page_at(Motion::Push, 0.0), 1.0);
        assert_eq!(page_at(Motion::Push, 1.0), 0.0);
        assert_eq!(page_at(Motion::Pop, 0.0), 0.0);
        assert_eq!(page_at(Motion::Pop, 1.0), 1.0);
    }

    #[test]
    fn an_open_and_a_close_cross_every_milestone_at_mirrored_times() {
        // How long the page is within `share` of the width of its place: the
        // open reaches it as late as the close leaves it.
        let within = |motion: Motion, share: f32| {
            (0..=3200)
                .filter(|step| page_at(motion, *step as f32 / 3200.0) <= share)
                .count() as f32
                / 3200.0
        };
        for share in [0.05, 0.25, 0.5, 0.9] {
            let open_in = within(Motion::Push, share);
            let close_in = within(Motion::Pop, share);
            assert!((open_in - close_in).abs() < 1e-3, "{share}: {open_in} against {close_in}");
        }
    }

    #[test]
    fn the_card_look_follows_where_the_page_is() {
        // The same place gives the same look, arriving or leaving.
        assert_eq!(card_amount(0.0), 0.0);
        assert_eq!(card_amount(0.15), 1.0);
        assert_eq!(card_amount(1.0), 1.0);
        for step in 0..=100 {
            let time = step as f32 / 100.0;
            let opening = card_amount(page_at(Motion::Push, 1.0 - time));
            let closing = card_amount(page_at(Motion::Pop, time));
            assert!((opening - closing).abs() < 1e-5);
        }
    }

    #[test]
    fn a_swipe_released_past_its_threshold_carries_on_from_the_finger_and_a_press_is_a_slide() {
        for step in 0..=100 {
            let time = step as f32 / 100.0;
            // A press, and a swipe let go at rest, are the same curve as every slide.
            assert_eq!(curve(Motion::Predictive, None, time), slide(time));
            assert_eq!(curve(Motion::Predictive, None, time), curve(Motion::Pop, None, time));
            assert_eq!(curve(Motion::Push, None, time), slide(time));
        }
        // A swipe let go going faster starts faster, and every one ends at 1.
        assert_eq!(swipe_slope(0.0, 2.5), None, "not a swipe");
        assert_eq!(swipe_slope(0.5, 0.0), Some(0.0), "let go at rest");
        // 2.5 widths a second with 0.5 of the way left: the drag pose
        // finishes over the first 40 % of the commit, so the commit starts at
        // 0.4 * 2.5 * 0.32 / 0.5 times its average speed.
        let slope = swipe_slope(0.5, 2.5).unwrap();
        assert!((slope - DRAG_SHARE * 2.5 * 0.32 / 0.5).abs() < 1e-6, "{slope}");
        // The drag then moves at 2.5 widths a second as the finger did: no stall.
        let dt = 0.001;
        let k_speed = curve(Motion::Predictive, Some(slope), dt / 0.32) / dt;
        let d_speed = 0.5 * k_speed / DRAG_SHARE;
        assert!((d_speed - 2.5).abs() < 0.05, "{d_speed}");
        assert_eq!(curve(Motion::Predictive, Some(slope), 0.0), 0.0);
        assert!((curve(Motion::Predictive, Some(slope), 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_move_that_interrupts_another_continues_from_where_the_page_is() {
        // A close that begins with the page 30 % across starts there and ends off screen.
        assert!((offset(Motion::Pop, 0.3, 0.0) - 0.3).abs() < 1e-6);
        assert_eq!(offset(Motion::Pop, 0.3, 1.0), 1.0);
        // An open that begins with it 60 % across starts there and ends in place.
        assert!((offset(Motion::Push, 0.6, 0.0) - 0.6).abs() < 1e-6);
        assert_eq!(offset(Motion::Push, 0.6, 1.0), 0.0);
    }
}
