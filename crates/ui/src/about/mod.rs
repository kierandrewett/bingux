//! The About window every Bingux application has.
//!
//! An application says who it is once, at launch:
//!
//! ```ignore
//! Application::new("bingux-files")
//!     .about(bingux_ui::app_info!(name = "Files", licence_text = include_str!("../../../COPYING")))
//!     .run(|| app::App);
//! ```
//!
//! [`app_info!`](crate::app_info) reads `Cargo.toml` (version, description,
//! homepage, repository, licence, authors) and the build's
//! `BINGUX_BUILD_COMMIT` and `BINGUX_BUILD_DATE`. Everything after that is a
//! reachable About window, made by [`Application::run`](crate::app::Application::run):
//!
//! * the application's main menu ends with **About <name>**, from
//!   [`app_menu_items`], or is [`app_menu`] when it has no menu of its own;
//! * the command palette (Ctrl+Shift+P) has **About <name>**, as the
//!   window's `app.about` command (see [`crate::commands`]);
//! * [`open`] opens it from code.
//!
//! It has no shortcut of its own: Ctrl+? is the keyboard shortcuts sheet, as
//! in GNOME.
//!
//! An application that never calls `about` still has the window, with the
//! name it launched with and no version; `scripts/ui-lint.py` fails an app
//! crate whose `main.rs` does not call it.
//!
//! # The window
//!
//! It is a [`framed_dialog`](crate::shell::framed_dialog) card of one size, a
//! popup over a wide window and the same card on a phone. The main page has the
//! application icon, name, version line and description; the website and the
//! issue tracker as links (opened with `xdg-open`); the licence (SPDX name and
//! the text on its page), Credits (authors, and the Rust, Freya and bingux-ui
//! versions) and Legal (the libraries linked in and their licences, from the
//! application's `third_party` text or the `<id>.LICENSE.dependencies` file its
//! package installed); the application id and build; and **Copy details**.
//!
//! # Copy details
//!
//! The clipboard gets [`details_text`]: the name and version, the app id,
//! the commit and build date, the licence, the Rust, Freya and bingux-ui
//! versions, the OS, kernel and session, the display scale, the graphics
//! driver, and the theme and density of the window. It is plain text for a bug
//! report, with one fact on a line and no line for a fact that is not known.

mod details;
mod info;
mod link;
mod view;

pub use details::{Environment, Facts, details_text};
pub use info::{AppInfo, CargoMeta, info, registered};
pub use view::{AboutBody, Page, open_at, open_with};

pub(crate) use details::read_facts;
pub(crate) use info::register;

use freya::prelude::*;

use crate::icons::Icon;
use crate::shell::menu::{MenuItem, menu_button, menu_item, menu_separator};
use crate::shell::overlays::{Overlays, use_overlays};

/// Open the application's About window in the window `overlays` belongs to.
/// Call it from an event handler.
pub fn open(overlays: Overlays) {
    open_with(overlays, info());
}

/// The items every application's main menu ends with: a separator and
/// **About <name>**. Append them after the app's own items:
///
/// ```ignore
/// let mut items = vec![menu_item("New tab", ..)];
/// items.extend(bingux_ui::about::app_menu_items(overlays));
/// ```
pub fn app_menu_items(overlays: Overlays) -> Vec<MenuItem> {
    vec![
        menu_separator(),
        menu_item(
            crate::tr!("ui-about-title", app = info().name),
            move |_: ()| open(overlays),
        ),
    ]
}

/// The main menu of an application that has no menu of its own: the menu
/// button of a header bar (the hamburger), holding `items` and then About.
/// Place it with [`NavigationSplitView::header_trailing`]
/// (crate::shell::NavigationSplitView::header_trailing).
pub fn app_menu(items: Vec<MenuItem>) -> AppMenu {
    AppMenu { items }
}

/// See [`app_menu`].
#[derive(PartialEq)]
pub struct AppMenu {
    items: Vec<MenuItem>,
}

impl Component for AppMenu {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let commands = crate::commands::use_commands();
        let own = self.items.clone();
        // The window's commands are read when the menu opens, so a title
        // such as "Undo rename" is the one that applies then.
        menu_button(crate::tr!("ui-main-menu"), Vec::new())
            .icon(Icon::OpenMenu)
            .items_on_open(move || main_menu(own.clone(), commands.main_menu_items(), overlays))
    }
}

/// The main menu: the app's own items, then the window's commands, then
/// About, with a separator between each part that has items.
fn main_menu(own: Vec<MenuItem>, commands: Vec<MenuItem>, overlays: Overlays) -> Vec<MenuItem> {
    let mut items = own;
    if !commands.is_empty() {
        if !items.is_empty() {
            items.push(menu_separator());
        }
        items.extend(commands);
    }
    let mut about = app_menu_items(overlays);
    if items.is_empty() {
        // Nothing before it to separate from.
        about.remove(0);
    }
    items.extend(about);
    items
}

#[cfg(all(test, feature = "testing"))]
mod tests;
