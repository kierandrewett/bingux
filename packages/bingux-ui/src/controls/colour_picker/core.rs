//! What the parts of a colour picker share: the one state model.
//!
//! The inline picker and the picker a [`ColourButton`](super::ColourButton)
//! opens are the same component over the same state: the colour being edited,
//! held as hue, saturation, value and alpha ([`Hsva`]), and the way out. A
//! part that edits the colour (the plane, a strip, a swatch, a box) goes
//! through [`Link::set`]; a part that draws it reads [`Link::shown`].
//!
//! A `value` that is the colour the picker just reported changes nothing; any
//! other (the caller set it, a swatch or the hex field made it) is taken in as
//! an edit of the colour on screen, so a grey keeps its hue and a black its
//! saturation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

use super::model::{Hsva, push_recent};

/// The most recent colours kept: one row of the palette.
pub const RECENT_MAX: usize = super::geometry::COLUMNS;

/// What the parts of the picker share. Equal to another link of the same
/// picker, so a part whose own values have not changed is not drawn again
/// when its neighbour is.
#[derive(Clone)]
pub(super) struct Link {
    /// The colour being edited, in the parts the picker edits it in.
    model: State<Hsva>,
    /// The caller's newest `on_change`.
    out: Rc<RefCell<EventHandler<Color>>>,
    /// Whether a drag is on: the caller's `value` is not taken in meanwhile.
    held: Rc<Cell<bool>>,
    /// Whether the colour has been edited since the last [`settle`](Self::settle).
    dirty: Rc<Cell<bool>>,
    /// What runs when an edit is over, with the colour it ended on.
    ended: Rc<RefCell<EventHandler<Color>>>,
    /// Whether the picker has an opacity strip. Without one it only makes
    /// opaque colours.
    alpha: bool,
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
            && self.alpha == other.alpha
            && Rc::ptr_eq(&self.out, &other.out)
            && Rc::ptr_eq(&self.held, &other.held)
    }
}

impl Link {
    pub(super) fn new(
        model: State<Hsva>,
        out: Rc<RefCell<EventHandler<Color>>>,
        held: Rc<Cell<bool>>,
        dirty: Rc<Cell<bool>>,
        ended: Rc<RefCell<EventHandler<Color>>>,
        alpha: bool,
    ) -> Self {
        Self {
            model,
            out,
            held,
            dirty,
            ended,
            alpha,
        }
    }

    /// Whether the picker edits opacity.
    pub(super) fn alpha(&self) -> bool {
        self.alpha
    }

    /// The colour being edited, without subscribing to it.
    pub(super) fn current(&self) -> Hsva {
        *self.model.peek()
    }

    /// `value` as this picker takes it in: without an opacity strip the
    /// picker has no opacity to give it.
    fn taken(&self, colour: Hsva) -> Hsva {
        if self.alpha { colour } else { colour.solid() }
    }

    /// Edits the colour, and reports it when the RGB colour is a new one.
    pub(super) fn set(&self, next: Hsva) {
        let next = self.taken(next);
        let before = self.current();
        if before == next {
            return;
        }
        let mut model = self.model;
        model.set(next);
        if before.colour() != next.colour() {
            self.dirty.set(true);
            let out = self.out.borrow().clone();
            out.call(next.colour());
        }
    }

    /// Edits the colour to `colour`, keeping what it cannot say.
    pub(super) fn set_colour(&self, colour: Color) {
        self.set(self.current().editing(colour));
    }

    /// The caller's `value` is not what is being edited: take it in. A drag
    /// in progress is the truth meanwhile; the caller's echo of it is late.
    pub(super) fn rebase(&self, value: Color) {
        if self.held.get() {
            return;
        }
        let current = self.current();
        let wanted = self.taken(Hsva::from_colour(value));
        if current.colour() != wanted.colour() {
            let mut model = self.model;
            model.set(self.taken(current.editing(value)));
        }
    }

    /// The colour to draw for the caller's `value`: the colour being edited,
    /// or `value` taken in when it is not that. Needs no state write, so no
    /// frame shows the old colour.
    pub(super) fn shown(&self, value: Color) -> Hsva {
        let current = *self.model.read();
        let wanted = self.taken(Hsva::from_colour(value));
        if self.held.get() || current.colour() == wanted.colour() {
            current
        } else {
            self.taken(current.editing(value))
        }
    }

    /// Notes a drag report: a drag is on from its start until its end.
    pub(super) fn grab(&self, held: bool) {
        self.held.set(held);
    }

    /// An edit is over (the pointer came up, a key was released, Enter was
    /// pressed): tell whoever remembers colours, once per edit.
    pub(super) fn settle(&self) {
        if self.dirty.replace(false) {
            let ended = self.ended.borrow().clone();
            ended.call(self.current().colour());
        }
    }
}

/// The recent colours of a picker: the caller's own list, or one the process
/// keeps in memory.
#[derive(Clone, PartialEq)]
pub(crate) struct Recents {
    /// The list the caller keeps, and what to tell it when a colour is added.
    controlled: Option<(Vec<Color>, EventHandler<Vec<Color>>)>,
    /// The process's list, mirrored here so a change draws the picker again.
    memory: State<Vec<Color>>,
    /// Whether the picker shows the row at all.
    enabled: bool,
}

thread_local! {
    /// The recent colours of this process, newest first, when no picker was
    /// given a list of its own. An app that wants them across runs passes its
    /// own list with `recent(list, on_change)`.
    static MEMORY: RefCell<Vec<Color>> = const { RefCell::new(Vec::new()) };
}

/// The recent colours the process has kept in memory, newest first.
pub(crate) fn remembered() -> Vec<Color> {
    MEMORY.with(|memory| memory.borrow().clone())
}

/// Forgets the colours the process kept in memory.
#[cfg(test)]
pub(crate) fn forget_remembered() {
    MEMORY.with(|memory| memory.borrow_mut().clear());
}

/// The recent colours for the calling picker. `controlled` is the caller's
/// own list, if it passed one; `enabled` is whether the row is shown.
pub(crate) fn use_recents(
    controlled: Option<(Vec<Color>, EventHandler<Vec<Color>>)>,
    enabled: bool,
) -> Recents {
    let memory = use_state(remembered);
    Recents {
        controlled,
        memory,
        enabled,
    }
}

impl Recents {
    /// Whether the row is shown.
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    /// The colours to show, newest first.
    pub(crate) fn list(&self) -> Vec<Color> {
        match &self.controlled {
            Some((list, _)) => list.clone(),
            None => self.memory.read().clone(),
        }
    }

    /// Adds `colour` to the front.
    pub(crate) fn add(&self, colour: Color) {
        if !self.enabled {
            return;
        }
        match &self.controlled {
            Some((list, on_change)) => {
                on_change.call(push_recent(list, colour, RECENT_MAX));
            }
            None => {
                let next = MEMORY.with(|memory| {
                    let next = push_recent(&memory.borrow(), colour, RECENT_MAX);
                    *memory.borrow_mut() = next.clone();
                    next
                });
                let mut local = self.memory;
                local.set(next);
            }
        }
    }
}
