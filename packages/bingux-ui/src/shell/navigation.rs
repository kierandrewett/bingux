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
use crate::scale::*;
use crate::shell::window::{HeaderBar, WindowControls};
use crate::surface::{focus_border, is_activation_key, over, use_tokens};
use crate::theme::{HEADER_HEIGHT, NARROW_BELOW, SIDEBAR_WIDTH, Tokens, WINDOW_CONTROL_BUTTON};

/// The widest the page column gets; it is centred in the content frame.
const PAGE_WIDTH: f32 = 600.0;

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
    pub selected: State<T>,
    pub content: Element,
    pub page_width: f32,
    /// Set while a subpage is open: its title and what Back does.
    pub subpage: Option<Subpage>,
    /// Settings the sidebar search can find, after the pages.
    pub hits: Vec<SearchHit<T>>,
    /// See [`NavigationSplitView::start_on_list`].
    pub start_on_list: bool,
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

/// What a page shows in the header bar. The title is the page's name, or the
/// subpage's title. `subtitle` is a muted line under it and `action` a button
/// at the right of the bar. Set it with [`use_page_header`].
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
    element: Option<Element>,
    pending: bool,
    duration: u64,
    active: Option<(PageMotion, Element)>,
}

/// The page column: centred, at most `page_width` wide, with the page margins.
fn page_column(content: Element, side: f32, page_width: f32) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .padding((S2, side, S5, side))
        .child(
            rect()
                .width(Size::px(page_width))
                .max_width(Size::fill())
                .child(content),
        )
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
            page_width: PAGE_WIDTH,
            subpage: None,
            hits: Vec::new(),
            start_on_list: false,
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

    pub fn page_width(mut self, width: f32) -> Self {
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
        let side = if narrow { S4 } else { S6 };
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
            window_width: width,
        };
        let show_list = narrow && sidebar_open();
        // The header carries the window buttons and the page's own header: its
        // title, a subtitle under the title, and an action at the right. It is
        // the drag region.
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
        let header = if show_list {
            HeaderBar::new(self.app_title.clone()).background(tokens.bg)
        } else {
            let title = match &self.subpage {
                Some(subpage) => subpage.title.clone(),
                None => selected().title().to_string(),
            };
            let header = HeaderBar::new(title).busy(busy);
            let header = match &self.subpage {
                Some(_) => header,
                None => header.background(tokens.bg),
            };
            match &back {
                Some(on_back) => {
                    let on_back = on_back.clone();
                    header.leading(
                        icon_button(Icon::GoPrevious, move |_| on_back.call(()))
                            .size(WINDOW_CONTROL_BUTTON)
                            .tooltip("Back"),
                    )
                }
                None => header,
            }
        };
        let page_header = if show_list {
            PageHeader::default()
        } else {
            page_header
        };
        let header = header.subtitle(page_header.subtitle.clone());
        let header = match page_header.action {
            Some(PageAction {
                on_press,
                switch: Some(on),
                ..
            }) => header.trailing(crate::controls::switch(on, move |_: bool| {
                on_press.call(())
            })),
            Some(action) => header.trailing(action_button(action.label, action.on_press)),
            None => header,
        };
        // A phone has no window buttons.
        let header = if narrow {
            header
        } else {
            header.trailing(WindowControls)
        };
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
        use_side_effect_with_deps(&page_key, move |_| {
            scroll_effect.scroll_to_y(0);
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
            memory.element = Some(self.content.clone());
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
                    .child(page_column(self.content.clone(), side, self.page_width)),
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
            .maybe_child(outgoing);
        rect()
            .expanded()
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(tokens.bg)
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
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .child(header)
                    .child(if show_list {
                        Element::from(sidebar(true))
                    } else {
                        Element::from(content_area)
                    }),
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
    /// The window width, for the search field of a full page.
    window_width: f32,
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

/// A hairline between groups of pages and before the search results. Groups
/// are split by a line, not a heading.
fn divider(tokens: &Tokens) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(1.0))
        .margin((S2, GUTTER, S2, GUTTER))
        .background(tokens.separator)
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
        let background = item_fill(&tokens, hovering() || self.highlighted, false, false);
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .width(Size::fill())
            .height(Size::auto())
            .padding((S1, GUTTER, S1, GUTTER))
            .corner_radius(RADIUS)
            .background(background)
            .border(focus_border(&tokens, focus()))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_: Event<PressEventData>| {
                open_result(selected, query, sidebar_open, full_page, target)
            })
            .child(
                label()
                    .text(self.hit.title.clone())
                    .color(tokens.text)
                    .font_size(TEXT_MD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                label()
                    .text(self.hit.detail.clone())
                    .color(tokens.muted)
                    .font_size(TEXT_SM)
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
            if group != last_group && !group.is_empty() && !entries.is_empty() {
                entries.push(divider(&tokens));
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
        if !visible.is_empty() && !hits.is_empty() {
            entries.push(divider(&tokens));
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

        let field_width = if full_page {
            self.window_width - 2.0 * S2
        } else {
            SIDEBAR_WIDTH - 2.0 * S2
        };
        rect()
            .width(if full_page {
                Size::fill()
            } else {
                Size::px(SIDEBAR_WIDTH)
            })
            .height(if full_page {
                Size::flex(1.0)
            } else {
                Size::fill()
            })
            .background(if full_page { tokens.bg } else { tokens.sidebar })
            .maybe(!full_page, |sidebar| {
                sidebar.border(
                    Border::new()
                        .width(BorderWidth {
                            top: 0.0,
                            right: 1.0,
                            bottom: 0.0,
                            left: 0.0,
                        })
                        .alignment(BorderAlignment::Inner)
                        .fill(tokens.separator),
                )
            })
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
            .child(search_band(
                query,
                self.highlight,
                self.search_id,
                field_width,
            ))
            .child(
                rect().width(Size::fill()).height(Size::flex(1.0)).child(
                    ScrollView::new()
                        .drag_scrolling(false)
                        .auto_hide_scrollbars(false)
                        .child(
                            rect()
                                .width(Size::fill())
                                .padding((S2, S2, S4, S2))
                                .spacing(S0)
                                .children(entries)
                                .maybe_child((typing && count == 0).then(|| {
                                    label()
                                        .text("Nothing matches.")
                                        .color(tokens.muted)
                                        .font_size(TEXT_MD)
                                        .margin((S3, GUTTER, 0.0, GUTTER))
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
        search_band(self.query, highlight, search_id, SIDEBAR_WIDTH - 2.0 * S2)
    }
}

/// The top band of the sidebar. It is HEADER_HEIGHT tall, like the page
/// header, so the search field sits on the window-control line. Typing resets
/// the highlight to the first result.
fn search_band(
    query: State<String>,
    highlight: State<usize>,
    search_id: AccessibilityId,
    field_width: f32,
) -> impl IntoElement {
    let mut highlight = highlight;
    rect()
        .width(Size::fill())
        .height(Size::px(HEADER_HEIGHT))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding((0.0, S2))
        .spacing(S2)
        .child(
            search_field(query, move |_: String| {
                highlight.set(0);
            })
            .a11y_id(search_id)
            .well()
            .width(field_width),
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
        let background = item_fill(
            &tokens,
            hovering() || self.highlighted,
            pressing(),
            is_selected,
        );
        let icon_color = if is_selected {
            tokens.accent
        } else {
            tokens.muted
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(is_selected))
            .width(Size::fill())
            .height(Size::px(row_height()))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(GUTTER)
            .padding((0.0, GUTTER))
            .corner_radius(RADIUS)
            .background(background)
            .border(focus_border(&tokens, focus()))
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
            .child(icons::icon(item.icon(), ICON_LG, icon_color))
            .child(
                label()
                    .text(item.title())
                    .color(tokens.text)
                    .font_size(TEXT_MD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

/// Background of a sidebar row. The selection layer is the base for the
/// selected row; hover and press are token layers drawn over whatever is
/// beneath, so they stack the way ControlCentreButtonSurface does.
fn item_fill(tokens: &Tokens, hovering: bool, pressing: bool, selected: bool) -> Color {
    let base = if selected {
        tokens.selection
    } else {
        Color::TRANSPARENT
    };
    if pressing {
        over(tokens.pressed, base)
    } else if hovering {
        over(tokens.hover, base)
    } else {
        base
    }
}
