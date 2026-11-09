//! The Bingux text engine: everything a text editor knows that is not
//! drawing. A rope with carets, selections and grouped undo ([`editor`]),
//! where each character is drawn ([`layout`]), finding and replacing
//! ([`search`]), reading and writing encodings and line endings
//! ([`encoding`]), languages and their highlighting ([`language`],
//! [`highlight`]), indentation ([`indent`]), `.editorconfig`
//! ([`editorconfig`]), statistics ([`stats`]) and Markdown as blocks for a
//! preview ([`markdown`]).
//!
//! The editor view is `bingux_ui::text_editor`; the Text Editor app is
//! `apps/text-editor`.

pub mod editor;
pub mod editorconfig;
pub mod encoding;
pub mod highlight;
pub mod history;
pub mod indent;
pub mod language;
pub mod layout;
pub mod markdown;
pub mod search;
pub mod selection;
pub mod stats;

pub use editor::{Case, Copied, Edit, Editor, Settings};
pub use encoding::{FileFormat, LineEnding};
pub use highlight::{Highlighter, TokenKind};
pub use indent::Indent;
pub use language::Language;
pub use ropey::Rope;
pub use search::{Matches, Query};
pub use selection::{Range, Selection};
