//! The font subpage: every installed family, narrowed by a category filter and
//! by typing. Each row sets its name in that font, with a sample beneath it,
//! and the chosen font is ticked. The first row restores the system font.
//!
//! Fontconfig reports spacing exactly, so monospace is `:spacing=100`. It does
//! not report serif or sans, so those come from the family name: a word such
//! as "Serif" or "Sans", or a family from the short lists below. Anything else
//! is "Other", which holds display, script and symbol faces.

use std::collections::HashSet;
use std::time::Duration;

use freya::prelude::*;

use super::{Loaded, reload, set_font};
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, Overlays, PageHead, Status, dropdown, menu_item, page_column, row, search_field,
    skeleton_row, skeleton_row_subtitled, status, use_overlays, use_tokens,
};
use bingux_ui::prelude::use_router;
use bingux_ui::style::*;

const FILTER_ALL: &str = "all";
const FILTER_SANS: &str = "sans";
const FILTER_SERIF: &str = "serif";
const FILTER_MONOSPACED: &str = "monospaced";
const FILTER_OTHER: &str = "other";
const SAMPLE: &str = "The quick brown fox jumps over the lazy dog";
/// Placeholder font rows shown while the list loads: enough to fill a tall
/// window (a font row is 56 px), so the list does not end short of the page.
const PLACEHOLDER_ROWS: usize = 24;

/// The search text the page starts with. Snapshots set
/// `BINGUX_SETTINGS_SNAPSHOT_FONT_SEARCH` so the search can be captured
/// without typing; a real window always starts empty.
fn initial_query() -> String {
    #[cfg(feature = "snapshot")]
    {
        std::env::var("BINGUX_SETTINGS_SNAPSHOT_FONT_SEARCH").unwrap_or_default()
    }
    #[cfg(not(feature = "snapshot"))]
    {
        String::new()
    }
}

/// The category the page starts with. Snapshots set
/// `BINGUX_SETTINGS_SNAPSHOT_FONT_FILTER` so a filter can be captured without
/// clicking; a real window always starts on All.
fn initial_filter() -> String {
    #[cfg(feature = "snapshot")]
    {
        std::env::var("BINGUX_SETTINGS_SNAPSHOT_FONT_FILTER")
            .unwrap_or_else(|_| FILTER_ALL.to_string())
    }
    #[cfg(not(feature = "snapshot"))]
    {
        FILTER_ALL.to_string()
    }
}

/// Serif families whose names do not say so. A word must match exactly, so
/// "lora" does not match "Flora".
const SERIF_NAMES: &[&str] = &[
    "bodoni",
    "caladea",
    "cambria",
    "charter",
    "cormorant",
    "crimson",
    "garamond",
    "georgia",
    "libertinus",
    "literata",
    "lora",
    "merriweather",
    "palatino",
    "playfair",
    "spectral",
    "stix",
    "tinos",
    "vollkorn",
    "baskerville",
    "times new roman",
];

/// Sans families whose names do not say so.
const SANS_NAMES: &[&str] = &[
    "arial",
    "arimo",
    "cabin",
    "calibri",
    "cantarell",
    "carlito",
    "comfortaa",
    "futura",
    "helvetica",
    "lato",
    "lexend",
    "montserrat",
    "nunito",
    "oxygen",
    "poppins",
    "quicksand",
    "raleway",
    "roboto",
    "tahoma",
    "trebuchet",
    "ubuntu",
    "verdana",
    "work sans",
];

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Class {
    Sans,
    Serif,
    Monospace,
    Other,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Font {
    name: String,
    class: Class,
}

/// Every installed family, sorted by name.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Catalogue {
    fonts: Vec<Font>,
}

/// Lists fontconfig families for a pattern, sorted and without repeats.
fn family_names(pattern: &str) -> Result<Vec<String>, CommandError> {
    let text = system::run("fc-list", &[pattern, "family"], Duration::from_secs(5))?;
    let mut families: Vec<String> = text
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    Ok(families)
}

/// Whether `lower` names one of `names`: a multi-word name as a phrase, and a
/// single word as a whole word of the family name.
fn named(lower: &str, names: &[&str]) -> bool {
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    names.iter().any(|name| {
        if name.contains(' ') {
            lower.contains(name)
        } else {
            words.contains(name)
        }
    })
}

