//! Foundations: colour, type, space, radii, elevation, density and motion.
//!
//! Every table here walks `style::catalogue`, the one list of tokens by name,
//! so a token added there shows up with no change to this file.

use bingux_ui::{contrast, contrast_matrix};
use bingux_ui::prelude::*;
use bingux_ui::style::catalogue::{ELEVATIONS, LINES, PAINTS, RADII, SPACES, TEXT_STYLES, dims};
use bingux_ui::style::{
    Align, SizeClass, colour_from_argb, colour_from_hex, colour_from_hsv, colour_to_hex,
};
use bingux_ui::surface::{over, surface_color};
use bingux_ui::theme::{
    DEFAULT_ACCENT, NARROW_BELOW, PHONE_PREVIEW_SIZE, WINDOW_MIN_SIZE, WINDOW_SIZE, accent_for,
    accent_names, rgba,
};
use freya::prelude::*;

use bingux_ui::motion::Focusable;
use super::common::LONG;
use crate::model::Spec;

/// A name at the left of a table row, the cells after it. The name column is
/// as wide as a field when there is room and gives way before the cells do,
/// so a value never wraps a letter to a line on a narrow stage.
fn table_row(tokens: &Tokens, name: &str, cells: Vec<Element>) -> Element {
    hstack(S3)
        .tw(tokens, &[w_full(), grow_children()])
        .child(
            rect().tw(tokens, &[w_flex(), max_w(Dim::Field)]).child(
                typography::BODY
                    .label(tokens, name.to_string())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            ),
        )
        .children(cells)
        .into_element()
}

/// A small filled square in one colour.
fn chip(tokens: &Tokens, paint: Paint, shown_in: &Tokens) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w(Dim::Box7),
                h(Dim::Box7),
                rounded(Radius::Mark),
                fill(paint.color(&shown_in)),
                border(Line::Hairline),
            ],
        )
        .into_element()
}

fn light_dark() -> (Tokens, Tokens) {
    let accent = use_tokens().accent_name;
    (
        Tokens::new(accent, Scheme::PreferLight, false),
        Tokens::new(accent, Scheme::PreferDark, false),
    )
}

// ---- Colour ----------------------------------------------------------------

pub const PAINT: &[Spec] = &[
    Spec::forced("Every paint, light and dark", || {
        let tokens = use_tokens();
        let (light, dark) = light_dark();
        let mut column = vstack(S2);
        for (name, paint) in PAINTS {
            column = column.child(table_row(
                &tokens,
                name,
                vec![
                    chip(&tokens, *paint, &light),
                    chip(&tokens, *paint, &dark),
                    typography::CODE
                        .in_ink(Ink::Muted)
                        .label(&tokens, colour_to_hex(paint.color(&dark)))
                        .into_element(),
                ],
            ));
        }
        column.into_element()
    }),
    Spec::forced("Text inks", || {
        let tokens = use_tokens();
        let mut column = vstack(S1);
        for (name, ink) in [
            ("Text", Ink::Text),
            ("Muted", Ink::Muted),
            ("Accent", Ink::Accent),
            ("Danger", Ink::Danger),
            ("Success", Ink::Success),
            ("Warning", Ink::Warning),
        ] {
            column = column.child(
                typography::BODY
                    .in_ink(ink)
                    .label(&tokens, format!("{name} ink")),
            );
        }
        column.into_element()
    }),
    Spec::forced("Tones", || {
        let tokens = use_tokens();
        hstack(S4)
            .children([Tone::Accent, Tone::Warning, Tone::Danger].map(|tone| {
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Box9),
                            h(Dim::Box9),
                            rounded(Radius::Control),
                            fill(tone.color(&tokens)),
                        ],
                    )
                    .into_element()
            }))
            .into_element()
    }),
    Spec::live("Hover, pressed and selection layers", || {
        let tokens = use_tokens();
        let mut column = vstack(S1);
        for (name, hover, press, selected) in [
            ("Rest", false, false, false),
            ("Hover", true, false, false),
            ("Pressed", false, true, false),
            ("Selected", false, false, true),
            ("Selected and hover", true, false, true),
        ] {
            column = column.child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h(Dim::Control),
                            px(S3),
                            items(Align::Center),
                            flex_row(),
                            rounded(Radius::Control),
                            surface(Paint::Card, hover, press, selected),
                        ],
                    )
                    .child(typography::BODY.label(&tokens, name)),
            );
        }
        column.into_element()
    }),
];

