//! The details pane: the selected node under three tabs. Properties edits the
//! node as bingux-ui code, Layout draws its box, Computed lists every value
//! the application resolved.

use std::rc::Rc;

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::box_model;
use super::computed::{self, Entry, Value};
use super::props::PropertiesPane;
use super::snapshot::{Node, Snapshot};
use crate::controls::colour_picker::colour_chip;
use crate::controls::{property_row, search_field, segmented};
use crate::group::Group;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, CODE, Ink};

#[derive(Clone, Copy, PartialEq)]
enum DetailsTab {
    Properties,
    Layout,
    Computed,
}

impl DetailsTab {
    const ALL: [DetailsTab; 3] = [
        DetailsTab::Properties,
        DetailsTab::Layout,
        DetailsTab::Computed,
    ];

    fn title(self) -> &'static str {
        match self {
            DetailsTab::Properties => "Properties",
            DetailsTab::Layout => "Layout",
            DetailsTab::Computed => "Computed",
        }
    }

    /// The key in the segmented control: the title, which is unique.
    fn key(self) -> &'static str {
        self.title()
    }

    fn from_key(key: &str) -> Option<DetailsTab> {
        DetailsTab::ALL.into_iter().find(|tab| tab.key() == key)
    }
}

/// The pane for one selected node of the application window.
pub struct DetailsPane {
    pub snap: Rc<Snapshot>,
    pub id: u64,
    pub window: WindowId,
}

impl PartialEq for DetailsPane {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.snap, &other.snap) && self.id == other.id && self.window == other.window
    }
}

impl Component for DetailsPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let tab = use_state(|| DetailsTab::Properties);
        // The Computed tab's filter; held here so it survives tab switches.
        let filter = use_state(String::new);
        let selected = *tab.read();
        let class = super::app_density(self.window);
        let body: Element = match self.snap.node(self.id) {
            Some(node) => match selected {
                DetailsTab::Properties => PropertiesPane {
                    snap: self.snap.clone(),
                    id: self.id,
                    window: self.window,
                }
                .into(),
                DetailsTab::Layout => box_model::view(&tokens, self.window, self.id, node, class),
                DetailsTab::Computed => computed_tab(&tokens, &self.snap, node, filter),
            },
            None => rect()
                .tw(&tokens, &[w_full(), h_full(), p(S4)])
                .child(CAPTION.label(&tokens, "This element is gone"))
                .into(),
        };
        let bar = segmented(
            DetailsTab::ALL
                .iter()
                .map(|tab| (tab.key().to_string(), tab.title().to_string()))
                .collect(),
            selected.key(),
            move |picked: String| {
                if let Some(picked) = DetailsTab::from_key(&picked) {
                    let mut tab = tab;
                    tab.set(picked);
                }
            },
        )
        .tabs()
        .fill();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col()])
            .child(
                rect()
                    .tw(&tokens, &[w_full(), px(S4), pt(S3), pb(S2)])
                    .child(bar),
            )
            .child(body)
    }
}

/// The Computed tab: every used value, under a filter. The list scrolls.
/// It re-renders with each new snapshot of the target, so the numbers follow
/// the layout; while another tab is shown nothing here runs.
fn computed_tab(tokens: &Tokens, snap: &Snapshot, node: &Node, filter: State<String>) -> Element {
    let query = filter.read().trim().to_lowercase();
    let mut column = rect().tw(tokens, &[w_full(), flex_col(), gap(S4), p(S4)]);
    for group in computed::groups(snap, node) {
        let rows: Vec<Element> = group
            .entries
            .into_iter()
            .filter(|found| {
                query.is_empty()
                    || found.name.to_lowercase().contains(&query)
                    || match &found.value {
                        Value::Text(text) => text.to_lowercase().contains(&query),
                        Value::Colour(colour) => colour_to_hex(*colour).contains(&query),
                        Value::Unknown => false,
                    }
                    || found
                        .declared
                        .as_ref()
                        .is_some_and(|declared| declared.to_lowercase().contains(&query))
            })
            .map(|found| value_row(tokens, found))
            .collect();
        if !rows.is_empty() {
            column = column.child(Group::titled(group.title).rows(rows).unfiltered());
        }
    }
    let scroll = ScrollView::new()
        .drag_scrolling(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(column);
    rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), gap(S1)])
        .child(
            rect()
                .tw(tokens, &[w_full(), px(S4), pt(S3)])
                .child(search_field(filter, |_: String| {}).dense()),
        )
        .child(scroll)
        .into()
}

/// A small muted chip beside a value.
fn chip(tokens: &Tokens, text: String, paint: Paint) -> Element {
    rect()
        .tw(
            tokens,
            &[px(S1), rounded(Radius::Mark), center(), bg(paint)],
        )
        .child(
            CAPTION
                .in_ink(Ink::Muted)
                .label(tokens, text)
                .max_lines(1),
        )
        .into()
}

/// A used value: its name at the left, and at the right the value (a swatch
/// first for a colour), then a `declared` chip when the node asked for
/// something else. A value the engine does not expose is an em dash with a
/// faint marker.
fn value_row(tokens: &Tokens, found: Entry) -> Element {
    let mut value = rect().tw(tokens, &[flex_row(), items(Align::Center), gap(S2)]);
    match found.value {
        Value::Text(text) => {
            value = value.child(
                CODE.label(tokens, text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        }
        Value::Colour(colour) => {
            value = value
                .child(colour_chip(*tokens, colour, Dim::Swatch.px()))
                .child(CODE.label(tokens, colour_to_hex(colour)).max_lines(1));
        }
        Value::Unknown => {
            value = value
                .child(CODE.in_ink(Ink::Muted).label(tokens, "—".to_string()))
                .child(chip(tokens, "not exposed".to_string(), Paint::MutedFaint));
        }
    }
    if let Some(declared) = found.declared {
        value = value.child(chip(tokens, format!("declared {declared}"), Paint::Control));
    }
    property_row(found.name).control(value).into()
}
