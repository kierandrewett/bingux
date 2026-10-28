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
//! [`TabMemory`], where the body keeps the tab's [`NavModel`] (the folder
//! shown and the Back and Forward history, on [`folder_table`]), so a tab
//! that is hidden and shown again, or restored from a session, is where the
//! person left it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use freya::prelude::*;

use super::chooser::browse_body;
use super::directory::Where;
use super::folder_table::{folder_table, location_of};
use crate::nav::NavModel;

/// A tab's navigation history, shared between the host and the body that
/// shows it.
#[derive(Clone)]
pub struct TabMemory {
    id: u64,
    model: Rc<RefCell<NavModel>>,
}

impl TabMemory {
    pub fn new(at: Where) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let model = NavModel::new(Arc::new(folder_table()), &location_of(&at));
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            model: Rc::new(RefCell::new(model)),
        }
    }

    /// A copy of the tab's history now.
    pub(super) fn model(&self) -> NavModel {
        self.model.borrow().clone()
    }

    pub(super) fn set(&self, model: NavModel) {
        *self.model.borrow_mut() = model;
    }

    /// A number no other tab's memory has. The view keys what it remembers
    /// of each folder (where it was scrolled, what was selected) by it, so a
    /// tab shown again comes back to its own place, also when another tab
    /// shows the same folder.
    pub(super) fn id(&self) -> u64 {
        self.id
    }
}

impl PartialEq for TabMemory {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.model, &other.model)
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