pub const ACCENT: &[Spec] = &[Spec::forced("Nine accents, light and dark", || {
    let tokens = use_tokens();
    let mut column = vstack(S2);
    for name in accent_names() {
        let light = accent_for(name, false);
        let dark = accent_for(name, true);
        let swatch = |colour: Color| {
            rect()
                .tw(
                    &tokens,
                    &[
                        w(Dim::Box7),
                        h(Dim::Box7),
                        rounded(Radius::Pill),
                        fill(colour),
                        border(Line::Hairline),
                    ],
                )
                .into_element()
        };
        column = column.child(table_row(
            &tokens,
            &if name == DEFAULT_ACCENT {
                format!("{name} (default)")
            } else {
                name.to_string()
            },
            vec![
                swatch(light),
                swatch(dark),
                typography::CODE
                    .in_ink(Ink::Muted)
                    .label(&tokens, colour_to_hex(dark))
                    .into_element(),
            ],
        ));
    }
    column.into_element()
})];

// ---- Typography ------------------------------------------------------------

pub const TYPE_RAMP: &[Spec] = &[
    Spec::forced("The ramp, by name", || {
        let tokens = use_tokens();
        let mut column = vstack(S3);
        for (name, style) in TEXT_STYLES {
            column = column.child(
                vstack(S0_5)
                    .child(style.label(&tokens, "Wi-Fi password, 1,024 files"))
                    .child(caption(format!(
                        "{name}: {} px, weight {}",
                        style.size.px(),
                        weight_text(style.weight)
                    ))),
            );
        }
        column.into_element()
    })
    .wide(),
    Spec::forced("Long text cut with an ellipsis", || {
        let tokens = use_tokens();
        vstack(S2)
            .child(
                typography::BODY
                    .label(&tokens, LONG)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                typography::CAPTION
                    .in_ink(Ink::Muted)
                    .paragraph(&tokens, LONG)
                    .max_lines(2)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .into_element()
    }),
    Spec::forced("Styled label and paragraph", || {
        vstack(S2)
            .child(styled_label(typography::EMPHASIS, "styled_label"))
            .child(styled_paragraph(typography::BODY, "styled_paragraph wraps long text across lines and keeps the style of its ramp step."))
            .into_element()
    }),
    Spec::forced("Monospace family", || {
        let tokens = use_tokens();
        typography::CODE
            .label(&tokens, typography::mono_family())
            .into_element()
    }),
];

fn weight_text(weight: FontWeight) -> String {
    bingux_ui::style::catalogue::weight_text(weight)
}

// ---- Space and radii -------------------------------------------------------

pub const SPACING: &[Spec] = &[
    Spec::forced("The scale, regular and compact", || {
        let tokens = use_tokens();
        let mut column = vstack(S1);
        for (name, space) in SPACES {
            let bar = space.px_in(SizeClass::Regular).max(1.0);
            column = column.child(table_row(
                &tokens,
                name,
                vec![
                    rect()
                        .width(Size::px(bar))
                        .tw(
                            &tokens,
                            &[h(Dim::Box5), rounded(Radius::Mark), bg(Paint::Accent)],
                        )
                        .into_element(),
                    caption(format!(
                        "{} / {} px",
                        space.px_in(SizeClass::Regular),
                        space.px_in(SizeClass::Compact)
                    )),
                ],
            ));
        }
        column.into_element()
    }),
    Spec::forced("Named spaces", || {
        let tokens = use_tokens();
        vstack(S1)
            .child(table_row(
                &tokens,
                "GUTTER",
                vec![caption(format!(
                    "{} px: card edge and row content",
                    GUTTER.px()
                ))],
            ))
            .child(table_row(
                &tokens,
                "NAV_INSET",
                vec![caption(format!("{} px: sidebar inset", NAV_INSET.px()))],
            ))
            .child(table_row(
                &tokens,
                "LIST_EDGE",
                vec![caption(format!("{} px: a list's sides", LIST_EDGE.px()))],
            ))
            .into_element()
    }),
];

pub const RADIUS: &[Spec] = &[
    Spec::forced("The radii", || {
        let tokens = use_tokens();
        let mut line = rect().tw(&tokens, &[w_full(), flex_row(), wrap(S3), gap(S3)]);
        for (name, radius) in RADII {
            line = line.child(
                vstack(S1)
                    .child(rect().tw(
                        &tokens,
                        &[
                            w(Dim::Box11),
                            h(Dim::Box11),
                            rounded(*radius),
                            bg(Paint::Card),
                            border(Line::Hairline),
                        ],
                    ))
                    .child(caption(format!("{name} {}", radius.px().min(99.0)))),
            );
        }
        line.into_element()
    }),
    Spec::forced("Nesting: inner radius = outer radius - inset (use Radius::Inner)", || {
        let tokens = use_tokens();
        let nest = |inner: Radius, label: &str| {
            vstack(S1)
                .child(
                    rect()
                        .tw(
                            &tokens,
                            &[
                                p(S1),
                                rounded(Radius::Control),
                                bg(Paint::Control),
                                border(Line::Hairline),
                            ],
                        )
                        .child(rect().tw(
                            &tokens,
                            &[
                                w(Dim::Box24),
                                h(Dim::Box9),
                                rounded(inner),
                                bg(Paint::Accent),
                            ],
                        )),
                )
                .child(caption(label.to_string()))
        };
        hstack(S4)
            .child(nest(Radius::Inner, "Inner: 12 - 4 = 8, even gap"))
            .into_element()
    }),
];

// ---- Elevation, layers, lines ----------------------------------------------

pub const ELEVATION: &[Spec] = &[Spec::forced("Raised, floating, knob", || {
    let tokens = use_tokens();
    let mut line = rect().tw(&tokens, &[w_full(), flex_row(), wrap(S6), gap(S6), p(S4)]);
    for (name, elevation) in ELEVATIONS {
        let (down, blur, opacity) = elevation.params();
        line = line.child(
            vstack(S2)
                .child(rect().tw(
                    &tokens,
                    &[
                        w(Dim::Box24),
                        h(Dim::Box11),
                        rounded(Radius::Control),
                        bg(Paint::Popover),
                        shadow(*elevation),
                    ],
                ))
                .child(caption(format!(
                    "{name}: {down} down, {blur} blur, {:.0}%",
                    opacity * 100.0
                ))),
        );
    }
    line.into_element()
})];

pub const LAYERS: &[Spec] = &[
    Spec::forced("Layers, back to front", || {
        let tokens = use_tokens();
        let layer = |paint: Paint, name: &str, child: Element| {
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_col(),
                        p(S2),
                        gap(S1),
                        rounded(Radius::Control),
                        bg(paint),
                        border(Line::Hairline),
                    ],
                )
                .child(caption(name.to_string()))
                .child(child)
                .into_element()
        };
        let popover = layer(
            Paint::Popover,
            "Popover: menus, popovers, dialogs",
            rect().into_element(),
        );
        let raised = layer(Paint::Raised, "Raised: the selected segment", popover);
        let control = layer(Paint::Control, "Control: buttons, fields", raised);
        let card = layer(Paint::Card, "Card: groups", control);
        layer(Paint::Bg, "Bg: the page", card)
    }),
    Spec::forced("Lines", || {
        let tokens = use_tokens();
        let mut column = vstack(S2);
        for (name, line) in LINES {
            column = column.child(table_row(
                &tokens,
                name,
                vec![
                    rect()
                        .tw(
                            &tokens,
                            &[
                                w(Dim::Box24),
                                h(Dim::Box7),
                                rounded(Radius::Mark),
                                bg(Paint::Card),
                                border(*line),
                            ],
                        )
                        .into_element(),
                ],
            ));
        }
        column.into_element()
    }),
    Spec::forced("Scrim and backdrop", || {
        let tokens = use_tokens();
        hstack(S4)
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::Box24),
                    h(Dim::Box11),
                    rounded(Radius::Control),
                    bg(Paint::Scrim),
                    border(Line::Hairline),
                ],
            ))
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::Box24),
                    h(Dim::Box11),
                    rounded(Radius::Control),
                    bg(Paint::Backdrop),
                    border(Line::Hairline),
                ],
            ))
            .child(caption("Scrim: black at 40%. Backdrop: opaque black."))
            .into_element()
    }),
    Spec::forced("Disabled opacity and inset scale", || {
        let tokens = use_tokens();
        vstack(S2)
            .child(caption(format!("DISABLED_OPACITY {}", DISABLED_OPACITY)))
            .child(caption(format!(
                "INSET_SCALE {}: the window content behind a sheet",
                INSET_SCALE
            )))
            // A disabled button: it is dimmed to DISABLED_OPACITY and takes neither the pointer nor
            // focus, which is the look the caption names.
            .child(action_button("A disabled look", |_| {}).compact().enabled(false))
            .child(rect().tw(&tokens, &[h(Dim::Hairline)]))
            .into_element()
    }),
];

