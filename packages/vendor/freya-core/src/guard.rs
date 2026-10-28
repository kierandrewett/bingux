//! Bingux patch: a panic in the UI never unwinds into the event loop.
//!
//! Upstream Freya lets a panic in a component, an event handler, an effect or a
//! task unwind through the runner into winit, which aborts the process. Here
//! every place that runs app code catches the panic, turns it into a
//! [`Failure`] and carries on:
//!
//! * a component render ([`render`]) is replaced by an empty placeholder, the
//!   component's hook state and tasks are dropped, and the failure is handed to
//!   the nearest [`RenderSink`] up the scope tree (an error boundary);
//! * an event handler, an effect, a task and a `use_drop` callback ([`run`])
//!   are abandoned, and the failure goes to the window's [`NoticeSink`];
//! * a panic in the framework itself ([`framework`]) is counted, and the
//!   renderer asks for the whole window to be rebuilt.
//!
//! Every failure is written to stderr with the component path, the panic
//! location and a backtrace (`RUST_BACKTRACE` is honoured), and passed to the
//! process-wide observer ([`set_observer`]). [`install_panic_hook`] makes
//! panics on other threads (worker pools) reach the same observer.

use std::{
    any::Any,
    backtrace::Backtrace,
    cell::{
        Cell,
        RefCell,
    },
    panic::{
        AssertUnwindSafe,
        PanicHookInfo,
        catch_unwind,
    },
    rc::Rc,
    sync::{
        Arc,
        Once,
        RwLock,
    },
    time::{
        Duration,
        Instant,
    },
};

use crate::{
    current_context::CurrentContext,
    element::Element,
    scope_id::ScopeId,
};

/// What was running when it panicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// A component's `render`.
    Render,
    /// An event handler.
    Event,
    /// A side effect (`use_side_effect` and friends).
    Effect,
    /// A spawned task.
    Task,
    /// A `use_drop` callback or the drop of a scope's state.
    Drop,
    /// The renderer or the runner itself.
    Framework,
    /// A thread that is not the UI thread, such as a worker.
    Thread,
}

impl FailureKind {
    pub fn label(self) -> &'static str {
        match self {
            FailureKind::Render => "render",
            FailureKind::Event => "event handler",
            FailureKind::Effect => "effect",
            FailureKind::Task => "task",
            FailureKind::Drop => "cleanup",
            FailureKind::Framework => "framework",
            FailureKind::Thread => "background thread",
        }
    }
}

/// One caught panic.
#[derive(Clone, Debug)]
pub struct Failure {
    pub kind: FailureKind,
    /// The scope that was running, when there was one.
    pub scope: Option<ScopeId>,
    /// The failing component's type name, or the thread name for
    /// [`FailureKind::Thread`].
    pub component: &'static str,
    /// Type names from the root scope down to the failing one.
    pub path: Vec<&'static str>,
    pub message: String,
    /// `file:line:column` of the `panic!`.
    pub location: Option<String>,
    /// Present when `RUST_BACKTRACE` (or `RUST_LIB_BACKTRACE`) asks for one.
    pub backtrace: Option<String>,
    pub thread: String,
    /// Out of memory, a lost GPU: nothing the UI can recover from in place.
    pub unrecoverable: bool,
}

impl Failure {
    /// The failure as plain text, for a clipboard or a bug report.
    pub fn details(&self) -> String {
        let mut text = format!(
            "{} failed: {}\ncomponent: {}\npath: {}\n",
            self.kind.label(),
            self.message,
            if self.component.is_empty() {
                "-"
            } else {
                self.component
            },
            if self.path.is_empty() {
                "-".to_string()
            } else {
                self.path.join(" > ")
            },
        );
        if let Some(location) = &self.location {
            text.push_str(&format!("at: {location}\n"));
        }
        text.push_str(&format!("thread: {}\n", self.thread));
        match &self.backtrace {
            Some(backtrace) => text.push_str(&format!("backtrace:\n{backtrace}\n")),
            None => text.push_str("backtrace: not captured (run with RUST_BACKTRACE=1)\n"),
        }
        text
    }
}

/// Provided by an error boundary. A render that panics hands its [`Failure`]
/// to the nearest sink above it, which swaps the subtree for an error card.
#[derive(Clone)]
pub struct RenderSink(pub Rc<dyn Fn(Failure)>);

/// Provided once per window. An event handler, effect or task that panics
/// hands its [`Failure`] here, which tells the user (a toast).
#[derive(Clone)]
pub struct NoticeSink(pub Rc<dyn Fn(&Failure)>);

type Observer = dyn Fn(&Failure) + Send + Sync;

