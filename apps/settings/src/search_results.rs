//! The search results: the hits of `bingux_ui::search` for the text in the
//! search field, grouped by page in the order of each page's best hit. A hit
//! shows its title and its path with the matched text marked; choosing one
//! hands its `uri` to the router, which opens the page or subpage and flashes
//! the row the fragment names, and clears the search.
//!
//! The documents come from [`crate::settings_index`]; nothing here matches
//! text. The search runs on the worker pool, so typing never waits for it.

use bingux_ui::prelude::*;
use bingux_ui::search::{self, Hit, Search};
use bingux_ui::search_highlight;
use freya::prelude::*;

use crate::pages::PageId;
use crate::settings_index::{APPS_INDEX, INDEX};

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

/// Most rows listed under one page. A short query can match a page's whole
/// index, so the list is capped; a longer query narrows it.
const MAX_ROWS_PER_PAGE: usize = 8;
/// Most hits asked of the engine.
const MAX_HITS: usize = 40;

/// The page a hit belongs to.
fn page_of(hit: &Hit) -> Option<PageId> {
    PageId::from_panel(hit.str_field("page")?)
}

/// A row for `hit`: its title and path with the match marked. Choosing it
/// opens its address and clears the search.
fn hit_row(router: Router, mut query: State<String>, hit: &Hit) -> Element {
    let title = hit.title().unwrap_or_default().to_string();
    let uri = hit.uri().unwrap_or_default().to_string();
    let mut row = row(title).title_marks(hit.ranges("title")).navigates();
    // A path with more than the page in it is the row's subtitle.
    if hit.breadcrumb().len() > 1
        && let Some((path, ranges)) = hit.joined("breadcrumb", " \u{203a} ")
    {
        row = row.subtitle(path).subtitle_marks(ranges);
    }
    if hit.str_field("kind") == Some("application")
        && let Some(icon) = hit.icon()
    {
        row = row.leading_app(icon);
    }
    row.on_press(move |_| {
        if !uri.is_empty() {
            router.navigate(&uri);
        }
        query.set(String::new());
    })
    .into()
}

/// Replaces the page content while the search box has text.
#[derive(PartialEq)]
pub struct SearchResults {
    pub query: State<String>,
    /// Where the address of the best hit is published, for Enter in the
    /// search field.
    pub first: State<Option<String>>,
}

impl Component for SearchResults {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let router = use_router();
        let query = self.query;
        let mut first = self.first;
        let text = query.read().trim().to_string();

        // Declared before any early return, so the hook order never depends on
        // the query.
        let request = Search::new(&text)
            .index(INDEX)
            .index(APPS_INDEX)
            .limit(MAX_HITS)
            .highlight(&["title", "breadcrumb"]);
        let state = search::use_search_with(request, !text.is_empty());
        // The hits answer what was typed a moment ago until the new search
        // lands; only an answer to the current text is used for Enter.
        let current = state.query == text;
        let best = if current {
            state.hits.first().and_then(Hit::uri).map(str::to_string)
        } else {
            None
        };
        use_side_effect_with_deps(&best, move |best| {
            if *first.peek() != *best {
                first.set(best.clone());
            }
        });

        let mut body = rect().tw(&tokens, PAGE);
        if text.is_empty() {
            return body;
        }
        // Pages in the order of their best hit.
        let mut groups: Vec<(PageId, Vec<&Hit>)> = Vec::new();
        for hit in &state.hits {
            let Some(page) = page_of(hit) else { continue };
            match groups.iter_mut().find(|(p, _)| *p == page) {
                Some((_, hits)) => hits.push(hit),
                None => groups.push((page, vec![hit])),
            }
        }
        for (page, hits) in groups {
            let rows: Vec<Element> = hits
                .into_iter()
                .take(MAX_ROWS_PER_PAGE)
                .map(|hit| hit_row(router, query, hit))
                .collect();
            body = body.child(search_highlight::page_block(
                page.title(),
                page.icon(),
                Group::new(None).unfiltered().rows(rows).into(),
            ));
        }
        // "No match" only once the search for this text has finished: an
        // empty list while it runs is not an answer.
        if state.hits.is_empty() && current && !state.loading {
            body = body.child(status(Status::Empty(format!(
                "No settings match \"{text}\"."
            ))));
        }
        body
    }
}
