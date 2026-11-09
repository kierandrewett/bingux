//! Undo and redo: one stack per window, shared by all of its views (AppKit
//! `NSUndoManager`, SwiftUI's `undoManager`).
//!
//! A change that can be taken back is pushed as an [`UndoStep`]: its name as
//! people read it ("Rename", "Move to Trash"), what undoes it and what does
//! it again. The window's Edit commands follow the stack: **Undo rename**
//! (Ctrl+Z) and **Redo rename** (Ctrl+Shift+Z, Ctrl+Y), disabled with nothing
//! to take back. They appear in the main menu, the palette and the shortcuts
//! sheet of any window whose components called [`use_undo`], and nowhere else.
//!
//! ```ignore
//! let undo = use_undo();
//! let old = name.peek().clone();
//! name.set(new.clone());
//! undo.push(UndoStep::new(
//!     "Rename",
//!     move || name.set(old.clone()),
//!     move || name.set(new.clone()),
//! ));
//! ```
//!
//! - **Redo.** Undo moves a step to the redo stack; Redo moves it back. A new
//!   change empties the redo stack.
//! - **Groups.** [`UndoStack::group`] makes every step pushed inside one
//!   step, undone and redone together under the group's name.
//! - **Coalescing.** Steps with the same [`UndoStep::coalesce`] key, pushed
//!   within [`COALESCE`] of each other, are one step: a run of typing or a
//!   slider drag is undone at once.
//! - **A limit.** The stack keeps the newest [`DEFAULT_LIMIT`] steps
//!   ([`UndoStack::set_limit`]); older ones are forgotten.
//! - **Steps that cannot be redone.** A step built with [`UndoStep::undo_only`]
//!   (restoring from the Trash, where the original is gone) is undone and then
//!   dropped; it also drops the redo history behind it.
//! - **Work that takes time.** The closures run on the UI thread and return at
//!   once; one that moves files starts the work and reports when it is done,
//!   as anything else on the UI thread does. A step pushed while the stack
//!   runs a step is ignored: undoing must not record itself.
//!
//! A text field keeps its own undo for the text it edits: Ctrl+Z in a field
//! undoes typing, and the field stops the key, so the window's Undo only
//! runs while the focus is elsewhere.

use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::commands::{Command, CommandGroup};

/// How many steps a stack keeps unless told otherwise.
pub const DEFAULT_LIMIT: usize = 100;

/// How close together coalescing steps must come to be merged.
pub const COALESCE: Duration = Duration::from_millis(1500);

type Action = Box<dyn FnMut()>;

/// One change that can be taken back. Build with [`UndoStep::new`].
pub struct UndoStep {
    name: String,
    undo: Action,
    redo: Option<Action>,
    coalesce: Option<String>,
}

impl UndoStep {
    /// A change called `name` ("Rename", "Delete row": a verb and what it
    /// acted on, in sentence case), undone by `undo` and done again by `redo`.
    pub fn new(
        name: impl Into<String>,
        undo: impl FnMut() + 'static,
        redo: impl FnMut() + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            undo: Box::new(undo),
            redo: Some(Box::new(redo)),
            coalesce: None,
        }
    }

    /// A change that can be undone but not redone.
    pub fn undo_only(name: impl Into<String>, undo: impl FnMut() + 'static) -> Self {
        Self {
            name: name.into(),
            undo: Box::new(undo),
            redo: None,
            coalesce: None,
        }
    }

    /// Merge with the step before when it has the same `key` and came less
    /// than [`COALESCE`] ago, so a run of small changes is undone at once.
    pub fn coalesce(mut self, key: impl Into<String>) -> Self {
        self.coalesce = Some(key.into());
        self
    }
}

/// One entry of the stack: a step, or a group or run of them.
struct Entry {
    name: String,
    /// In the order they were made. Undo runs them backwards.
    steps: Vec<UndoStep>,
    coalesce: Option<String>,
    last: Instant,
}

impl Entry {
    fn redoable(&self) -> bool {
        self.steps.iter().all(|step| step.redo.is_some())
    }
}

