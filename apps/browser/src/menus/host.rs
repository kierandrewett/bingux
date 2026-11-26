//! Opens the page's context menu when the engine asks for it.

use freya::prelude::*;

use crate::ui::Ctx;

/// Opens the page's context menu when the engine asks for it. It draws
/// nothing.
#[derive(PartialEq)]
pub struct ContextMenuHost {
    pub ctx: Ctx,
}

impl Component for ContextMenuHost {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let id = ctx.browser.read().menu.as_ref().map(|menu| {
            (
                menu.tab,
                menu.request,
                menu.target.position.0 as i32,
                menu.target.position.1 as i32,
            )
        });
        use_side_effect_with_deps(
            &id,
            move |id: &Option<(u64, Option<bingux_ui::web_view::RequestId>, i32, i32)>| {
                if id.is_none() {
                    return;
                }
                let Some(menu) = ctx.browser.peek().menu.clone() else {
                    return;
                };
                let (x, y) = *ctx.chrome.page_origin.peek();
                let at = (x + menu.target.position.0, y + menu.target.position.1);
                let items = super::ContextMenu::items(ctx, &menu);
                // The engine waits for the menu to be closed: it is told now, since
                // what the menu does is the browser's, not the page's.
                ctx.with(|browser| browser.dismiss_menu());
                ctx.overlays.show_menu(at, items);
            },
        );
        rect().width(Size::px(0.0)).height(Size::px(0.0))
    }
}