// ---- Density ---------------------------------------------------------------

pub const DENSITY: &[Spec] = &[
    Spec::forced("Box sizes at both densities", || {
        let tokens = use_tokens();
        let regular = dims(SizeClass::Regular);
        let mut column = vstack(S1);
        for (name, dim) in regular {
            column = column.child(table_row(
                &tokens,
                name,
                vec![caption(format!(
                    "{} / {} px",
                    dim.px_in(SizeClass::Regular),
                    dim.px_in(SizeClass::Compact)
                ))],
            ));
        }
        column.into_element()
    }),
    Spec::forced("Icon sizes", || {
        let tokens = use_tokens();
        let mut line = hstack(S4);
        for (name, size) in [
            ("Mark", IconSize::Mark),
            ("Text", IconSize::Text),
            ("Control", IconSize::Control),
            ("Row", IconSize::Row),
            ("Hero", IconSize::Hero),
        ] {
            line = line.child(
                vstack(S1)
                    .child(icon(Icon::Network, size, Paint::Text.color(&tokens)))
                    .child(caption(format!("{name} {}", size.px()))),
            );
        }
        line.into_element()
    }),
    Spec::forced("What the window reads", || {
        let tokens = use_tokens();
        vstack(S1)
            .child(table_row(
                &tokens,
                "size_class()",
                vec![caption(format!("{:?}", size_class()))],
            ))
            .child(table_row(
                &tokens,
                "is_compact()",
                vec![caption(is_compact().to_string())],
            ))
            .child(table_row(
                &tokens,
                "pointer_kind()",
                vec![caption(format!("{:?}", pointer_kind()))],
            ))
            .child(table_row(
                &tokens,
                "device_preview()",
                vec![caption(device_preview().to_string())],
            ))
            .child(table_row(
                &tokens,
                "pick(36, 44)",
                vec![caption(pick(36, 44).to_string())],
            ))
            .child(table_row(
                &tokens,
                "NARROW_BELOW",
                vec![caption(format!("{NARROW_BELOW} px"))],
            ))
            .child(table_row(
                &tokens,
                "PHONE_PREVIEW_SIZE",
                vec![caption(format!("{PHONE_PREVIEW_SIZE:?}"))],
            ))
            .child(table_row(
                &tokens,
                "WINDOW_SIZE",
                vec![caption(format!("{WINDOW_SIZE:?}"))],
            ))
            .child(table_row(
                &tokens,
                "WINDOW_MIN_SIZE",
                vec![caption(format!("{WINDOW_MIN_SIZE:?}"))],
            ))
            .child(table_row(
                &tokens,
                "font_family()",
                vec![caption(bingux_ui::theme::font_family().to_string())],
            ))
            .into_element()
    }),
];

