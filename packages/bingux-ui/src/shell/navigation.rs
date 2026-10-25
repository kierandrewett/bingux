//! The navigation split view: a sidebar with a search field and a flat list
//! of destinations beside the page column. The sidebar is the shell's
//! sidebar surface with a hairline on its right edge. Items are rows with a
//! muted icon and a normal-weight title; the selected row carries the accent
//! selection layer, and hover and press are token layers drawn over it.
//!
//! Below [`NARROW_BELOW`] the window is a phone: the list of pages is a
//! screen of its own, a page opens over it with a Back button in the header,
//! and the window buttons are hidden.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use freya::animation::*;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, search_field};
use crate::focus::{RegionKind, claim_press, use_focus_core, use_region};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, crossfade};
use crate::shell::sections::{self, SearchHit};
use crate::shell::window::{HeaderBar, WindowControls};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::theme::{NARROW_BELOW, Tokens};
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink, PAGE_TITLE};

/// One sidebar destination.
pub trait NavItem: Copy + PartialEq + 'static {
    fn title(self) -> &'static str;
    fn icon(self) -> Icon;
    /// The sidebar section this item is listed under.
    fn group(self) -> &'static str {
        ""
    }
    /// Extra words the search field matches, space separated.
    fn keywords(self) -> &'static str {
        ""
    }
}

/// The split view. `content` renders the selected item's page.
#[derive(PartialEq)]
pub struct NavigationSplitView<T: NavItem> {
    pub app_title: String,
    pub items: Vec<T>,
    pub selected: T,
    /// Called with the page a sidebar row or the profile chooses.
    pub on_select: EventHandler<T>,
    pub content: Element,
    pub page_width: Dim,
    /// Set while a subpage is open: its title and what Back does.
    pub subpage: Option<Subpage>,
    /// Narrow windows: whether the list of pages is the screen, and the request
    /// to show or hide it. Without it the split view keeps that state itself.
    pub list: Option<(bool, EventHandler<bool>)>,
    pub profile: Option<Profile<T>>,
    /// While the search has text, `results` replaces the page content, and
    /// Enter in the search field calls `on_submit`. See
    /// [`NavigationSplitView::search`].
    pub search: Option<(State<String>, Element, EventHandler<()>)>,
}

/// The signed-in person at the top of the sidebar: an avatar and the name,
/// laid out like a sidebar item. Pressing it opens `target`.
#[derive(Clone, PartialEq)]
pub struct Profile<T> {
    pub name: String,
    pub target: T,
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
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
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
            )
            .focusable(focus(), Radius::Control)
            .tw(&tokens, &[w_full()])
    }
}

#[derive(PartialEq)]
struct ProfileCard<T: NavItem> {
    profile: Profile<T>,
    selected: T,
    on_select: EventHandler<T>,
    on_show_list: EventHandler<bool>,
    full_page: bool,
}

impl<T: NavItem> Component for ProfileCard<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let target = self.profile.target;
        let on_select = self.on_select.clone();
        let on_show_list = self.on_show_list.clone();
        let full_page = self.full_page;
        let row = SidebarRow {
            mark: SidebarMark::Avatar(crate::controls::avatar::current_user()),
            label: self.profile.name.clone(),
            selected: self.selected == target,
            on_press: (move |_: ()| {
                on_select.call(target);
                if full_page {
                    on_show_list.call(false);
                }
            })
            .into(),
            on_key: None,
        };
        rect()
            .component("ProfileCard")
            .tw(&tokens, &[w_full(), px(S2), pt(S1), pb(S2)])
            .child(Element::from(row))
    }
}

