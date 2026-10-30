//! The Icons page: every SVG in `crates/ui/icons` as one grid.
//!
//! One cell per file, from `icon_table::ICON_FILES` (written by `build.rs`
//! from the directory, so a file added to `icons/` shows up here), sorted by
//! file name: the glyph with its name under it. A segmented control picks the
//! glyph size (16 to 48 px, 24 by default) and a search field filters by name.
//! Hovering a cell shows the name in a tooltip; pressing it copies the name.
//! Under the symbolic grid, "Colour file icons" lists the colour set of the Files
//! app (`ColourIcon::ALL`, `scripts/icons/colour`) the same way, in the theme's
//! tones, with the same size control and search.

use bingux_ui::component::ComponentName;
use bingux_ui::files::{ColourIcon, colour_icon_view};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::icon_table::{ICON_FILES, IconFile};
use crate::model::Spec;

/// The glyph sizes the control offers, in pixels.
pub const SIZES: [u32; 5] = [16, 20, 24, 32, 48];
/// The size the grid opens at.
const DEFAULT_SIZE: u32 = 24;

/// The names the grid shows for `needle` (empty shows all): the file names of
/// the icon table, alphabetical.
pub fn grid_names(needle: &str) -> Vec<&'static str> {
    let needle = needle.trim().to_lowercase();
    let mut names: Vec<&'static str> = ICON_FILES
        .iter()
        .map(|file| file.file)
        .filter(|name| needle.is_empty() || name.to_lowercase().contains(&needle))
        .collect();
    names.sort_unstable();
    names
}

/// The colour icons the grid shows for `needle` (empty shows all): those whose
/// name or family contains it, in the set's order.
pub fn colour_names(needle: &str) -> Vec<ColourIcon> {
    let needle = needle.trim().to_lowercase();
    ColourIcon::ALL
        .iter()
        .copied()
        .filter(|icon| {
            needle.is_empty()
                || icon.name().contains(&needle)
                || icon.family().to_lowercase().contains(&needle)
        })
        .collect()
}

/// The custom body of the "Icon set" entry.
pub fn icon_set() -> Element {
    IconSet.into_element()
}

#[derive(PartialEq)]
struct IconSet;

impl Component for IconSet {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let query = use_state(String::new);
        let mut size = use_state(|| DEFAULT_SIZE);
        let names = grid_names(&query.read());
        let options: Vec<(String, String)> = SIZES
            .iter()
            .map(|px| (px.to_string(), px.to_string()))
            .collect();
        let mut grid = rect().tw(&tokens, &[w_full(), flex_row(), wrap(S2), gap(S2)]);
        for name in &names {
            grid = grid.child(IconCell { name, size: size() });
        }
        let mut column = vstack(S3)
            .tw(&tokens, &[w_full()])
            .child(
                hstack(S2)
                    .child(search_field(query, |_| {}).width(Dim::Field))
                    .child(segmented(options, &size().to_string(), move |key: String| {
                        if let Ok(px) = key.parse() {
                            size.set(px);
                        }
                    })),
            );
        let colour = colour_names(&query.read());
        if names.is_empty() && colour.is_empty() {
            column = column.child(status(Status::Empty("No icon matches.".into())));
        }
        column = column.child(grid);
        if !colour.is_empty() {
            let mut colours = rect().tw(&tokens, &[w_full(), flex_row(), wrap(S2), gap(S2)]);
            for icon in colour {
                colours = colours.child(ColourCell { icon, size: size() });
            }
            column = column
                .child(
                    typography::HEADING
                        .label(&tokens, "Colour file icons".to_string()),
                )
                .child(
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, "The file and folder icons of the Files app: the page, the accent and the glyph follow the theme.".to_string()),
                )
                .child(colours);
        }
        column
    }
}

/// One colour icon over its name, as [`IconCell`] shows a symbolic one.
#[derive(PartialEq)]
struct ColourCell {
    icon: ColourIcon,
    size: u32,
}