// ---- Recipes and utilities -------------------------------------------------

pub const RECIPES: &[Spec] = &[Spec::forced("Each recipe on a box", || {
    let tokens = use_tokens();
    let recipes: [(&str, Recipe); 12] = [
        ("BOX", BOX),
        ("ROW", ROW),
        ("CONTROL", CONTROL),
        ("BUTTON", BUTTON),
        ("BUTTON_COMPACT", BUTTON_COMPACT),
        ("MENU_ITEM", MENU_ITEM),
        ("TOOLTIP", TOOLTIP),
        ("DIALOG", DIALOG),
        ("LIST_ITEM", LIST_ITEM),
        ("NAV_ITEM", NAV_ITEM),
        ("BADGE_BOX", BADGE_BOX),
        ("ICON_BUTTON", ICON_BUTTON),
    ];
    let mut column = vstack(S2);
    for (name, recipe) in recipes {
        // The tooltip recipe fills with the dark floating paint, so its text is the ink for that
        // paint, as the tooltip's own is (pixel-check.py found it dark on dark in the light scheme).
        let label = if name == "TOOLTIP" {
            typography::BODY.in_ink(Ink::OnFloating).label(&tokens, name)
        } else {
            typography::BODY.label(&tokens, name)
        };
        column = column.child(
            hstack(S3).child(
                rect()
                    .tw(&tokens, recipe)
                    .tw(&tokens, &[w(Dim::Field)])
                    .child(label),
            ),
        );
    }
    column.into_element()
})
.wide()];

// ---- Motion ----------------------------------------------------------------

