//! Live overrides: the inspector edits an element's layout and style while the
//! app runs. Each edit is a closure keyed by window, node and property name,
//! remembered with the value it replaced. Before every layout pass the plugin
//! re-applies the enabled edits to the freshly diffed elements, so an override
//! survives re-renders until it is reset. Disabling an override restores the
//! element the application produced.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::rc::Rc;

use freya::prelude::{Label, Rect};
use freya::winit::window::WindowId;
use freya_core::element::ElementExt;
use freya_core::elements::label::LabelElement;
use freya_core::elements::rect::RectElement;
use freya_core::node_id::NodeId;
use freya_core::runner::{MutationModified, Mutations};
use freya_core::tree::{DiffModifies, Tree};

/// Edits one element. The closure receives a `RectElement` or a `LabelElement`
/// as `dyn Any` and ignores the kinds it does not handle.
pub type Edit = Rc<dyn Fn(&mut dyn Any)>;

/// One override: the edit, the value text it sets, the value text it replaced
/// and whether it is on.
#[derive(Clone)]
pub struct Entry {
    pub edit: Edit,
    /// The value the override sets, as the editor shows it.
    pub value: String,
    /// The value shown before the first override, as the editor reads it.
    pub original: String,
    pub enabled: bool,
}

/// The overrides in force, keyed by `(window, node id, property)`.
#[derive(Default)]
pub struct Overrides {
    edits: HashMap<(WindowId, u64, &'static str), Entry>,
    /// For each node we modified: the element the application produced and
    /// the copy we handed to the tree. The copy is recognised next pass by
    /// pointer, which tells a re-render (fresh element) from a retained one.
    applied: RefCell<HashMap<(WindowId, u64), Applied>>,
}

struct Applied {
    modified: Rc<dyn ElementExt>,
    pristine: Rc<dyn ElementExt>,
}

impl Overrides {
    /// Sets the edit for one property of one node. The first `original` text
    /// is kept, so the editor can show what the application produced.
    pub fn set(
        &mut self,
        window: WindowId,
        node: u64,
        property: &'static str,
        value: String,
        edit: Edit,
        original: String,
    ) {
        let original = self
            .edits
            .get(&(window, node, property))
            .map_or(original, |entry| entry.original.clone());
        self.edits.insert(
            (window, node, property),
            Entry {
                edit,
                value,
                original,
                enabled: true,
            },
        );
    }

    /// Drops the edit for one property, or every edit on the node when
    /// `property` is `None`.
    pub fn reset(&mut self, window: WindowId, node: u64, property: Option<&'static str>) {
        self.edits.retain(|(w, n, p), _| {
            !(*w == window && *n == node && property.is_none_or(|prop| prop == *p))
        });
    }

    /// Turns one override on or off, keeping its edit. Returns whether it is
    /// now on.
    pub fn toggle(&mut self, window: WindowId, node: u64, property: &'static str) -> bool {
        match self.edits.get_mut(&(window, node, property)) {
            Some(entry) => {
                entry.enabled = !entry.enabled;
                entry.enabled
            }
            None => false,
        }
    }

    /// Whether this property of this node is overridden and the override is on.
    pub fn has(&self, window: WindowId, node: u64, property: &'static str) -> bool {
        self.edits
            .get(&(window, node, property))
            .is_some_and(|entry| entry.enabled)
    }

    /// Every override on the node, sorted by property name.
    pub fn list(&self, window: WindowId, node: u64) -> Vec<(&'static str, Entry)> {
        let mut list: Vec<(&'static str, Entry)> = self
            .edits
            .iter()
            .filter(|((w, n, _), _)| *w == window && *n == node)
            .map(|((_, _, property), entry)| (*property, entry.clone()))
            .collect();
        list.sort_by_key(|(property, _)| *property);
        list
    }

    /// Applies every enabled edit for `window` to `tree`. Runs before measure,
    /// so the layout and paint of this pass use the edited values.
    pub fn apply(&self, window: WindowId, tree: &mut Tree, scale_factor: f32) {
        let mut nodes: Vec<u64> = self
            .edits
            .keys()
            .filter(|(w, _, _)| *w == window)
            .map(|(_, node, _)| *node)
            .collect();
        nodes.extend(
            self.applied
                .borrow()
                .keys()
                .filter(|(w, _)| *w == window)
                .map(|(_, node)| *node),
        );
        nodes.sort_unstable();
        nodes.dedup();

        let mut modified = Vec::new();
        for node in nodes {
            let Some(node_id) = NonZeroU64::new(node).map(NodeId) else {
                continue;
            };
            let Some(element) = tree.elements.get(&node_id) else {
                continue;
            };
            let edits: Vec<Edit> = self
                .edits
                .iter()
                .filter(|((w, n, _), entry)| *w == window && *n == node && entry.enabled)
                .map(|(_, entry)| entry.edit.clone())
                .collect();
            let mut applied = self.applied.borrow_mut();
            let key = (window, node);
            // Our own copy in the tree means the element was not re-rendered,
            // so its pristine original is the one we stored.
            let pristine = match applied.get(&key) {
                Some(state) if Rc::ptr_eq(&state.modified, element) => state.pristine.clone(),
                _ => element.clone(),
            };
            if edits.is_empty() {
                if applied.remove(&key).is_some() {
                    modified.push(MutationModified {
                        node_id,
                        element: pristine,
                        flags: DiffModifies::all(),
                    });
                }
                continue;
            }
            if let Some(copy) = edit_element(&pristine, &edits) {
                applied.insert(
                    key,
                    Applied {
                        modified: copy.clone(),
                        pristine,
                    },
                );
                modified.push(MutationModified {
                    node_id,
                    element: copy,
                    flags: DiffModifies::all(),
                });
            }
        }
        if modified.is_empty() {
            return;
        }
        tree.apply_mutations(
            Mutations {
                added: Vec::new(),
                modified,
                removed: Vec::new(),
                moved: Default::default(),
            },
            scale_factor,
        );
    }
}

/// Copies the element, applies the edits to the copy and returns it. `None`
/// for element kinds the inspector cannot edit.
fn edit_element(element: &Rc<dyn ElementExt>, edits: &[Edit]) -> Option<Rc<dyn ElementExt>> {
    if let Some(mut rect) = Rect::try_downcast(&**element) {
        for edit in edits {
            edit(&mut rect);
        }
        return Some(Rc::new(rect));
    }
    if let Some(mut label) = Label::try_downcast(&**element) {
        for edit in edits {
            edit(&mut label);
        }
        return Some(Rc::new(label));
    }
    None
}

/// Runs `f` on the rect element behind `element`, if it is one.
pub fn with_rect(element: &mut dyn Any, f: impl FnOnce(&mut RectElement)) {
    if let Some(rect) = element.downcast_mut::<RectElement>() {
        f(rect);
    }
}

/// Runs `f` on the label element behind `element`, if it is one.
pub fn with_label(element: &mut dyn Any, f: impl FnOnce(&mut LabelElement)) {
    if let Some(label) = element.downcast_mut::<LabelElement>() {
        f(label);
    }
}
