//! What the pieces of a table share: the rows, the columns, the plan and every
//! state, and the actions a press or a key takes on them.
//!
//! A table is a header, a batch bar and a scroller of rows, each its own
//! component. They hold no closure over the table's data: they read the
//! latest [`Ctx`] through [`Latest`], so a row that did not change is not drawn
//! again when the table is.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::column::Column;
use super::model::{
    ColumnLayout, ColumnSpec, Cursor, EdgeDrag, Plan, SavedSort, SortOrder, Step, drag_edge,
    find_prefix, offset_to_show, step,
};
use super::rows::Rows;
use crate::controls::direction::Direction;
use crate::controls::path_label::text_width;
use crate::selection::{Gesture, Selection, SelectionMode};
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::{MenuItem, Overlays, menu_item, menu_separator};
use crate::typography::BODY;

/// A press on a row within this long of the last on the same row is a double.
const DOUBLE_PRESS: Duration = Duration::from_millis(400);
/// Typing pauses this long and the typed text starts over.
const TYPE_AHEAD: Duration = Duration::from_millis(1000);
/// How far Ctrl+Left and Ctrl+Right resize a column from the keyboard.
const KEY_RESIZE_STEP: f32 = 8.0;
/// How many rows around the view a double click on a column's edge measures.
const FIT_SAMPLE: usize = 300;

/// The columns of a table, shared. A `Vec<Column<T>>` or an `Rc` of one becomes
/// it: keep the `Rc` in a hook and a render that changes nothing leaves the
/// rows alone.
pub struct Columns<T>(pub(super) Rc<Vec<Column<T>>>);

impl<T> Clone for Columns<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> PartialEq for Columns<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl<T> From<Vec<Column<T>>> for Columns<T> {
    fn from(columns: Vec<Column<T>>) -> Self {
        Self(Rc::new(columns))
    }
}

impl<T> From<Rc<Vec<Column<T>>>> for Columns<T> {
    fn from(columns: Rc<Vec<Column<T>>>) -> Self {
        Self(columns)
    }
}

