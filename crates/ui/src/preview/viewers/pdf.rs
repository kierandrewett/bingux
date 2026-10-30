//! Documents: the PDF viewer.
//!
//! `bingux-preview-host` opens the file with poppler and answers four kinds of
//! request: where each page is and how big (sent at once), a page rendered at a
//! scale into shared memory, the words of a page with their boxes, and a search.
//! This viewer keeps the pages in one scrolling column, draws only the pages
//! near the viewport, asks for each at the scale on screen (times the display
//! scale, rounded), and shows the nearest scale it has until the right one
//! arrives, so a zoom never flashes white. Renders that are no longer wanted
//! are cancelled before they start. Nothing waits on the helper.
//!
//! | Key | Does |
//! | --- | ---- |
//! | Page Up, Page Down, Home, End | scroll (not the arrows: they choose the file) |
//! | Space, Shift+Space | in a docked pane that has the keyboard, a page down or up (in a popup Space closes) |
//! | `+`, `=`, `-`, `0`, `1`, `9` | zoom in, out, fit width, 100 %, fit page |
//! | Ctrl and wheel | zoom about the middle; Shift and wheel scrolls sideways |
//! | Ctrl+F | find; Enter and Shift+Enter (F3) next and previous; Escape closes it |
//! | F9 | the page thumbnails |
//! | drag over text, Ctrl+C | select words, copy them |
//!
//! A document opens fitted to the page: the whole first page, centred. The
//! toolbar is the page field, the zoom (out, menu, in) and find, in a popup
//! and in a pane alike. A pane takes the keys above only once it has the
//! keyboard (a click on a page), like the player.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use bingux_preview_core::host::protocol::{ErrorCode, Message, Packet, Rect, Word};
use freya::engine::prelude::AlphaType;
use freya::prelude::*;
use freya_core::elements::image::ImageHandle;

use super::super::chrome::PreviewNav;
use super::super::host_link::{HostLink, LinkEvent};
use super::super::keys::{KeyClaim, Keyboard};
use super::super::mode::Mode;
use super::super::subject::Resolved;
use super::card::Card;
use super::pdf_layout::{
    CACHE_LIMIT, GAP, Layout, MARGIN, PageSize, RenderCache, THUMB_WIDTH, WordRef, Zoom, find_status, key_scale, layout, next_hit, render_key, selected_runs, text_of, word_at,
};
use crate::component::ComponentName;
use crate::controls::capture::{CaptureEvent, Phase, use_capture};
use crate::controls::progress::{SpinnerSize, spinner};
use crate::controls::{icon_button, search_field, text_field};
use crate::icons::Icon;
use crate::shell::{MenuItem, menu_button, menu_item};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The pages kept as renders: pages and thumbnails together.
const KEPT: usize = CACHE_LIMIT + 10;
/// Thumbnails are requested under keys with this bit set.
const THUMB_KEY: u32 = 1 << 31;
/// The thumbnail column's width, and the label under each thumbnail.
const SIDEBAR_WIDTH: f32 = THUMB_WIDTH + 2.0 * S4_PX;
const S4_PX: f32 = 16.0;
const THUMB_LABEL: f32 = 20.0;

/// What a person asked of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DocumentAction {
    /// Go to this page (zero based).
    Page(u32),
    ZoomIn,
    ZoomOut,
    Zoom(ZoomChoice),
    Find,
}

/// A zoom the menu offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomChoice {
    FitWidth,
    FitPage,
    /// A percentage of 100 %.
    Percent(u32),
}

impl ZoomChoice {
    fn zoom(self) -> Zoom {
        match self {
            ZoomChoice::FitWidth => Zoom::FitWidth,
            ZoomChoice::FitPage => Zoom::FitPage,
            ZoomChoice::Percent(percent) => Zoom::Percent(percent as f32 / 100.0),
        }
    }
}

/// What the toolbar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentState {
    /// The page the person is on, zero based.
    pub page: usize,
    pub pages: usize,
    /// "Fit width", "125%".
    pub zoom: String,
    pub find: bool,
}

/// The document viewer's toolbar: the page field, the zoom and find. The
/// same in a popup, in a pane and on a phone.
#[derive(Clone, PartialEq)]
pub struct DocumentControls {
    state: DocumentState,
    page_text: State<String>,
    on_action: EventHandler<DocumentAction>,
}

impl DocumentControls {
    /// `page_text` is the page field's text: it follows the page the person
    /// scrolls to, and a typed number goes to that page on Enter.
    pub fn new(state: DocumentState, page_text: State<String>, on_action: impl Into<EventHandler<DocumentAction>>) -> Self {
        DocumentControls { state, page_text, on_action: on_action.into() }
    }
}

/// The zoom menu's slot: wide enough for "Fit width", so the buttons around it
/// do not move when the label changes to "125%".
const ZOOM_SLOT: f32 = 108.0;

