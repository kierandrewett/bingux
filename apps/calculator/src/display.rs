//! The display: what is typed, and what it comes to, on one surface.
//!
//! The entry is a text field in its `display` form
//! (`bingux_ui::controls::text_field`): a muted line with no box around it, so
//! the caret, the selection, Copy and Paste, the touch handles and the
//! right-click menu are the toolkit's but it reads as the top line of the
//! display. After an answer the line holds the finished expression until the
//! next key. Under it, the live result in a `Readout` that scales its digits
//! down to fit and then scrolls.

use bingux_calc::store::Mode;
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, CAPTION, Ink};
use freya::prelude::*;

use crate::model::Calc;

/// The size of the result relative to the hero style.
const RESULT_SCALE: f32 = 1.5;

/// The same in Programmer, which shows the value in other bases under it.
const PROGRAMMER_SCALE: f32 = 0.75;

#[derive(PartialEq)]
pub struct Display {
    pub calc: Calc,
}

impl Component for Display {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let calc = self.calc.clone();
        let overlays = use_overlays();
        calc.watch();
        let mode = calc.with(|s| s.mode());
        let shown = calc.shown();
        let echo = calc.echo.read().clone();
        let error = calc.error.read().clone();
        let typed_empty = calc.field.read().is_empty();
        let has_memory = calc.with(|s| s.has_memory());
        let radians = calc.with(|s| s.angle()) == bingux_calc::Angle::Radians;

        // Basic and Convert have nothing to say above the entry line.
        let has_badges = matches!(mode, Mode::Scientific | Mode::Programmer);
        let mut badges = hstack(S2);
        if has_memory && mode == Mode::Scientific {
            badges = badges.child(CAPTION.in_ink(Ink::Accent).label(&tokens, "M").max_lines(1));
        }
        if mode == Mode::Scientific {
            badges = badges.child(
                CAPTION
                    .label(&tokens, if radians { "Rad" } else { "Deg" })
                    .max_lines(1),
            );
        }
        if mode == Mode::Programmer {
            let (bits, signed, base) = calc.with(|s| (s.word().bits, s.word().signed, s.base()));
            let name = match base {
                2 => "BIN",
                8 => "OCT",
                16 => "HEX",
                _ => "DEC",
            };
            let mut text = format!(
                "{name}  {bits}-bit {}",
                if signed { "signed" } else { "unsigned" }
            );
            if let Some(character) = calc.int_answer().character {
                text.push_str(&format!("  \u{b7}  Character {character}"));
            }
            badges = badges.child(CAPTION.label(&tokens, text).max_lines(1));
            // The note (a value that wrapped, an error) shares this line, as
            // the window has no height to spare.
            if let Some(note) = error.clone().or(shown.note.clone()) {
                badges = badges.child(
                    CAPTION
                        .in_ink(Ink::Danger)
                        .label(&tokens, note)
                        .max_lines(1),
                );
            }
        }

        let copy_calc = calc.clone();
        let copy_text = shown.copy.clone();
        let copy = icon_button(Icon::EditCopy, move |_: ()| {
            copy_calc.copy_text(copy_text.clone(), overlays)
        })
        .size(Dim::InlineButton)
        .tooltip("Copy result")
        .refocus(calc.field_id);

        let submit = calc.clone();
        let change = calc.clone();
        let escape = calc.clone();
        // With nothing typed after an answer the line shows what was worked
        // out, in the place the next entry will go.
        let hint = match (&echo, typed_empty) {
            (Some(text), true) => format!("{text} ="),
            _ => match mode {
                Mode::Programmer => "Number or expression",
                Mode::Convert => "Value to convert",
                _ => "Expression",
            }
            .to_string(),
        };
        let field = text_field(calc.field, move |_: String| {
            // Enter is the window's: it finishes the entry whether or not the
            // field has the keyboard (see `app::Content`).
            let _ = &submit;
        })
        .fill_width()
        .display(BODY.in_ink(Ink::Muted), TextAlign::End)
        .a11y_id(calc.field_id)
        .edits(calc.edits)
        .placeholder(hint)
        .on_change(move |text: String| change.edited(&text))
        .on_escape(move |_: ()| escape.clear());
        let field = if is_compact() {
            field
        } else {
            field.autofocus()
        };

        let programmer = mode == Mode::Programmer;
        let result = readout(shown.text.clone())
            .scale(if programmer {
                PROGRAMMER_SCALE
            } else {
                RESULT_SCALE
            })
            .spoken(shown.spoken.clone())
            .copy_text(shown.copy.clone());

        // Programmer shows the value in the bases the entry is not in, one
        // short line each, under the digits of the base it is in. A
        // 32 or 64 bit binary line would be wider than the window: the bit
        // grid below is that view.
        let base_lines = programmer.then(|| {
            let answer = calc.int_answer();
            let (base, bits) = calc.with(|s| (s.base(), s.word().bits));
            let mut lines = rect().width(Size::fill()).tw(&tokens, &[flex_col()]);
            for (value, name) in [(16u32, "HEX"), (10, "DEC"), (8, "OCT"), (2, "BIN")] {
                if value == base || (value == 2 && bits > 16) {
                    continue;
                }
                let digits = answer
                    .rows
                    .iter()
                    .find(|(b, _)| *b == value)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                lines = lines.child(
                    rect()
                        .width(Size::fill())
                        .tw(
                            &tokens,
                            &[
                                flex_row(),
                                items(Align::Center),
                                justify(Align::Between),
                                gap(S3),
                                grow_children(),
                            ],
                        )
                        .child(CAPTION.label(&tokens, name).max_lines(1))
                        .child(
                            CAPTION
                                .label(&tokens, digits)
                                .font_family(bingux_ui::typography::mono_family())
                                .max_lines(1),
                        ),
                );
            }
            lines
        });

        // A note or an error under the result takes a line of its own while
        // there is one; nothing is held back for it.
        let note = error.or(shown.note.clone());

        rect()
            .component("CalculatorDisplay")
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_col(),
                    gap(S1),
                    px(S4),
                    py(S3),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                ],
            )
            .maybe_child(has_badges.then_some(badges))
            .child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), gap(S2), grow_children()],
                    )
                    .child(rect().width(Size::flex(1.0)).child(field))
                    .child(copy),
            )
            .child(result)
            .maybe_child(base_lines)
            .maybe_child(note.filter(|_| mode != Mode::Programmer).map(|text| {
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_row(), justify(Align::End)])
                    .child(
                        CAPTION
                            .in_ink(Ink::Danger)
                            .paragraph(&tokens, text)
                            .max_lines(2)
                            .text_align(TextAlign::End),
                    )
            }))
    }
}
