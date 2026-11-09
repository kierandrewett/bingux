//! What a toolbar is made of: items, in groups.

use freya::prelude::*;

use crate::icons::Icon;
use crate::shell::{MenuItem, menu_item, submenu};

/// Where an item sits, and how soon it goes into the overflow menu when the bar
/// is too narrow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// Before the title, for the way around the app: Back and Forward, a
    /// sidebar button. The last to go into the overflow menu.
    Leading,
    /// After the title: what the window is for. The default.
    #[default]
    Primary,
    /// After the title, and the first to go into the overflow menu.
    Secondary,
}

#[derive(Clone, PartialEq)]
pub(super) enum Action {
    /// A button that runs `on_press`; `toggled` makes it a toggle that is on or off.
    Press {
        on_press: EventHandler<()>,
        toggled: Option<bool>,
    },
    /// A button that opens a menu.
    Menu(Vec<MenuItem>),
    /// An element of the app's own (a search field), `width` px wide. It cannot
    /// go into a menu, so it never leaves the bar.
    Element { element: Element, width: f32 },
}

/// One item of a toolbar. Build it with [`toolbar_item`], [`toolbar_menu`] or
/// [`toolbar_element`].
#[derive(Clone, PartialEq)]
pub struct ToolbarItem {
    pub(super) id: &'static str,
    pub(super) label: String,
    pub(super) icon: Option<Icon>,
    pub(super) action: Action,
    pub(super) enabled: bool,
    pub(super) placement: Placement,
    pub(super) prominent: bool,
    pub(super) shortcut: Option<String>,
}

/// A button. `id` names the item in the saved customisation, so it never
/// changes once shipped; `label` is its tooltip, its accessible name and its row
/// in the overflow menu: a verb ("Add", "Share") or a noun for a toggle
/// ("Sidebar").
pub fn toolbar_item(
    id: &'static str,
    label: impl Into<String>,
    icon: Icon,
    on_press: impl Into<EventHandler<()>>,
) -> ToolbarItem {
    ToolbarItem {
        id,
        label: label.into(),
        icon: Some(icon),
        action: Action::Press {
            on_press: on_press.into(),
            toggled: None,
        },
        enabled: true,
        placement: Placement::Primary,
        prominent: false,
        shortcut: None,
    }
}

/// A button that opens a menu of `items` under itself. In the overflow menu it
/// is a submenu.
pub fn toolbar_menu(
    id: &'static str,
    label: impl Into<String>,
    icon: Icon,
    items: Vec<MenuItem>,
) -> ToolbarItem {
    ToolbarItem {
        id,
        label: label.into(),
        icon: Some(icon),
        action: Action::Menu(items),
        enabled: true,
        placement: Placement::Primary,
        prominent: false,
        shortcut: None,
    }
}

/// An element of the app's own, `width` px wide, such as a search field or a
/// path bar. `label` is its name in the customise menu. It never goes into the
/// overflow menu.
pub fn toolbar_element(
    id: &'static str,
    label: impl Into<String>,
    element: impl Into<Element>,
    width: f32,
) -> ToolbarItem {
    ToolbarItem {
        id,
        label: label.into(),
        icon: None,
        action: Action::Element {
            element: element.into(),
            width,
        },
        enabled: true,
        placement: Placement::Primary,
        prominent: false,
        shortcut: None,
    }
}

impl ToolbarItem {
    /// Makes the button a toggle: drawn on when `on`, and the overflow menu
    /// gives it a tick.
    pub fn toggled(mut self, on: bool) -> Self {
        if let Action::Press { toggled, .. } = &mut self.action {
            *toggled = Some(on);
        }
        self
    }

    /// Dims the button and ignores presses. It stays in the bar, so the bar
    /// keeps its shape.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// [`Placement::Leading`].
    pub fn leading(self) -> Self {
        self.placement(Placement::Leading)
    }

    /// [`Placement::Secondary`].
    pub fn secondary(self) -> Self {
        self.placement(Placement::Secondary)
    }

    /// The bar's one prominent action: a labelled accent button at the
    /// trailing end that never goes into the overflow menu. Use it for the
    /// thing most people open the window to do ("Save", "Send").
    pub fn prominent(mut self) -> Self {
        self.prominent = true;
        self
    }

    /// The shortcut shown beside the item in the overflow menu, such as `Ctrl+N`.
    /// It is a hint: the app handles the keys.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The item as a menu row: every toolbar item is also a menu command, so
    /// people who hide or customise the bar still reach it. The overflow menu
    /// uses it, and an app can put the same rows in its own menu. An element
    /// item has none.
    pub fn menu_item(&self) -> Option<MenuItem> {
        let mut item = match &self.action {
            Action::Press { on_press, toggled } => {
                let on_press = on_press.clone();
                let item = menu_item(self.label.clone(), move |()| on_press.call(()));
                match toggled {
                    Some(on) => item.checked(*on),
                    None => item,
                }
            }
            Action::Menu(items) => submenu(self.label.clone(), items.clone()),
            Action::Element { .. } => return None,
        };
        if let Some(icon) = self.icon {
            item = item.icon(icon);
        }
        if let Some(shortcut) = &self.shortcut {
            item = item.shortcut(shortcut.clone());
        }
        if !self.enabled {
            item = item.disabled();
        }
        Some(item)
    }
}

/// Items that belong together, set apart from the next group by a wider gap. A
/// bar has at most three: more is a menu.
#[derive(Clone, PartialEq)]
pub struct ToolbarGroup {
    pub(super) items: Vec<ToolbarItem>,
    pub(super) linked: bool,
}

/// A group of `items`.
pub fn toolbar_group(items: Vec<ToolbarItem>) -> ToolbarGroup {
    ToolbarGroup {
        items,
        linked: false,
    }
}

impl ToolbarGroup {
    /// Joins the group's buttons in one rounded bar with hairlines between them,
    /// as Back and Forward are, or the choices of a view switch.
    pub fn linked(mut self) -> Self {
        self.linked = true;
        self
    }
}
