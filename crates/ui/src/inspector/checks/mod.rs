//! The spacing and alignment checks of the Layout tab: rules that hold the
//! padding model of `design/rules.md` to the measured tree. Each one cites its
//! rule IDs from `design/rules.json` (`[SP-01]`) in every finding, so a
//! finding can be read against what Kieran said.
//!
//! This module is the one registration point: [`run`] calls every rule,
//! [`meta`] says what each rule's key, label, reason and fix are, and
//! [`EXEMPT`] lists the components a rule leaves alone by design, each with
//! its reason. The rules themselves are in the files below.

use super::{Box4, Context, Finding, Node, Rule, border_box, is_visible};

mod align_checks;
mod inset_checks;
pub(super) mod parity_checks;
mod scale_checks;
#[cfg(test)]
mod tests;

/// Below this window width the apps lay out at compact (finger) density.
const COMPACT_BELOW: f32 = 740.0;

/// Runs every rule of this module.
pub(super) fn run(cx: &Context, out: &mut Vec<Finding>) {
    inset_checks::inset_asymmetry(cx, out);
    inset_checks::trailing_inset(cx, out);
    align_checks::column_aligned(cx, out);
    align_checks::list_row_inset(cx, out);
    align_checks::field_text_start(cx, out);
    align_checks::header_measure(cx, out);
    scale_checks::radius_concentric(cx, out);
    scale_checks::off_scale(cx, out);
}

/// The rules of this module, in the order the tab lists them.
pub(super) const RULES: [Rule; 8] = [
    Rule::InsetAsymmetry,
    Rule::TrailingInset,
    Rule::ColumnAligned,
    Rule::ListRowInset,
    Rule::FieldTextStart,
    Rule::HeaderMeasure,
    Rule::RadiusConcentric,
    Rule::OffScale,
];

/// What a rule of this module says about itself.
pub(super) struct Meta {
    pub key: &'static str,
    pub label: &'static str,
    /// The rule IDs of `design/rules.json` it enforces.
    pub ids: &'static str,
    pub why: &'static str,
    pub fix: &'static str,
}

