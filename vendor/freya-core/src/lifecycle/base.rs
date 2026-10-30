use std::rc::Rc;

use crate::{
    current_context::CurrentContext,
    runner::Message,
    scope_id::ScopeId,
};

/// The short rules, appended to a hook-order violation.
static HOOKS_RULES: &str = "Hooks must run in the same order on every render: no conditions, no loops, \
and not inside event handlers, effects or other hooks.";

static HOOKS_ERROR: &str = "
Hook functions must follow these rules:
1. You cannot call them conditionally

The following is not allowed and will result in this runtime error.

#[derive(PartialEq)]
struct CoolComp(u8);

impl Component for CoolComp {
    fn render(&self) -> impl IntoElement {
        if self.0 == 2 {
            let state = use_state(|| 5);
        }

        rect().into()
    }
}

2. You cannot call them in for-loops

The following is not allowed and will result in this runtime error.

#[derive(PartialEq)]
struct CoolComp(u8);

impl Component for CoolComp {
    fn render(&self) -> impl IntoElement {
        for i in 0..self.0 {
            let state = use_state(|| 5);
        }

        rect().into()
    }
}

3. You cannot call hooks inside other hooks, event handlers, they should be called in the top of `render` methods from components.

The following is not allowed and will result in this runtime error.

#[derive(PartialEq)]
struct CoolComp(u8);

impl Component for CoolComp {
    fn render(&self) -> impl IntoElement {
        use_side_effect(|| {
            let state = use_state(|| 5);
        })

        rect().into()
    }
}
";

/// This is the foundational hook used by all. It's simple, it accepts an initialization callback
/// whose return value will be stored in this component instance until its dropped.
/// In subsequent renders the returned value of the function will be a [Clone]d value of what
/// had been previously returned by the initialization callback.
///
/// ```rust, no_run
/// # use freya::prelude::*;
/// let my_cloned_value = use_hook(|| 1);
/// ```
///
/// In order to maintain the concept of "instance", other hooks use [State::create](crate::prelude::State::create) to store their values.
/// One could also use `Rc<RefCell<T>>` but then you would not be able to make your hook values `Copy`.
///
/// Bingux patch: a hook-order violation panics with the component, the hook's index and type and
/// the place that called it, and the panic is caught by [`crate::guard`] at the component.
#[track_caller]
pub fn use_hook<T: Clone + 'static>(init: impl FnOnce() -> T) -> T {
    let caller = std::panic::Location::caller();
    if let Some(value) = CurrentContext::with(|context| {
        let mut scopes_storages = context.scopes_storages.borrow_mut();
        let scopes_storage = scopes_storages
            .get_mut(&context.scope_id)
            .expect(HOOKS_ERROR);
        if let Some(value) = scopes_storage
            .values
            .get(scopes_storage.current_value)
            .cloned()
        {
            let index = scopes_storage.current_value;
            match value.downcast_ref::<T>().cloned() {
                Some(value) => {
                    scopes_storage.current_value += 1;
                    Some(value)
                }
                // The index stays where it was: a hook called from an event handler
                // (which has no place in the order) fails without moving the
                // component's next render off its hooks.
                None => panic!(
                    "hook #{index} of component `{}` ({caller}) asked for a `{}`, but the hook \
                     at that position in the previous render stored a different type: the \
                     component calls its hooks in a different order than before.\n{HOOKS_RULES}",
                    scopes_storage.name,
                    std::any::type_name::<T>(),
                ),
            }
        } else if scopes_storage.current_run > 0 {
            panic!(
                "hook #{} of component `{}` ({caller}, a `{}`) has no match in the previous \
                 render, which called only {} hooks: the component calls more hooks than \
                 before.\n{HOOKS_RULES}",
                scopes_storage.current_value,
                scopes_storage.name,
                std::any::type_name::<T>(),
                scopes_storage.values.len(),
            )
        } else {
            None
        }
    }) {
        value
    } else {
        let value = init();
        CurrentContext::with(|context| {
            let mut scopes_storages = context.scopes_storages.borrow_mut();
            let scopes_storage = scopes_storages
                .get_mut(&context.scope_id)
                .expect(HOOKS_ERROR);
            scopes_storage.values.push(Rc::new(value.clone()));
            scopes_storage.current_value += 1;
            value
        })
    }
}

struct DropInner(Option<Box<dyn FnOnce()>>);

impl std::ops::Drop for DropInner {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            // Bingux patch: a panicking cleanup is caught, as it may itself run
            // while another panic unwinds, which would abort.
            let _ = crate::guard::run(crate::guard::FailureKind::Drop, f);
        }
    }
}

/// Run a callback for when the component gets dropped. Useful to clean resources.
/// ```rust, no_run
/// # use freya::prelude::*;
/// use_drop(|| {
///     println!("Dropping this component.");
/// });
/// ```
#[track_caller]
pub fn use_drop(drop: impl FnOnce() + 'static) {
    use_hook(|| Rc::new(DropInner(Some(Box::new(drop)))));
}

pub fn current_scope_id() -> ScopeId {
    CurrentContext::with(|context| context.scope_id)
}

pub fn mark_scope_as_dirty(scope_id: ScopeId) {
    CurrentContext::with(|context| {
        context
            .sender
            .unbounded_send(Message::MarkScopeAsDirty(scope_id))
            .unwrap();
    })
}