pub const MOTION: &[Spec] = &[
    Spec::forced("Durations", || {
        let tokens = use_tokens();
        vstack(S1)
            .child(table_row(
                &tokens,
                "MOVE_MS",
                vec![caption(format!(
                    "{} ms: a box that moves or resizes",
                    bingux_ui::motion::MOVE_MS
                ))],
            ))
            .child(table_row(
                &tokens,
                "FOCUS_IN_MS",
                vec![caption(format!("{} ms", bingux_ui::motion::FOCUS_IN_MS))],
            ))
            .child(table_row(
                &tokens,
                "FOCUS_OUT_MS",
                vec![caption(format!("{} ms", bingux_ui::motion::FOCUS_OUT_MS))],
            ))
            .child(table_row(
                &tokens,
                "FOCUS_GAP, WIDTH, TRAVEL",
                vec![caption(format!(
                    "{} px gap, {} px ring, {} px travel",
                    bingux_ui::motion::FOCUS_GAP,
                    bingux_ui::motion::FOCUS_WIDTH,
                    bingux_ui::motion::FOCUS_TRAVEL
                ))],
            ))
            .child(table_row(
                &tokens,
                "PRESENCE_SCALE",
                vec![caption(format!("{}", bingux_ui::motion::PRESENCE_SCALE))],
            ))
            .child(table_row(
                &tokens,
                "reduced motion",
                vec![caption(format!(
                    "{} (BINGUX_REDUCED_MOTION=1 sets 0 ms)",
                    tokens.reduced_motion
                ))],
            ))
            .child(table_row(
                &tokens,
                "Dwell::LOADING",
                vec![caption(format!(
                    "shown after {} ms, held at least {} ms",
                    bingux_ui::motion::Dwell::LOADING.show_after.as_millis(),
                    bingux_ui::motion::Dwell::LOADING.hold.as_millis()
                ))],
            ))
            .into_element()
    }),
    // One icon turning into another, frozen at five points of the progress:
    // the icons of a Reload button that becomes Stop while a folder loads.
    Spec::forced("Icon morph", || {
        let tokens = use_tokens();
        let mut strip = hstack(S5);
        for progress in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            strip = strip.child(
                vstack(S1)
                    .child(bingux_ui::motion::icon_morph(
                        Icon::Refresh,
                        Icon::MediaPlaybackStop,
                        IconSize::Row,
                        Ink::Text.color(&tokens),
                        progress,
                    ))
                    .child(caption(format!("{progress}"))),
            );
        }
        strip.into_element()
    }),
    // The ring as it settles on each kind of face: its box is the face's box
    // grown by FOCUS_GAP and FOCUS_WIDTH, and each corner is the face's own
    // painted radius plus that distance, or square where the face is square.
    // Held on (`Focus::Keyboard`), so it is always on screen.
    Spec::forced("Focus ring", || {
        let tokens = use_tokens();
        let face = |label: &str, paint: Paint, on: Paint, radius: Radius, corners: Corners| {
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        px(S4),
                        h(Dim::Control),
                        bg(paint),
                        ink(on),
                        rounded_on(radius, corners, 0.0),
                    ],
                )
                .child(typography::BODY.label(&tokens, label.to_string()))
        };
        let round = |label: &str, paint: Paint, on: Paint, radius: Radius| face(label, paint, on, radius, Corners::All);
        vstack(S5)
            .tw(&tokens, &[p(S2)])
            .child(
                hstack(S6)
                    .child(round("Button", Paint::Control, Paint::Text, Radius::Control).focusable(Focus::Keyboard))
                    .child(round("Suggested", Paint::Accent, Paint::OnAccent, Radius::Control).focusable(Focus::Keyboard))
                    .child(round("Destructive", Paint::Danger, Paint::OnDanger, Radius::Control).focusable(Focus::Keyboard))
                    .child(round("Row", Paint::Card, Paint::Text, Radius::Inner).focusable(Focus::Keyboard)),
            )
            .child(
                hstack(S6)
                    .child(
                        face("Square", Paint::Control, Paint::Text, Radius::Control, Corners::None)
                            .focusable(Focus::Keyboard),
                    )
                    .child(
                        face("Tab", Paint::Control, Paint::Text, Radius::Control, Corners::Top)
                            .focusable(Focus::Keyboard),
                    )
                    .child(
                        face("Segment", Paint::Control, Paint::Text, Radius::Control, Corners::Left)
                            .focusable(Focus::Keyboard),
                    )
                    .child(
                        round("Invalid, focused", Paint::Control, Paint::Text, Radius::Control)
                            .focusable_invalid(Focus::Keyboard, true),
                    )
                    .child(
                        round("Invalid", Paint::Control, Paint::Text, Radius::Control)
                            .focusable_invalid(Focus::Not, true),
                    ),
            )
            .into_element()
    })
    .wide(),
    // Focus gained and lost: the ring blurs in from a wider, softer state and
    // blurs out again. Press a button to give it the focus; pressing the other
    // cross-fades the two rings.
    Spec::live("Focus blur", || {
        let tokens = use_tokens();
        let mut which = use_state(|| 0_u8);
        let face = |label: &'static str, index: u8| {
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        px(S4),
                        h(Dim::Control),
                        bg(Paint::Control),
                        rounded(Radius::Control),
                    ],
                )
                .on_press(move |_| which.set(if which() == index { 0 } else { index }))
                .child(typography::BODY.label(&tokens, label.to_string()))
                .focusable(if which() == index { Focus::Keyboard } else { Focus::Not })
        };
        vstack(S2)
            .child(hstack(S6).tw(&tokens, &[p(S2)]).child(face("First", 1)).child(face("Second", 2)))
            .child(caption("Press one: its ring blurs in. Press the other: they cross-fade. Press again: it blurs out."))
            .into_element()
    }),
    Spec::live("Reveal", || {
        let mut expanded = use_state(|| false);
        vstack(S2)
            .child(switch_row("Show more", expanded(), move |next| {
                expanded.set(next)
            }))
            .child(reveal(
                expanded(),
                styled_label(typography::BODY, "Revealed text. Its height animates."),
            ))
            .child(caption("Siblings below never jump."))
            .into_element()
    }),
    Spec::live("Presence", || {
        let mut badge = use_state(|| false);
        vstack(S2)
            .child(switch_row("Show badge", badge(), move |next| {
                badge.set(next)
            }))
            .child(presence(
                badge(),
                styled_label(typography::BODY, "Badge appears in place."),
            ))
            .into_element()
    }),
    Spec::live("Crossfade", || {
        let mut long = use_state(|| false);
        vstack(S2)
            .child(switch_row("Longer label", long(), move |next| {
                long.set(next)
            }))
            .child(crossfade(styled_label(
                typography::BODY,
                if long() {
                    "A longer label that takes more room"
                } else {
                    "Short"
                },
            )))
            .into_element()
    }),
    // The digits roll as the shell clock's do: each one that changes slides in
    // a clipped column of its own, up for a value that went up and down for
    // one that went down. The text around a number never moves while it counts.
    Spec::live("Animated number: counting up and down, a jump", || {
        let mut count = use_state(|| 128_i64);
        vstack(S2)
            .child(
                hstack(S2)
                    .child(styled_label(typography::BODY, "Selected"))
                    .child(animated_number(count() as f64))
                    .child(styled_label(typography::BODY, "items")),
            )
            .child(
                hstack(S2)
                    .child(action_button("Up", move |_| count.set(count() + 1)).compact())
                    .child(action_button("Down", move |_| count.set(count() - 1)).compact())
                    .child(action_button("Jump", move |_| count.set(count() * 7 % 1000 + 3)).compact())
                    .child(action_button("Reset", move |_| count.set(0)).compact()),
            )
            .into_element()
    }),
    Spec::live("Animated number: a digit more or less (99 and 100)", || {
        let mut wide = use_state(|| false);
        vstack(S2)
            .child(
                hstack(S2)
                    .child(styled_label(typography::BODY, "Download"))
                    .child(animated_number(if wide() { 100.0 } else { 99.0 }).suffix("%"))
                    .child(styled_label(typography::BODY, "done")),
            )
            .child(switch_row("Three digits", wide(), move |next| wide.set(next)))
            .child(caption("The number grows and shrinks smoothly; the words after it follow."))
            .into_element()
    }),
    Spec::live("Animated number: clock-like (zero padded, 59 wraps to 00)", || {
        let mut seconds = use_state(|| 12 * 3600 + 59 * 60 + 56_i64);
        let now = seconds();
        let (hours, minutes, secs) = (now / 3600 % 24, now / 60 % 60, now % 60);
        let colon = || styled_label(typography::NUMERIC, ":");
        vstack(S2)
            .child(
                hstack(S0)
                    .child(animated_number(hours as f64).min_digits(2))
                    .child(colon())
                    .child(animated_number(minutes as f64).min_digits(2))
                    .child(colon())
                    .child(animated_number(secs as f64).min_digits(2)),
            )
            .child(
                hstack(S2)
                    .child(action_button("+1 s", move |_| seconds.set(seconds() + 1)).compact())
                    .child(action_button("+1 min", move |_| seconds.set(seconds() + 60)).compact())
                    .child(action_button("-1 s", move |_| seconds.set(seconds() - 1)).compact()),
            )
            .into_element()
    }),
    Spec::live("Animated number: percent", || {
        let mut level = use_state(|| 87_i64);
        vstack(S2)
            .child(
                hstack(S2)
                    .child(styled_label(typography::BODY, "Battery"))
                    .child(animated_number(level() as f64).suffix("%")),
            )
            .child(
                hstack(S2)
                    .child(action_button("Drain", move |_| level.set((level() - 1).max(0))).compact())
                    .child(action_button("Charge", move |_| level.set((level() + 1).min(100))).compact())
                    .child(action_button("Full", move |_| level.set(100)).compact())
                    .child(action_button("Empty", move |_| level.set(0)).compact()),
            )
            .into_element()
    }),
    Spec::live("Animated number: speed with decimals and a unit", || {
        let mut tenths = use_state(|| 99_i64);
        vstack(S2)
            .child(
                hstack(S2)
                    .child(styled_label(typography::BODY, "Download"))
                    .child(animated_number(tenths() as f64 / 10.0).decimals(1).suffix(" MB/s")),
            )
            .child(
                hstack(S2)
                    .child(action_button("Faster", move |_| tenths.set(tenths() + 7)).compact())
                    .child(action_button("Slower", move |_| tenths.set((tenths() - 7).max(0))).compact()),
            )
            .into_element()
    }),
    Spec::live("Animated number: big display, grouped, signed", || {
        let mut value = use_state(|| 1_999_i64);
        vstack(S2)
            .child(animated_number(value() as f64).style(typography::HERO).grouped())
            .child(
                hstack(S2)
                    .child(action_button("+1", move |_| value.set(value() + 1)).compact())
                    .child(action_button("x10", move |_| value.set(value() * 10)).compact())
                    .child(action_button("Negate", move |_| value.set(-value())).compact())
                    .child(action_button("Reset", move |_| value.set(1_999)).compact()),
            )
            .into_element()
    }),
    Spec::live("Animated number: small caption", || {
        let mut count = use_state(|| 41_i64);
        hstack(S2)
            .child(animated_number(count() as f64).style(typography::CAPTION).suffix(" unread"))
            .child(action_button("Add", move |_| count.set(count() + 1)).compact())
            .into_element()
    }),
    Spec::live("Animated number: many updates in a row", || {
        let mut count = use_state(|| 0_i64);
        hstack(S2)
            .child(animated_number(count() as f64))
            .child(
                action_button("Count to 100", move |_| {
                    count.set(0);
                    spawn(async move {
                        for _ in 0..100 {
                            timer(std::time::Duration::from_millis(30)).await;
                            count.set(count() + 1);
                        }
                    });
                })
                .compact(),
            )
            .into_element()
    }),
];

