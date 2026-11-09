//! The GTK settings source: `gtk-4.0/settings.ini` and `gtk-3.0/settings.ini`.
//!
//! Plain files that desktops, theme tools and people write for GTK
//! applications. They are read as files; GTK is not needed. A key counts only
//! when it says something: `gtk-application-prefer-dark-theme=false` is GTK's
//! default and is not a preference for light.

use std::path::{Path, PathBuf};

use super::Reading;
use crate::theme::Scheme;

/// `$XDG_CONFIG_HOME`, or `~/.config`.
fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

/// The two files, GTK 4's first.
fn files(home: &Path) -> [PathBuf; 2] {
    [
        home.join("gtk-4.0").join("settings.ini"),
        home.join("gtk-3.0").join("settings.ini"),
    ]
}

pub(super) fn read() -> Reading {
    let texts: Vec<String> = files(&config_home())
        .iter()
        .filter_map(|path| crate::runtime::read_to_string(path).ok())
        .collect();
    parse(&texts.iter().map(String::as_str).collect::<Vec<_>>())
}

/// What `settings.ini` texts say, the first text's keys winning.
pub(super) fn parse(texts: &[&str]) -> Reading {
    let value = |key: &str| texts.iter().find_map(|text| setting(text, key));
    Reading {
        scheme: value("gtk-application-prefer-dark-theme")
            .filter(|raw| truthy(raw))
            .map(|_| Scheme::PreferDark),
        icon_theme: value("gtk-icon-theme-name"),
        reduced_motion: value("gtk-enable-animations")
            .filter(|raw| falsy(raw))
            .map(|_| true),
        ..Reading::default()
    }
}

fn truthy(raw: &str) -> bool {
    matches!(raw.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

fn falsy(raw: &str) -> bool {
    matches!(raw.to_ascii_lowercase().as_str(), "0" | "false" | "no")
}

/// The value of `key` in the `[Settings]` group of an ini text.
fn setting(text: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Settings]";
            continue;
        }
        if !inside || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            let value = value.trim();
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTK4: &str =
        "[Settings]\ngtk-application-prefer-dark-theme=1\ngtk-icon-theme-name=Papirus-Dark\n";
    const GTK3: &str = "# written by a tool\n[Settings]\ngtk-icon-theme-name = Adwaita\ngtk-enable-animations=false\n";

    #[test]
    fn the_settings_group_is_read() {
        let found = parse(&[GTK4]);
        assert_eq!(found.scheme, Some(Scheme::PreferDark));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus-Dark"));
        assert_eq!(found.reduced_motion, None);
    }

    #[test]
    fn gtk_4_wins_over_gtk_3_key_by_key() {
        let found = parse(&[GTK4, GTK3]);
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus-Dark"));
        // GTK 4's file says nothing about animations, so GTK 3's does.
        assert_eq!(found.reduced_motion, Some(true));
    }

    #[test]
    fn the_default_value_is_not_a_preference() {
        let found = parse(&["[Settings]\ngtk-application-prefer-dark-theme=false\n"]);
        assert_eq!(found.scheme, None);
    }

    #[test]
    fn keys_outside_the_settings_group_are_ignored() {
        let found = parse(&["[Other]\ngtk-icon-theme-name=Wrong\n"]);
        assert_eq!(found.icon_theme, None);
    }
}
