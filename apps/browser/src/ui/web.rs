//! The page of a tab: bingux-ui's web view control, hosted by the browser.
//!
//! The control draws the page and sends everything the person does over it to
//! the engine. The browser only shapes it, as iexplore.exe shaped IE's
//! WebBrowser control: its shortcuts come before the page's keys, it sizes
//! every tab when the window changes, and it shows a menu of its own when the
//! engine cannot say what is under the pointer.

use std::sync::Arc;

use bingux_ui::frame_view::{KeyInput, ViewSize};
use bingux_ui::web_view::{PageSurface, TabId, web_view};
use freya::prelude::*;

use crate::commands::Command;

use crate::engine::Engine;

use super::ctx::Ctx;

/// A surface, comparable by identity so it can be a prop.
#[derive(Clone)]
pub struct Surface(pub Arc<PageSurface>);

impl PartialEq for Surface {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The page of one tab.
#[derive(PartialEq)]
pub struct TabPage {
    pub ctx: Ctx,
    pub engine: Engine,
    pub key: u64,
    pub session: TabId,
    pub surface: Surface,
}

impl Component for TabPage {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let key = self.key;
        web_view(
            self.engine.backend().clone(),
            self.session,
            self.surface.0.clone(),
        )
        .on_resize(move |size: ViewSize| {
            ctx.with(|browser| {
                browser.resize(
                    size.width.round() as u32,
                    size.height.round() as u32,
                    size.scale as f32,
                )
            });
        })
        .on_host_key(move |input: KeyInput| {
            // Escape leaves a fullscreen page, whatever the page says.
            if input.key == Key::Named(NamedKey::Escape) && ctx.browser.peek().fullscreen {
                ctx.with(|browser| browser.run(Command::Fullscreen));
                return true;
            }
            // The browser's shortcuts come first: a page cannot take
            // Ctrl+T, and Ctrl+C is the engine's own copy.
            match ctx.platform.page_command(&input.key, input.modifiers) {
                Some(command) => {
                    ctx.with(|browser| browser.run(command));
                    true
                }
                None => false,
            }
        })
        .on_local_menu(move |position: (f32, f32)| {
            ctx.with(|browser| browser.open_local_menu(key, position));
        })
    }
}
