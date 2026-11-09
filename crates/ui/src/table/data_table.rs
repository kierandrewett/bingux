//! The data table: typed columns over any number of rows.

use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;

use freya::prelude::*;

use super::batch::{BatchAction, BatchBar};
use super::column::Column;
use super::ctx::{CellEdit, CellIds, Columns, Ctx, Latest, MenuFn, Order};
use super::header::TableHeader;
use super::model::{
    ColumnLayout, ColumnSpec, Cursor, SortDirection, SortOrder, Width, plan, sorted_order,
};
use super::row::{CHECK_CELL, TableRow, side};
use super::rows::Rows;
use crate::component::ComponentName;
use crate::controls::checkbox::CheckState;
use crate::controls::direction::use_direction;
use crate::icons::Icon;
use crate::motion::reveal_if;
use crate::selection::{Selection, SelectionMode};
use crate::shell::{MenuItem, Overlays, menu_button, menu_item, status_page};
use crate::style::*;
use crate::surface::use_tokens;

/// How tall the rows are. A phone's rows are as tall as a finger needs
/// whichever is chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowDensity {
    /// Menu-item height: for tables people scan, such as logs and process lists.
    Compact,
    /// The sidebar's row height. The default.
    #[default]
    Comfortable,
    /// A full settings row: for tables of a few things with room around them.
    Spacious,
}

impl RowDensity {
    fn row_height(self) -> f32 {
        match self {
            RowDensity::Compact => Dim::MenuItem.px(),
            RowDensity::Comfortable => Dim::NavItem.px(),
            RowDensity::Spacious => Dim::Row.px(),
        }
    }
}

/// What the table has to show.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TableStatus {
    /// The rows are here (or there are none: see [`DataTable::empty_state`]).
    #[default]
    Ready,
    /// The rows are on their way: quiet placeholder rows in the rows' places,
    /// under the real headings.
    Loading,
    /// The rows could not be had.
    Failed(TableFailure),
}

impl TableStatus {
    /// A failure with a short title saying what failed.
    pub fn failed(title: impl Into<String>) -> Self {
        TableStatus::Failed(TableFailure {
            title: title.into(),
            detail: None,
            retry: None,
        })
    }

    /// A line under the title saying what to do next. Only on a failure.
    pub fn detail(mut self, text: impl Into<String>) -> Self {
        if let TableStatus::Failed(failure) = &mut self {
            failure.detail = Some(text.into());
        }
        self
    }

    /// A button that tries again. Only on a failure.
    pub fn retry(mut self, on_retry: impl Into<EventHandler<()>>) -> Self {
        if let TableStatus::Failed(failure) = &mut self {
            failure.retry = Some(on_retry.into());
        }
        self
    }
}

/// A table that could not load.
#[derive(Clone, Debug, PartialEq)]
pub struct TableFailure {
    title: String,
    detail: Option<String>,
    retry: Option<EventHandler<()>>,
}

/// What [`data_table`] draws, as it is configured.
pub struct DataTable<T, K> {
    rows: Rows<T>,
    columns: Columns<T>,
    key: Rc<dyn Fn(&T) -> K>,
    id: Option<&'static str>,
    selection: Option<State<Selection<K>>>,
    mode: SelectionMode,
    sort: Option<State<Option<SortOrder>>>,
    default_sort: Option<SortOrder>,
    status: TableStatus,
    empty: Option<Element>,
    density: RowDensity,
    marks: bool,
    striped: bool,
    header: bool,
    batch: Vec<BatchAction<K>>,
    menu: Option<MenuFn<K>>,
    on_activate: Option<EventHandler<K>>,
    on_edit: EventHandler<CellEdit<K>>,
    on_sort: EventHandler<Option<SortOrder>>,
    label: String,
    chevrons: bool,
}

impl<T, K: 'static> PartialEq for DataTable<T, K> {
    fn eq(&self, other: &Self) -> bool {
        fn same<F: ?Sized>(a: &Option<Rc<F>>, b: &Option<Rc<F>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
        }
        self.rows == other.rows
            && self.columns == other.columns
            && Rc::ptr_eq(&self.key, &other.key)
            && self.id == other.id
            && self.selection == other.selection
            && self.mode == other.mode
            && self.sort == other.sort
            && self.default_sort == other.default_sort
            && self.status == other.status
            && self.empty == other.empty
            && self.density == other.density
            && self.marks == other.marks
            && self.striped == other.striped
            && self.header == other.header
            && self.batch == other.batch
            && same(&self.menu, &other.menu)
            && self.chevrons == other.chevrons
            && self.on_activate == other.on_activate
            && self.on_edit == other.on_edit
            && self.on_sort == other.on_sort
            && self.label == other.label
    }
}

