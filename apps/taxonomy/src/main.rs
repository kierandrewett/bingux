//! Bingux Taxonomy: every component, icon and token of bingux-ui, live.
//!
//! The registry (`registry.rs`) lists what bingux-ui exports and how to see
//! it. The sidebar is the taxonomy, each page shows its entries' states side by
//! side at desktop and phone density, and the header switcher changes the
//! theme, accent, density and display scale of the whole window, so a change
//! to bingux-ui is judged in a real environment. `bingux-taxonomy --snapshot
//! DIR` renders every page to PNG without a compositor.

mod app;
mod demos;
mod env;
mod icon_table;
mod model;
mod registry;
mod snapshot_mode;
mod switcher;
#[cfg(test)]
mod tests;
mod view;

use bingux_ui::prelude::Application;

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--snapshot") {
        let dir = args
            .next()
            .unwrap_or_else(|| "/tmp/bingux-taxonomy-snapshots".to_string());
        let only: Vec<String> = args
            .next()
            .map(|list| list.split(',').map(str::to_string).collect())
            .unwrap_or_default();
        snapshot_mode::run(&dir, &only);
        return;
    }
    Application::new("bingux-taxonomy")
        .routes(app::table())
        .size(1320.0, 820.0)
        .title("Taxonomy")
        .borderless()
        .about(bingux_ui::app_info!(
            description = "Every Bingux UI component, icon and token, running",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(usage())
        .run(|| app::App);
}

/// The text `--help` prints.
fn usage() -> String {
    let mut text = String::from(
        "Usage: bingux-taxonomy [ADDRESS]\n       bingux-taxonomy --snapshot DIR [PAGE,PAGE...]\n\n\
         Addresses are CATEGORY/PAGE, for example controls/buttons. Pages:\n",
    );
    for page in model::pages() {
        let [category, title] = model::PageId(
            model::pages()
                .iter()
                .position(|candidate| std::ptr::eq(candidate, page))
                .unwrap_or_default(),
        )
        .segments();
        text.push_str(&format!("  {category}/{title}\n"));
    }
    text.push_str(
        "\nEnvironment: BINGUX_TAXONOMY_THEME=system|light|dark|both, \
         BINGUX_TAXONOMY_ACCENT=<accent>, BINGUX_TAXONOMY_DENSITY=auto|desktop|phone, \
         BINGUX_TAXONOMY_SCALE=100|125|150|200.\n",
    );
    text
}