/// The open groups, outermost first, each with what was pushed inside it.
struct OpenGroup {
    name: String,
    steps: Vec<UndoStep>,
}

struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    limit: usize,
    groups: Vec<OpenGroup>,
    running: bool,
    /// A component of the window called [`use_undo`]: the window shows Undo
    /// and Redo.
    adopted: bool,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: DEFAULT_LIMIT,
            groups: Vec::new(),
            running: false,
            adopted: false,
        }
    }
}

impl History {
    fn push(&mut self, step: UndoStep, now: Instant) {
        if self.running {
            return;
        }
        if let Some(group) = self.groups.last_mut() {
            group.steps.push(step);
            return;
        }
        self.redo.clear();
        if let (Some(key), Some(top)) = (step.coalesce.as_ref(), self.undo.last_mut())
            && top.coalesce.as_ref() == Some(key)
            && now.duration_since(top.last) < COALESCE
        {
            top.steps.push(step);
            top.last = now;
            return;
        }
        self.undo.push(Entry {
            name: step.name.clone(),
            coalesce: step.coalesce.clone(),
            steps: vec![step],
            last: now,
        });
        self.trim();
    }

    fn trim(&mut self) {
        let extra = self.undo.len().saturating_sub(self.limit);
        self.undo.drain(..extra);
    }

    fn close_group(&mut self, now: Instant) {
        let Some(group) = self.groups.pop() else {
            return;
        };
        if group.steps.is_empty() {
            return;
        }
        if let Some(outer) = self.groups.last_mut() {
            outer.steps.extend(group.steps);
            return;
        }
        self.redo.clear();
        self.undo.push(Entry {
            name: group.name,
            steps: group.steps,
            coalesce: None,
            last: now,
        });
        self.trim();
    }

    /// Takes the newest entry of the undo (or redo) stack out to run it, or
    /// `None` when there is none or a step is already running.
    fn begin(&mut self, redo: bool) -> Option<Entry> {
        if self.running {
            return None;
        }
        let entry = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        }?;
        self.running = true;
        Some(entry)
    }

    /// An undone entry goes to the redo stack, if it can be redone.
    fn end_undo(&mut self, entry: Entry) {
        self.running = false;
        if entry.redoable() {
            self.redo.push(entry);
        } else {
            self.redo.clear();
        }
    }

    /// A redone entry goes back to the undo stack.
    fn end_redo(&mut self, mut entry: Entry, now: Instant) {
        self.running = false;
        entry.last = now;
        self.undo.push(entry);
        self.trim();
    }
}

fn run_undo(entry: &mut Entry) {
    for step in entry.steps.iter_mut().rev() {
        (step.undo)();
    }
}

fn run_redo(entry: &mut Entry) {
    for step in &mut entry.steps {
        if let Some(redo) = step.redo.as_mut() {
            redo();
        }
    }
}

/// The window's undo stack. A `Copy` handle: get it with [`use_undo`].
#[derive(Clone, Copy, PartialEq)]
pub struct UndoStack {
    history: State<History>,
}

/// The window's undo stack, and a promise that the window shows Undo and
/// Redo in its Edit menu, palette and shortcuts sheet.
pub fn use_undo() -> UndoStack {
    let stack = use_hook(|| try_consume_context::<UndoStack>().unwrap_or_else(UndoStack::detached));
    use_hook(move || {
        let mut history = stack.history;
        if !history.peek().adopted {
            history.write().adopted = true;
        }
    });
    stack
}

/// Provides the window's stack to the tree below, or returns the one an
/// outer host already provides.
pub(crate) fn provide_undo() -> UndoStack {
    use_hook(|| {
        try_consume_context::<UndoStack>().unwrap_or_else(|| {
            let stack = UndoStack::detached();
            provide_context(stack);
            stack
        })
    })
}

impl UndoStack {
    /// A stack of its own, not the window's: a document in a window of
    /// several keeps one per document.
    pub fn detached() -> Self {
        Self {
            history: State::create(History::default()),
        }
    }

