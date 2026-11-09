//! The config source: `[appearance]` in `bingux.toml`.
//!
//! The file belongs to `bingux-config`, which validates all of it and refuses a
//! bad key. This reader is deliberately more forgiving: it takes the table it
//! needs and ignores the rest, so a key it does not understand, in this table
//! or another, costs one value and not the whole appearance.

use std::path::{Path, PathBuf};

use super::{Reading, parse_scheme};

/// The directory of `bingux.toml`: `$XDG_CONFIG_HOME/bingux`, or
/// `~/.config/bingux`. The same rule as `bingux_config::config_dir`.
pub(crate) fn directory() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("bingux")
}

/// Read the file. A missing or unreadable file says nothing.
pub(super) fn read() -> Reading {
    match crate::runtime::read_to_string(directory().join("bingux.toml")) {
        Ok(text) => parse(&text),
        Err(_) => Reading::default(),
    }
}

/// What a `bingux.toml` says about appearance. A syntax error says nothing.
pub(super) fn parse(text: &str) -> Reading {
    let Ok(document) = text.parse::<toml::Table>() else {
        return Reading::default();
    };
    let Some(table) = document.get("appearance").and_then(toml::Value::as_table) else {
        return Reading::default();
    };
    let text_of = |key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    Reading {
        // "auto" is the file's way to say "ask the next source".
        scheme: text_of("color_scheme").and_then(parse_scheme),
        accent: text_of("accent").and_then(crate::theme::accent_named),
        high_contrast: table.get("high_contrast").and_then(toml::Value::as_bool),
        reduced_motion: table.get("reduced_motion").and_then(toml::Value::as_bool),
        text_scale: table.get("text_scale").and_then(|value| {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|whole| whole as f64))
                .map(|scale| scale as f32)
        }),
        icon_theme: text_of("icon_theme").map(str::to_owned),
    }
}

/// `appearance.font_family` when it is set and not empty. Read from the file
/// directly, so the interface font needs no `bingux-config` program.
pub(super) fn font_family() -> Option<String> {
    let text = crate::runtime::read_to_string(directory().join("bingux.toml")).ok()?;
    font_family_in(&text)
}

fn font_family_in(text: &str) -> Option<String> {
    let document = text.parse::<toml::Table>().ok()?;
    let family = document
        .get("appearance")?
        .as_table()?
        .get("font_family")?
        .as_str()?
        .trim();
    (!family.is_empty()).then(|| family.to_owned())
}

/// The folder to watch for a change to the file, when it exists.
pub(crate) fn watched_directory() -> Option<PathBuf> {
    let directory = directory();
    Path::is_dir(&directory).then_some(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Scheme;

    #[test]
    fn the_appearance_table_is_read() {
        let found = parse(
            "[appearance]\n\
             font_family = \"Cantarell\"\n\
             color_scheme = \"light\"\n\
             accent = \"purple\"\n\
             high_contrast = true\n\
             reduced_motion = false\n\
             text_scale = 1.25\n\
             icon_theme = \"Papirus\"\n",
        );
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        assert_eq!(found.accent, Some("purple"));
        assert_eq!(found.high_contrast, Some(true));
        assert_eq!(found.reduced_motion, Some(false));
        assert_eq!(found.text_scale, Some(1.25));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus"));
    }

    #[test]
    fn a_whole_number_is_a_text_scale_too() {
        assert_eq!(
            parse("[appearance]\ntext_scale = 2\n").text_scale,
            Some(2.0)
        );
    }

    #[test]
    fn a_file_without_the_table_says_nothing() {
        assert_eq!(parse(""), Reading::default());
        assert_eq!(parse("[desktop]\ndock = true\n"), Reading::default());
    }

    #[test]
    fn the_font_family_is_read_unless_empty() {
        assert_eq!(
            font_family_in("[appearance]\nfont_family = \" Cantarell \"\n").as_deref(),
            Some("Cantarell")
        );
        assert_eq!(font_family_in("[appearance]\nfont_family = \"\"\n"), None);
        assert_eq!(font_family_in("[desktop]\n"), None);
    }

    #[test]
    fn a_bad_value_costs_that_value_only() {
        let found = parse(
            "[appearance]\ncolor_scheme = \"sepia\"\naccent = \"teal\"\nreduced_motion = \"yes\"\n\
             [search]\nengines = 7\n",
        );
        assert_eq!(found.scheme, None);
        assert_eq!(found.accent, Some("teal"));
        assert_eq!(found.reduced_motion, None);
    }

    #[test]
    fn auto_and_empty_ask_the_next_source() {
        assert_eq!(
            parse("[appearance]\ncolor_scheme = \"auto\"\n").scheme,
            None
        );
        assert_eq!(parse("[appearance]\ncolor_scheme = \"\"\n").scheme, None);
    }

    #[test]
    fn a_syntax_error_says_nothing() {
        assert_eq!(parse("[appearance\naccent = "), Reading::default());
    }
}
