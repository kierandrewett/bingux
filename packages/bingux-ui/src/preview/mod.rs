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
//! | `Pane` | a column at the right edge of the view, which stays usable | stays in the list |
//! | `Fullscreen` | the whole window | the preview's |
//!
//! A compact window (a phone) has no pane: it opens the popup as a
//! full-window sheet.
//!
//! # Keys
//!
//! | Key | Does |
//! | --- | ---- |
//! | Space | close; in the player, play and pause |
//! | Escape | leave full screen, else close |
//! | Left, Right, Up, Down | the previous and next file (Up and Down step a line in a grid) |
//! | Alt with an arrow | the previous and next file, in a viewer that uses the arrows |
//! | Enter | close and open the file |
//! | Alt+Enter, Ctrl+I | the Properties dialog |
//! | I | show or hide the Info panel |
//! | F, F11 | full screen |
//! | Alt+P | dock the popup into a pane, or undock the pane |
//! | `+`, `-`, `0`, `1`, R | pictures: zoom, fit, actual size, rotate |
//! | W | text: wrap lines |

mod chrome;
mod dock;
mod host;
mod info;
mod keys;
mod mode;
mod pane_state;
mod popup;
mod step;
mod subject;
mod viewers;

pub use bingux_preview_core::{PreviewSource, ViewerId};
pub use chrome::PreviewNav;
pub use dock::PreviewDock;
pub use host::PreviewHost;
pub use mode::Mode;
pub use pane_state::{DEFAULT_WIDTH, MIN_WIDTH, MIN_WINDOW};
pub use popup::{card_size, open, show};
pub use step::{Step, neighbour};
