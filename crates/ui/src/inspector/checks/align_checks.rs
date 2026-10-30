//! Alignment: rows of one kind share a column (`AL-01`, `SZ-05`), a list row's
//! x padding evens out its y padding (`SP-09`, `SP-10`), a field's text starts
//! one S1 after its icon slot (`AL-03`) and a header is measured the same in
//! every app (`AL-04`, `SZ-02`).

use std::collections::BTreeMap;

use super::super::{Box4, Context, Finding, Kind, Node, Rule, TOLERANCE, is_visible, paints};
use super::{area_of, compact, component, descendants, focusable, report, texts};

/// How far two edges of one column may differ, in logical pixels.
const COLUMN: f32 = 0.5;

/// The widest a node may be and still be an icon in a row.
const ICON_MAX: f32 = 40.0;

/// The icon and the label a row starts with: the first image-like node
/// before the first text, and that text.
pub(super) fn icon_and_label<'a>(cx: &Context<'a>, row: &'a Node) -> (Option<&'a Node>, Option<&'a Node>) {
    let below = descendants(cx, row);
    let label = below.iter().copied().find(|node| super::super::is_text(node) && is_visible(node));
    let (Some(label_area), Some(row_area)) = (label.and_then(area_of), area_of(row)) else {
        return (None, None);
    };
    let icon = below.iter().copied().find(|node| {
        node.kind == Kind::Other
            && is_visible(node)
            && area_of(node).is_some_and(|area| {
                area[2] <= ICON_MAX && area[3] <= ICON_MAX && area[0] + area[2] <= label_area[0] + COLUMN
            })
    });
    // The icon's slot is what the column is made of: a glyph is smaller than
    // its slot, and a profile's avatar is not the same size as a page's glyph.
    let slot = icon.map(|icon| {
        let mut slot = icon;
        while let Some(parent) = cx.parent_of(slot) {
            // A slot is a square box round the glyph, as small as a row's
            // icon cell and smaller than the row (a cell as tall as the row
            // is the row's own column, not a slot).
            let fits = parent.id != row.id
                && area_of(parent).is_some_and(|area| {
                    (area[2] - area[3]).abs() <= 1.0
                        && area[2] <= ICON_MAX
                        && area[3] < row_area[3] - 1.0
                        && area[0] + area[2] <= label_area[0] + COLUMN
                });
            if !fits {
                break;
            }
            slot = parent;
        }
        slot
    });
    (slot, label)
}

/// The rows whose icon and label columns are compared, by component.
const COLUMN_ROWS: [&str; 4] = ["SidebarRow", "PlaceRow", "FileRow", "Row"];

/// `AL-01`: rows of one kind in one column (the same left edge and width)
/// that have an icon put it in one column and start their label at one x.
/// `SZ-05`: trailing dropdowns of one card are one width.
pub(super) fn column_aligned(cx: &Context, out: &mut Vec<Finding>) {
    // (component, row left, row width) -> the rows with an icon.
    let mut columns: BTreeMap<(String, i64, i64, i64), Vec<(&Node, Box4, Box4)>> = BTreeMap::new();
    for node in &cx.snapshot.nodes {
        if !COLUMN_ROWS.contains(&component(node)) || !is_visible(node) {
            continue;
        }
        let (Some(row), (Some(icon), Some(label))) = (area_of(node), icon_and_label(cx, node)) else {
            continue;
        };
        let (Some(icon), Some(label)) = (area_of(icon), area_of(label)) else {
            continue;
        };
        let key = (
            component(node).to_owned(),
            (row[0] * 2.0).round() as i64,
            (row[2] * 2.0).round() as i64,
            // Rows whose icon slots are different widths lead with
            // different things (a glyph, an app icon): their labels start
            // apart by design.
            (icon[2] * 2.0).round() as i64,
        );
        columns.entry(key).or_default().push((node, icon, label));
    }
    for ((name, _, _, _), rows) in &columns {
        if rows.len() < 2 {
            continue;
        }
        for (what, pick, id) in [
            ("icon", 1usize, 0usize),
            ("label", 2usize, 1usize),
        ] {
            let value = |entry: &(&Node, Box4, Box4)| if pick == 1 { entry.1[0] } else { entry.2[0] };
            let modal = modal_value(rows.iter().map(value));
            for entry in rows {
                if (value(entry) - modal).abs() > COLUMN {
                    report(
                        cx,
                        out,
                        Rule::ColumnAligned,
                        entry.0,
                        format!(
                            "The {what} of this {name} starts at x {:.1}, the others of its column at {modal:.1}: {:.1} px off",
                            value(entry),
                            (value(entry) - modal).abs()
                        ),
                        vec![
                            (if id == 0 { "icon-x" } else { "label-x" }, value(entry)),
                            ("column-x", modal),
                            ("difference", (value(entry) - modal).abs()),
                        ],
                    );
                }
            }
        }
    }
    dropdown_widths(cx, out);
}