/// The order the rows are shown in: as given, or as sorted.
#[derive(Clone)]
pub(super) enum Order {
    Natural(usize),
    Sorted(Rc<Vec<u32>>),
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Order::Natural(a), Order::Natural(b)) => a == b,
            (Order::Sorted(a), Order::Sorted(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Order {
    pub fn len(&self) -> usize {
        match self {
            Order::Natural(len) => *len,
            Order::Sorted(order) => order.len(),
        }
    }

    /// The place in the rows of the row shown at `display`.
    pub fn source(&self, display: usize) -> usize {
        match self {
            Order::Natural(_) => display,
            Order::Sorted(order) => order.get(display).map_or(display, |&at| at as usize),
        }
    }
}

/// An edit of a cell's text, handed to the table's `on_edit`.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEdit<K> {
    /// The key of the row.
    pub key: K,
    /// The id of the column.
    pub column: &'static str,
    /// The text the person entered.
    pub text: String,
}

pub(super) type MenuFn<K> = Rc<dyn Fn(&[K]) -> Vec<MenuItem>>;

pub(super) struct Ctx<T, K> {
    pub rows: Rows<T>,
    pub columns: Columns<T>,
    pub key: Rc<dyn Fn(&T) -> K>,
    pub order: Order,
    pub plan: Plan,
    pub mode: SelectionMode,
    pub selection: State<Selection<K>>,
    pub sort: State<Option<SortOrder>>,
    pub layout: State<ColumnLayout>,
    pub cursor: State<Option<Cursor>>,
    pub editing: State<Option<(K, usize)>>,
    pub persist: Option<&'static str>,
    pub a11y_id: AccessibilityId,
    pub overlays: Option<Overlays>,
    pub direction: Direction,
    pub scroll: ScrollController,
    /// The scroller's visible area, in window coordinates.
    pub viewport: State<Area>,
    pub row_height: f32,
    pub header: bool,
    pub marks: bool,
    pub menu: Option<MenuFn<K>>,
    pub batch: bool,
    pub on_activate: Option<EventHandler<K>>,
    pub on_edit: EventHandler<CellEdit<K>>,
    pub on_sort: EventHandler<Option<SortOrder>>,
    pub typed: Rc<RefCell<(String, Option<Instant>)>>,
    pub last_press: Rc<Cell<Option<(Instant, usize)>>>,
    pub cells: CellIds,
}

/// The accessibility ids of the cells that have been drawn, by row and column
/// (`usize::MAX` for the heading row). The grid names the cell that holds the
/// cursor as its active descendant, and the cell draws under the same id, so
/// each asks for it here.
#[derive(Clone, Default)]
pub(crate) struct CellIds(Rc<RefCell<HashMap<(usize, usize), AccessibilityId>>>);

impl CellIds {
    /// More than this many ids are forgotten and made again as cells are drawn.
    const LIMIT: usize = 8192;

    pub fn get(&self, row: usize, col: usize) -> AccessibilityId {
        let mut ids = self.0.borrow_mut();
        if ids.len() > Self::LIMIT {
            ids.clear();
        }
        *ids.entry((row, col))
            .or_insert_with(AccessibilityId::new_unique)
    }
}

/// The latest [`Ctx`], shared by every piece. Always equal to another, so a
/// piece is drawn again only for what it was given itself.
pub(super) struct Latest<T, K>(Rc<RefCell<Option<Rc<Ctx<T, K>>>>>);

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
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }

    pub fn set(&self, ctx: Ctx<T, K>) {
        *self.0.borrow_mut() = Some(Rc::new(ctx));
    }

    pub fn get(&self) -> Rc<Ctx<T, K>> {
        self.0
            .borrow()
            .clone()
            .expect("the table sets its context before it draws a piece")
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Ctx<T, K> {
    pub fn count(&self) -> usize {
        self.order.len()
    }

    /// Whether the rows start with a column of selection marks.
    pub fn has_marks(&self) -> bool {
        self.marks
    }

    pub fn specs(&self) -> Vec<ColumnSpec> {
        self.columns
            .0
            .iter()
            .map(|column| column.spec.clone())
            .collect()
    }

    /// The row shown at `display`.
    pub fn row(&self, display: usize) -> Option<&T> {
        self.rows.get(self.order.source(display))
    }

    pub fn key_at(&self, display: usize) -> Option<K> {
        self.row(display).map(|row| (self.key)(row))
    }

    /// The text of the row at `display` in plan column `col`, or `None` when
    /// the column has no text.
    pub fn text_at(&self, display: usize, col: usize) -> Option<String> {
        let placed = self.plan.columns.get(col)?;
        self.columns.0[placed.index].text_of(self.row(display)?)
    }

    fn anchor_at(&self) -> Option<usize> {
        let selection = self.selection.peek();
        let anchor = selection.anchor()?;
        (0..self.count()).find(|&display| self.key_at(display).as_ref() == Some(anchor))
    }

    pub fn selecting(&self) -> bool {
        !self.selection.peek().is_empty()
    }

    /// Carries out `gesture` on the row at `display`.
    pub fn select(&self, display: usize, gesture: Gesture) -> bool {
        let anchor = match gesture {
            Gesture::Extend | Gesture::ExtendAdd => self.anchor_at(),
            _ => None,
        };
        let mut next = self.selection.peek().clone();
        let changed = next.apply(self.mode, gesture, display, |at| self.key_at(at), anchor);
        if changed {
            let mut selection = self.selection;
            selection.set(next);
        }
        changed
    }

    pub fn select_all(&self) {
        if self.mode != SelectionMode::Multiple {
            return;
        }
        let mut next = self.selection.peek().clone();
        next.select_all((0..self.count()).filter_map(|display| self.key_at(display)));
        let mut selection = self.selection;
        selection.set(next);
    }

    pub fn clear_selection(&self) {
        if self.selecting() {
            let mut next = self.selection.peek().clone();
            next.clear();
            let mut selection = self.selection;
            selection.set(next);
        }
    }

    /// The selected rows' keys, in the order the rows are shown.
    pub fn selected_keys(&self) -> Vec<K> {
        let selection = self.selection.peek();
        (0..self.count())
            .filter_map(|display| self.key_at(display))
            .filter(|key| selection.contains(key))
            .collect()
    }

    /// The keys a command on the row at `display` applies to: the whole
    /// selection when the row is in it, else the row alone, which becomes
    /// the selection first.
    pub fn keys_for(&self, display: usize) -> Vec<K> {
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

    fn set_cursor(&self, cursor: Option<Cursor>) {
        let mut state = self.cursor;
        if *state.peek() != cursor {
            state.set(cursor);
        }
    }

    fn cancel_edit(&self) {
        if self.editing.peek().is_some() {
            let mut editing = self.editing;
            editing.set(None);
        }
    }

    // ---- pointer ----------------------------------------------------------

    /// A mouse press went down on the cell of plan column `col` of the row at
    /// `display`.
    pub fn press_cell(&self, display: usize, col: usize, held: Modifiers) {
        self.a11y_id.request_focus();
        self.cancel_edit();
        self.set_cursor(Some(Cursor {
            row: Some(display),
            col,
        }));
        self.select(display, Gesture::from_modifiers(held));
    }

    /// The press on a row finished: a second one in a beat on the same row
    /// activates it.
    pub fn press_done(&self, display: usize, held: Modifiers) {
        let now = Instant::now();
        let double = matches!(
            self.last_press.get(),
            Some((at, row)) if row == display && now.duration_since(at) < DOUBLE_PRESS
        );
        if double && Gesture::from_modifiers(held) == Gesture::Replace {
            self.last_press.set(None);
            // A double press on a cell that can be edited edits it.
            let col = self.cursor.peek().map_or(0, |cursor| cursor.col);
            let editable = self
                .plan
                .columns
                .get(col)
                .is_some_and(|placed| self.columns.0[placed.index].editable);
            if editable {
                self.begin_edit(display, col);
            } else {
                self.activate(display);
            }
        } else {
            self.last_press.set(Some((now, display)));
        }
    }

    /// A finger tapped the row at `display`: while rows are selected a tap
    /// joins or leaves the selection; otherwise it opens the row, or selects
    /// it when the table has nothing to open.
    pub fn tap(&self, display: usize) {
        self.cancel_edit();
        self.set_cursor(Some(Cursor {
            row: Some(display),
            col: 0,
        }));
        if self.mode == SelectionMode::Multiple && self.selecting() {
            self.select(display, Gesture::Toggle);
        } else if self.on_activate.is_some() {
            self.activate(display);
        } else {
            self.select(display, Gesture::Replace);
        }
    }

    /// A finger rested on the row at `display`, at window point `at`: it joins
    /// the selection when the table has batch actions for a selection, and
    /// opens its menu otherwise.
    pub fn long_press(&self, display: usize, at: (f32, f32)) {
        if self.mode == SelectionMode::Multiple && self.batch {
            self.select(display, Gesture::Toggle);
        } else {
            self.open_menu(display, PanelTrigger::Pointer(at), false);
        }
    }

    pub fn activate(&self, display: usize) {
        if let (Some(on_activate), Some(key)) = (&self.on_activate, self.key_at(display)) {
            on_activate.call(key);
        }
    }

    /// Opens the row's menu against `trigger`, on the selection it acts on.
    pub fn open_menu(&self, display: usize, trigger: PanelTrigger, keyboard: bool) {
        let (Some(menu), Some(overlays)) = (&self.menu, self.overlays) else {
            return;
        };
        let keys = self.keys_for(display);
        if keys.is_empty() {
            return;
        }
        let items = menu(&keys);
        if items.is_empty() {
            return;
        }
        overlays.show_menu_from(Some(self.a11y_id), trigger, items, keyboard);
    }

    /// The row's menu from the keyboard, against the row's place on screen.
    fn open_menu_at_cursor(&self, display: usize) {
        let view = self.viewport.peek().clone();
        let scrolled = <(i32, i32)>::from(self.scroll).1 as f32;
        let top = view.min_y() + display as f32 * self.row_height + scrolled;
        let area = Area::new(
            (view.min_x() + 24.0, top).into(),
            (view.width().min(240.0), self.row_height).into(),
        );
        self.open_menu(
            display,
            PanelTrigger::Keyboard(PanelRect::from_area(&area)),
            true,
        );
    }

    // ---- columns ----------------------------------------------------------

    /// Applies `change` to the layout. Returns whether it differed.
    pub fn edit_layout(&self, change: impl FnOnce(&mut ColumnLayout)) -> bool {
        let mut layout = self.layout;
        let mut next = layout.peek().clone();
        change(&mut next);
        let changed = next != *layout.peek();
        layout.set_if_modified(next);
        changed
    }

    /// Remembers the layout, when the table has an id to remember it under.
    pub fn save_layout(&self) {
        if let Some(id) = self.persist {
            crate::saved::save(&format!("table.{id}"), &*self.layout.peek());
        }
    }

    /// A heading was pressed: sort by plan column `col`.
    pub fn sort_press(&self, col: usize) {
        let Some(placed) = self.plan.columns.get(col) else {
            return;
        };
        let spec = &self.columns.0[placed.index].spec;
        if !spec.sortable {
            return;
        }
        // Read first: a borrow held across `set_sort` would meet its write.
        let current = *self.sort.peek();
        self.set_sort(Some(SortOrder::pressed(current, spec.id)));
    }

    pub fn set_sort(&self, order: Option<SortOrder>) {
        let mut sort = self.sort;
        if *sort.peek() == order {
            return;
        }
        sort.set(order);
        self.edit_layout(|layout| {
            layout.sort = order.map(|order| SavedSort {
                column: order.column.to_string(),
                direction: order.direction,
            });
        });
        self.save_layout();
        self.on_sort.call(order);
    }

    /// Resizes by an edge drag: the effect of [`drag_edge`] on the layout,
    /// measured from `plan`, the plan the drag began with.
    pub fn resize_edge(&self, plan: &Plan, edge: usize, dx: f32) {
        if let Some(EdgeDrag { target, width }) = drag_edge(plan, edge, dx)
            && let Some(placed) = plan.columns.get(target)
        {
            let spec = &self.columns.0[placed.index].spec;
            if spec.resizable {
                self.edit_layout(|layout| layout.resize(spec.id, width, spec.min_width));
            }
        }
    }

    /// A double click on the trailing edge of plan column `edge` fits the
    /// column that edge changes to its widest cell.
    pub fn fit_edge(&self, plan: &Plan, edge: usize) {
        let Some(EdgeDrag { target, .. }) = drag_edge(plan, edge, 0.0) else {
            return;
        };
        let Some(placed) = plan.columns.get(target) else {
            return;
        };
        let spec = &self.columns.0[placed.index].spec;
        if !spec.resizable {
            return;
        }
        let width = self.fit(target);
        self.edit_layout(|layout| layout.resize(spec.id, width, spec.min_width));
        self.save_layout();
    }

    /// The keyboard cursor goes to the heading of plan column `col`.
    pub fn set_header_cursor(&self, col: usize) {
        if self.header {
            self.set_cursor(Some(Cursor { row: None, col }));
        }
    }

    /// The width that fits the widest text of plan column `col` among the rows
    /// near the view, with the heading and its padding and sort arrow.
    pub fn fit(&self, col: usize) -> f32 {
        let Some(placed) = self.plan.columns.get(col) else {
            return 0.0;
        };
        let column = &self.columns.0[placed.index];
        let first =
            (-(<(i32, i32)>::from(self.scroll).1) as f32 / self.row_height).max(0.0) as usize;
        let from = first.saturating_sub(FIT_SAMPLE / 2);
        let to = (from + FIT_SAMPLE).min(self.count());
        let widest = (from..to)
            .filter_map(|display| column.text_of(self.row(display)?))
            .map(|text| text_width(&text, BODY))
            .fold(0.0_f32, f32::max);
        // The heading has the sort arrow and a gap beside its title.
        let title = text_width(&column.spec.title, crate::typography::CAPTION)
            + crate::style::IconSize::Mark.px()
            + crate::style::S1.px();
        widest.max(title).ceil() + 2.0 * crate::style::S2.px()
    }

    /// Menu items that show and hide the table's columns.
    pub fn columns_menu_items(&self) -> Vec<MenuItem> {
        let layout = self.layout.peek().clone();
        let specs = self.specs();
        let mut items: Vec<MenuItem> = layout
            .order(&specs)
            .into_iter()
            .map(|index| {
                let spec = &specs[index];
                let shown = layout.is_shown(spec);
                let id = spec.id;
                let ctx = self.edit_handle();
                let item = menu_item(spec.title.clone(), move |()| {
                    ctx.edit(move |layout| layout.set_shown(id, !shown));
                })
                .checked(shown);
                if spec.can_hide { item } else { item.disabled() }
            })
            .collect();
        items.push(menu_separator());
        let ctx = self.edit_handle();
        items.push(menu_item("Reset columns", move |()| {
            ctx.edit(ColumnLayout::reset);
        }));
        items
    }

    /// Opens the columns menu at a window point, or against the header's place.
    pub fn open_columns_menu(&self, trigger: PanelTrigger, keyboard: bool) {
        if let Some(overlays) = self.overlays {
            overlays.show_menu_from(
                Some(self.a11y_id),
                trigger,
                self.columns_menu_items(),
                keyboard,
            );
        }
    }

    fn edit_handle(&self) -> LayoutHandle {
        LayoutHandle {
            layout: self.layout,
            persist: self.persist,
        }
    }

    // ---- keyboard ---------------------------------------------------------

    /// How many rows a page key moves.
    fn page(&self) -> usize {
        ((self.viewport.peek().height() / self.row_height) as usize).max(1)
    }

    /// Puts the cursor on `to`, scrolls to it and carries out `gesture` on its
    /// row, when there is one.
    fn move_to(&self, to: Cursor, gesture: Option<Gesture>) {
        self.set_cursor(Some(to));
        let Some(display) = to.row else {
            return;
        };
        let current = <(i32, i32)>::from(self.scroll).1;
        let offset = offset_to_show(
            display,
            current,
            self.count(),
            self.viewport.peek().height(),
            self.row_height,
        );
        if offset != current {
            let mut scroll = self.scroll;
            scroll.scroll_to_y(offset);
        }
        if let Some(gesture) = gesture {
            self.select(display, gesture);
        }
    }

    /// Where the cursor starts when the keyboard reaches the table: the first
    /// selected row, else the first row.
    pub fn start_cursor(&self) -> Cursor {
        let selected = (0..self.count()).find(|&display| {
            self.key_at(display)
                .is_some_and(|key| self.selection.peek().contains(&key))
        });
        Cursor {
            row: selected.or((self.count() > 0).then_some(0)),
            col: 0,
        }
    }

    /// A key went down while the table holds the keyboard. Follows the WAI-ARIA
    /// grid pattern: the arrows, Home, End and the page keys move the cursor
    /// over the cells, Ctrl+Home and Ctrl+End to the first and last cell; the
    /// row under the cursor is selected as it moves, Shift extends the range
    /// and Ctrl moves without selecting. Space selects or toggles the row,
    /// Enter opens it, F2 edits a cell, Ctrl+A selects every row, Escape
    /// clears the selection. On a heading, Enter and Space sort, Shift+Left and
    /// Shift+Right move the column and Ctrl+Left and Ctrl+Right resize it.
    /// Returns whether the key was used.
    pub fn key_down(&self, event: &Event<KeyboardEventData>) -> bool {
        if self.editing.peek().is_some() {
            return false;
        }
        let held = event.modifiers;
        let shift = held.contains(Modifiers::SHIFT);
        let ctrl = held.contains(Modifiers::CONTROL) || held.contains(Modifiers::META);
        let alt = held.contains(Modifiers::ALT);
        let cols = self.plan.columns.len();
        let rows = self.count();
        let cursor = (*self.cursor.peek()).unwrap_or_else(|| self.start_cursor());
        let cursor = Cursor {
            row: cursor.row.filter(|&row| row < rows),
            col: cursor.col.min(cols.saturating_sub(1)),
        };
        let on_header = cursor.row.is_none() && self.header;
        let key = self.direction.reading_key(&event.key);

        if let Key::Named(named) = &key {
            let moved = match named {
                NamedKey::ArrowLeft | NamedKey::ArrowRight if on_header && (shift || ctrl) => {
                    let towards = if *named == NamedKey::ArrowLeft { -1 } else { 1 };
                    self.header_keys(cursor.col, towards, shift);
                    return true;
                }
                NamedKey::ArrowLeft => Some(Step::Left),
                NamedKey::ArrowRight => Some(Step::Right),
                NamedKey::ArrowUp => Some(Step::Up),
                NamedKey::ArrowDown => Some(Step::Down),
                NamedKey::Home if ctrl => Some(Step::First),
                NamedKey::End if ctrl => Some(Step::Last),
                NamedKey::Home => Some(Step::Home),
                NamedKey::End => Some(Step::End),
                NamedKey::PageUp => Some(Step::PageUp),
                NamedKey::PageDown => Some(Step::PageDown),
                _ => None,
            };
            if let Some(motion) = moved {
                if alt {
                    return false;
                }
                let to = step(cursor, motion, rows, cols, self.header, self.page());
                let vertical = !matches!(motion, Step::Left | Step::Right | Step::Home | Step::End);
                // Moving over the cells of a row keeps its selection; moving to
                // another row selects it, as Shift and plain arrows ask.
                let gesture = match Gesture::from_modifiers(held) {
                    Gesture::Toggle => None,
                    _ if !vertical && to.row == cursor.row => None,
                    gesture => Some(gesture),
                };
                self.move_to(to, gesture);
                return true;
            }
            match named {
                NamedKey::Enter => {
                    match cursor.row {
                        None if self.header => self.sort_press(cursor.col),
                        Some(row) => self.activate(row),
                        None => {}
                    }
                    return true;
                }
                NamedKey::Escape if self.selecting() => {
                    self.clear_selection();
                    return true;
                }
                NamedKey::F2 => {
                    if let Some(row) = cursor.row {
                        self.begin_edit(row, cursor.col);
                        return true;
                    }
                }
                NamedKey::ContextMenu => {
                    self.open_menu_for_cursor(cursor);
                    return true;
                }
                NamedKey::F10 if shift => {
                    self.open_menu_for_cursor(cursor);
                    return true;
                }
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
                match cursor.row {
                    None if self.header => self.sort_press(cursor.col),
                    Some(row) => {
                        let gesture = match Gesture::from_modifiers(held) {
                            Gesture::Replace if self.mode == SelectionMode::Multiple => {
                                Gesture::Toggle
                            }
                            gesture => gesture,
                        };
                        self.select(row, gesture);
                    }
                    None => {}
                }
                return true;
            }
            return self.type_ahead(text, cursor);
        }
        false
    }

    fn typed_text(&self) -> String {
        let typed = self.typed.borrow();
        match typed.1 {
            Some(at) if at.elapsed() < TYPE_AHEAD => typed.0.clone(),
            _ => String::new(),
        }
    }

    /// Typing jumps to the next row whose first column starts with what was
    /// typed, and selects it.
    fn type_ahead(&self, text: &str, cursor: Cursor) -> bool {
        if text.chars().any(char::is_control) || self.count() == 0 {
            return false;
        }
        let mut typed = self.typed_text();
        let fresh = typed.is_empty();
        typed.push_str(text);
        *self.typed.borrow_mut() = (typed.clone(), Some(Instant::now()));
        let from = cursor
            .row
            .map_or(0, |row| if fresh { row + 1 } else { row });
        let found = find_prefix(&typed, from, self.count(), |display| {
            self.text_at(display, 0).unwrap_or_default()
        });
        if let Some(display) = found {
            self.move_to(
                Cursor {
                    row: Some(display),
                    col: cursor.col,
                },
                Some(Gesture::Replace),
            );
        }
        true
    }

    fn open_menu_for_cursor(&self, cursor: Cursor) {
        match cursor.row {
            Some(row) => self.open_menu_at_cursor(row),
            None => {
                let view = self.viewport.peek().clone();
                let area = Area::new(
                    (view.min_x() + 24.0, view.min_y() - self.row_height).into(),
                    (view.width().min(240.0), self.row_height).into(),
                );
                self.open_columns_menu(PanelTrigger::Keyboard(PanelRect::from_area(&area)), true);
            }
        }
    }

    /// Shift moves the column of plan column `col` one place; Ctrl widens or
    /// narrows it.
    fn header_keys(&self, col: usize, towards: i32, move_it: bool) {
        let Some(placed) = self.plan.columns.get(col) else {
            return;
        };
        let specs = self.specs();
        let spec = &specs[placed.index];
        if move_it {
            let to = col as i32 + towards;
            if to < 0 || to as usize >= self.plan.columns.len() {
                return;
            }
            // Before the column that is at `to` now, or after it when moving on.
            let other = &specs[self.plan.columns[to as usize].index];
            let before = if towards < 0 {
                Some(other.id)
            } else {
                self.plan
                    .columns
                    .get(to as usize + 1)
                    .map(|next| specs[next.index].id)
            };
            if self.edit_layout(|layout| layout.move_before(&specs, spec.id, before)) {
                self.save_layout();
                self.set_cursor(Some(Cursor {
                    row: None,
                    col: to as usize,
                }));
            }
        } else if spec.resizable {
            let width = placed.width + towards as f32 * KEY_RESIZE_STEP;
            if self.edit_layout(|layout| layout.resize(spec.id, width, spec.min_width)) {
                self.save_layout();
            }
        }
    }

    /// Starts editing the cell of plan column `col` of the row at `display`,
    /// when the column can be edited.
    pub fn begin_edit(&self, display: usize, col: usize) {
        let Some(placed) = self.plan.columns.get(col) else {
            return;
        };
        let column = &self.columns.0[placed.index];
        if !column.editable || column.text.is_none() {
            return;
        }
        if let Some(key) = self.key_at(display) {
            let mut editing = self.editing;
            editing.set(Some((key, placed.index)));
        }
    }

    /// The text of a cell has been entered.
    pub fn commit_edit(&self, key: K, column: usize, text: String) {
        let mut editing = self.editing;
        editing.set(None);
        self.a11y_id.request_focus();
        self.on_edit.call(CellEdit {
            key,
            column: self.columns.0[column].spec.id,
            text,
        });
    }

    pub fn stop_edit(&self) {
        self.cancel_edit();
        self.a11y_id.request_focus();
    }
}

/// A handle that changes a table's layout from a menu item, which outlives the
/// table's context.
#[derive(Clone, Copy)]
struct LayoutHandle {
    layout: State<ColumnLayout>,
    persist: Option<&'static str>,
}

impl LayoutHandle {
    fn edit(self, change: impl FnOnce(&mut ColumnLayout)) {
        let mut layout = self.layout;
        let mut next = layout.peek().clone();
        change(&mut next);
        if next != *layout.peek() {
            layout.set(next);
            if let Some(id) = self.persist {
                crate::saved::save(&format!("table.{id}"), &*layout.peek());
            }
        }
    }
}
