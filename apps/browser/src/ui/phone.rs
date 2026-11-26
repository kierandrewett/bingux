//! The list of tabs of a phone: a sheet with a row for each tab, in place of
//! the strip that has no room.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::address;

use super::ctx::Ctx;
use super::strip::icon_of;

/// Open the list of tabs.
pub fn open_tabs(ctx: Ctx) {
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
    ctx.window
        .open(popup("Tabs", TabsSheet { ctx }).size(Dim::PopupWidth.px(), height));
}

#[derive(PartialEq)]
struct TabsSheet {
    ctx: Ctx,
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
                        .leading(icon_of(tab))
                        .selected(key == active)
                        .on_press(move |_: ()| {
                            ctx.with(|browser| browser.select(key));
                            window.close();
                        })
                        .trailing(
                            icon_button(Icon::WindowClose, move |_: ()| {
                                ctx.with(|browser| browser.close_tab(key))
                            })
                            .tooltip("Close tab"),
                        );
                    let detail = address::display(&tab.url);
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
