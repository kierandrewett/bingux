//! The two cards that hang from the toolbar: what is known about the page's
//! connection and permissions, and the downloads.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::downloads::popover::download_list;
use crate::site_info::SiteInfoPopover;

use super::ctx::Ctx;

/// Both popovers, open or not.
#[derive(PartialEq)]
pub struct Popovers {
    pub ctx: Ctx,
}

impl Component for Popovers {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let chrome = ctx.chrome;
        let mut downloads_open = chrome.downloads_open;
        let downloads_anchor = chrome.downloads_anchor.read().clone();
        rect()
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .child(SiteInfoPopover::element(ctx))
            .child(
                popover(downloads_open(), &downloads_anchor, download_list(ctx))
                    .width_px(ctx.popover_width())
                    .on_dismiss(move |_: ()| downloads_open.set(false)),
            )
    }
}
