//! The page of a tab that has no address.

use bingux_ui::prelude::*;
use bingux_ui::search;
use freya::prelude::*;

use crate::address::Address;
use crate::profile::history;
use crate::ui::Ctx;

/// How many visited pages the new tab page lists.
const RECENT: usize = 5;

/// The page of a tab that has no address.
pub fn new_tab(ctx: Ctx, key: u64) -> Element {
    NewTab { ctx, key }.into()
}

/// The new tab page. A component of its own: the pages visited last are the
/// `history` searchable's, asked off the UI thread.
#[derive(PartialEq)]
struct NewTab {
    ctx: Ctx,
    key: u64,
}

impl Component for NewTab {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let (private, has_history) = {
            let browser = ctx.browser.read();
            (browser.options.private, browser.has_history())
        };
        let found = search::use_search_with(history::recent(RECENT), has_history);
        let page = if private {
            status_page(Icon::NetworkWirelessEncrypted, "Private browsing")
                .description("Nothing you do in this window is kept: no history, cookies or site data. Downloads you save stay on this computer.")
        } else {
            status_page(Icon::WebBrowser, "New tab")
                .description("Search or enter an address in the bar above.")
        };
        if found.hits.is_empty() {
            return page.into_element();
        }
        let mut group = Group::new("Recently visited".to_string());
        for visit in found.hits.iter().map(history::visited) {
            let name = if visit.title.is_empty() {
                Address::new(&visit.url).display()
            } else {
                visit.title
            };
            let target = visit.url.clone();
            group = group.row(
                row(name)
                    .subtitle(Address::new(&visit.url).display())
                    .leading(Icon::Clock)
                    .navigates()
                    .on_press(move |_: ()| ctx.with(|browser| browser.navigate(key, &target))),
            );
        }
        page.child(clamp(Dim::PopupNarrow, group)).into_element()
    }
}
