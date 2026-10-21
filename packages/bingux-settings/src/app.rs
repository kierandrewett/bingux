//! The window: bingux-ui's navigation split view over the page registry.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;

impl NavItem for PageId {
    fn title(self) -> &'static str {
        PageId::title(self)
    }

    fn icon(self) -> Icon {
        PageId::icon(self)
    }

    fn keywords(self) -> &'static str {
        PageId::keywords(self)
    }
}

/// Window root. `initial` is the page named on the command line.
#[derive(PartialEq)]
pub struct App {
    pub initial: PageId,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let page = use_state(|| self.initial);
        rect()
            .expanded()
            .color(tokens.text)
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(NavigationSplitView::new(
                "Settings",
                PageId::ALL.to_vec(),
                page,
                page().render(),
            )))
    }
}
