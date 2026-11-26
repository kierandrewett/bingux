//! The page or the proxy asks who you are.

use bingux_ui::prelude::*;
use bingux_ui::web_view::requests::AuthRequest;
use freya::prelude::*;

use crate::ui::Ctx;

/// A sign-in dialog.
pub struct AuthPrompt;

impl AuthPrompt {
    pub fn show(ctx: Ctx, key: u64, request: &AuthRequest) {
        let mut name = ctx.prompts.text;
        let mut password = ctx.prompts.password;
        name.set(String::new());
        password.set(String::new());
        let who = if request.proxy {
            format!("The proxy {} asks you to sign in", request.host)
        } else {
            format!("Sign in to {}", request.host)
        };
        let body = match (request.realm.is_empty(), request.retry) {
            (false, true) => format!(
                "{} \u{2014} the name or password was not accepted.",
                request.realm
            ),
            (false, false) => request.realm.clone(),
            (true, true) => "The name or password was not accepted.".to_string(),
            (true, false) => "The site asks who you are.".to_string(),
        };
        let form = rect()
            .tw_layout(&[flex_col(), w_full(), gap(S2)])
            .child(
                text_field(ctx.prompts.text, |_: String| {})
                    .fill_width()
                    .autofocus()
                    .placeholder("Name"),
            )
            .child(
                text_field(ctx.prompts.password, |_: String| {})
                    .fill_width()
                    .password()
                    .placeholder("Password"),
            );
        ctx.overlays.show_dialog(
            dialog(who, body)
                .content(form)
                .action(CANCEL_ID, "Cancel", ResponseKind::Default, move |_: ()| {
                    ctx.with(|browser| browser.answer_auth(key, None));
                })
                .response("Sign in", ResponseKind::Suggested, move |_: ()| {
                    let credentials = (
                        ctx.prompts.text.peek().clone(),
                        ctx.prompts.password.peek().clone(),
                    );
                    ctx.with(|browser| browser.answer_auth(key, Some(credentials)));
                })
                .default_action("sign_in")
                .cancel_action(CANCEL_ID),
        );
    }
}
