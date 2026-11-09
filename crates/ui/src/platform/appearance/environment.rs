//! The environment source: `BINGUX_*` variables that pin a value for one run.

use std::env::VarError;

use super::{Reading, parse_scheme};

/// Read the pins from `var`, which is `std::env::var` outside a test.
pub(super) fn read(var: &impl Fn(&str) -> Result<String, VarError>) -> Reading {
    let get = |name: &str| var(name).ok().filter(|value| !value.is_empty());
    let flag = |name: &str| match get(name)?.as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    };
    Reading {
        scheme: get("BINGUX_COLOR_SCHEME")
            .or_else(|| get("BINGUX_SETTINGS_SNAPSHOT_SCHEME"))
            .and_then(|raw| parse_scheme(&raw)),
        accent: get("BINGUX_ACCENT").and_then(|raw| crate::theme::accent_named(&raw)),
        high_contrast: flag("BINGUX_HIGH_CONTRAST"),
        reduced_motion: flag("BINGUX_REDUCED_MOTION"),
        text_scale: get("BINGUX_TEXT_SCALE").and_then(|raw| raw.parse().ok()),
        icon_theme: get("BINGUX_ICON_THEME"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Scheme;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Result<String, VarError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| map.get(name).cloned().ok_or(VarError::NotPresent)
    }

    #[test]
    fn nothing_set_says_nothing() {
        assert_eq!(read(&env(&[])), Reading::default());
    }

    #[test]
    fn each_pin_is_read() {
        let found = read(&env(&[
            ("BINGUX_COLOR_SCHEME", "light"),
            ("BINGUX_ACCENT", "teal"),
            ("BINGUX_HIGH_CONTRAST", "1"),
            ("BINGUX_REDUCED_MOTION", "0"),
            ("BINGUX_TEXT_SCALE", "1.5"),
            ("BINGUX_ICON_THEME", "Papirus"),
        ]));
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        assert_eq!(found.accent, Some("teal"));
        assert_eq!(found.high_contrast, Some(true));
        // "0" is a pin too: it overrides a desktop that asks for reduced motion.
        assert_eq!(found.reduced_motion, Some(false));
        assert_eq!(found.text_scale, Some(1.5));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus"));
    }

    #[test]
    fn the_older_snapshot_variable_still_pins_the_scheme() {
        let found = read(&env(&[("BINGUX_SETTINGS_SNAPSHOT_SCHEME", "prefer-light")]));
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        // The newer name wins when both are set.
        let both = read(&env(&[
            ("BINGUX_SETTINGS_SNAPSHOT_SCHEME", "prefer-light"),
            ("BINGUX_COLOR_SCHEME", "dark"),
        ]));
        assert_eq!(both.scheme, Some(Scheme::PreferDark));
    }

    #[test]
    fn a_value_that_means_nothing_is_not_a_pin() {
        let found = read(&env(&[
            ("BINGUX_COLOR_SCHEME", "sepia"),
            ("BINGUX_ACCENT", "chartreuse"),
            ("BINGUX_REDUCED_MOTION", "maybe"),
            ("BINGUX_TEXT_SCALE", "big"),
        ]));
        assert_eq!(found, Reading::default());
    }
}
