//! Back, Forward, Reload and Stop: the buttons that move a tab through its
//! history. Stateless builders, drawn by the toolbar's own render.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::ui::Ctx;

/// The navigation buttons of the toolbar.
pub struct NavButtons;

impl NavButtons {
    /// Back and Forward.
    pub fn history(ctx: Ctx, key: u64, can_back: bool, can_forward: bool) -> Vec<Element> {
        vec![
            icon_button(Icon::GoPrevious, move |_: ()| {
                ctx.with(|browser| browser.back(key))
            })
            .tooltip(&tr!("browser-nav-back-tooltip"))
            .enabled(can_back)
            .into(),
            icon_button(Icon::GoNext, move |_: ()| {
                ctx.with(|browser| browser.forward(key))
            })
            .tooltip(&tr!("browser-nav-forward-tooltip"))
            .enabled(can_forward)
            .into(),
        ]
    }

    /// Reload, or Stop while the page loads.
    pub fn reload(ctx: Ctx, key: u64, loading: bool, is_web: bool) -> Element {
        if loading {
            icon_button(Icon::WindowClose, move |_: ()| {
                ctx.with(|browser| browser.stop(key))
            })
            .tooltip(&tr!("browser-nav-stop-tooltip"))
            .into()
        } else {
            icon_button(Icon::Refresh, move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            })
            .tooltip(&tr!("browser-nav-reload-tooltip"))
            .enabled(is_web)
            .into()
        }
    }
}