static OBSERVER: RwLock<Option<Arc<Observer>>> = RwLock::new(None);
static UNRECOVERABLE: RwLock<Option<Arc<Observer>>> = RwLock::new(None);

/// Called, from any thread, for every failure that is reported. The ring
/// buffer of recent errors and the inspector hang here.
pub fn set_observer(observer: impl Fn(&Failure) + Send + Sync + 'static) {
    *OBSERVER.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(observer));
}

/// Called, from any thread, when a failure cannot be recovered in place: out
/// of memory, a lost GPU, or the framework failing again and again. The app
/// relaunches itself here.
pub fn set_unrecoverable_handler(handler: impl Fn(&Failure) + Send + Sync + 'static) {
    *UNRECOVERABLE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(handler));
}

/// Wording that no UI can recover from in place.
const UNRECOVERABLE_WORDING: &[&str] = &[
    "out of memory",
    "outofmemory",
    "out_of_memory",
    "memory allocation of",
    "failed to allocate",
    "device lost",
    "device_lost",
    "devicelost",
    "context lost",
    "context_lost",
    "contextlost",
    "surface lost",
    "surface_lost",
    "lost the gpu",
    "gpu hang",
];

fn is_unrecoverable(message: &str) -> bool {
    let lower = message.to_lowercase();
    UNRECOVERABLE_WORDING
        .iter()
        .any(|needle| lower.contains(needle))
}

/// What the panic hook saw, kept for the catch site on the same thread.
#[derive(Clone)]
struct Captured {
    message: String,
    location: Option<String>,
    backtrace: Option<String>,
    thread: String,
}

thread_local! {
    static LAST_PANIC: RefCell<Option<Captured>> = const { RefCell::new(None) };
    /// How many catch sites this thread is inside. The hook leaves the report
    /// to the catch site, which knows the component path; outside any, the
    /// hook reports the panic itself.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    static RENDER_PANICKED: Cell<bool> = const { Cell::new(false) };
    static RECENT: RefCell<Vec<(String, Instant)>> = const { RefCell::new(Vec::new()) };
    static FRAMEWORK_FAILURES: RefCell<Vec<Instant>> = const { RefCell::new(Vec::new()) };
    /// Set while the hook reports, so a panic in an observer cannot recurse.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

struct Depth;

impl Depth {
    fn enter() -> Self {
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        Depth
    }
}

impl Drop for Depth {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic with a payload that is not text".to_string()
    }
}

fn capture(info: &PanicHookInfo<'_>) -> Captured {
    let backtrace = Backtrace::capture();
    let backtrace = (backtrace.status() == std::backtrace::BacktraceStatus::Captured)
        .then(|| backtrace.to_string());
    Captured {
        message: payload_text(info.payload()),
        location: info
            .location()
            .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column())),
        backtrace,
        thread: std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string(),
    }
}

/// Replace the process's panic hook. It records what each panic said, so the
/// catch site can report it with the component path, and reports by itself
/// the panics nothing catches (a worker thread, the code outside the UI).
/// Never aborts: unwinding goes on as usual.
pub fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let captured = capture(info);
            let guarded = DEPTH.with(|depth| depth.get() > 0);
            LAST_PANIC.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut() {
                    *slot = Some(captured.clone());
                }
            });
            if !guarded && !IN_HOOK.with(|flag| flag.replace(true)) {
                let failure = Failure {
                    kind: FailureKind::Thread,
                    scope: None,
                    component: "",
                    path: Vec::new(),
                    unrecoverable: is_unrecoverable(&captured.message),
                    message: captured.message,
                    location: captured.location,
                    backtrace: captured.backtrace,
                    thread: captured.thread,
                };
                announce(&failure);
                IN_HOOK.with(|flag| flag.set(false));
            }
        }));
    });
}

/// Print and observe a failure.
fn announce(failure: &Failure) {
    let mut line = format!(
        "[bingux] {} panicked",
        match failure.kind {
            FailureKind::Thread => format!("thread `{}`", failure.thread),
            kind => kind.label().to_string(),
        }
    );
    if !failure.path.is_empty() {
        line.push_str(&format!(" in {}", failure.path.join(" > ")));
    }
    line.push_str(&format!(": {}", failure.message));
    if let Some(location) = &failure.location {
        line.push_str(&format!("\n    at {location}"));
    }
    match &failure.backtrace {
        Some(backtrace) => line.push_str(&format!("\n{backtrace}")),
        None => line.push_str("\n    note: run with RUST_BACKTRACE=1 for a backtrace"),
    }
    eprintln!("{line}");
    let observer = OBSERVER
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .cloned();
    if let Some(observer) = observer {
        let _ = catch_unwind(AssertUnwindSafe(|| observer(failure)));
    }
    if failure.unrecoverable {
        let handler = UNRECOVERABLE
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .cloned();
        if let Some(handler) = handler {
            let _ = catch_unwind(AssertUnwindSafe(|| handler(failure)));
        }
    }
}