    /// Remember a change. It empties the redo stack, unless it is pushed
    /// inside a [`group`](Self::group) or while a step runs.
    pub fn push(self, step: UndoStep) {
        self.push_at(step, Instant::now());
    }

    fn push_at(self, step: UndoStep, now: Instant) {
        let mut history = self.history;
        if let Some(mut history) = history.try_write() {
            history.push(step, now);
        }
    }

    /// Run `changes`, and make every step it pushes one step called `name`.
    /// Groups nest: an inner group joins the outer one.
    pub fn group<R>(self, name: impl Into<String>, changes: impl FnOnce() -> R) -> R {
        let mut history = self.history;
        history.write().groups.push(OpenGroup {
            name: name.into(),
            steps: Vec::new(),
        });
        let result = changes();
        history.write().close_group(Instant::now());
        result
    }

    /// Take back the newest change. Returns its name, or `None` when there
    /// was nothing to undo.
    pub fn undo(self) -> Option<String> {
        let mut history = self.history;
        let mut entry = history.write().begin(false)?;
        run_undo(&mut entry);
        let name = entry.name.clone();
        history.write().end_undo(entry);
        Some(name)
    }

    /// Do again the change undone last. Returns its name, or `None` when
    /// there was nothing to redo.
    pub fn redo(self) -> Option<String> {
        let mut history = self.history;
        let mut entry = history.write().begin(true)?;
        run_redo(&mut entry);
        let name = entry.name.clone();
        history.write().end_redo(entry, Instant::now());
        Some(name)
    }

    /// Whether there is a change to take back. Reading it in `render`
    /// redraws the component when that changes.
    pub fn can_undo(self) -> bool {
        !self.history.read().undo.is_empty()
    }

    pub fn can_redo(self) -> bool {
        !self.history.read().redo.is_empty()
    }

    /// The name of the change Undo would take back.
    pub fn undo_name(self) -> Option<String> {
        self.history
            .read()
            .undo
            .last()
            .map(|entry| entry.name.clone())
    }

    /// The name of the change Redo would do again.
    pub fn redo_name(self) -> Option<String> {
        self.history
            .read()
            .redo
            .last()
            .map(|entry| entry.name.clone())
    }

    /// The Undo command's title: "Undo rename", or "Undo" with nothing to
    /// undo.
    pub fn undo_title(self) -> String {
        titled(Verb::Undo, self.undo_name())
    }

    /// The Redo command's title: "Redo rename", or "Redo".
    pub fn redo_title(self) -> String {
        titled(Verb::Redo, self.redo_name())
    }

    /// How many steps the stack keeps. Older ones are forgotten at once.
    pub fn set_limit(self, limit: usize) {
        let mut history = self.history;
        let mut history = history.write();
        history.limit = limit.max(1);
        history.trim();
    }

    /// Forget every step: after a save that cannot be undone, or when the
    /// document the steps change is closed.
    pub fn clear(self) {
        let mut history = self.history;
        let mut history = history.write();
        history.undo.clear();
        history.redo.clear();
    }

    /// The Edit menu's Undo and Redo, as they are now, for a window whose
    /// components use the stack. Read when the router or a menu asks, so the
    /// titles are never stale. Each says what it did through `announcer`
    /// ("Undid rename"): the change may be nowhere near the focus, so without
    /// it a screen reader hears nothing.
    pub(crate) fn commands(self, announcer: crate::a11y::Announcer) -> Vec<Command> {
        let Some(history) = self.history.try_peek() else {
            return Vec::new();
        };
        if !history.adopted {
            return Vec::new();
        }
        let undo_name = history.undo.last().map(|entry| entry.name.clone());
        let redo_name = history.redo.last().map(|entry| entry.name.clone());
        drop(history);
        vec![
            Command::new(
                "edit.undo",
                titled(Verb::Undo, undo_name.clone()),
                move |_| {
                    if let Some(name) = self.undo() {
                        announcer.announce(titled(Verb::Undid, Some(name)));
                    }
                },
            )
            .shortcut("Ctrl+Z")
            .enabled(undo_name.is_some())
            .group(CommandGroup::Edit)
            .section("history")
            .hidden_from_palette_if(undo_name.is_none()),
            Command::new(
                "edit.redo",
                titled(Verb::Redo, redo_name.clone()),
                move |_| {
                    if let Some(name) = self.redo() {
                        announcer.announce(titled(Verb::Redid, Some(name)));
                    }
                },
            )
            .shortcut("Ctrl+Shift+Z")
            .shortcut("Ctrl+Y")
            .enabled(redo_name.is_some())
            .group(CommandGroup::Edit)
            .section("history")
            .hidden_from_palette_if(redo_name.is_none()),
        ]
    }
}