fn zoom_items(current: &str, on_action: &EventHandler<DocumentAction>) -> Vec<MenuItem> {
    let mut choices: Vec<(String, ZoomChoice)> = vec![("Fit width".to_owned(), ZoomChoice::FitWidth), ("Fit page".to_owned(), ZoomChoice::FitPage)];
    choices.extend([50, 75, 100, 125, 150, 200, 300, 400].map(|percent| (format!("{percent}%"), ZoomChoice::Percent(percent))));
    choices
        .into_iter()
        .map(|(label, choice)| {
            let on_action = on_action.clone();
            let checked = label == current;
            menu_item(label, move |_| on_action.call(DocumentAction::Zoom(choice))).checked(checked)
        })
        .collect()
}

impl Component for DocumentControls {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let on_action = self.on_action.clone();
        let state = &self.state;
        let page_field = {
            let on_action = on_action.clone();
            let pages = state.pages;
            text_field(self.page_text, move |text: String| {
                if let Ok(page) = text.trim().parse::<usize>() {
                    on_action.call(DocumentAction::Page(page.clamp(1, pages.max(1)) as u32 - 1));
                }
            })
            .dense()
            .width(Dim::Box7)
        };
        let zoom_out = {
            let on_action = on_action.clone();
            icon_button(Icon::ZoomOut, move |_| on_action.call(DocumentAction::ZoomOut)).tooltip("Zoom out (-)")
        };
        let zoom_in = {
            let on_action = on_action.clone();
            icon_button(Icon::ZoomIn, move |_| on_action.call(DocumentAction::ZoomIn)).tooltip("Zoom in (+)")
        };
        let find = {
            let on_action = on_action.clone();
            icon_button(Icon::Search, move |_| on_action.call(DocumentAction::Find)).active(state.find).tooltip("Find (Ctrl+F)")
        };
        let of_pages = CAPTION.in_ink(Ink::Muted).label(&tokens, format!("of {}", state.pages)).max_lines(1);
        let zoom_menu = rect()
            .width(Size::px(ZOOM_SLOT))
            .tw(&tokens, &[flex_row(), justify(Align::Center)])
            .child(menu_button(state.zoom.clone(), zoom_items(&state.zoom, &on_action)));
        rect()
            .component("DocumentControls")
            .tw(&tokens, &[w_full(), flex_row(), grow_children(), items(Align::Center), gap(S1), px(S2), py(S1), bg(Paint::Surface)])
            .child(page_field)
            .child(of_pages)
            .child(rect().width(Size::flex(1.0)))
            .child(zoom_out)
            .child(zoom_menu)
            .child(zoom_in)
            .child(find)
            .into_element()
    }
}

/// How the helper's answer to opening is kept.
#[derive(Clone, Debug, PartialEq)]
enum Load {
    Opening,
    Ready,
    Failed { code: ErrorCode, message: String },
}

#[derive(Clone, Debug, PartialEq)]
struct Doc {
    load: Load,
    /// Page sizes in points, as the helper sends them.
    sizes: Vec<PageSize>,
    pages: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Find {
    open: bool,
    request: u32,
    hits: Vec<(usize, Rect)>,
    active: Option<usize>,
    searching: bool,
}

/// What lives outside the signals: renders, outstanding requests, words.
struct Caches {
    renders: RenderCache<ImageHandle>,
    /// A request that is out, and what it is for.
    pending: HashMap<u32, (usize, u32)>,
    words: HashMap<usize, Vec<Word>>,
    /// Pages whose words were asked for.
    words_asked: HashSet<usize>,
    next_request: u32,
}

impl Caches {
    fn new() -> Self {
        Caches { renders: RenderCache::new(KEPT), pending: HashMap::new(), words: HashMap::new(), words_asked: HashSet::new(), next_request: 10 }
    }

    fn request(&mut self) -> u32 {
        self.next_request += 1;
        self.next_request
    }

