//! Files: the file views and the file chooser dialog of the Bingux SDK.
//!
//! An app that needs a file or a folder calls [`choose`] and gets the
//! chooser the whole desktop uses: a popup on a desktop window, a full sheet
//! on a phone, with the places sidebar, the path bar, a sortable list or a
//! grid of thumbnails, filters, a name field for saving, and keyboard control
//! throughout.
//!
//! ```ignore
//! use bingux_ui::files::{ChooserRequest, Filter, choose};
//!
//! choose(
//!     use_window(),
//!     ChooserRequest::open()
//!         .title("Choose a wallpaper")
//!         .filter(Filter::new("Pictures").glob("*.png").glob("*.jpg")),
//!     move |chosen| {
//!         if let Some(chosen) = chosen {
//!             println!("{:?}", chosen.paths);
//!         }
//!     },
//! );
//! ```
//!
//! # Layers
//!
//! | Layer | Where | Depends on |
//! | ----- | ----- | ---------- |
//! | Filesystem model, request and answer types, selection and accept logic | the `bingux-files` crate, re-exported here | `libc` only |
//! | Views: [`PlacesSidebar`], [`PathBar`], [`FileList`], [`FileGrid`], [`use_directory`] | this module | the model, bingux-ui controls |
//! | The dialog: [`choose`] | this module | the views |
//!
//! The model has no UI, so the future Files app, a command-line tool and a
//! `bingux-contrib-portals` backend share it. The views are ordinary
//! bingux-ui components and work in any window, not only in the chooser.
//!
//! # Versions
//!
//! [`VERSION`] numbers the dialog's behaviour. It rises when a request option
//! or the dialog's behaviour changes. A system file chooser service, when one
//! is installed, reports its own version; the newer of the two is the one an
//! app should use, and an unreachable service means the compiled-in dialog.
//! See `docs/bingux-ui.md`.

mod chooser;
mod directory;
mod file_grid;
mod file_list;
mod glyphs;
mod path_bar;
mod places_view;
mod states;

pub use bingux_files as model;
pub use bingux_files::places::user_dir;
pub use bingux_files::{
    Accept, Action, Category, Choice, ChooserRequest, Chosen, Entry, Filter, Kind, Listing, Place,
    PlaceKind, Places, Rule, Selection, Sort, SortKey, ViewMode,
};
pub use chooser::choose;
pub use directory::{Load, Where, home_folder, use_directory, use_places};
pub use file_grid::FileGrid;
pub use file_list::FileList;
pub use glyphs::{entry_icon, place_icon};
pub use path_bar::PathBar;
pub use places_view::PlacesSidebar;
pub use states::{empty_state, error_state, loading_state};

/// The version of the compiled-in dialog; see the module documentation.
pub const VERSION: u32 = 1;