// ---- Colour maths, theme, utilities ----------------------------------------

pub const COLOUR_MATHS: &[Spec] = &[Spec::forced("Hex, ARGB, HSV, over, surface_color", || {
    let tokens = use_tokens();
    let swatch = |colour: Color| {
        rect()
            .tw(
                &tokens,
                &[
                    w(Dim::Box7),
                    h(Dim::Box7),
                    rounded(Radius::Mark),
                    fill(colour),
                    border(Line::Hairline),
                ],
            )
            .into_element()
    };
    let blue = colour_from_hex("#3584e4").unwrap_or_else(|| accent_for("blue", false));
    let half_red = colour_from_argb(128, 230, 45, 66);
    let card = Paint::Card.color(&tokens);
    vstack(S2)
        .child(table_row(
            &tokens,
            "colour_from_hex",
            vec![swatch(blue), caption("#3584e4")],
        ))
        .child(table_row(
            &tokens,
            "colour_to_hex",
            vec![swatch(blue), caption(colour_to_hex(blue))],
        ))
        .child(table_row(
            &tokens,
            "colour_from_argb",
            vec![swatch(half_red), caption("alpha 128")],
        ))
        .child(table_row(
            &tokens,
            "colour_from_hsv",
            vec![
                swatch(colour_from_hsv(150.0, 0.7, 0.8)),
                caption("150, 70%, 80%"),
            ],
        ))
        .child(table_row(
            &tokens,
            "over(top, bottom)",
            vec![
                swatch(over(half_red, card)),
                caption("half red over the card"),
            ],
        ))
        .child(table_row(
            &tokens,
            "surface_color",
            vec![
                swatch(surface_color(&tokens, card, true, false, false)),
                swatch(surface_color(&tokens, card, false, true, false)),
                swatch(surface_color(&tokens, card, false, false, true)),
            ],
        ))
        .child(table_row(
            &tokens,
            "rgba",
            vec![swatch(rgba(0.2, 0.5, 0.9, 1.0))],
        ))
        .child(caption("surface_color swatches: hover, pressed, selected"))
        .into_element()
})];