    /// Whether a render of `page` at `key` is already out.
    fn is_pending(&self, page: usize, key: u32) -> bool {
        self.pending.values().any(|entry| *entry == (page, key))
    }
}

#[derive(PartialEq)]
pub(super) struct DocumentView {
    pub resolved: Arc<Resolved>,
    pub nav: PreviewNav,
    pub mode: Mode,
}

impl Component for DocumentView {
    fn render(&self) -> impl IntoElement {
        let mut attempt = use_state(|| 0u32);
        Reading {
            resolved: self.resolved.clone(),
            nav: self.nav.clone(),
            mode: self.mode,
            attempt: *attempt.read(),
            on_retry: EventHandler::new(move |_: ()| *attempt.write() += 1),
        }
    }
}

#[derive(PartialEq)]
struct Reading {
    resolved: Arc<Resolved>,
    nav: PreviewNav,
    mode: Mode,
    attempt: u32,
    on_retry: EventHandler<()>,
}

/// A paper-white page box with the picture, the find hits and the selection.
#[allow(clippy::too_many_arguments)]
fn page_box(
    tokens: &crate::theme::Tokens,
    size: (f32, f32),
    scale: f32,
    image_for: Option<ImageHandle>,
    hits: &[(Rect, bool)],
    selected: &[Rect],
) -> Element {
    let to_px = |value: i32| value as f32 / 1000.0 * scale;
    let highlight = |rect: &Rect, paint: Paint, opacity: f32| {
        Element::from(
            rect_at(to_px(rect.x0), to_px(rect.y0), to_px(rect.x1 - rect.x0), to_px(rect.y1 - rect.y0))
                .opacity(opacity)
                .tw(tokens, &[bg(paint)]),
        )
    };
    let paper = rect()
        .width(Size::px(size.0))
        .height(Size::px(size.1))
        // The paper of a PDF page is white in both schemes, like a slider's knob.
        .tw(tokens, &[bg(Paint::Knob), shadow(Elevation::Knob), clip()]);
    let paper = match image_for {
        Some(handle) => paper.child(image(handle).width(Size::fill()).height(Size::fill()).aspect_ratio(AspectRatio::None).sampling_mode(SamplingMode::Trilinear)),
        None => paper,
    };
    paper
        .children(selected.iter().map(|rect| highlight(rect, Paint::Accent, 0.3)))
        .children(hits.iter().map(|(rect, active)| if *active { highlight(rect, Paint::Warning, 0.55) } else { highlight(rect, Paint::AccentSoft, 1.0) }))
        .into()
}

/// An absolutely placed box inside a page.
fn rect_at(x: f32, y: f32, width: f32, height: f32) -> Rect_ {
    // ui-lint: allow(page boxes take the renderer's measured pixel size)
    rect().position(Position::new_absolute().left(x).top(y)).width(Size::px(width.max(1.0))).height(Size::px(height.max(1.0)))
}

/// Freya's `rect()` builder under the name this file can use as a type.
type Rect_ = freya::prelude::Rect;

impl Component for Reading {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(u64::from(self.attempt))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let docked = self.mode == Mode::Pane;
        let compact = docked || crate::style::is_compact();
        let path: Option<PathBuf> = self.resolved.source.as_path().map(ToOwned::to_owned);
        let keyboard = use_try_consume::<Keyboard>().unwrap_or_else(Keyboard::always);
        let claim = use_try_consume::<KeyClaim>().unwrap_or_default();

        let link = use_hook({
            let path = path.clone();
            move || {
                let link = HostLink::start();
                if let Some(path) = &path {
                    link.open_document(path);
                }
                Rc::new(link)
            }
        });
        let caches = use_hook(|| Rc::new(RefCell::new(Caches::new())));
        let mut doc = use_state(|| Doc { load: Load::Opening, sizes: Vec::new(), pages: 0 });
        let mut find = use_state(Find::default);
        let picked = use_state(|| None::<(WordRef, WordRef)>);
        let zoom = use_state(|| Zoom::FitPage);
        let sidebar = use_state(|| false);
        let viewport = use_state(|| (0.0f32, 0.0f32));
        let side_viewport = use_state(|| (0.0f32, 0.0f32));
        let pan_x = use_state(|| 0.0f32);
        let mut version = use_state(|| 0u32);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let side_scroll = use_scroll_controller(ScrollConfig::default);
        let query = use_state(String::new);
        let page_text = use_state(|| "1".to_owned());
        let last_page = use_hook(|| Rc::new(Cell::new(usize::MAX)));
        let moved = use_hook(|| Rc::new(Cell::new(false)));

        // What the helper says becomes state, on this thread, in one task.
        use_hook({
            let link = link.clone();
            let caches = caches.clone();
            move || {
                spawn(async move {
                    while let Some(event) = link.next().await {
                        match event {
                            LinkEvent::Packet(Packet { request, message }) => match message {
                                Message::Opened { pages, .. } => {
                                    let mut now = doc.write();
                                    now.pages = pages as usize;
                                    now.load = Load::Ready;
                                }
                                Message::PageSizes { sizes, .. } => {
                                    let mut now = doc.write();
                                    now.sizes.extend(sizes.into_iter().map(|(w, h)| (w as f32 / 1000.0, h as f32 / 1000.0)));
                                }
                                Message::PageTextIs { page, words, .. } => {
                                    caches.borrow_mut().words.entry(page as usize).or_default().extend(words);
                                    *version.write() += 1;
                                }
                                Message::SearchHit { page, rects } => {
                                    let mut now = find.write();
                                    if request == now.request {
                                        now.hits.extend(rects.into_iter().map(|rect| (page as usize, rect)));
                                    }
                                }
                                Message::SearchDone { .. } => {
                                    let mut now = find.write();
                                    if request == now.request {
                                        now.searching = false;
                                    }
                                }
                                Message::Error { code, message } => {
                                    let rendering = caches.borrow_mut().pending.remove(&request).is_some();
                                    if !rendering && doc.peek().load != Load::Ready {
                                        doc.write().load = Load::Failed { code, message };
                                    }
                                }
                                _ => {}
                            },
                            LinkEvent::Page(page) => {
                                let entry = caches.borrow_mut().pending.remove(&page.request);
                                if let (Some((index, key)), Some(handle)) = (entry, ImageHandle::from_rgba(page.width, page.height, page.pixels, AlphaType::Opaque)) {
                                    caches.borrow_mut().renders.insert(index, key, handle);
                                    *version.write() += 1;
                                }
                            }
                            LinkEvent::Frame => {}
                            LinkEvent::Died(failure) => {
                                doc.write().load = Load::Failed { code: failure.code, message: failure.message };
                            }
                        }
                    }
                })
            }
        });
        use_drop({
            let link = link.clone();
            move || link.close()
        });

