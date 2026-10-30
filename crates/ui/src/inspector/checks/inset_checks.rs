//! Insets: how far content sits from the edges of its box (`SP-01`) and how far
//! a trailing control sits from the top, bottom and end of its row (`SP-02`).

use super::super::{
    Box4, Context, Finding, Kind, Node, Rule, TOLERANCE, border_box, content_extent, is_visible, paints,
    sized_by_parent,
};
use super::{component, focusable, holds_focusable, insets, report};

/// The components that are a box with an inset by design: a card, a row, a
/// bar, a popup. An unnamed painted box is not one (a specimen's frame, a
/// swatch, a track), and a control's own face is held by the rules of
/// controls.
const BOXES: [&str; 13] = [
    "Row",
    "PropertyRow",
    "HeaderBar",
    "ActionStrip",
    "Banner",
    "Toast",
    "AlertDialog",
    "Dialog",
    "Popup",
    "Sheet",
    "StatusPage",
    "ProfileCard",
    "PopupBody",
];

/// S1 in logical pixels, which `style/tokens.rs` keeps the same at both densities.
const S1_PX: f32 = 4.0;

/// How much of a box's width the content must cover before its left and
/// right insets are compared: a short label at the start of a wide box
/// leads on purpose.
const SPANS: f32 = 0.85;

/// The boxes whose vertical insets are measured on what they hold (the ink),
/// not on the boxes of their children: a row's leading slot may be taller than
/// its icon. A card's rows are flush with it, and a banner's buttons have no
/// fill of their own, so those are measured on their children's boxes.
const INK_BOXES: [&str; 3] = ["Row", "PropertyRow", "HeaderBar"];

/// A box this rounded that a group paints round its rows is the group's card.
fn is_card(cx: &Context, node: &Node) -> bool {
    let rounded = node
        .props
        .as_ref()
        .is_some_and(|props| paints(node) && props.corner_radii.iter().any(|radius| *radius >= 12.0));
    rounded && cx.parent_of(node).is_some_and(|parent| component(parent) == "Group")
}

fn is_box(cx: &Context, node: &Node) -> bool {
    BOXES.contains(&component(node)) || is_card(cx, node)
}

/// The box of what a node's in-flow children paint: their ink, and what is
/// below it.
fn ink_extent(cx: &Context, node: &Node) -> Option<Box4> {
    cx.children_of(node)
        .filter(|child| is_visible(child) && !child.used.out_of_flow)
        .filter_map(|child| cx.within[cx.index[&child.id]])
        .fold(None, |extent, area| match extent {
            None => Some(area),
            Some(have) => Some(union_box(have, area)),
        })
}

fn union_box(a: Box4, b: Box4) -> Box4 {
    let (x, y) = (a[0].min(b[0]), a[1].min(b[1]));
    let right = (a[0] + a[2]).max(b[0] + b[2]);
    let down = (a[1] + a[3]).max(b[1] + b[3]);
    [x, y, right - x, down - y]
}

