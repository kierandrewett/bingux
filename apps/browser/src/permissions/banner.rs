//! The question a page asks about a permission, as a banner under the toolbar.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::permissions::Permissions;
use crate::prompts::Prompt;
use crate::ui::Ctx;

/// The banner of a permission request.
#[derive(PartialEq)]
pub struct PermissionBanner {
    pub ctx: Ctx,
}

impl Component for PermissionBanner {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let (key, text) = {
            let browser = ctx.browser.read();
            let key = browser.tabs.active_key();
            let text = match browser.prompt(key) {
                Some(Prompt::Permission(_, request)) => {
                    Some(Permissions::prompt_text(&request.origin, &request.kinds))
                }
                _ => None,
            };
            (key, text)
        };
        // The banner keeps the last text while it closes, so it does not empty first.
        let mut shown = use_state(String::new);
        if let Some(text) = &text {
            if *shown.peek() != *text {
                let text = text.clone();
                spawn(async move { shown.set(text) });
            }
        }
        let visible = text.is_some();
        let tokens = use_tokens();
        let banner = banner(text.unwrap_or_else(|| shown.read().clone()))
            .kind(BannerKind::Info)
            .icon(Icon::CircleInfo)
            .action(tr!("browser-permissions-block-button"), move |_: ()| {
                ctx.with(|browser| browser.answer_permission(key, false, true))
            })
            .action(
                tr!("browser-permissions-allow-once-button"),
                move |_: ()| ctx.with(|browser| browser.answer_permission(key, true, false)),
            )
            .action(tr!("browser-permissions-allow-button"), move |_: ()| {
                ctx.with(|browser| browser.answer_permission(key, true, true))
            })
            .dismissible(move |_: ()| {
                ctx.with(|browser| browser.answer_permission(key, false, false))
            })
            .visible(visible)
            .gap(S1);
        // The pill stands one step in from the window's edge, like a toast.
        rect().tw(&tokens, &[w_full(), px(S2)]).child(banner)
    }
}
