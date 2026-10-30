//! Scales: spacing is a Space step (`SP-15`, `SP-21`) and radii are nested
//! (`BR-01`, `BR-02`).

use super::super::{Box4, Context, Finding, Kind, Node, Rule, THIN, border_box, is_visible, paints};
use super::{component, report};

/// The spacing a box may have at regular density: `S0` to `S12` of
/// `style/tokens.rs` and the two insets that are not a step, `FIELD_ROW_INSET`
/// (10) and `SWITCH_INSET` (18).
const REGULAR: [f32; 14] = [0.0, 2.0, 4.0, 8.0, 10.0, 12.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 40.0, 48.0];

/// The same at compact density: each step's compact value, and the held
/// insets (`DIALOG_INSET` 12, `NAV_INSET` and `SWITCH_INSET` 16, the 8 and 12
/// that `same()` tokens keep).
const COMPACT: [f32; 14] = [0.0, 2.0, 4.0, 8.0, 12.0, 14.0, 16.0, 18.0, 22.0, 26.0, 32.0, 36.0, 44.0, 54.0];

/// A value above the scale's top that is a whole number is a sum of steps
/// (a sheet's bottom plus the safe area, a page's top plus its title).
const SCALE_TOP: f32 = 54.0;

fn on_scale(value: f32, narrow: bool) -> bool {
    let value = value.abs();
    let near = |steps: &[f32]| steps.iter().any(|step| (step - value).abs() < 0.05);
    // A window narrower than 740 px is compact. A wider one may hold a
    // compact specimen (the taxonomy shows both densities on one page), so a
    // value of either density passes there.
    let known = if narrow { near(&COMPACT) } else { near(&REGULAR) || near(&COMPACT) };
    known || (value > SCALE_TOP && (value - value.round()).abs() < 0.05)
}