/// `SP-01`: the content of a card, row, header or popup has the same room at
/// the start and the end, and above and below, whatever padding the box
/// declares. `uneven-padding` only looks at boxes that declare equal padding
/// and size to their content; this measures the gaps to the four edges.
///
/// Left and right are compared only where the content spans the box and
/// neither end is a control: a row with a control at the end is held by
/// `trailing-inset` (its control sits 8 px in, its text 16).
pub(super) fn inset_asymmetry(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if node.kind != Kind::Rect || node.children.is_empty() || !is_visible(node) || node.used.out_of_flow {
            continue;
        }
        if !is_box(cx, node) || node.a11y.role == "ScrollView" {
            continue;
        }
        let Some(area) = border_box(node) else {
            continue;
        };
        let (wide, tall) = node
            .props
            .as_ref()
            .map_or((false, false), |props| (sized_by_parent(&props.width), sized_by_parent(&props.height)));
        let name = match component(node) {
            "" => "box",
            name => name,
        };
        // Above and below: the ink, so an empty line kept free for a message
        // counts as the dead space it is. A box taller than the window is a
        // virtual list: only the rows in view are built.
        let vertical = if INK_BOXES.contains(&component(node)) { ink_extent(cx, node) } else { content_extent(cx, node) };
        if let Some(ink) = vertical
            && !tall
            && cx.window.is_none_or(|window| area[3] <= window[3])
        {
            let [above, _, below, _] = insets(area, ink);
            // A count badge is nudged down by half an S1 (`Row`, "Optical":
            // the title's caps sit about 2 px below its line-box centre), so a
            // row with one is allowed S1 of difference. Content that leaves
            // the box is the overflow rules' business.
            let optical = if super::descendants(cx, node).iter().any(|below| component(below) == "Badge") {
                S1_PX
            } else {
                0.0
            };
            if above >= 0.0 && below >= 0.0 && (above - below).abs() > TOLERANCE + optical {
                report(
                    cx,
                    out,
                    Rule::InsetAsymmetry,
                    node,
                    format!(
                        "The content of this {name} is {above:.0} px from the top and {below:.0} px from the bottom: {:.0} px uneven",
                        (above - below).abs()
                    ),
                    vec![("inset-top", above), ("inset-bottom", below), ("difference", (above - below).abs())],
                );
            }
        }
        // At the start and the end: the boxes, so a slot kept free for an
        // icon that has not loaded still counts as the start of the row.
        let Some(ink) = structure_extent(cx, node) else {
            continue;
        };
        let [_, end, _, start] = insets(area, ink);
        let spans = ink[2] >= area[2] * SPANS;
        if spans
            && (start - end).abs() > TOLERANCE
            && !ends_are_controls(cx, node, ink)
            && !(wide && start < TOLERANCE)
        {
            report(
                cx,
                out,
                Rule::InsetAsymmetry,
                node,
                format!(
                    "The content of this {name} is {start:.0} px from the start and {end:.0} px from the end: {:.0} px uneven",
                    (start - end).abs()
                ),
                vec![("inset-start", start), ("inset-end", end), ("difference", (start - end).abs())],
            );
        }
    }
}

/// The union of the boxes of the in-flow children, ink or not.
fn structure_extent(cx: &Context, node: &Node) -> Option<Box4> {
    // An empty box kept for a trailing control that is not there still takes
    // its place in the row (and a gap before it), so it counts, though it has
    // no size.
    cx.children_of(node)
        .filter(|child| !child.used.hidden && !child.used.out_of_flow)
        .filter_map(border_box)
        .fold(None, |extent, area| match extent {
            None => Some(area),
            Some(have) => Some(union_box(have, area)),
        })
}

/// Whether the child at the start or at the end of `node`'s content is, or
/// holds, a control: that edge is a control's own inset.
fn ends_are_controls(cx: &Context, node: &Node, ink: Box4) -> bool {
    cx.children_of(node)
        .filter(|child| is_visible(child) && !child.used.out_of_flow)
        .filter_map(|child| border_box(child).map(|area| (child, area)))
        .any(|(child, area)| {
            let at_start = (area[0] - ink[0]).abs() <= TOLERANCE;
            let at_end = ((area[0] + area[2]) - (ink[0] + ink[2])).abs() <= TOLERANCE;
            (at_start || at_end) && holds_focusable(cx, child)
        })
}

/// A trailing control is within this of the end of its box.
const TRAILING_REACH: f32 = 24.0;

/// The components whose last control is held to one inset on three sides.
const TRAILING_HOSTS: [&str; 9] = [
    "Row",
    "PropertyRow",
    "HeaderBar",
    "Banner",
    "Toast",
    "TextInput",
    "TabBar",
    "TabStrip",
    "ProfileCard",
];

