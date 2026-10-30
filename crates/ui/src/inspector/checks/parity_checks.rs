//! Phone and desktop parity (`PH-01`, `PH-03`, `SP-13`). One window shows one
//! density, so the comparison is between two captures of the same page:
//! this module measures the named insets of each component class in one
//! capture ([`metrics`]), `BINGUX_UI_LAYOUT_LINT` writes them next to the
//! findings, and `scripts/layout-lint.py --parity` compares the desktop and
//! the 390x800 capture of every page.

use super::super::{Box4, Context, Kind, Node, content_extent, is_visible, paints};
use super::align_checks::icon_and_label;
use super::{area_of, component, descendants, focusable, insets};

/// One measured inset of one component, by the component's class and the
/// inset's name, in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub class: String,
    pub name: &'static str,
    pub value: f32,
}

/// The components whose insets are compared.
const CLASSES: [&str; 24] = [
    "SidebarRow",
    "PlaceRow",
    "FileRow",
    "Row",
    "PropertyRow",
    "HeaderBar",
    "Group",
    "ActionStrip",
    "AlertDialog",
    "Dialog",
    "Popup",
    "Sheet",
    "PopupBody",
    "Banner",
    "Toast",
    "TextInput",
    "Page",
    "PageColumn",
    "StatusPage",
    "ProfileCard",
    "Menu",
    "DropdownItem",
    "Tab",
    "TabBar",
];

/// The boxes whose content insets are compared: a card or a popup, whose free
/// content sits a token inset in. A row's or a bar's content is placed by its
/// own height and what it holds.
const CARDS: [&str; 9] = [
    "Group",
    "ActionStrip",
    "AlertDialog",
    "Dialog",
    "Popup",
    "Sheet",
    "PopupBody",
    "Banner",
    "Toast",
];

/// A control within this of the end of its row trails it, in logical pixels.
const TRAILING_REACH: f32 = 24.0;

/// The rows that hold an icon and a label.
const ROWS: [&str; 5] = ["SidebarRow", "PlaceRow", "FileRow", "Row", "PropertyRow"];

/// The named insets of every component class of the capture: its padding, its
/// gap, its height, where its content sits inside it and, for a row, where
/// its icon, its label and its trailing control sit.
pub(crate) fn metrics(cx: &Context) -> Vec<Metric> {
    let mut out = Vec::new();
    for node in &cx.snapshot.nodes {
        let class = component(node);
        if !CLASSES.contains(&class) || !is_visible(node) {
            continue;
        }
        let Some(area) = area_of(node) else {
            continue;
        };
        // A row's insets depend on what it holds (one line or tall, which
        // control ends it, which kind of icon starts it), so each kind is its
        // own class: the comparison is between rows of one shape.
        let class = if ROWS.contains(&class) { row_class(cx, node, area) } else { class.to_owned() };
        let mut push = |name: &'static str, value: f32| {
            out.push(Metric {
                class: class.clone(),
                name,
                value,
            });
        };
        let [top, right, bottom, left] = node.used.padding.unwrap_or([0.0; 4]);
        push("padding-top", top);
        push("padding-right", right);
        push("padding-bottom", bottom);
        push("padding-left", left);
        push("height", area[3]);
        if let Some(props) = &node.props {
            push("gap", props.gap);
            if let Some(radius) = props.corner_radii.first().filter(|_| paints(node)) {
                push("radius", *radius);
            }
        }
        if node.kind == Kind::Rect
            && CARDS.contains(&component(node))
            && let Some(ink) = content_extent(cx, node)
        {
            let [above, end, below, start] = insets(area, ink);
            push("content-top", above);
            push("content-bottom", below);
            push("content-start", start);
            push("content-end", end);
        }
        if ROWS.contains(&component(node)) {
            row_metrics(cx, node, area, &mut push);
        }
    }
    out
}

/// The class of a row: its component, whether it is one line, the kind of its
/// leading icon and of its trailing control.
fn row_class(cx: &Context, node: &Node, area: Box4) -> String {
    let (icon, _) = icon_and_label(cx, node);
    let leading = icon.map_or("none", |icon| {
        descendants(cx, icon)
            .into_iter()
            .chain(std::iter::once(icon))
            .map(|below| component(below))
            .find(|name| !name.is_empty())
            .unwrap_or("icon")
    });
    let control = descendants(cx, node)
        .into_iter()
        .filter(|below| focusable(below) && is_visible(below) && !below.used.out_of_flow)
        .filter_map(|below| area_of(below).map(|area| (below, area)))
        .filter(|(_, below)| below[2] <= area[2] / 2.0)
        .max_by(|a, b| (a.1[0] + a.1[2]).total_cmp(&(b.1[0] + b.1[2])))
        .map_or("none", |(below, _)| match component(below) {
            "" => match below.a11y.role.as_str() {
                "" => "control",
                role => role,
            },
            name => name,
        });
    let lines = if area[3] <= 60.5 { "1" } else { "n" };
    format!("{}[{lines}|{leading}|{control}]", component(node))
}

fn row_metrics(cx: &Context, node: &Node, row: Box4, push: &mut dyn FnMut(&'static str, f32)) {
    let (icon, label) = icon_and_label(cx, node);
    if let Some(icon) = icon.and_then(area_of) {
        push("icon-left", icon[0] - row[0]);
    }
    if let Some(label) = label.and_then(area_of) {
        push("label-start", label[0] - row[0]);
    }
    let control = descendants(cx, node)
        .into_iter()
        .filter(|below| focusable(below) && is_visible(below) && !below.used.out_of_flow)
        .filter_map(area_of)
        .filter(|area| area[2] <= row[2] / 2.0)
        .max_by(|a, b| (a[0] + a[2]).total_cmp(&(b[0] + b[2])));
    if let Some(control) = control {
        let [_, end, _, _] = insets(row, control);
        // A control that is not at the end (stacked under its label on a
        // phone) has no trailing inset.
        if end <= TRAILING_REACH {
            push("trailing-end", end);
        }
    }
}
