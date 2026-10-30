//! A taxonomy page: every entry on it, each state of each entry in a stage.

use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, CAPTION, CODE, EMPHASIS, Ink};
use freya::prelude::*;

use crate::env::{Stage, kind_tag, use_env};
use crate::model::{Body, Entry, PageId, Spec};
use crate::registry::ENTRIES;

/// The doc file every entry links into.
pub const DOC_FILE: &str = "docs/bingux-ui.md";

/// `child` under a key: when the window moves to another page or entry, the
/// components below are rebuilt instead of reused. A specimen's hooks are its
/// own, so reusing one specimen's component for another's (same type, same
/// place in the tree, different demo) breaks Freya's hook order and panics.
pub fn keyed(key: impl std::hash::Hash, child: impl Into<Element>) -> Element {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    rect()
        .key(hasher.finish())
        .width(Size::fill())
        .child(child)
        .into()
}

/// The entries of one sidebar page, one after the other.
#[derive(PartialEq)]
pub struct PageView {
    pub page: PageId,
}

impl Component for PageView {
    fn render(&self) -> impl IntoElement {
        let info = self.page.info();
        let mut column = page_column();
        for entry in &info.entries {
            let entry = index_of(entry);
            column = column.child(keyed(entry, EntryView { entry }));
        }
        column
    }
}

/// The position of `entry` in the registry.
pub fn index_of(entry: &Entry) -> usize {
    ENTRIES
        .iter()
        .position(|candidate| std::ptr::eq(candidate, entry))
        .unwrap_or_default()
}

/// One entry: its name, its Rust API, its doc anchor, then the states.
#[derive(PartialEq)]
struct EntryView {
    entry: usize,
}

impl Component for EntryView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let entry = &ENTRIES[self.entry];
        let doc = format!("{DOC_FILE}#{}", entry.doc);
        let copy_doc = doc.clone();
        let covers = entry.covers.join(", ");
        // Beside the copy button on a desktop window; under it on a phone, where the button's
        // path leaves the list of what the entry covers a column of letters.
        let (docs_row, docs_widths, covers_width) = if is_compact() {
            (vstack(S2), vec![w_full()], w_full())
        } else {
            (hstack(S2), vec![w_full(), grow_children()], w_flex())
        };
        let mut column = vstack(S3)
            .tw(&tokens, &[w_full()])
            .child(heading(entry.name))
            // The API and what an entry covers are read in full, and copied
            // from: they wrap, never end in an ellipsis.
            .child(Element::from(
                bingux_ui::controls::select_text(entry.api)
                    .style(CODE)
                    .breaks_anywhere(),
            ))
            .child(
                docs_row
                    .tw(&tokens, &docs_widths)
                    .child(
                        action_button(doc, move |_| {
                            // The anchor is a path: copy it so it can be pasted
                            // into an editor, a terminal or a review.
                            if freya::clipboard::Clipboard::set(copy_doc.clone()).is_ok() {
                                overlays.toast(format!("Copied {copy_doc}"));
                            }
                        })
                        .flat()
                        .compact()
                        .icon(Icon::EditCopy),
                    )
                    .child(
                        rect().tw(&tokens, &[covers_width]).child(Element::from(
                            bingux_ui::controls::select_text(format!("Covers {covers}"))
                                .style(CAPTION.in_ink(Ink::Muted))
                                .breaks_anywhere(),
                        )),
                    ),
            );
        match entry.body {
            Body::States(specs) => {
                for (index, spec) in specs.iter().enumerate() {
                    column = column.child(keyed(
                        (self.entry, index),
                        StateRow {
                            entry: self.entry,
                            spec: index,
                        },
                    ));
                }
            }
            Body::Custom(build) => {
                column = column.child(keyed((self.entry, "custom"), Custom { build }));
            }
        }
        column
    }
}

/// A custom body: built once, in its own scope so it may hold state.
#[derive(PartialEq)]
struct Custom {
    build: fn() -> Element,
}

impl Component for Custom {
    fn render(&self) -> impl IntoElement {
        (self.build)()
    }
}

/// One state, shown at desktop and at phone density, and in light and dark
/// when the theme switcher says Both.
#[derive(PartialEq)]
struct StateRow {
    entry: usize,
    spec: usize,
}

impl Component for StateRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let env = *use_env().read();
        let Body::States(specs) = ENTRIES[self.entry].body else {
            return rect().into_element();
        };
        let spec: &Spec = &specs[self.spec];
        let compact_window = is_compact();
        // The stages stack. A wrapping row of them came out shorter than the
        // stages in it (a banner that wraps to two lines on the phone stage),
        // so the next state's heading was laid over the stage above it.
        let mut stages = vstack(S3).tw(&tokens, &[w_full()]);
        for dark in env.variants() {
            let scheme = match dark {
                None => "",
                Some(false) => " · light",
                Some(true) => " · dark",
            };
            // A phone window has no room for the desktop stage: it shows the
            // phone stage. Switch the density to Desktop to see the other.
            for compact in [false, true] {
                if compact_window && !compact {
                    continue;
                }
                let label = format!("{}{scheme}", if compact { "Phone" } else { "Desktop" });
                stages = stages.child(
                    vstack(S1)
                        .child(CAPTION.in_ink(Ink::Muted).label(&tokens, label))
                        .child(keyed(
                            (self.entry, self.spec, compact, *dark),
                            Stage {
                                entry: self.entry,
                                spec: self.spec,
                                compact,
                                dark: *dark,
                            },
                        )),
                );
            }
        }
        vstack(S2)
            .tw(&tokens, &[w_full()])
            .child(
                // On a phone the tag goes under the name: beside a long one it is a column of letters.
                (if is_compact() { vstack(S1) } else { hstack(S2) })
                    .child(EMPHASIS.label(&tokens, spec.label))
                    .child(
                        BODY.in_ink(match spec.kind {
                            crate::model::Kind::Live => Ink::Accent,
                            crate::model::Kind::Forced => Ink::Muted,
                        })
                        .label(&tokens, kind_tag(spec.kind)),
                    ),
            )
            .child(stages)
            .into_element()
    }
}
