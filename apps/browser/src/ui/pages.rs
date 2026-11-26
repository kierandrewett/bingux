//! The pages the browser draws itself in the content area: the new tab page and
//! the pages that stand where a page could not be shown (an address that could
//! not be loaded, a connection that cannot be trusted, a tab that crashed).
//! Each is a status page of bingux-ui with the words of `model::failure`.

use bingux_ui::prelude::*;
use bingux_ui::web_view::FailureCause;
use freya::prelude::*;

use crate::model::failure;

use super::ctx::Ctx;

/// The engine itself is gone: nothing in the window can work. When a new one
/// may be started the tabs are kept, and "Start again" opens them where they
/// were; a window that cannot go on offers only to close.
pub fn engine_failed(
    ctx: Ctx,
    cause: FailureCause,
    message: &str,
    code: u32,
    restartable: bool,
) -> Element {
    EngineFailed {
        ctx,
        cause,
        message: message.to_string(),
        code,
        restartable,
    }
    .into()
}

/// The page of a window whose engine is gone. It is a component of its own so
/// that it paints the window's background (nothing else is on screen) with the
/// tokens of the window.
#[derive(PartialEq)]
struct EngineFailed {
    ctx: Ctx,
    cause: FailureCause,
    message: String,
    code: u32,
    restartable: bool,
}

impl Component for EngineFailed {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let window = ctx.window;
        let page =
            failure::for_engine_failure(self.cause, &self.message, self.code, self.restartable);
        let mut view = status_page(page.icon, page.title).description(page.text);
        // A status page holds one element under its action: the way out, when
        // there is a way on, and how the engine ended, when it ran.
        let mut under = Vec::new();
        if page.restart {
            view = view.action("Start again", move |_: ()| {
                ctx.with(|browser| browser.restart_engine())
            });
            under.push(
                action_button("Close window", move |_: ()| window.close())
                    .flat()
                    .into_element(),
            );
        } else {
            view = view.action("Close window", move |_: ()| window.close());
        }
        if !page.detail.is_empty() {
            under.push(caption(page.detail));
        }
        if !under.is_empty() {
            view = view.child(
                rect()
                    .tw(&tokens, &[flex_col(), items(Align::Center), gap(S2)])
                    .children(under),
            );
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .child(view)
    }
}