/// A bar of `ink` on a square of `surface`: how a pair reads, with the two
/// colours exactly as measured.
fn ink_on(tokens: &Tokens, ink: Color, surface: Color) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w(Dim::Box9),
                h(Dim::Box9),
                rounded(Radius::Mark),
                fill(surface),
                border(Line::Hairline),
                center(),
            ],
        )
        .child(
            rect()
                .tw(tokens, &[w(Dim::Box5), h(Dim::Box5), rounded(Radius::Mark), fill(ink)]),
        )
        .into_element()
}

/// One measured pair: the colours, what is drawn, and the reading, each on
/// one line so a narrow stage cuts them with an ellipsis instead of wrapping.
fn pair_row(tokens: &Tokens, ink: Color, surface: Color, title: String, reading: String) -> Element {
    hstack(S3)
        .tw(tokens, &[w_full(), items(Align::Center)])
        .child(ink_on(tokens, ink, surface))
        .child(
            vstack(S0_5)
                .tw(tokens, &[w_flex()])
                .min_width(Size::px(0.0))
                .child(typography::BODY.label(tokens, title).width(Size::fill()).max_lines(1))
                .child(
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(tokens, reading)
                        .width(Size::fill())
                        .max_lines(1),
                ),
        )
        .into_element()
}

/// The contrast matrix measured under this window's tokens: the verdict, the
/// pairs closest to their requirement, and the helpers that pick an ink.
pub const CONTRAST: &[Spec] = &[Spec::forced("The matrix under this window's tokens", || {
    let tokens = use_tokens();
    let rows = contrast_matrix::evaluate(&tokens);
    let (total, failing, exempt) = contrast_matrix::counts(&rows);
    let mut closest: Vec<&contrast_matrix::Row> = rows.iter().collect();
    closest.sort_by(|a, b| (a.ratio - a.required()).total_cmp(&(b.ratio - b.required())));
    let shown = closest.len().min(8);
    let mut column = vstack(S2).tw(&tokens, &[w_full()]).child(
        typography::BODY
            .label(
                &tokens,
                format!("{total} pairs, {failing} failing, {exempt} exempt below requirement"),
            )
            .width(Size::fill())
            .max_lines(1),
    );
    column = column.child(
        typography::CAPTION
            .in_ink(Ink::Muted)
            .label(&tokens, format!("The {shown} closest to their requirement:"))
            .width(Size::fill())
            .max_lines(1),
    );
    for row in closest.into_iter().take(shown) {
        column = column.child(pair_row(
            &tokens,
            row.fg,
            row.bg,
            format!("{} on {}", row.pair.fg.name(), row.pair.bg.name()),
            format!(
                "{:.2}:1, needs {}, {} (APCA {:.0})",
                row.ratio,
                row.pair.need.label(),
                row.badge(),
                row.apca,
            ),
        ));
    }
    let card = Paint::Card.color(&tokens);
    let on = contrast::on_colour(card);
    let held = contrast::ensure_contrast(Ink::Muted.color(&tokens), &[card], contrast::AAA_TEXT);
    column
        .child(pair_row(
            &tokens,
            on,
            card,
            "on_colour(card)".to_string(),
            format!(
                "{:.2}:1, {}",
                contrast::ratio(on, card),
                contrast::Level::of(contrast::ratio(on, card)).label(),
            ),
        ))
        .child(pair_row(
            &tokens,
            held,
            card,
            "ensure_contrast(Muted, card, 7)".to_string(),
            format!(
                "{:.2}:1 (AA text {}:1, AA large {}:1)",
                contrast::worst_ratio(held, &[card]),
                contrast::AA_TEXT,
                contrast::AA_LARGE,
            ),
        ))
        .into_element()
})];