/// An open subpage. The header shows a Back button and the title in place of
/// the empty header; Alt+Left and the mouse Back button go back too.
#[derive(PartialEq)]
pub struct Subpage {
    pub title: String,
    pub on_back: EventHandler<()>,
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

/// The page as the split view draws it: its name, its scroll, and whether a
/// phone draws it, provided to the page content. The split view keeps the
/// frame of a page that is sliding out, so that page keeps its own name while
/// it moves.
#[derive(Clone, PartialEq)]
struct PageFrame {
    title: String,
    content: Element,
    scroll: ScrollController,
    compact: bool,
}

impl Component for PageFrame {
    fn render(&self) -> impl IntoElement {
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

/// The page's scroll and size class, provided by the frame. A compact
/// [`PageHead`] reads the scroll to shrink and fade its large title.
#[derive(Clone, Copy, PartialEq)]
struct PageScroll {
    scroll: ScrollController,
    compact: bool,
}

/// Where a page sits: its index in the sidebar, the title of its open subpage,
/// and whether the narrow list of pages is the screen.
type PageKey = (usize, Option<String>, bool);

/// The kind of move between two pages.
#[derive(Clone, Copy, PartialEq)]
enum PageMotion {
    Push,
    Pop,
    Fade,
}

impl PageMotion {
    fn ms(self) -> u64 {
        match self {
            PageMotion::Fade => 120,
            PageMotion::Push | PageMotion::Pop => 320,
        }
    }
}

/// The move from `previous` to `next`, or none when the page did not change.
fn page_motion(previous: &PageKey, next: &PageKey) -> Option<PageMotion> {
    // Showing or hiding the list of pages is not a page move: the list replaces
    // the page at once, and only a different page slides.
    if previous.2 != next.2 {
        return (previous.0 != next.0).then_some(PageMotion::Fade);
    }
    match (previous.1.is_some(), next.1.is_some()) {
        (false, true) => Some(PageMotion::Push),
        (true, false) => Some(PageMotion::Pop),
        _ if previous.0 != next.0 => Some(PageMotion::Fade),
        _ if previous.1 != next.1 => Some(PageMotion::Push),
        _ => None,
    }
}

/// Layers for the slide. Freya gives each node the layer of its parent plus
/// one, so the rows of a deep page sit several layers up and paint over a
/// shallow sheet. The moving sheet sits above the window header, which it
/// covers as it moves; its own header strip sits above the sheet's rows.
const SLIDE_SHEET_LAYER: i16 = 1600;
const SLIDE_HEADER_LAYER: i16 = 500;
/// The layer of the window buttons lifted over a moving sheet, above the
/// sheet's own header strip.
const SLIDE_CONTROLS_LAYER: i16 = 2600;

/// A slide in progress: the page that moved away, its header strip when it was
/// a sheet, and where the moving page starts from. The value runs from the last
/// frame drawn, so a move that starts mid-slide continues from there.
#[derive(Clone)]
struct Slide {
    motion: PageMotion,
    outgoing: Element,
    outgoing_header: Option<Element>,
    start_x: f32,
}

impl Slide {
    /// A slide from what is on screen: `shown` is the last frame's slide, when
    /// one was running, so a move that interrupts it continues from there.
    fn start(
        motion: PageMotion,
        outgoing: Element,
        outgoing_header: Option<Element>,
        shown: Option<(PageMotion, f32)>,
    ) -> Self {
        let start_x = match (motion, shown) {
            (PageMotion::Push, Some((PageMotion::Pop, x))) => x,
            (PageMotion::Push, _) => 1.0,
            (PageMotion::Pop, Some((PageMotion::Push, x))) => x,
            (PageMotion::Pop, _) => 0.0,
            (PageMotion::Fade, _) => 0.0,
        };
        Self {
            motion,
            outgoing,
            outgoing_header,
            start_x,
        }
    }

    /// The moving page's offset, as a fraction of the width, at `progress`.
    fn at(&self, progress: f32) -> f32 {
        match self.motion {
            PageMotion::Push => self.start_x * (1.0 - progress),
            PageMotion::Pop => self.start_x + (1.0 - self.start_x) * progress,
            PageMotion::Fade => 0.0,
        }
    }
}

/// How far a moving page is in its card look at `progress`. A push is a card
/// while it slides in and flat over its last 15 %; a pop is flat, then a card
/// over its first 15 %.
fn card_amount(motion: PageMotion, progress: f32) -> f32 {
    const EDGE: f32 = 0.15;
    match motion {
        PageMotion::Push => ((1.0 - progress) / EDGE).clamp(0.0, 1.0),
        PageMotion::Pop => (progress / EDGE).clamp(0.0, 1.0),
        PageMotion::Fade => 0.0,
    }
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
fn sheet_header(page: PageKey, back: Option<EventHandler<()>>) -> Element {
    let header = HeaderBar::new(String::new()).fill(Paint::Bg);
    let header = match back {
        Some(on_back) => header.leading(PageBack { page, on_back }),
        None => header,
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
struct PageMemory {
    key: Option<PageKey>,
    /// The page the scroll position was last reset for.
    scrolled_key: Option<PageKey>,
    element: Option<Element>,
    pending: bool,
    duration: u64,
    /// The animation run count when the queued slide was set up.
    start_runs: Option<usize>,
    active: Option<Slide>,
    /// The moving page's offset in the last frame drawn, when that frame was a slide.
    shown: Option<(PageMotion, f32)>,
    /// The header strip the page on screen carries, when it is a sheet.
    element_header: Option<Element>,
}

/// The page column: centred, at most `page_width` wide, with the page margins.
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
    let head = rect().component("PageHead").tw(tokens, PAGE_HEAD);
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

impl<T: NavItem> NavigationSplitView<T> {
    pub fn new(
        app_title: impl Into<String>,
        items: Vec<T>,
        selected: T,
        on_select: impl Into<EventHandler<T>>,
        content: impl Into<Element>,
    ) -> Self {
        Self {
            app_title: app_title.into(),
            items,
            selected,
            on_select: on_select.into(),
            content: content.into(),
            page_width: Dim::Page,
            subpage: None,
            list: None,
            profile: None,
            search: None,
        }
    }

    /// Show `content` as a subpage called `title`; `on_back` returns to the
    /// page below it.
    pub fn subpage(
        mut self,
        title: impl Into<String>,
        on_back: impl Into<EventHandler<()>>,
    ) -> Self {
        self.subpage = Some(Subpage {
            title: title.into(),
            on_back: on_back.into(),
        });
        self
    }

    pub fn page_width(mut self, width: Dim) -> Self {
        self.page_width = width;
        self
    }

    /// Narrow windows only: whether the list of pages is the screen. `on_show_list`
    /// is called with `true` when the list is asked for (Back from a page, Ctrl+F)
    /// and with `false` when a page is chosen. Without this the split view keeps
    /// the state itself.
    pub fn list(mut self, shown: bool, on_show_list: impl Into<EventHandler<bool>>) -> Self {
        self.list = Some((shown, on_show_list.into()));
        self
    }

    /// The signed-in person, shown at the top of the sidebar. Pressing it
    /// opens `profile.target`.
    pub fn profile(mut self, profile: Profile<T>) -> Self {
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

impl<T: NavItem> Component for NavigationSplitView<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
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
        // On a narrow window this is true while the list of pages is the screen.
        let width = Platform::get().current_window().root_size.read().width;
        let narrow = width < NARROW_BELOW || device_preview();

        let fallback_list = use_state(|| false);
        let (list_shown, on_show_list) = match &self.list {
            Some((shown, on_show_list)) => (*shown, on_show_list.clone()),
            None => {
                let mut fallback = fallback_list;
                (
                    fallback_list(),
                    EventHandler::new(move |shown: bool| fallback.set(shown)),
                )
            }
        };
        let side = if narrow { S4 } else { S8 };
        let header_region = use_region(RegionKind::Header);
        let selected = self.selected;
        // The page on screen. Its key tells a subpage from the page it was
        // opened from.
        let index = self
            .items
            .iter()
            .position(|item| *item == selected)
            .unwrap_or(0);
        // On a narrow window the list of pages is the screen.
        let list_key = narrow && list_shown;
        let page_key: PageKey = (
            index,
            self.subpage.as_ref().map(|s| s.title.clone()),
            list_key,
        );
        let scroll = use_scroll_controller(ScrollConfig::default);
        // The pages with results, when the split view searches.
        let search_hits = self
            .search
            .as_ref()
            .map(|_| sections::hits(&section_registry.0.read()));
        let sidebar = |full_page: bool| Sidebar {
            items: self.items.clone(),
            selected,
            query,
            search_id,
            on_select: self.on_select.clone(),
            on_show_list: on_show_list.clone(),
            full_page,
            profile: self.profile.clone(),
            hits: search_hits.clone(),
            scroll,
        };
        let results = self
            .search
            .as_ref()
            .map(|(_, results, on_submit)| (results.clone(), on_submit.clone()));
        let searching = results.is_some() && !query.read().trim().is_empty();
        let submit = results.as_ref().map(|(_, on_submit)| on_submit.clone());
        let show_list = list_key && !searching;
        // The header is the drag region. It carries Back, the spinner while a
        // page is busy and the window buttons. The page title shows here once
        // the title in the page scrolls under the header.
        // On a narrow window a page goes back to the list of pages.
        let to_list: EventHandler<()> = {
            let on_show_list = on_show_list.clone();
            (move |_: ()| on_show_list.call(true)).into()
        };
        let clear_search: EventHandler<()> = (move |_: ()| {
            let mut query = query;
            query.set(String::new());
        })
        .into();
        let back = match &self.subpage {
            Some(subpage) => Some(subpage.on_back.clone()),
            None if searching => narrow.then(|| clear_search.clone()),
            None => (narrow && !show_list).then(|| to_list.clone()),
        };
        // A desktop header floats over the page and blurs what scrolls under it.
        let overlay = !narrow && !show_list;
        let title = match &self.subpage {
            Some(subpage) => subpage.title.clone(),
            None if searching => "Search results".to_string(),
            None => selected.title().to_string(),
        };
        // Results replace the page content while the search has text. Results
        // carry their own page head, as a page does.
        let shown: Element = match results {
            Some((results, _)) if searching => rect()
                .tw(&tokens, PAGE_BODY)
                .child(PageHead::new())
                .child(results)
                .into(),
            _ => self.content.clone(),
        };
        // The page as the split view draws it, framed with its name. The page
        // head reads that name, and a page sliding out keeps its own frame.
        let body: Element = PageFrame {
            title: title.clone(),
            content: shown,
            scroll,
            compact: narrow,
        }
        .into();
        // Alt+Left and the mouse Back button go back one step.
        let back_key = back.clone();
        let request_list = on_show_list.clone();
        let back_pointer = back_key.clone();
        // Page changes move the content. A subpage slides in from the right, a
        // pop slides it out to the right, and a sidebar switch cross-fades. The
        // page being replaced stays on screen in `memory` while the motion runs.
        let memory = use_hook(|| Rc::new(RefCell::new(PageMemory::default())));
        let mut generation = use_state(|| 0u64);
        let anim_memory = memory.clone();
        let anim = use_animation_transition(generation(), move |_, _| {
            AnimNum::new(0.0, 1.0)
                .time(anim_memory.borrow().duration)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let mut scroll_effect = scroll;
        let effect_memory = memory.clone();
        // A new page starts at the top. The effect also runs again when the
        // scroll position changes, because it reads the scroll state, so it
        // only resets when the page itself changed; otherwise every wheel
        // turn would jump back to the top.
        use_side_effect_with_deps(&page_key, move |key| {
            let fresh = effect_memory.borrow().scrolled_key.as_ref() != Some(key);
            if fresh {
                effect_memory.borrow_mut().scrolled_key = Some(key.clone());
                scroll_effect.scroll_to_y(0);
            }
            if effect_memory.borrow().pending {
                effect_memory.borrow_mut().pending = false;
                *generation.write() += 1;
            }
        });
        let mut changed = false;
        {
            let mut memory = memory.borrow_mut();
            if let Some(previous) = memory.key.clone()
                && previous != page_key
            {
                changed = true;
                let motion = page_motion(&previous, &page_key)
                    .filter(|motion| tokens.motion(motion.ms()) > 0);
                let shown = memory.shown.take();
                let runs = *anim.runs().peek();
                memory.active = match (motion, memory.element.clone()) {
                    (Some(motion), Some(outgoing)) => {
                        memory.duration = tokens.motion(motion.ms());
                        memory.pending = true;
                        memory.start_runs = Some(runs);
                        Some(Slide::start(
                            motion,
                            outgoing,
                            memory.element_header.clone(),
                            shown,
                        ))
                    }
                    _ => {
                        memory.start_runs = None;
                        None
                    }
                };
            }
            memory.key = Some(page_key.clone());
            memory.element = Some(body.clone());
            memory.element_header =
                overlay.then(|| sheet_header(page_key.clone(), back_key.clone()));
        }
        // The animation restarts after this render, so until its run count
        // moves the last run's value is stale and the slide holds at its start.
        let waiting = !changed
            && memory
                .borrow()
                .start_runs
                .is_some_and(|start| *anim.runs().peek() <= start);
        // Read the value on every render, even when it is not used this frame:
        // the read is what subscribes this component to the animation's ticks.
        // Without it a slide that restarts while waiting never re-renders.
        let value = anim.get().value().clamp(0.0, 1.0);
        let progress = if changed || waiting { 0.0 } else { value };
        let active = {
            let mut memory = memory.borrow_mut();
            if !changed && progress >= 1.0 {
                memory.active = None;
                memory.shown = None;
            }
            memory.active.clone()
        };
        // The moving page is a sheet while it slides. The sheet takes the slide
        // layer, above the window header, so it covers the header as it moves.
        // Each frame is recorded so an interrupting move starts from there.
        let (page_offset, page_opacity, page_sheet, page_header, outgoing) = match active.clone() {
            None => (0.0, 1.0, None, None, None),
            Some(slide) => {
                let x = slide.at(progress);
                let moved = x * width;
                // The sheet is rounded and shadowed while it moves, and flat at rest.
                let card = card_amount(slide.motion, progress);
                match slide.motion {
                    PageMotion::Fade => (
                        0.0,
                        progress,
                        None,
                        None,
                        Some((0.0, 1.0 - progress, slide.outgoing, None, None)),
                    ),
                    PageMotion::Push => {
                        memory.borrow_mut().shown = Some((slide.motion, x));
                        (
                            moved,
                            1.0,
                            Some(card),
                            overlay.then(|| sheet_header(page_key.clone(), back_key.clone())),
                            Some((0.0, 1.0, slide.outgoing, None, None)),
                        )
                    }
                    PageMotion::Pop => {
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
                            )),
                        )
                    }
                }
            }
        };
        let page = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::fill())
            .child(
                // Bingux patch: keep the bar clear of the floating header and of
                // the rounded window corner.
                ScrollView::new_controlled(scroll)
                    .scrollbar_inset_top(if overlay { Dim::Header.px() } else { 0.0 })
                    .scrollbar_inset_bottom(Radius::Window.px() + 3.0)
                    .drag_scrolling(false)
                    .child(
                        rect()
                            .width(Size::fill())
                            .on_sized(sections::origin_handler(section_registry))
                            .child(page_column(body, side, self.page_width, overlay)),
                    ),
            )
            .maybe_child(page_header);
        let page = slide_frame(page_offset, page_opacity, page_sheet, &tokens, page);
        let outgoing = outgoing.map(|(offset, opacity, element, sheet, header)| {
            let outgoing_page = rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .height(Size::fill())
                .child(
                    ScrollView::new()
                        .scrollbar_inset_top(if overlay { Dim::Header.px() } else { 0.0 })
                        .scrollbar_inset_bottom(Radius::Window.px() + 3.0)
                        .drag_scrolling(false)
                        .child(rect().width(Size::fill()).child(page_column(
                            element,
                            side,
                            self.page_width,
                            overlay,
                        ))),
                )
                .maybe_child(header);
            slide_frame(offset, opacity, sheet, &tokens, outgoing_page)
        });
        let edge_top = if overlay { Dim::Header.px() } else { 0.0 };
        let content_region = use_region(RegionKind::Content);
        let content = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .on_press({
                let press = content_region.clone();
                move |event: Event<PressEventData>| press.press(&event)
            });
        let content = content
            .child(content_region.sentinel())
            .child(page)
            .maybe_child(outgoing);
        let content_area = content.child(ScrollEdge {
            scroll,
            top: edge_top,
        });
        // A page that slides in over a desktop header brings its own Back on the
        // sheet. The header's Back waits until the slide settles, so one chevron
        // shows at a time and it does not appear before the page arrives.
        let pushing = overlay
            && matches!(active.as_ref(), Some(slide) if matches!(slide.motion, PageMotion::Push));
        let back = back.filter(|_| !pushing);
        let header = if show_list {
            HeaderBar::new(self.app_title.clone()).fill(Paint::Bg)
        } else {
            let header = HeaderBar::new(String::new()).busy(busy).fill(if overlay {
                Paint::Frosted
            } else {
                Paint::Bg
            });
            match &back {
                Some(on_back) => header.leading(PageBack {
                    page: page_key.clone(),
                    on_back: on_back.clone(),
                }),
                None => header,
            }
        };
        // A phone has no window buttons. A desktop window dragged narrow keeps
        // them, so a mouse user can still close it: the buttons go only while
        // a finger drives a narrow window.
        let show_controls = !(narrow && pointer_kind() == PointerKind::Touch);
        // While a sheet moves over the header, the window buttons are lifted
        // above it, so the header keeps none and the buttons stay put.
        let lifted = overlay
            && show_controls
            && matches!(active.as_ref(), Some(slide) if !matches!(slide.motion, PageMotion::Fade));
        let header = if show_controls && !lifted {
            header.trailing(WindowControls)
        } else {
            header
        };
        let header = if show_list {
            header
        } else {
            header.title_slot(crossfade(PageTitle {
                page: page_key.clone(),
                scroll,
                title,
            }))
        };
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
                if let Some(on_back) = &back_key
                    && event.modifiers == Modifiers::ALT
                    && event.key == Key::Named(NamedKey::ArrowLeft)
                {
                    on_back.call(());
                } else if control && letter == "f" {
                    // Focus the search. A narrow window shows the list first.
                    if narrow {
                        request_list.call(true);
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
                if let Some(on_back) = &back_pointer
                    && matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Back))
                {
                    on_back.call(());
                }
            })
            .maybe_child((!narrow).then(|| sidebar(false)))
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
                    .child(if overlay {
                        Element::from(
                            rect()
                                .position(Position::new_absolute().top(0.0).left(0.0))
                                .width(Size::fill())
                                .layer(Layer::Relative(HEADER_LAYER))
                                .backdrop_blur(HEADER_BLUR)
                                .child(header),
                        )
                    } else {
                        Element::from(header)
                    })
                    .maybe_child(lifted.then(|| lifted_controls(&tokens)))
                    .child(if show_list {
                        Element::from(sidebar(true))
                    } else {
                        Element::from(content_area)
                    }),
            )
    }
}

/// How much the content behind a floating header bar is blurred.
const HEADER_BLUR: f32 = 20.0;
/// The layer the floating header bar draws on. Layers add up with element
/// depth, so this sits above the deepest page content and below overlays.
const HEADER_LAYER: i16 = 1500;

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

/// The page title in the header bar. Crossfade compares it by page and title,
/// so the title fades when the page changes and not on every scroll.
#[derive(Clone, PartialEq)]
struct PageTitle {
    page: PageKey,
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

/// The header's Back button. It stays in place when the page changes, so the
/// chevron does not jump while a page slides; only its action moves.
#[derive(Clone)]
struct PageBack {
    page: PageKey,
    on_back: EventHandler<()>,
}

impl PartialEq for PageBack {
    fn eq(&self, other: &Self) -> bool {
        self.page == other.page
    }
}

impl Component for PageBack {
    fn render(&self) -> impl IntoElement {
        let on_back = self.on_back.clone();
        let button: Element = icon_button(Icon::GoPrevious, move |_| on_back.call(()))
            .size(Dim::WindowControlTarget)
            .tooltip("Back")
            .into();
        button
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

#[derive(PartialEq)]
struct Sidebar<T: NavItem> {
    items: Vec<T>,
    selected: T,
    on_select: EventHandler<T>,
    query: State<String>,
    search_id: AccessibilityId,
    on_show_list: EventHandler<bool>,
    /// Drawn as the whole window below the header, not as a column.
    full_page: bool,
    profile: Option<Profile<T>>,
    /// The pages with search results, when the split view searches. While
    /// the search has text, the sidebar lists these instead of its pages.
    hits: Option<Vec<SearchHit>>,
    scroll: ScrollController,
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

impl<T: NavItem> Component for Sidebar<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let region = use_region(RegionKind::Sidebar);
        let selected = self.selected;
        let on_select = self.on_select.clone();
        let on_show_list = self.on_show_list.clone();
        let query = self.query;
        let full_page = self.full_page;
        let needle = query.read().trim().to_lowercase();
        let typing = !needle.is_empty();
        let visible: Vec<T> = self
            .items
            .iter()
            .copied()
            .filter(|item| {
                needle.is_empty()
                    || item.title().to_lowercase().contains(&needle)
                    || item
                        .keywords()
                        .split_whitespace()
                        .any(|word| word.starts_with(&needle))
            })
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
            let group = item.group();
            if group != last_group && !group.is_empty() {
                entries.push(group_label(&tokens, group));
            }
            last_group = group;
            entries.push(Element::from(NavEntry {
                item: *item,
                siblings: visible.clone(),
                selected,
                on_select: on_select.clone(),
                on_show_list: on_show_list.clone(),
                full_page,
            }));
        }

        let shell = if full_page {
            rect()
                .component("Sidebar")
                .width(Size::fill())
                .height(Size::flex(1.0))
                .tw(&tokens, &[flex_col(), bg(Paint::Bg)])
        } else {
            rect().tw(&tokens, SIDEBAR).component("Sidebar")
        };
        let card = self.profile.clone().filter(|_| !typing).map(|profile| {
            Element::from(ProfileCard {
                profile,
                selected,
                on_select: on_select.clone(),
                on_show_list: on_show_list.clone(),
                full_page,
            })
        });
        shell
            .on_press({
                let press = region.clone();
                move |event: Event<PressEventData>| press.press(&event)
            })
            .child(search_band(query, self.search_id))
            .child(region.sentinel())
            .maybe_child(card)
            .child(
                rect()
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
                                    rect().tw(&tokens, &[pt(S3), px(S3)]).child(
                                        BODY.in_ink(Ink::Muted).label(&tokens, "Nothing matches."),
                                    )
                                })),
                        ),
                    ),
            )
    }
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

