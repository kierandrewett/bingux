//! The Tab walk (CK-18): Tab through the whole window and back.
//!
//! Starting with nothing focused, Tab is pressed until focus comes round to
//! where it began. The stops are held to what FC-04, FC-05 and FC-07 say:
//!
//! - every interactive node is a stop, and none is a stop twice (one stop per
//!   control, `tab-reach`);
//! - the walk comes round: a control that swallows Tab, or focus that never
//!   returns, is a trap (`tab-trap`);
//! - the order is the reading order: down the page, start to end, never back
//!   up it (`tab-order`);
//! - Shift+Tab retraces the same stops in reverse (`tab-reach`);
//! - every stop shows its ring (`tab-ring`);
//! - in a window with a modal scope, no stop is outside it (`tab-trap`);
//! - Escape closes the popup (`popup-escape`).

use std::collections::{HashMap, HashSet};

use freya::prelude::{Key, Modifiers, NamedKey};
use freya_core::accessibility::id::AccessibilityId;
use freya_core::accessibility::tree::ACCESSIBILITY_ROOT_ID;

use super::rig::Rig;
use super::sweep::ring_check;
use super::view::{self, Target, describe_node, within};
use super::{Report, Rule};
use crate::inspector::snapshot::Snapshot as Tree;

/// More Tab presses than there are controls before the walk is called a trap.
const SLACK: usize = 6;

/// Whether the step from one stop to the next goes back up the page: the next
/// stop is above the last by more than half a control's height and in the same
/// column (it overlaps the last by at least half of the narrower one's width).
/// A step to another column or region (the sidebar to the content) is forward
/// whichever way it points.
fn backwards(from: &Target, to: &Target) -> bool {
    // Into a region from a control inside it, or out of it, is not a step
    // across the page: the region holds the control.
    let holds = |outer: &Target, inner: &Target| outer.face.inflate(1.0, 1.0).contains_rect(&inner.face);
    if !from.control() || !to.control() || holds(from, to) || holds(to, from) {
        return false;
    }
    let half = from.face.height().min(to.face.height()) * 0.5;
    let (a, b) = (from.face.center(), to.face.center());
    let shared = from.face.max_x().min(to.face.max_x()) - from.face.min_x().max(to.face.min_x());
    let narrow = from.face.width().min(to.face.width());
    b.y < a.y - half && shared >= narrow * 0.5
}

