//! The toolbar: groups of items that fit the bar they are in.
//!
//! [`toolbar`] lays out [`ToolbarItem`]s in [`ToolbarGroup`]s and keeps them
//! legible at any width:
//!
//! * **Groups and placement.** Items sit in groups, set apart by a wider gap.
//!   A [`Placement`] puts an item before the title (`Leading`, for Back and
//!   Forward), after it (`Primary`) or after it and first to go (`Secondary`).
//!   One item can be [`prominent`](ToolbarItem::prominent): a labelled accent
//!   button at the trailing end that stays. A group can be
//!   [`linked`](ToolbarGroup::linked) into one rounded bar, as [`button_group`] is.
//! * **Overflow.** When the bar is too narrow, items go into a "More" menu,
//!   secondary first, then primary, then leading, the last item of each rank
//!   first, until the rest fit. Every item is also a menu row
//!   ([`ToolbarItem::menu_item`], [`Toolbar::menu_items`]), so the commands stay
//!   reachable and an app's own menu can list the same ones.
//! * **Customising.** With [`customizable`](Toolbar::customizable) and an
//!   [`id`](Toolbar::id), a right click on the bar (or the overflow menu) lists
//!   the items with ticks and a Reset; the choice is saved in the app's state.
//! * **The window's title.** [`Toolbar::title`] and [`Toolbar::subtitle`] are the
//!   window's, and [`Toolbar::header`] draws the whole toolbar as the header bar,
//!   with [`window_controls`](Toolbar::window_controls).
//! * **Keyboard.** The bar's buttons are one Tab stop: Left and Right (Home and
//!   End) move between them, Tab leaves the bar.

mod group;
mod item;
mod layout;
mod toolbar;

pub use group::{ButtonGroup, GroupButton, button_group, group_button};
pub use item::{
    Placement, ToolbarGroup, ToolbarItem, toolbar_element, toolbar_group, toolbar_item,
    toolbar_menu,
};
pub use toolbar::{Toolbar, toolbar};
