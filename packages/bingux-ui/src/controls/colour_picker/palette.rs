//! The palette: one-press swatches of the system accents and a list the app
//! gives, and the row of recent colours.
//!
//! A swatch is a disc inside a ring that shows when it is the colour being
//! edited, with a check mark drawn in whichever of white and black reads on
//! it. Every swatch is a keyboard stop, at least [`Dim::PickerSwatch`] across
//! (32 px on a desktop, 44 px for a finger); the disc inside is smaller, as
//! the target around it is the control and not the picture. A press chooses the
//! colour, keeping the opacity being edited.

use freya::prelude::*;

use super::core::{Link, RECENT_MAX};
use super::geometry::{COLUMNS, Geometry};
use crate::controls::focus_stop::focus_stop;
use crate::controls::tooltip::hover_tip;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The colours a picker offers after the system accents, as hex text: white,
/// a grey and black. An app passes its own with `presets`.
pub const PRESETS: &[&str] = &["#ffffff", "#9a9996", "#000000"];

/// One swatch of the palette: a colour, and what it is called when it has a name.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub colour: Color,
    pub name: Option<String>,
}

impl Entry {
    /// What a screen reader and the tooltip call the swatch.
    pub(super) fn label(&self) -> String {
        let hex = colour_to_hex(self.colour);
        match &self.name {
            Some(name) => format!("{name}, {hex}"),
            None => hex,
        }
    }
}

/// White or black, whichever reads on `colour`: the check mark's colour.
pub(super) fn on_colour(colour: Color) -> Color {
    let linear = |channel: u8| {
        let c = channel as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b());
    // The colour whose contrast against `colour` is larger: the crossover.
    if (luminance + 0.05) / 0.05 > 1.05 / (luminance + 0.05) {
        colour_from_argb(255, 0, 0, 0)
    } else {
        colour_from_argb(255, 255, 255, 255)
    }
}

/// Whether `candidate` is the colour being edited. The opacity is the
/// editor's own, so only the colour counts.
pub(super) fn is_chosen(candidate: Color, editing: Color) -> bool {
    candidate.r() == editing.r() && candidate.g() == editing.g() && candidate.b() == editing.b()
}

/// One swatch: a disc in a ring, a keyboard stop.
#[derive(PartialEq)]
struct Swatch {
    entry: Entry,
    selected: bool,
    link: Link,
}

impl Component for Swatch {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let colour = self.entry.colour;
        let link = self.link.clone();
        let selected = self.selected;
        let label = self.entry.label();
        let disc = (Dim::PickerSwatch.px() - 2.0 * S1.px()).max(0.0);
        let face = rect()
            .component("ColourSwatch")
            .a11y_builder(move |builder| builder.set_selected(selected))
            .tw(
                &tokens,
                &[
                    w(Dim::PickerSwatch),
                    h(Dim::PickerSwatch),
                    center(),
                    rounded(Radius::Pill),
                ],
            )
            .maybe(selected, |face| {
                face.tw(&tokens, &[border(Line::Ring(Paint::Text))])
            })
            .child(
                rect()
                    .width(Size::px(disc))
                    .height(Size::px(disc))
                    .tw(
                        &tokens,
                        &[center(), rounded(Radius::Pill), fill(colour), border(Line::Hairline)],
                    )
                    .maybe_child(selected.then(|| {
                        icons::icon(Icon::Check, IconSize::Control, on_colour(colour))
                    })),
            );
        let stop = focus_stop(
            face,
            format!("Colour {label}"),
            AccessibilityRole::Button,
            true,
            Radius::Pill,
            EventHandler::new(move |_: ()| {
                let keep = link.current().alpha;
                link.set_colour(if link.alpha() { colour.with_a(keep) } else { colour });
                link.settle();
            }),
        );
        hover_tip(stop, label)
    }
}

/// An empty slot of the recent row: an outline where a colour will go.
fn slot(tokens: &crate::theme::Tokens) -> Rect {
    let disc = (Dim::PickerSwatch.px() - 2.0 * S1.px()).max(0.0);
    rect()
        .tw(
            tokens,
            &[w(Dim::PickerSwatch), h(Dim::PickerSwatch), center()],
        )
        .child(
            rect()
                .width(Size::px(disc))
                .height(Size::px(disc))
                .interactive(false)
                .opacity(0.6)
                .tw(tokens, &[rounded(Radius::Pill), border(Line::Hairline)]),
        )
}