/// The class of a family: monospace from its spacing, otherwise from its name.
fn classify(name: &str, monospaced: bool) -> Class {
    if monospaced {
        return Class::Monospace;
    }
    let lower = name.to_lowercase();
    let serif_word = named(&lower, &["serif"]);
    let sans_word = named(&lower, &["sans"]);
    if serif_word && !sans_word {
        Class::Serif
    } else if sans_word {
        Class::Sans
    } else if named(&lower, SERIF_NAMES) {
        Class::Serif
    } else if named(&lower, SANS_NAMES) {
        Class::Sans
    } else {
        Class::Other
    }
}

fn installed_fonts() -> Result<Catalogue, CommandError> {
    let monospaced: HashSet<String> = family_names(":spacing=100")?.into_iter().collect();
    let fonts = family_names(":")?
        .into_iter()
        .map(|name| {
            let class = classify(&name, monospaced.contains(&name));
            Font { name, class }
        })
        .collect();
    Ok(Catalogue { fonts })
}

/// The families the filter keeps and the search matches, in the catalogue's
/// order.
fn matching<'a>(catalogue: &'a Catalogue, filter: &str, query: &str) -> Vec<&'a Font> {
    let needle = query.trim().to_lowercase();
    catalogue
        .fonts
        .iter()
        .filter(|font| match filter {
            FILTER_SANS => font.class == Class::Sans,
            FILTER_SERIF => font.class == Class::Serif,
            FILTER_MONOSPACED => font.class == Class::Monospace,
            FILTER_OTHER => font.class == Class::Other,
            _ => true,
        })
        .filter(|font| needle.is_empty() || font.name.to_lowercase().contains(&needle))
        .collect()
}

/// Copies a font's name to the clipboard in the background, then says so with
/// a toast on the UI thread.
fn copy_text(overlays: Overlays, name: &str) {
    let text = name.to_string();
    background_then(
        move || freya::clipboard::Clipboard::set(text).is_ok(),
        move |copied| {
            if copied {
                overlays.toast("Font name copied.");
            } else {
                overlays.toast("The font name could not be copied.");
            }
        },
    );
}

/// One installed font: its name in its own face, a sample beneath it in the
/// same face, the tick when it is chosen, and a right-click menu to use it or
/// copy its name.
fn font_row(
    name: &str,
    chosen: &str,
    on_pick: &EventHandler<String>,
    overlays: Overlays,
) -> Element {
    let is_chosen = name == chosen;
    let pick = on_pick.clone();
    let picked = name.to_string();
    let mut use_font = menu_item("Use this font", move |_| pick.call(picked.clone()));
    if is_chosen {
        use_font = use_font.disabled();
    }
    let copied = name.to_string();
    let copy = menu_item("Copy name", move |_| copy_text(overlays, &copied));
    let pick = on_pick.clone();
    let picked = name.to_string();
    row(name)
        .font(name)
        .subtitle(SAMPLE)
        .subtitle_one_line()
        .selected(is_chosen)
        .on_press(move |_| pick.call(picked.clone()))
        .context_menu(vec![use_font, copy])
        .into()
}

/// How far above and below the viewport a font row is mounted, so that it is
/// ready when it scrolls into view.
const FONT_MARGIN: f32 = 240.0;

/// One font row of the list, laid out at its full height whether or not it is
/// mounted. The row itself is built only while it is in view or within
/// `FONT_MARGIN` of it: the list keeps its height and the scrollbar stays true,
/// and a long list does not build every row in its own face.
#[derive(PartialEq)]
struct FontSlot {
    name: String,
    chosen: String,
    on_pick: EventHandler<String>,
    overlays: Overlays,
}

impl Component for FontSlot {
    fn render(&self) -> impl IntoElement {
        let mut near = use_state(|| false);
        let height = Dim::RowSubtitle.px();
        let probe_height = height + FONT_MARGIN * 2.0;
        // An empty box, taller than the row by the margin above and below it,
        // reports when the row comes near the viewport. It is out of flow, so
        // it takes no room.
        let probe = rect()
            .position(Position::new_absolute().top(-FONT_MARGIN).left(0.0))
            .width(Size::fill())
            .height(Size::px(probe_height))
            .on_visible(move |_| near.set_if_modified(true))
            .on_hidden(move |_| near.set_if_modified(false));
        let shown = *near.read();
        rect()
            .width(Size::fill())
            .height(Size::px(height))
            .child(probe)
            .maybe_child(
                shown.then(|| font_row(&self.name, &self.chosen, &self.on_pick, self.overlays)),
            )
    }
}