        let now = doc.read().clone();
        let _ = *version.read();
        let (vw, vh) = *viewport.read();
        let display = *Platform::get().current_window().scale_factor.read();
        let scroll_y = -(<(i32, i32)>::from(scroll).1 as f32);

        // Where the pages are at this zoom.
        let sizes = &now.sizes;
        let probe = layout(sizes, 1.0);
        let current_guess = probe.current(scroll_y / 1.0_f32.max(1.0), vh);
        let _ = current_guess;
        let first_scale = zoom.read().scale(sizes, 0, (vw, vh));
        let lay = layout(sizes, first_scale);
        let current = lay.current(scroll_y, vh);
        let scale = zoom.read().scale(sizes, current, (vw, vh));
        let lay = if (scale - first_scale).abs() > f32::EPSILON { layout(sizes, scale) } else { lay };
        let current = if lay.len() == 0 { 0 } else { lay.current(scroll_y, vh).min(lay.len() - 1) };
        // A stack shorter than the viewport (a page that fits) sits in the middle of it.
        let centre = ((vh - lay.height) / 2.0).floor().max(0.0);
        let key = render_key(scale, display);
        let visible = lay.visible(scroll_y, vh);

        // The page field follows the page the person scrolled to.
        if last_page.replace(current) != current {
            let mut page_text = page_text;
            page_text.set((current + 1).to_string());
        }

        // Ask for what is on screen, cancel what scrolled away.
        if matches!(now.load, Load::Ready) && !visible.is_empty() && vw > 1.0 && vh > 1.0 {
            let mut state = caches.borrow_mut();
            let mut wanted: Vec<(usize, u32)> = visible.clone().map(|page| (page, key)).collect();
            if *sidebar.read() && !compact {
                let thumbs = thumb_layout(sizes);
                let shown = thumbs.visible(-(<(i32, i32)>::from(side_scroll).1 as f32), side_viewport.read().1);
                let thumb_key = THUMB_KEY | ((THUMB_WIDTH * display as f32).round() as u32);
                wanted.extend(shown.map(|page| (page, thumb_key)));
            }
            // The current page first: it is the one being read.
            wanted.sort_by_key(|(page, key)| (*key & THUMB_KEY != 0, page.abs_diff(current)));
            let stale: Vec<u32> = state.pending.iter().filter(|(_, entry)| !wanted.contains(entry)).map(|(request, _)| *request).collect();
            for request in stale {
                state.pending.remove(&request);
                link.send(Packet { request: 0, message: Message::Cancel { target: request } });
            }
            for (page, wanted_key) in wanted {
                if state.renders.has(page, wanted_key) || state.is_pending(page, wanted_key) {
                    continue;
                }
                let request = state.request();
                state.pending.insert(request, (page, wanted_key));
                let message = if wanted_key & THUMB_KEY != 0 {
                    Message::RenderPage { page: page as u32, scale_milli: 4000, max_w: wanted_key & !THUMB_KEY, max_h: 0 }
                } else {
                    Message::RenderPage { page: page as u32, scale_milli: (key_scale(wanted_key) * 1000.0).round() as u32, max_w: 0, max_h: 0 }
                };
                link.send(Packet { request, message });
            }
            // Words of the pages being read, so a drag can select.
            for page in visible.clone().filter(|page| page.abs_diff(current) <= 1) {
                if state.words_asked.insert(page) {
                    let request = state.request();
                    link.send(Packet { request, message: Message::PageText { page: page as u32 } });
                }
            }
        }