/// The value most rows share, the smallest of a tie.
fn modal_value(values: impl Iterator<Item = f32>) -> f32 {
    let mut counts: BTreeMap<i64, (usize, f32)> = BTreeMap::new();
    for value in values {
        let entry = counts.entry((value * 2.0).round() as i64).or_insert((0, value));
        entry.0 += 1;
    }
    counts
        .values()
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)))
        .map_or(0.0, |entry| entry.1)
}

/// The nearest ancestor that is a painted, rounded box: a card.
fn card_of<'a>(cx: &Context<'a>, node: &'a Node) -> Option<&'a Node> {
    let mut current = cx.parent_of(node);
    while let Some(here) = current {
        let rounded = here
            .props
            .as_ref()
            .is_some_and(|props| props.corner_radii.iter().any(|radius| *radius >= 8.0));
        if paints(here) && rounded {
            return Some(here);
        }
        current = cx.parent_of(here);
    }
    None
}

fn dropdown_widths(cx: &Context, out: &mut Vec<Finding>) {
    let mut cards: BTreeMap<u64, Vec<(&Node, f32)>> = BTreeMap::new();
    for node in &cx.snapshot.nodes {
        if component(node) != "Dropdown" || !is_visible(node) {
            continue;
        }
        let (Some(card), Some(area)) = (card_of(cx, node), area_of(node)) else {
            continue;
        };
        cards.entry(card.id).or_default().push((node, area[2]));
    }
    for dropdowns in cards.values() {
        if dropdowns.len() < 2 {
            continue;
        }
        let modal = modal_value(dropdowns.iter().map(|entry| entry.1));
        for (node, width) in dropdowns {
            if (width - modal).abs() > TOLERANCE {
                report(
                    cx,
                    out,
                    Rule::ColumnAligned,
                    node,
                    format!(
                        "This dropdown is {width:.0} px wide, the others of its card {modal:.0} px: dropdowns of one card share one width"
                    ),
                    vec![("width", *width), ("column-width", modal), ("difference", (width - modal).abs())],
                );
            }
        }
    }
}

/// The rows whose icon sits in the space above and below it.
const LIST_ROWS: [&str; 2] = ["SidebarRow", "PlaceRow"];

/// `SP-09`: a sidebar or place row's icon sits as far from the row's start edge
/// as from its top. A file row (`SP-10`) is a table row: it keeps its own
/// inset (`FILE_ROW_INSET`, held at every width, `PH-03`), which the parity
/// pass compares between the two sizes instead.
pub(super) fn list_row_inset(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        let name = component(node);
        if !LIST_ROWS.contains(&name) || !is_visible(node) {
            continue;
        }
        let (Some(row), (Some(icon), _)) = (area_of(node), icon_and_label(cx, node)) else {
            continue;
        };
        let Some(icon) = area_of(icon) else {
            continue;
        };
        // A row with a second line (a drive's usage) is taller than a nav
        // item and centres its icon: only a one-line row's x padding is held
        // to its y padding.
        let nav = if compact(cx) { 56.0 } else { 40.0 };
        if row[3] > nav + TOLERANCE {
            continue;
        }
        let left = icon[0] - row[0];
        let top = icon[1] - row[1];
        if (left - top).abs() > TOLERANCE {
            report(
                cx,
                out,
                Rule::ListRowInset,
                node,
                format!(
                    "The icon of this {name} is {left:.0} px from the start of the row and {top:.0} px from its top: the x padding should even out the y padding"
                ),
                vec![("icon-left", left), ("icon-top", top), ("row-height", row[3])],
            );
        }
    }
}

/// How far after the leading slot a field's text starts: S1.
const FIELD_TEXT_GAP: f32 = 4.0;

/// `AL-03`: in a field with a leading `IconSlot` the text starts S1 after the
/// slot.
pub(super) fn field_text_start(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if component(node) != "TextInput" || !is_visible(node) {
            continue;
        }
        let below = descendants(cx, node);
        let Some(slot) = below.iter().copied().find(|below| component(below) == "IconSlot") else {
            continue;
        };
        let Some(slot_area) = area_of(slot) else {
            continue;
        };
        let slot_ids: Vec<u64> = descendants(cx, slot).iter().map(|below| below.id).collect();
        let Some(text) = texts(cx, node)
            .into_iter()
            .find(|text| text.id != slot.id && !slot_ids.contains(&text.id))
        else {
            continue;
        };
        // Where the text starts, inside the padding the text node keeps.
        let start = text
            .used
            .content
            .map(|content| content.min_x())
            .or_else(|| area_of(text).map(|area| area[0]));
        let Some(start) = start else {
            continue;
        };
        let gap = start - (slot_area[0] + slot_area[2]);
        if (gap - FIELD_TEXT_GAP).abs() > TOLERANCE {
            report(
                cx,
                out,
                Rule::FieldTextStart,
                text,
                format!("The text of this field starts {gap:.0} px after its icon slot, not {FIELD_TEXT_GAP:.0} (S1)"),
                vec![("gap", gap), ("expected", FIELD_TEXT_GAP)],
            );
        }
    }
}

