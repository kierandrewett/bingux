//! Dragging files out of the file view onto other parts of the window: a
//! place in the sidebar, a tab.
//!
//! The view that starts a drag keeps the files being dragged and, when the
//! pointer is released, moves or copies them (Ctrl copies) into the folder
//! the pointer is over. It cannot see what is outside itself, so a window
//! that has drop targets elsewhere shares one [`FileDrag`]: the view says a
//! drag is on, a target says which folder the pointer is over, and the view
//! reads it back when the pointer goes up. The sidebar of the file view
//! is a target already. A host with more (the tabs of the Files app) calls
//! [`provide_file_drag`] once, above both, and sets [`FileDrag::hover`] from
//! its own targets.
//!
//! Without a provided [`FileDrag`] a drag only reaches the folders of its own
//! list, as in the chooser.

use std::path::{Path, PathBuf};

use freya::prelude::*;

/// The files being dragged in this window and the folder under the pointer.
/// `Copy`, so each handler takes its own.
#[derive(Clone, Copy, PartialEq)]
pub struct FileDrag {
    active: State<bool>,
    over: State<Option<PathBuf>>,
}

/// Provide the window's [`FileDrag`], from a component above every target
/// and every file view. Returns it for the host's own targets.
pub fn provide_file_drag() -> FileDrag {
    use_provide_root_context(|| FileDrag {
        active: State::create(false),
        over: State::create(None),
    })
}

/// The window's [`FileDrag`], if one was provided.
pub fn file_drag() -> Option<FileDrag> {
    try_consume_root_context::<FileDrag>()
}

impl FileDrag {
    /// Whether files are being dragged. Reactive.
    pub fn active(&self) -> bool {
        *self.active.read()
    }

    /// Whether the pointer is over `folder` with files in hand. Reactive.
    pub fn is_over(&self, folder: &Path) -> bool {
        *self.active.read() && self.over.read().as_deref() == Some(folder)
    }

    /// The pointer entered a target that takes files into `folder`. Nothing
    /// happens unless a drag is on.
    pub fn hover(mut self, folder: &Path) {
        if *self.active.peek() {
            self.over.set_if_modified(Some(folder.to_path_buf()));
        }
    }

    /// The pointer left the target of `folder`.
    pub fn leave(mut self, folder: &Path) {
        if self.over.peek().as_deref() == Some(folder) {
            self.over.set(None);
        }
    }

    /// The view started a drag.
    pub(super) fn begin(mut self) {
        self.over.set_if_modified(None);
        self.active.set_if_modified(true);
    }

    /// The drag is over, dropped or cancelled. Returns the folder it was
    /// dropped on, if any.
    pub(super) fn end(mut self) -> Option<PathBuf> {
        self.active.set_if_modified(false);
        let over = self.over.peek().clone();
        self.over.set_if_modified(None);
        over
    }
}
