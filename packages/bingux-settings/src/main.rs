//! Bingux Settings: a Freya application for the Bingux desktop shell.
//!
//! Usage: `bingux-settings [PANEL]`. `PANEL` is one of the names the shell
//! passes (`network`, `bluetooth`, `sound`, `power`, `users`, `background`,
//! `display`, `desktop`, `search`) or a page name; unknown names open the
//! first page.

mod app;
mod pages;

/// Pages were written against these module paths; they map onto bingux-ui.
pub(crate) use bingux_ui::icons;
pub(crate) use bingux_ui::runtime as system;
pub(crate) use bingux_ui::theme;
pub(crate) mod ui {
    pub use bingux_ui::prelude::*;
    pub use bingux_ui::surface::use_tokens;

    use crate::pages::PageId;
    use freya::prelude::Element;

    /// The page hero for the page whose title is `text`.
    pub fn page_title(text: impl Into<String>) -> Element {
        let text = text.into();
        let page = PageId::ALL.into_iter().find(|id| id.title() == text);
        let hero = hero(text.clone());
        match page {
            Some(page) => hero
                .icon(page.icon(), page.tile_rgb())
                .description(page.description())
                .into(),
            None => hero.into(),
        }
    }
}

use freya::prelude::*;

use crate::pages::PageId;

fn main() {
    let panel = std::env::args().nth(1).unwrap_or_default();
    if panel == "--help" || panel == "-h" {
        println!("usage: bingux-settings [PANEL]");
        println!(
            "panels: {}",
            PageId::ALL.map(|id| id.panel_name()).join(", ")
        );
        return;
    }
    let initial = PageId::from_panel(&panel).unwrap_or(PageId::ALL[0]);
    let (width, height) = theme::WINDOW_SIZE;
    let (min_width, min_height) = theme::WINDOW_MIN_SIZE;
    launch(
        LaunchConfig::new()
            .with_plugin(
                freya::borderless::BorderlessPlugin::new().with_corner_radius(theme::WINDOW_RADIUS),
            )
            .with_window(
                WindowConfig::new(move || app::App { initial })
                    .with_title("Settings")
                    .with_app_id("bingux-settings")
                    .with_decorations(false)
                    .with_transparency(true)
                    .with_size(width as f64, height as f64)
                    .with_min_size(min_width as f64, min_height as f64),
            ),
    );
}
