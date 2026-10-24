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

use crate::controls::{action_button, icon_button, search_field};
use crate::icons::{self, Icon};
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
    /// One sentence under the page title that says what the page is for.
    fn lead(self) -> &'static str {
        ""
    }
}

/// The split view. `content` renders the selected item's page.
#[derive(PartialEq)]
pub struct NavigationSplitView<T: NavItem> {
    pub app_title: String,
    pub items: Vec<T>,
    pub selected: State<T>,
    pub content: Element,
    pub page_width: Dim,
    /// Set while a subpage is open: its title and what Back does.
    pub subpage: Option<Subpage>,
    /// See [`NavigationSplitView::start_on_list`].
    pub start_on_list: bool,
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

#[derive(PartialEq)]
struct ProfileCard<T: NavItem> {
    profile: Profile<T>,
    selected: State<T>,
    sidebar_open: State<bool>,
    full_page: bool,
}

impl<T: NavItem> Component for ProfileCard<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let target = self.profile.target;
        let mut selected = self.selected;
        let mut sidebar_open = self.sidebar_open;
        let full_page = self.full_page;
        let user = crate::controls::avatar::current_user();
        let name = self.profile.name.clone();
        let is_selected = selected() == target;
        // The avatar is centred on the icon column of the sidebar items, so it
        // is drawn larger than the icon's slot and pulled back over it.
        let slot = IconSize::Row.px();
        let overhang = (Dim::Box7.px() - slot) / 2.0;
        rect()
            .tw(&tokens, &[w_full(), px(S2), pt(S1), pb(S2)])
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .tw(&tokens, &[focus_ring(focus())])
                    .tw(&tokens, NAV_ITEM)
                    .tw(
                        &tokens,
                        &[surface(
                            Paint::Transparent,
                            hovering(),
                            pressing(),
                            is_selected,
                        )],
                    )
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_press(move |_: Event<PressEventData>| {
                        selected.set(target);
                        if full_page {
                            sidebar_open.set(false);
                        }
                        a11y_id.request_focus();
                    })
                    .child(
                        rect().width(Size::px(slot)).height(Size::px(slot)).child(
                            rect()
                                .position(Position::new_absolute().top(-overhang).left(-overhang))
                                .child(crate::controls::avatar(user, Dim::Box7)),
                        ),
                    )
                    .child(
                        BODY.label(&tokens, name)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
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

/// What a page adds to its title row: a status line that replaces the
/// sentence under the title, such as "Connected to Home", and an action at
/// the right of the title, a button or a master switch. Set it with
/// [`use_page_header`].
#[derive(Clone, Default, PartialEq)]
pub struct PageHeader {
    pub subtitle: Option<String>,
    pub action: Option<PageAction>,
}

impl PageHeader {
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

    /// A master switch at the right of the header, as GNOME puts Wi-Fi and
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

/// The headers that pages have set, most recent last. The last one is shown.
#[derive(Clone, Copy, PartialEq)]
struct PageHeaders(State<Vec<(u64, PageHeader)>>);

/// Shows `header` in the header bar while the calling page is open. Call it
/// on every render with the header the page wants; a changed header replaces
/// the one shown.
pub fn use_page_header(header: PageHeader) {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = use_hook(|| NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let headers = use_try_consume::<PageHeaders>().map(|found| found.0);
    use_side_effect_with_deps(&header, move |header| {
        let Some(mut headers) = headers else { return };
        let shown = headers
            .peek()
            .iter()
            .find(|(other, _)| *other == id)
            .map(|(_, set)| set.clone());
        if shown.as_ref() == Some(header) {
            return;
        }
        let mut list = headers.write();
        list.retain(|(other, _)| *other != id);
        list.push((id, header.clone()));
    });
    use_drop(move || {
        if let Some(mut headers) = headers {
            headers.write().retain(|(other, _)| *other != id);
        }
    });
}

/// Where a page sits: its index in the sidebar and the title of its open subpage.
type PageKey = (usize, Option<String>);

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
            PageMotion::Push | PageMotion::Pop => 160,
        }
    }
}

/// The move from `previous` to `next`, or none when the page did not change.
fn page_motion(previous: &PageKey, next: &PageKey) -> Option<PageMotion> {
    match (previous.1.is_some(), next.1.is_some()) {
        (false, true) => Some(PageMotion::Push),
        (true, false) => Some(PageMotion::Pop),
        _ if previous.0 != next.0 => Some(PageMotion::Fade),
        _ if previous.1 != next.1 => Some(PageMotion::Push),
        _ => None,
    }
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
    active: Option<(PageMotion, Element)>,
}

/// The page column: centred, at most `page_width` wide, with the page margins.
fn page_column(content: Element, side: Space, page_width: Dim, inset: bool) -> impl IntoElement {
    rect()
        .tw_layout(&[w_full(), items(Align::Center), pt(S4), pb(S8), px(side)])
        // Under a floating header bar the page starts below it at rest.
        .maybe_child(inset.then(|| rect().tw_layout(&[h(Dim::Header)])))
        .child(
            rect()
                .tw_layout(&[w(page_width)])
                .max_width(Size::fill())
                .child(content),
        )
}

/// The title row at the top of a page on a desktop window: the page name
/// large, a sentence under it that says what the page is for, and the page's
/// action at the right.
fn title_row(tokens: &Tokens, title: String, lead: String, action: Option<Element>) -> Element {
    rect()
        .tw(tokens, PAGE_HEAD)
        .child(
            rect()
                .tw(tokens, PAGE_HEAD_TEXT)
                .child(
                    PAGE_TITLE
                        .label(tokens, title)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .maybe_child((!lead.is_empty()).then(|| lead_line(tokens, &lead))),
        )
        .maybe_child(action)
        .into()
}

/// The sentence under a page title.
fn lead_line(tokens: &Tokens, lead: &str) -> Element {
    BODY.in_ink(Ink::Muted)
        .paragraph(tokens, lead)
        .width(Size::fill())
        .into()
}

impl<T: NavItem> NavigationSplitView<T> {
    pub fn new(
        app_title: impl Into<String>,
        items: Vec<T>,
        selected: State<T>,
        content: impl Into<Element>,
    ) -> Self {
        Self {
            app_title: app_title.into(),
            items,
            selected,
            content: content.into(),
            page_width: Dim::Page,
            subpage: None,
            start_on_list: false,
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

    /// On a narrow window, open on the list of pages instead of the selected
    /// page. Use it when the user did not ask for a page.
    pub fn start_on_list(mut self, start: bool) -> Self {
        self.start_on_list = start;
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
        let page_headers = use_provide_context(|| PageHeaders(State::create(Vec::new())));
        let section_registry = sections::provide();
        let busy = !busy_pages.0.read().is_empty();
        // On a narrow window this is true while the list of pages is the screen.
        let width = Platform::get().current_window().root_size.read().width;
        let narrow = width < NARROW_BELOW;
        // Controls and rows read their sizes from this before they are built.
        set_touch(narrow);
        let start_on_list = self.start_on_list;
        let mut sidebar_open = use_state(move || narrow && start_on_list);
        let side = if narrow { S4 } else { S8 };
        let selected = self.selected;
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
            sidebar_open,
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
        let show_list = narrow && sidebar_open() && !searching;
        // The header is the drag region. It carries Back, the spinner while a
        // page is busy and the window buttons. On a phone it also carries the
        // page title and the page's action; on a desktop window those are the
        // title row at the top of the page.
        let page_header = page_headers
            .0
            .read()
            .last()
            .map(|(_, header)| header.clone())
            .unwrap_or_default();
        // On a narrow window a page goes back to the list of pages.
        let to_list: EventHandler<()> = (move |_: ()| {
            let mut sidebar_open = sidebar_open;
            sidebar_open.set(true);
        })
        .into();
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
        let (title, mut lead) = match &self.subpage {
            Some(subpage) => (subpage.title.clone(), String::new()),
            None if searching => ("Search results".to_string(), String::new()),
            None => (
                selected().title().to_string(),
                selected().lead().to_string(),
            ),
        };
        let page_header = if show_list || searching {
            PageHeader::default()
        } else {
            page_header
        };
        if let Some(status) = &page_header.subtitle {
            lead = status.clone();
        }
        let action: Option<Element> = page_header.action.map(|action| match action {
            PageAction {
                on_press,
                switch: Some(on),
                ..
            } => crate::controls::switch(on, move |_: bool| on_press.call(())).into(),
            action => action_button(action.label, action.on_press).into(),
        });
        // A desktop header floats over the page and blurs what scrolls under it.
        let overlay = !narrow && !show_list;
        let header = if show_list {
            HeaderBar::new(self.app_title.clone()).fill(Paint::Bg)
        } else {
            let header = HeaderBar::new(if narrow { title.clone() } else { String::new() })
                .busy(busy)
                .fill(if overlay { Paint::Frosted } else { Paint::Bg });
            match &back {
                Some(on_back) => {
                    let on_back = on_back.clone();
                    header.leading(
                        icon_button(Icon::GoPrevious, move |_| on_back.call(()))
                            .size(Dim::WindowControlTarget)
                            .tooltip("Back"),
                    )
                }
                None => header,
            }
        };
        // A phone has no window buttons.
        let header = if narrow {
            match action.clone() {
                Some(action) => header.trailing(action),
                None => header,
            }
        } else {
            header.trailing(WindowControls)
        };
        let head: Option<Element> = if show_list {
            None
        } else if narrow {
            (!lead.is_empty()).then(|| lead_line(&tokens, &lead))
        } else {
            Some(title_row(&tokens, title.clone(), lead, action))
        };
        // Results replace the page content while the search has text.
        let shown: Element = match results {
            Some((results, _)) if searching => results,
            _ => self.content.clone(),
        };
        let body: Element = rect()
            .tw(&tokens, PAGE_BODY)
            .maybe_child(head)
            .child(shown)
            .into();
        // Alt+Left and the mouse Back button go back one step.
        let back_key = back;
        let back_pointer = back_key.clone();
        // Page changes move the content. A subpage slides in from the right, a
        // pop slides it out to the right, and a sidebar switch cross-fades. The
        // page being replaced stays on screen in `memory` while the motion runs.
        let index = self
            .items
            .iter()
            .position(|item| *item == selected())
            .unwrap_or(0);
        let page_key: PageKey = (index, self.subpage.as_ref().map(|s| s.title.clone()));
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
                memory.active = match (motion, memory.element.clone()) {
                    (Some(motion), Some(outgoing)) => {
                        memory.duration = tokens.motion(motion.ms());
                        memory.pending = true;
                        Some((motion, outgoing))
                    }
                    _ => None,
                };
            }
            memory.key = Some(page_key.clone());
            memory.element = Some(body.clone());
        }
        let progress = if changed {
            0.0
        } else {
            anim.get().value().clamp(0.0, 1.0)
        };
        let active = {
            let mut memory = memory.borrow_mut();
            if !changed && progress >= 1.0 {
                memory.active = None;
            }
            memory.active.clone()
        };
        // The incoming page: offset and opacity, and whether it draws over the
        // outgoing page.
        let travel = width;
        let (in_offset, in_opacity, in_layer, outgoing) = match active {
            None => (0.0, 1.0, 0, None),
            Some((motion, element)) => match motion {
                PageMotion::Push => (
                    (1.0 - progress) * travel,
                    1.0,
                    1,
                    Some((0.0, 1.0 - 0.12 * progress, 0, element)),
                ),
                PageMotion::Pop => (
                    0.0,
                    0.88 + 0.12 * progress,
                    0,
                    Some((progress * travel, 1.0, 1, element)),
                ),
                PageMotion::Fade => (0.0, progress, 0, Some((0.0, 1.0 - progress, 1, element))),
            },
        };
        let page = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .offset_x(in_offset)
            .opacity(in_opacity)
            .layer(Layer::Relative(in_layer))
            .child(
                ScrollView::new_controlled(scroll)
                    .drag_scrolling(false)
                    .child(
                        rect()
                            .width(Size::fill())
                            .on_sized(sections::origin_handler(section_registry))
                            .child(page_column(body, side, self.page_width, overlay)),
                    ),
            );
        let outgoing = outgoing.map(|(offset, opacity, layer, element)| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .height(Size::fill())
                .offset_x(offset)
                .opacity(opacity)
                .layer(Layer::Relative(layer))
                .child(ScrollView::new().drag_scrolling(false).child(page_column(
                    element,
                    side,
                    self.page_width,
                    overlay,
                )))
        });
        let edge_top = if overlay { Dim::Header.px() } else { 0.0 };
        let content_area = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .child(page)
            .maybe_child(outgoing)
            .child(ScrollEdge {
                scroll,
                top: edge_top,
            });
        let header = if narrow || show_list {
            header
        } else {
            header.title_slot(ScrolledTitle { scroll, title })
        };
        rect()
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
                    // Focus the search. A narrow window opens its sidebar first.
                    if narrow {
                        sidebar_open.set(true);
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
            .child(
                rect()
                    // Sizes change with the touch density, so a change
                    // remounts the page and it is laid out again.
                    .key(narrow)
                    .tw(&tokens, &[w_flex(), h_full(), bg(Paint::Bg)])
                    // On a desktop window the page is a rounded panel inset
                    // in the sidebar's frame.
                    .maybe(!narrow, |panel| {
                        panel.tw(
                            &tokens,
                            &[my(S2), mr(S2), rounded(Radius::Card), clip()],
                        )
                    })
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

/// How far the page has to scroll before its title row is out of view and
/// the title moves to the header bar.
const TITLE_SCROLLED_OUT: i32 = 48;

/// How long the header title takes to slide into place.
const TITLE_SLIDE_MS: u64 = 220;
/// How far below its resting place the header title starts, in pixels.
const TITLE_SLIDE_PX: f32 = 8.0;

/// The page title in the header bar, centred, shown once the title row at
/// the top of the page has scrolled out of view.
#[derive(PartialEq)]
struct ScrolledTitle {
    scroll: ScrollController,
    title: String,
}

impl Component for ScrolledTitle {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let shown = -<(i32, i32)>::from(self.scroll).1 > TITLE_SCROLLED_OUT;
        let duration = tokens.motion(TITLE_SLIDE_MS);
        let anim = use_animation_transition(shown, move |_, _| {
            AnimNum::new(0.0, 1.0)
                .time(duration)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let progress = anim.get().value().clamp(0.0, 1.0);
        let title = EMPHASIS
            .label(&tokens, self.title.clone())
            .width(Size::fill())
            .text_align(TextAlign::Center)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        rect().tw_layout(&[w_full()]).maybe_child(shown.then(|| {
            rect()
                .width(Size::fill())
                .offset_y((1.0 - progress) * TITLE_SLIDE_PX)
                .opacity(progress)
                .child(title)
        }))
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
    selected: State<T>,
    query: State<String>,
    search_id: AccessibilityId,
    sidebar_open: State<bool>,
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
        let selected = self.selected;
        let query = self.query;
        let sidebar_open = self.sidebar_open;
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
                sidebar_open,
                full_page,
            }));
        }

        let shell = if full_page {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .tw(&tokens, &[flex_col(), bg(Paint::Bg)])
        } else {
            rect().tw(&tokens, SIDEBAR)
        };
        let card = self.profile.clone().filter(|_| !typing).map(|profile| {
            Element::from(ProfileCard {
                profile,
                selected,
                sidebar_open,
                full_page,
            })
        });
        shell
            .child(search_band(query, self.search_id))
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
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut scroll = self.scroll;
        let offset = self.hit.offset;
        rect()
            .tw(&tokens, NAV_ITEM)
            .tw(
                &tokens,
                &[surface(Paint::Transparent, hovering(), pressing(), false)],
            )
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .tw(&tokens, &[focus_ring(focus())])
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
                scroll.scroll_to_y((section_top() - offset as f32).round() as i32);
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    event.stop_propagation();
                    scroll.scroll_to_y((section_top() - offset as f32).round() as i32);
                }
            })
            .child(icons::icon(
                self.hit.icon,
                IconSize::Row,
                Paint::Muted.color(&tokens),
            ))
            .child(
                BODY.label(&tokens, self.hit.title.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
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
            .fill_width(),
    )
}

#[derive(PartialEq)]
struct NavEntry<T: NavItem> {
    item: T,
    /// The items currently listed, for Up and Down.
    siblings: Vec<T>,
    selected: State<T>,
    sidebar_open: State<bool>,
    full_page: bool,
}

impl<T: NavItem> Component for NavEntry<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let item = self.item;
        let siblings = self.siblings.clone();
        let mut selected = self.selected;
        let mut sidebar_open = self.sidebar_open;
        let full_page = self.full_page;
        let is_selected = selected() == item;
        let icon_color = if is_selected {
            Paint::Accent.color(&tokens)
        } else {
            Paint::Muted.color(&tokens)
        };
        rect()
            .tw(&tokens, NAV_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering(),
                    pressing(),
                    is_selected,
                )],
            )
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(is_selected))
            .tw(&tokens, &[focus_ring(focus())])
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
                selected.set(item);
                if full_page {
                    sidebar_open.set(false);
                }
                a11y_id.request_focus();
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                // Walk from the selected item, not this entry: focus stays
                // here while repeated arrows move the selection on.
                let position = siblings.iter().position(|other| *other == selected());
                let target = match &event.key {
                    key if is_activation_key(key) => Some(item),
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
                    selected.set(target);
                }
            })
            .child(icons::icon(item.icon(), IconSize::Row, icon_color))
            .child(
                BODY.label(&tokens, item.title())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}
