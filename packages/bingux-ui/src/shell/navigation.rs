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
    /// Settings the sidebar search can find, after the pages.
    pub hits: Vec<SearchHit<T>>,
    /// See [`NavigationSplitView::start_on_list`].
    pub start_on_list: bool,
    pub profile: Option<Profile<T>>,
}

/// The signed-in person at the top of the sidebar: an avatar with
/// `initials`, the name, a muted line under it, and a chevron. Pressing it
/// opens `target`.
#[derive(Clone, PartialEq)]
pub struct Profile<T> {
    pub name: String,
    pub detail: String,
    pub initials: String,
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
        // The Users page is also in the page list, which shows the selection.
        let avatar = crate::controls::avatar(self.profile.initials.clone(), Dim::Box11);
        rect().tw(&tokens, &[w_full(), px(S2), pb(S1)]).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        w_full(),
                        h(Dim::RowTall),
                        px(S3),
                        gap(S3),
                        rounded(Radius::Control),
                        surface(Paint::Transparent, hovering(), pressing(), false),
                    ],
                )
                .tw(&tokens, &[focus_ring(focus())])
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
                .child(avatar)
                .child(
                    rect()
                        .tw(&tokens, &[w_flex()])
                        .child(
                            BODY.label(&tokens, self.profile.name.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        )
                        .child(
                            CAPTION
                                .label(&tokens, self.profile.detail.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                )
                .child(icons::icon(
                    Icon::GoNext,
                    IconSize::Mark,
                    Paint::Muted.color(&tokens),
                )),
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

/// A setting the sidebar search can find. `detail` names the page it is on,
/// and choosing the hit opens `target`. `keywords` are extra words to match.
#[derive(Clone, PartialEq)]
pub struct SearchHit<T> {
    pub title: String,
    pub detail: String,
    pub keywords: String,
    pub target: T,
}

impl<T> SearchHit<T> {
    fn matches(&self, needle: &str) -> bool {
        self.title.to_lowercase().contains(needle)
            || self.detail.to_lowercase().contains(needle)
            || self.keywords.to_lowercase().contains(needle)
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
fn page_column(content: Element, side: Space, page_width: Dim) -> impl IntoElement {
    rect()
        .tw_layout(&[w_full(), items(Align::Center), pt(S4), pb(S8), px(side)])
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
            hits: Vec::new(),
            start_on_list: false,
            profile: None,
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

    /// Settings the sidebar search can find, listed after the pages.
    pub fn search_hits(mut self, hits: Vec<SearchHit<T>>) -> Self {
        self.hits = hits;
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
}

impl<T: NavItem> Component for NavigationSplitView<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut query = use_state(String::new);
        let highlight = use_state(|| 0usize);
        let search_id = use_a11y();
        let busy_pages = use_provide_context(|| BusyPages(State::create(Vec::new())));
        let page_headers = use_provide_context(|| PageHeaders(State::create(Vec::new())));
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
        let sidebar = |full_page: bool| Sidebar {
            items: self.items.clone(),
            hits: self.hits.clone(),
            selected,
            query,
            highlight,
            search_id,
            sidebar_open,
            full_page,
            profile: self.profile.clone(),
        };
        let show_list = narrow && sidebar_open();
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
        let back = match &self.subpage {
            Some(subpage) => Some(subpage.on_back.clone()),
            None => (narrow && !show_list).then(|| to_list.clone()),
        };
        let (title, mut lead) = match &self.subpage {
            Some(subpage) => (subpage.title.clone(), String::new()),
            None => (
                selected().title().to_string(),
                selected().lead().to_string(),
            ),
        };
        let page_header = if show_list {
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
        let header = if show_list {
            HeaderBar::new(self.app_title.clone()).fill(Paint::Bg)
        } else {
            let header = HeaderBar::new(if narrow { title.clone() } else { String::new() })
                .busy(busy)
                .fill(Paint::Bg);
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
        let body: Element = rect()
            .tw(&tokens, PAGE_BODY)
            .maybe_child(head)
            .child(self.content.clone())
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
        let scroll = use_scroll_controller(ScrollConfig::default);
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
                    .auto_hide_scrollbars(false)
                    .child(page_column(body, side, self.page_width)),
            );
        let outgoing = outgoing.map(|(offset, opacity, layer, element)| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .height(Size::fill())
                .offset_x(offset)
                .opacity(opacity)
                .layer(Layer::Relative(layer))
                .child(
                    ScrollView::new()
                        .drag_scrolling(false)
                        .auto_hide_scrollbars(false)
                        .child(page_column(element, side, self.page_width)),
                )
        });
        let content_area = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .child(page)
            .maybe_child(outgoing)
            .child(ScrollEdge { scroll });
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
                    .child(header)
                    .child(if show_list {
                        Element::from(sidebar(true))
                    } else {
                        Element::from(content_area)
                    }),
            )
    }
}

/// How far the page has to scroll before its title row is out of view and
/// the title moves to the header bar.
const TITLE_SCROLLED_OUT: i32 = 48;

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
        rect().tw_layout(&[w_full()]).maybe_child(shown.then(|| {
            EMPHASIS
                .label(&tokens, self.title.clone())
                .width(Size::fill())
                .text_align(TextAlign::Center)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
        }))
    }
}

/// A hairline under the header bar while the page is scrolled.
#[derive(PartialEq)]
struct ScrollEdge {
    scroll: ScrollController,
}

impl Component for ScrollEdge {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scrolled = <(i32, i32)>::from(self.scroll).1 < 0;
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
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
    hits: Vec<SearchHit<T>>,
    selected: State<T>,
    query: State<String>,
    highlight: State<usize>,
    search_id: AccessibilityId,
    sidebar_open: State<bool>,
    /// Drawn as the whole window below the header, not as a column.
    full_page: bool,
    profile: Option<Profile<T>>,
}

/// The most settings the sidebar search lists.
const MAX_HITS: usize = 20;

/// Makes `target` the selected page and clears the search. On a narrow window
/// the page list closes too.
fn open_result<T: NavItem>(
    mut selected: State<T>,
    mut query: State<String>,
    mut sidebar_open: State<bool>,
    full_page: bool,
    target: T,
) {
    selected.set(target);
    query.set(String::new());
    if full_page {
        sidebar_open.set(false);
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

/// A search result for a setting: its title, with the page it is on under it.
#[derive(PartialEq)]
struct HitRow<T: NavItem> {
    hit: SearchHit<T>,
    highlighted: bool,
    selected: State<T>,
    query: State<String>,
    sidebar_open: State<bool>,
    full_page: bool,
}

impl<T: NavItem> Component for HitRow<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let target = self.hit.target;
        let selected = self.selected;
        let query = self.query;
        let sidebar_open = self.sidebar_open;
        let full_page = self.full_page;
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .tw(
                &tokens,
                &[
                    w_full(),
                    py(S1),
                    px(GUTTER),
                    rounded(Radius::Control),
                    surface(
                        Paint::Transparent,
                        hovering() || self.highlighted,
                        false,
                        false,
                    ),
                ],
            )
            .height(Size::auto())
            .tw(&tokens, &[focus_ring(focus())])
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_: Event<PressEventData>| {
                open_result(selected, query, sidebar_open, full_page, target)
            })
            .child(
                BODY.label(&tokens, self.hit.title.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                CAPTION
                    .label(&tokens, self.hit.detail.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

impl<T: NavItem> Component for Sidebar<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let selected = self.selected;
        let query = self.query;
        let mut highlight = self.highlight;
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
        let hits: Vec<SearchHit<T>> = if typing {
            self.hits
                .iter()
                .filter(|hit| hit.matches(&needle))
                .take(MAX_HITS)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        // Pages first, then settings: one list that Up, Down and Enter walk.
        let targets: Vec<T> = visible
            .iter()
            .copied()
            .chain(hits.iter().map(|hit| hit.target))
            .collect();
        let count = targets.len();
        let current = highlight().min(count.saturating_sub(1));
        let mut entries: Vec<Element> = Vec::new();
        let mut last_group = "";
        for (index, item) in visible.iter().enumerate() {
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
                highlighted: typing && index == current,
            }));
        }
        if !hits.is_empty() {
            entries.push(group_label(&tokens, "settings"));
        }
        for (offset, hit) in hits.iter().enumerate() {
            entries.push(Element::from(HitRow {
                hit: hit.clone(),
                highlighted: visible.len() + offset == current,
                selected,
                query,
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
        shell
            .on_key_down(move |event: Event<KeyboardEventData>| {
                // Up, Down and Enter walk the results only while a search is typed.
                if !typing {
                    return;
                }
                match &event.key {
                    Key::Named(NamedKey::ArrowDown) => {
                        event.stop_propagation();
                        highlight.set((current + 1).min(count.saturating_sub(1)));
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        event.stop_propagation();
                        highlight.set(current.saturating_sub(1));
                    }
                    Key::Named(NamedKey::Enter) => {
                        if let Some(target) = targets.get(current).copied() {
                            event.stop_propagation();
                            open_result(selected, query, sidebar_open, full_page, target);
                        }
                    }
                    _ => {}
                }
            })
            .child(search_band(query, self.highlight, self.search_id))
            .maybe_child(self.profile.clone().filter(|_| !typing).map(|profile| {
                Element::from(ProfileCard {
                    profile,
                    selected,
                    sidebar_open,
                    full_page,
                })
            }))
            .child(
                rect().width(Size::fill()).height(Size::flex(1.0)).child(
                    ScrollView::new()
                        .drag_scrolling(false)
                        .auto_hide_scrollbars(false)
                        .child(
                            rect()
                                .tw(&tokens, SIDEBAR_LIST)
                                .tw(&tokens, &[pt(S2)])
                                .children(entries)
                                .maybe_child((typing && count == 0).then(|| {
                                    rect().tw(&tokens, &[pt(S3), px(S3)]).child(
                                        BODY.in_ink(Ink::Muted).label(&tokens, "Nothing matches."),
                                    )
                                })),
                        ),
                ),
            )
    }
}

/// The sidebar search field: a rounded well with a search glyph, on the top band.
#[derive(PartialEq)]
pub struct SearchField {
    pub query: State<String>,
}

impl Component for SearchField {
    fn render(&self) -> impl IntoElement {
        let highlight = use_state(|| 0usize);
        let search_id = use_a11y();
        search_band(self.query, highlight, search_id)
    }
}

/// The top band of the sidebar. It is the header height, like the page
/// header, so the search field sits on the window-control line. Typing resets
/// the highlight to the first result.
fn search_band(
    query: State<String>,
    highlight: State<usize>,
    search_id: AccessibilityId,
) -> impl IntoElement {
    let mut highlight = highlight;
    rect().tw_layout(SIDEBAR_SEARCH).child(
        search_field(query, move |_: String| {
            highlight.set(0);
        })
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
    /// Drawn with the hover layer: the keyboard highlight of the search.
    highlighted: bool,
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
                    hovering() || self.highlighted,
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
