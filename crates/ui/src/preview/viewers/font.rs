//! A font: its name, a specimen at several sizes, and every character it
//! draws.
//!
//! The file's bytes are handed to the text engine under a name of their own
//! (`Platform::load_font`, which makes a Skia typeface from them), so the
//! specimen is drawn by the font itself and not by a thumbnail. The viewer
//! never reads the file again: the bytes came with the facts.
//!
//! | | Quick look | Preview window |
//! | - | ---------- | -------------- |
//! | Specimen | the name and three sizes of the sample sentence | the name and six sizes, from 12 to 72 px |
//! | Sample | the pangram of the interface language | the same, or what you type in the field |
//! | Characters | a grid you can pick from; the pick shows its code point | the same, with Copy character |
//! | Install | | puts the file in `~/.local/share/fonts` and refreshes fontconfig |
//!
//! A font stays registered with the text engine until the program ends: the
//! engine has no way to forget one, and a font is registered once however
//! often it is looked at.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use bingux_preview_core::font::{self, FontFacts, Installed};
use bingux_preview_core::Loaded;
use freya::prelude::*;

use super::{Stage, tool_bar};
use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, text_field};
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// The sizes of the specimen, in px.
const GLANCE_SIZES: [f32; 3] = [16.0, 28.0, 44.0];
const DEDICATED_SIZES: [f32; 6] = [12.0, 18.0, 24.0, 36.0, 48.0, 72.0];
/// The size of the family name above the specimen.
const NAME_SIZE: f32 = 40.0;
/// The size of a character in the grid, and the side of its cell.
const GLYPH_SIZE: f32 = 22.0;
const CELL: f32 = 44.0;

/// The fonts already given to the text engine, by name.
static REGISTERED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// A name for the font that no other font has: the same bytes always have the
/// same name, so looking at a font again does not register it again.
fn alias_of(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("bingux-specimen-{:016x}", hasher.finish())
}

/// Give the font to the text engine, once, and return the name to draw it by.
fn register(data: &Arc<[u8]>) -> String {
    let alias = alias_of(data);
    let mut registered = REGISTERED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if registered.get_or_insert_with(HashSet::new).insert(alias.clone()) {
        Platform::get().load_font(alias.clone(), bytes::Bytes::copy_from_slice(data));
    }
    alias
}

/// "Regular · Version 1.0 · 1,204 glyphs": what the line above the specimen says.
fn facts_line(facts: &FontFacts) -> String {
    let mut parts = Vec::new();
    parts.extend(facts.style.clone());
    parts.extend(facts.version.clone());
    parts.push(crate::tr!("ui-preview-font-glyphs", count = u64::from(facts.glyphs)));
    if facts.faces > 1 {
        parts.push(crate::tr!("ui-preview-font-faces", count = u64::from(facts.faces)));
    }
    parts.join(" · ")
}

/// How many cells of the grid fit `width`.
pub(super) fn columns(width: f32) -> usize {
    ((width - 2.0 * S3.px()) / CELL).floor().max(1.0) as usize
}

/// "U+00E9", as Unicode writes a code point.
pub(super) fn code_point(character: char) -> String {
    format!("U+{:04X}", u32::from(character))
}

#[derive(PartialEq)]
pub(super) struct FontView {
    pub stage: Stage,
}

/// What a row of the grid draws from.
#[derive(Clone)]
struct GridData {
    characters: Arc<Vec<char>>,
    columns: usize,
    /// The list's width: the virtual list gives its items none of their own.
    width: f32,
    alias: String,
    selected: Option<char>,
    on_select: EventHandler<char>,
}

impl PartialEq for GridData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.characters, &other.characters)
            && self.columns == other.columns
            && self.width == other.width
            && self.alias == other.alias
            && self.selected == other.selected
    }
}

#[derive(PartialEq)]
struct Cell {
    character: char,
    alias: String,
    selected: bool,
    on_select: EventHandler<char>,
}

