//! The router: the [`NavModel`] in a Freya state, plus what the rest of the
//! SDK needs to reach it.
//!
//! [`Application::run`](crate::app::Application::run) provides one per window
//! from the app's [`RouteTable`]. Pages use [`use_router`] (push, back, up) and
//! the hooks below; [`NavigationSplitView`](crate::shell::NavigationSplitView)
//! draws the stack. Anything outside a component (a row's closure, the
//! single-instance hand-off, a search action) calls [`navigate`] or
//! [`router`].

use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

use freya::prelude::*;

use super::location::Location;
use super::model::{
    Change, Confirmation, Entry, Guard, HistoryItem, ModalKind, NavModel, Op, Outcome, PopGuard,
    Snapshot,
};
use super::table::{RouteTable, Transition};
use crate::icons::Icon;
use crate::shell::dialog::{CANCEL_ID, ResponseKind, dialog};
use crate::shell::overlays::Overlays;
use crate::shell::status_page::status_page;

/// The last change, for views that animate it. `seq` moves on every change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Journal {
    pub seq: u64,
    pub change: Option<Change>,
}

type Retained = HashMap<(u64, &'static str), Box<dyn Any>>;

/// Where a row leads: a location, and the title the row already knows (a
/// network's name, a device's name) so the header shows it while the page
/// arrives. Build one with `Target::from(location)` and
/// [`Target::titled`]; open it with [`Router::open`].
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub location: Location,
    pub title: Option<String>,
}

impl Target {
    /// The same target with the title the row knows.
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl From<Location> for Target {
    fn from(location: Location) -> Self {
        Self {
            location,
            title: None,
        }
    }
}

/// A handle to a window's navigation. Copy it into closures.
#[derive(Clone, Copy)]
pub struct Router {
    model: State<NavModel>,
    journal: State<Journal>,
    dismissers: State<HashMap<u64, EventHandler<()>>>,
    overlays: State<Option<Overlays>>,
    retained: State<Retained>,
    scope: ScopeId,
}

impl PartialEq for Router {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
    }
}

thread_local! {
    /// The window's router, for code with no component scope.
    static WINDOW_ROUTER: Cell<Option<Router>> = const { Cell::new(None) };
}

/// Provide the window's router, starting at `initial`, to everything below the
/// window root. [`Application::run`](crate::app::Application::run) calls this;
/// a snapshot calls it to render a page at a location.
pub fn provide_router(table: Arc<RouteTable>, initial: &Location) -> Router {
    let scope = current_scope_id();
    let router = use_provide_root_context(move || Router {
        model: State::create(NavModel::new(table, initial)),
        journal: State::create(Journal::default()),
        dismissers: State::create(HashMap::new()),
        overlays: State::create(None),
        retained: State::create(HashMap::new()),
        scope,
    });
    WINDOW_ROUTER.with(|slot| slot.set(Some(router)));
    // The inspector reads this window's stack from its Properties pane; the
    // reader goes away with the window, so it never reads a dropped state.
    let window = use_hook(move || {
        let window = Platform::window_id();
        crate::inspector::register_navigation(window, std::rc::Rc::new(move || router.snapshot()));
        window
    });
    use_drop(move || crate::inspector::unregister_navigation(window));
    router
}

/// A router for the components below the caller only, starting at `initial`.
/// A specimen that shows a navigation split view inside a larger window (the
/// taxonomy, a gallery) uses this, so each specimen has a stack of its own and
/// none is the window's. [`navigate`] never reaches it.
pub fn use_local_router(table: Arc<RouteTable>, initial: &Location) -> Router {
    let scope = current_scope_id();
    use_provide_context(move || Router {
        model: State::create(NavModel::new(table, initial)),
        journal: State::create(Journal::default()),
        dismissers: State::create(HashMap::new()),
        overlays: State::create(None),
        retained: State::create(HashMap::new()),
        scope,
    })
}

/// The window's router.
pub fn use_router() -> Router {
    use_consume::<Router>()
}

/// The window's router from anywhere (an event handler, another thread's
/// callback on the window's thread). `None` before the window opened.
pub fn try_router() -> Option<Router> {
    WINDOW_ROUTER.with(Cell::get)
}

/// The window's router, for code that only runs once a window exists.
pub fn router() -> Router {
    try_router().expect("navigation used before the window opened")
}

/// Open `uri` in the window: a full URI (`bingux-settings://network/proxy#m`)
/// or a bare path of the app's scheme (`network/proxy`). An unparsable
/// string is ignored. This is what a deep link, a search action and a second
/// launch call.
pub fn navigate(uri: &str) {
    if let Some(router) = try_router() {
        router.navigate(uri);
    }
}

