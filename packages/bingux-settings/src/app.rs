//! The window: bingux-ui's navigation split view over the page registry.
//!
//! The window's address is the state. The sidebar's selected page and the
//! subpage shown are both derived from it, and a change to either one (a
//! sidebar click, a row, Back, a forwarded address) goes through the address.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;
use crate::routes::{Route, page_of, use_navigator};
use crate::search_results::SearchResults;
use crate::settings_index;

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

/// Window root. The page or subpage shown is the window's address, provided
/// by [`bingux_ui::app::Application`] or a snapshot. The root address shows
/// the list of pages on a narrow window and the first page on a wide one.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        use_hook(crate::pages::prefetch_all);
        let address = use_address();
        // The address is the state: the sidebar shows the page it names, and
        // the root address names none, so the first page is selected.
        let selected = page_of(&address).unwrap_or(PageId::ALL[0]);
        let navigator = use_navigator();
        let top = Route::from_address(&address);
        let signed_in = crate::pages::users::use_signed_in();
        let content: Element = match &top {
            Some(route) => route.render(),
            None => rect()
                .tw(&tokens, PAGE)
                .child(selected.render())
                .child(RelatedPages { page: selected })
                .into(),
        };
        let query = use_state(crate::search_results::initial_query);
        let results: Element = SearchResults { query }.into();
        // Enter opens the page of the best match and clears the search.
        let open_first: EventHandler<()> = (move |_: ()| {
            let text = query.read().clone();
            if let Some((page, _)) = settings_index::matching(&text).first() {
                navigator.goto(*page);
            }
            let mut query = query;
            query.set(String::new());
        })
        .into();
        // A sidebar click opens the page's top level, unless the address
        // already names that page.
        let on_select: EventHandler<PageId> = (move |page: PageId| {
            if current_address().as_ref().and_then(page_of) != Some(page) {
                navigate(page.address());
            }
        })
        .into();
        // Back from a page and Ctrl+F on a narrow window ask for the list of
        // pages, which is the root address.
        let on_show_list: EventHandler<bool> = (move |show: bool| {
            if show {
                navigate(Address::new(crate::routes::SCHEME));
            }
        })
        .into();
        let mut split = NavigationSplitView::new(
            "Settings",
            PageId::ALL.to_vec(),
            selected,
            on_select,
            content,
        )
        .search(query, results, open_first)
        .list(address.is_root(), on_show_list);
        if let Some((name, _)) = &signed_in {
            split = split.profile(Profile {
                name: name.clone(),
                target: PageId::Users,
            });
        }
        if let Some(route) = &top {
            split = split.subpage(route.title(), move |_| navigator.pop());
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
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
