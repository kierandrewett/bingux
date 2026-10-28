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
//! * the shortcut **Ctrl+Shift+?** opens it in the main window (see
//!   [`is_shortcut`]);
//! * the application's main menu ends with **About <name>**, from
//!   [`app_menu_items`], or is [`app_menu`] when it has no menu of its own;
//! * [`open`] opens it from code, and the headless runner's script command
//!   `key ctrl+shift+?` does the same, so a test reaches it as a person does.
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

/// The shortcut text menus show next to About.
pub const SHORTCUT: &str = "Ctrl+Shift+?";

/// Open the application's About window in the window `overlays` belongs to.
/// Call it from an event handler.
pub fn open(overlays: Overlays) {
    open_with(overlays, info());
}

/// Whether `event` is the About shortcut: Ctrl+Shift+? (the key that types `?`
/// with Shift held, whichever layout puts it there, or `/` for a layout that
/// reports the unshifted key). Alt and Meta are not held.
pub fn is_shortcut(event: &KeyboardEventData) -> bool {
    let modifiers = event.modifiers;
    modifiers.contains(Modifiers::CONTROL)
        && modifiers.contains(Modifiers::SHIFT)
        && !modifiers.contains(Modifiers::ALT)
        && !modifiers.contains(Modifiers::META)
        && matches!(&event.key, Key::Character(text) if text == "?" || text == "/")
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
        menu_item(format!("About {}", info().name), move |_: ()| open(overlays)).shortcut(SHORTCUT),
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
        let mut items = self.items.clone();
        let mut about = app_menu_items(overlays);
        if items.is_empty() {
            // Nothing before it to separate from.
            about.remove(0);
        }
        items.extend(about);
        menu_button("Main menu", items).icon(Icon::OpenMenu)
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests;
