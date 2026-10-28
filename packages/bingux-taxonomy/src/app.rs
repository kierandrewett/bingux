//! The window: bingux-ui's navigation split view over the registry's pages.
//!
//! The window's address is the state, as in Settings. `bingux-taxonomy
//! controls/buttons` opens that page; the root address shows the first page
//! on a wide window and the list of pages on a narrow one.

use std::sync::LazyLock;

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::env::{describe, provide_env, use_env};
use crate::model::{PageId, pages};
use crate::switcher::Switcher;
use crate::view::PageView;

/// The address scheme: `bingux-taxonomy://controls/buttons`.
pub const SCHEME: &str = "bingux-taxonomy";

/// The words the sidebar search matches for each page: the names, Rust items
/// and API lines of its entries. Built once.
static KEYWORDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    pages()
        .iter()
        .map(|page| {
            let mut words = String::new();
            for entry in &page.entries {
                words.push_str(entry.name);
                words.push(' ');
                words.push_str(&entry.covers.join(" "));
                words.push(' ');
                words.push_str(entry.api);
                words.push(' ');
            }
            words
        })
        .collect()
});

/// The location of a page: `bingux-taxonomy://controls/buttons`.
pub fn location_of(page: PageId) -> Location {
    let [category, title] = page.segments();
    Location::new(SCHEME).join(category).join(title)
}

/// The page a location names, if it names one.
pub fn page_of(location: &Location) -> Option<PageId> {
    PageId::from_segments(location.segments())
}

/// The route table: one sidebar root per page, `category/page`, grouped by
/// category, with the entries' names and Rust items as search keywords.
pub fn table() -> RouteTable {
    let mut table = RouteTable::new(SCHEME).index("Taxonomy", |_| rect().into());
    for page in PageId::all() {
        let info = page.info();
        table = table
            .root(&page.segments().join("/"))
            .title(info.title)
            .icon(info.category.icon())
            .group(info.category.title())
            .keywords(KEYWORDS[page.0].as_str())
            .page(move |_| crate::view::keyed(page.0, PageView { page }));
    }
    table
}

#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_env();
        let env = *use_env().read();
        let router = use_router();
        let query = use_state(String::new);
        let results: Element = SearchResults { query }.into();
        let open_first: EventHandler<()> = (move |_: ()| {
            let text = query.read().to_lowercase();
            if let Some(page) = matching(&text).first() {
                router.switch_root(&location_of(*page));
            }
            let mut query = query;
            query.set(String::new());
        })
        .into();
        let split = NavigationSplitView::new("Taxonomy")
            // The widest page column there is: a desktop stage and a phone
            // stage fit side by side.
            .page_width(Dim::ChooserWidth)
            .search(query, results, open_first)
            .header_trailing(hstack(S2).child(Switcher));
        // The stress switches change how text is made, not what a component
        // reads, so the whole window is rebuilt when they change.
        let rebuilt = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (env.pseudo, env.text_scale).hash(&mut hasher);
            hasher.finish()
        };
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(
                rect()
                    .key(rebuilt)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(OverlayHost::new(split)),
            )
            .child(EnvLabel {
                env: describe(&env),
            })
    }
}

/// The environment in words, for a screenshot to carry. Hidden from the eye:
/// it is an accessibility description of the window, not a drawn label.
#[derive(PartialEq)]
struct EnvLabel {
    env: String,
}

impl Component for EnvLabel {
    fn render(&self) -> impl IntoElement {
        rect()
            .a11y_role(AccessibilityRole::Status)
            .a11y_alt(self.env.clone())
    }
}

/// The pages whose title or keywords hold `text`, best first.
fn matching(text: &str) -> Vec<PageId> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, PageId)> = PageId::all()
        .into_iter()
        .filter_map(|page| {
            let title = page.info().title.to_lowercase();
            if title.contains(text) {
                Some((0, page))
            } else if KEYWORDS[page.0].to_lowercase().contains(text) {
                Some((1, page))
            } else {
                None
            }
        })
        .collect();
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, page)| page).collect()
}

/// The sidebar search's results: the pages that hold a match.
#[derive(PartialEq)]
struct SearchResults {
    query: State<String>,
}

impl Component for SearchResults {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let text = self.query.read().to_lowercase();
        let hits = matching(&text);
        let mut group = Group::new("Results".to_string());
        if hits.is_empty() {
            group = group
                .row(row("Nothing matches").subtitle("Try a Rust name such as Switch or Icon"));
        }
        for page in hits {
            let mut query = self.query;
            group = group.row(
                row(page.info().title)
                    .subtitle(page.info().category.title())
                    .leading(page.info().category.icon())
                    .navigates()
                    .on_press(move |_| {
                        query.set(String::new());
                        router.switch_root(&location_of(page));
                    }),
            );
        }
        page_column().child(group)
    }
}