pub(super) fn walk(rig: &mut Rig, ids: &[u64], trialed: &HashSet<u64>, report: &mut Report) {
    rig.park();
    let tree = rig.tree();
    let all = view::focusables(&tree);
    if all.is_empty() {
        return;
    }
    let modal = tree.nodes.iter().rev().find(|node| node.a11y.modal);
    let limit = all.len() + SLACK;
    let lookup = |id: AccessibilityId| all.iter().find(|target| target.focus == id);
    let describe = |id: AccessibilityId, tree: &Tree| {
        tree.nodes
            .iter()
            .find(|node| node.a11y.id == Some(id))
            .map(|node| describe_node(tree, node))
            .unwrap_or_else(|| format!("accessibility id {}", id.0))
    };

    // Forward.
    let mut stops: Vec<AccessibilityId> = Vec::new();
    let mut closed = false;
    // Focus left the page for the window before the lap was done.
    let mut left_early = false;
    let mut per_kind: HashMap<String, usize> = HashMap::new();
    // Each step with both ends as laid out when Tab made it: a Tab scrolls the
    // page, so positions read before the walk are not the ones it moved over.
    let mut steps: Vec<(Target, Target)> = Vec::new();
    for _ in 0..limit {
        rig.named(NamedKey::Tab);
        // A modal scope hands focus on from its zero-size sentinel a frame or
        // two after the key: read it when it has.
        rig.settle(super::rig::SETTLE * 3);
        let now = rig.focused();
        if stops.last() == Some(&now) && all.len() > 1 {
            report.push(
                Rule::TabTrap,
                stops.last().and_then(|id| lookup(*id)).map_or_else(|| describe(now, &tree), Target::label),
                "Tab does not move focus off it: the control swallows the key".to_owned(),
                [0.0; 4],
            );
            break;
        }
        if stops.first() == Some(&now) || (stops.last() == Some(&now) && all.len() == 1) {
            closed = true;
            left_early = true;
            break;
        }
        if stops.contains(&now) {
            let again = lookup(now).map_or_else(|| describe(now, &tree), Target::label);
            report.push(
                Rule::TabReach,
                again,
                "Tab stops on it a second time before the walk has come round: it has two stops".to_owned(),
                lookup(now).map_or([0.0; 4], Target::rect),
            );
            break;
        }
        if now == ACCESSIBILITY_ROOT_ID {
            // Focus left every control: the walk comes round through the window.
            if std::env::var_os("BINGUX_UI_STATE_TRACE").is_some() {
                eprintln!("[state] tab: focus left to the window after {} stops of {}", stops.len(), all.len());
            }
            closed = true;
            break;
        }
        stops.push(now);
        if stops.len() >= 2 {
            let previous = stops[stops.len() - 2];
            let here = view::focusables(&rig.tree());
            if let (Some(from), Some(to)) = (
                here.iter().find(|target| target.focus == previous),
                here.iter().find(|target| target.focus == now),
            ) {
                steps.push((from.clone(), to.clone()));
            }
        }
        if std::env::var_os("BINGUX_UI_STATE_TRACE").is_some() {
            eprintln!(
                "[state] tab {} -> {}",
                stops.len(),
                lookup(now).map_or_else(|| describe(now, &tree), Target::label)
            );
        }
        // The ring at this stop, from the real key, for a few of each kind
        // that the sweep did not already try.
        if let Some(target) = lookup(now).filter(|target| target.visible.width() >= 1.0)
            && !trialed.contains(&now.0)
            && !rig.over_budget()
        {
            let count = per_kind.entry(format!("{} {}", target.role, target.component)).or_default();
            if *count < rig.per_kind {
                *count += 1;
                stop_ring(rig, target, report);
            }
        }
    }
    report.stops = stops.len();
    if !closed && stops.len() + 1 >= limit && stops.last() != Some(&ACCESSIBILITY_ROOT_ID) {
        report.push(
            Rule::TabTrap,
            "the window".to_owned(),
            format!(
                "{limit} Tab presses and focus has not come round: there are {} focusable nodes, so something takes focus more than once or never lets go",
                all.len()
            ),
            [0.0; 4],
        );
    }

    // What is a stop, and what is not.
    let focus_of: Vec<Option<&Target>> = stops.iter().map(|id| lookup(*id)).collect();
    for (id, target) in stops.iter().zip(&focus_of) {
        if target.is_some() {
            continue;
        }
        let what = describe(*id, &tree);
        let outside_modal = modal.is_some_and(|scope| {
            tree.nodes
                .iter()
                .find(|node| node.a11y.id == Some(*id))
                .is_none_or(|node| !within(&tree, node, scope))
        });
        if outside_modal {
            report.push(
                Rule::TabTrap,
                what,
                "focus left the popup: a modal scope holds every stop inside itself".to_owned(),
                [0.0; 4],
            );
        } else {
            report.push(
                Rule::TabReach,
                what,
                "Tab stops on something that is not a focusable node of the page (hidden, inert or new)".to_owned(),
                [0.0; 4],
            );
        }
    }
    let missed_count = all.iter().filter(|target| !stops.contains(&target.focus)).count();
    if (left_early || closed) && missed_count > 3 {
        // One finding for the lap, at the last stop: a page whose lap ends
        // part way round (focus goes out to the window, or round to the
        // start) has not many unreachable controls but one place where Tab
        // does not go on.
        let last = stops.last().and_then(|id| lookup(*id));
        let missed = all.iter().filter(|target| !stops.contains(&target.focus)).count();
        report.measure(Rule::TabReach);
        report.push(
            Rule::TabReach,
            last.map_or_else(|| "the window".to_owned(), Target::label),
            format!(
                "Tab ends its lap here, after {} stops ({}): {missed} of {} focusable nodes are not reached",
                stops.len(),
                if left_early { "the next press loses focus" } else { "the next press goes back to the first stop" },
                all.len()
            ),
            last.map_or([0.0; 4], Target::rect),
        );
    } else if closed && !left_early {
        for target in all.iter().filter(|target| !stops.contains(&target.focus)) {
            report.measure(Rule::TabReach);
            report.push(
                Rule::TabReach,
                target.label(),
                format!("Tab never stops on it ({} stops of {} focusable nodes)", stops.len(), all.len()),
                target.rect(),
            );
        }
    }
    for (from, to) in &steps {
        let same = (from.face.min_x() - to.face.min_x()).abs() < 0.5
            && (from.face.min_y() - to.face.min_y()).abs() < 0.5
            && (from.face.width() - to.face.width()).abs() < 0.5
            && (from.face.height() - to.face.height()).abs() < 0.5;
        let holds = |outer: &Target, inner: &Target| outer.face.inflate(8.0, 8.0).contains_rect(&inner.face);
        let same = same || (from.component == to.component && from.name == to.name && (holds(from, to) || holds(to, from)));
        if same {
            report.push(
                Rule::TabReach,
                to.label(),
                format!("Tab makes two stops on one box ({} then {}): a control has one stop", from.label(), to.label()),
                to.rect(),
            );
        } else if backwards(from, to) {
            report.measure(Rule::TabOrder);
            report.push(
                Rule::TabOrder,
                to.label(),
                format!(
                    "Tab goes to it from {} which is {:.0} px below it: back up the page",
                    from.label(),
                    from.face.center().y - to.face.center().y
                ),
                to.rect(),
            );
        }
    }

    // Backward: Shift+Tab from the first stop retraces the lap in reverse.
    if closed && stops.len() > 1 {
        rig.park();
        rig.focus(stops[0]);
        let mut back: Vec<AccessibilityId> = Vec::new();
        for _ in 1..stops.len() {
            rig.key_with(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
            back.push(rig.focused());
        }
        let want: Vec<AccessibilityId> = stops.iter().skip(1).rev().copied().collect();
        report.measure(Rule::TabReach);
        if back != want {
            let at = back.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0);
            report.push(
                Rule::TabReach,
                want.get(at).map_or_else(|| "the window".to_owned(), |id| describe(*id, &tree)),
                format!(
                    "Shift+Tab does not retrace the Tab walk: step {} of the way back should be on this control",
                    at + 1
                ),
                [0.0; 4],
            );
        }
    }
    rig.park();
    rig.restore(ids, 3);
}