/// What a title or an announcement says was, or will be, done.
#[derive(Clone, Copy)]
enum Verb {
    Undo,
    Redo,
    Undid,
    Redid,
}

/// "Undo rename": the catalogue's sentence with the change's name, its first
/// letter in lower case, so the title stays in sentence case ("Undo move to
/// Trash" from "Move to Trash"). With no name, the verb alone.
fn titled(verb: Verb, name: Option<String>) -> String {
    let Some(name) = name else {
        return match verb {
            Verb::Undo => crate::tr!("ui-undo"),
            Verb::Redo => crate::tr!("ui-redo"),
            Verb::Undid => crate::tr!("ui-undo-done-status"),
            Verb::Redid => crate::tr!("ui-redo-done-status"),
        };
    };
    let mut chars = name.chars();
    let first: String = chars
        .next()
        .map(|c| c.to_lowercase().collect())
        .unwrap_or_default();
    let action = format!("{first}{}", chars.as_str());
    match verb {
        Verb::Undo => crate::tr!("ui-undo-named", action = action),
        Verb::Redo => crate::tr!("ui-redo-named", action = action),
        Verb::Undid => crate::tr!("ui-undo-named-done-status", action = action),
        Verb::Redid => crate::tr!("ui-redo-named-done-status", action = action),
    }
}