/// A table of `rows` with `columns`. `key` is the identity of a row (an id, a
/// path): the selection and the cursor keep to it through a sort or a reload.
///
/// Keep the rows in an `Rc` (a hook or a state) and the columns in a `Vec`
/// built once (`use_hook`), so a render that changed neither draws the same
/// rows; see [`Rows`]. The table fills the room its parent gives it and
/// scrolls its rows inside it, under a heading row that stays in place; give
/// its parent a height.
///
/// ```ignore
/// data_table(rows.clone(), columns.clone(), |app: &App| app.id)
///     .id("apps")
///     .selection(selection)
///     .on_activate(open)
/// ```
pub fn data_table<T: 'static, K: Clone + Eq + Hash + 'static>(
    rows: impl Into<Rows<T>>,
    columns: impl Into<Columns<T>>,
    key: impl Fn(&T) -> K + 'static,
) -> DataTable<T, K> {
    DataTable {
        rows: rows.into(),
        columns: columns.into(),
        key: Rc::new(key),
        id: None,
        selection: None,
        mode: SelectionMode::Multiple,
        sort: None,
        default_sort: None,
        status: TableStatus::Ready,
        empty: None,
        density: RowDensity::Comfortable,
        marks: false,
        striped: false,
        header: true,
        batch: Vec::new(),
        menu: None,
        on_activate: None,
        on_edit: EventHandler::new(|_: CellEdit<K>| {}),
        on_sort: EventHandler::new(|_: Option<SortOrder>| {}),
        label: String::new(),
        chevrons: true,
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> DataTable<T, K> {
    /// Remembers the table's column order, widths, visibility and sort under
    /// `id`, in the application's own saved state. Without it the layout lasts
    /// as long as the window.
    pub fn id(mut self, id: &'static str) -> Self {
        self.id = Some(id);
        self
    }

    /// The selection, owned by the caller so it can read it. Without it the
    /// table keeps its own.
    pub fn selection(mut self, selection: State<Selection<K>>) -> Self {
        self.selection = Some(selection);
        self
    }

    /// How many rows can be selected: one, any number (the default), or none.
    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.mode = mode;
        self
    }

    /// The sort, owned by the caller. A press on a sortable heading changes it.
    /// Without it the table keeps its own, starting from the saved sort or the
    /// one given by [`DataTable::sorted_by`].
    pub fn sort(mut self, sort: State<Option<SortOrder>>) -> Self {
        self.sort = Some(sort);
        self
    }

    /// The sort the table starts with.
    pub fn sorted_by(mut self, column: &'static str, direction: SortDirection) -> Self {
        self.default_sort = Some(SortOrder::new(column, direction));
        self
    }

    /// Told when the sort changes, `None` when it is cleared.
    pub fn on_sort(mut self, handler: impl Into<EventHandler<Option<SortOrder>>>) -> Self {
        self.on_sort = handler.into();
        self
    }

    /// Whether the rows are there, on their way, or could not be had.
    pub fn status(mut self, status: TableStatus) -> Self {
        self.status = status;
        self
    }

    /// What shows when there are no rows: a [`status_page`] with one short
    /// title, one line and one action. The default says there is nothing to show.
    pub fn empty_state(mut self, empty: impl Into<Element>) -> Self {
        self.empty = Some(empty.into());
        self
    }

    pub fn density(mut self, density: RowDensity) -> Self {
        self.density = density;
        self
    }

    /// A column of selection marks at the start of each row, with a mark in
    /// the heading that selects every row. For a table people select from with
    /// the pointer; the keyboard and a phone need no marks.
    pub fn marks(mut self) -> Self {
        self.marks = true;
        self
    }

    /// Every other row on a faint fill.
    pub fn striped(mut self) -> Self {
        self.striped = true;
        self
    }

    /// Leaves out the heading row: the columns are laid out and the rows
    /// scroll, but nothing sorts or resizes by pointer.
    pub fn headerless(mut self) -> Self {
        self.header = false;
        self
    }

    /// The actions a selection of several rows has, in a bar over the table
    /// while it holds them (one row on a phone, which picks rows by a long
    /// press). Each is called with the keys of the selected rows in the order
    /// they are shown.
    pub fn batch_actions(mut self, actions: Vec<BatchAction<K>>) -> Self {
        self.batch = actions;
        self
    }

    /// The menu of a row, from a right click, a long press on a phone, the
    /// Menu key or Shift+F10. It is given the keys of the rows the menu acts
    /// on: the whole selection when the row is in it, else the row, which is
    /// selected first. An empty menu opens nothing.
    pub fn row_menu(mut self, menu: impl Fn(&[K]) -> Vec<MenuItem> + 'static) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }

    /// A row was opened: a double press, Enter, or a tap on a phone.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<K>>) -> Self {
        self.on_activate = Some(handler.into());
        self
    }

    /// Told when the person finishes editing the text of a cell of a column
    /// made with [`Column::editable`].
    pub fn on_edit(mut self, handler: impl Into<EventHandler<CellEdit<K>>>) -> Self {
        self.on_edit = handler.into();
        self
    }

    /// The table's name for assistive technology, such as "Installed apps".
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Whether a phone row that opens shows a chevron (it does by default).
    /// Leave it out when opening a row does not go to a place: a song plays.
    pub fn chevrons(mut self, show: bool) -> Self {
        self.chevrons = show;
        self
    }
}

/// The order of the last sort, and what it was made for.
struct SortCache {
    rows: usize,
    len: usize,
    columns: usize,
    order: SortOrder,
    result: Rc<Vec<u32>>,
}

/// What every visible row is given.
struct RowsData<T, K> {
    latest: Latest<T, K>,
    rows: Rows<T>,
    columns: Columns<T>,
    key: Rc<dyn Fn(&T) -> K>,
    plan: super::model::Plan,
    order: Order,
    selection: Selection<K>,
    cursor: Option<Cursor>,
    editing: Option<(K, usize)>,
    height: f32,
    compact: bool,
    marks: bool,
    striped: bool,
    picking: bool,
    activates: bool,
}

impl<T, K: Clone> Clone for RowsData<T, K> {
    fn clone(&self) -> Self {
        Self {
            latest: self.latest.clone(),
            rows: self.rows.clone(),
            columns: self.columns.clone(),
            key: self.key.clone(),
            plan: self.plan.clone(),
            order: self.order.clone(),
            selection: self.selection.clone(),
            cursor: self.cursor,
            editing: self.editing.clone(),
            height: self.height,
            compact: self.compact,
            marks: self.marks,
            striped: self.striped,
            picking: self.picking,
            activates: self.activates,
        }
    }
}

impl<T, K: Eq + Hash> PartialEq for RowsData<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.columns == other.columns
            && Rc::ptr_eq(&self.key, &other.key)
            && self.plan == other.plan
            && self.order == other.order
            && self.selection == other.selection
            && self.cursor == other.cursor
            && self.editing == other.editing
            && self.height == other.height
            && self.compact == other.compact
            && self.marks == other.marks
            && self.striped == other.striped
            && self.picking == other.picking
            && self.activates == other.activates
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> DataTable<T, K> {
    /// The rows in the order the sort asks for, sorting only when the rows, the
    /// columns or the sort changed.
    fn ordered(&self, sort: Option<SortOrder>, cache: &RefCell<Option<SortCache>>) -> Order {
        let len = self.rows.len();
        let Some(order) = sort else {
            return Order::Natural(len);
        };
        let Some(column) = self
            .columns
            .0
            .iter()
            .find(|column| column.spec.id == order.column)
        else {
            return Order::Natural(len);
        };
        let Some(compare) = column.compare.clone() else {
            return Order::Natural(len);
        };
        let columns = Rc::as_ptr(&self.columns.0) as usize;
        if let Some(last) = cache.borrow().as_ref()
            && last.rows == self.rows.id()
            && last.len == len
            && last.columns == columns
            && last.order == order
        {
            return Order::Sorted(last.result.clone());
        }
        let rows = &self.rows;
        let result = Rc::new(sorted_order(len, order.direction, |a, b| {
            match (rows.get(a), rows.get(b)) {
                (Some(a), Some(b)) => compare(a, b),
                _ => std::cmp::Ordering::Equal,
            }
        }));
        *cache.borrow_mut() = Some(SortCache {
            rows: self.rows.id(),
            len,
            columns,
            order,
            result: result.clone(),
        });
        Order::Sorted(result)
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for DataTable<T, K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = crate::style::is_compact();
        let direction = use_direction();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let mut outer = use_state(Area::default);
        let mut viewport = use_state(Area::default);
        let overlays = use_try_consume::<Overlays>();
        let specs: Vec<ColumnSpec> = self
            .columns
            .0
            .iter()
            .map(|column| column.spec.clone())
            .collect();

        let persist = self.id;
        let layout = use_state({
            let persist = persist;
            move || {
                persist
                    .and_then(|id| crate::saved::load::<ColumnLayout>(&format!("table.{id}")))
                    .unwrap_or_default()
            }
        });
        let own_selection = use_state(Selection::<K>::new);
        let selection = self.selection.unwrap_or(own_selection);
        let own_sort = use_state({
            let specs = specs.clone();
            let default = self.default_sort;
            move || layout.peek().saved_sort(&specs).or(default)
        });
        let sort = self.sort.unwrap_or(own_sort);
        let cursor = use_state(|| None::<Cursor>);
        let editing = use_state(|| None::<(K, usize)>);
        let typed = use_hook(|| Rc::new(RefCell::new((String::new(), None))));
        let last_press = use_hook(|| Rc::new(Cell::new(None)));
        let latest = use_hook(Latest::<T, K>::new);
        let sort_cache = use_hook(|| Rc::new(RefCell::new(None::<SortCache>)));
        let cells = use_hook(CellIds::default);

        // A change of the selection or the sort is read out ("3 selected",
        // "Sorted by Name, ascending").
        let sort_now = (*sort.read()).and_then(|order| {
            specs
                .iter()
                .find(|spec| spec.id == order.column)
                .map(|spec| {
                    (
                        spec.title.clone(),
                        order.direction == SortDirection::Ascending,
                    )
                })
        });
        crate::a11y::use_collection_announcements(selection.read().len(), sort_now);
        let order = self.ordered(*sort.read(), &sort_cache);
        let count = order.len();
        let mode = self.mode;
        let marks = self.marks && !compact && mode == SelectionMode::Multiple && self.header;
        let height = if compact {
            Dim::Row.px()
        } else {
            self.density.row_height()
        };

        // The room the rows have. Until the scroller has been laid out, the
        // table's own width stands in for it.
        let measured = viewport.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            outer.read().width()
        };
        let inner = (width - 2.0 * side() - if marks { CHECK_CELL } else { 0.0 }).max(0.0);
        let shown: Vec<usize> = {
            let layout = layout.read();
            layout
                .order(&specs)
                .into_iter()
                .filter(|&index| layout.is_shown(&specs[index]))
                .collect()
        };
        let widths: Vec<Width> = {
            let layout = layout.read();
            shown
                .iter()
                .map(|&index| layout.width_of(&specs[index]))
                .collect()
        };
        // A phone has no columns to fit: every shown column goes into the row.
        let layout_plan = plan(
            &specs,
            &shown,
            &widths,
            if compact { f32::MAX } else { inner },
        );

        let on_activate = self.on_activate.clone();
        let batch_on = !self.batch.is_empty();
        latest.set(Ctx {
            rows: self.rows.clone(),
            columns: self.columns.clone(),
            key: self.key.clone(),
            order: order.clone(),
            plan: layout_plan.clone(),
            mode,
            selection,
            sort,
            layout,
            cursor,
            editing,
            persist,
            a11y_id,
            overlays,
            direction,
            scroll,
            viewport,
            row_height: height,
            header: self.header && !compact,
            marks,
            menu: self.menu.clone(),
            batch: batch_on,
            on_activate: on_activate.clone(),
            on_edit: self.on_edit.clone(),
            on_sort: self.on_sort.clone(),
            typed,
            last_press,
            cells: cells.clone(),
        });

        // The keyboard cursor starts where the selection is when the keyboard
        // first reaches the table.
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
                    state.set(Some(ctx.start_cursor()));
                }
            }
        });

        let held = selection.read().clone();
        let picking = compact && !held.is_empty();
        let shown_cursor = cursor.read().filter(|_| keyboard).map(|at| Cursor {
            row: at.row.filter(|&row| row < count),
            col: at.col.min(layout_plan.columns.len().saturating_sub(1)),
        });

        // ---- the body -------------------------------------------------
        let status = &self.status;
        let body: Element = match status {
            TableStatus::Failed(failure) => {
                let mut page = status_page(Icon::DialogError, failure.title.clone());
                if let Some(detail) = &failure.detail {
                    page = page.description(detail.clone());
                }
                if let Some(retry) = &failure.retry {
                    page = page.action("Try again", retry.clone());
                }
                page.into()
            }
            TableStatus::Loading => placeholder_rows(&tokens, &layout_plan, height, compact, marks),
            TableStatus::Ready if count == 0 => match &self.empty {
                Some(empty) => empty.clone(),
                None => status_page(Icon::ViewList, "Nothing to show").into(),
            },
            TableStatus::Ready => {
                let data = RowsData {
                    latest: latest.clone(),
                    rows: self.rows.clone(),
                    columns: self.columns.clone(),
                    key: self.key.clone(),
                    plan: layout_plan.clone(),
                    order: order.clone(),
                    selection: held.clone(),
                    cursor: shown_cursor,
                    editing: editing.read().clone(),
                    height,
                    compact,
                    marks,
                    striped: self.striped,
                    picking,
                    activates: on_activate.is_some() && self.chevrons,
                };
                VirtualScrollView::new_with_data_controlled(
                    data,
                    |item, data: &RowsData<T, K>| {
                        let display = item.index;
                        let source = data.order.source(display);
                        let Some(row) = data.rows.get(source) else {
                            return rect().key(display).into();
                        };
                        let key = (data.key)(row);
                        let selected = data.selection.contains(&key);
                        Element::from(TableRow {
                            latest: data.latest.clone(),
                            display,
                            source,
                            rows: data.rows.clone(),
                            columns: data.columns.clone(),
                            plan: data.plan.clone(),
                            selected,
                            cursor: data
                                .cursor
                                .filter(|at| at.row == Some(display))
                                .map(|at| at.col),
                            editing: data
                                .editing
                                .as_ref()
                                .filter(|(edited, _)| *edited == key)
                                .map(|(_, column)| *column),
                            height: data.height,
                            compact: data.compact,
                            marks: data.marks,
                            striped: data.striped,
                            picking: data.picking,
                            activates: data.activates,
                        })
                    },
                    scroll,
                )
                .length(count)
                .item_size(height)
                .scroll_with_arrows(false)
                .drag_scrolling(compact)
                .initial_size(viewport.peek().size)
                .on_sized(move |event: Event<SizedEventData>| {
                    viewport.set_if_modified(event.area);
                })
                .into()
            }
        };

        // ---- the heading ----------------------------------------------
        let sorted = *sort.read();
        let header =
            (self.header && !compact && !matches!(status, TableStatus::Failed(_))).then(|| {
                let mark = marks.then(|| {
                    if held.is_empty() {
                        CheckState::Off
                    } else if held.len() >= count && count > 0 {
                        CheckState::On
                    } else {
                        CheckState::Mixed
                    }
                });
                Element::from(TableHeader {
                    latest: latest.clone(),
                    columns: self.columns.clone(),
                    plan: layout_plan.clone(),
                    sort: sorted,
                    width: if measured > 0.0 { measured } else { width },
                    marks: mark,
                    cursor: shown_cursor.and_then(|at| at.row.is_none().then_some(at.col)),
                })
            });

        // ---- the sort control of a phone ---------------------------------
        let sort_bar = (compact && count > 0 && matches!(status, TableStatus::Ready))
            .then(|| sort_menu(&latest, &specs, sorted))
            .flatten();

        // ---- the batch bar -------------------------------------------------
        let threshold = if compact { 1 } else { 2 };
        let bar = (batch_on && held.len() >= threshold).then(|| {
            Element::from(BatchBar {
                count: held.len(),
                actions: self.batch.clone(),
                latest: latest.clone(),
            })
        });

        let columns = layout_plan.columns.len();
        let label = self.label.clone();
        // The cell that holds the cursor, for assistive technology: the grid is
        // the one Tab stop, so it names the cell its arrows reached.
        let active = cursor.read().filter(|_| focus() != Focus::Not).map(|at| {
            let row = at.row.filter(|&row| row < count).unwrap_or(usize::MAX);
            cells.get(row, at.col.min(columns.saturating_sub(1)))
        });
        rect()
            .component("DataTable")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Grid)
            .a11y_alt(label)
            .a11y_builder(move |node| {
                node.set_row_count(count + 1);
                node.set_column_count(columns);
                if mode == SelectionMode::Multiple {
                    node.set_multiselectable();
                }
                if let Some(active) = active {
                    node.set_active_descendant(active);
                }
            })
            .tw(&tokens, &[flex_col(), w_full(), h_full()])
            .on_sized(move |event: Event<SizedEventData>| outer.set_if_modified(event.area))
            .on_key_down({
                let latest = latest.clone();
                move |event: Event<KeyboardEventData>| {
                    if latest.get().key_down(&event) {
                        event.stop_propagation();
                        event.prevent_default();
                    }
                }
            })
            .child(reveal_if(bar))
            .maybe_child(sort_bar)
            .maybe_child(header)
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(body))
    }
}

