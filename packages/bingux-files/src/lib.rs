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
//! | [`entry`] | One directory entry: name, kind, size, times, owner, MIME type, special-folder icon |
//! | [`mime`] | The MIME database: file name to type, description and category |
//! | [`listing`] | A folder's entries, hiding, filtering and sorting them, following loader events |
//! | [`loader`] | Opening a folder: streaming, cancellable, cached, per-drive policy, never blocks |
//! | [`cache`] | The small persistent cache of folder listings (stale while revalidate) |
//! | [`mounts`] | Mounted drives, their kind and size, change notification, whether writes there must be flushed (removable media, shares, FAT), unmount and eject (which write the drive out first) |
//! | [`stat`] | The one `statx` call the model makes |
//! | [`sort`] | Sort keys, natural name order, sorting by precomputed byte keys |
//! | [`filter`] | Portal-style filters: globs, extensions, MIME types, presets |
//! | [`places`] | The sidebar: XDG user directories, GTK bookmarks, which folders get a special icon |
//! | [`recent`] | `recently-used.xbel`, read only |
//! | [`location`] | Typed paths, `~` and `file://` URIs, completion, breadcrumbs, file names |
//! | [`watch`] | An inotify watcher that reports a changed directory |
//! | [`search`] | Path or search for a typed text, name matching and ranking, the subfolder walker |
//! | [`selection`] | Multi-selection, cursor movement and type-ahead |
//! | [`request`] | The chooser request and response, shaped like the portal's |
//! | [`accept`](request::accept) | What pressing Open or Save does for a request |
//! | [`display`] | Sizes and times as people read them |
//! | [`ops`] | Rename, create, bin, delete, copy, move, compress, extract and properties (blocking, cancellable; atomic renames, durable on removable media, nothing removed before it is safe) |
//! | [`thumbs`] | Looking up thumbnails the desktop has already made |
//!
//! # Reading a folder
//!
//! A view opens a folder with the [`loader`] and keeps a [`Listing`] up to
//! date from its events. The loader shows the cache first, streams the rest,
//! and reads sizes only for the rows in view on a network drive.
//!
//! ```no_run
//! use bingux_files::loader::{self, Event, LoadOptions};
//! use bingux_files::{Listing, Sort, View};
//!
//! let (sender, events) = std::sync::mpsc::channel();
//! let path = std::path::Path::new("/home");
//! let load = loader::open(path, LoadOptions::default(), move |event| {
//!     let _ = sender.send(event);
//! });
//! let mut listing = Listing::new(path);
//! let view = View { show_hidden: false, filter: None, directories_only: false, sort: Sort::default() };
//! while let Ok(event) = events.recv() {
//!     let done = matches!(event, Event::Done { .. });
//!     if listing.apply_event(event) {
//!         let shown = listing.select(&view); // sort and filter again
//!         // draw `shown`, then ask for the sizes of the rows in view:
//!         load.want_stats(&listing, &shown[..shown.len().min(40)]);
//!     }
//!     if done {
//!         break;
//!     }
//! }
//! ```
//!
//! [`listing::list_dir`] reads a whole folder at once, with no cache; use it
//! from scripts and tests, not from a view.

pub mod cache;
pub mod display;
pub mod entry;
pub mod filter;
pub mod listing;
pub mod loader;
pub mod location;
pub mod mime;
pub mod mounts;
pub mod ops;
pub mod places;
pub mod recent;
pub mod request;
pub mod search;
pub mod selection;
pub mod sort;
pub mod stat;
pub mod thumbs;
pub mod volumes;
pub mod watch;

pub use entry::{Entry, Kind};
pub use filter::{Filter, Rule};
pub use listing::{Cancel, ListError, Listing, View, list_dir};
pub use mime::{Category, Type};
pub use places::{Place, PlaceKind, Places};
pub use request::{Accept, Action, Choice, ChooserRequest, Chosen, ViewMode};
pub use selection::Selection;
pub use sort::{Sort, SortKey};