pub(super) fn meta(rule: Rule) -> Meta {
    match rule {
        Rule::InsetAsymmetry => Meta {
            key: "inset-asymmetry",
            label: "Inset asymmetry",
            ids: "SP-01",
            why: "Content sits as far from the start edge of its card, row, header or popup as from the end edge, and as far from the top as from the bottom, whatever padding the box declares. A row with a control at the end follows the trailing-inset rule instead.",
            fix: "Give both sides one inset token, take the extra space off the heavier side, or (if the box leads on purpose) allow it with the reason.",
        },
        Rule::TrailingInset => Meta {
            key: "trailing-inset",
            label: "Trailing inset",
            ids: "SP-02, AL-09, SP-12, SZ-12",
            why: "The last button, switch, dropdown or icon button of a row, field, toast or strip sits the same distance from the top, the bottom and the end of its box, so it is flush with the box's own padding.",
            fix: "Size the control to the box less twice one inset (Dim::RowButton, Dim::InlineButton, NOTICE_BUTTON) and set the end inset to the same token (ROW_INSET, SWITCH_INSET, FIELD_INSET, NOTICE_PAD).",
        },
        Rule::ColumnAligned => Meta {
            key: "column-aligned",
            label: "Column not aligned",
            ids: "AL-01, SZ-05",
            why: "Rows of one kind in one column share one icon column and one label start; trailing dropdowns of one card share one width.",
            fix: "Build the row from the shared component (SidebarRow, Row) and give the controls one width token.",
        },
        Rule::ListRowInset => Meta {
            key: "list-row-inset",
            label: "List row inset",
            ids: "SP-09, SP-10",
            why: "A sidebar or list row's icon sits as far from the row's start edge as from its top: the x padding evens out the y padding, at both densities.",
            fix: "Set the row's side padding to NAV_INSET (the space above and below its icon), or give the file row its own token and allow it with the reason.",
        },
        Rule::FieldTextStart => Meta {
            key: "field-text-start",
            label: "Field text start",
            ids: "AL-03, SF-01",
            why: "In a field with a leading icon slot the text starts S1 (4 px) after the slot, at every width.",
            fix: "Use the shared text field's leading slot (IconSlot) and take any padding off the text that is not the slot's gap.",
        },
        Rule::HeaderMeasure => Meta {
            key: "header-measure",
            label: "Header measure",
            ids: "AL-04, SZ-02",
            why: "A header's title is centred on the header, window controls included in its width, and every button, segmented control and split button in a header is Dim::Control tall (36, 44 at compact density).",
            fix: "Do not size a header's buttons by hand (HEADER_BUTTON), and give the title the whole bar to centre in.",
        },
        Rule::RadiusConcentric => Meta {
            key: "radius-concentric",
            label: "Radius not concentric",
            ids: "BR-01, BR-02",
            why: "A box inside a rounded box that shares its corner at an even inset d (at most half the outer radius) has the outer radius less d (Radius::nested), so the two curves are concentric; a button's face is never a circle and a capsule is only a switch, notice or overlay bar.",
            fix: "Set the radius with Radius::nested(inset) from the outer box's radius, take the round radius off the button face, or (a capsule by design) list the component in PILLS with the reason.",
        },
        Rule::OffScale => Meta {
            key: "off-scale-spacing",
            label: "Off-scale spacing",
            ids: "SP-15, SP-21",
            why: "Every padding, margin and gap of the rendered tree is a Space step (or a held inset token), never an in-between number; a window narrower than 740 px uses the compact steps.",
            fix: "Use a Space token (S1..S12) or the inset token for the job; a number measured from content must not become a gap.",
        },
        // Only the rules of this module are asked.
        _ => Meta {
            key: "",
            label: "",
            ids: "",
            why: "",
            fix: "",
        },
    }
}

/// Controls that are in a row's end but are not buttons: the trailing-inset
/// rule does not measure them, with why.
pub(super) const NOT_BUTTONS: &[(&str, &str)] = &[
    (
        "TextField",
        "A field in a form row keeps the row's 16 px gutter at its end and FIELD_ROW_INSET (10) above and below: design/rules.md padding model, 'One-line field in a form row'. A field's own trailing button is still held to one inset.",
    ),
    (
        "ColourButtonHost",
        "A colour field in a form row is a field: it keeps the row's 16 px gutter at its end and FIELD_ROW_INSET (10) above and below.",
    ),
    (
        "TabClose",
        "A tab's close button belongs to the tab, which is flush with its strip by design (design/tabs.md: close 24 px in a 36 px tab).",
    ),
    (
        "Slider",
        "A slider is a track and a thumb, not a button: its track ends at the row's gutter and its thumb is held by Dim::Control, not ROW_INSET.",
    ),
];

/// A component a rule leaves alone, with why.
pub(super) struct Exempt {
    pub rule: Rule,
    /// A component name that must be the node's own or one of the components
    /// it is built in.
    pub component: &'static str,
    pub reason: &'static str,
}

/// Where a rule does not apply by design. A finding in a component listed
/// here is not reported at all (the lint's own allowlist,
/// `ALLOWED` in `scripts/layout-lint.py`, is for one capture at a time).
pub(super) const EXEMPT: &[Exempt] = &[
    Exempt {
        rule: Rule::TrailingInset,
        component: "HeaderBar",
        reason: "The header rail's side inset is 8 px (the list edge) while its 36 px buttons sit in a 48 px bar, 6 px above and below: design/rules.md padding model, 'Header bar'.",
    },
];

/// Whether `rule` leaves `node` alone, by a component the node is built in.
pub(super) fn exempt(cx: &Context, rule: Rule, node: &Node) -> bool {
    let path = cx.path_of(node);
    EXEMPT
        .iter()
        .filter(|entry| entry.rule == rule)
        .any(|entry| path.split(" > ").any(|name| name == entry.component))
}

