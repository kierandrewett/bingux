//! A small icon action at the end of a list row: eject a drive, remove a
//! bookmark, dismiss a notification.
//!
//! The row is the keyboard stop and has the same action in its context menu,
//! so the button is for the pointer and for touch only. It is not a Tab stop
//! and the arrow keys walk past it, which keeps Up and Down moving row by row.

use freya::prelude::*;

use super::button::icon_button;
use crate::icons::Icon;
use crate::style::*;

/// An icon at the end of a row. While `busy` it shows a spinner and ignores
/// presses, so a slow action cannot be started twice.
#[derive(PartialEq)]
pub struct RowAction {
    icon: Icon,
    tooltip: String,
    busy: bool,
    on_press: EventHandler<()>,
}

pub fn row_action(
    icon: Icon,
    tooltip: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> RowAction {
    RowAction {
        icon,
        tooltip: tooltip.into(),
        busy: false,
        on_press: on_press.into(),
    }
}

impl RowAction {
    /// Shows a spinner in place of the icon and ignores presses.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
}

impl Component for RowAction {
    /// The standard icon button, so a row's action has the hover and pressed
    /// fill, tooltip, name and busy spinner every icon button has. It is the
    /// one that skips the Tab order.
    fn render(&self) -> impl IntoElement {
        icon_button(self.icon, self.on_press.clone())
            .size(Dim::Box7)
            .tooltip(&self.tooltip)
            .busy(self.busy)
            .pointer_only()
    }
}
