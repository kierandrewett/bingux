//! The history: finished calculations, newest first.
//!
//! A press brings the expression back into the entry. The context menu has
//! the rest: type the result at the caret, copy either, delete the entry.

use bingux_calc::store::Entry;
use bingux_ui::prelude::*;
use bingux_ui::shell::MenuItem;
use freya::prelude::*;

use crate::model::Calc;

#[derive(PartialEq)]
pub struct HistoryPanel {
    pub calc: Calc,
}

impl Component for HistoryPanel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let calc = self.calc.clone();
        calc.watch();
        let entries: Vec<(usize, Entry, String, String)> = calc.with(|s| {
            s.history()
                .into_iter()
                .map(|(index, entry)| {
                    let (expr, result) = s.describe(entry);
                    (index, entry.clone(), expr, result)
                })
                .collect()
        });

        if entries.is_empty() {
            return rect()
                .width(Size::fill())
                .height(Size::fill())
                .tw(&tokens, &[flex_col(), center()])
                .child(
                    status_page(Icon::Clock, "No history")
                        .description("Finished calculations appear here."),
                )
                .into_element();
        }

        let rows: Vec<Element> = entries
            .into_iter()
            .map(|(index, entry, expr, result)| {
                let reuse = calc.clone();
                let reuse_entry = entry.clone();
                let menu = history_menu_items(&calc, &entry, index, overlays);
                row(expr)
                    .value(result)
                    .on_press(move |_: ()| reuse.reuse(&reuse_entry))
                    .context_menu(menu)
                    .into()
            })
            .collect();

        let clear = calc.clone();
        let group = Group::new("History".to_string())
            .trailing(
                action_button("Clear", move |_: ()| clear.clear_history())
                    .flat()
                    .compact(),
            )
            .rows(rows);

        ScrollView::new()
            .child(
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_col(), p(S3)])
                    .child(group),
            )
            .into_element()
    }
}

fn history_menu_items(
    calc: &Calc,
    entry: &Entry,
    index: usize,
    overlays: Overlays,
) -> Vec<MenuItem> {
    let use_expr = {
        let (calc, entry) = (calc.clone(), entry.clone());
        menu_item("Use expression", move |_| calc.reuse(&entry))
    };
    let use_result = {
        let (calc, entry) = (calc.clone(), entry.clone());
        menu_item("Use result", move |_| calc.insert_result(&entry))
    };
    let copy_expr = {
        let (calc, text) = (calc.clone(), entry.expr.clone());
        menu_item("Copy expression", move |_| {
            calc.copy_text(text.clone(), overlays)
        })
    };
    let copy_result = {
        let (calc, text) = (calc.clone(), entry.result.clone());
        menu_item("Copy result", move |_| {
            calc.copy_text(text.clone(), overlays)
        })
    };
    let delete = {
        let calc = calc.clone();
        menu_item("Delete", move |_| calc.delete_history(index)).destructive()
    };
    vec![
        use_expr,
        use_result,
        menu_separator(),
        copy_expr,
        copy_result,
        menu_separator(),
        delete,
    ]
}
