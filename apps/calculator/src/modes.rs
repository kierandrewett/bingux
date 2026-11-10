//! What each mode adds around the display and the keys: the base, word and
//! bits of Programmer, the units of Convert.

use bingux_calc::units;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::Calc;
use crate::pads;

/// The rows of the bit grid are shorter than a key of the main pads.
const BIT_HEIGHT: Dim = Dim::Box7;

/// The base and word controls, and the bits. The value in each base is part
/// of the display above.
#[derive(PartialEq)]
pub struct ProgrammerPanel {
    pub calc: Calc,
}

impl Component for ProgrammerPanel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let calc = self.calc.clone();
        calc.watch();
        let answer = calc.int_answer();
        let (word, base) = calc.with(|s| (s.word(), s.base()));

        let bases: Vec<(String, String)> = [(16u32, "HEX"), (10, "DEC"), (8, "OCT"), (2, "BIN")]
            .iter()
            .map(|(value, name)| (value.to_string(), name.to_string()))
            .collect();
        let base_calc = calc.clone();
        // boundary: value, the base the keypad types in; it edits the calculation.
        let base_switch = segmented(bases, &base.to_string(), move |value: String| {
            base_calc.set_base(value.parse().unwrap_or(10))
        });

        let sizes: Vec<(String, String)> = [8u32, 16, 32, 64]
            .iter()
            .map(|b| (b.to_string(), b.to_string()))
            .collect();
        let size_calc = calc.clone();
        let signed_calc = calc.clone();
        let (bits, signed) = (word.bits, word.signed);
        // boundary: value, the word the value wraps at; it edits the calculation.
        let word_switch = segmented(sizes, &bits.to_string(), move |value: String| {
            let bits = value.parse().unwrap_or(64);
            size_calc.set_word(bits, signed);
        });
        let signed_switch = hstack(S2)
            .child(bingux_ui::typography::BODY.label(&tokens, tr!("calculator-signed")))
            .child(switch(signed, move |on: bool| {
                signed_calc.set_word(bits, on)
            }));

        let controls = if is_compact() {
            rect()
                .width(Size::fill())
                .tw(&tokens, &[flex_col(), gap(S2)])
                .child(base_switch.fill())
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
                        .child(word_switch)
                        .child(signed_switch),
                )
        } else {
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
                .child(base_switch)
                .child(word_switch)
                .child(signed_switch)
        };

        let columns = pads::bit_columns(word);
        let rows_of_bits = (word.bits as usize).div_ceil(columns as usize);
        let grid_height =
            BIT_HEIGHT.px() * rows_of_bits as f32 + S1.px() * (rows_of_bits as f32 - 1.0).max(0.0);
        let bit_pad = keypad(columns, pads::bit_keys(&calc, &answer.bits))
            .dense()
            .refocus(calc.field_id);

        rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col(), gap(S2)])
            .child(controls)
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
        let category_row = row(tr!("calculator-convert-category")).trailing(
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
                rect()
                    .width(Size::flex(1.0))
                    .min_width(Size::px(0.0))
                    .child(
                        dropdown(unit_options.clone(), from.clone(), move |id: String| {
                            from_calc.with(|s| s.set_units(&id, &keep_to));
                            from_calc.changed();
                        })
                        .dense()
                        .width(Dim::FieldMin),
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
                                keypad_key(tr!("calculator-convert-swap"), move |_: ()| {
                                    swap_calc.with(|s| s.swap_units());
                                    swap_calc.changed();
                                })
                                .icon(Icon::Swap)
                                .name(tr!("calculator-convert-swap.a11y"))
                                .tooltip(tr!("calculator-convert-swap.a11y"))
                                .tone(KeyTone::Plain),
                            ],
                        )
                        .refocus(calc.field_id),
                    ),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .min_width(Size::px(0.0))
                    .child(
                        dropdown(unit_options, to.clone(), move |id: String| {
                            to_calc.with(|s| s.set_units(&keep_from, &id));
                            to_calc.changed();
                        })
                        .dense()
                        .width(Dim::FieldMin),
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
                            .tooltip(&tr!("calculator-rates-update.tooltip"))
                            .busy(fetching)
                            .refocus(calc.field_id),
                    ),
            );
        }
        column
    }
}
