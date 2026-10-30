//! The picker's typed entries: the hex field and the numeric boxes.
//!
//! A box shows the colour as text and takes what is typed as an edit of the
//! colour as it is typed. The text is rewritten only when the colour changes
//! to one the text does not already say, so dragging a thumb shows its numbers
//! here and typing here is never replaced under the caret. Enter, which leaves
//! the text as typed, tidies it into the colour it makes.
//!
//! A box that cannot be read says so with the danger outline, and the hex
//! field says why in the line kept free under it, so the layout never moves.
//!
//! Typing a six digit hex passes through the three and four digit forms, which
//! are colours too (and the four digit one has an opacity). So every entry is
//! read as an edit of the colour the typing began on, not of the one the last
//! keystroke made: `#ff0000` typed over an opaque blue is an opaque red, and
//! not the transparent one `#ff00` was on the way.

use freya::prelude::*;

use std::cell::Cell;
use std::rc::Rc;

use super::core::Link;
use super::geometry::Geometry;
use super::model::{Hsva, Mode, alpha_from_percent, alpha_percent, parse_hex};
use crate::component::ComponentName;
use crate::controls::segmented::segmented;
use crate::controls::text_field::text_field;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// Whether `text` already says `colour`, as the hex field reads it.
fn says(text: &str, colour: Hsva) -> bool {
    colour
        .edited_by_hex(text)
        .is_ok_and(|read| read.colour() == colour.colour())
}

/// The hex field for `colour`, with the line under it that says what is wrong.
#[derive(PartialEq)]
pub(super) struct HexField {
    pub colour: Hsva,
    pub link: Link,
    pub geometry: Geometry,
}

impl Component for HexField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let alpha = self.link.alpha();
        let colour = self.colour;
        let text = use_state(|| colour.to_hex(alpha));
        // The colour the typing began on; none while the text is not being typed.
        let began = use_hook(|| Rc::new(Cell::new(None::<Hsva>)));
        let latest = self.link.clone();
        let forgetting = began.clone();
        use_side_effect_with_deps(&colour.colour(), move |_: &Color| {
            let now = latest.current();
            if !says(text.peek().as_str(), now) {
                let mut text = text;
                text.set(now.to_hex(alpha));
                // Something else made the colour: a new entry starts from it.
                forgetting.set(None);
            }
        });
        let verdict = parse_hex(&text.read());
        let typed = self.link.clone();
        let submitted = self.link.clone();
        let typing = began.clone();
        let finished = began;
        let message = verdict.err().map(|error| error.message());
        rect()
            .component("ColourHex")
            .tw(&tokens, &[w_flex(), flex_col()])
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt("Hex colour")
            .child(
                text_field(text, move |_: String| {
                    // Enter tidies what was typed into the colour it makes.
                    let mut text = text;
                    text.set(submitted.current().to_hex(alpha));
                    finished.set(None);
                    submitted.settle();
                })
                .on_change(move |shown: String| {
                    let start = typing.get().unwrap_or_else(|| typed.current());
                    typing.set(Some(start));
                    if let Ok(read) = start.edited_by_hex(&shown) {
                        typed.set(read);
                    }
                })
                .monospace()
                .danger_outline(message.is_some())
                .placeholder("#rrggbb")
                .fill_width(),
            )
            .child(
                rect()
                    .component("ColourHexMessage")
                    .tw(&tokens, &[w_full(), h(Dim::CaptionLine), flex_row(), items(Align::Center)])
                    .a11y_role(AccessibilityRole::Status)
                    .a11y_alt(message.clone().unwrap_or_default())
                    .maybe_child(message.map(|message| {
                        CAPTION
                            .in_ink(Ink::Danger)
                            .label(&tokens, message)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                    })),
            )
    }
}

