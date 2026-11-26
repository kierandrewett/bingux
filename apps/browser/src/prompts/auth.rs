//! The page or the proxy asks who you are.

use bingux_ui::prelude::*;
use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::AuthRequest;
use freya::prelude::*;

use crate::ui::Ctx;

/// A sign-in dialog.
pub struct AuthPrompt;

impl AuthPrompt {
    /// Show the sign-in dialog for question `id` of tab `key`.
    pub fn show(ctx: Ctx, key: u64, id: RequestId, request: &AuthRequest) {
        let mut name = ctx.prompts.text;
        let mut password = ctx.prompts.password;
        name.set(String::new());
        password.set(String::new());
        let who = if request.proxy {
            tr!("browser-prompt-proxy-title", host = request.host.as_str())
        } else {
            tr!("browser-prompt-sign-in-title", host = request.host.as_str())
        };
        let body = match (request.realm.is_empty(), request.retry) {
            (false, true) => tr!(
                "browser-prompt-realm-rejected-description",
                realm = request.realm.as_str()
            ),
            (false, false) => request.realm.clone(),
            (true, true) => tr!("browser-prompt-rejected-description"),
            (true, false) => tr!("browser-prompt-who-description"),
        };
        let form = rect()
            .tw_layout(&[flex_col(), w_full(), gap(S2)])
            .child(
                text_field(ctx.prompts.text, |_: String| {})
                    .fill_width()
                    .autofocus()
                    .placeholder(tr!("browser-prompt-name-placeholder")),
            )
            .child(
                text_field(ctx.prompts.password, |_: String| {})
                    .fill_width()
                    .password()
                    .placeholder(tr!("browser-prompt-password-placeholder")),
            );
        ctx.overlays.show_dialog(
            dialog(who, body)
                .content(form)
                .action(
                    CANCEL_ID,
                    tr!("browser-prompt-cancel-button"),
                    ResponseKind::Default,
                    move |_: ()| {
                        ctx.with(|browser| browser.answer_auth(key, id, None));
                    },
                )
                .response(
                    tr!("browser-prompt-sign-in-button"),
                    ResponseKind::Suggested,
                    move |_: ()| {
                        let credentials = (
                            ctx.prompts.text.peek().clone(),
                            ctx.prompts.password.peek().clone(),
                        );
                        ctx.with(|browser| browser.answer_auth(key, id, Some(credentials)));
                    },
                )
                .default_action("sign_in")
                .cancel_action(CANCEL_ID),
        );
    }
}
