//! The header switcher: colour scheme, accent, density, display scale, text
//! size, contrast and pseudo-localised strings for the whole window.
//!
//! Text size and strings are the stress switches of `bingux_ui::stress`: a
//! specimen can be looked at with every string 40 % longer and the type ramp
//! at 200 %, as `scripts/overflow-check.py` runs it.
//!
//! On a desktop window the choices sit in the header bar as dense
//! dropdowns. At phone density, or in a window too narrow for six of them
//! beside the window buttons, there is no room, so the header holds one
//! button that opens them in a sheet, which is also how an app built on
//! bingux-ui puts a window-level tool on a phone.

use bingux_ui::prelude::*;
use bingux_ui::theme::accent_names;
use freya::prelude::*;

use crate::env::{DensityMode, Env, SCALES, TEXT_SCALES, ThemeMode, apply_window, use_env};

/// The element the split view carries in its header.
#[derive(PartialEq)]
pub struct Switcher;

/// The narrowest window whose header holds the six dropdowns beside the window's own buttons.
/// The taxonomy's default window (1320 px) is wider; below it the header's trailing side would
/// run over the sidebar's search field, so the dropdowns go in the sheet a phone gets.
const DENSE_FROM: f32 = 1240.0;

impl Component for Switcher {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let width = bingux_ui::surface::window_size().width;
        // The dropdowns' labels grow with the text size, and the room they need with them.
        if is_compact() || width < DENSE_FROM * bingux_ui::access::text_scale().max(1.0) {
            icon_button(Icon::Appearance, move |_| {
                window.open(
                    popup(tr!("taxonomy-switcher-title"), SwitcherSheet).width(Dim::PopupNarrow),
                );
            })
            .tooltip(tr!("taxonomy-switcher-tooltip"))
            .into_element()
        } else {
            hstack(S2).child(Choices { dense: true }).into_element()
        }
    }
}

/// The switcher's body inside the sheet a phone opens.
#[derive(PartialEq)]
struct SwitcherSheet;

impl Component for SwitcherSheet {
    fn render(&self) -> impl IntoElement {
        popup_column().child(Choices { dense: false })
    }
}

/// The four choices. `dense` is the header form: dropdowns with no labels.
#[derive(PartialEq)]
struct Choices {
    dense: bool,
}

impl Component for Choices {
    fn render(&self) -> impl IntoElement {
        let mut env = use_env();
        let now: Env = *env.read();
        let theme = dropdown(
            ThemeMode::ALL
                .iter()
                .map(|mode| {
                    (
                        mode.key().to_string(),
                        tr!("taxonomy-choice-theme-option", name = mode.label()),
                    )
                })
                .collect(),
            now.theme.key(),
            move |key: String| {
                if let Some(mode) = ThemeMode::from_key(&key) {
                    env.write().theme = mode;
                }
            },
        )
        .title(tr!("taxonomy-choice-theme"));
        let accent = dropdown(
            std::iter::once(("system".to_string(), tr!("taxonomy-choice-accent-system")))
                .chain(accent_names().map(|name| {
                    (
                        name.to_string(),
                        tr!("taxonomy-choice-accent-option", name = name),
                    )
                }))
                .collect(),
            now.accent.unwrap_or("system"),
            move |key: String| {
                env.write().accent = accent_names().find(|name| *name == key);
            },
        )
        .title(tr!("taxonomy-choice-accent"));
        let density = dropdown(
            DensityMode::ALL
                .iter()
                .map(|mode| {
                    (
                        mode.key().to_string(),
                        tr!("taxonomy-choice-density-option", name = mode.label()),
                    )
                })
                .collect(),
            now.density.key(),
            move |key: String| {
                if let Some(mode) = DensityMode::from_key(&key) {
                    env.write().density = mode;
                    apply_window(*env.peek());
                }
            },
        )
        .title(tr!("taxonomy-choice-density"));
        let scale = dropdown(
            SCALES
                .iter()
                .map(|scale| {
                    (
                        scale.to_string(),
                        tr!("taxonomy-choice-scale-option", scale = *scale),
                    )
                })
                .collect(),
            now.scale.to_string(),
            move |key: String| {
                if let Ok(scale) = key.parse() {
                    env.write().scale = scale;
                    apply_window(*env.peek());
                }
            },
        )
        .title(tr!("taxonomy-choice-scale"));
        let text = dropdown(
            TEXT_SCALES
                .iter()
                .map(|scale| {
                    (
                        scale.to_string(),
                        tr!("taxonomy-choice-text-option", scale = *scale),
                    )
                })
                .collect(),
            now.text_scale.to_string(),
            move |key: String| {
                if let Ok(scale) = key.parse() {
                    env.write().text_scale = scale;
                    apply_window(*env.peek());
                }
            },
        )
        .title(tr!("taxonomy-choice-text-size"));
        let strings = dropdown(
            vec![
                (
                    "as-written".to_string(),
                    tr!("taxonomy-choice-strings-plain"),
                ),
                ("pseudo".to_string(), tr!("taxonomy-choice-strings-pseudo")),
            ],
            if now.pseudo { "pseudo" } else { "as-written" },
            move |key: String| {
                env.write().pseudo = key == "pseudo";
                apply_window(*env.peek());
            },
        )
        .title(tr!("taxonomy-choice-strings"));
        let contrast = dropdown(
            vec![
                (
                    "standard".to_string(),
                    tr!("taxonomy-choice-contrast-standard"),
                ),
                ("high".to_string(), tr!("taxonomy-choice-contrast-high")),
            ],
            if now.high_contrast {
                "high"
            } else {
                "standard"
            },
            move |key: String| {
                env.write().high_contrast = key == "high";
                apply_window(*env.peek());
            },
        )
        .title(tr!("taxonomy-choice-contrast"));
        if self.dense {
            hstack(S2)
                .child(theme.dense())
                .child(accent.dense())
                .child(density.dense())
                .child(scale.dense())
                .child(text.dense())
                .child(contrast.dense())
                .child(strings.dense())
                .into_element()
        } else {
            Group::new(None::<String>)
                .row(form_row(tr!("taxonomy-choice-theme"), theme))
                .row(form_row(tr!("taxonomy-choice-accent"), accent))
                .row(form_row(tr!("taxonomy-choice-density"), density))
                .row(form_row(tr!("taxonomy-choice-scale"), scale))
                .row(form_row(tr!("taxonomy-choice-text-size"), text))
                .row(form_row(tr!("taxonomy-choice-contrast"), contrast))
                .row(form_row(tr!("taxonomy-choice-strings"), strings))
                .into_element()
        }
    }
}