impl Component for Cell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let character = self.character;
        let on_select = self.on_select.clone();
        let fill = if self.selected {
            Paint::AccentSoft
        } else if *hovering.read() {
            Paint::Control
        } else {
            Paint::Transparent
        };
        rect()
            .component("FontCell")
            .width(Size::px(CELL))
            .height(Size::px(CELL))
            .tw(&tokens, &[flex_row(), center(), rounded(Radius::Control), bg(fill)])
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_| on_select.call(character))
            .child(
                BODY.label(&tokens, character.to_string())
                    .font_family(self.alias.clone())
                    .font_size(GLYPH_SIZE)
                    .max_lines(1),
            )
    }
}

#[derive(PartialEq)]
struct GridRow {
    data: GridData,
    index: usize,
}

impl Component for GridRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let start = self.index * self.data.columns;
        let end = (start + self.data.columns).min(self.data.characters.len());
        rect()
            .width(Size::px(self.data.width))
            .tw(&tokens, &[flex_row(), px(S3)])
            .children(self.data.characters[start..end].iter().map(|character| {
                Element::from(Cell {
                    character: *character,
                    alias: self.data.alias.clone(),
                    selected: self.data.selected == Some(*character),
                    on_select: self.data.on_select.clone(),
                })
            }))
    }
}

/// Put the font in the person's font folder off the UI thread and say so.
fn install(facts: &FontFacts, name: String, mut installed: State<bool>, overlays: Option<crate::shell::Overlays>) {
    let Some(dir) = font::user_font_dir() else {
        if let Some(overlays) = overlays {
            overlays.toast(crate::tr!("ui-preview-font-install-failed"));
        }
        return;
    };
    let data = facts.data.clone();
    let family = facts.family.clone();
    spawn(async move {
        let result = crate::task::spawn_blocking(move || {
            let done = font::install(data.bytes(), &name, &dir)?;
            if matches!(done, Installed::New(_)) {
                font::refresh_cache(&dir);
            }
            Ok::<_, std::io::Error>(done)
        })
        .await;
        match result {
            Ok(_) => {
                installed.set(true);
                if let Some(overlays) = overlays {
                    overlays.toast(crate::tr!("ui-preview-font-installed", family = family));
                }
            }
            Err(_) => {
                if let Some(overlays) = overlays {
                    overlays.toast(crate::tr!("ui-preview-font-install-failed"));
                }
            }
        }
    });
}

