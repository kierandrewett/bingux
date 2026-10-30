use rustc_hash::{
    FxHashMap,
    FxHashSet,
};

use crate::{
    node_id::NodeId,
    prelude::AccessibilityFocusStrategy,
};

#[derive(Default)]
pub struct AccessibilityDirtyNodes {
    pub requested_focus: Option<AccessibilityFocusStrategy>,
    /// A focus on a node that a later relative request (Tab, Shift+Tab) moves on from. The
    /// last request still wins, but an absolute focus it would have replaced is applied first,
    /// so two moves requested before the next accessibility update land one after the other.
    pub anchor_focus: Option<AccessibilityFocusStrategy>,
    pub requested_auto_focus: Option<AccessibilityFocusStrategy>,
    pub added_or_updated: FxHashSet<NodeId>,
    pub removed: FxHashMap<NodeId, NodeId>,
}

impl AccessibilityDirtyNodes {
    pub fn request_focus(&mut self, strategy: AccessibilityFocusStrategy) {
        let relative = matches!(
            strategy,
            AccessibilityFocusStrategy::Forward(_) | AccessibilityFocusStrategy::Backward(_)
        );
        match (&self.requested_focus, relative) {
            (Some(AccessibilityFocusStrategy::Node(id)), true) => {
                self.anchor_focus = Some(AccessibilityFocusStrategy::Node(*id));
            }
            (_, false) => self.anchor_focus = None,
            _ => {}
        }
        self.requested_focus = Some(strategy);
    }

    pub fn request_auto_focus(&mut self, strategy: AccessibilityFocusStrategy) {
        self.requested_auto_focus = Some(strategy);
    }

    pub fn add_or_update(&mut self, node_id: NodeId) {
        self.added_or_updated.insert(node_id);
    }

    pub fn remove(&mut self, node_id: NodeId, parent_id: NodeId) {
        self.removed.insert(node_id, parent_id);
    }

    pub fn clear(&mut self) {
        self.requested_focus.take();
        self.anchor_focus.take();
        self.requested_auto_focus.take();
        self.added_or_updated.clear();
        self.removed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{
        AccessibilityFocusMovement,
        AccessibilityId,
    };

    fn forward() -> AccessibilityFocusStrategy {
        AccessibilityFocusStrategy::Forward(AccessibilityFocusMovement::OutsideGroup)
    }

    #[test]
    fn a_relative_move_keeps_the_absolute_focus_it_follows() {
        let mut dirty = AccessibilityDirtyNodes::default();
        dirty.request_focus(AccessibilityFocusStrategy::Node(AccessibilityId(7)));
        dirty.request_focus(forward());
        assert!(matches!(
            dirty.anchor_focus,
            Some(AccessibilityFocusStrategy::Node(AccessibilityId(7)))
        ));
        assert!(matches!(dirty.requested_focus, Some(AccessibilityFocusStrategy::Forward(_))));
    }

    #[test]
    fn a_later_absolute_focus_replaces_both() {
        let mut dirty = AccessibilityDirtyNodes::default();
        dirty.request_focus(AccessibilityFocusStrategy::Node(AccessibilityId(7)));
        dirty.request_focus(forward());
        dirty.request_focus(AccessibilityFocusStrategy::Node(AccessibilityId(9)));
        assert!(dirty.anchor_focus.is_none());
        assert!(matches!(
            dirty.requested_focus,
            Some(AccessibilityFocusStrategy::Node(AccessibilityId(9)))
        ));
    }

    #[test]
    fn two_relative_moves_in_a_row_are_still_one_move() {
        let mut dirty = AccessibilityDirtyNodes::default();
        dirty.request_focus(forward());
        dirty.request_focus(forward());
        assert!(dirty.anchor_focus.is_none());
    }
}
