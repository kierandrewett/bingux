//! A column of a table, typed by the row it shows.

use std::cmp::Ordering;
use std::rc::Rc;

use freya::prelude::*;

use super::model::{ColumnSpec, DEFAULT_WIDTH, MIN_WIDTH, Width, natural_cmp};
use crate::icons::Icon;

type Cell<T> = Rc<dyn Fn(&T) -> Element>;
type Text<T> = Rc<dyn Fn(&T) -> String>;
type Compare<T> = Rc<dyn Fn(&T, &T) -> Ordering>;
type IconOf<T> = Rc<dyn Fn(&T) -> Icon>;

/// One column: a title, how a row draws in it, and what it can do.
///
/// Build it with [`Column::text`] when the cell is a line of text, or
/// [`Column::new`] when it draws something else (a switch, an icon and a name,
/// a bar). A column made with `new` can still give [`Column::text`]'s
/// counterpart [`Column::summary`], which says the row's value in words for a
/// phone's second line, for a column's fit-to-content and for a screen reader.
///
/// ```ignore
/// let columns = vec![
///     Column::text("name", "Name", |app: &App| app.name.clone())
///         .flex()
///         .sort_by_text()
///         .priority(0),
///     Column::text("size", "Size", |app: &App| format_size(app.bytes))
///         .numeric()
///         .width(96.0)
///         .sort_by_key(|app| app.bytes),
/// ];
/// ```
pub struct Column<T> {
    pub(super) spec: ColumnSpec,
    pub(super) cell: Option<Cell<T>>,
    pub(super) text: Option<Text<T>>,
    pub(super) compare: Option<Compare<T>>,
    pub(super) editable: bool,
    pub(super) muted: bool,
    pub(super) icon: Option<IconOf<T>>,
    pub(super) phone: bool,
}

impl<T> Clone for Column<T> {
    fn clone(&self) -> Self {
        Self {
            spec: self.spec.clone(),
            cell: self.cell.clone(),
            text: self.text.clone(),
            compare: self.compare.clone(),
            editable: self.editable,
            muted: self.muted,
            icon: self.icon.clone(),
            phone: self.phone,
        }
    }
}

impl<T> PartialEq for Column<T> {
    fn eq(&self, other: &Self) -> bool {
        fn same<F: ?Sized>(a: &Option<Rc<F>>, b: &Option<Rc<F>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
        }
        self.spec == other.spec
            && self.editable == other.editable
            && self.muted == other.muted
            && self.phone == other.phone
            && same(&self.icon, &other.icon)
            && same(&self.cell, &other.cell)
            && same(&self.text, &other.text)
            && same(&self.compare, &other.compare)
    }
}