#[derive(PartialEq)]
pub struct FontPage;

impl Component for FontPage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Loaded>> = system::cached("appearance");
        let router = use_router();
        let overlays: Overlays = use_overlays();
        let tokens = use_tokens();
        let fonts = system::use_cached_persistent("fonts", installed_fonts);
        let filter = use_state(initial_filter);
        let query = use_state(initial_query);

        // The font in the settings file, or None while that is loading. `shown`
        // is the choice on screen, which leads the file while a change saves.
        let saved: Option<String> = match model.read().as_ref() {
            Some(Ok(data)) => Some(data.font.clone()),
            _ => None,
        };
        let mut shown = use_state(|| saved.clone().unwrap_or_default());
        use_side_effect_with_deps(&saved, move |saved: &Option<String>| {
            if let Some(font) = saved {
                shown.set_if_modified(font.clone());
            }
        });

        let catalogue: Option<Catalogue> = match fonts.read().clone() {
            Some(Ok(catalogue)) => Some(catalogue),
            Some(Err(failure)) => {
                return page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(failure.0)));
            }
            None => None,
        };

        let chosen = shown.read().clone();
        // The value to go back to if a change fails: the saved font, or the
        // choice on screen while the settings file has not loaded.
        let previous = saved.clone().unwrap_or_else(|| chosen.clone());
        let on_pick: EventHandler<String> = (move |family: String| {
            system::optimistic_then(
                shown,
                Some(overlays),
                previous.clone(),
                family.clone(),
                move || set_font(&family).map(|_| ()),
                move || {
                    reload(model);
                    router.pop();
                },
            );
        })
        .into();

        let filter_key = filter.read().clone();
        let query_text = query.read().clone();
        let loaded = catalogue.is_some();
        let found: Vec<&Font> = catalogue
            .as_ref()
            .map(|catalogue| matching(catalogue, &filter_key, &query_text))
            .unwrap_or_default();
        let mut names: Vec<String> = found.iter().map(|font| font.name.clone()).collect();
        let searching = !query_text.trim().is_empty();
        // Before anything is typed, the current choice comes first, so it is in
        // view whichever part of a long list it belongs to.
        if !searching && let Some(at) = names.iter().position(|name| *name == chosen) {
            let name = names.remove(at);
            names.insert(0, name);
        }

        // The list keeps its shape while the fonts load: the System default row,
        // then rows with a sample line under the name, as the loaded list has.
        let mut rows: Vec<Element> = Vec::new();
        if !loaded {
            rows.push(skeleton_row());
            rows.extend((0..PLACEHOLDER_ROWS).map(|_| skeleton_row_subtitled()));
        }
        if loaded && !searching && filter_key == FILTER_ALL {
            let pick = on_pick.clone();
            rows.push(
                row("System default")
                    .selected(chosen.is_empty())
                    .on_press(move |_| pick.call(String::new()))
                    .into(),
            );
        }
        rows.extend(names.iter().map(|name| {
            Element::from(FontSlot {
                name: name.clone(),
                chosen: chosen.clone(),
                on_pick: on_pick.clone(),
                overlays,
            })
        }));
        let mut list = Group::new(None::<String>).rows(rows);
        if loaded && names.is_empty() {
            list = list.row(status(Status::Empty("Nothing matches.".to_string())));
        }

        let filter_options = vec![
            (FILTER_ALL.to_string(), "All".to_string()),
            (FILTER_SANS.to_string(), "Sans serif".to_string()),
            (FILTER_SERIF.to_string(), "Serif".to_string()),
            (FILTER_MONOSPACED.to_string(), "Monospace".to_string()),
            (FILTER_OTHER.to_string(), "Other".to_string()),
        ];
        let toolbar = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                    grow_children(),
                ],
            )
            .child(
                rect().tw(&tokens, &[flex_row(), w_flex()]).child(
                    search_field(query, |_: String| {})
                        .placeholder("Search for a font")
                        .fill_width(),
                ),
            )
            .child(dropdown(filter_options, filter_key, move |key: String| {
                let mut filter = filter;
                filter.set(key);
            }));

        page_column()
            .child(PageHead::new())
            .child(toolbar)
            .child(list)
    }
}