/// One numeric box: a whole number in `0..=max`, with its letter under it.
#[derive(PartialEq)]
struct NumberBox {
    /// The box's spoken name.
    name: &'static str,
    /// The letter under it.
    letter: &'static str,
    value: i32,
    max: i32,
    /// A unit after the number, such as `%`.
    unit: Option<&'static str>,
    link: Link,
    on_edit: EventHandler<i32>,
}

impl Component for NumberBox {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let value = self.value;
        let max = self.max;
        let text = use_state(|| value.to_string());
        use_side_effect_with_deps(&value, move |value: &i32| {
            if text.peek().trim().parse::<i32>().ok() != Some(*value) {
                let mut text = text;
                text.set(value.to_string());
            }
        });
        let invalid = text.read().trim().parse::<i32>().is_err();
        let on_edit = self.on_edit.clone();
        let on_arrow = self.on_edit.clone();
        let submitted = self.link.clone();
        let mut field = text_field(text, move |_: String| {
            let mut text = text;
            text.set(value.to_string());
            submitted.settle();
        })
        .on_change(move |shown: String| {
            if let Ok(number) = shown.trim().parse::<i32>() {
                on_edit.call(number.clamp(0, max));
            }
        })
        .on_arrow(move |delta: f64| {
            // Up and Down are one, ten with Shift; Alt's tenth rounds to one.
            let step = (delta.abs().round() as i32).max(1) * if delta < 0.0 { -1 } else { 1 };
            on_arrow.call((value + step).clamp(0, max));
        })
        .monospace()
        .danger_outline(invalid)
        .fill_width();
        if let Some(unit) = self.unit {
            field = field.suffix(unit);
        }
        rect()
            .component("ColourNumber")
            .tw(&tokens, &[w_flex(), flex_col()])
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt(self.name)
            .child(field)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::CaptionLine), center()])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, self.letter)),
            )
    }
}

/// The mode switch and the boxes it chooses: RGB, HSL or HSV, and the opacity
/// box when the picker has an opacity strip.
#[derive(PartialEq)]
pub(super) struct Values {
    pub colour: Hsva,
    pub mode: Mode,
    pub link: Link,
    pub geometry: Geometry,
    pub on_mode: EventHandler<Mode>,
}

impl Component for Values {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mode = self.mode;
        let colour = self.colour;
        let numbers = mode.fields(colour);
        let maxima = mode.maxima();
        let (letters, names) = (mode.letters(), mode.names());
        let on_mode = self.on_mode.clone();
        let switch = segmented(
            Mode::ALL
                .iter()
                .map(|mode| (mode.key().to_string(), mode.label().to_string()))
                .collect(),
            mode.key(),
            move |key: String| {
                if let Some(mode) = Mode::from_key(&key) {
                    on_mode.call(mode);
                }
            },
        )
        .fill();
        let mut row = rect().component("ColourNumbers").tw(
            &tokens,
            &[w_full(), flex_row(), grow_children(), gap(S2)],
        );
        for index in 0..3 {
            let link = self.link.clone();
            let edit_link = self.link.clone();
            row = row.child(NumberBox {
                name: names[index],
                letter: letters[index],
                value: numbers[index],
                max: maxima[index],
                unit: None,
                link,
                on_edit: EventHandler::new(move |number: i32| {
                    edit_link.set(mode.edited(edit_link.current(), index, number));
                }),
            });
        }
        if self.link.alpha() {
            let edit_link = self.link.clone();
            row = row.child(NumberBox {
                name: "Opacity",
                letter: "A %",
                value: alpha_percent(colour.alpha),
                max: 100,
                unit: None,
                link: self.link.clone(),
                on_edit: EventHandler::new(move |percent: i32| {
                    edit_link.set(edit_link.current().with_alpha(alpha_from_percent(percent)));
                }),
            });
        }
        rect()
            .component("ColourValues")
            .tw(&tokens, &[w_full(), flex_col(), gap(S2)])
            .child(switch)
            .child(row)
    }
}
