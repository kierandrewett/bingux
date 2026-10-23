//! The window: bingux-ui's navigation split view over the page registry.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;
use crate::routes::{Route, provide_navigator, use_navigator};

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

    fn lead(self) -> &'static str {
        PageId::lead(self)
    }
}

/// Window root. `initial` is the page named on the command line, and
/// `subpage` the subpage named after it, if any. With `list_first`, a narrow
/// window opens on the list of pages: no page was asked for.
#[derive(PartialEq)]
pub struct App {
    pub initial: PageId,
    pub subpage: Option<Route>,
    pub list_first: bool,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        use_hook(crate::pages::prefetch_all);
        let page = use_state(|| self.initial);
        let navigator = provide_navigator(page, self.subpage.iter().cloned().collect());
        let open = navigator.routes();
        let top: Option<Route> = open.last().cloned();
        let signed_in = use_hook(crate::pages::users::signed_in);
        let content: Element = match &top {
            Some(route) => route.render(),
            None => {
                let current = page();
                rect()
                    .width(Size::fill())
                    .spacing(S6)
                    .child(current.render())
                    .child(RelatedPages { page: current })
                    .into()
            }
        };
        let mut split = NavigationSplitView::new("Settings", PageId::ALL.to_vec(), page, content)
            .search_hits(crate::settings_index::hits())
            .start_on_list(self.list_first);
        if let Some((name, user)) = &signed_in {
            split = split.profile(Profile {
                initials: crate::pages::users::initials(name),
                detail: if name == user {
                    "Your account".to_string()
                } else {
                    user.clone()
                },
                name: name.clone(),
                target: PageId::Users,
            });
        }
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

/// The Related group under a top-level page: links to the pages that go with
/// it. Choosing one opens that page at its top level.
#[derive(PartialEq)]
struct RelatedPages {
    page: PageId,
}

impl Component for RelatedPages {
    fn render(&self) -> impl IntoElement {
        let navigator = use_navigator();
        let mut group = Group::new(Some("Related".to_string()));
        for target in self.page.related().iter().copied() {
            group = group.row(
                row(target.title())
                    .leading(target.icon())
                    .navigates()
                    .on_press(move |_| navigator.goto(target)),
            );
        }
        group
    }
}
