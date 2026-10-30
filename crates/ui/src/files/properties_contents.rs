//! The Contents tab of the Properties dialog, for one folder: how many items
//! it holds, how big they add up to, with a Stop button while it counts, and
//! the items that take the most room.
//!
//! The figures grow on screen as the folder is walked. The list of largest
//! items holds its five places while the walk runs, as "…", so a place that
//! is filled moves nothing; when the walk ends the list is as long as what was
//! found. It is the last group of the tab, so that only shortens the tab.

use freya::prelude::*;

use super::properties::Page;
use super::properties_words::{PENDING, child_line, contents, kinds, total_size};
use crate::group::Group;
use crate::rows::{property_row, row};
use crate::style::*;
use crate::typography::{BODY, Ink};

/// How many of the largest items are listed.
pub(super) const LARGEST: usize = 5;

pub(super) fn contents_tab(page: &Page) -> Vec<Element> {
    let tokens = page.tokens;
    let size = row("Total size").trailing(
        rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
            .child(BODY.in_ink(Ink::Muted).selectable(&tokens, total_size(page.totals)))
            .maybe_child(page.stop_button()),
    );
    let (files, folders) = kinds(page.totals);
    let summary = Group::titled("Contents")
        .row(property_row("Items", contents(page.totals)))
        .row(property_row("Files", files))
        .row(property_row("Folders", folders))
        .row(size);

    let mut largest = Group::titled("Largest items");
    let found = &page.largest;
    for child in found {
        largest = largest.row(property_row(child.name.clone(), child_line(child)));
    }
    if !page.totals.done {
        for _ in found.len()..LARGEST {
            largest = largest.row(property_row(PENDING, PENDING));
        }
    } else if found.is_empty() {
        largest = largest.row(property_row("This folder is empty", ""));
    }
    vec![summary.into(), largest.into()]
}
