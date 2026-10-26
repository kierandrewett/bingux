//! Bingux files: the filesystem model behind every Bingux file chooser and,
//! later, the `bingux-files` app and a `bingux-portal` backend.
//!
//! This crate has **no UI and no async runtime**: it depends on `libc` only.
//! Everything here is a plain function or type that a worker thread, a CLI, a
//! portal service or a UI can call. `bingux-ui` builds its file views and the
//! chooser dialog (`bingux_ui::files`) on top of it.
//!
//! | Module | What it holds |
//! | ------ | ------------- |
//! | [`entry`] | One directory entry: name, kind, size, modified time, MIME type |
//! | [`mime`] | The MIME database: file name to type, description and category |
//! | [`listing`] | Reading a directory (cancellable), hiding, filtering and sorting it |
//! | [`sort`] | Sort keys, natural name order |
//! | [`filter`] | Portal-style filters: globs and MIME types |
//! | [`places`] | The sidebar: XDG user directories, volumes, GTK bookmarks |
//! | [`recent`] | `recently-used.xbel`, read only |
//! | [`location`] | Typed paths, `~` and `file://` URIs, completion, breadcrumbs, file names |
//! | [`watch`] | An inotify watcher that reports a changed directory |
//! | [`selection`] | Multi-selection, cursor movement and type-ahead |
//! | [`request`] | The chooser request and response, shaped like the portal's |
//! | [`accept`](request::accept) | What pressing Open or Save does for a request |
//! | [`display`] | Sizes and times as people read them |
//!
//! # Reading a folder
//!
//! ```no_run
//! use bingux_files::{Cancel, Sort, View, listing::list_dir};
//!
//! // Run this on a worker: it blocks on the disk.
//! let listing = list_dir(std::path::Path::new("/home"), &Cancel::new())?;
//! let view = View { show_hidden: false, filter: None, directories_only: false, sort: Sort::default() };
//! for index in listing.select(&view) {
//!     println!("{}", listing.entries[index].name);
//! }
//! # Ok::<(), bingux_files::ListError>(())
//! ```

pub mod display;
pub mod entry;
pub mod filter;
pub mod listing;
pub mod location;
pub mod mime;
pub mod places;
pub mod recent;
pub mod request;
pub mod selection;
pub mod sort;
pub mod watch;

pub use entry::{Entry, Kind};
pub use filter::{Filter, Rule};
pub use listing::{Cancel, ListError, Listing, View, list_dir};
pub use mime::{Category, Type};
pub use places::{Place, PlaceKind, Places};
pub use request::{Accept, Action, Choice, ChooserRequest, Chosen, ViewMode};
pub use selection::Selection;
pub use sort::{Sort, SortKey};
