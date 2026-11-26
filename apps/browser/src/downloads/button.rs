//! The downloads button of the toolbar.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::Downloads;
use crate::ui::Ctx;

/// The downloads button; busy while something downloads.
pub struct DownloadsButton;

impl DownloadsButton {
    pub fn element(ctx: Ctx, downloads: &Downloads) -> Element {
        let state = ctx.downloads_popover;
        let mut anchor = state.anchor;
        let mut open = state.open;
        let active = downloads.active();
        // An engine that does not save downloads has nothing to list: the button
        // stays, so the toolbar keeps its shape, and says why it does nothing.
        let can = ctx.browser.read().engine.capabilities().downloads;
        let tip = match active {
            _ if !can => "Downloads are not available in this engine yet".to_string(),
            0 => "Downloads".to_string(),
            1 => "Downloads: 1 running".to_string(),
            count => format!("Downloads: {count} running"),
        };
        rect()
            .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
            .child(
                icon_button(Icon::FolderDownload, move |_: ()| {
                    open.set(true);
                })
                .tooltip(&tip)
                .working(active > 0)
                .enabled(can),
            )
            .into_element()
    }
}
