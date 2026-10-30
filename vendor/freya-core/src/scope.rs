use std::{
    any::{
        Any,
        TypeId,
    },
    rc::Rc,
};

use generational_box::{
    AnyStorage,
    GenerationalBox,
    Owner,
    UnsyncStorage,
};
use pathgraph::PathGraph;
use rustc_hash::FxHashMap;

use crate::{
    diff_key::DiffKey,
    element::{
        ComponentProps,
        Element,
    },
    node_id::NodeId,
    path_element::PathElement,
    reactive_context::ReactiveContext,
    scope_id::ScopeId,
};

pub struct Scope {
    pub id: ScopeId,
    pub parent_id: Option<ScopeId>,
    pub height: usize,

    pub parent_node_id_in_parent: NodeId,
    pub path_in_parent: Box<[u32]>, // TODO: Maybe just store the index rather than the whole path?

    pub nodes: PathGraph<PathNode>,

    pub key: DiffKey,

    pub comp: Rc<dyn Fn(Rc<dyn ComponentProps>) -> Element>,

    pub props: Rc<dyn ComponentProps>,

    pub element: Option<PathElement>,
}

impl Scope {
    pub fn with_element<D>(&self, target_path: &[u32], with: D)
    where
        D: FnOnce(&PathElement),
    {
        if let Some(element) = &self.element {
            element.with_element(target_path, with);
        }
    }
}

#[derive(Clone)]
pub struct ScopeStorage {
    pub parent_id: Option<ScopeId>,
    pub current_run: usize,
    pub current_value: usize,
    pub values: Vec<Rc<dyn Any>>,

    /// Bingux patch: the type name of the component this scope runs, for the
    /// component path of a failure. Set on every render.
    pub name: &'static str,

    pub contexts: FxHashMap<TypeId, Rc<dyn Any>>,

    /// Bingux patch: contexts this scope provided to another scope (the root:
    /// `provide_root_context`), as (scope, type, address of the value). They are
    /// removed when this scope unmounts, so the root never names state that was
    /// dropped with its provider. A window root that failed and was replaced by an
    /// error card would otherwise leave the tokens it provided pointing at dropped
    /// state, and a Retry would reuse them.
    pub provided_to: Vec<(ScopeId, TypeId, usize)>,

    pub reactive_context: ReactiveContext,

    pub owner: Owner,
}

impl ScopeStorage {
    pub(crate) fn new<T: 'static>(
        parent_id: Option<ScopeId>,
        reactive_context: impl FnOnce(&dyn Fn(T) -> GenerationalBox<T>) -> ReactiveContext,
    ) -> Self {
        let owner = UnsyncStorage::owner();
        let reactive_context = (reactive_context)(&|d: T| owner.insert(d));
        Self {
            parent_id,
            current_run: Default::default(),
            current_value: Default::default(),
            values: Default::default(),
            name: "",
            contexts: Default::default(),
            provided_to: Vec::new(),
            reactive_context,
            owner,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.current_run = 0;
        self.current_value = 0;
        self.values.clear();
        self.contexts.clear();
    }

    /// Resets only the hooks state, preserving contexts.
    #[must_use]
    pub(crate) fn reset_hooks(&mut self) -> Vec<Rc<dyn Any>> {
        self.current_run = 0;
        self.current_value = 0;
        std::mem::take(&mut self.values)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathNode {
    pub node_id: NodeId,
    pub scope_id: Option<ScopeId>,
}