/// The location the window shows. Reactive: the component redraws when it moves.
pub fn use_location() -> Location {
    let router = use_router();
    let location = router.model.read().location().clone();
    location
}

/// The fragment of the window's location, or `None` without a router.
/// Reactive.
pub(crate) fn use_fragment() -> Option<String> {
    let router = use_try_consume::<Router>()?;
    let fragment = router.model.read().location().fragment().map(str::to_string);
    fragment
}

impl Router {
    pub fn table(&self) -> Arc<RouteTable> {
        self.model.peek().table().clone()
    }

    /// The location now, without subscribing.
    pub fn location(&self) -> Location {
        self.model.peek().location().clone()
    }

    /// The id of the current entry, without subscribing.
    pub fn current_id(&self) -> u64 {
        self.model.peek().current().id
    }

    /// The id of the current entry. Subscribes: the component redraws when
    /// another entry becomes current.
    pub fn use_current_id(&self) -> u64 {
        self.model.read().current().id
    }

    /// The whole model, without subscribing.
    pub fn model(&self) -> NavModel {
        self.model.peek().clone()
    }

    /// The model, subscribing the calling component to every change.
    pub fn read(&self) -> NavModel {
        self.model.read().clone()
    }

    /// Whether a menu, choice, dialog or sheet is open in this window. Subscribes the
    /// calling component, so it redraws when one opens or closes.
    pub fn modal_open(&self) -> bool {
        !self.model.read().modals().is_empty()
    }

    pub fn journal(&self) -> Journal {
        self.journal.read().clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.model.peek().snapshot()
    }

    /// Tell the router whether the window shows the sidebar beside the page.
    pub fn set_wide(&self, wide: bool) {
        if self.model.peek().is_wide() != wide {
            let mut model = self.model;
            model.write().set_wide(wide);
        }
    }

    /// The window's overlay handle, once the overlay host has registered it. The
    /// resilience layer toasts a worker thread's failure through it.
    pub(crate) fn overlays_handle(&self) -> Option<Overlays> {
        *self.overlays.peek()
    }

    /// The overlay host registers its handle so a vetoed pop can ask.
    pub(crate) fn attach_overlays(&self, overlays: Overlays) {
        let mut slot = self.overlays;
        if slot.peek().as_ref() != Some(&overlays) {
            slot.set(Some(overlays));
        }
    }

    // ---- operations -------------------------------------------------------

    /// Push `to`: what a row that opens a subpage does.
    pub fn push(&self, to: &Location) {
        self.run(
            Op::Push {
                to: to.clone(),
                title: None,
            },
            false,
        );
    }

    /// Open `target`: push its location, with the title the row that opens it
    /// already knows, if it has one.
    pub fn open(&self, target: impl Into<Target>) {
        let target = target.into();
        match target.title {
            Some(title) => self.push_titled(&target.location, &title),
            None => self.push(&target.location),
        }
    }

    /// Push `to`, with the title the caller already knows.
    pub fn push_titled(&self, to: &Location, title: &str) {
        self.run(
            Op::Push {
                to: to.clone(),
                title: Some(title.to_string()),
            },
            false,
        );
    }

    /// Back: the top modal if any, else a pop. Escape, Alt+Left, the mouse
    /// Back button, the Back button and the edge swipe all call this.
    pub fn back(&self) {
        self.run(Op::Back, false);
    }

    /// Leave the current page for the one below it.
    pub fn pop(&self) {
        self.run(Op::Pop, false);
    }

    /// Leave every page above `entry` in one move.
    pub fn pop_to(&self, entry: u64) {
        self.run(Op::PopTo(entry), false);
    }

    pub fn up(&self) {
        self.run(Op::Up, false);
    }

    pub fn replace(&self, to: &Location) {
        self.run(Op::Replace(to.clone()), false);
    }

    /// A sidebar choice: the stack becomes the root, and the page fades.
    pub fn switch_root(&self, to: &Location) {
        self.run(Op::SwitchRoot(to.clone()), false);
    }

    pub fn reset(&self, to: &Location) {
        self.run(Op::Reset(to.clone()), false);
    }

    pub fn forward(&self) {
        self.run(Op::Forward, false);
    }

    pub fn forward_to(&self, entry: u64) {
        self.run(Op::ForwardTo(entry), false);
    }

    /// Open `uri` as a deep link. See [`navigate`].
    pub fn navigate(&self, uri: &str) {
        let scheme = self.table().scheme().to_string();
        if let Some(location) = Location::parse_in(&scheme, uri) {
            self.navigate_to(&location);
        }
    }

    pub fn navigate_to(&self, location: &Location) {
        self.run(Op::Navigate(location.clone()), false);
    }

