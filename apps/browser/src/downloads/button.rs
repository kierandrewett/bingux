//! The downloads button of the toolbar.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::ui::Ctx;

/// The downloads button; busy while something downloads.
#[derive(PartialEq)]
pub struct DownloadsButton {
    pub ctx: Ctx,
}

impl Component for DownloadsButton {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let active = ctx.browser.read().downloads.active();
        let state = ctx.downloads_popover;
        let mut anchor = state.anchor;
        let mut open = state.open;
        // An engine that does not save downloads has nothing to list: the button
        // stays, so the toolbar keeps its shape, and says why it does nothing.
        let can = ctx.browser.read().engine.capabilities().downloads;
        let tip = if can {
            tr!("browser-downloads-button-tooltip", count = active)
        } else {
            tr!("browser-downloads-unavailable-tooltip")
        };
        rect()
            .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
            .child(
                icon_button(Icon::FolderArrowDown, move |_: ()| {
                    open.set(true);
                })
                .tooltip(&tip)
                .working(active > 0)
                .enabled(can),
            )
    }
}