/// The phone's sort control: a button that names the sort and opens a menu of
/// the sortable columns and the two directions.
fn sort_menu<T: 'static, K: Clone + Eq + Hash + 'static>(
    latest: &Latest<T, K>,
    specs: &[ColumnSpec],
    sorted: Option<SortOrder>,
) -> Option<Element> {
    let sortable: Vec<&ColumnSpec> = specs.iter().filter(|spec| spec.sortable).collect();
    if sortable.is_empty() {
        return None;
    }
    let mut items: Vec<MenuItem> = sortable
        .iter()
        .map(|spec| {
            let id = spec.id;
            let current = sorted.map(|order| order.column) == Some(id);
            let direction = sorted.map_or(SortDirection::Ascending, |order| order.direction);
            let latest = latest.clone();
            menu_item(spec.title.clone(), move |()| {
                latest.get().set_sort(Some(SortOrder::new(id, direction)));
            })
            .checked(current)
        })
        .collect();
    items.push(crate::shell::menu_separator());
    for (label, direction) in [
        ("Ascending", SortDirection::Ascending),
        ("Descending", SortDirection::Descending),
    ] {
        let latest = latest.clone();
        let column = sorted.map_or(sortable[0].id, |order| order.column);
        items.push(
            menu_item(label, move |()| {
                latest
                    .get()
                    .set_sort(Some(SortOrder::new(column, direction)));
            })
            .checked(sorted.is_some_and(|order| order.direction == direction)),
        );
    }
    let title = sorted
        .and_then(|order| specs.iter().find(|spec| spec.id == order.column))
        .map_or_else(
            || "Sort".to_string(),
            |spec| format!("Sort by {}", spec.title),
        );
    Some(
        rect()
            .tw_layout(&[flex_row(), justify(Align::End), w_full(), px(S2), py(S1)])
            .child(menu_button(title, items))
            .into(),
    )
}

