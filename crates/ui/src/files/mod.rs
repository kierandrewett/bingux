//! Files: the file views and the file chooser dialog of the Bingux SDK.
//!
//! An app that needs a file or a folder calls [`choose`] and gets the
//! chooser the whole desktop uses: a popup on a desktop window, a full sheet
//! on a phone, with the places sidebar (bookmarks and every mounted drive,
//! with its free space), an address bar of crumbs that turns into a text field
//! with completions, a sortable list or a grid of thumbnails, a file type
//! filter, a name field for saving, and keyboard control throughout. The
//! search bar under the toolbar (Ctrl+F) filters the folder as you type or
//! searches subfolders, Home or everywhere, with filters for type, date and
//! size (see [`Find`] and `design/files-search.md`).
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
//! | The dialog: [`choose`], and the same body in a window: [`browse`] | this module | the views |
//!
//! Every file, folder, place and drive icon any of them draws comes from one
//! description, [`FileIcon`], drawn by one element, [`file_icon_view`]; see
//! the `file_icon` module for the rules (a folder is decided by the text of
//! its path, a link has an emblem, the accent marks the selected row).
//!
//! The model has no UI, so the Files app, a command-line tool and a
//! `bingux-contrib-portals` backend share it. The views are ordinary
//! bingux-ui components and work in any window, not only in the chooser:
//! [`browse`] is the dialog's body hosted in an application window, which is
//! how the Files app (`apps/files`) is built.
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
mod colour;
mod columns;
mod compress;
mod compress_media;
mod compress_model;
mod compress_run;
#[cfg(all(test, feature = "testing"))]
mod compress_tests;
mod context_menu;
mod extract_dialog;
mod directory;
mod drag;
mod drives;
mod file_grid;
mod file_list;
mod folder_table;
mod named;
pub mod history;
#[cfg(test)]
mod history_fuzz;
#[cfg(all(test, feature = "testing"))]
mod history_ui_tests;
mod file_icon;
mod hover_card;
mod interaction;
mod preview;
mod properties;
mod properties_access;
mod properties_basic;
mod properties_contents;
mod properties_details;
mod properties_permissions;
mod properties_tabs;
mod properties_words;
mod place_menu;
#[cfg(all(test, feature = "testing"))]
mod place_tests;
mod places_model;
mod places_view;
mod reload_button;
mod rename_field;
mod scroll_memory;
#[cfg(all(test, feature = "testing"))]
mod reload_stop_tests;
#[cfg(all(test, feature = "testing"))]
mod colour_tests;
#[cfg(all(test, feature = "testing"))]
mod file_icon_surfaces;
#[cfg(all(test, feature = "testing"))]
mod scroll_tests;
#[cfg(all(test, feature = "testing"))]
mod preview_step_tests;
#[cfg(all(test, feature = "testing"))]
mod tab_tests;
#[cfg(all(test, feature = "testing"))]
mod hover_tests;
#[cfg(all(test, feature = "testing"))]
mod address_tests;
mod search;
mod search_bar;
mod search_filters;
#[cfg(all(test, feature = "testing"))]
mod search_tests;
mod skeleton;
mod states;
#[cfg(test)]
mod test_state;
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
pub use search::{Deep, DeepResults, DeepStatus, Find, Marks, RunKey, SearchView, use_search_run};
pub use search_bar::{BarStatus, ScopeChoice, SearchBar, place_phrase, scope_choices, scope_key, status_text, token_label};
pub use actions::operations_running;
pub use browser::{Browse, ClosedTab, ClosedTabs, TabMemory, browse};
pub use chooser::choose;
pub use compress::{CompressPreview, compress_preview};
pub use directory::{Load, Where, home_folder, use_directory, use_places};
pub use folder_table::{APP_SCHEME, FOLDER_SCHEME, folder_table, location_of, place_of};
pub use named::{RECENT_SCHEME, Root, Roots, TRASH_SCHEME, Typed};
pub use colour::{COLOUR_GRIDS, ColourIcon, PIXEL_GRIDS, Tone, VECTOR_GRID, colour_grid, colour_icon_view, colour_tone};
pub use file_icon::{
    Emblem, FileIcon, FileIconSize, FileKind, Source, Tint, file_icon_view, file_icon_view_floating,
};
pub use places_view::{PlacesMenu, PlacesSidebar};
pub use skeleton::skeleton;
pub use zoom::default_zoom_px;
pub use states::{empty_state, error_state, loading_state, no_results_state};
pub use view::FileView;
pub use actions::Capabilities;
pub use drag::{FileDrag, file_drag, provide_file_drag};
pub use view_state::{load as remembered, save as remember};

/// The version of the compiled-in dialog; see the module documentation.
pub const VERSION: u32 = 1;
