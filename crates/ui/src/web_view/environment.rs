//! What the host's appearance means to a page.
//!
//! Pages ask the system how it looks through media queries and system
//! colours: `prefers-color-scheme`, `prefers-reduced-motion`,
//! `prefers-contrast`, `AccentColor`, the text scale. The host has these as
//! bingux-ui tokens; [`PageEnvironment`] is the one value that carries them to
//! the engine, built from the tokens by [`PageEnvironment::from_tokens`] and
//! sent again by the browser whenever the tokens change.
//!
//! | bingux-ui | Page sees |
//! | --------- | --------- |
//! | `Tokens::scheme` (`PreferDark`) | `prefers-color-scheme: dark`, `color-scheme: light dark` pages draw dark, scrollbars and form controls in the dark theme; `Default` and `PreferLight` are light |
//! | `Tokens::accent` | the `AccentColor` / `AccentColorText` system colours (`accent-color: auto`, focus rings) |
//! | `Tokens::reduced_motion` | `prefers-reduced-motion: reduce` |
//! | the stress/text scale (`BINGUX_TEXT_SCALE`) | the text scale the engine multiplies its default font size by |
//! | the locale of the process (`LANG`) | `navigator.languages` and `Accept-Language` |
//! | contrast | `prefers-contrast`: bingux-ui has no contrast preference yet, so pages always see "no preference" |
//! | the interface font | none: the ABI has no setting for the default fonts, so pages use the engine's |

use crate::theme::{Scheme, Tokens};

/// The colour scheme pages see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageScheme {
    Light,
    Dark,
}

/// The appearance a page sees. Engine wide: every tab, now and later.
#[derive(Clone, Debug, PartialEq)]
pub struct PageEnvironment {
    pub scheme: PageScheme,
    /// The accent colour, `0xAARRGGBB`.
    pub accent: u32,
    pub reduced_motion: bool,
    /// `prefers-contrast: more`. Nothing in bingux-ui sets it yet.
    pub more_contrast: bool,
    /// 0.5 to 3.
    pub text_scale: f32,
    /// BCP 47 tags, most preferred first.
    pub locales: Vec<String>,
}

impl Default for PageEnvironment {
    fn default() -> Self {
        Self {
            scheme: PageScheme::Light,
            accent: 0xFF35_84E4,
            reduced_motion: false,
            more_contrast: false,
            text_scale: 1.0,
            locales: Vec::new(),
        }
    }
}

impl PageEnvironment {
    /// The environment of a window with these `tokens`, at `text_scale`.
    /// `locales` is the host's choice and does not follow the tokens.
    pub fn from_tokens(tokens: &Tokens, text_scale: f32, locales: Vec<String>) -> Self {
        Self {
            scheme: if matches!(tokens.scheme, Scheme::PreferDark) {
                PageScheme::Dark
            } else {
                PageScheme::Light
            },
            accent: tokens.accent_argb(),
            reduced_motion: tokens.reduced_motion,
            more_contrast: false,
            text_scale: text_scale.clamp(0.5, 3.0),
            locales,
        }
    }

    /// Whether pages see the dark scheme.
    pub fn dark(&self) -> bool {
        self.scheme == PageScheme::Dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_scheme_and_its_accent_reach_the_page() {
        let tokens = Tokens::new("blue", Scheme::PreferDark, true);
        let environment = PageEnvironment::from_tokens(&tokens, 1.25, vec!["en-GB".to_string()]);
        assert!(environment.dark());
        assert!(environment.reduced_motion);
        assert_eq!(environment.text_scale, 1.25);
        assert_eq!(environment.accent >> 24, 0xFF, "the accent is opaque");
        assert_eq!(environment.locales, ["en-GB"]);
    }

    #[test]
    fn the_default_and_light_schemes_are_light() {
        for scheme in [Scheme::Default, Scheme::PreferLight] {
            let tokens = Tokens::new("blue", scheme, false);
            assert!(!PageEnvironment::from_tokens(&tokens, 1.0, Vec::new()).dark());
        }
    }

    #[test]
    fn the_text_scale_stays_in_what_the_engine_accepts() {
        let tokens = Tokens::new("blue", Scheme::PreferDark, false);
        assert_eq!(PageEnvironment::from_tokens(&tokens, 9.0, Vec::new()).text_scale, 3.0);
        assert_eq!(PageEnvironment::from_tokens(&tokens, 0.1, Vec::new()).text_scale, 0.5);
    }
}