impl<T: 'static> Column<T> {
    fn base(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            spec: ColumnSpec {
                id,
                title: title.into(),
                width: Width::Px(DEFAULT_WIDTH),
                min_width: MIN_WIDTH,
                priority: 1,
                right: false,
                can_hide: true,
                shown_by_default: true,
                resizable: true,
                sortable: false,
            },
            cell: None,
            text: None,
            compare: None,
            editable: false,
            muted: false,
            icon: None,
            phone: true,
        }
    }

    /// A column whose cell draws `cell(row)`. `id` names the column in the
    /// saved layout and in the sort, so it never changes once shipped.
    pub fn new(
        id: &'static str,
        title: impl Into<String>,
        cell: impl Fn(&T) -> Element + 'static,
    ) -> Self {
        let mut column = Self::base(id, title);
        column.cell = Some(Rc::new(cell));
        column
    }

    /// A column whose cell is one line of text, `text(row)`, cut with an
    /// ellipsis when it does not fit.
    pub fn text(
        id: &'static str,
        title: impl Into<String>,
        text: impl Fn(&T) -> String + 'static,
    ) -> Self {
        let mut column = Self::base(id, title);
        column.text = Some(Rc::new(text));
        column
    }

    /// The row's value in words, for a column whose cell draws something other
    /// than text: the phone's second line, fitting the column to its content,
    /// type-ahead, copying, and what a screen reader says.
    pub fn summary(mut self, text: impl Fn(&T) -> String + 'static) -> Self {
        self.text = Some(Rc::new(text));
        self
    }

    /// The column takes what the fixed columns leave, with no width of its
    /// own until the person drags its edge. The first column is usually this.
    pub fn flex(mut self) -> Self {
        self.spec.width = Width::Flex;
        self
    }

    /// A fixed starting width in logical pixels.
    pub fn width(mut self, px: f32) -> Self {
        self.spec.width = Width::Px(px);
        self
    }

    /// The least width the column is dragged to, and keeps when the table
    /// gives it a share.
    pub fn min_width(mut self, px: f32) -> Self {
        self.spec.min_width = px;
        self
    }

    /// Numbers and sizes: aligned to the end and set in figures of one width,
    /// so a column of them lines up.
    pub fn numeric(mut self) -> Self {
        self.spec.right = true;
        self
    }

    /// A symbol before the cell's text, chosen by the row: the kind of a
    /// connection, the type of a file.
    pub fn leading_icon(mut self, icon: impl Fn(&T) -> Icon + 'static) -> Self {
        self.icon = Some(Rc::new(icon));
        self
    }

    /// Leaves the column out of a phone's second line, which has room for a
    /// few values: the columns that matter least there.
    pub fn not_on_phone(mut self) -> Self {
        self.phone = false;
        self
    }

    /// Draws the column's text in the muted ink, for secondary information
    /// beside a primary column.
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    /// Aligns the heading and the cells to the end.
    pub fn align_end(mut self) -> Self {
        self.spec.right = true;
        self
    }

    /// How soon the column goes when the table is too narrow for all of them:
    /// 0 never goes (and cannot be hidden by the person); a higher number goes
    /// sooner. Columns start at 1.
    pub fn priority(mut self, priority: u8) -> Self {
        self.spec.priority = priority;
        if priority == 0 {
            self.spec.can_hide = false;
        }
        self
    }

    /// Hidden until the person turns it on in the columns menu.
    pub fn hidden_by_default(mut self) -> Self {
        self.spec.shown_by_default = false;
        self
    }

    /// The person cannot drag the column's edge.
    pub fn fixed(mut self) -> Self {
        self.spec.resizable = false;
        self
    }

    /// Sorts by `compare`: a heading press sorts the table by it.
    pub fn sort_by(mut self, compare: impl Fn(&T, &T) -> Ordering + 'static) -> Self {
        self.spec.sortable = true;
        self.compare = Some(Rc::new(compare));
        self
    }

    /// Sorts by a key of the row: `sort_by_key(|app| app.bytes)`.
    pub fn sort_by_key<K: Ord>(self, key: impl Fn(&T) -> K + 'static) -> Self {
        self.sort_by(move |a, b| key(a).cmp(&key(b)))
    }

    /// Sorts by the column's text the way a person reads it: case ignored and
    /// numbers by value. The column needs [`Column::text`] or
    /// [`Column::summary`].
    pub fn sort_by_text(mut self) -> Self {
        if let Some(text) = self.text.clone() {
            self.spec.sortable = true;
            self.compare = Some(Rc::new(move |a, b| natural_cmp(&text(a), &text(b))));
        }
        self
    }

    /// A double click on a cell, or F2, edits its text in place; Enter hands
    /// the new text to the table's `on_edit`, Escape drops it. The column needs
    /// [`Column::text`] or [`Column::summary`] for the text it starts from.
    pub fn editable(mut self) -> Self {
        self.editable = true;
        self
    }

    pub fn id(&self) -> &'static str {
        self.spec.id
    }

    pub fn title(&self) -> &str {
        &self.spec.title
    }

    pub fn is_sortable(&self) -> bool {
        self.spec.sortable
    }

    pub(super) fn text_of(&self, row: &T) -> Option<String> {
        self.text.as_ref().map(|text| text(row))
    }
}