/// Reports a finding of `rule` on `node` unless the rule is exempt there. The
/// message starts with the rule's IDs.
pub(super) fn report(
    cx: &Context,
    out: &mut Vec<Finding>,
    rule: Rule,
    node: &Node,
    what: String,
    numbers: Vec<(&'static str, f32)>,
) {
    if exempt(cx, rule, node) {
        return;
    }
    let what = format!("[{}] {what}", meta(rule).ids);
    out.push(cx.finding(rule, node, what, numbers));
}

/// The name of the component that built the node, or "".
fn component(node: &Node) -> &str {
    node.a11y.component.as_deref().unwrap_or("")
}

/// Whether the window lays out at compact density. A window narrower than
/// 740 px does. A wider one may still be shown at compact density (the
/// taxonomy's density switch), so the first bar or row of the tree that has
/// the height of one density settles it; with none, regular.
fn compact(cx: &Context) -> bool {
    if cx.window.is_some_and(|window| window[2] < COMPACT_BELOW) {
        return true;
    }
    for node in &cx.snapshot.nodes {
        let Some(area) = border_box(node).filter(|_| is_visible(node)) else {
            continue;
        };
        let height = area[3];
        let evidence = match component(node) {
            // HEADER 48 | 56.
            "HeaderBar" if (height - 48.0).abs() < 0.5 => Some(false),
            "HeaderBar" if (height - 56.0).abs() < 0.5 => Some(true),
            // Dim::NavItem 40 | 56.
            "SidebarRow" if (height - 40.0).abs() < 0.5 => Some(false),
            "SidebarRow" if (height - 56.0).abs() < 0.5 => Some(true),
            // Dim::Row 56 | 60, one line.
            "Row" if (height - 56.0).abs() < 0.5 => Some(false),
            "Row" if (height - 60.0).abs() < 0.5 => Some(true),
            _ => None,
        };
        if let Some(compact) = evidence {
            return compact;
        }
    }
    false
}

/// Whether the node is a control: it takes focus, or its accessibility role is
/// one a person presses, toggles or types in (a dropdown's face is a combo
/// box that takes the press for its row).
fn focusable(node: &Node) -> bool {
    node.a11y.focusable == Some(true)
        || matches!(
            node.a11y.role.as_str(),
            "ComboBox" | "Button" | "Switch" | "CheckBox" | "RadioButton" | "ToggleButton" | "SpinButton" | "Slider" | "TextInput"
        )
}

/// Every node below `node`, in preorder.
fn descendants<'a>(cx: &Context<'a>, node: &'a Node) -> Vec<&'a Node> {
    let mut out = Vec::new();
    let mut stack: Vec<&Node> = cx.children_of(node).collect();
    stack.reverse();
    while let Some(here) = stack.pop() {
        out.push(here);
        let mut children: Vec<&Node> = cx.children_of(here).collect();
        children.reverse();
        stack.extend(children);
    }
    out
}

/// Whether `node` or a node below it can take focus.
fn holds_focusable(cx: &Context, node: &Node) -> bool {
    focusable(node) || descendants(cx, node).iter().any(|below| focusable(below))
}

/// The distances from `inner` to the four sides of `outer`: top, right,
/// bottom, left.
fn insets(outer: Box4, inner: Box4) -> [f32; 4] {
    [
        inner[1] - outer[1],
        (outer[0] + outer[2]) - (inner[0] + inner[2]),
        (outer[1] + outer[3]) - (inner[1] + inner[3]),
        inner[0] - outer[0],
    ]
}

/// The visible text nodes below `node`.
fn texts<'a>(cx: &Context<'a>, node: &'a Node) -> Vec<&'a Node> {
    descendants(cx, node)
        .into_iter()
        .filter(|below| super::is_text(below) && is_visible(below))
        .collect()
}

fn area_of(node: &Node) -> Option<Box4> {
    border_box(node)
}
