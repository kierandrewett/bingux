//! The gsettings source: GNOME's interface and accessibility keys, read with the
//! `gsettings` command when it and the schemas are installed.
//!
//! A legacy source. The Mutter session keeps its appearance here, so a session
//! that has only gsettings still looks right; everything else reaches the same
//! values through the portal or `bingux.toml` and never starts `gsettings`.
//! Without the program or the schema, each read fails at once and says nothing.

use std::time::Duration;

use super::{Reading, parse_scheme};

const INTERFACE: &str = "org.gnome.desktop.interface";
const A11Y: &str = "org.gnome.desktop.a11y.interface";
const WITHIN: Duration = Duration::from_millis(1200);

pub(super) fn read() -> Reading {
    let dump = |schema: &str| {
        crate::runtime::run("gsettings", &["list-recursively", schema], WITHIN).unwrap_or_default()
    };
    parse(&dump(INTERFACE), &dump(A11Y))
}

/// What the two schemas' `list-recursively` output says.
pub(super) fn parse(interface: &str, a11y: &str) -> Reading {
    let key = |text: &str, schema: &str, name: &str| -> Option<String> {
        text.lines().find_map(|line| {
            let mut words = line.splitn(3, ' ');
            (words.next()? == schema && words.next()? == name)
                .then(|| words.next().unwrap_or("").trim().to_owned())
        })
    };
    let unquote = |raw: String| raw.trim_matches('\'').to_owned();
    Reading {
        scheme: key(interface, INTERFACE, "color-scheme").and_then(|raw| parse_scheme(&raw)),
        accent: key(interface, INTERFACE, "accent-color")
            .and_then(|raw| crate::theme::accent_named(&raw)),
        icon_theme: key(interface, INTERFACE, "icon-theme")
            .map(unquote)
            .filter(|name| !name.is_empty()),
        text_scale: key(interface, INTERFACE, "text-scaling-factor")
            .and_then(|raw| raw.parse().ok()),
        // Animations are on by default: `true` is no preference, `false` asks
        // for less motion.
        reduced_motion: key(interface, INTERFACE, "enable-animations")
            .filter(|raw| raw == "false")
            .map(|_| true),
        high_contrast: key(a11y, A11Y, "high-contrast")
            .filter(|raw| raw == "true")
            .map(|_| true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Scheme;

    const INTERFACE_DUMP: &str = "org.gnome.desktop.interface color-scheme 'prefer-light'\n\
org.gnome.desktop.interface accent-color 'pink'\n\
org.gnome.desktop.interface icon-theme 'Papirus'\n\
org.gnome.desktop.interface text-scaling-factor 1.25\n\
org.gnome.desktop.interface enable-animations false\n\
org.gnome.desktop.interface font-name 'Cantarell 11'\n";

    #[test]
    fn the_interface_keys_are_read() {
        let found = parse(INTERFACE_DUMP, "");
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        assert_eq!(found.accent, Some("pink"));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus"));
        assert_eq!(found.text_scale, Some(1.25));
        assert_eq!(found.reduced_motion, Some(true));
        assert_eq!(found.high_contrast, None);
    }

    #[test]
    fn the_accessibility_schema_gives_contrast() {
        let found = parse("", "org.gnome.desktop.a11y.interface high-contrast true\n");
        assert_eq!(found.high_contrast, Some(true));
        let off = parse("", "org.gnome.desktop.a11y.interface high-contrast false\n");
        assert_eq!(off.high_contrast, None);
    }

    #[test]
    fn animations_left_on_are_no_preference() {
        let found = parse("org.gnome.desktop.interface enable-animations true\n", "");
        assert_eq!(found.reduced_motion, None);
    }

    #[test]
    fn no_schema_says_nothing() {
        assert_eq!(parse("", ""), Reading::default());
    }
}
