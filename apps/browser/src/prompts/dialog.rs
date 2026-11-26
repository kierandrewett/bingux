//! `alert`, `confirm`, `prompt` and `beforeunload`.

use bingux_ui::prelude::*;
use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::{DialogKind, DialogRequest};
use freya::prelude::*;

use crate::address::Address;
use crate::ui::Ctx;

/// A question of the page that wants an answer in a dialog.
pub struct DialogPrompt;

impl DialogPrompt {
    fn field(state: State<String>) -> Element {
        text_field(state, |_: String| {})
            .fill_width()
            .autofocus()
            .into()
    }

    /// Show the dialog for question `id` of tab `key`. Its buttons answer that
    /// question and no other.
    pub fn show(ctx: Ctx, key: u64, id: RequestId, request: &DialogRequest) {
        let host = Address::new(&request.origin).host();
        let title = match &request.kind {
            DialogKind::BeforeUnload => tr!("browser-prompt-leave-title"),
            _ if host.is_empty() => tr!("browser-prompt-page-says-title"),
            _ => tr!("browser-prompt-host-says-title", host = host.clone()),
        };
        let answer = move |accept: bool| {
            move |_: ()| {
                let text = ctx.prompts.text.peek().clone();
                ctx.with(|browser| browser.answer_dialog(key, id, accept, accept.then_some(text)));
            }
        };
        let message = if request.kind == DialogKind::BeforeUnload && request.message.is_empty() {
            tr!("browser-prompt-leave-description")
        } else {
            request.message.clone()
        };
        let shown = match &request.kind {
            DialogKind::Alert => dialog(title, message)
                .response(
                    tr!("browser-prompt-ok-button"),
                    ResponseKind::Suggested,
                    answer(true),
                )
                .default_action("ok")
                .cancel_action("ok"),
            DialogKind::Confirm => dialog(title, message)
                .action(
                    CANCEL_ID,
                    tr!("browser-prompt-cancel-button"),
                    ResponseKind::Default,
                    answer(false),
                )
                .response(
                    tr!("browser-prompt-ok-button"),
                    ResponseKind::Suggested,
                    answer(true),
                )
                .default_action("ok"),
            DialogKind::Prompt { default } => {
                let mut text = ctx.prompts.text;
                text.set(default.clone());
                dialog(title, message)
                    .content(Self::field(ctx.prompts.text))
                    .action(
                        CANCEL_ID,
                        tr!("browser-prompt-cancel-button"),
                        ResponseKind::Default,
                        answer(false),
                    )
                    .response(
                        tr!("browser-prompt-ok-button"),
                        ResponseKind::Suggested,
                        answer(true),
                    )
                    .default_action("ok")
            }
            DialogKind::BeforeUnload => dialog(title, message)
                .action(
                    CANCEL_ID,
                    tr!("browser-prompt-stay-button"),
                    ResponseKind::Default,
                    answer(false),
                )
                .response(
                    tr!("browser-prompt-leave-button"),
                    ResponseKind::Destructive,
                    answer(true),
                )
                .default_action(CANCEL_ID),
        };
        ctx.overlays
            .show_dialog(shown.cancel_action(if request.kind == DialogKind::Alert {
                "ok"
            } else {
                CANCEL_ID
            }));
    }
}