/// Rows of swatches, [`COLUMNS`] to a row, each in a cell of equal width.
fn grid(
    tokens: &crate::theme::Tokens,
    entries: &[Entry],
    slots: usize,
    editing: Color,
    link: &Link,
) -> Rect {
    let mut column = rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S1)]);
    let total = slots.max(entries.len());
    for row in 0..total.div_ceil(COLUMNS) {
        let mut line = rect().tw(tokens, &[w_full(), flex_row(), grow_children()]);
        for position in row * COLUMNS..(row + 1) * COLUMNS {
            let cell = rect().tw(tokens, &[w_flex(), h(Dim::PickerSwatch), center()]);
            let content = match entries.get(position) {
                Some(entry) => Some(
                    Swatch {
                        selected: is_chosen(entry.colour, editing),
                        entry: entry.clone(),
                        link: link.clone(),
                    }
                    .into_element(),
                ),
                None if position < total && slots > 0 => Some(slot(tokens).into_element()),
                None => None,
            };
            line = line.child(cell.maybe_child(content));
        }
        column = column.child(line);
    }
    column
}

/// The palette: the system accents and the app's presets.
#[derive(PartialEq)]
pub(super) struct Palette {
    pub entries: Vec<Entry>,
    pub editing: Color,
    pub link: Link,
}

impl Component for Palette {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        grid(&tokens, &self.entries, 0, self.editing, &self.link).component("ColourPalette")
    }
}

/// The recent colours, newest first, in a row whose room is kept.
#[derive(PartialEq)]
pub(super) struct Recent {
    pub colours: Vec<Color>,
    pub editing: Color,
    pub link: Link,
    pub geometry: Geometry,
}

impl Component for Recent {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let entries: Vec<Entry> = self
            .colours
            .iter()
            .map(|colour| Entry {
                colour: *colour,
                name: None,
            })
            .collect();
        rect()
            .component("ColourRecent")
            .tw(&tokens, &[w_full(), flex_col()])
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::CaptionLine), flex_row(), items(Align::Center)])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, "Recent")),
            )
            .child(grid(&tokens, &entries, RECENT_MAX, self.editing, &self.link))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
        colour_from_argb(a, r, g, b)
    }

    #[test]
    fn a_check_reads_on_every_swatch() {
        let black = argb(255, 0, 0, 0);
        let white = argb(255, 255, 255, 255);
        assert_eq!(on_colour(white), black);
        assert_eq!(on_colour(black), white);
        assert_eq!(on_colour(argb(255, 0xff, 0xe6, 0x00)), black, "yellow takes black");
        assert_eq!(on_colour(argb(255, 0x1c, 0x71, 0xd8)), white, "a blue takes white");
        // Whichever it picks has the larger contrast, over the whole grid.
        let ratio = |a: Color, b: Color| {
            let l = |c: Color| {
                let f = |v: u8| {
                    let v = v as f32 / 255.0;
                    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
                };
                0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
            };
            let (hi, lo) = (l(a).max(l(b)), l(a).min(l(b)));
            (hi + 0.05) / (lo + 0.05)
        };
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let colour = argb(255, r as u8, g as u8, b as u8);
                    let picked = on_colour(colour);
                    let other = if picked == black { white } else { black };
                    assert!(ratio(colour, picked) >= ratio(colour, other), "{r},{g},{b}");
                    assert!(ratio(colour, picked) >= 4.5, "{r},{g},{b}: a check must be legible");
                }
            }
        }
    }

    #[test]
    fn a_swatch_is_chosen_by_its_colour_and_not_its_opacity() {
        let blue = argb(255, 53, 132, 228);
        assert!(is_chosen(blue, blue));
        assert!(is_chosen(blue, blue.with_a(90)));
        assert!(!is_chosen(blue, argb(255, 53, 132, 229)));
    }

    #[test]
    fn a_swatch_is_named_for_assistive_technology_by_its_hex() {
        let entry = Entry {
            colour: argb(255, 0x35, 0x84, 0xe4),
            name: Some("Blue".to_string()),
        };
        assert_eq!(entry.label(), "Blue, #3584e4");
        let plain = Entry { colour: entry.colour, name: None };
        assert_eq!(plain.label(), "#3584e4");
    }

    #[test]
    fn the_presets_are_valid_hex() {
        for hex in PRESETS {
            assert!(colour_from_hex(hex).is_some(), "{hex}");
        }
    }
}
