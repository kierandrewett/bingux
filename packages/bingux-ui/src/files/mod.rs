//! Files: the file views and the file chooser dialog of the Bingux SDK.
//!
//! An app that needs a file or a folder calls [`choose`] and gets the
//! chooser the whole desktop uses: a popup on a desktop window, a full sheet
//! on a phone, with the places sidebar (bookmarks and every mounted drive,
//! with its free space), an address bar of crumbs that turns into a text field
//! with completions, a sortable list or a grid of thumbnails, a file type
//! filter, a name field for saving, and keyboard control throughout. The
//! address bar is also the search box: typing a name that is not a path
//! filters the folder, and one more press searches the subfolders (see
//! [`Find`] for the rules).
//!
//! ```ignore
//! use bingux_ui::files::{ChooserRequest, choose};
//!
//! choose(
//!     use_window(),
//!     ChooserRequest::open()
//!         .title("Choose a wallpaper")
//!         // File extensions, globs and MIME types, in one string.
//!         .filter_spec("Pictures", "png jpg image/webp"),
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
//! | Views: [`PlacesSidebar`], [`AddressBar`], [`FileView`] (list and grid), [`use_directory`] | this module | the model, bingux-ui controls |
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

mod actions;
mod address_bar;
mod address_complete;
mod address_crumbs;
mod address_edit;
mod address_list;
mod address_target;
mod address_trail;
mod browser;
mod chooser;
mod columns;
mod context_menu;
mod directory;
mod drives;
mod file_grid;
mod file_list;
mod glyphs;
mod interaction;
mod preview;
mod preview_stage;
mod properties;
mod properties_permissions;
mod places_model;
mod places_view;
mod rename_field;
mod search;
mod skeleton;
mod states;
mod status_line;
mod thumbnails;
mod view;
mod view_state;
mod zoom;

pub use bingux_files as model;
pub use bingux_files::places::user_dir;
pub use bingux_files::{
    Accept, Action, Category, Choice, ChooserRequest, Chosen, Entry, Filter, Kind, Listing, Place,
    PlaceKind, Places, Rule, Selection, Sort, SortKey, ViewMode,
};
pub use address_bar::AddressBar;
pub use search::{Deep, DeepResults, DeepStatus, Find, SearchLink, SearchView, use_subfolder_search};
pub use browser::{Browse, TabMemory, Trail, browse};
pub use chooser::choose;
pub use directory::{Load, Where, home_folder, use_directory, use_places};
pub use glyphs::{entry_icon, folder_icon, place_icon};
pub use places_view::{PlacesMenu, PlacesSidebar};
pub use skeleton::skeleton;
pub use states::{empty_state, error_state, loading_state};
pub use view::FileView;
pub use actions::Capabilities;
pub use view_state::{load as remembered, save as remember};
pub use context_menu::{PlaceEnv, place_menu, place_menu_with};

/// The version of the compiled-in dialog; see the module documentation.
pub const VERSION: u32 = 1;