/// The ring at a Tab stop: the frame with the control focused against the
/// same frame with focus dropped, so a page the Tab scrolled is compared with
/// itself. Focus is put back for the walk to go on.
fn stop_ring(rig: &mut Rig, target: &Target, report: &mut Report) {
    let on = rig.frame();
    let tree = rig.tree();
    let off = rig.rest_for(target.focus);
    rig.focus(target.focus);
    ring_check(rig, &off, &on, &tree, target, Rule::TabRing, report);
}

/// Escape closes a popup or dialog: with a modal scope open, focus into it and
/// press Escape; the scope must go.
pub(super) fn escape(rig: &mut Rig, report: &mut Report) {
    let tree = rig.tree();
    let Some(modal) = tree.nodes.iter().rev().find(|node| node.a11y.modal) else { return };
    let what = describe_node(&tree, modal);
    rig.park();
    rig.named(NamedKey::Tab);
    // A text field takes the first Escape to give up its focus and revert its
    // text (controls/text_field.rs); the popup closes on the next. Anything
    // else closes on the first.
    let focused = rig.focused();
    let in_field = tree
        .nodes
        .iter()
        .find(|node| node.a11y.id == Some(focused))
        .is_some_and(|node| node.a11y.role.contains("Input"));
    rig.named(NamedKey::Escape);
    rig.settle(super::rig::SETTLE * 4);
    if in_field && rig.tree().nodes.iter().any(|node| node.a11y.modal) {
        rig.named(NamedKey::Escape);
        rig.settle(super::rig::SETTLE * 4);
    }
    let after = rig.tree();
    report.measure(Rule::PopupEscape);
    if after.nodes.iter().any(|node| node.a11y.modal) {
        report.push(
            Rule::PopupEscape,
            what,
            if in_field {
                "two Escapes (one to leave the focused field) do not close it".to_owned()
            } else {
                "Escape does not close it".to_owned()
            },
            [0.0; 4],
        );
    }
}