/// Where a search hit's block top sits when it is opened: just under the frosted header.
fn section_top() -> f32 {
    Dim::Header.px() + S2.px()
}

/// The sidebar search field: a rounded well with a search glyph, on the top band.
#[derive(PartialEq)]
pub struct SearchField {
    pub query: State<String>,
}

impl Component for SearchField {
    fn render(&self) -> impl IntoElement {
        let search_id = use_a11y();
        search_band(self.query, search_id)
    }
}

/// The top band of the sidebar. It is the header height, like the page
/// header, so the search field sits on the window-control line.
fn search_band(query: State<String>, search_id: AccessibilityId) -> impl IntoElement {
    rect().tw_layout(SIDEBAR_SEARCH).child(
        search_field(query, |_: String| {})
            .a11y_id(search_id)
            .well()
            // The glyph shares the page icons' 24 px column, so it sits above
            // them and the placeholder lines up with the page names.
            .leading_column(Dim::NavIcon)
            .fill_width(),
    )
}

#[derive(PartialEq)]
struct NavEntry<T: NavItem> {
    item: T,
    /// The items currently listed, for Up and Down.
    siblings: Vec<T>,
    selected: T,
    on_select: EventHandler<T>,
    on_show_list: EventHandler<bool>,
    full_page: bool,
}

