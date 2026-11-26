use std::ops::Deref;

use rustc_hash::FxHashMap;

use crate::{
    data::LayerState,
    node_id::NodeId,
};

/// The painting layer of an element, controlling what it draws on top of.
///
/// Converts from an `i16`, which becomes a [`Layer::Relative`] offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layer {
    /// Relative layer to the parent's layer. `0` (the default) keeps the normal stacking order.
    /// Negative values move behind siblings and positive values in front.
    Relative(i16),
    /// Adds a big layer jump relative to the parents layer.
    /// You may stack multiple overlays on top of each other.
    Overlay,
    /// Paint on a specific, numbered overlay level, regardless of the parent's layer.
    /// There are up to 16 levels you can use, anything above will be capped at 16.
    OverlayLevel(u8),
}

impl Default for Layer {
    fn default() -> Self {
        Layer::Relative(0)
    }
}

impl From<i16> for Layer {
    fn from(value: i16) -> Self {
        Layer::Relative(value)
    }
}

/// The nodes of each layer, in tree order (the order of a depth-first walk from the root).
///
/// Bingux patch: within one layer a node is painted after, and hit before, every node that comes
/// before it in the tree, as in a document. Upstream keeps each layer in a hash set and paints
/// it in the set's iteration order, so which of two overlapping nodes on one layer showed
/// depended on their node ids (`bingux-ui`'s `paint_order_tests`).
///
/// The lists are derived from the tree. A change of the tree's shape or of a node's layer marks
/// them stale ([`Layers::mark_stale`]), and [`Layers::rebuild`] walks the tree once when the
/// mutations are applied, reusing each list's storage; a frame with no such change does nothing.
#[derive(Default, Clone, Debug)]
pub struct Layers {
    layers: FxHashMap<i16, Vec<NodeId>>,
    stale: bool,
    /// The walk's stack, kept between rebuilds.
    walk: Vec<NodeId>,
}

impl Layers {
    /// The tree's shape or a node's layer changed: rebuild the lists before they are read.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Put every node of the tree in its layer's list, in tree order, when the lists are stale.
    pub(crate) fn rebuild(
        &mut self,
        children: &FxHashMap<NodeId, Vec<NodeId>>,
        layer_state: &FxHashMap<NodeId, LayerState>,
    ) {
        if !self.stale {
            return;
        }
        self.stale = false;
        for nodes in self.layers.values_mut() {
            nodes.clear();
        }
        self.walk.push(NodeId::ROOT);
        while let Some(node_id) = self.walk.pop() {
            if let Some(state) = layer_state.get(&node_id) {
                self.layers.entry(state.layer).or_default().push(node_id);
            }
            if let Some(children) = children.get(&node_id) {
                self.walk.extend(children.iter().rev());
            }
        }
        self.layers.retain(|_, nodes| !nodes.is_empty());
    }
}

impl Deref for Layers {
    type Target = FxHashMap<i16, Vec<NodeId>>;

    fn deref(&self) -> &Self::Target {
        &self.layers
    }
}
