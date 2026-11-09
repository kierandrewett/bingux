//! Documents: files that a person opens, edits and saves.
//!
//! The app holds its content; this module holds the file. An app that edits
//! or views files makes a [`Documents`] with [`use_documents`] in its window
//! root, and gets:
//!
//! - open (the Bingux file chooser, recents, a path, a second launch), save,
//!   save as, close, reload, with the questions a person expects asked at the
//!   right moments ([`dialogs`]);
//! - the unsaved mark in the tab and the window title ([`UNSAVED_MARK`],
//!   [`Document::display_title`], [`use_document_title`]);
//! - saves that never leave half a file ([`write_atomic`]) and say why one
//!   failed;
//! - drafts of unsaved work, and their recovery after a crash ([`Drafts`]);
//! - a file that another program changed, replaced or deleted noticed
//!   ([`FileWatch`], [`Stamp`]);
//! - the list of recent documents shared with Files and the file chooser
//!   ([`recents`]);
//! - the scene: window size, open documents, scroll and a value of the
//!   app's per document, brought back on the next start ([`SceneState`]);
//! - the close guard: closing a window that holds unsaved work asks first,
//!   whichever way the window is closed.
//!
//! The building blocks are public too, for an app whose flow is not a
//! document in tabs: a viewer that watches the file it shows ([`FileWatch`]),
//! a tool that saves one file ([`write_atomic`]).
//!
//! ```ignore
//! let documents = use_documents(
//!     DocumentsConfig::new("bingux-text-editor", "Text Editor").mime("text/plain"),
//!     |init| Rc::new(EditorContent::new(init)),
//!     move |event| match event {
//!         DocumentEvent::Opened(document) => tabs.open(Tab(document.id())),
//!         DocumentEvent::Reveal(document) => tabs.select(document.id()),
//!         DocumentEvent::Closed(id) => tabs.close(id),
//!     },
//! );
//! use_document_title(documents, "Text Editor");
//! ```

pub mod dialogs;
mod documents;
mod drafts;
pub(crate) mod guard;
mod io;
pub mod recents;
mod scene;
mod stamp;
mod watch;
mod write;

pub use documents::{
    Document, DocumentContent, DocumentEvent, DocumentId, DocumentInit, DocumentSnapshot,
    Documents, DocumentsConfig, LoadReason, Origin, Phase, Prepared, Preparer, UNSAVED_MARK,
    use_document_title, use_documents,
};
pub use drafts::{Draft, DraftMeta, Drafts};
pub use io::OpenError;
pub use scene::{SceneDocument, SceneState, use_scene_state};
pub(crate) use scene::{loaded_window_size, preload as preload_scene};
pub use stamp::Stamp;
pub use watch::{FileChange, FileWatch};
pub use write::{SaveError, write_atomic};
