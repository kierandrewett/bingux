//! The page of a tab that has no address.

use bingux_ui::prelude::*;
use bingux_ui::search;
use freya::prelude::*;

use crate::address::Address;
use crate::history;
use crate::ui::Ctx;

/// How many visited pages the new tab page lists.
const RECENT: usize = 5;

/// The page of a tab that has no address. A component of its own: the pages visited last are the
/// `history` searchable's, asked off the UI thread.
#[derive(PartialEq)]
pub struct NewTabPage {
    pub ctx: Ctx,
    pub key: u64,
}

impl Component for NewTabPage {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let (private, has_history) = {
            let browser = ctx.browser.read();
            (browser.options.private, browser.has_history())
        };
        let found = search::use_search_with(history::recent(RECENT), has_history);
        let page = if private {
            status_page(
                Icon::NetworkWirelessEncrypted,
                tr!("browser-new-tab-private-title"),
            )
            .description(tr!("browser-new-tab-private-description"))
        } else {
            status_page(Icon::WebBrowser, tr!("browser-new-tab-title"))
                .description(tr!("browser-new-tab-description"))
        };
        if found.hits.is_empty() {
            return page.into_element();
        }
        let mut group = Group::new(tr!("browser-new-tab-recent-title"));
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