/// The names of the scopes from the root to `scope`.
fn scope_path(scope: ScopeId) -> Vec<&'static str> {
    CurrentContext::try_with(|context| {
        let storages = context.scopes_storages.try_borrow().ok()?;
        let mut names = Vec::new();
        let mut next = Some(scope);
        while let Some(id) = next {
            let storage = storages.get(&id)?;
            names.push(if storage.name.is_empty() {
                "?"
            } else {
                storage.name
            });
            next = storage.parent_id;
        }
        names.reverse();
        Some(names)
    })
    .flatten()
    .unwrap_or_default()
}

fn current_scope() -> Option<ScopeId> {
    CurrentContext::try_with(|context| context.scope_id)
}

/// Build the failure for a panic just caught on this thread.
fn failure_of(
    kind: FailureKind,
    scope: Option<ScopeId>,
    component: &'static str,
    payload: Box<dyn Any + Send>,
) -> Failure {
    let captured = LAST_PANIC.with(|slot| slot.try_borrow_mut().ok().and_then(|mut s| s.take()));
    let (message, location, backtrace, thread) = match captured {
        Some(captured) => (
            captured.message,
            captured.location,
            captured.backtrace,
            captured.thread,
        ),
        None => (
            payload_text(payload.as_ref()),
            None,
            None,
            std::thread::current()
                .name()
                .unwrap_or("<unnamed>")
                .to_string(),
        ),
    };
    // Dropping the payload may run a destructor that panics.
    let _ = catch_unwind(AssertUnwindSafe(move || drop(payload)));
    let path = scope.map(scope_path).unwrap_or_default();
    Failure {
        kind,
        scope,
        component: if component.is_empty() {
            path.last().copied().unwrap_or("")
        } else {
            component
        },
        path,
        unrecoverable: is_unrecoverable(&message),
        message,
        location,
        backtrace,
        thread,
    }
}

/// The same panic repeated in a loop (a handler that fails on every pointer
/// move) is reported once for a while, not on every call.
fn is_repeat(failure: &Failure) -> bool {
    const WINDOW: Duration = Duration::from_secs(2);
    let key = format!(
        "{:?}|{}|{}",
        failure.kind,
        failure.message,
        failure.location.as_deref().unwrap_or("")
    );
    let now = Instant::now();
    RECENT.with(|recent| {
        let mut recent = recent.borrow_mut();
        recent.retain(|(_, at)| now.duration_since(*at) < WINDOW);
        if recent.iter().any(|(seen, _)| *seen == key) {
            return true;
        }
        recent.push((key, now));
        false
    })
}

fn report(failure: &Failure) {
    if is_repeat(failure) {
        return;
    }
    announce(failure);
    if matches!(failure.kind, FailureKind::Render | FailureKind::Framework) {
        return;
    }
    let sink = CurrentContext::try_with(|_| {
        crate::lifecycle::context::try_consume_context::<NoticeSink>()
    })
    .flatten();
    if let Some(sink) = sink {
        let _ = catch_unwind(AssertUnwindSafe(|| (sink.0)(failure)));
    }
}

/// Run `run`, catching a panic. `None` after a panic, which has been reported:
/// the work is abandoned and the caller carries on.
pub fn run<T>(kind: FailureKind, run: impl FnOnce() -> T) -> Option<T> {
    let depth = Depth::enter();
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => Some(value),
        Err(payload) => {
            drop(depth);
            let failure = failure_of(kind, current_scope(), "", payload);
            report(&failure);
            None
        }
    }
}

/// Run `run` for the renderer or the runner itself. Several failures in a few
/// seconds mean the window cannot be kept consistent: the unrecoverable
/// handler is asked to relaunch the app.
pub fn framework<T>(what: &'static str, run: impl FnOnce() -> T) -> Option<T> {
    let depth = Depth::enter();
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => Some(value),
        Err(payload) => {
            drop(depth);
            let mut failure = failure_of(FailureKind::Framework, None, what, payload);
            const BURST: usize = 5;
            const WINDOW: Duration = Duration::from_secs(3);
            let now = Instant::now();
            let burst = FRAMEWORK_FAILURES.with(|failures| {
                let mut failures = failures.borrow_mut();
                failures.retain(|at| now.duration_since(*at) < WINDOW);
                failures.push(now);
                failures.len() >= BURST
            });
            if burst {
                failure.unrecoverable = true;
                // Past the repeat filter: the handler must hear this one.
                announce(&failure);
            } else {
                report(&failure);
            }
            None
        }
    }
}