pub const THEME_STATE: &[Spec] = &[Spec::forced("What the window provides", || {
    let tokens = use_tokens();
    vstack(S1)
        .child(table_row(
            &tokens,
            "accent_name",
            vec![caption(tokens.accent_name)],
        ))
        .child(table_row(
            &tokens,
            "scheme",
            vec![caption(format!(
                "{:?}, dark: {}",
                tokens.scheme,
                tokens.scheme.is_dark()
            ))],
        ))
        .child(table_row(
            &tokens,
            "reduced_motion",
            vec![caption(tokens.reduced_motion.to_string())],
        ))
        .child(table_row(
            &tokens,
            "font_family()",
            vec![caption(bingux_ui::theme::font_family())],
        ))
        .child(table_row(
            &tokens,
            "font files",
            vec![caption(format!(
                "{} found",
                bingux_ui::theme::font_files(bingux_ui::theme::font_family(), 8).len()
            ))],
        ))
        .into_element()
})];

pub const UTILITIES: &[Spec] = &[
    Spec::forced("Padding, gap, radius, fill", || {
        let tokens = use_tokens();
        rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    p(S3),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                    border(Line::Hairline),
                ],
            )
            .child(typography::BODY.label(&tokens, "p(S3), gap(S2), rounded(Card), bg(Card)"))
            .into_element()
    }),
    Spec::forced("Sizes by role", || {
        let tokens = use_tokens();
        hstack(S2)
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::Box9),
                    h(Dim::Box9),
                    rounded(Radius::Control),
                    bg(Paint::Accent),
                ],
            ))
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::Box11),
                    h(Dim::Box11),
                    rounded(Radius::Control),
                    bg(Paint::Accent),
                ],
            ))
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::Box24),
                    h(Dim::Box11),
                    rounded(Radius::Control),
                    bg(Paint::Accent),
                ],
            ))
            .into_element()
    }),
    Spec::forced("Flex: grow, justify, wrap", || {
        let tokens = use_tokens();
        let cell = |label: &str| {
            rect()
                .tw(
                    &tokens,
                    &[p(S2), rounded(Radius::Control), bg(Paint::Control)],
                )
                .child(typography::CAPTION.label(&tokens, label.to_string()))
        };
        vstack(S2)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), grow_children(), gap(S2)])
                    .child(rect().tw(&tokens, &[w_flex()]).child(cell("w_flex")))
                    .child(rect().tw(&tokens, &[w_flex()]).child(cell("w_flex"))),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), justify(Align::Between)])
                    .child(cell("start"))
                    .child(cell("end")),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), wrap(S2), gap(S2)])
                    .children((1..=8).map(|n| cell(&format!("wrap {n}")).into_element())),
            )
            .into_element()
    }),
];
