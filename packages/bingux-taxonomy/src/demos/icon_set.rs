//! The Icons page: every SVG in `packages/bingux-ui/icons`, searchable.
//!
//! One card per file, from `icon_table::ICON_FILES` (written by `build.rs`
//! from the directory, so a file added to `icons/` shows up here). A card
//! shows the file's name and its `Icon` variant, where the label sheet says
//! the icon is used and which fork glyph and weight it is drawn from, and the
//! glyph at the six sizes the icon sheet uses (12, 14, 16, 18, 20 and 48 px)
//! on a light and a dark background. Pressing a card copies `Icon::Name`.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::icon_table::{ICON_FILES, IconFile};
use crate::model::Spec;

/// The sizes the icon sheet draws, in pixels.
pub const SIZES: [u32; 6] = [12, 14, 16, 18, 20, 48];

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
        let mut group = use_state(|| "all".to_string());
        let needle = query.read().to_lowercase();
        let wanted = group.read().clone();
        let mut groups: Vec<&str> = ICON_FILES
            .iter()
            .map(|file| file.group)
            .filter(|group| !group.is_empty())
            .collect();
        groups.sort();
        groups.dedup();
        let mut options = vec![("all".to_string(), "All groups".to_string())];
        options.extend(
            groups
                .iter()
                .map(|name| (name.to_string(), name.to_string())),
        );
        options.push(("unlisted".to_string(), "In no group".to_string()));
        let shown: Vec<usize> = ICON_FILES
            .iter()
            .enumerate()
            .filter(|(_, file)| match wanted.as_str() {
                "all" => true,
                "unlisted" => file.group.is_empty(),
                name => file.group == name,
            })
            .filter(|(_, file)| matches(file, &needle))
            .map(|(index, _)| index)
            .collect();
        let with_variant = ICON_FILES
            .iter()
            .filter(|file| file.variant.is_some())
            .count();
        let mut column = vstack(S3)
            .tw(&tokens, &[w_full()])
            .child(
                hstack(S2)
                    .child(search_field(query, |_| {}).fill_width())
                    .child(
                        dropdown(options, wanted, move |key: String| group.set(key))
                            .title("Group"),
                    ),
            )
            .child(caption(format!(
                "{} of {} icon files shown; {} have an Icon variant. Press a card to copy its name.",
                shown.len(),
                ICON_FILES.len(),
                with_variant
            )));
        if shown.is_empty() {
            column = column.child(status(Status::Empty("No icon matches.".into())));
        }
        for index in shown {
            column = column.child(IconCard { index });
        }
        column
    }
}

/// Whether `file` matches the search text: its file name, variant, where it
/// is used, its metaphor or its glyph.
fn matches(file: &IconFile, needle: &str) -> bool {
    needle.is_empty()
        || [
            file.file,
            file.variant.unwrap_or_default(),
            file.place,
            file.metaphor,
            file.glyph,
            file.group,
        ]
        .iter()
        .any(|text| text.to_lowercase().contains(needle))
}

/// What pressing a card copies: the Rust name, or the file when the set has
/// no variant for it.
fn copy_text(file: &IconFile) -> String {
    match file.variant {
        Some(variant) => format!("Icon::{variant}"),
        None => format!("icons/{}-symbolic.svg", file.file),
    }
}

#[derive(PartialEq)]
struct IconCard {
    index: usize,
}

impl Component for IconCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let mut hovering = use_state(|| false);
        let file = &ICON_FILES[self.index];
        let text = copy_text(file);
        let copied = text.clone();
        let accent = tokens.accent_name;
        let light = Tokens::new(accent, Scheme::PreferLight, false);
        let dark = Tokens::new(accent, Scheme::PreferDark, false);
        let info = vstack(S1)
            .tw(&tokens, &[w(Dim::FieldWide), max_w(Dim::FieldWide)])
            .child(
                typography::EMPHASIS
                    .label(&tokens, file.file.to_string())
                    .max_lines(1),
            )
            .child(
                typography::CODE
                    .in_ink(if file.variant.is_some() {
                        Ink::Accent
                    } else {
                        Ink::Muted
                    })
                    .label(
                        &tokens,
                        file.variant.map_or("no Icon variant".to_string(), |name| {
                            format!("Icon::{name}")
                        }),
                    ),
            )
            .child(
                typography::CAPTION
                    .in_ink(Ink::Muted)
                    .paragraph(
                        &tokens,
                        if file.place.is_empty() {
                            "Not in labels.tsv"
                        } else {
                            file.place
                        },
                    )
                    .max_lines(3),
            )
            .child(
                typography::CAPTION
                    .in_ink(Ink::Muted)
                    .paragraph(&tokens, format!("{}{}", glyph_line(file), group_line(file)))
                    .max_lines(3),
            );
        let panels = hstack(S2)
            .child(sizes_panel(file, &light))
            .child(sizes_panel(file, &dark));
        let card = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    wrap(S3),
                    gap(S3),
                    p(S3),
                    rounded(Radius::Card),
                    surface(Paint::Card, hovering(), false, false),
                    border(Line::Hairline),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_| {
                if freya::clipboard::Clipboard::set(copied.clone()).is_ok() {
                    overlays.toast(format!("Copied {copied}"));
                }
            })
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Copy {text}"))
            .child(info)
            .child(panels);
        card
    }
}

fn glyph_line(file: &IconFile) -> String {
    if file.glyph.is_empty() {
        String::new()
    } else {
        format!("Glyph: {}", file.glyph)
    }
}

fn group_line(file: &IconFile) -> String {
    if file.group.is_empty() {
        String::new()
    } else {
        format!(" · sheet: {}", file.group)
    }
}

/// The glyph at every size on one background, each with its size under it.
fn sizes_panel(file: &IconFile, shown_in: &Tokens) -> Element {
    let ink = Paint::Text.color(&shown_in);
    let mut line = rect().tw(
        shown_in,
        &[
            flex_row(),
            items(Align::End),
            gap(S3),
            p(S3),
            rounded(Radius::Control),
            bg(Paint::Bg),
            border(Line::Hairline),
        ],
    );
    for size in SIZES {
        line = line.child(
            vstack(S1)
                .tw(shown_in, &[items(Align::Center), justify(Align::End)])
                .child(
                    SvgViewer::new(file.svg)
                        .width(Size::px(size as f32))
                        .height(Size::px(size as f32))
                        .fill(ink),
                )
                .child(
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(shown_in, size.to_string()),
                ),
        );
    }
    line.into_element()
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
