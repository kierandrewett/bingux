//! A small icon action at the end of a list row: eject a drive, remove a
//! bookmark, dismiss a notification.
//!
//! The row is the keyboard stop and has the same action in its context menu,
//! so the button is for the pointer and for touch only. It is not a Tab stop
//! and the arrow keys walk past it, which keeps Up and Down moving row by row.

use freya::prelude::*;

use super::press::use_press;
use super::progress::{SpinnerSize, spinner};
use super::tooltip::hover_tip;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;

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
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let press = use_press();
        let busy = self.busy;
        let on_press = self.on_press.clone();
        let glyph: Element = if busy {
            spinner(SpinnerSize::Small).into()
        } else {
            icons::icon(self.icon, IconSize::Control, Paint::Muted.color(&tokens)).into()
        };
        let button = rect()
            .component("RowAction")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.tooltip.clone())
            .tw(
                &tokens,
                &[
                    center(),
                    w(Dim::Box7),
                    h(Dim::Box7),
                    rounded(Radius::Control),
                    surface(Paint::Transparent, hovering() && !busy, pressing(), false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                // The press belongs to the button, not to the row under it.
                event.stop_propagation();
                if !busy {
                    pressing.set(true);
                    press.begin(&event, None);
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| {
                press.release();
                pressing.set_if_modified(false);
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                if !busy && press.tap(&event) {
                    on_press.call(());
                }
            })
            .child(glyph);
        hover_tip(button, self.tooltip.clone())
    }
}