        // Keys the viewer takes, and the claim that tells the preview not to
        // act on typed letters while a field has the keyboard.
        claim.set(find.read().open);
        let scroll_by = {
            move |delta: f32, max: f32| {
                let mut scroll = scroll;
                let now = -(<(i32, i32)>::from(scroll).1 as f32);
                scroll.scroll_to_y(-((now + delta).clamp(0.0, max)).round() as i32);
            }
        };
        let jump_to = {
            let lay = lay.clone();
            move |page: usize| {
                let mut scroll = scroll;
                scroll.scroll_to_y(-(lay.scroll_to_page(page.min(lay.len().saturating_sub(1)), vh).round() as i32));
            }
        };
        // A zoom keeps the middle of the viewport where it is, once the new
        // layout has told the scroll view how tall the page stack became.
        let set_zoom = {
            let sizes = sizes.clone();
            move |next: Zoom| {
                let mut zoom = zoom;
                let mut scroll = scroll;
                let before = *zoom.peek();
                let (vw, vh) = *viewport.peek();
                let at = -(<(i32, i32)>::from(scroll).1 as f32);
                let old_layout = layout(&sizes, before.scale(&sizes, 0, (vw, vh)));
                let current = old_layout.current(at, vh);
                let old_scale = before.scale(&sizes, current, (vw, vh));
                let new_scale = next.scale(&sizes, current, (vw, vh));
                if (new_scale - old_scale).abs() < 1e-4 && before == next {
                    return;
                }
                let ratio = new_scale / old_scale.max(1e-4);
                let target = ((at + vh / 2.0) * ratio - vh / 2.0).max(0.0);
                zoom.set(next);
                let ticker = RenderingTicker::get();
                let window = Platform::get().current_window();
                spawn(async move {
                    window.request_redraw();
                    ticker.tick().await;
                    ticker.tick().await;
                    scroll.scroll_to_y(-(target.round() as i32));
                });
            }
        };
        let zoom_step = {
            let set_zoom = set_zoom.clone();
            let sizes = sizes.clone();
            move |direction: i32| {
                let (vw, vh) = *viewport.peek();
                let at = -(<(i32, i32)>::from(scroll).1 as f32);
                let current = layout(&sizes, zoom.peek().scale(&sizes, 0, (vw, vh))).current(at, vh);
                let scale = zoom.peek().scale(&sizes, current, (vw, vh));
                set_zoom(Zoom::step(scale, direction));
            }
        };
        let search = {
            let link = link.clone();
            let caches = caches.clone();
            move |text: String| {
                let mut find = find;
                let request = caches.borrow_mut().request();
                let previous = find.peek().request;
                if previous != 0 {
                    link.send(Packet { request: 0, message: Message::Cancel { target: previous } });
                }
                let searching = !text.trim().is_empty();
                find.set(Find { open: true, request, searching, ..Find::default() });
                if searching {
                    link.send(Packet { request, message: Message::Search { query: text } });
                }
            }
        };
        let step_hit = {
            let lay = lay.clone();
            move |direction: i32| {
                let mut find = find;
                let (count, active) = {
                    let now = find.peek();
                    (now.hits.len(), now.active)
                };
                let Some(next) = next_hit(active, count, direction) else { return };
                find.write().active = Some(next);
                let (page, rect) = find.peek().hits[next];
                let (_, vh) = *viewport.peek();
                let mut scroll = scroll;
                let at = -(<(i32, i32)>::from(scroll).1 as f32);
                if let Some(to) = lay.scroll_to_hit(page, rect, scale, at, vh) {
                    scroll.scroll_to_y(-(to.round() as i32));
                }
            }
        };
        // The first hit at or after the current page becomes the active one.
        {
            let hits = find.read().hits.clone();
            if find.read().active.is_none() && !hits.is_empty() && (hits.last().is_some_and(|(page, _)| *page >= current) || !find.read().searching) {
                let first = hits.iter().position(|(page, _)| *page >= current).unwrap_or(0);
                let mut find = find;
                find.write().active = Some(first);
                let (page, rect) = hits[first];
                if let Some(to) = lay.scroll_to_hit(page, rect, scale, scroll_y, vh) {
                    let mut scroll = scroll;
                    scroll.scroll_to_y(-(to.round() as i32));
                }
            }
        }

        let copy = {
            let caches = caches.clone();
            move || {
                let Some((a, b)) = *picked.peek() else { return };
                let state = caches.borrow();
                let runs = selected_runs(a, b, |page| state.words.get(&page).map_or(0, Vec::len));
                let words: Vec<&Word> = runs.iter().flat_map(|(page, first, last)| state.words.get(page).map(|words| words[*first..=*last].iter()).into_iter().flatten()).collect();
                if !words.is_empty() {
                    let _ = freya::clipboard::Clipboard::set(text_of(&words));
                }
            }
        };

