//! The window: bingux-ui's navigation split view over Settings' route table.
//!
//! The window's router holds the stack of locations ([`crate::routes`] declares
//! them). The sidebar lists the root routes, the page is the route of the
//! current location, and Back, the header title and the history menu are the
//! split view's: a page never wires them.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;
use crate::search_results::SearchResults;

/// Window root.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        use_hook(crate::pages::prefetch_all);
        use_hook(crate::settings_index::start);
        let router = use_router();
        let signed_in = crate::pages::users::use_signed_in();
        let query = use_state(crate::search_results::initial_query);
        // The address of the best hit, published by the results for Enter.
        let first = use_state(|| None::<String>);
        let results: Element = SearchResults { query, first }.into();
        // Enter opens the best hit and clears the search.
        let open_first: EventHandler<()> = (move |_: ()| {
            if let Some(uri) = first.peek().clone() {
                router.navigate(&uri);
            }
            let mut query = query;
            query.set(String::new());
        })
        .into();
        let mut split = NavigationSplitView::new("Settings").search(query, results, open_first);
        if let Some((name, _)) = &signed_in {
            split = split.profile(Profile {
                name: name.clone(),
                target: PageId::Users.location(),
            });
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(split))
    }
}

/// A sidebar page with the Related group under it.
#[derive(PartialEq)]
pub(crate) struct RootPage {
    pub page: PageId,
}

impl Component for RootPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .tw(&tokens, PAGE)
            .child(self.page.render())
            .child(RelatedPages { page: self.page })
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
        let router = use_router();
        let mut group = Group::new(Some("Related".to_string()));
        for target in self.page.related().iter().copied() {
            group = group.row(
                row(target.title())
                    .leading(target.icon())
                    .navigates()
                    .on_press(move |_| router.switch_root(&target.location())),
            );
        }
        group
    }
}
