//! The list view: rows of one kind that the person selects, opens, groups under
//! headers and reorders.
//!
//! [`list_view`] draws any number of rows from a [`Rows`](crate::table::Rows)
//! source, building only those in view, so a list of a hundred thousand scrolls
//! as smoothly as one of ten. Each row is a [`ListRow`]: a title, a subtitle, a
//! symbol, a value, a chevron. It is the single-column sibling of
//! [`data_table`](crate::table::data_table) and shares its [`Selection`], its
//! keys and its menus:
//!
//! * **Selection modes.** [`SelectionMode::None`] (the default: a press opens
//!   the row), `Single` and `Multiple`, with Shift for a range, Ctrl for single
//!   rows and a mark at the start of the row.
//! * **Sections.** [`ListView::sections`] groups rows under headers.
//! * **Keyboard.** One Tab stop; Up, Down, Home, End and the page keys move a
//!   cursor over the rows (headers are skipped) and select as they go, Space
//!   toggles, Enter opens, Ctrl+A selects all, Escape clears, the Menu key opens
//!   the row's menu and letters jump to a row.
//! * **Phone.** A row that has [`actions`](ListRow::actions) swipes sideways to
//!   show them; a long press selects or opens the menu; while rows are selected a
//!   tap joins or leaves the selection.
//! * **Reordering.** [`ListView::on_reorder`] gives each row a grip and the
//!   keys of [`sortable`](crate::controls::sortable), for short lists.
//!
//! [`Selection`]: crate::selection::Selection
//! [`SelectionMode::None`]: crate::selection::SelectionMode::None

mod list_view;
mod model;
mod row;

pub use list_view::{ListView, list_view};
pub use row::{ListRow, SwipeAction, list_row, swipe_action};