impl Command {
    fn hidden_from_palette_if(self, hidden: bool) -> Self {
        if hidden {
            self.hidden_from_palette()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    // Failure modes: Redo history surviving a new change; a group undone a
    // step at a time; coalescing across a pause or across different keys; a
    // step recording itself while it is undone; the limit dropping the newest
    // instead of the oldest; an undo-only step leaving a redo it cannot do.

    type Shared = Rc<RefCell<History>>;
    type Log = Rc<RefCell<Vec<i32>>>;

    // The same steps as `UndoStack::undo` and `redo`, on a history that is
    // not in a Freya state: the borrow is released while the steps run.
    fn undo(history: &Shared) -> Option<String> {
        let mut entry = history.borrow_mut().begin(false)?;
        run_undo(&mut entry);
        let name = entry.name.clone();
        history.borrow_mut().end_undo(entry);
        Some(name)
    }

    fn redo(history: &Shared) -> Option<String> {
        let mut entry = history.borrow_mut().begin(true)?;
        run_redo(&mut entry);
        let name = entry.name.clone();
        history.borrow_mut().end_redo(entry, Instant::now());
        Some(name)
    }

    fn add_step(log: &Log, value: i32) -> UndoStep {
        log.borrow_mut().push(value);
        let (undo_log, redo_log) = (log.clone(), log.clone());
        UndoStep::new(
            format!("Add {value}"),
            move || {
                undo_log.borrow_mut().pop();
            },
            move || redo_log.borrow_mut().push(value),
        )
    }

    fn add(history: &Shared, log: &Log, value: i32) {
        let step = add_step(log, value);
        history.borrow_mut().push(step, Instant::now());
    }

    fn top_name(history: &Shared) -> Option<String> {
        history.borrow().undo.last().map(|entry| entry.name.clone())
    }

    #[test]
    fn undo_then_redo_and_a_new_change_ends_redo() {
        let (history, log) = (Shared::default(), Log::default());
        add(&history, &log, 1);
        add(&history, &log, 2);
        assert_eq!(titled(Verb::Undo, top_name(&history)), "Undo add 2");
        assert_eq!(undo(&history).as_deref(), Some("Add 2"));
        assert_eq!(*log.borrow(), [1]);
        assert_eq!(redo(&history).as_deref(), Some("Add 2"));
        assert_eq!(*log.borrow(), [1, 2]);
        undo(&history);
        add(&history, &log, 3);
        assert!(
            history.borrow().redo.is_empty(),
            "a new change ends the redo history"
        );
        assert_eq!(redo(&history), None);
    }

    #[test]
    fn a_group_is_one_step() {
        let (history, log) = (Shared::default(), Log::default());
        let open = |name: &str| {
            history.borrow_mut().groups.push(OpenGroup {
                name: name.to_owned(),
                steps: Vec::new(),
            })
        };
        open("Add two");
        add(&history, &log, 1);
        open("Inner");
        add(&history, &log, 2);
        history.borrow_mut().close_group(Instant::now());
        history.borrow_mut().close_group(Instant::now());
        assert_eq!(top_name(&history).as_deref(), Some("Add two"));
        undo(&history);
        assert!(log.borrow().is_empty(), "both steps are undone together");
        assert!(history.borrow().undo.is_empty());
        redo(&history);
        assert_eq!(*log.borrow(), [1, 2]);
    }

    #[test]
    fn coalescing_needs_one_key_and_no_pause() {
        let (history, log) = (Shared::default(), Log::default());
        let start = Instant::now();
        let push = |value: i32, key: &str, at: Duration| {
            let step = add_step(&log, value).coalesce(key);
            history.borrow_mut().push(step, start + at);
        };
        push(1, "field", Duration::ZERO);
        push(2, "field", Duration::from_millis(400));
        push(3, "other", Duration::from_millis(500));
        push(4, "other", COALESCE + Duration::from_secs(1));
        undo(&history);
        assert_eq!(*log.borrow(), [1, 2, 3], "a pause starts a new step");
        undo(&history);
        assert_eq!(*log.borrow(), [1, 2], "another key is another step");
        undo(&history);
        assert!(log.borrow().is_empty(), "one run is undone at once");
    }

    #[test]
    fn a_step_does_not_record_itself_while_it_runs() {
        let history = Shared::default();
        let inner = history.clone();
        history.borrow_mut().push(
            UndoStep::new(
                "Rename",
                move || {
                    // The app's own rename path, which pushes a step.
                    inner
                        .borrow_mut()
                        .push(UndoStep::new("Rename", || {}, || {}), Instant::now());
                },
                || {},
            ),
            Instant::now(),
        );
        undo(&history);
        assert!(
            history.borrow().undo.is_empty(),
            "nothing was recorded during the undo"
        );
        assert_eq!(history.borrow().redo.len(), 1);
    }

    #[test]
    fn the_limit_forgets_the_oldest() {
        let (history, log) = (Shared::default(), Log::default());
        history.borrow_mut().limit = 3;
        for value in 0..5 {
            add(&history, &log, value);
        }
        let mut names = Vec::new();
        while let Some(name) = undo(&history) {
            names.push(name);
        }
        assert_eq!(names, ["Add 4", "Add 3", "Add 2"]);
    }

    #[test]
    fn an_undo_only_step_leaves_nothing_to_redo() {
        let (history, log) = (Shared::default(), Log::default());
        add(&history, &log, 1);
        undo(&history);
        assert_eq!(history.borrow().redo.len(), 1);
        let inner = log.clone();
        history.borrow_mut().push(
            UndoStep::undo_only("Restore", move || inner.borrow_mut().push(9)),
            Instant::now(),
        );
        undo(&history);
        assert_eq!(*log.borrow(), [9]);
        assert!(history.borrow().redo.is_empty());
    }

    #[test]
    fn titles_are_sentence_case() {
        assert_eq!(
            titled(Verb::Undo, Some("Move to Trash".into())),
            "Undo move to Trash"
        );
        assert_eq!(titled(Verb::Redo, None), "Redo");
    }
}