/// `SP-02`, `AL-09`, `SP-12`, `SZ-12`: the last control of a row, field,
/// toast or strip sits the same distance from the top, the bottom and the end
/// of its box. The chevron zone and a strip's end cell are flush by design:
/// they are full height and touch the end, so all three distances are 0.
pub(super) fn trailing_inset(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if !TRAILING_HOSTS.contains(&component(node)) || !is_visible(node) {
            continue;
        }
        let Some(host) = border_box(node) else {
            continue;
        };
        // The control nearest the end among those narrower than half the box:
        // a pressable row is itself focusable, and is not the control.
        let control = super::descendants(cx, node)
            .into_iter()
            .filter(|below| {
                focusable(below)
                    && is_visible(below)
                    && !below.used.out_of_flow
                    && !is_nested_host(below)
                    && !is_not_a_button(cx, node, below)
            })
            .filter_map(|below| border_box(below).map(|area| (below, area)))
            .filter(|(_, area)| area[2] <= host[2] / 2.0 && area[3] > 0.0)
            .max_by(|a, b| {
                (a.1[0] + a.1[2]).total_cmp(&(b.1[0] + b.1[2]))
            });
        let Some((control, area)) = control else {
            continue;
        };
        let [top, end, bottom, _] = insets(host, area);
        if end > TRAILING_REACH || end < -TOLERANCE {
            continue;
        }
        // Another host between this one and the control owns it.
        if owned_by_inner_host(cx, node, control) {
            continue;
        }
        // A control below the middle of a banner is stacked under its text.
        if matches!(component(node), "Banner" | "Toast") && area[1] > host[1] + host[3] / 2.0 {
            continue;
        }
        // A row taller than one line (a subtitle: 63 px) is as high as its
        // text, so a 40 px button is centred in it and cannot be 8 px from the
        // top and bottom as well as the end on the scale: only its vertical
        // centring is held. A banner whose message wraps is the same.
        let tall = match component(node) {
            "Row" | "PropertyRow" => host[3] > row_height(cx) + TOLERANCE,
            "Banner" | "Toast" => host[3] > area[3] + 2.0 * end + TOLERANCE,
            _ => false,
        };
        let name = component(node);
        if (top - bottom).abs() > TOLERANCE || (!tall && (end - top).abs() > TOLERANCE) {
            report(
                cx,
                out,
                Rule::TrailingInset,
                control,
                format!(
                    "The last control of this {name} is {top:.0} px from the top, {bottom:.0} px from the bottom and {end:.0} px from the end: it should be one inset on all three"
                ),
                vec![
                    ("inset-top", top),
                    ("inset-bottom", bottom),
                    ("inset-end", end),
                    ("control-height", area[3]),
                    ("host-height", host[3]),
                ],
            );
        }
    }
}

/// The height of a one-line row at the window's density: `Dim::Row`.
fn row_height(cx: &Context) -> f32 {
    if super::compact(cx) { 60.0 } else { 56.0 }
}

/// Whether `control` is inside a field or a slider that is itself inside
/// `host`: the control is not a button then ([`super::NOT_BUTTONS`]).
fn is_not_a_button(cx: &Context, host: &Node, control: &Node) -> bool {
    let mut current = Some(control);
    while let Some(here) = current {
        if here.id == host.id {
            return false;
        }
        if super::NOT_BUTTONS
            .iter()
            .any(|(name, _)| *name == component(here) || *name == here.a11y.role)
        {
            return true;
        }
        current = cx.parent_of(here);
    }
    false
}

/// A control that is itself a host of controls is not the trailing control.
fn is_nested_host(node: &Node) -> bool {
    TRAILING_HOSTS.contains(&component(node))
}

/// Whether a host closer to `control` than `outer` holds it.
fn owned_by_inner_host(cx: &Context, outer: &Node, control: &Node) -> bool {
    let mut current = cx.parent_of(control);
    while let Some(here) = current {
        if here.id == outer.id {
            return false;
        }
        if TRAILING_HOSTS.contains(&component(here)) {
            return true;
        }
        current = cx.parent_of(here);
    }
    false
}