    /// Run `op`. A page that vetoes leaving, a modal that must close first and
    /// a change that must be animated are all handled here, so every
    /// way of moving shares one path.
    pub fn run(&self, op: Op, force: bool) -> Outcome {
        let mut model = self.model;
        let mut next = model.peek().clone();
        let outcome = next.apply(op, force);
        match &outcome {
            Outcome::Changed(change) => {
                *model.write() = next;
                self.record(change);
                self.forget(&change.removed);
            }
            Outcome::Anchored(_) => *model.write() = next,
            Outcome::DismissModal(id) => {
                let handler = self.dismissers.peek().get(id).cloned();
                match handler {
                    Some(dismiss) => dismiss.call(()),
                    None => {
                        // The surface is gone without saying so; forget it and
                        // let Back do what it was asked.
                        model.write().close_modal(*id);
                        return self.run(Op::Back, force);
                    }
                }
            }
            Outcome::Vetoed(PopGuard::Confirm(question), op) => self.ask(question, (**op).clone()),
            Outcome::Vetoed(..) | Outcome::Unchanged => {}
        }
        outcome
    }

    fn record(&self, change: &Change) {
        let mut journal = self.journal;
        let seq = journal.peek().seq + 1;
        *journal.write() = Journal {
            seq,
            change: Some(change.clone()),
        };
    }

    /// Drop the retained state of entries that left the stack.
    fn forget(&self, removed: &[Entry]) {
        if removed.is_empty() {
            return;
        }
        let model = self.model.peek();
        let alive: Vec<u64> = model
            .back_entries()
            .iter()
            .chain(model.forward_entries())
            .map(|entry| entry.id)
            .chain(std::iter::once(model.current().id))
            .collect();
        let mut retained = self.retained;
        retained
            .write()
            .retain(|(entry, _), _| alive.contains(entry) || !removed.iter().any(|gone| gone.id == *entry));
    }

    /// Ask whether to leave a page that has unsaved changes.
    fn ask(&self, question: &Confirmation, op: Op) {
        let Some(overlays) = *self.overlays.peek() else {
            return;
        };
        let router = *self;
        overlays.show_dialog(
            dialog(question.title.clone(), question.body.clone())
                .response(
                    question.discard.clone(),
                    ResponseKind::Destructive,
                    EventHandler::new(move |_: ()| {
                        router.run(op.clone(), true);
                    }),
                )
                .cancel("Keep editing")
                .default_action(CANCEL_ID),
        );
    }

    // ---- derived state ----------------------------------------------------

    /// The title of the current page. Subscribes.
    pub fn title(&self) -> String {
        let model = self.model.read();
        model.title(model.current())
    }

    pub fn back_menu(&self) -> Vec<HistoryItem> {
        self.model.read().back_menu()
    }

    pub fn forward_menu(&self) -> Vec<HistoryItem> {
        self.model.read().forward_menu()
    }

    pub fn can_back(&self) -> bool {
        self.model.read().can_back()
    }

    /// Whether there is a page to go back to; modals do not count. The Back
    /// button shows exactly when this is true.
    pub fn can_go_back(&self) -> bool {
        self.model.read().can_go_back()
    }

    pub fn can_forward(&self) -> bool {
        self.model.read().can_forward()
    }

    // ---- registrations ----------------------------------------------------

    pub(crate) fn set_title(&self, entry: u64, title: &str) {
        if self.model.peek().current().id != entry
            && !self.model.peek().back_entries().iter().any(|e| e.id == entry)
        {
            return;
        }
        let mut next = self.model.peek().clone();
        if next.set_title(entry, title) {
            let mut model = self.model;
            *model.write() = next;
        }
    }

    pub(crate) fn set_guard(&self, entry: u64, guard: Option<Guard>) {
        self.model.peek().set_guard(entry, guard);
    }

    pub(crate) fn open_modal(&self, kind: ModalKind, title: &str, dismiss: EventHandler<()>) -> u64 {
        let mut model = self.model;
        let id = model.write().open_modal(kind, title);
        let mut dismissers = self.dismissers;
        dismissers.write().insert(id, dismiss);
        id
    }

    pub(crate) fn set_dismiss(&self, id: u64, dismiss: EventHandler<()>) {
        let mut dismissers = self.dismissers;
        dismissers.write().insert(id, dismiss);
    }

    pub(crate) fn close_modal(&self, id: u64) {
        let mut dismissers = self.dismissers;
        dismissers.write().remove(&id);
        if self.model.peek().modals().iter().any(|modal| modal.id == id) {
            let mut model = self.model;
            model.write().close_modal(id);
        }
    }

    /// The state `key` of entry `entry`, made with `init` the first time. It
    /// lives while the entry is on a stack.
    pub fn retained_state<T: Clone + 'static>(
        &self,
        entry: u64,
        key: &'static str,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        if let Some(found) = self
            .retained
            .peek()
            .get(&(entry, key))
            .and_then(|boxed| boxed.downcast_ref::<State<T>>())
        {
            return *found;
        }
        let state = State::create_in_scope(init(), self.scope);
        let mut retained = self.retained;
        retained.write().insert((entry, key), Box::new(state));
        state
    }
}

