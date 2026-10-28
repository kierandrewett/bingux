//! The Back and Forward buttons with their history menus.
//!
//! A right click, or a hold of 450 ms (primary button or finger), opens a menu
//! under the button listing the path the button would take, nearest first. A
//! choice is one `pop_to` (or `forward_to`): a multi-level move is one
//! transition. The tooltip stays for hover and is closed by the menu.
//!
//! [`BackButton`] and [`ForwardButton`] read the window's router; an app that
//! keeps its own [`NavModel`](crate::nav::NavModel) (the file chooser) builds
//! the same menu with [`history_menu`].

use freya::prelude::*;

use crate::controls::{IconButton, icon_button};
use crate::icons::Icon;
use crate::nav::{HistoryItem, use_router};
use crate::shell::menu::{MenuItem, menu_item};

/// The rows of a history menu: one per item, in the order given (nearest
/// first), each with its route icon when it has one. `on_pick` receives the
/// entry id of the chosen row.
pub fn history_menu(
    items: Vec<HistoryItem>,
    on_pick: impl Fn(u64) + Clone + 'static,
) -> Vec<MenuItem> {
    items
        .into_iter()
        .map(|item| {
            let pick = on_pick.clone();
            let entry = item.entry;
            let row = menu_item(item.title, move |_| pick(entry));
            match item.icon {
                Some(icon) => row.icon(icon),
                None => row,
            }
        })
        .collect()
}

/// The window's Back button. Show it only when
/// [`Router::can_go_back`](crate::nav::Router::can_go_back): open menus and
/// dialogs never make it appear.
#[derive(PartialEq)]
pub struct BackButton;

impl Component for BackButton {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let items = history_menu(router.back_menu(), move |entry| router.pop_to(entry));
        icon_button(Icon::GoPrevious, move |_| router.back())
            .tooltip("Back")
            .context_menu(items)
    }
}

/// The window's Forward button, for an app that keeps forward history. It is
/// dimmed when there is nowhere forward.
#[derive(PartialEq)]
pub struct ForwardButton;

impl Component for ForwardButton {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let items = history_menu(router.forward_menu(), move |entry| router.forward_to(entry));
        let button: IconButton = icon_button(Icon::GoNext, move |_| router.forward())
            .tooltip("Forward")
            .enabled(router.can_forward())
            .context_menu(items);
        button
    }
}