impl Component for ColourCell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let mut hovering = use_state(|| false);
        let name = self.icon.name();
        let cell = rect()
            .tw(
                &tokens,
                &[
                    w(Dim::GridTile),
                    flex_col(),
                    items(Align::Center),
                    gap(S1),
                    p(S2),
                    rounded(Radius::Control),
                    surface(Paint::Bg, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_| {
                if freya::clipboard::Clipboard::set(name.to_string()).is_ok() {
                    overlays.toast(format!("Copied {name}"));
                }
            })
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Copy {name}"))
            .child(colour_icon_view(&tokens, self.icon, self.size as f32))
            // The name is cut on purpose and the tile's tooltip shows it whole: `Elided` says so
            // to the layout checks.
            .child(
                rect().component("Elided").child(
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, name.to_string())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            );
        hover_tip(cell, format!("{name}: {}", self.icon.family()))
    }
}

/// One icon over its name. Pressing copies the name; the tooltip shows it whole
/// when the caption is cut.
#[derive(PartialEq)]
struct IconCell {
    name: &'static str,
    size: u32,
}

impl Component for IconCell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let mut hovering = use_state(|| false);
        let file: &IconFile = ICON_FILES
            .iter()
            .find(|file| file.file == self.name)
            .expect("a grid name comes from the icon table");
        let name = file.file;
        let cell = rect()
            .tw(
                &tokens,
                &[
                    w(Dim::GridTile),
                    flex_col(),
                    items(Align::Center),
                    gap(S1),
                    p(S2),
                    rounded(Radius::Control),
                    surface(Paint::Bg, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_| {
                if freya::clipboard::Clipboard::set(name.to_string()).is_ok() {
                    overlays.toast(format!("Copied {name}"));
                }
            })
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Copy {name}"))
            .child(
                SvgViewer::new(file.svg)
                    .width(Size::px(self.size as f32))
                    .height(Size::px(self.size as f32))
                    .fill(Paint::Text.color(&tokens)),
            )
            .child(
                rect().component("Elided").child(
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, name.to_string())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            );
        hover_tip(cell, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every catalogue icon is in the grid once, and nothing else is.
    #[test]
    fn grid_lists_the_catalogue_once() {
        let mut catalogue: Vec<&str> = ICON_FILES.iter().map(|file| file.file).collect();
        catalogue.sort_unstable();
        assert!(!catalogue.is_empty());
        assert_eq!(grid_names(""), catalogue);
        let mut unique = catalogue.clone();
        unique.dedup();
        assert_eq!(unique.len(), catalogue.len(), "a file name is listed twice");
    }

    /// Every colour icon is in the colour grid once, and a search finds one by
    /// its name or its family.
    #[test]
    fn colour_grid_lists_the_colour_set() {
        assert_eq!(colour_names(""), ColourIcon::ALL);
        assert_eq!(colour_names("file-pdf"), [ColourIcon::FilePdf]);
        assert!(colour_names("spreadsheet").contains(&ColourIcon::FileSpreadsheet));
        assert!(colour_names("no-such-icon-anywhere").is_empty());
    }

    #[test]
    fn filter_narrows_the_grid() {
        let first = grid_names("")[0];
        let hits = grid_names(&first.to_uppercase());
        assert!(hits.contains(&first));
        assert!(grid_names("no-such-icon-anywhere").is_empty());
    }
}

/// The "Application icons" demo: full-colour theme icons, as the dock shows.
pub const APP_ICON: &[Spec] = &[
    Spec::forced("Sizes", || {
        hstack(S3)
            .child(app_icon("org.gnome.Nautilus", IconSize::AppSmall))
            .child(app_icon("org.gnome.Nautilus", IconSize::App))
            .child(app_icon("org.gnome.Nautilus", IconSize::AppLarge))
            .child(app_icon("org.gnome.Nautilus", IconSize::Hero))
            .into_element()
    }),
    Spec::forced("Missing icon", || {
        app_icon("no-such-icon", IconSize::App).into_element()
    }),
    Spec::forced("In a row", || {
        rows(vec![
            row("Files")
                .leading_app("org.gnome.Nautilus")
                .subtitle("A leading application icon")
                .into_element(),
        ])
    }),
];
