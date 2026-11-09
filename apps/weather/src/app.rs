//! The window root: the theme, the overlays, and the settings being read.

use bingux_ui::prelude::*;
use bingux_ui::task::use_resource;
use freya::prelude::*;

use crate::load;
use crate::shell::Shell;

/// The window root.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

/// Waits for the settings, then is the window.
#[derive(PartialEq)]
struct Content;

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let boot = use_resource((), load::boot);
        match boot.with(|state| state.value().cloned()) {
            Some(boot) => Shell { boot }.into_element(),
            // The settings are a small file: this frame is the first, and the
            // window's colour is all there is to show for it.
            None => rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                .into_element(),
        }
    }
}
