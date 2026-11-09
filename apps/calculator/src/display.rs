//! The display: what is typed, and what it comes to.
//!
//! The entry is a text field (`bingux_ui::controls::text_field`), so the
//! caret, the selection, Copy and Paste, the touch handles and the right-click
//! menu are the toolkit's. Under it, the live result in a `Readout` that
//! scales its digits down to fit and then scrolls.

use bingux_calc::store::Mode;
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_ui::typography::{CAPTION, Ink};
use freya::prelude::*;

use crate::model::Calc;

/// The size of the result relative to the hero style.
const RESULT_SCALE: f32 = 1.5;

/// Lines kept free under the result for a note or an error.
const NOTE_LINES: f32 = 2.0;

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
            badges = badges.child(
                CAPTION
                    .label(
                        &tokens,
                        format!(
                            "{name}  {bits}-bit {}",
                            if signed { "signed" } else { "unsigned" }
                        ),
                    )
                    .max_lines(1),
            );
        }

        let copy_calc = calc.clone();
        let copy_text = shown.copy.clone();
        let copy = icon_button(Icon::EditCopy, move |_: ()| {
            copy_calc.copy_text(copy_text.clone(), overlays)
        })
        .tooltip("Copy result")
        .refocus(calc.field_id);

        let submit = calc.clone();
        let change = calc.clone();
        let escape = calc.clone();
        let field = text_field(calc.field, move |_: String| {
            // Enter is the window's: it finishes the entry whether or not the
            // field has the keyboard (see `app::Content`).
            let _ = &submit;
        })
        .fill_width()
        .a11y_id(calc.field_id)
        .edits(calc.edits)
        .placeholder(match mode {
            Mode::Programmer => "Type a number or an expression",
            Mode::Convert => "Type a value",
            _ => "Type an expression",
        })
        .on_change(move |text: String| change.edited(&text))
        .on_escape(move |_: ()| escape.clear());
        let field = if is_compact() {
            field
        } else {
            field.autofocus()
        };

        let echo_line = match (&echo, typed_empty) {
            (Some(text), true) => Some(format!("{text} =")),
            _ => None,
        };

        let result = readout(shown.text.clone())
            .scale(RESULT_SCALE)
            .spoken(shown.spoken.clone())
            .copy_text(shown.copy.clone());

        let note = error.or(shown.note.clone());
        let caption_line = CAPTION.leading * CAPTION.size.px();
        let note_lines = caption_line * NOTE_LINES;

        rect()
            .component("CalculatorDisplay")
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_col(),
                    gap(S2),
                    p(S4),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                ],
            )
            .child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            items(Align::Center),
                            justify(Align::Between),
                            grow_children(),
                        ],
                    )
                    .child(badges)
                    .child(copy),
            )
            .child(field)
            .child(
                rect()
                    .width(Size::fill())
                    .min_height(Size::px(caption_line))
                    .tw(&tokens, &[flex_row(), justify(Align::End)])
                    .maybe_child(echo_line.map(|text| CAPTION.label(&tokens, text).max_lines(1))),
            )
            .child(result)
            .child(
                rect()
                    .width(Size::fill())
                    .min_height(Size::px(note_lines))
                    .tw(&tokens, &[flex_row(), justify(Align::End)])
                    .maybe_child(note.map(|text| {
                        CAPTION
                            .in_ink(Ink::Danger)
                            .paragraph(&tokens, text)
                            .max_lines(2)
                            .text_align(TextAlign::End)
                    })),
            )
    }
}
