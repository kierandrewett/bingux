//! What a row of a list says about itself.

use freya::prelude::*;

use crate::icons::Icon;

/// An action a row has beyond opening it: on a phone the row swipes to show it,
/// and on a desktop it joins the row's menu.
#[derive(Clone, PartialEq)]
pub struct SwipeAction {
    pub(super) label: String,
    pub(super) icon: Option<Icon>,
    pub(super) destructive: bool,
    pub(super) on_press: EventHandler<()>,
}

/// An action for a row. `label` is a verb that names the result ("Archive",
/// "Delete"); `on_press` runs once.
pub fn swipe_action(
    label: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> SwipeAction {
    SwipeAction {
        label: label.into(),
        icon: None,
        destructive: false,
        on_press: on_press.into(),
    }
}

impl SwipeAction {
    /// An icon above the label of the swiped button, and before it in a menu.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draws the action in the danger colour: it loses something.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// How a row of a [`list_view`](super::list_view) looks. Build it with
/// [`list_row`].
#[derive(Clone, PartialEq)]
pub struct ListRow {
    pub(super) title: String,
    pub(super) subtitle: Option<String>,
    pub(super) icon: Option<Icon>,
    pub(super) leading: Option<Element>,
    pub(super) value: Option<String>,
    pub(super) chevron: bool,
    pub(super) enabled: bool,
    pub(super) swipe: Vec<SwipeAction>,
}

/// A row with a title.
pub fn list_row(title: impl Into<String>) -> ListRow {
    ListRow {
        title: title.into(),
        subtitle: None,
        icon: None,
        leading: None,
        value: None,
        chevron: false,
        enabled: true,
        swipe: Vec::new(),
    }
}

impl ListRow {
    /// A line under the title. It shows only in a list made
    /// [`two_line`](super::ListView::two_line), because every row of a long list
    /// is as tall as the others.
    pub fn subtitle(mut self, text: impl Into<String>) -> Self {
        self.subtitle = Some(text.into());
        self
    }

    /// A symbol before the title.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Any element before the title, in place of an icon: an avatar, an
    /// application icon. It is at most as tall as the row.
    pub fn leading(mut self, element: impl Into<Element>) -> Self {
        self.leading = Some(element.into());
        self
    }

    /// A value at the end of the row, in the muted ink: a size, a date, a state.
    pub fn value(mut self, text: impl Into<String>) -> Self {
        self.value = Some(text.into());
        self
    }

    /// A chevron at the end of the row, for a row that opens something.
    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }

    /// A row that cannot be chosen: dimmed, skipped by the keyboard cursor.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The row's other actions: swiped open on a phone, in the menu on a
    /// desktop.
    pub fn actions(mut self, actions: Vec<SwipeAction>) -> Self {
        self.swipe = actions;
        self
    }
}
