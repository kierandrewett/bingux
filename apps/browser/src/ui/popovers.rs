//! The two cards that hang from the toolbar: what is known about the page's
//! connection and permissions, and the downloads.

use freya::prelude::*;

use crate::downloads::DownloadsPopover;
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
        rect()
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .child(SiteInfoPopover::element(ctx))
            .child(DownloadsPopover::element(ctx))
    }
}
