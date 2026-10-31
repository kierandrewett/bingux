//! Tabs in a window, as an application uses them: the set of tabs, what a
//! person does with them, and moving them between windows.
//!
//! The strip a tab is drawn in is [`controls::tab_strip`](crate::controls::tab_strip);
//! it holds no state. Everything behind it is here, and is the same for every
//! application that has tabs (Files, an editor with a document to a tab):
//!
//! | Module | What it holds |
//! | ------ | ------------- |
//! | `set` | [`TabSet`]: the tabs as plain data (open beside the one on show, close and who is selected after, reopen, duplicate, reorder, take out for another window and put in), tested without a window |
//! | `actions` | [`Tabs`]: the set put on screen. Handlers for the strip, the keys (Ctrl+T, Ctrl+W, Ctrl+Tab, Alt+1 to 9) and the menu entries; the last tab closes the window |
//! | `windows` | [`use_tab_window`]: the windows of the running application know each other, so a tab can go from one to another; [`Birth`], the tabs a new window is born with |
//! | `tearoff` | a tab torn off its strip, the window that follows the pointer, and the drop on another window's strip (`design/tab-tearoff.md`) |
//!
//! The compositor questions a tear-off asks ("where are the windows?") are
//! [`window::compositor`](crate::window).
//!
//! What a tab *shows* is the application's: a [`TabContent`]. The set asks it
//! for a copy (Duplicate tab) and tells it when it leaves for another window;
//! the rest, a folder and its history for Files, it never looks at.
//!
//! ```ignore
//! let set = use_state(|| TabSet::new(MyTab::new()));
//! let tabs = Tabs::new(set, use_window(), use_overlays()).guard(unsaved_work);
//! let window = use_tab_window(set, &title_of_the_tab_on_show);
//! let strip = tabs.strip(items, &window, move |_: ()| tabs.open(MyTab::new()));
//! ```

mod actions;
mod set;
mod tearoff;
mod windows;

pub use actions::{CloseWarning, Tabs, tab_items};
pub use set::{CLOSED_TABS_KEPT, Closed, ClosedTab, ClosedTabs, Moved, Tab, TabContent, TabSet};
pub use windows::{Birth, GHOST_OPACITY, TabWindow, use_tab_window};
