//! The bingux-ui gallery: every component, live, in a split view.
//!
//! Run with `cargo run --example gallery`.

mod pages;

use freya::prelude::*;
use pages::{Gallery, Page};

fn main() {
    let (width, height) = bingux_ui::theme::WINDOW_SIZE;
    launch(
        bingux_ui::theme::with_font(LaunchConfig::new())
            .with_plugin(freya::borderless::BorderlessPlugin::new())
            .with_window(
                WindowConfig::new(|| Gallery {
                    initial: Page::Controls,
                })
                .with_title("Bingux UI gallery")
                .with_app_id("bingux-ui-gallery")
                .with_decorations(false)
                .with_size(width as f64, height as f64),
            ),
    );
}
