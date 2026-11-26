//! Back, Forward, Reload and Stop: the buttons that move a tab through its
//! history.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::ui::Ctx;

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;

/// Back: the tab goes one page back in its history.
#[derive(PartialEq)]
pub struct BackButton {
    pub ctx: Ctx,
    pub key: u64,
    pub enabled: bool,
}

impl Component for BackButton {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        icon_button(Icon::GoPrevious, move |_: ()| {
            ctx.with(|browser| browser.back(key))
        })
        .tooltip(&tr!("browser-nav-back-tooltip"))
        .enabled(self.enabled)
    }
}

/// Forward: the tab goes one page forward in its history.
#[derive(PartialEq)]
pub struct ForwardButton {
    pub ctx: Ctx,
    pub key: u64,
    pub enabled: bool,
}

impl Component for ForwardButton {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        icon_button(Icon::GoNext, move |_: ()| {
            ctx.with(|browser| browser.forward(key))
        })
        .tooltip(&tr!("browser-nav-forward-tooltip"))
        .enabled(self.enabled)
    }
}

/// Reload, or Stop while the page loads.
#[derive(PartialEq)]
pub struct ReloadButton {
    pub ctx: Ctx,
    pub key: u64,
    pub loading: bool,
    pub is_web: bool,
}

impl Component for ReloadButton {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        if self.loading {
            icon_button(Icon::WindowClose, move |_: ()| {
                ctx.with(|browser| browser.stop(key))
            })
            .tooltip(&tr!("browser-nav-stop-tooltip"))
        } else {
            icon_button(Icon::Refresh, move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            })
            .tooltip(&tr!("browser-nav-reload-tooltip"))
            .enabled(self.is_web)
        }
    }
}
