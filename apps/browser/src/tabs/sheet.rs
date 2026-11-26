//! The list of tabs of a phone: a sheet with a row for each tab, in place of
//! the strip that has no room.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::address::Address;

use crate::ui::Ctx;

#[derive(PartialEq)]
pub struct TabsSheet {
    ctx: Ctx,
}

impl TabsSheet {
    /// Open the list of tabs.
    pub fn open(ctx: Ctx) {
        // The sheet is as tall as its rows. The popup measures nothing here (a body
        // of rows in a window that already holds the page leaves it at the most the
        // window allows), so the height is asked for: the header, the padding and a
        // row with a subtitle for each tab, which the popup limits to the window.
        let rows = ctx.browser.peek().tabs.all().len() as f32;
        let height = Dim::Header.px()
            + Dim::Hairline.px()
            + 2.0 * S4.px()
            + rows * Dim::RowSubtitle.px()
            + S4.px();
        ctx.window.open(
            popup(tr!("browser-tabs-sheet-title"), TabsSheet { ctx })
                .size(Dim::PopupWidth.px(), height),
        );
    }
}

impl Component for TabsSheet {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let window = use_window();
        let rows: Vec<Element> = {
            let browser = ctx.browser.read();
            let active = browser.tabs.active_key();
            browser
                .tabs
                .all()
                .iter()
                .map(|tab| {
                    let key = tab.key;
                    let mut line = row(tab.display_title())
                        .leading(tab.icon(browser.options.private))
                        .selected(key == active)
                        .on_press(move |_: ()| {
                            ctx.with(|browser| browser.select(key));
                            window.close();
                        })
                        .trailing(
                            icon_button(Icon::WindowClose, move |_: ()| {
                                ctx.with(|browser| browser.close_tab(key))
                            })
                            .tooltip(&tr!("browser-tabs-close-tooltip")),
                        );
                    let detail = Address::new(&tab.url).display();
                    if !detail.is_empty() {
                        line = line.subtitle(detail);
                    }
                    line.into()
                })
                .collect()
        };
        popup_column().child(Group::new(None::<String>).rows(rows))
    }
}
