//! The header switcher: colour scheme, accent, density, display scale, text
//! size and pseudo-localised strings for the whole window.
//!
//! Text size and strings are the stress switches of `bingux_ui::stress`: a
//! specimen can be looked at with every string 40 % longer and the type ramp
//! at 200 %, as `scripts/overflow-check.py` runs it.
//!
//! On a desktop window the choices sit in the header bar as dense
//! dropdowns. At phone density there is no room, so the header holds one
//! button that opens them in a sheet, which is also how an app built on
//! bingux-ui puts a window-level tool on a phone.

use bingux_ui::prelude::*;
use bingux_ui::theme::accent_names;
use freya::prelude::*;

use crate::env::{DensityMode, Env, SCALES, TEXT_SCALES, ThemeMode, apply_window, use_env};

/// The element the split view carries in its header.
#[derive(PartialEq)]
pub struct Switcher;

impl Component for Switcher {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        if is_compact() {
            icon_button(Icon::Appearance, move |_| {
                window.open(popup("Display", SwitcherSheet).width(Dim::PopupNarrow));
            })
            .tooltip("Theme, accent, density and scale")
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
                .map(|mode| (mode.key().to_string(), format!("Theme: {}", mode.label())))
                .collect(),
            now.theme.key(),
            move |key: String| {
                if let Some(mode) = ThemeMode::from_key(&key) {
                    env.write().theme = mode;
                }
            },
        )
        .title("Theme");
        let accent = dropdown(
            std::iter::once(("system".to_string(), "Accent: system".to_string()))
                .chain(accent_names().map(|name| (name.to_string(), format!("Accent: {name}"))))
                .collect(),
            now.accent.unwrap_or("system"),
            move |key: String| {
                env.write().accent = accent_names().find(|name| *name == key);
            },
        )
        .title("Accent");
        let density = dropdown(
            DensityMode::ALL
                .iter()
                .map(|mode| (mode.key().to_string(), format!("Density: {}", mode.label())))
                .collect(),
            now.density.key(),
            move |key: String| {
                if let Some(mode) = DensityMode::from_key(&key) {
                    env.write().density = mode;
                    apply_window(*env.peek());
                }
            },
        )
        .title("Density");
        let scale = dropdown(
            SCALES
                .iter()
                .map(|scale| (scale.to_string(), format!("Scale: {scale}%")))
                .collect(),
            now.scale.to_string(),
            move |key: String| {
                if let Ok(scale) = key.parse() {
                    env.write().scale = scale;
                    apply_window(*env.peek());
                }
            },
        )
        .title("Scale");
        let text = dropdown(
            TEXT_SCALES
                .iter()
                .map(|scale| (scale.to_string(), format!("Text: {scale}%")))
                .collect(),
            now.text_scale.to_string(),
            move |key: String| {
                if let Ok(scale) = key.parse() {
                    env.write().text_scale = scale;
                    apply_window(*env.peek());
                }
            },
        )
        .title("Text size");
        let strings = dropdown(
            vec![
                ("as-written".to_string(), "Strings: as written".to_string()),
                ("pseudo".to_string(), "Strings: pseudo-locale".to_string()),
            ],
            if now.pseudo { "pseudo" } else { "as-written" },
            move |key: String| {
                env.write().pseudo = key == "pseudo";
                apply_window(*env.peek());
            },
        )
        .title("Strings");
        if self.dense {
            hstack(S2)
                .child(theme.dense())
                .child(accent.dense())
                .child(density.dense())
                .child(scale.dense())
                .child(text.dense())
                .child(strings.dense())
                .into_element()
        } else {
            Group::new(None::<String>)
                .row(form_row("Theme", theme))
                .row(form_row("Accent", accent))
                .row(form_row("Density", density))
                .row(form_row("Scale", scale))
                .row(form_row("Text size", text))
                .row(form_row("Strings", strings))
                .into_element()
        }
    }
}