        let on_key = {
            let step_hit = step_hit.clone();
            let claim = claim.clone();
            let zoom_step = zoom_step.clone();
            let set_zoom = set_zoom.clone();
            let copy = copy.clone();
            let max_scroll = lay.max_scroll(vh);
            move |event: Event<KeyboardEventData>| {
                // A pane leaves its keys with the list until it is clicked.
                if !keyboard.held() {
                    return;
                }
                let mut find = find;
                let mut sidebar = sidebar;
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                let alt = event.modifiers.contains(Modifiers::ALT);
                let page = vh * 0.9;
                let handled = match &event.key {
                    Key::Named(NamedKey::Escape) if find.peek().open => {
                        find.write().open = false;
                        true
                    }
                    // Typing in the find field: the field has these.
                    _ if claim.get() && !ctrl && !matches!(event.key, Key::Named(NamedKey::F3 | NamedKey::F9)) => false,
                    // No arrows here: they choose the file (the preview host steps it).
                    Key::Named(NamedKey::PageDown) => {
                        scroll_by(page, max_scroll);
                        true
                    }
                    Key::Named(NamedKey::PageUp) => {
                        scroll_by(-page, max_scroll);
                        true
                    }
                    Key::Named(NamedKey::Home) => {
                        scroll_by(-1e9, max_scroll);
                        true
                    }
                    Key::Named(NamedKey::End) => {
                        scroll_by(1e9, max_scroll);
                        true
                    }
                    // Space closes a popup; in a pane that has the keyboard it pages.
                    Key::Character(text) if text.as_str() == " " && keyboard.is_pane() && !ctrl && !alt => {
                        scroll_by(if shift { -page } else { page }, max_scroll);
                        true
                    }
                    // Shift+Space pages back in a popup too.
                    Key::Character(text) if text.as_str() == " " && shift && !ctrl && !alt => {
                        scroll_by(-page, max_scroll);
                        true
                    }
                    Key::Named(NamedKey::F9) => {
                        let shown = *sidebar.peek();
                        sidebar.set(!shown);
                        true
                    }
                    Key::Named(NamedKey::F3) => {
                        step_hit(if shift { -1 } else { 1 });
                        true
                    }
                    Key::Character(text) if ctrl && !alt => match text.as_str() {
                        "f" | "F" => {
                            find.write().open = true;
                            true
                        }
                        "c" | "C" => {
                            copy();
                            true
                        }
                        "+" | "=" => {
                            zoom_step(1);
                            true
                        }
                        "-" | "_" => {
                            zoom_step(-1);
                            true
                        }
                        "0" => {
                            set_zoom(Zoom::FitWidth);
                            true
                        }
                        _ => false,
                    },
                    Key::Character(text) if !ctrl && !alt => match text.as_str() {
                        "+" | "=" => {
                            zoom_step(1);
                            true
                        }
                        "-" | "_" => {
                            zoom_step(-1);
                            true
                        }
                        "0" => {
                            set_zoom(Zoom::FitWidth);
                            true
                        }
                        "1" => {
                            set_zoom(Zoom::Percent(1.0));
                            true
                        }
                        "9" => {
                            set_zoom(Zoom::FitPage);
                            true
                        }
                        _ => false,
                    },
                    _ => false,
                };
                if handled {
                    event.stop_propagation();
                }
            }
        };

        // The wheel: Ctrl zooms, Shift moves sideways, the rest scrolls (the
        // scroll view's own). This handler is inside the scroll view, so it
        // runs first and can keep the wheel from scrolling when it zooms.
        let on_wheel = {
            let zoom_step = zoom_step.clone();
            let lay_width = lay.width;
            move |event: Event<WheelEventData>| {
                let modifiers = freya_winit::modifiers::live();
                if modifiers.contains(Modifiers::CONTROL) {
                    event.stop_propagation();
                    zoom_step(if event.delta_y < 0.0 { 1 } else { -1 });
                } else if modifiers.contains(Modifiers::SHIFT) {
                    event.stop_propagation();
                    let (vw, _) = *viewport.peek();
                    let room = (lay_width + 2.0 * MARGIN - vw).max(0.0);
                    let mut pan_x = pan_x;
                    let next = (*pan_x.peek() - (event.delta_y as f32).clamp(-120.0, 120.0)).clamp(0.0, room);
                    pan_x.set_if_modified(next);
                } else if event.delta_x != 0.0 {
                    let (vw, _) = *viewport.peek();
                    let room = (lay_width + 2.0 * MARGIN - vw).max(0.0);
                    let mut pan_x = pan_x;
                    let next = (*pan_x.peek() + event.delta_x as f32).clamp(0.0, room);
                    pan_x.set_if_modified(next);
                }
            }
        };

        // A drag over the pages selects words.
        let capture = use_capture({
            let caches = caches.clone();
            let lay = lay.clone();
            move |event: CaptureEvent| {
                let mut picked = picked;
                let mut version = version;
                let (vw, _) = *viewport.peek();
                let pan = *pan_x.peek();
                let local_y = event.local.1 - centre;
                let page = lay.page_at(local_y);
                let Some(&width) = lay.widths.get(page) else { return };
                let left = if width + 2.0 * MARGIN > vw { MARGIN - pan } else { (vw - width) / 2.0 };
                let x = (event.local.0 - left) / scale;
                let y = (local_y - lay.tops[page]) / scale;
                let at = caches.borrow().words.get(&page).and_then(|words| word_at(words, x, y)).map(|index| WordRef { page, index });
                match event.phase {
                    Phase::Start => {
                        moved.set(false);
                        keyboard.take();
                        picked.set(at.map(|at| (at, at)));
                    }
                    Phase::Move => {
                        moved.set(true);
                        let anchor = (*picked.peek()).map(|(anchor, _)| anchor);
                        if let (Some(at), Some(anchor)) = (at, anchor) {
                            picked.set_if_modified(Some((anchor, at)));
                        }
                    }
                    Phase::End => {
                        if !moved.get() {
                            picked.set_if_modified(None);
                        }
                        *version.write() += 1;
                    }
                    Phase::Cancel => {}
                }
            }
        });

