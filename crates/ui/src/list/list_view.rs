//! The list view: rows of one kind, selectable, in sections, reorderable.

use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::model::{Entry, end_item, flatten, next_item};
use super::row::{ListRow, SwipeAction};
use crate::component::ComponentName;
use crate::controls::direction::{Direction, use_direction};
use crate::controls::press::{is_touch, point_of, use_press};
use crate::controls::sortable::sortable;
use crate::icons::{self, Icon};
use crate::motion::{MOVE_MS, use_tween};
use crate::selection::{Gesture, Selection, SelectionMode};
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::{MenuItem, Overlays, menu_item, menu_separator, status_page};
use crate::style::*;
use crate::surface::use_tokens;
use crate::table::{CellIds, Rows, find_prefix, offset_to_show};
use crate::typography::{BODY, CAPTION, Ink};

/// The width of a swiped action's button.
const ACTION_WIDTH: f32 = 76.0;
/// How far a finger travels sideways before the row follows it.
const SWIPE_SLOP: f32 = 8.0;
const DOUBLE_PRESS: Duration = Duration::from_millis(400);
const TYPE_AHEAD: Duration = Duration::from_millis(1000);

type Build<T> = Rc<dyn Fn(&T) -> ListRow>;
type SectionOf<T> = Rc<dyn Fn(&T) -> Option<String>>;
type MenuFn<K> = Rc<dyn Fn(&[K]) -> Vec<MenuItem>>;

/// The lines of the list: the rows as they are, or with section headers.
#[derive(Clone)]
enum Lines {
    Natural(usize),
    Sectioned(Rc<(Vec<Entry>, Vec<String>)>),
}

