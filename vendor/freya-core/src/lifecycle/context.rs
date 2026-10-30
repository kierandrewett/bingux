use std::{
    any::TypeId,
    rc::Rc,
};

use crate::{
    current_context::CurrentContext,
    prelude::use_hook,
    scope_id::ScopeId,
};

/// Every context a scope can see, copied out so another window can be given them.
///
/// Bingux patch. A popup is a window of its own, with a runner of its own, yet its content
/// reads the tokens, direction, router, overlays and the rest of the window it opened from.
/// [`snapshot_contexts`] collects them where the popup is opened;
/// [`provide_context_snapshot`] gives them to the popup's root.
#[derive(Clone, Default)]
pub struct ContextSnapshot(Vec<(TypeId, Rc<dyn std::any::Any>)>);

impl ContextSnapshot {
    /// Drops the context of type `T`: the popup brings its own.
    pub fn without<T: 'static>(mut self) -> Self {
        let type_id = TypeId::of::<T>();
        self.0.retain(|(kind, _)| *kind != type_id);
        self
    }
}

/// Copies every context visible from the current scope, nearest provider winning,
/// root contexts included.
pub fn snapshot_contexts() -> ContextSnapshot {
    CurrentContext::with(|context| {
        let scopes_storages = context.scopes_storages.borrow();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut found = Vec::new();
        let mut next = Some(context.scope_id);
        while let Some(scope_id) = next {
            let Some(storage) = scopes_storages.get(&scope_id) else {
                break;
            };
            for (type_id, value) in &storage.contexts {
                if seen.insert(*type_id) {
                    found.push((*type_id, value.clone()));
                }
            }
            next = storage.parent_id;
        }
        ContextSnapshot(found)
    })
}

/// Provides the contexts of `snapshot` to the current scope, except the types this window's
/// root already has: those belong to the window (its ticker, platform, font collection) and
/// the snapshot's copy came from another window.
pub fn provide_context_snapshot(snapshot: &ContextSnapshot) {
    CurrentContext::with(|context| {
        let mut scopes_storages = context.scopes_storages.borrow_mut();
        let own: rustc_hash::FxHashSet<TypeId> = scopes_storages
            .get(&ScopeId::ROOT)
            .map(|root| root.contexts.keys().copied().collect())
            .unwrap_or_default();
        let Some(target) = scopes_storages.get_mut(&context.scope_id) else {
            return;
        };
        for (type_id, value) in &snapshot.0 {
            if !own.contains(type_id) {
                target.contexts.insert(*type_id, value.clone());
            }
        }
    })
}

pub fn provide_context<T: Clone + 'static>(value: T) {
    provide_context_for_scope_id(value, None)
}

/// Store the given value in the root scope of the current window. If a value of the same
/// type already exists, it is replaced.
///
/// For state shared across all windows use [GlobalContexts](crate::prelude::GlobalContexts) instead.
pub fn provide_root_context<T: Clone + 'static>(value: T) -> T {
    provide_context_for_scope_id(value.clone(), Some(ScopeId::ROOT));
    value
}

pub fn provide_context_for_scope_id<T: Clone + 'static>(
    value: T,
    scope_id: impl Into<Option<ScopeId>>,
) {
    CurrentContext::with(|context| {
        let mut scopes_storages = context.scopes_storages.borrow_mut();
        let target = scope_id.into().unwrap_or(context.scope_id);
        let type_id = TypeId::of::<T>();
        let value: Rc<dyn std::any::Any> = Rc::new(value);
        let address = Rc::as_ptr(&value) as *const () as usize;
        let replaced = scopes_storages
            .get_mut(&target)
            .unwrap()
            .contexts
            .insert(type_id, value);
        // Bingux patch: a value given to another scope is remembered by its
        // provider, which takes it back when it unmounts (see
        // `ScopeStorage::provided_to`).
        if target != context.scope_id
            && let Some(provider) = scopes_storages.get_mut(&context.scope_id)
        {
            match provider
                .provided_to
                .iter_mut()
                .find(|(scope, kind, _)| *scope == target && *kind == type_id)
            {
                Some(entry) => entry.2 = address,
                None => provider.provided_to.push((target, type_id, address)),
            }
        }
        drop(scopes_storages);
        drop(replaced);
    })
}

/// Try to get a context value stored only in the current scope, without walking up to ancestors.
pub fn try_consume_own_context<T: Clone + 'static>() -> Option<T> {
    CurrentContext::with(|context| {
        let scopes_storages = context.scopes_storages.borrow();
        let scopes_storage = scopes_storages.get(&context.scope_id)?;
        let type_id = TypeId::of::<T>();
        scopes_storage
            .contexts
            .get(&type_id)?
            .downcast_ref::<T>()
            .cloned()
    })
}

pub fn try_consume_root_context<T: Clone + 'static>() -> Option<T> {
    try_consume_context_from_scope_id(Some(ScopeId::ROOT))
}

pub fn try_consume_context<T: Clone + 'static>() -> Option<T> {
    try_consume_context_from_scope_id(None)
}

#[track_caller]
pub fn consume_context<T: Clone + 'static>() -> T {
    match try_consume_context_from_scope_id(None) {
        Some(context) => context,
        None => panic!("Context <{}> was not found.", std::any::type_name::<T>()),
    }
}

#[track_caller]
pub fn consume_root_context<T: Clone + 'static>() -> T {
    match try_consume_root_context() {
        Some(context) => context,
        None => panic!(
            "Root context <{}> was not found.",
            std::any::type_name::<T>()
        ),
    }
}

pub fn try_consume_context_from_scope_id<T: Clone + 'static>(
    scope_id: Option<ScopeId>,
) -> Option<T> {
    CurrentContext::with(|context| {
        let scopes_storages = context.scopes_storages.borrow_mut();

        let mut ladder = vec![scope_id.unwrap_or(context.scope_id)];

        let type_id = TypeId::of::<T>();

        while let Some(scope_id) = ladder.pop() {
            let scopes_storage = scopes_storages.get(&scope_id)?;

            if let Some(context) = scopes_storage.contexts.get(&type_id) {
                return context.downcast_ref::<T>().cloned();
            } else if let Some(parent_scope_id) = scopes_storage.parent_id {
                ladder.push(parent_scope_id);
            }
        }

        None
    })
}

/// Store the given value in this component instance.
/// Any descendant component of this component calling [use_consume] or [consume_context] will have access to it.
#[track_caller]
pub fn use_provide_context<T: Clone + 'static>(init: impl FnOnce() -> T) -> T {
    use_hook(|| {
        let ctx = init();
        provide_context(ctx.clone());
        ctx
    })
}

/// Store the given value in the root scope of the current window. If a value of the same
/// type already exists, that one is returned and `init` is not called.
///
/// For state shared across all windows use [GlobalContexts](crate::prelude::GlobalContexts) instead.
#[track_caller]
pub fn use_provide_root_context<T: Clone + 'static>(init: impl FnOnce() -> T) -> T {
    use_hook(|| match try_consume_root_context::<T>() {
        Some(context) => context,
        None => provide_root_context(init()),
    })
}

/// Get access to a value stored in this component instance or some ancestor.
#[track_caller]
pub fn use_consume<T: Clone + 'static>() -> T {
    match use_hook(|| try_consume_context()) {
        Some(context) => context,
        None => panic!("Context <{}> was not found.", std::any::type_name::<T>()),
    }
}

/// Try to get access to a value stored in this component instance or some ancestor.
#[track_caller]
pub fn use_try_consume<T: Clone + 'static>() -> Option<T> {
    use_hook(|| try_consume_context())
}
