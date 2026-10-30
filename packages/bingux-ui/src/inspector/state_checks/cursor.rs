//! The cursor map (CK-25): the cursor the window asks for with the pointer on
//! the middle of every interactive node.
//!
//! Cursors are quiet (ST-03): the arrow over rows, buttons, switches and the
//! controls inside a field, the I-beam only over a field's own text, a grab
//! hand only over something that can be dragged, and a resize arrow only over
//! a resize handle. A pointer hand anywhere, or an I-beam over a button that
//! sits in a text field, is the finding.

use freya::prelude::CursorIcon;

use super::rig::Rig;
use super::view::Target;
use super::{Report, Rule};

/// What cursor a control may ask for.
fn allowed(target: &Target, cursor: CursorIcon) -> bool {
    let editable = target.role.contains("Input") || target.role.contains("TextField");
    match cursor {
        CursorIcon::Default => true,
        // Selectable text shows the I-beam too (ST-10).
        CursorIcon::Text => editable || target.role == "Label",
        // A draggable list or tab strip says so with a grab hand.
        CursorIcon::Grab | CursorIcon::Grabbing => target.path.contains("Sortable") || target.path.contains("Tab"),
        // A splitter or a column edge is a resize handle: its own node.
        CursorIcon::ColResize | CursorIcon::RowResize => target.role.is_empty() || target.path.contains("Column"),
        _ => false,
    }
}

pub(super) fn run(rig: &mut Rig, targets: &[Target], report: &mut Report) {
    for target in targets.iter().filter(|target| target.control()) {
        let cursor = rig.cursor_at(target.centre());
        report.cursors += 1;
        report.measure(Rule::Cursor);
        if !allowed(target, cursor) {
            let resize = matches!(
                cursor,
                CursorIcon::NsResize
                    | CursorIcon::EwResize
                    | CursorIcon::NwseResize
                    | CursorIcon::NeswResize
            );
            let rule = if resize { Rule::ResizeClear } else { Rule::Cursor };
            let want = if matches!(cursor, CursorIcon::Text) { "the arrow (a control is not text)" } else { "the arrow" };
            report.push(
                rule,
                target.label(),
                format!("the cursor over its middle is {cursor:?}, expected {want}"),
                target.rect(),
            );
        }
    }
    rig.park();
}