/// Run `run`, a step the window cannot go on without (rebuilding a lost graphics
/// driver). A panic is unrecoverable by definition: it is reported, the
/// unrecoverable handler gets its chance to relaunch the app, and `None` comes
/// back if the process is still here, so the caller can close the window.
pub fn unrecoverable<T>(what: &'static str, run: impl FnOnce() -> T) -> Option<T> {
    let depth = Depth::enter();
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => Some(value),
        Err(payload) => {
            drop(depth);
            let mut failure = failure_of(FailureKind::Framework, None, what, payload);
            failure.unrecoverable = true;
            announce(&failure);
            None
        }
    }
}

/// Record the type name of the component the current scope is running, for
/// failure paths. Cheap: one map lookup per render.
pub(crate) fn name_current_scope(name: &'static str) {
    CurrentContext::try_with(|context| {
        if let Ok(mut storages) = context.scopes_storages.try_borrow_mut()
            && let Some(storage) = storages.get_mut(&context.scope_id)
        {
            storage.name = name;
        }
    });
}

/// Run a component's render. A panic is caught, the component's hook state and
/// tasks are dropped (the scope starts clean if it ever renders again), the
/// failure goes to the nearest [`RenderSink`] above it, and an empty placeholder
/// stands in for the component's output.
pub fn render(name: &'static str, run: impl FnOnce() -> Element) -> Element {
    name_current_scope(name);
    let depth = Depth::enter();
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(element) => element,
        Err(payload) => {
            drop(depth);
            render_failed(name, payload)
        }
    }
}

fn render_failed(name: &'static str, payload: Box<dyn Any + Send>) -> Element {
    let scope = current_scope();
    let failure = failure_of(FailureKind::Render, scope, name, payload);
    RENDER_PANICKED.with(|flag| flag.set(true));
    if let Some(scope) = scope {
        reset_failed_scope(scope);
    }
    if !is_repeat(&failure) {
        announce(&failure);
    }
    let parent = scope.and_then(|scope| {
        CurrentContext::try_with(|context| {
            context
                .scopes_storages
                .try_borrow()
                .ok()
                .and_then(|storages| storages.get(&scope).and_then(|storage| storage.parent_id))
        })
        .flatten()
    });
    let sink = parent.and_then(|parent| {
        CurrentContext::try_with(|_| {
            crate::lifecycle::context::try_consume_context_from_scope_id::<RenderSink>(Some(
                parent,
            ))
        })
        .flatten()
    });
    match sink {
        Some(sink) => {
            let handed = failure.clone();
            let _ = catch_unwind(AssertUnwindSafe(|| (sink.0)(handed)));
            placeholder()
        }
        None => fallback(&failure),
    }
}

/// Drop everything a failed render left in its scope: the hooks it had made,
/// the contexts it had provided, the tasks it had spawned. The scope's owner
/// (its `State` boxes) lives on until the scope unmounts, which the boundary
/// does by replacing the subtree.
fn reset_failed_scope(scope: ScopeId) {
    let hooks = CurrentContext::try_with(|context| {
        let mut storages = context.scopes_storages.try_borrow_mut().ok()?;
        let storage = storages.get_mut(&scope)?;
        let contexts = std::mem::take(&mut storage.contexts);
        Some((storage.reset_hooks(), contexts))
    })
    .flatten();
    let tasks = CurrentContext::try_with(|context| {
        let mut tasks = context.tasks.try_borrow_mut().ok()?;
        let mut removed = Vec::new();
        tasks.retain(|_, task| {
            let own = task.try_borrow().map(|t| t.scope_id == scope).unwrap_or(false);
            if own {
                removed.push(task.clone());
            }
            !own
        });
        Some(removed)
    })
    .flatten();
    // Destructors of user state run outside every borrow, and may panic.
    let _ = catch_unwind(AssertUnwindSafe(move || drop((hooks, tasks))));
}

/// Whether the render that just ran was replaced after a panic. The runner
/// asks once per render, right after it: a failed scope's hook counters stay
/// at zero so a later render starts from scratch.
pub(crate) fn take_render_panicked() -> bool {
    RENDER_PANICKED.with(|flag| flag.replace(false))
}

fn placeholder() -> Element {
    use crate::elements::rect::rect;
    rect().into()
}

/// What stands in for a failed component when no boundary is above it.
fn fallback(failure: &Failure) -> Element {
    use crate::{
        elements::{
            extensions::*,
            label::label,
            rect::rect,
        },
        style::color::Color,
    };
    rect()
        .child(
            label()
                .text(format!(
                    "{} failed: {}",
                    failure.component, failure.message
                ))
                .color(Color::RED),
        )
        .into()
}
