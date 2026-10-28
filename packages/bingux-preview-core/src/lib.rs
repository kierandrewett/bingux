//! Bingux preview core: what a file is, which viewer shows it, and what the
//! viewer reads from it. No UI, no async runtime.
//!
//! `bingux_ui::preview` draws the viewers; this crate decides and reads. An
//! app that has no window (a thumbnailer, a command-line tool) can use it
//! alone. Everything that touches the disk blocks: call it from a worker.
//!
//! | Module | What it holds |
//! | ------ | ------------- |
//! | [`source`] | [`PreviewSource`]: a path, a `file://` address or bytes in memory; never a file-list row |
//! | [`sniff`] | What a source is: the first bytes decide, the name settles what they cannot ([`Sniffed`], [`Content`]) |
//! | [`registry`] | [`Previewer`], [`ViewerId`] and the [`Registry`] that answers "which viewers, best first" |
//! | [`facts`] | The file's own facts: name, type, size, date |
//! | [`text`] | Windowed reading of text in UTF-8, UTF-16 or Windows-1252 |
//! | [`highlight`] | A small line lexer that colours code, markup and data |
//! | [`archive`] | Listing a zip or tar without extracting it |
//! | [`font`] | A font's names and metrics |
//! | [`host`] | The wire format of the decoding helper process (`bingux-preview-host`), version 1 |
//! | [`media`] | A picture's size and camera, a recording's length and codecs, from the header |
//!
//! # Choosing a viewer
//!
//! ```
//! use bingux_preview_core::{PreviewSource, Registry, Sniffed, ViewerId};
//!
//! let source = PreviewSource::bytes("notes.txt", b"hello".to_vec());
//! let sniffed = Sniffed::of(&source).unwrap();
//! let registry = Registry::standard();
//! let chain = registry.chain(&sniffed);
//! assert_eq!(chain.first().map(|previewer| previewer.id()), Some(ViewerId::Text));
//! // The last viewer of every chain is the card: nothing is unpreviewable.
//! assert_eq!(chain.last().map(|previewer| previewer.id()), Some(ViewerId::Card));
//! ```

pub mod archive;
pub mod facts;
pub mod font;
pub mod highlight;
pub mod host;
pub mod media;
pub mod registry;
pub mod sniff;
pub mod source;
pub mod text;

pub use bingux_files::Cancel;
pub use facts::FileFacts;
pub use registry::{LoadError, Loaded, Previewer, Priority, Registry, ViewerId};
pub use sniff::{Content, Sniffed};
pub use source::PreviewSource;
