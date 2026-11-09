//! What each mode adds around the display and the keys: the bases and the
//! bits of Programmer, the units of Convert.

use bingux_calc::session::IntOutcome;
use bingux_calc::units;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::Calc;
use crate::pads;

/// Bases, word size and bits.
#[derive(PartialEq)]
pub struct ProgrammerPanel {
    pub calc: Calc,
}

impl Component for ProgrammerPanel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let calc = self.calc.clone();
        calc.watch();
        let text = calc.field.read().clone();
        let answer = calc.with(|s| match s.int_preview(&text) {
            IntOutcome::Value(v) => v,
            _ => s.last_int(),
        });
        let (word, base) = calc.with(|s| (s.word(), s.base()));

        let bases = [(16u32, "HEX"), (10, "DEC"), (8, "OCT"), (2, "BIN")];
        let mut rows: Vec<Element> = bases
            .iter()
            .map(|(value, name)| {
                let digits = answer
                    .rows
                    .iter()
                    .find(|(b, _)| b == value)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                let pick = calc.clone();
                let target = *value;
                row(*name)
                    .mono_value(digits)
                    .selected(base == *value)
                    .on_press(move |_: ()| pick.set_base(target))
                    .into()
            })
            .collect();
        if let Some(character) = answer.character {
            rows.push(row("Character").value(character.to_string()).into());
        }

        let sizes: Vec<(String, String)> = [8u32, 16, 32, 64]
            .iter()
            .map(|b| (b.to_string(), b.to_string()))
            .collect();
        let size_calc = calc.clone();
        let signed_calc = calc.clone();
        let (bits, signed) = (word.bits, word.signed);
        let word_rows: Vec<Element> = vec![
            row("Word size")
                .trailing(segmented(sizes, &bits.to_string(), move |value: String| {
                    let bits = value.parse().unwrap_or(64);
                    size_calc.set_word(bits, signed);
                }))
                .into(),
            switch_row("Signed", signed, move |on: bool| {
                signed_calc.set_word(bits, on)
            })
            .into(),
        ];

        let columns = pads::bit_columns(word);
        let rows_of_bits = (word.bits as usize).div_ceil(columns as usize);
        let grid_height =
            Dim::Box9.px() * rows_of_bits as f32 + S1.px() * (rows_of_bits as f32 - 1.0).max(0.0);
        let bit_pad = keypad(columns, pads::bit_keys(&calc, &answer.bits))
            .dense()
            .refocus(calc.field_id);

        rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col(), gap(S3)])
            .child(Group::new(None::<String>).rows(rows))
            .child(Group::new(None::<String>).rows(word_rows))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(grid_height))
                    .child(bit_pad),
            )
    }
}

/// Category and the two units.
#[derive(PartialEq)]
pub struct ConvertPanel {
    pub calc: Calc,
}

impl Component for ConvertPanel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let calc = self.calc.clone();
        calc.watch();
        let category = calc.with(|s| s.category());
        let (from, to) =
            calc.with(|s| (s.saved.settings.from.clone(), s.saved.settings.to.clone()));
        let is_currency = category.id == "currency";
        let swap_width = Dim::Button.px();
        let swap_height = Dim::Control.px();

        let categories: Vec<(String, String)> = units::categories()
            .iter()
            .map(|c| (c.id.to_string(), c.name.to_string()))
            .collect();
        let unit_options: Vec<(String, String)> = category
            .units
            .iter()
            .map(|u| {
                let label = if u.name == u.symbol || is_currency {
                    if is_currency {
                        format!("{} ({})", u.name, u.id)
                    } else {
                        u.name.to_string()
                    }
                } else {
                    format!("{} ({})", u.name, u.symbol)
                };
                (u.id.to_string(), label)
            })
            .collect();

        let pick_category = calc.clone();
        let category_row = row("Category").trailing(
            dropdown(categories, category.id, move |id: String| {
                pick_category.with(|s| s.set_category(&id));
                pick_category.changed();
            })
            .min_width(Dim::Field),
        );

        let from_calc = calc.clone();
        let to_calc = calc.clone();
        let (keep_to, keep_from) = (to.clone(), from.clone());
        let swap_calc = calc.clone();
        let units_row = rect()
            .width(Size::fill())
            .tw(
                &tokens,
                &[flex_row(), items(Align::Center), gap(S2), grow_children()],
            )
            .child(
                rect().width(Size::flex(1.0)).child(
                    dropdown(unit_options.clone(), from.clone(), move |id: String| {
                        from_calc.with(|s| s.set_units(&id, &keep_to));
                        from_calc.changed();
                    })
                    .dense(),
                ),
            )
            .child(
                rect()
                    .width(Size::px(swap_width))
                    .height(Size::px(swap_height))
                    .child(
                        keypad(
                            1,
                            vec![
                                keypad_key("\u{2194}\u{fe0e}", move |_: ()| {
                                    swap_calc.with(|s| s.swap_units());
                                    swap_calc.changed();
                                })
                                .name("Swap units")
                                .tone(KeyTone::Plain),
                            ],
                        )
                        .refocus(calc.field_id),
                    ),
            )
            .child(
                rect().width(Size::flex(1.0)).child(
                    dropdown(unit_options, to.clone(), move |id: String| {
                        to_calc.with(|s| s.set_units(&keep_from, &id));
                        to_calc.changed();
                    })
                    .dense(),
                ),
            );

        let mut column = rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col(), gap(S3)])
            .child(Group::new(None::<String>).rows(vec![category_row.into()]))
            .child(units_row);
        if is_currency {
            let note = calc.with(|s| s.rates_note());
            let refresh = calc.clone();
            let fetching = *calc.fetching.read();
            column = column.child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), gap(S2), grow_children()],
                    )
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            bingux_ui::typography::CAPTION
                                .paragraph(&tokens, note)
                                .max_lines(3),
                        ),
                    )
                    .child(
                        icon_button(Icon::Refresh, move |_: ()| refresh.refresh_rates())
                            .tooltip("Update rates")
                            .busy(fetching)
                            .refocus(calc.field_id),
                    ),
            );
        }
        column
    }
}
