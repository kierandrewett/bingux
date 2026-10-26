//! The search results. A page whose own title or words match the search is
//! listed whole: a row opens it, with its index entries under it, and its
//! controls are not filtered. Other pages show their real controls, filtered
//! to the matches and stacked under the page's highlighted heading. A page
//! with no match is hidden. Pages that are too heavy or cannot be filtered
//! sensibly are listed by their index entries instead. Choosing a row opens
//! its page and clears the search.

use bingux_ui::prelude::*;
use bingux_ui::search_highlight;
use freya::prelude::*;

use crate::pages::PageId;
use crate::routes::{Navigator, use_navigator};
use crate::settings_index;

/// The search text the window starts with. Snapshots set
/// `BINGUX_SETTINGS_SNAPSHOT_SEARCH` so the results can be captured without
/// typing; a real window always starts with an empty search.
pub fn initial_query() -> String {
    #[cfg(feature = "snapshot")]
    {
        std::env::var("BINGUX_SETTINGS_SNAPSHOT_SEARCH").unwrap_or_default()
    }
    #[cfg(not(feature = "snapshot"))]
    {
        String::new()
    }
}

/// Most rows listed under one page, its own row included. A short query can
/// match a page's whole index, so the list is capped; a longer query narrows it.
const MAX_INDEX_ROWS: usize = 8;

/// Pages listed by their index entries rather than rendered in place. Wi-Fi
/// and Bluetooth list devices that appear and disappear, the displays page
/// draws the arrangement canvas and its own banner, and Applications lists
/// every installed app. None of them filters sensibly per control.
fn index_only(page: PageId) -> bool {
    matches!(
        page,
        PageId::Network | PageId::Bluetooth | PageId::Displays | PageId::Applications
    )
}

/// Opens `page` and clears the search, as choosing its index entry does.
fn open_page(navigator: Navigator, page: PageId, mut query: State<String>) {
    navigator.goto(page);
    query.set(String::new());
}

/// A row labelled `title` that opens `page` and clears the search.
fn open_row(navigator: Navigator, page: PageId, query: State<String>, title: &str) -> Element {
    row(title)
        .navigates()
        .on_press(move |_| open_page(navigator, page, query))
        .context_menu(vec![menu_item("Open page", move |_| {
            open_page(navigator, page, query)
        })])
        .into()
}

/// Replaces the page content while the search box has text.
#[derive(PartialEq)]
pub struct SearchResults {
    pub query: State<String>,
}

impl Component for SearchResults {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let navigator = use_navigator();
        let query = self.query;
        search_highlight::provide(query);
        // Every in-place section counts itself here while it shows. Declared
        // before the early return so the hook order never depends on the query.
        let shown_sections = use_state(|| 0usize);

        let text = query.read().clone();
        let mut body = rect().tw(&tokens, PAGE);
        if text.trim().is_empty() {
            return body;
        }
        let hits = settings_index::matching(&text);
        let own_pages = settings_index::matching_pages(&text);
        let mut listed = 0usize;
        for page in PageId::ALL.iter().copied() {
            if own_pages.contains(&page) {
                // The page itself matches, so it is listed whole and its
                // controls are not filtered.
                let mut rows = vec![open_row(navigator, page, query, page.title())];
                rows.extend(
                    hits.iter()
                        .filter(|(hit_page, title)| *hit_page == page && *title != page.title())
                        .take(MAX_INDEX_ROWS - 1)
                        .map(|(_, title)| open_row(navigator, page, query, title)),
                );
                listed += rows.len();
                body = body.child(search_highlight::page_block(
                    page.title(),
                    page.icon(),
                    Group::new(None).unfiltered().rows(rows).into(),
                ));
            } else if index_only(page) {
                let rows: Vec<Element> = hits
                    .iter()
                    .filter(|(hit_page, _)| *hit_page == page)
                    .take(MAX_INDEX_ROWS)
                    .map(|(_, title)| open_row(navigator, page, query, title))
                    .collect();
                if !rows.is_empty() {
                    listed += rows.len();
                    body = body.child(search_highlight::page_block(
                        page.title(),
                        page.icon(),
                        Group::new(None).unfiltered().rows(rows).into(),
                    ));
                }
            } else {
                body = body.child(search_highlight::section(
                    page.title(),
                    page.icon(),
                    page.render(),
                    shown_sections,
                ));
            }
        }
        if listed == 0 && *shown_sections.read() == 0 {
            body = body.child(status(Status::Empty(format!(
                "No settings match \"{}\".",
                text.trim()
            ))));
        }
        body
    }
}