/// The controls of a header that are `Dim::Control` tall.
const HEADER_CONTROLS: [&str; 6] = [
    "IconButton",
    "IconMenuButton",
    "SplitButton",
    "Segmented",
    "SegmentedControl",
    "ActionButton",
];

/// Nothing may come nearer to a centred title than this before the title is
/// pushed aside, in logical pixels.
const TITLE_ROOM: f32 = 4.0;

/// `AL-04`, `SZ-02`: the header's title is centred on the whole bar (the
/// window controls count as part of its width), unless a control is in the
/// way, and every control of the header is `Dim::Control` tall: 36, or 44 at
/// compact density.
pub(super) fn header_measure(cx: &Context, out: &mut Vec<Finding>) {
    // A window narrower than 740 px is compact. A wider one may hold a
    // compact specimen (the taxonomy shows a header at touch size), so either
    // control height passes there.
    let narrow = cx.window.is_some_and(|window| window[2] < super::COMPACT_BELOW);
    let control = if narrow { 44.0 } else { 36.0 };
    for node in &cx.snapshot.nodes {
        if component(node) != "HeaderBar" || !is_visible(node) {
            continue;
        }
        let Some(bar) = area_of(node) else {
            continue;
        };
        let below = descendants(cx, node);
        // A bar that holds tabs is a tab strip: its buttons are inline
        // (design/tabs.md: close 24, new tab 28/32), not header controls.
        if below.iter().any(|below| matches!(component(below), "Tab" | "TabBar" | "TabStrip" | "TabEnd")) {
            continue;
        }
        for button in below.iter().filter(|below| {
            HEADER_CONTROLS.contains(&component(below)) && is_visible(below) && !below.used.out_of_flow
        }) {
            if let Some(area) = area_of(button).filter(|area| {
                (area[3] - control).abs() > TOLERANCE && (narrow || (area[3] - 44.0).abs() > TOLERANCE)
            }) {
                report(
                    cx,
                    out,
                    Rule::HeaderMeasure,
                    button,
                    format!(
                        "This header {} is {:.0} px tall, a header's controls are Dim::Control, {control:.0} px",
                        component(button),
                        area[3]
                    ),
                    vec![("height", area[3]), ("expected", control)],
                );
            }
        }
        let titles = texts(cx, node);
        // The centred title is drawn on its own layer over the whole bar
        // (out of the flow); a title that starts the bar (`title_start`) is in
        // the flow and is not centred.
        let title = titles.iter().copied().find(|text| {
            let mut current = cx.parent_of(text);
            let mut layered = false;
            while let Some(here) = current {
                if here.id == node.id {
                    return layered;
                }
                if focusable(here) || matches!(component(here), "WindowControls" | "Segment" | "Segmented") {
                    return false;
                }
                layered |= here.used.out_of_flow;
                current = cx.parent_of(here);
            }
            false
        });
        let Some(title) = title else {
            continue;
        };
        let Some(text) = area_of(title) else {
            continue;
        };
        let bar_centre = bar[0] + bar[2] / 2.0;
        let text_centre = text[0] + text[2] / 2.0;
        // The things the title may not run into.
        let blockers: Vec<Box4> = below
            .iter()
            .filter(|below| {
                is_visible(below) && (focusable(below) || component(below) == "WindowControls")
            })
            .filter_map(|below| area_of(below))
            .collect();
        let left = blockers
            .iter()
            .filter(|area| area[0] + area[2] / 2.0 < bar_centre)
            .map(|area| area[0] + area[2])
            .fold(f32::MIN, f32::max);
        let right = blockers
            .iter()
            .filter(|area| area[0] + area[2] / 2.0 >= bar_centre)
            .map(|area| area[0])
            .fold(f32::MAX, f32::min);
        let wanted = (bar_centre - text[2] / 2.0, bar_centre + text[2] / 2.0);
        if left > wanted.0 - TITLE_ROOM || right < wanted.1 + TITLE_ROOM {
            continue;
        }
        if (text_centre - bar_centre).abs() > TOLERANCE {
            report(
                cx,
                out,
                Rule::HeaderMeasure,
                title,
                format!(
                    "The title is centred {:.0} px {} of the header's centre, with room to centre it",
                    (text_centre - bar_centre).abs(),
                    if text_centre < bar_centre { "left" } else { "right" }
                ),
                vec![("title-centre", text_centre), ("header-centre", bar_centre)],
            );
        }
    }
}