// ---- hooks ----------------------------------------------------------------

/// The entry a page is drawn for, provided around every page.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct EntryContext {
    pub(crate) id: u64,
}

/// The id of the entry the calling page belongs to.
pub fn use_entry_id() -> Option<u64> {
    use_try_consume::<EntryContext>().map(|entry| entry.id)
}

/// Name the calling page for the header, the history menu and the window
/// title, once it knows better than the route (a device's name, a profile's).
/// The route's own title shows until then.
pub fn use_page_title(title: &str) {
    let router = use_try_consume::<Router>();
    let entry = use_entry_id();
    use_side_effect_with_deps(&(entry, title.to_string()), move |(entry, title)| {
        if let (Some(router), Some(entry)) = (router, entry) {
            router.set_title(*entry, title);
        }
    });
}

/// Veto leaving the calling page. `guard` runs whenever the stack is about to
/// drop the page (Back, a menu choice, Up, a sidebar switch, a deep link):
/// `PopGuard::Allow`, `Block`, or `Confirm` to ask first.
pub fn use_pop_guard(guard: impl Fn() -> PopGuard + 'static) {
    let router = use_try_consume::<Router>();
    let entry = use_entry_id();
    // The closure changes every render; the registration does not.
    let current: std::rc::Rc<std::cell::RefCell<Box<dyn Fn() -> PopGuard>>> =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Box::new(|| PopGuard::Allow) as Box<dyn Fn() -> PopGuard>)));
    *current.borrow_mut() = Box::new(guard);
    let registered = current.clone();
    use_side_effect_with_deps(&entry, move |entry| {
        if let (Some(router), Some(entry)) = (router, *entry) {
            let shared = registered.clone();
            router.set_guard(entry, Some(std::rc::Rc::new(move || (shared.borrow())())));
        }
    });
    use_drop(move || {
        if let (Some(router), Some(entry)) = (router, entry) {
            router.set_guard(entry, None);
        }
    });
}

/// UI state that survives Back: the same `State` comes back when the page is
/// returned to, and goes when its entry leaves the stack. Outside a router it
/// is an ordinary `use_state`.
pub fn use_retained_state<T: Clone + 'static>(
    key: &'static str,
    init: impl FnOnce() -> T,
) -> State<T> {
    let router = use_try_consume::<Router>();
    let entry = use_entry_id();
    match (router, entry) {
        (Some(router), Some(entry)) => router.retained_state(entry, key, init),
        _ => use_state(init),
    }
}

/// The calling floating surface is part of the stack while `open`: Back
/// dismisses it (`on_dismiss`) before any page is left. Panels call this.
pub(crate) fn use_modal(
    open: bool,
    kind: ModalKind,
    title: &str,
    on_dismiss: impl Into<EventHandler<()>>,
) {
    let router = use_try_consume::<Router>();
    let on_dismiss = on_dismiss.into();
    let mut slot = use_state(|| None::<u64>);
    if let (Some(router), Some(id)) = (router, *slot.peek()) {
        router.set_dismiss(id, on_dismiss.clone());
    }
    let title = title.to_string();
    use_side_effect_with_deps(&open, move |open| {
        let Some(router) = router else { return };
        let current = *slot.peek();
        match (*open, current) {
            (true, None) => {
                let id = router.open_modal(kind, &title, on_dismiss.clone());
                slot.set(Some(id));
            }
            (false, Some(id)) => {
                router.close_modal(id);
                slot.set(None);
            }
            _ => {}
        }
    });
    use_drop(move || {
        if let (Some(router), Some(id)) = (router, slot.try_peek().and_then(|id| *id)) {
            router.close_modal(id);
        }
    });
}

// ---- the page for a location nothing owns ----------------------------------

/// What a location no route owns shows: where it was, and a way out.
pub(crate) fn not_found_page(location: Location) -> Element {
    NotFound { location }.into()
}

#[derive(PartialEq)]
struct NotFound {
    location: Location,
}

impl Component for NotFound {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let table = router.table();
        let target = table.up(&self.location);
        let home = table.home();
        let page = status_page(Icon::DialogWarning, "Page not found")
            .description(self.location.to_string());
        match target {
            Some(up) => page.action("Go up", move |_| router.navigate_to(&up)),
            None => page.action("Go home", move |_| router.navigate_to(&home)),
        }
    }
}

/// Whether a change moves the page (as opposed to doing nothing visible).
pub fn animates(transition: Transition) -> bool {
    transition != Transition::None
}