/// Quiet bars where the rows will be.
fn placeholder_rows(
    tokens: &crate::theme::Tokens,
    plan: &super::model::Plan,
    height: f32,
    compact: bool,
    marks: bool,
) -> Element {
    let rows = 12;
    let bar = |width: f32| {
        rect().width(Size::px(width.max(16.0))).tw(
            tokens,
            &[h(Dim::Box5), rounded(Radius::Mark), bg(Paint::Control)],
        )
    };
    rect()
        .tw(tokens, &[flex_col(), w_full(), h_full(), clip()])
        .children((0..rows).map(|row| {
            let cells: Vec<Element> = if compact {
                vec![
                    rect()
                        .tw(tokens, &[flex_col(), gap(S1), w_flex(), px(S2)])
                        .child(bar(180.0))
                        .child(bar(110.0))
                        .into(),
                ]
            } else {
                plan.columns
                    .iter()
                    .map(|placed| {
                        rect()
                            .width(Size::px(placed.width))
                            .tw(
                                tokens,
                                &[flex_row(), items(Align::Center), px(S2), h_full()],
                            )
                            .child(bar((placed.width - 2.0 * S2.px())
                                * if row % 3 == 0 { 0.6 } else { 0.85 }))
                            .into()
                    })
                    .collect()
            };
            rect()
                .key(row)
                .height(Size::px(height))
                .tw(tokens, &[w_full(), px(LIST_EDGE)])
                .child(
                    rect()
                        .tw(
                            tokens,
                            &[
                                flex_row(),
                                items(Align::Center),
                                w_full(),
                                h_full(),
                                px(FILE_ROW_INSET),
                            ],
                        )
                        .maybe_child(marks.then(|| rect().width(Size::px(CHECK_CELL))))
                        .children(cells),
                )
                .into_element()
        }))
        .into()
}