impl<T: NavItem> Component for NavEntry<T> {
    fn render(&self) -> impl IntoElement {
        let item = self.item;
        let siblings = self.siblings.clone();
        let selected = self.selected;
        let full_page = self.full_page;
        let press_select = self.on_select.clone();
        let press_list = self.on_show_list.clone();
        let key_select = self.on_select.clone();
        Element::from(SidebarRow {
            mark: SidebarMark::Icon(item.icon()),
            label: item.title().to_string(),
            selected: selected == item,
            on_press: (move |_: ()| {
                press_select.call(item);
                if full_page {
                    press_list.call(false);
                }
            })
            .into(),
            on_key: Some(
                (move |event: Event<KeyboardEventData>| {
                    // Walk from the selected item, the page on screen.
                    let position = siblings.iter().position(|other| *other == selected);
                    let target = match &event.key {
                        Key::Named(NamedKey::ArrowDown) => {
                            position.and_then(|index| siblings.get(index + 1)).copied()
                        }
                        Key::Named(NamedKey::ArrowUp) => position
                            .and_then(|index| index.checked_sub(1))
                            .and_then(|index| siblings.get(index))
                            .copied(),
                        Key::Named(NamedKey::Home) => siblings.first().copied(),
                        Key::Named(NamedKey::End) => siblings.last().copied(),
                        _ => None,
                    };
                    if let Some(target) = target {
                        event.stop_propagation();
                        key_select.call(target);
                    }
                })
                .into(),
            ),
        })
    }
}
