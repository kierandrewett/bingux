//! The checks that need the frame's geometry and nothing else: touch targets
//! (CK-12) and the clearance of controls and scrollbars from the resize
//! handles and the header (CK-13).

use torin::prelude::Area;

use super::rig::Rig;
use super::view::{Target, border, describe_node};
use super::{Report, Rule};
use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::window::popup::{RESIZE_CORNER, RESIZE_INSIDE, RESIZE_OUTSIDE};

/// The touch minimum at compact density, in logical pixels (SZ-01).
pub(super) const TOUCH_MIN: f32 = 44.0;

/// A box smaller than this on a side is not a scrollbar or a handle.
const TOLERANCE: f32 = 0.5;

pub(super) fn run(rig: &mut Rig, targets: &[Target], report: &mut Report) {
    if rig.compact() {
        touch(targets, report);
    }
    // Wheel a pixel and back so a scrollbar that hides itself when idle is
    // drawn while the geometry is read.
    let centre = rig.window.center();
    let at = (centre.x as f64 * rig.scale as f64, centre.y as f64 * rig.scale as f64);
    rig.shot.runner().scroll(at, (0.0, -1.0));
    rig.settle(2);
    let shown = rig.tree();
    rig.shot.runner().scroll(at, (0.0, 1.0));
    rig.settle(2);
    clearance(&shown, targets, report);
}

/// SZ-01: every interactive node is a touch target at compact density.
fn touch(targets: &[Target], report: &mut Report) {
    for target in targets.iter().filter(|target| target.control()) {
        report.measure(Rule::Touch);
        let (width, height) = (target.hit.width(), target.hit.height());
        if width + TOLERANCE < TOUCH_MIN || height + TOLERANCE < TOUCH_MIN {
            report.push(
                Rule::Touch,
                target.label(),
                format!("{width:.0} x {height:.0} px; a finger needs {TOUCH_MIN:.0} x {TOUCH_MIN:.0}"),
                target.rect(),
            );
        }
    }
}

/// The resize handles of the frame: the eight absolute boxes a popup draws
/// beside its panel (four corner squares, four edge bands), siblings in one
/// parent. A box of that size on its own (a slider's 6 px track) is not one.
pub(super) fn handles(tree: &Tree) -> Vec<Area> {
    let corner = format!("{RESIZE_CORNER}");
    let band = format!("{}", RESIZE_OUTSIDE + RESIZE_INSIDE);
    let mut by_parent: std::collections::BTreeMap<u64, Vec<(bool, Area)>> = Default::default();
    for node in tree.nodes.iter().filter(|node| node.kind == Kind::Rect && node.used.out_of_flow) {
        let (Some(props), Some(parent), Some(area)) = (node.props.as_ref(), node.parent, border(node)) else {
            continue;
        };
        let (w, h) = (props.width.as_str(), props.height.as_str());
        if w == corner && h == corner {
            by_parent.entry(parent).or_default().push((true, area));
        } else if (w == band) != (h == band) && w != "auto" && h != "auto" {
            by_parent.entry(parent).or_default().push((false, area));
        }
    }
    by_parent
        .into_values()
        .filter(|group| group.iter().filter(|(corner, _)| *corner).count() == 4 && group.len() == 8)
        .flat_map(|group| group.into_iter().map(|(_, area)| area))
        .collect()
}

/// A scrollbar track: the absolute box of a scroll view's bar, 20 px across
/// and as long as the track.
pub(super) fn scrollbars(tree: &Tree) -> Vec<&Node> {
    tree.nodes
        .iter()
        .filter(|node| node.kind == Kind::Rect && node.used.out_of_flow)
        .filter(|node| {
            let Some(props) = node.props.as_ref() else { return false };
            let thin = |side: &str| side == "20";
            let long = |side: &str| side.parse::<f32>().is_ok_and(|px| px > 20.0);
            (thin(&props.width) && long(&props.height)) || (thin(&props.height) && long(&props.width))
        })
        .filter(|node| node.children.len() == 2)
        .collect()
}

fn overlap(a: Area, b: Area) -> Option<Area> {
    a.intersection(&b).filter(|both| both.width() > TOLERANCE && both.height() > TOLERANCE)
}

/// SZ-06, SC-01, SC-02: nothing interactive and no scrollbar under a resize
/// handle, and no scrollbar under the header.
fn clearance(tree: &Tree, targets: &[Target], report: &mut Report) {
    let handles = handles(tree);
    for target in targets.iter().filter(|target| target.control()) {
        report.measure(Rule::ResizeClear);
        if let Some(both) = handles.iter().find_map(|handle| overlap(target.visible, *handle)) {
            report.push(
                Rule::ResizeClear,
                target.label(),
                format!(
                    "the control's box reaches into a resize handle by {:.0} x {:.0} px at ({:.0}, {:.0})",
                    both.width(),
                    both.height(),
                    both.min_x(),
                    both.min_y()
                ),
                target.rect(),
            );
        }
    }
    let headers: Vec<Area> = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("HeaderBar"))
        .filter_map(border)
        .collect();
    for bar in scrollbars(tree) {
        let Some(track) = border(bar) else { continue };
        let label = describe_node(tree, bar);
        let name = if label.is_empty() { "scrollbar".to_owned() } else { format!("scrollbar in {label}") };
        report.measure(Rule::ScrollbarClear);
        if let Some(both) = handles.iter().find_map(|handle| overlap(track, *handle)) {
            report.push(
                Rule::ScrollbarClear,
                name.clone(),
                format!(
                    "the scrollbar's gutter reaches into a resize handle by {:.0} x {:.0} px at ({:.0}, {:.0}): grabbing the edge presses the bar",
                    both.width(),
                    both.height(),
                    both.min_x(),
                    both.min_y()
                ),
                [track.min_x(), track.min_y(), track.width(), track.height()],
            );
        }
        // The bar is the track's inner drawn part; the track itself starts
        // where the scroll view's inset puts it.
        if let Some(both) = headers.iter().find_map(|header| overlap(track, *header)) {
            report.push(
                Rule::ScrollbarClear,
                name,
                format!(
                    "the scrollbar runs {:.0} px under the header bar: it is cut off at the top",
                    both.height()
                ),
                [track.min_x(), track.min_y(), track.width(), track.height()],
            );
        }
    }
}
