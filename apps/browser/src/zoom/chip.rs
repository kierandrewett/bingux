//! The zoom of the tab on show, as a button that puts it back to 100 %.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::Zoom;
use crate::ui::Ctx;

/// The zoom, as a button that puts it back to 100 %. The toolbar shows it
/// while the zoom is not 100 %.
#[derive(PartialEq)]
pub struct ZoomChip {
    pub ctx: Ctx,
    /// The tab the zoom is of.
    pub key: u64,
    pub zoom: Zoom,
}

impl Component for ZoomChip {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let key = self.key;
        action_button(format!("{}%", self.zoom.percent()), move |_: ()| {
            ctx.with(|browser| browser.set_zoom(key, Zoom::DEFAULT));
        })
        .flat()
        .compact()
    }
}
