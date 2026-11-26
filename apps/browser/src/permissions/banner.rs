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
        // The buttons answer the question the banner was drawn for (its id),
        // not whichever is first when they are pressed.
        let (key, id, text) = {
            let browser = ctx.browser.read();
            let key = browser.tabs.active_key();
            match browser.prompt(key) {
                Some(Prompt::Permission(id, request)) => (
                    key,
                    Some(*id),
                    Some(Permissions::prompt_text(&request.origin, &request.kinds)),
                ),
                _ => (key, None, None),
            }
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
        // While the banner fades out there is no question: its buttons do nothing.
        let answer = move |allow: bool, remember: bool| {
            if let Some(id) = id {
                ctx.with(|browser| browser.answer_permission(key, id, allow, remember));
            }
        };
        let banner = banner(text.unwrap_or_else(|| shown.read().clone()))
            .kind(BannerKind::Info)
            .icon(Icon::DialogInformation)
            .action(tr!("browser-permissions-block-button"), move |_: ()| {
                answer(false, true)
            })
            .action(
                tr!("browser-permissions-allow-once-button"),
                move |_: ()| answer(true, false),
            )
            .action(tr!("browser-permissions-allow-button"), move |_: ()| {
                answer(true, true)
            })
            .dismissible(move |_: ()| answer(false, false))
            .visible(visible)
            .gap(S1);
        // The pill stands one step in from the window's edge, like a toast.
        rect().tw(&tokens, &[w_full(), px(S2)]).child(banner)
    }
}