impl Component for FontView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let resolved = &self.stage.resolved;
        let Loaded::Font(facts) = &resolved.loaded else {
            return self.stage.card().into_element();
        };
        let dedicated = self.stage.dedicated();
        let overlays = use_try_consume::<crate::shell::Overlays>();
        let alias = use_hook({
            let bytes = facts.data.bytes().clone();
            move || register(&bytes)
        });
        let file_name = resolved.source.name();
        let installed = use_state({
            let facts = facts.clone();
            let file_name = file_name.clone();
            move || {
                font::user_font_dir().is_some_and(|dir| font::is_installed(facts.data.bytes(), &file_name, &dir))
            }
        });
        let sample = use_state(String::new);
        let mut selected = use_state(|| None::<char>);
        let mut width = use_state(|| 0.0f32);

        // Typing in the sample field keeps its keys from the preview.
        let field_id = use_a11y();
        let field_focus = use_focus(field_id);
        if let Some(claim) = use_try_consume::<super::super::keys::KeyClaim>() {
            claim.set(field_focus.read().is_focused());
        }

        let typed = sample.read().clone();
        let text = if typed.trim().is_empty() {
            crate::tr!("ui-preview-font-pangram")
        } else {
            typed
        };
        let sizes: &[f32] = if dedicated { &DEDICATED_SIZES } else { &GLANCE_SIZES };
        let specimen = sizes.iter().map(|size| {
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), items(Align::Center), gap(S3)])
                    .child(
                        rect().width(Size::px(S6.px())).child(
                            CAPTION
                                .in_ink(Ink::Muted)
                                .label(&tokens, format!("{size:.0}"))
                                .max_lines(1)
                                .text_align(TextAlign::Right),
                        ),
                    )
                    .child(
                        rect().tw(&tokens, &[w_flex(), clip()]).child(
                            BODY.label(&tokens, text.clone())
                                .font_family(alias.clone())
                                .font_size(*size)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                    ),
            )
        });

        let tools: Vec<Element> = if dedicated {
            let install_now = *installed.read();
            let facts_for_install = facts.clone();
            let install_name = file_name.clone();
            vec![
                Element::from(
                    text_field(sample, |_: String| {})
                        .placeholder(crate::tr!("ui-preview-font-sample-placeholder"))
                        .dense()
                        .clearable()
                        .a11y_id(field_id),
                ),
                Element::from(
                    action_button(
                        if install_now {
                            crate::tr!("ui-preview-font-installed-button")
                        } else {
                            crate::tr!("ui-preview-font-install")
                        },
                        move |_| install(&facts_for_install, install_name.clone(), installed, overlays),
                    )
                    .compact()
                    .enabled(!install_now),
                ),
            ]
        } else {
            Vec::new()
        };

        // The grid of characters.
        let characters = Arc::new(facts.characters.clone());
        let columns = columns(*width.read());
        let rows = characters.len().div_ceil(columns);
        let on_select = EventHandler::new(move |character: char| selected.set(Some(character)));
        let data = GridData {
            characters: characters.clone(),
            columns,
            width: *width.read(),
            alias: alias.clone(),
            selected: *selected.read(),
            on_select,
        };
        let grid = VirtualScrollView::new_with_data(data, |item, data: &GridData| {
            Element::from(GridRow {
                data: data.clone(),
                index: item.index,
            })
        })
        .length(rows)
        .item_size(CELL)
        .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()));
        let pick = *selected.read();
        let pick_line = pick.map(|character| {
            let copy = dedicated.then(|| {
                Element::from(
                    icon_button(Icon::EditCopy, move |_| {
                        crate::clipboard::Clipboard::general().write_text(character.to_string());
                        if let Some(overlays) = overlays {
                            overlays.toast(crate::tr!("ui-preview-font-character-copied"));
                        }
                    })
                    .tooltip(&crate::tr!("ui-preview-font-copy-character.tooltip")),
                )
            });
            Element::from(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                    .child(
                        BODY.label(&tokens, character.to_string())
                            .font_family(alias.clone())
                            .font_size(GLYPH_SIZE)
                            .max_lines(1),
                    )
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, code_point(character)).max_lines(1))
                    .maybe_child(copy),
            )
        });

        rect()
            .component("PreviewFont")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .child(tool_bar(&tokens, facts_line(facts), tools))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new().child(
                            rect()
                                .tw(&tokens, &[w_full(), flex_col(), p(S4), gap(S2)])
                                .child(
                                    rect().tw(&tokens, &[w_full(), pb(S2)]).child(
                                        BODY.label(&tokens, facts.family.clone())
                                            .font_family(alias.clone())
                                            .font_size(NAME_SIZE)
                                            .max_lines(1)
                                            .text_overflow(TextOverflow::Ellipsis),
                                    ),
                                )
                                .children(specimen),
                        ),
                    ),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h(Dim::Control),
                            px(S4),
                            flex_row(),
                            items(Align::Center),
                            gap(S3),
                            bg(Paint::Surface),
                        ],
                    )
                    .child(
                        rect().tw(&tokens, &[w_flex()]).child(
                            CAPTION
                                .in_ink(Ink::Muted)
                                .label(
                                    &tokens,
                                    crate::tr!("ui-preview-font-characters", count = characters.len()),
                                )
                                .max_lines(1),
                        ),
                    )
                    .maybe_child(pick_line),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(grid),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_have_the_same_name_and_others_do_not() {
        assert_eq!(alias_of(b"one"), alias_of(b"one"));
        assert_ne!(alias_of(b"one"), alias_of(b"two"));
        assert!(alias_of(b"one").starts_with("bingux-specimen-"));
    }

    #[test]
    fn the_grid_has_at_least_one_column() {
        assert_eq!(columns(0.0), 1);
        assert_eq!(columns(10_000.0), ((10_000.0 - 2.0 * S3.px()) / CELL).floor() as usize);
        assert!(columns(400.0) >= 8);
    }

    #[test]
    fn code_points_are_written_as_unicode_does() {
        assert_eq!(code_point('A'), "U+0041");
        assert_eq!(code_point('é'), "U+00E9");
        assert_eq!(code_point('😀'), "U+1F600");
    }
}
