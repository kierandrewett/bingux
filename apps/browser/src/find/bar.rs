//! Find in page: bingux-ui's find card over the page, with the browser's
//! options (match case, highlight all).

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::ui::Ctx;

#[derive(PartialEq)]
pub struct FindBar {
    pub ctx: Ctx,
    /// The tab whose page is searched. A new tab is a new card.
    pub key: u64,
}

impl Component for FindBar {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let key = self.key;
        let find = ctx
            .browser
            .read()
            .tabs
            .get(key)
            .map(|tab| tab.find.clone())
            .unwrap_or_default();
        let text = use_state({
            let initial = find.text().to_string();
            move || initial
        });
        let (match_case, whole_words, highlight_all) = find.options();
        find_card(text)
            .placeholder("Find in page")
            .result(find.result())
            .focus_requests(ctx.find.requests())
            .options(vec![
                FindOption::new("Match case", match_case, move |on: bool| {
                    ctx.with(|browser| {
                        browser.set_find_options(key, on, whole_words, highlight_all)
                    });
                }),
                FindOption::new("Highlight all", highlight_all, move |on: bool| {
                    ctx.with(|browser| browser.set_find_options(key, match_case, whole_words, on));
                }),
            ])
            .on_change(move |changed: String| {
                ctx.with(|browser| browser.find(key, &changed, true, true))
            })
            .on_step(move |forward: bool| {
                let current = text.peek().clone();
                ctx.with(|browser| browser.find(key, &current, forward, false));
            })
            .on_close(move |_: ()| ctx.with(|browser| browser.close_find(key)))
    }
}
