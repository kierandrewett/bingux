//! The by-interface and by-connection breakdowns: a table the person sorts by
//! what was received, sent or used, with the columns they want.

use bingux_ui::prelude::tr;
use std::rc::Rc;

use bingux_ui::prelude::{Column, Group, Rows, SortDirection, data_table};
use bingux_ui::style::{Dim, is_compact};
use freya::prelude::*;

use super::format;
use super::model::Breakdown;
use super::page::{kind_icon, kind_name};

/// The most rows the table shows before it scrolls inside its card.
const VISIBLE_ROWS: usize = 8;

fn columns(first: String) -> Vec<Column<Breakdown>> {
    vec![
        Column::text("name", first, |entry: &Breakdown| entry.name.clone())
            .leading_icon(|entry: &Breakdown| kind_icon(entry.kind))
            .flex()
            .min_width(140.0)
            .priority(0)
            .sort_by_text(),
        Column::text("kind", tr!("data-usage-col-type"), |entry: &Breakdown| {
            kind_name(entry.kind).to_string()
        })
        .width(110.0)
        .priority(3)
        .muted()
        .sort_by_text(),
        Column::text("rx", tr!("data-usage-received"), |entry: &Breakdown| {
            format::bytes(entry.totals.rx)
        })
        .width(110.0)
        .numeric()
        .priority(2)
        .not_on_phone()
        .sort_by_key(|entry: &Breakdown| entry.totals.rx),
        Column::text("tx", tr!("data-usage-sent"), |entry: &Breakdown| {
            format::bytes(entry.totals.tx)
        })
        .width(110.0)
        .numeric()
        .priority(2)
        .not_on_phone()
        .sort_by_key(|entry: &Breakdown| entry.totals.tx),
        Column::text("total", tr!("data-usage-total"), |entry: &Breakdown| {
            format::bytes(entry.totals.total())
        })
        .width(110.0)
        .numeric()
        .priority(1)
        .sort_by_key(|entry: &Breakdown| entry.totals.total()),
    ]
}

/// One breakdown as a table. `first` titles the first column ("Interface",
/// "Connection"); `id` is where the table remembers its columns and sort.
#[derive(PartialEq)]
struct BreakdownTable {
    entries: Vec<Breakdown>,
    id: &'static str,
    first: String,
    open: EventHandler<String>,
}

impl Component for BreakdownTable {
    fn render(&self) -> impl IntoElement {
        let first = self.first.clone();
        let columns = use_hook(move || Rc::new(columns(first)));
        let rows: Rows<Breakdown> = Rc::new(self.entries.clone()).into();
        let open = self.open.clone();
        let count = self.entries.len().min(VISIBLE_ROWS);
        // The heading row (a sort button on a phone), the rows and a little air.
        let row = if is_compact() {
            Dim::Row.px()
        } else {
            Dim::NavItem.px()
        };
        let height = Dim::Control.px() + 8.0 + count as f32 * row;
        rect().width(Size::fill()).height(Size::px(height)).child(
            data_table(rows, columns, |entry: &Breakdown| entry.name.clone())
                .id(self.id)
                .label(&self.first)
                .sorted_by("total", SortDirection::Descending)
                .on_activate(move |name: String| open.call(name)),
        )
    }
}

/// A group titled `title` holding the breakdown of `entries`, or `empty` when
/// there are none. Pressing a row opens `open` with its name.
pub fn breakdown_group(
    title: String,
    description: String,
    id: &'static str,
    first: String,
    entries: &[Breakdown],
    empty: String,
    open: impl Fn(String) + 'static,
) -> Group {
    let group = Group::titled(&title).description(&description);
    if entries.is_empty() {
        return group.row(crate::ui::status(crate::ui::Status::Empty(empty)));
    }
    group.row(BreakdownTable {
        entries: entries.to_vec(),
        id,
        first,
        open: EventHandler::new(open),
    })
}