        let toolbar_state = DocumentState {
            page: current,
            pages: now.pages,
            zoom: zoom.read().label(scale),
            find: find.read().open,
        };
        let on_action = EventHandler::new({
            let jump_to = jump_to.clone();
            let zoom_step = zoom_step.clone();
            let set_zoom = set_zoom.clone();
            move |action: DocumentAction| {
                let mut find = find;
                match action {
                    DocumentAction::Page(page) => jump_to(page as usize),
                    DocumentAction::ZoomIn => zoom_step(1),
                    DocumentAction::ZoomOut => zoom_step(-1),
                    DocumentAction::Zoom(choice) => set_zoom(choice.zoom()),
                    DocumentAction::Find => {
                        let open = find.peek().open;
                        find.write().open = !open;
                    }
                }
            }
        });
        let toolbar = DocumentControls::new(toolbar_state, page_text, on_action);

        let find_bar = find.read().open.then(|| {
            let state = find.read().clone();
            let search = search.clone();
            let status = find_status(state.active, state.hits.len(), state.searching, &query.read());
            let on_enter = EventHandler::new({
                let step_hit = step_hit.clone();
                move |modifiers: Modifiers| step_hit(if modifiers.contains(Modifiers::SHIFT) { -1 } else { 1 })
            });
            let prev = icon_button(Icon::GoPrevious, {
                let step_hit = step_hit.clone();
                move |_| step_hit(-1)
            }).tooltip("Previous match (Shift+Enter)").enabled(!state.hits.is_empty());
            let next = icon_button(Icon::GoNext, {
                let step_hit = step_hit.clone();
                move |_| step_hit(1)
            }).tooltip("Next match (Enter)").enabled(!state.hits.is_empty());
            let close = icon_button(Icon::WindowClose, move |_| {
                let mut find = find;
                find.write().open = false;
            })
            .tooltip("Close find (Escape)");
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), grow_children(), items(Align::Center), gap(S2), px(S2), py(S1), bg(Paint::Surface)])
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            search_field(query, move |text: String| search(text))
                                .placeholder("Find in document")
                                .dense()
                                .autofocus()
                                .fill_width()
                                .on_enter_modifiers(on_enter)
                                .on_escape(move |_| {
                                    let mut find = find;
                                    find.write().open = false;
                                }),
                        ),
                    )
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, status).max_lines(1))
                    .child(prev)
                    .child(next)
                    .child(close),
            )
        });

        // The column of pages: drawn ones carry their render, the rest hold the room.
        let selection_runs: Vec<(usize, usize, usize)> = (*picked.read())
            .map(|(a, b)| {
                let state = caches.borrow();
                selected_runs(a, b, |page| state.words.get(&page).map_or(0, Vec::len))
            })
            .unwrap_or_default();
        let active_hit = find.read().active;
        let hits_now = find.read().hits.clone();
        let rows: Vec<Element> = (0..lay.len())
            .map(|page| {
                let (width, height) = (lay.widths[page], lay.heights[page]);
                let row = rect().width(Size::fill()).height(Size::px(height)).key(page);
                if !visible.contains(&page) {
                    return Element::from(row);
                }
                let handle = caches.borrow_mut().renders.best(page, key).map(|(handle, _)| handle);
                let hits: Vec<(Rect, bool)> = hits_now.iter().enumerate().filter(|(_, (hit_page, _))| *hit_page == page).map(|(index, (_, rect))| (*rect, Some(index) == active_hit)).collect();
                let selected: Vec<Rect> = selection_runs
                    .iter()
                    .filter(|(run_page, _, _)| *run_page == page)
                    .flat_map(|(_, first, last)| caches.borrow().words.get(&page).map(|words| words[*first..=*last].iter().map(|word| word.rect).collect::<Vec<_>>()).unwrap_or_default())
                    .collect();
                let left = if width + 2.0 * MARGIN > vw { MARGIN - *pan_x.read() } else { (vw - width) / 2.0 };
                // ui-lint: allow(a page's left offset is computed from the viewport width and pan)
                Element::from(row.child(rect().margin(Gaps::new(0.0, 0.0, 0.0, left)).child(page_box(&tokens, (width, height), scale, handle, &hits, &selected))))
            })
            .collect();
        let column = capture.handlers(
            rect()
                .width(Size::fill())
                .on_wheel(on_wheel)
                .on_sized({
                    let capture = capture.clone();
                    move |event: Event<SizedEventData>| capture.sized(&event)
                })
                // ui-lint: allow(the page stack's end insets are the layout's measured margin plus the centring offset)
                .padding(Gaps::new(MARGIN + centre, 0.0, MARGIN + centre, 0.0))
                .children(rows),
        );
        // ui-lint: allow(the page gap is the document layout's measured geometry)
        let pages_view = ScrollView::new_controlled(scroll).width(Size::fill()).height(Size::fill()).spacing(GAP).child(column);

        let thumbs = (*sidebar.read() && !compact && matches!(now.load, Load::Ready)).then(|| {
            let thumbs = thumb_layout(sizes);
            let shown = thumbs.visible(-(<(i32, i32)>::from(side_scroll).1 as f32), side_viewport.read().1);
            let thumb_key = THUMB_KEY | ((THUMB_WIDTH * display as f32).round() as u32);
            let jump_to = jump_to.clone();
            let rows: Vec<Element> = (0..thumbs.len())
                .map(|page| {
                    let height = thumbs.heights[page];
                    let row = rect().width(Size::fill()).height(Size::px(height)).key(page);
                    if !shown.contains(&page) {
                        return Element::from(row);
                    }
                    let handle = caches.borrow_mut().renders.best(page, thumb_key).map(|(handle, _)| handle);
                    let on_current = page == current;
                    let jump_to = jump_to.clone();
                    let picture = {
                        let paper = rect()
                            .width(Size::px(THUMB_WIDTH))
                            .height(Size::px(height - THUMB_LABEL))
                            .tw(&tokens, &[bg(Paint::Knob), shadow(Elevation::Knob), clip()]);
                        match handle {
                            Some(handle) => paper.child(image(handle).width(Size::fill()).height(Size::fill()).aspect_ratio(AspectRatio::None)),
                            None => paper,
                        }
                    };
                    Element::from(
                        row.tw(&tokens, &[flex_col(), items(Align::Center), gap(S1)])
                            .on_press(move |_| jump_to(page))
                            .child(rect().tw(&tokens, &[rounded(Radius::Inner), if on_current { bg(Paint::Selection) } else { bg(Paint::Transparent) }, p(S1)]).child(picture))
                            .child(CAPTION.in_ink(if on_current { Ink::Text } else { Ink::Muted }).label(&tokens, (page + 1).to_string()).max_lines(1)),
                    )
                })
                .collect();
            Element::from(
                rect()
                    .width(Size::px(SIDEBAR_WIDTH))
                    .height(Size::fill())
                    .tw(&tokens, &[bg(Paint::Sidebar), flex_row()])
                    .on_sized(move |event: Event<SizedEventData>| {
                        let mut side_viewport = side_viewport;
                        side_viewport.set_if_modified((event.area.width(), event.area.height()));
                    })
                    // ui-lint: allow(ScrollView takes a plain spacing and has no tw; the insets are the thumb layout's measured geometry)
                    .child(ScrollView::new_controlled(side_scroll).width(Size::fill()).height(Size::fill()).spacing(8.0).child(rect().width(Size::fill()).padding(Gaps::new(S4_PX, 0.0, S4_PX, 0.0)).children(rows)))
                    .child(rect().tw(&tokens, &[w(Dim::Hairline), h_full(), bg(Paint::Separator)])),
            )
        });

        let body: Element = match &now.load {
            Load::Failed { code, message } => {
                let retry = matches!(code, ErrorCode::Internal | ErrorCode::NotFound).then(|| self.on_retry.clone());
                // The retry is a button of the card: the card is as tall as the stage, so a button under it was cut off.
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Card)])
                    .child(Card::of(&self.resolved, &self.nav).noted(message.clone()).retrying(retry))
                    .into()
            }
            Load::Opening => rect()
                .tw(&tokens, &[w_full(), h_full(), flex_col(), center(), bg(Paint::Recess)])
                .child(spinner(SpinnerSize::Medium))
                .into(),
            Load::Ready => rect()
                .tw(&tokens, &[w_full(), h_full(), flex_row(), grow_children(), bg(Paint::Recess)])
                .maybe_child(thumbs)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .height(Size::fill())
                        .on_sized(move |event: Event<SizedEventData>| {
                            let mut viewport = viewport;
                            viewport.set_if_modified((event.area.width(), event.area.height()));
                        })
                        .child(pages_view),
                )
                .into(),
        };

        rect()
            .component("PreviewDocument")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children(), clip()])
            .on_global_key_down(on_key)
            .maybe_child(matches!(now.load, Load::Ready).then(|| Element::from(toolbar)))
            .maybe_child(find_bar)
            .child(rect().width(Size::fill()).height(Size::flex(1.0)).child(body))
    }
}

/// Thumbnail sizes of the pages, as a layout: each is the sidebar's width wide
/// and as tall as its page, plus a line for the number.
fn thumb_layout(sizes: &[PageSize]) -> Layout {
    let scaled: Vec<PageSize> = sizes.iter().map(|(w, h)| (THUMB_WIDTH, THUMB_WIDTH * h / w.max(1.0) + THUMB_LABEL)).collect();
    layout(&scaled, 1.0)
}