/// `SP-15`, `SP-21`: every padding, margin and gap of the rendered tree is a
/// Space step at the window's density. The source is held by `ui-lint`; this
/// catches a measured width pushed into a gap, a hand-set number inside a
/// component, and a number from the wrong density.
pub(super) fn off_scale(cx: &Context, out: &mut Vec<Finding>) {
    let compact = cx.window.is_some_and(|window| window[2] < super::COMPACT_BELOW);
    for node in &cx.snapshot.nodes {
        if !is_visible(node) {
            continue;
        }
        let mut off: Vec<(&'static str, f32)> = Vec::new();
        let padding = node.used.padding.unwrap_or([0.0; 4]);
        for (name, value) in ["padding-top", "padding-right", "padding-bottom", "padding-left"]
            .into_iter()
            .zip(padding)
        {
            if !on_scale(value, compact) {
                off.push((name, value));
            }
        }
        // A thin box is a scrollbar thumb: the layout places it by its margin.
        let thin = border_box(node).is_some_and(|area| area[2].min(area[3]) < THIN);
        for (name, value) in ["margin-top", "margin-right", "margin-bottom", "margin-left"]
            .into_iter()
            .zip(node.used.margin)
        {
            if !thin && !on_scale(value, compact) {
                off.push((name, value));
            }
        }
        if let Some(props) = &node.props
            && !on_scale(props.gap, compact)
        {
            off.push(("gap", props.gap));
        }
        if off.is_empty() {
            continue;
        }
        let list = off
            .iter()
            .map(|(name, value)| format!("{name} {value:.1}"))
            .collect::<Vec<_>>()
            .join(", ");
        report(
            cx,
            out,
            Rule::OffScale,
            node,
            format!(
                "{list} px is not a Space step at {} density",
                if compact { "compact" } else { "regular or compact" }
            ),
            off,
        );
    }
}

/// Components whose capsule (a pill wider than it is tall) is round by design,
/// with why. A box inside one of them may be a pill.
pub(super) const PILLS: &[(&str, &str)] = &[
    ("Switch", "A switch's track is a pill: a switch is round by design."),
    ("Banner", "A banner and a toast are a pill: the notice is its button height plus 4 either side, round by design (design/rules.md padding model, 'Banner and toast')."),
    ("Toast", "A toast is a pill by design (see Banner)."),
    ("DockPreview", "The dock in the desktop preview is a picture of the real dock, a pill with round app slots and running dots."),
    ("MediaControls", "The media controls float over the picture as a pill: an overlay by design."),
    ("PreviewHost", "The preview's floating info and zoom bar is a pill over the picture: an overlay by design."),
    ("TreeView", "A tree row's count badge is a pill."),
    ("QueryPill", "A query pill is a pill, by name."),
    ("Badge", "A badge is a pill."),
];

/// The components whose face is a button: a circle there is the round icon
/// button that `BR-02` forbids (icon buttons follow the radii).
const BUTTONS: [&str; 8] = [
    "IconButton",
    "IconMenuButton",
    "ActionButton",
    "RowButton",
    "SplitButton",
    "TabClose",
    "NoticeAction",
    "MenuButton",
];

/// `Radius::Pill` is 999: a radius this big is fully round whatever the box.
const PILL_RADIUS: f32 = 100.0;

/// A box this small is a mark or a dot, not a shape to check.
const SHAPE_MIN: f32 = 16.0;

/// How far a radius may be from the nested one, in logical pixels.
const RADIUS_TOLERANCE: f32 = 0.5;

/// `Radius::nested` of `style/tokens.rs`: the outer radius less the inset,
/// rounded down to Control 12, Inner 8 or Mark 4.
fn nested(outer: f32, inset: f32) -> f32 {
    let wanted = outer - inset;
    [12.0, 8.0, 4.0]
        .into_iter()
        .find(|radius| *radius <= wanted + 0.01)
        .unwrap_or(4.0)
}

fn radii(node: &Node) -> Option<[f32; 4]> {
    let props = node.props.as_ref()?;
    paints(node).then_some(props.corner_radii)
}

/// The nearest painted ancestor in the same layer.
fn painted_ancestor<'a>(cx: &Context<'a>, node: &'a Node) -> Option<&'a Node> {
    let mut current = cx.parent_of(node);
    while let Some(here) = current {
        if cx.layer[cx.index[&here.id]] != cx.layer[cx.index[&node.id]] {
            return None;
        }
        if radii(here).is_some() && border_box(here).is_some() {
            return Some(here);
        }
        current = cx.parent_of(here);
    }
    None
}

/// The distances of corner `corner` (top left, top right, bottom right,
/// bottom left) of `inner` from the two sides of `outer` it meets.
fn corner_insets(outer: Box4, inner: Box4, corner: usize) -> (f32, f32) {
    let (left, right) = (inner[0] - outer[0], (outer[0] + outer[2]) - (inner[0] + inner[2]));
    let (top, bottom) = (inner[1] - outer[1], (outer[1] + outer[3]) - (inner[1] + inner[3]));
    match corner {
        0 => (left, top),
        1 => (right, top),
        2 => (right, bottom),
        _ => (left, bottom),
    }
}

/// `BR-01`: a painted box that shares a corner with its painted ancestor, an
/// even inset d in from both sides, has the ancestor's radius less d. A box
/// flush with the corner (d = 0) is square or takes the ancestor's radius.
/// `BR-02`: a box this round (a pill) is only a switch, avatar, swatch, badge,
/// notice or track ([`PILLS`]).
pub(super) fn radius_concentric(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if node.kind != Kind::Rect || !is_visible(node) {
            continue;
        }
        let (Some(own), Some(area)) = (radii(node), border_box(node)) else {
            continue;
        };
        let shorter = area[2].min(area[3]);
        let round = own.iter().all(|radius| *radius >= PILL_RADIUS);
        if round {
            // A circle (a knob, a dot, a swatch, a radio) is round by design,
            // except where it is a button's face; a capsule is a pill only
            // inside the components that list it.
            let circle = (area[2] - area[3]).abs() <= 1.0;
            let face = BUTTONS.contains(&component(node));
            let wrong = if circle { face } else { !in_pill_component(cx, node) };
            if shorter >= SHAPE_MIN && wrong {
                report(
                    cx,
                    out,
                    Rule::RadiusConcentric,
                    node,
                    format!(
                        "A {:.0} by {:.0} box with a pill radius ({:.0} px): a button face is never a circle and a capsule is only a switch, notice or overlay bar",
                        area[2], area[3], own[0].min(shorter / 2.0)
                    ),
                    vec![("radius", own[0]), ("width", area[2]), ("height", area[3])],
                );
            }
            continue;
        }
        let Some(parent) = painted_ancestor(cx, node) else {
            continue;
        };
        let (Some(outer), Some(parent_radii)) = (border_box(parent), radii(parent)) else {
            continue;
        };
        for corner in 0..4 {
            let outer_radius = parent_radii[corner];
            if outer_radius <= 0.0 {
                continue;
            }
            let (across, down) = corner_insets(outer, area, corner);
            if across < -RADIUS_TOLERANCE
                || down < -RADIUS_TOLERANCE
                || across > outer_radius / 2.0
                || down > outer_radius / 2.0
                || (across - down).abs() > RADIUS_TOLERANCE * 2.0
            {
                continue;
            }
            let inset = (across + down) / 2.0;
            let wanted = nested(outer_radius, inset);
            let have = own[corner];
            let flush = inset < RADIUS_TOLERANCE;
            let ok = (have - wanted).abs() <= RADIUS_TOLERANCE
                || (have - (outer_radius - inset)).abs() <= RADIUS_TOLERANCE
                || (flush && (have == 0.0 || (have - outer_radius).abs() <= RADIUS_TOLERANCE));
            if !ok {
                report(
                    cx,
                    out,
                    Rule::RadiusConcentric,
                    node,
                    format!(
                        "Corner {} is {have:.0} px round, {inset:.0} px inside a {outer_radius:.0} px corner of {}: concentric is {wanted:.0} px",
                        ["top left", "top right", "bottom right", "bottom left"][corner],
                        match component(parent) {
                            "" => "its card",
                            name => name,
                        }
                    ),
                    vec![("radius", have), ("outer-radius", outer_radius), ("inset", inset), ("expected", wanted)],
                );
                break;
            }
        }
    }
}

fn in_pill_component(cx: &Context, node: &Node) -> bool {
    let path = cx.path_of(node);
    path.split(" > ")
        .any(|name| PILLS.iter().any(|(pill, _)| *pill == name))
}
