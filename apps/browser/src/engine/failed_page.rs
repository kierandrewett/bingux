//! The page of a window whose engine is gone.

use bingux_ui::prelude::*;
use bingux_ui::web_view::FailureCause;
use freya::prelude::*;

use super::failure;
use crate::ui::Ctx;

/// The page of a window whose engine is gone. The window is a `Screen` with
/// nothing in its header but the window's buttons, so it can still be moved and
/// closed; the page is its content.
#[derive(PartialEq)]
pub struct EngineFailedPage {
    pub ctx: Ctx,
    pub cause: FailureCause,
    pub message: String,
    pub code: u32,
    pub restartable: bool,
}

impl Component for EngineFailedPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let window = ctx.window;
        let page = failure::EnginePage::of(self.cause, &self.message, self.code, self.restartable);
        let mut view = status_page(page.icon, page.title).description(page.text);
        // A status page holds one element under its action: the way out, when
        // there is a way on, and how the engine ended, when it ran.
        let mut under = Vec::new();
        if page.restart {
            view = view.action(tr!("browser-engine-restart-button"), move |_: ()| {
                ctx.with(|browser| browser.restart_engine())
            });
            under.push(
                action_button(tr!("browser-engine-close-button"), move |_: ()| {
                    window.close()
                })
                .flat()
                .into_element(),
            );
        } else {
            view = view.action(tr!("browser-engine-close-button"), move |_: ()| {
                window.close()
            });
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
        Screen::new(HeaderBar::new(tr!("browser-title")).trailing(WindowControls))
            .content(view.into_element())
    }
}
