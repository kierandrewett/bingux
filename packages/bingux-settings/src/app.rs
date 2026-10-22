//! The window: bingux-ui's navigation split view over the page registry.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;
use crate::routes::{Route, provide_navigator};

impl NavItem for PageId {
    fn title(self) -> &'static str {
        PageId::title(self)
    }

    fn icon(self) -> Icon {
        PageId::icon(self)
    }

    fn group(self) -> &'static str {
        PageId::group(self)
    }

    fn keywords(self) -> &'static str {
        PageId::keywords(self)
    }
}

/// Window root. `initial` is the page named on the command line, and
/// `subpage` the subpage named after it, if any.
#[derive(PartialEq)]
pub struct App {
    pub initial: PageId,
    pub subpage: Option<Route>,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        use_hook(crate::pages::prefetch_all);
        let page = use_state(|| self.initial);
        let navigator = provide_navigator(page, self.subpage.iter().cloned().collect());
        let open = navigator.routes();
        let top: Option<Route> = open.last().cloned();
        let content = top.as_ref().map_or_else(|| page().render(), Route::render);
        let mut split = NavigationSplitView::new("Settings", PageId::ALL.to_vec(), page, content);
        if let Some(route) = &top {
            split = split.subpage(route.title(), move |_| navigator.pop());
        }
        rect()
            .expanded()
            .color(tokens.text)
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(split))
    }
}
