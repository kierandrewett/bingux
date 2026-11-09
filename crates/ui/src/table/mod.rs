//! The data table: rows by columns, typed by the row.
//!
//! [`data_table`] draws any number of rows (only the ones in view are built, so
//! a hundred thousand scroll as smoothly as ten) in typed [`Column`]s:
//!
//! * **Sorting.** A sortable column's heading sorts the table and shows the
//!   way with an arrow; pressing it again turns it round. The sort is stable.
//!   [`Column::sort_by`] says how two rows compare, [`Column::sort_by_text`]
//!   compares the way a person reads (case ignored, numbers by value).
//! * **Columns.** The edge of a heading drags to resize it, a double click on
//!   the edge fits it to its widest cell, dragging a heading moves the column,
//!   and a right click on a heading lists the columns to show or hide. Give the
//!   table an [`id`](DataTable::id) and the order, widths, visibility and sort
//!   are remembered. When the table is too narrow, the columns with the highest
//!   [`priority`](Column::priority) number are left out before any is squeezed.
//! * **Selection.** One row or many ([`SelectionMode`]), with Shift for a range
//!   and Ctrl for single rows, a [`Selection`] the app owns and keys that keep
//!   to the same rows through a sort. Rows can start with a mark
//!   ([`marks`](DataTable::marks)). Several selected rows show a batch bar with
//!   the table's [`BatchAction`]s.
//! * **Keyboard.** The table is one Tab stop and a WAI-ARIA grid: the arrows
//!   move a cursor over the cells, Home and End to the ends of the row,
//!   Ctrl+Home and Ctrl+End to the ends of the grid, Page Up and Page Down by a
//!   page; the row the cursor reaches is selected, Shift extends the range and
//!   Ctrl moves without selecting. Space selects or toggles, Enter opens, F2
//!   edits a cell of an [`editable`](Column::editable) column, Ctrl+A selects
//!   every row, Escape clears the selection, the Menu key opens the row's menu,
//!   and typing jumps to the row whose first column starts with it. On a heading
//!   Enter sorts, Shift with an arrow moves the column and Ctrl with an arrow
//!   resizes it.
//! * **States.** An empty table says so and offers a way on
//!   ([`empty_state`](DataTable::empty_state)); a loading one shows quiet rows
//!   under the real headings; a failed one says what failed and how to try
//!   again ([`TableStatus`]).
//! * **Density and width.** [`RowDensity`] sets the row height. On a phone the
//!   heading row gives way to a two-line row (the first column, then the other
//!   columns' text in one line) and a button that sorts; a long press selects.
//!
//! The table fills the room its parent gives it. Rows come from a [`Rows`]
//! source and are never copied.

mod batch;
mod column;
mod ctx;
mod data_table;
mod header;
mod model;
mod row;
mod rows;
#[cfg(all(test, feature = "testing"))]
mod tests;

pub use batch::{BatchAction, batch_action};
pub use column::Column;
pub use ctx::{CellEdit, Columns};
pub use data_table::{DataTable, RowDensity, TableFailure, TableStatus, data_table};
pub use model::{SortDirection, SortOrder, natural_cmp};
pub use rows::{RowSource, Rows};

pub(crate) use ctx::CellIds;
pub(crate) use model::{find_prefix, offset_to_show};
pub(crate) use row::selection_mark;