impl PartialEq for Lines {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Lines::Natural(a), Lines::Natural(b)) => a == b,
            (Lines::Sectioned(a), Lines::Sectioned(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Lines {
    fn len(&self) -> usize {
        match self {
            Lines::Natural(len) => *len,
            Lines::Sectioned(lines) => lines.0.len(),
        }
    }

    fn entry(&self, display: usize) -> Option<Entry> {
        match self {
            Lines::Natural(len) => (display < *len).then_some(Entry::Item(display)),
            Lines::Sectioned(lines) => lines.0.get(display).copied(),
        }
    }

    fn title(&self, index: usize) -> String {
        match self {
            Lines::Sectioned(lines) => lines.1.get(index).cloned().unwrap_or_default(),
            Lines::Natural(_) => String::new(),
        }
    }

    /// The nearest row from `from` going `delta` over the lines.
    fn step(&self, from: usize, delta: i32) -> Option<usize> {
        match self {
            Lines::Natural(len) => {
                let to = from as i32 + delta;
                (to >= 0 && (to as usize) < *len).then_some(to as usize)
            }
            Lines::Sectioned(lines) => next_item(&lines.0, from, delta),
        }
    }

    fn end(&self, last: bool) -> Option<usize> {
        match self {
            Lines::Natural(len) => (*len > 0).then(|| if last { len - 1 } else { 0 }),
            Lines::Sectioned(lines) => end_item(&lines.0, last),
        }
    }
}

/// A list of `rows`. Build it with [`list_view`].
pub struct ListView<T, K> {
    rows: Rows<T>,
    build: Build<T>,
    key: Rc<dyn Fn(&T) -> K>,
    selection: Option<State<Selection<K>>>,
    mode: SelectionMode,
    sections: Option<SectionOf<T>>,
    two_line: bool,
    marks: bool,
    menu: Option<MenuFn<K>>,
    on_activate: Option<EventHandler<K>>,
    on_reorder: Option<EventHandler<(usize, usize)>>,
    empty: Option<Element>,
    label: String,
}

impl<T, K: 'static> PartialEq for ListView<T, K> {
    fn eq(&self, other: &Self) -> bool {
        fn same<F: ?Sized>(a: &Option<Rc<F>>, b: &Option<Rc<F>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
        }
        self.rows == other.rows
            && Rc::ptr_eq(&self.build, &other.build)
            && Rc::ptr_eq(&self.key, &other.key)
            && self.selection == other.selection
            && self.mode == other.mode
            && same(&self.sections, &other.sections)
            && self.two_line == other.two_line
            && self.marks == other.marks
            && same(&self.menu, &other.menu)
            && self.on_activate == other.on_activate
            && self.on_reorder == other.on_reorder
            && self.empty == other.empty
            && self.label == other.label
    }
}

/// A list of `rows`, each drawn by `row`, identified by `key`.
///
/// The rows come from a [`Rows`] source and only those in view are built, so a
/// list of a hundred thousand scrolls as smoothly as one of ten. Every row has
/// the same height; see [`ListView::two_line`].
///
/// ```ignore
/// list_view(&contacts, |c: &Contact| list_row(c.name.clone()).icon(Icon::Avatar), |c| c.id)
///     .selection_mode(SelectionMode::Single)
///     .selection(selected)
///     .sections(|c| Some(c.initial.to_string()))
/// ```
pub fn list_view<T: 'static, K: Clone + Eq + Hash + 'static>(
    rows: impl Into<Rows<T>>,
    row: impl Fn(&T) -> ListRow + 'static,
    key: impl Fn(&T) -> K + 'static,
) -> ListView<T, K> {
    ListView {
        rows: rows.into(),
        build: Rc::new(row),
        key: Rc::new(key),
        selection: None,
        mode: SelectionMode::None,
        sections: None,
        two_line: false,
        marks: false,
        menu: None,
        on_activate: None,
        on_reorder: None,
        empty: None,
        label: String::new(),
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> ListView<T, K> {
    /// The selection, owned by the caller. Without it the list keeps its own.
    pub fn selection(mut self, selection: State<Selection<K>>) -> Self {
        self.selection = Some(selection);
        self
    }

    /// How many rows can be selected. A list starts with none: a press on a row
    /// opens it. With `Single` or `Multiple` a press selects, and a second one
    /// (or Enter) opens.
    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Groups the rows under headers. `section` gives a row's section title, or
    /// `None` for a row with no header. Rows of one section must be next to each
    /// other: a header goes before each run. A reorderable list has no
    /// sections.
    pub fn sections(mut self, section: impl Fn(&T) -> Option<String> + 'static) -> Self {
        self.sections = Some(Rc::new(section));
        self
    }

    /// Rows are as tall as a title and a subtitle need, and show the
    /// subtitle.
    pub fn two_line(mut self) -> Self {
        self.two_line = true;
        self
    }

    /// A selection mark at the start of each row, ticked when it is selected.
    /// A phone shows marks by itself while rows are selected.
    pub fn marks(mut self) -> Self {
        self.marks = true;
        self
    }

    /// The menu of a row: a right click, a long press, the Menu key. It is given
    /// the keys it acts on: the whole selection when the row is in it, else the
    /// row. The row's own actions are added after it.
    pub fn row_menu(mut self, menu: impl Fn(&[K]) -> Vec<MenuItem> + 'static) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }

    /// A row was opened: a press, or a double press when rows are selectable;
    /// Enter; a tap on a phone.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<K>>) -> Self {
        self.on_activate = Some(handler.into());
        self
    }

    /// Makes the list reorderable. Each row gets a grip (drag it, or focus it
    /// and press Up and Down), and `handler(from, to)` is called once when a row
    /// has been moved; the caller removes the row at `from` and inserts it at
    /// `to` (see [`crate::reorder`]). A reorderable list builds all its rows, is
    /// not virtual and has no sections or selection: use it for a few dozen
    /// rows, such as a queue.
    pub fn on_reorder(mut self, handler: impl Into<EventHandler<(usize, usize)>>) -> Self {
        self.on_reorder = Some(handler.into());
        self
    }

    /// What shows when there are no rows.
    pub fn empty_state(mut self, empty: impl Into<Element>) -> Self {
        self.empty = Some(empty.into());
        self
    }

    /// The list's name for assistive technology.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

struct Ctx<T, K> {
    rows: Rows<T>,
    build: Build<T>,
    key: Rc<dyn Fn(&T) -> K>,
    lines: Lines,
    mode: SelectionMode,
    selection: State<Selection<K>>,
    cursor: State<Option<usize>>,
    /// The line whose row is swiped open.
    swiped: State<Option<usize>>,
    a11y_id: AccessibilityId,
    overlays: Option<Overlays>,
    direction: Direction,
    scroll: ScrollController,
    viewport: State<Area>,
    pitch: f32,
    menu: Option<MenuFn<K>>,
    on_activate: Option<EventHandler<K>>,
    typed: Rc<RefCell<(String, Option<Instant>)>>,
    last_press: Rc<Cell<Option<(Instant, usize)>>>,
    ids: CellIds,
}

struct Latest<T, K>(Rc<RefCell<Option<Rc<Ctx<T, K>>>>>);

impl<T, K> Clone for Latest<T, K> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, K> PartialEq for Latest<T, K> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T, K> Latest<T, K> {
    fn get(&self) -> Rc<Ctx<T, K>> {
        self.0
            .borrow()
            .clone()
            .expect("the list sets its context before it draws a row")
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Ctx<T, K> {
    /// The row at line `display`, when the line is a row.
    fn row(&self, display: usize) -> Option<&T> {
        match self.lines.entry(display)? {
            Entry::Item(at) => self.rows.get(at),
            Entry::Header(_) => None,
        }
    }

    fn key_at(&self, display: usize) -> Option<K> {
        self.row(display).map(|row| (self.key)(row))
    }

    fn spec(&self, display: usize) -> Option<ListRow> {
        self.row(display).map(|row| (self.build)(row))
    }

    fn enabled(&self, display: usize) -> bool {
        self.spec(display).is_some_and(|spec| spec.enabled)
    }

    fn anchor_at(&self) -> Option<usize> {
        let selection = self.selection.peek();
        let anchor = selection.anchor()?;
        (0..self.lines.len()).find(|&at| self.key_at(at).as_ref() == Some(anchor))
    }

    fn selecting(&self) -> bool {
        !self.selection.peek().is_empty()
    }

    fn select(&self, display: usize, gesture: Gesture) -> bool {
        if !self.enabled(display) {
            return false;
        }
        let anchor = match gesture {
            Gesture::Extend | Gesture::ExtendAdd => self.anchor_at(),
            _ => None,
        };
        let mut next = self.selection.peek().clone();
        // A range counts the rows between, not the headers: the lines in
        // between that are rows, in order.
        let rows: Vec<usize> = (0..self.lines.len())
            .filter(|&at| matches!(self.lines.entry(at), Some(Entry::Item(_))))
            .collect();
        let place = |line: usize| rows.iter().position(|&at| at == line);
        let (Some(here), anchor) = (place(display), anchor.and_then(place)) else {
            return false;
        };
        let changed = next.apply(
            self.mode,
            gesture,
            here,
            |at| rows.get(at).and_then(|&line| self.key_at(line)),
            anchor,
        );
        if changed {
            let mut selection = self.selection;
            selection.set(next);
        }
        changed
    }

    fn select_all(&self) {
        if self.mode != SelectionMode::Multiple {
            return;
        }
        let mut next = self.selection.peek().clone();
        next.select_all((0..self.lines.len()).filter_map(|at| self.key_at(at)));
        let mut selection = self.selection;
        selection.set(next);
    }

    fn clear_selection(&self) {
        if self.selecting() {
            let mut next = self.selection.peek().clone();
            next.clear();
            let mut selection = self.selection;
            selection.set(next);
        }
    }

    fn selected_keys(&self) -> Vec<K> {
        let selection = self.selection.peek();
        (0..self.lines.len())
            .filter_map(|at| self.key_at(at))
            .filter(|key| selection.contains(key))
            .collect()
    }

    fn keys_for(&self, display: usize) -> Vec<K> {
        let Some(key) = self.key_at(display) else {
            return Vec::new();
        };
        if self.selection.peek().contains(&key) {
            self.selected_keys()
        } else {
            self.select(display, Gesture::Replace);
            vec![key]
        }
    }

    fn set_cursor(&self, cursor: Option<usize>) {
        let mut state = self.cursor;
        if *state.peek() != cursor {
            state.set(cursor);
        }
    }

    fn close_swipe(&self) -> bool {
        let mut swiped = self.swiped;
        let was = swiped.peek().is_some();
        if was {
            swiped.set(None);
        }
        was
    }

    fn activate(&self, display: usize) {
        if !self.enabled(display) {
            return;
        }
        if let (Some(on_activate), Some(key)) = (&self.on_activate, self.key_at(display)) {
            on_activate.call(key);
        }
    }

    fn press_down(&self, display: usize, held: Modifiers) {
        self.a11y_id.request_focus();
        self.close_swipe();
        self.set_cursor(Some(display));
        if self.mode != SelectionMode::None {
            self.select(display, Gesture::from_modifiers(held));
        }
    }

    fn press_up(&self, display: usize, held: Modifiers) {
        if self.mode == SelectionMode::None {
            if Gesture::from_modifiers(held) == Gesture::Replace {
                self.activate(display);
            }
            return;
        }
        let now = Instant::now();
        let double = matches!(
            self.last_press.get(),
            Some((at, line)) if line == display && now.duration_since(at) < DOUBLE_PRESS
        );
        if double && Gesture::from_modifiers(held) == Gesture::Replace {
            self.last_press.set(None);
            self.activate(display);
        } else {
            self.last_press.set(Some((now, display)));
        }
    }

    fn tap(&self, display: usize) {
        if self.close_swipe() {
            return;
        }
        self.set_cursor(Some(display));
        if self.mode == SelectionMode::Multiple && self.selecting() {
            self.select(display, Gesture::Toggle);
        } else if self.on_activate.is_some() {
            self.activate(display);
        } else if self.mode != SelectionMode::None {
            self.select(display, Gesture::Replace);
        }
    }

    fn long_press(&self, display: usize, at: (f32, f32)) {
        if self.mode == SelectionMode::Multiple && self.menu.is_none() {
            self.select(display, Gesture::Toggle);
        } else {
            self.open_menu(display, PanelTrigger::Pointer(at), false);
        }
    }

    fn open_menu(&self, display: usize, trigger: PanelTrigger, keyboard: bool) {
        let Some(overlays) = self.overlays else {
            return;
        };
        let Some(spec) = self.spec(display) else {
            return;
        };
        let keys = if self.mode == SelectionMode::None {
            self.key_at(display).into_iter().collect()
        } else {
            self.keys_for(display)
        };
        let mut items = self.menu.as_ref().map_or_else(Vec::new, |menu| menu(&keys));
        if !spec.swipe.is_empty() {
            if !items.is_empty() {
                items.push(menu_separator());
            }
            for action in &spec.swipe {
                let on_press = action.on_press.clone();
                let mut item = menu_item(action.label.clone(), move |()| on_press.call(()));
                if let Some(icon) = action.icon {
                    item = item.icon(icon);
                }
                if action.destructive {
                    item = item.destructive();
                }
                items.push(item);
            }
        }
        if !items.is_empty() {
            overlays.show_menu_from(Some(self.a11y_id), trigger, items, keyboard);
        }
    }

    fn move_to(&self, to: usize, gesture: Option<Gesture>) {
        self.set_cursor(Some(to));
        let current = <(i32, i32)>::from(self.scroll).1;
        let offset = offset_to_show(
            to,
            current,
            self.lines.len(),
            self.viewport.peek().height(),
            self.pitch,
        );
        if offset != current {
            let mut scroll = self.scroll;
            scroll.scroll_to_y(offset);
        }
        if let Some(gesture) = gesture {
            self.select(to, gesture);
        }
    }

    fn page(&self) -> usize {
        ((self.viewport.peek().height() / self.pitch) as usize).max(1)
    }

    fn start_cursor(&self) -> Option<usize> {
        (0..self.lines.len())
            .find(|&at| {
                self.key_at(at)
                    .is_some_and(|key| self.selection.peek().contains(&key))
            })
            .or_else(|| self.lines.end(false))
    }

    /// The row's menu from the keyboard, against the row's place on screen.
    fn menu_at_cursor(&self, cursor: Option<usize>) -> bool {
        let Some(at) = cursor else {
            return false;
        };
        let view = self.viewport.peek().clone();
        let scrolled = <(i32, i32)>::from(self.scroll).1 as f32;
        let area = Area::new(
            (
                view.min_x() + 24.0,
                view.min_y() + at as f32 * self.pitch + scrolled,
            )
                .into(),
            (view.width().min(240.0), self.pitch).into(),
        );
        self.open_menu(
            at,
            PanelTrigger::Keyboard(PanelRect::from_area(&area)),
            true,
        );
        true
    }

    fn typed_text(&self) -> String {
        let typed = self.typed.borrow();
        match typed.1 {
            Some(at) if at.elapsed() < TYPE_AHEAD => typed.0.clone(),
            _ => String::new(),
        }
    }

    fn key_down(&self, event: &Event<KeyboardEventData>) -> bool {
        let held = event.modifiers;
        let shift = held.contains(Modifiers::SHIFT);
        let ctrl = held.contains(Modifiers::CONTROL) || held.contains(Modifiers::META);
        let alt = held.contains(Modifiers::ALT);
        let cursor = (*self.cursor.peek())
            .filter(|&at| at < self.lines.len())
            .or_else(|| self.start_cursor());
        let key = self.direction.reading_key(&event.key);
        let gesture = match Gesture::from_modifiers(held) {
            Gesture::Toggle => None,
            gesture => Some(gesture),
        };
        if let Key::Named(named) = &key {
            let to = match (named, cursor) {
                (NamedKey::ArrowDown, Some(at)) => Some(self.lines.step(at, 1).unwrap_or(at)),
                (NamedKey::ArrowUp, Some(at)) => Some(self.lines.step(at, -1).unwrap_or(at)),
                (NamedKey::ArrowDown | NamedKey::ArrowUp, None) => self.lines.end(false),
                (NamedKey::PageDown, Some(at)) => {
                    Some((0..self.page()).fold(at, |at, _| self.lines.step(at, 1).unwrap_or(at)))
                }
                (NamedKey::PageUp, Some(at)) => {
                    Some((0..self.page()).fold(at, |at, _| self.lines.step(at, -1).unwrap_or(at)))
                }
                (NamedKey::Home, _) => self.lines.end(false),
                (NamedKey::End, _) => self.lines.end(true),
                _ => None,
            };
            if let Some(to) = to {
                if alt {
                    return false;
                }
                self.move_to(to, gesture);
                return true;
            }
            match named {
                NamedKey::Enter => {
                    if let Some(at) = cursor {
                        self.activate(at);
                        return true;
                    }
                }
                NamedKey::Escape => {
                    if self.close_swipe() || self.selecting() {
                        self.clear_selection();
                        return true;
                    }
                }
                NamedKey::ContextMenu => return self.menu_at_cursor(cursor),
                NamedKey::F10 if shift => return self.menu_at_cursor(cursor),
                _ => {}
            }
        }
        if let Key::Character(text) = &key {
            if ctrl && !alt && text.eq_ignore_ascii_case("a") {
                self.select_all();
                return true;
            }
            if alt || ctrl && text != " " {
                return false;
            }
            if text == " " && self.typed_text().is_empty() {
                if let Some(at) = cursor {
                    let gesture = match Gesture::from_modifiers(held) {
                        Gesture::Replace if self.mode == SelectionMode::Multiple => Gesture::Toggle,
                        gesture => gesture,
                    };
                    self.select(at, gesture);
                    return true;
                }
                return false;
            }
            if text.chars().any(char::is_control) || self.lines.len() == 0 {
                return false;
            }
            let mut typed = self.typed_text();
            let fresh = typed.is_empty();
            typed.push_str(text);
            *self.typed.borrow_mut() = (typed.clone(), Some(Instant::now()));
            let from = cursor.map_or(0, |at| if fresh { at + 1 } else { at });
            let found = find_prefix(&typed, from, self.lines.len(), |at| {
                self.spec(at).map(|spec| spec.title).unwrap_or_default()
            });
            if let Some(at) = found {
                self.move_to(at, Some(Gesture::Replace));
            }
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

struct LinesCache {
    rows: usize,
    len: usize,
    sections: usize,
    lines: Rc<(Vec<Entry>, Vec<String>)>,
}

/// What every visible line is given.
struct LinesData<T, K> {
    latest: Latest<T, K>,
    rows: Rows<T>,
    build: Build<T>,
    key: Rc<dyn Fn(&T) -> K>,
    lines: Lines,
    selection: Selection<K>,
    cursor: Option<usize>,
    swiped: Option<usize>,
    pitch: f32,
    compact: bool,
    two_line: bool,
    marks: bool,
    picking: bool,
    activates: bool,
}

impl<T, K: Clone> Clone for LinesData<T, K> {
    fn clone(&self) -> Self {
        Self {
            latest: self.latest.clone(),
            rows: self.rows.clone(),
            build: self.build.clone(),
            key: self.key.clone(),
            lines: self.lines.clone(),
            selection: self.selection.clone(),
            cursor: self.cursor,
            swiped: self.swiped,
            pitch: self.pitch,
            compact: self.compact,
            two_line: self.two_line,
            marks: self.marks,
            picking: self.picking,
            activates: self.activates,
        }
    }
}

impl<T, K: Eq + Hash> PartialEq for LinesData<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && Rc::ptr_eq(&self.build, &other.build)
            && Rc::ptr_eq(&self.key, &other.key)
            && self.lines == other.lines
            && self.selection == other.selection
            && self.cursor == other.cursor
            && self.swiped == other.swiped
            && self.pitch == other.pitch
            && self.compact == other.compact
            && self.two_line == other.two_line
            && self.marks == other.marks
            && self.picking == other.picking
            && self.activates == other.activates
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for ListView<T, K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = crate::style::is_compact();
        let direction = use_direction();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let overlays = use_try_consume::<Overlays>();
        let own_selection = use_state(Selection::<K>::new);
        let selection = self.selection.unwrap_or(own_selection);
        // A change of the selection is read out ("3 selected").
        crate::a11y::use_collection_announcements(selection.read().len(), None);
        let cursor = use_state(|| None::<usize>);
        let swiped = use_state(|| None::<usize>);
        let typed = use_hook(|| Rc::new(RefCell::new((String::new(), None))));
        let last_press = use_hook(|| Rc::new(Cell::new(None)));
        let latest: Rc<RefCell<Option<Rc<Ctx<T, K>>>>> = use_hook(|| Rc::new(RefCell::new(None)));
        let latest = Latest(latest);
        let cache = use_hook(|| Rc::new(RefCell::new(None::<LinesCache>)));
        let ids = use_hook(CellIds::default);

        let pitch = if self.two_line {
            Dim::RowSubtitle.px()
        } else {
            Dim::NavItem.px()
        };
        let reorder = self.on_reorder.clone();

        let lines = match (&self.sections, reorder.is_some()) {
            (Some(section), false) => {
                let id = Rc::as_ptr(section) as *const () as usize;
                let rows = &self.rows;
                let reuse = cache
                    .borrow()
                    .as_ref()
                    .filter(|last| {
                        last.rows == rows.id() && last.len == rows.len() && last.sections == id
                    })
                    .map(|last| last.lines.clone());
                let flat = reuse.unwrap_or_else(|| {
                    let flat = Rc::new(flatten(rows.len(), |at| {
                        rows.get(at).and_then(|row| section(row))
                    }));
                    *cache.borrow_mut() = Some(LinesCache {
                        rows: rows.id(),
                        len: rows.len(),
                        sections: id,
                        lines: flat.clone(),
                    });
                    flat
                });
                Lines::Sectioned(flat)
            }
            _ => Lines::Natural(self.rows.len()),
        };
        let count = lines.len();
        let mode = if reorder.is_some() {
            SelectionMode::None
        } else {
            self.mode
        };

        latest.0.replace(Some(Rc::new(Ctx {
            rows: self.rows.clone(),
            build: self.build.clone(),
            key: self.key.clone(),
            lines: lines.clone(),
            mode,
            selection,
            cursor,
            swiped,
            a11y_id,
            overlays,
            direction,
            scroll,
            viewport,
            pitch,
            menu: self.menu.clone(),
            on_activate: self.on_activate.clone(),
            typed,
            last_press,
            ids: ids.clone(),
        })));

        let keyboard = focus() == Focus::Keyboard;
        use_side_effect_with_deps(&keyboard, {
            let latest = latest.clone();
            move |&keyboard: &bool| {
                if !keyboard {
                    return;
                }
                let ctx = latest.get();
                if ctx.cursor.peek().is_none() {
                    let mut state = ctx.cursor;
                    state.set(ctx.start_cursor());
                }
            }
        });

        let held = selection.read().clone();
        let shown_cursor = cursor.read().filter(|&at| keyboard && at < count);
        let swiped_now = *swiped.read();

        if count == 0 {
            return match &self.empty {
                Some(empty) => empty.clone(),
                None => status_page(Icon::ViewList, "Nothing to show").into(),
            };
        }

        // A list that is reordered is a card of rows that slide as one is dragged.
        if let Some(on_reorder) = reorder {
            let mut items = Vec::new();
            let mut labels = Vec::new();
            for at in 0..self.rows.len() {
                let Some(row) = self.rows.get(at) else {
                    continue;
                };
                let spec = (self.build)(row);
                labels.push(spec.title.clone());
                items.push(Element::from(StaticRow {
                    spec,
                    height: pitch,
                    two_line: self.two_line,
                }));
            }
            return sortable(items, on_reorder).labels(labels).into_element();
        }

        let data = LinesData {
            latest: latest.clone(),
            rows: self.rows.clone(),
            build: self.build.clone(),
            key: self.key.clone(),
            lines: lines.clone(),
            selection: held.clone(),
            cursor: shown_cursor,
            swiped: swiped_now,
            pitch,
            compact,
            two_line: self.two_line,
            marks: self.marks && !compact,
            picking: compact && !held.is_empty(),
            activates: self.on_activate.is_some(),
        };
        let label = self.label.clone();
        let active = cursor
            .read()
            .filter(|&at| focus() != Focus::Not && at < count)
            .map(|at| ids.get(at, 0));
        let list = VirtualScrollView::new_with_data_controlled(
            data,
            |item, data: &LinesData<T, K>| {
                let display = item.index;
                let Some(entry) = data.lines.entry(display) else {
                    return rect().key(display).into();
                };
                match entry {
                    Entry::Header(index) => Element::from(SectionHeader {
                        title: data.lines.title(index),
                        display,
                        height: data.pitch,
                    }),
                    Entry::Item(at) => {
                        let Some(row) = data.rows.get(at) else {
                            return rect().key(display).into();
                        };
                        let key = (data.key)(row);
                        Element::from(ListLine {
                            latest: data.latest.clone(),
                            rows: data.rows.clone(),
                            build: data.build.clone(),
                            display,
                            at,
                            selected: data.selection.contains(&key),
                            cursor: data.cursor == Some(display),
                            swiped: data.swiped == Some(display),
                            height: data.pitch,
                            compact: data.compact,
                            two_line: data.two_line,
                            marks: data.marks,
                            picking: data.picking,
                            activates: data.activates,
                        })
                    }
                }
            },
            scroll,
        )
        .length(count)
        .item_size(pitch)
        .scroll_with_arrows(false)
        .drag_scrolling(compact)
        .initial_size(viewport.peek().size)
        .on_sized(move |event: Event<SizedEventData>| viewport.set_if_modified(event.area));

        rect()
            .component("ListView")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::ListBox)
            .a11y_alt(label)
            .a11y_builder(move |node| {
                if mode == SelectionMode::Multiple {
                    node.set_multiselectable();
                }
                if let Some(active) = active {
                    node.set_active_descendant(active);
                }
            })
            .tw(&tokens, &[flex_col(), w_full(), h_full()])
            .on_key_down({
                let latest = latest.clone();
                move |event: Event<KeyboardEventData>| {
                    if latest.get().key_down(&event) {
                        event.stop_propagation();
                        event.prevent_default();
                    }
                }
            })
            .child(list)
            .into_element()
    }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
struct SectionHeader {
    title: String,
    display: usize,
    height: f32,
}

impl Component for SectionHeader {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.display as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .component("SectionHeader")
            .a11y_role(AccessibilityRole::Heading)
            .a11y_alt(self.title.clone())
            .height(Size::px(self.height))
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::End),
                    w_full(),
                    px(LIST_EDGE),
                    pb(S1),
                ],
            )
            .child(
                rect().tw(&tokens, &[px(FILE_ROW_INSET), px(S2)]).child(
                    CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, self.title.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
    }
}

/// The glyph, title and subtitle of a row, and its value and chevron.
fn row_content(
    tokens: &crate::theme::Tokens,
    spec: &ListRow,
    two_line: bool,
    chevron: bool,
) -> Element {
    let lead: Option<Element> = match (&spec.leading, spec.icon) {
        (Some(element), _) => Some(element.clone()),
        (None, Some(icon)) => Some(
            rect()
                .tw(tokens, &[flex_row(), center(), w(Dim::Box7), h(Dim::Box7)])
                .child(icons::icon(icon, IconSize::Row, Ink::Muted.color(tokens)))
                .into(),
        ),
        _ => None,
    };
    let mut text = rect()
        .tw(
            tokens,
            &[flex_col(), w_flex(), justify(Align::Center), gap(S0_5)],
        )
        .child(
            BODY.in_ink(Ink::Text)
                .label(tokens, spec.title.clone())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        );
    if let (true, Some(subtitle)) = (two_line, &spec.subtitle) {
        text = text.child(
            CAPTION
                .label(tokens, subtitle.clone())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        );
    }
    let mut line = rect()
        .tw(
            tokens,
            &[
                flex_row(),
                items(Align::Center),
                grow_children(),
                w_full(),
                h_full(),
                gap(S2),
                px(S2),
            ],
        )
        .maybe_child(lead)
        .child(text);
    if let Some(value) = &spec.value {
        line = line.child(CAPTION.label(tokens, value.clone()).max_lines(1));
    }
    if chevron || spec.chevron {
        line = line.child(icons::icon(
            Icon::PanEnd,
            IconSize::Mark,
            Ink::Muted.color(tokens),
        ));
    }
    line.into()
}

/// A row of a reorderable list: drawn flush, the card is the sortable's.
#[derive(PartialEq)]
struct StaticRow {
    spec: ListRow,
    height: f32,
    two_line: bool,
}

impl Component for StaticRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .height(Size::px(self.height))
            .tw(&tokens, &[w_full(), pr(S0)])
            .child(row_content(&tokens, &self.spec, self.two_line, false))
    }
}

/// One row, with its press, hover, swipe and selection. Equal rows are not
/// drawn again.
struct ListLine<T, K> {
    latest: Latest<T, K>,
    rows: Rows<T>,
    build: Build<T>,
    display: usize,
    at: usize,
    selected: bool,
    cursor: bool,
    swiped: bool,
    height: f32,
    compact: bool,
    two_line: bool,
    marks: bool,
    picking: bool,
    activates: bool,
}

impl<T, K> PartialEq for ListLine<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && Rc::ptr_eq(&self.build, &other.build)
            && self.display == other.display
            && self.at == other.at
            && self.selected == other.selected
            && self.cursor == other.cursor
            && self.swiped == other.swiped
            && self.height == other.height
            && self.compact == other.compact
            && self.two_line == other.two_line
            && self.marks == other.marks
            && self.picking == other.picking
            && self.activates == other.activates
    }
}

/// A finger on a row that may swipe.
#[derive(Clone, Copy, PartialEq)]
struct Swipe {
    start: (f32, f32),
    base: f32,
    active: bool,
    live: f32,
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for ListLine<T, K> {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.display as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut swipe = use_state(|| None::<Swipe>);
        let press = use_press();
        let display = self.display;
        let Some(row) = self.rows.get(self.at) else {
            return rect().into_element();
        };
        let spec = (self.build)(row);
        let selected = self.selected;
        let enabled = spec.enabled;
        let can_swipe = self.compact && !spec.swipe.is_empty();
        let reach = ACTION_WIDTH * spec.swipe.len() as f32;
        let target = if self.swiped { -reach } else { 0.0 };
        let settled = use_tween(target, tokens.motion(MOVE_MS));
        let offset = match *swipe.read() {
            Some(drag) if drag.active => drag.live,
            _ => settled,
        };
        let touch_pick = self.compact && (self.picking || selected);
        let show_mark = self.marks || touch_pick;

        let mut line = rect().tw(
            &tokens,
            &[
                flex_row(),
                items(Align::Center),
                grow_children(),
                w_full(),
                h_full(),
                px(FILE_ROW_INSET),
                rounded(Radius::Control),
                surface(
                    Paint::Transparent,
                    hovering() && enabled,
                    pressing() && enabled,
                    selected,
                ),
            ],
        );
        if show_mark {
            line = line.child(
                rect()
                    .tw(&tokens, &[flex_row(), center(), mr(S1), w(Dim::Box7)])
                    .child(crate::table::selection_mark(&tokens, selected)),
            );
        }
        line = line
            .maybe(self.cursor, |line| {
                line.tw(
                    &tokens,
                    &[border(Line::Stroke(Dim::Ring, Paint::FocusRing))],
                )
            })
            .child(row_content(
                &tokens,
                &spec,
                self.two_line,
                self.activates && self.compact && !show_mark,
            ));

        let actions = can_swipe.then(|| {
            let buttons: Vec<Element> = spec
                .swipe
                .iter()
                .map(|action| swipe_button(&tokens, action, self.latest.clone(), self.height))
                .collect();
            rect()
                .position(Position::new_absolute().top(0.0).right(0.0))
                .height(Size::px(self.height))
                .tw(&tokens, &[flex_row(), py(S1), pr(S2)])
                .children(buttons)
        });

        let latest = self.latest.clone();
        let content = rect()
            .tw(&tokens, &[w_full(), h_full()])
            .offset_x(offset)
            .maybe(!enabled, |content| content.opacity(DISABLED_OPACITY))
            .child(line);

        rect()
            .component("ListLine")
            .height(Size::px(self.height))
            .tw(&tokens, &[w_full(), px(LIST_EDGE), clip()])
            .a11y_id(self.latest.get().ids.get(display, 0))
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(spec.title.clone())
            .a11y_builder(move |node| {
                node.set_selected(selected);
                node.set_position_in_set(display);
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down({
                let latest = latest.clone();
                move |event: Event<PointerEventData>| {
                    let ctx = latest.get();
                    match event.data() {
                        PointerEventData::Touch(_) => {
                            pressing.set(true);
                            if can_swipe {
                                swipe.set(Some(Swipe {
                                    start: point_of(&event),
                                    base: if ctx.swiped.peek().is_some_and(|open| open == display) {
                                        -reach
                                    } else {
                                        0.0
                                    },
                                    active: false,
                                    live: 0.0,
                                }));
                            }
                            press.begin(
                                &event,
                                Some(EventHandler::new({
                                    let latest = latest.clone();
                                    move |at: (f32, f32)| latest.get().long_press(display, at)
                                })),
                            );
                        }
                        PointerEventData::Mouse(mouse) => match mouse.button {
                            Some(MouseButton::Left) if enabled => {
                                pressing.set(true);
                                ctx.press_down(display, freya_winit::modifiers::live());
                            }
                            Some(MouseButton::Right) => {
                                event.stop_propagation();
                                ctx.open_menu(
                                    display,
                                    PanelTrigger::Pointer(point_of(&event)),
                                    false,
                                );
                            }
                            _ => {}
                        },
                    }
                }
            })
            .on_global_pointer_move({
                move |event: Event<PointerEventData>| {
                    press.moved(&event);
                    if !is_touch(&event) {
                        return;
                    }
                    if !press.pending() {
                        pressing.set_if_modified(false);
                    }
                    let Some(mut drag) = *swipe.peek() else {
                        return;
                    };
                    let (x, y) = point_of(&event);
                    let (dx, dy) = (x - drag.start.0, y - drag.start.1);
                    if !drag.active && dx.abs() > SWIPE_SLOP && dx.abs() > dy.abs() {
                        drag.active = true;
                    }
                    if drag.active {
                        drag.live = (drag.base + dx).clamp(-reach, 0.0);
                        swipe.set(Some(drag));
                        event.prevent_default();
                    }
                }
            })
            .on_global_pointer_up({
                let latest = latest.clone();
                move |_| {
                    press.release();
                    pressing.set_if_modified(false);
                    let Some(drag) = swipe.peek().as_ref().copied() else {
                        return;
                    };
                    swipe.set(None);
                    if drag.active {
                        let ctx = latest.get();
                        let mut open = ctx.swiped;
                        open.set((drag.live < -reach / 2.0).then_some(display));
                    }
                }
            })
            .on_press({
                let latest = latest.clone();
                move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let ctx = latest.get();
                    // A swipe that ended over the row is not a tap.
                    if swipe.peek().is_some_and(|drag| drag.active) {
                        return;
                    }
                    match event.data() {
                        PressEventData::Touch(_) => {
                            if press.tap(&event) && enabled {
                                ctx.tap(display);
                            }
                        }
                        PressEventData::Mouse(_) => {
                            ctx.press_up(display, freya_winit::modifiers::live())
                        }
                        _ => {}
                    }
                }
            })
            .maybe_child(actions)
            .child(content)
            .into_element()
    }
}

/// One button of a swiped-open row.
fn swipe_button<T: 'static, K: Clone + Eq + Hash + 'static>(
    tokens: &crate::theme::Tokens,
    action: &SwipeAction,
    latest: Latest<T, K>,
    height: f32,
) -> Element {
    let on_press = action.on_press.clone();
    let ink = Ink::OnAccent;
    rect()
        .width(Size::px(ACTION_WIDTH))
        .height(Size::px(height - 2.0 * S1.px()))
        .tw(
            tokens,
            &[
                flex_col(),
                center(),
                gap(S0_5),
                rounded(Radius::Control),
                bg(if action.destructive {
                    Paint::Danger
                } else {
                    Paint::Accent
                }),
                mx(S0_5),
            ],
        )
        .a11y_role(AccessibilityRole::Button)
        .a11y_alt(action.label.clone())
        .on_press(move |event: Event<PressEventData>| {
            event.stop_propagation();
            on_press.call(());
            latest.get().close_swipe();
        })
        .maybe_child(
            action
                .icon
                .map(|icon| icons::icon(icon, IconSize::Control, Ink::OnAccent.color(tokens))),
        )
        .child(
            CAPTION
                .in_ink(ink)
                .label(tokens, action.label.clone())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
        .into()
}
