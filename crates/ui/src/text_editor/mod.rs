//! A multi-line text editor: SwiftUI's `TextEditor` with the code
//! features a programmer expects. The text engine is `bingux-text` (a rope,
//! carets, undo, search, encodings, languages); this module draws it and
//! turns keys and pointer presses into its commands.
//!
//! | Item | What it is |
//! | ---- | ---------- |
//! | [`EditorHandle`], [`use_editor`] | One document being edited, shared by the views that show it |
//! | [`text_editor`] / [`TextEditorView`] | The editing surface: line numbers, wrap, highlighting, carets, find hits |
//! | [`FindBar`], [`FindState`] | Find and replace over a handle (regex, case, whole word, match count) |
//! | [`GoToLineBar`] | Go to a line and column |
//! | [`markdown_view`] / [`MarkdownView`] | Markdown drawn with the type ramp, for a preview |
//! | [`keymap`] | The keys and the commands they run |
//! | [`SyntaxPalette`] | The colours of highlighted code for the current tokens |
//!
//! The view draws only the rows on screen and lays a line out only when it
//! is shown, so a 100 MB file scrolls like a short one; highlighting is
//! lazy too (see `bingux_text::highlight`). Its scroll position, its
//! highlighting and the caret it last followed live in the handle, so a
//! document shown again in a tab comes back exactly where it was.
//!
//! ```ignore
//! let doc = use_editor(|| Editor::new("hello"));
//! text_editor(doc.clone()).line_numbers(true).wrap(false)
//! ```

mod find;
pub mod keymap;
mod markdown;
mod palette;
mod view;

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use freya::prelude::*;

use bingux_text::{Editor, Highlighter, Selection};

pub use find::{
    FindBar, FindState, GoToLineBar, find_bar, go_to_line_bar, parse_line_target, use_find,
};
pub use keymap::{Command, Effect, ZoomStep};
pub use markdown::{MarkdownView, markdown_view};
pub use palette::{Ink as SyntaxInk, SyntaxPalette};
pub use view::{HIGHLIGHT_LIMIT, TextEditorView, caret_caption, status_caption, text_editor};

/// Where a view of a document is scrolled to: the line and wrapped row at
/// the top, how far that row is scrolled past (px), and the horizontal
/// scroll (px) when lines do not wrap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollPos {
    pub line: usize,
    pub row: usize,
    pub offset: f32,
    pub x: f32,
}

/// What a view remembers about a document between frames and between the
/// times it is on show.
pub(crate) struct ViewMemory {
    pub(crate) scroll: ScrollPos,
    /// The revision and selection the view last scrolled to show.
    pub(crate) followed: Option<(u64, Selection)>,
    pub(crate) highlight: Option<HighlightCache>,
    /// Lines on screen at the last frame, for a preview that follows.
    pub(crate) shown: (usize, usize),
}

pub(crate) struct HighlightCache {
    pub(crate) highlighter: Highlighter,
    pub(crate) language: String,
    pub(crate) revision: u64,
}

/// One document being edited: the engine's [`Editor`], a signal that
/// re-renders the views reading it, and what those views remember (scroll
/// position, highlighting). Cheap to clone; clones are the same document.
///
/// A window that edits several documents may give them all one signal (a
/// `State<u64>` of its own): only the document on show is drawn, so one
/// signal costs nothing and nothing is left behind when a document closes.
#[derive(Clone)]
pub struct EditorHandle {
    editor: Rc<RefCell<Editor>>,
    signal: State<u64>,
    memory: Rc<RefCell<ViewMemory>>,
}

impl PartialEq for EditorHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.editor, &other.editor) && self.signal == other.signal
    }
}

impl EditorHandle {
    /// A handle on `editor` that re-renders through `signal`.
    pub fn new(editor: Editor, signal: State<u64>) -> Self {
        Self {
            editor: Rc::new(RefCell::new(editor)),
            signal,
            memory: Rc::new(RefCell::new(ViewMemory {
                scroll: ScrollPos::default(),
                followed: None,
                highlight: None,
                shown: (0, 0),
            })),
        }
    }

    /// The editor, subscribing the component that reads it to its changes.
    pub fn read(&self) -> Ref<'_, Editor> {
        let _ = *self.signal.read();
        self.editor.borrow()
    }

    /// The editor, without subscribing (in an event handler).
    pub fn peek(&self) -> Ref<'_, Editor> {
        self.editor.borrow()
    }

    /// Changes the editor and re-renders its views.
    pub fn update<R>(&self, edit: impl FnOnce(&mut Editor) -> R) -> R {
        let result = edit(&mut self.editor.borrow_mut());
        let mut signal = self.signal;
        *signal.write() += 1;
        result
    }

    /// Re-renders the views without a change to the text: a new scroll
    /// position, say.
    pub fn refresh(&self) {
        let mut signal = self.signal;
        *signal.write() += 1;
    }

    /// Whether `other` is the same document.
    pub fn same(&self, other: &EditorHandle) -> bool {
        Rc::ptr_eq(&self.editor, &other.editor)
    }

    /// Where the view is scrolled to.
    pub fn scroll(&self) -> ScrollPos {
        self.memory.borrow().scroll
    }

    /// Scrolls the view: a document restored with its scroll position.
    pub fn set_scroll(&self, scroll: ScrollPos) {
        let mut memory = self.memory.borrow_mut();
        memory.scroll = scroll;
        // Do not jump back to the caret on the next frame.
        let editor = self.editor.borrow();
        memory.followed = Some((editor.revision(), editor.selection().clone()));
        drop(memory);
        drop(editor);
        self.refresh();
    }

    /// The first and last line on screen at the last frame.
    pub fn shown_lines(&self) -> (usize, usize) {
        self.memory.borrow().shown
    }

    pub(crate) fn memory(&self) -> RefMut<'_, ViewMemory> {
        self.memory.borrow_mut()
    }
}

/// A document of the component's own, made by `init` once.
#[track_caller]
pub fn use_editor(init: impl FnOnce() -> Editor) -> EditorHandle {
    let signal = use_state(|| 0u64);
    use_hook(move || EditorHandle::new(init(), signal))
}
