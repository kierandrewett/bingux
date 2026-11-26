//! The layout of a page with its toolbox docked under it.
//!
//! A split pane with the page above and the toolbox below, divided by the
//! control's draggable splitter. The page keeps its own web view (and so its
//! sizing and keys); the toolbox is drawn by a second web view over the
//! toolbox session, which reports its own size back to the browser.

use bingux_ui::controls::split_pane;
use bingux_ui::frame_view::{KeyInput, ViewSize};
use bingux_ui::prelude::*;
use bingux_ui::web_view::{TabId, web_view};
use freya::prelude::*;

use super::ctx::Ctx;
use super::web::Surface;
use crate::commands::Command;
use crate::devtools::DockedToolbox;

/// A page and its toolbox in one content area.
#[derive(PartialEq)]
pub struct DockedPage {
    /// The window's shared state, to send the toolbox's size and the splitter's
    /// position back to the browser.
    pub ctx: Ctx,
    /// The tab that owns the toolbox.
    pub key: u64,
    /// The page's own view, built by the caller so the page keeps its keys and
    /// sizing unchanged.
    pub page: Element,
    /// The toolbox to draw, and where the splitter starts.
    pub toolbox: DockedToolbox,
    /// The toolbox session's surface.
    pub surface: Surface,
    /// The session that draws the toolbox (the same as `toolbox.handle`).
    pub session: TabId,
    /// How the web view reaches the engine.
    pub backend: crate::engine::Engine,
}

impl Component for DockedPage {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let key = self.key;
        // The splitter owns the ratio while it is dragged; the browser keeps a
        // copy so the next toolbox opens at the same height.
        let share = use_state(|| self.toolbox.page_share);
        use_side_effect(move || {
            let now = share();
            if (ctx.browser.peek().devtools_page_share - now).abs() > f32::EPSILON {
                ctx.with(|browser| browser.set_page_share(key, now));
            }
        });
        let toolbox = web_view(
            self.backend.backend().clone(),
            self.session,
            self.surface.0.clone(),
        )
        .on_resize(move |size: ViewSize| {
            ctx.with(|browser| {
                browser.resize_toolbox(
                    key,
                    size.width.round() as u32,
                    size.height.round() as u32,
                    size.scale as f32,
                )
            });
        })
        // The browser's shortcuts work while the toolbox has the keys, as they
        // do over a page: F12 closes it, Ctrl+T opens a tab. Editing commands
        // are the engine's own in the toolbox, so they are not taken: the
        // browser would send them to the page of the tab instead.
        .on_host_key(move |input: KeyInput| {
            match ctx.platform.page_command(&input.key, input.modifiers) {
                Some(Command::Edit(_)) | None => false,
                Some(command) => {
                    ctx.with(|browser| browser.run(command));
                    true
                }
            }
        });
        let docked: Element = split_pane(self.page.clone(), toolbox, share)
            .vertical()
            .into();
        docked
    }
}
