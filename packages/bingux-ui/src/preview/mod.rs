//! Previewing files: one content, three frames, a viewer for every kind.
//!
//! `bingux_preview_core` decides what a source is and which viewer shows it;
//! this module draws it. The design, the options compared and what is not
//! done are in `design/preview.md`.
//!
//! ```ignore
//! use bingux_ui::preview::{PreviewSource, open};
//!
//! // A popup over the window: Escape closes it.
//! open(use_overlays(), PreviewSource::path("/home/me/Pictures/aurora.png"));
//! ```
//!
//! An app that has a list of files and wants the selection to follow wraps
//! [`PreviewHost`] itself (the file view does) and gives it a [`PreviewNav`]
//! with what its buttons do; a docked pane is [`PreviewDock`].
//!
//! # Frames
//!
//! | [`Mode`] | Where | Keyboard |
//! | -------- | ----- | -------- |
//! | `Popup` | a card over the window | the preview's |
//! | `Pane` | a column at the right edge of the view, which stays usable | stays in the list until the pane is clicked; the arrows choose the file either way |
//! | `Fullscreen` | the whole window | the preview's |
//!
//! A compact window (a phone) has no pane: it opens the popup as a
//! full-window sheet.
//!
//! # Keys
//!
//! The arrows choose the file, everywhere: in a popup, in a docked pane and
//! in every viewer. They never scroll, seek or zoom. A document scrolls with
//! the wheel, Page Up, Page Down, Home and End; a film seeks with Shift and an
//! arrow.
//!
//! | Key | Does |
//! | --- | ---- |
//! | Space, Escape | close a popup (Space is a pane's page down or play and pause, once it has been clicked; Escape gives the pane's keys back to the list) |
//! | Left, Right, Up, Down | the previous and next file (Up and Down step a line in a grid) |
//! | Alt with an arrow | the same |
//! | Enter | close and open the file |
//! | Alt+Enter, Ctrl+I | the Properties dialog |
//! | I | show or hide the Info panel |
//! | F, F11 | full screen |
//! | Alt+P | dock the popup into a pane, or undock the pane |
//! | `+`, `-`, `0`, `1`, R | pictures: zoom, fit, actual size, rotate |
//! | `+`, `-`, `0`, `1`, `9`, Page Up, Page Down, Ctrl+F | documents: zoom, fit width, 100 %, fit page, scroll, find |
//! | K, J, L, M, Shift with an arrow | films and songs: play and pause, seek, mute |
//! | W | text: wrap lines |

mod chrome;
mod dock;
mod host;
mod host_link;
mod info;
mod keys;
mod mode;
mod pane_state;
mod popup;
#[cfg(all(test, feature = "testing"))]
mod behaviour_tests;
#[cfg(all(test, feature = "testing"))]
mod render_tests;
mod step;
mod subject;
mod viewers;

pub use bingux_preview_core::{PreviewSource, ViewerId};
pub use chrome::PreviewNav;
pub use dock::PreviewDock;
pub use host::PreviewHost;
pub use host_link::{allow_helpers, host_available};
pub use mode::Mode;
pub use pane_state::{DEFAULT_WIDTH, MIN_WIDTH, MIN_WINDOW};
pub use popup::{card_size, open, show};
pub use step::{Step, neighbour};
pub use viewers::{ControlAction, ControlState, DocumentAction, DocumentControls, DocumentState, MediaControls, ZoomChoice};
