//! The file view as an application's content, not a dialog.
//!
//! [`browse`] is the chooser's body (toolbar, places sidebar, address bar,
//! file view, status pill) hosted in a window. It is the same component, so a
//! fix to one is a fix to the other. What is absent is what only a question
//! has: Cancel and the accept button, the file type filter, the name field and
//! the request's choices. What differs is what a person means by opening:
//! a file opens in its default application, and a folder can open in another
//! tab.
//!
//! The host (the Files application) owns the tabs. Each tab gives its body a
//! [`TabMemory`], where the body keeps the folder shown and the Back and
//! Forward lists, so a tab that is hidden and shown again, or restored from a
//! session, is where the person left it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use freya::prelude::*;

use super::chooser::browse_body;
use super::directory::Where;

/// Where one tab is: the folder on show and the folders before and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trail {
    pub at: Where,
    pub back: Vec<Where>,
    pub forward: Vec<Where>,
}

impl Trail {
    pub fn new(at: Where) -> Self {
        Self {
            at,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }
}

/// A tab's [`Trail`], shared between the host and the body that shows it.
#[derive(Clone, Debug)]
pub struct TabMemory(Rc<RefCell<Trail>>);

impl TabMemory {
    pub fn new(at: Where) -> Self {
        Self(Rc::new(RefCell::new(Trail::new(at))))
    }

    /// A copy of the trail now.
    pub fn trail(&self) -> Trail {
        self.0.borrow().clone()
    }

    pub(super) fn set(&self, trail: Trail) {
        *self.0.borrow_mut() = trail;
    }
}

impl PartialEq for TabMemory {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What a host gives the file view in a window: its tab's memory and what to
/// do with the things the view cannot do itself.
#[derive(Clone, PartialEq)]
pub struct Browse {
    pub(super) memory: TabMemory,
    pub(super) on_location: EventHandler<Where>,
    pub(super) on_new_tab: EventHandler<PathBuf>,
}

impl Browse {
    pub fn new(memory: TabMemory) -> Self {
        Self {
            memory,
            on_location: EventHandler::new(|_: Where| {}),
            on_new_tab: EventHandler::new(|_: PathBuf| {}),
        }
    }

    /// The tab went to another folder (or the Recent place).
    pub fn on_location(mut self, handler: impl Into<EventHandler<Where>>) -> Self {
        self.on_location = handler.into();
        self
    }

    /// A folder was asked for in a tab of its own: a middle click, or
    /// "Open in new tab" in a menu.
    pub fn on_new_tab(mut self, handler: impl Into<EventHandler<PathBuf>>) -> Self {
        self.on_new_tab = handler.into();
        self
    }
}

/// The file view of a window: the chooser's body with no question to answer.
pub fn browse(browse: Browse) -> Element {
    browse_body(browse)
}
