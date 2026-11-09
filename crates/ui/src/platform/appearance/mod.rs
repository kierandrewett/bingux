//! How the desktop wants windows to look: colour scheme, accent, contrast,
//! reduced motion, text size and icon theme, from the first source that has an
//! opinion.
//!
//! Sources, strongest first (each [`Source`]):
//!
//! 1. **Environment.** `BINGUX_COLOR_SCHEME`, `BINGUX_ACCENT`,
//!    `BINGUX_REDUCED_MOTION`, `BINGUX_HIGH_CONTRAST`, `BINGUX_TEXT_SCALE` and
//!    `BINGUX_ICON_THEME` pin one value for one run: a capture, a test, a
//!    bug report. `BINGUX_SETTINGS_SNAPSHOT_SCHEME` is the older name of the
//!    first.
//! 2. **Portal.** `org.freedesktop.portal.Settings` on the session bus:
//!    `org.freedesktop.appearance` (`color-scheme`, `accent-color`,
//!    `contrast`, `reduced-motion`) and `org.gnome.desktop.interface`
//!    (`icon-theme`, `enable-animations`, `text-scaling-factor`). Gnoblin's
//!    portal backend publishes the same names, so this is the source of a
//!    Gnoblin session.
//! 3. **Config.** `[appearance]` in `bingux.toml` (`color_scheme`, `accent`,
//!    `high_contrast`, `reduced_motion`, `text_scale`, `icon_theme`): the
//!    desktop's own file, which needs no bus and no GNOME.
//! 4. **GTK settings.** `gtk-4.0/settings.ini` and `gtk-3.0/settings.ini`
//!    (`gtk-application-prefer-dark-theme`, `gtk-icon-theme-name`): files, read
//!    without GTK.
//! 5. **gsettings.** `org.gnome.desktop.interface` and
//!    `org.gnome.desktop.a11y.interface`, only when `gsettings` and those
//!    schemas are installed. A legacy source: the Mutter session still has it.
//! 6. **Default.** Dark, blue, normal contrast, motion on, text at 100 %.
//!
//! Each value is resolved on its own. A portal that knows the scheme but not
//! the accent leaves the accent to `bingux.toml`. A source that has no
//! preference ("no preference" from the portal, a key that is not there) says
//! nothing, and the next source is asked.
//!
//! [`current`] reads every source and blocks, so call it from a worker
//! ([`crate::runtime::background_then`]). [`startup`] reads the environment
//! only and is safe in `render`: it is what the first frame is drawn with.
//! [`watch`] says when a source changed. `BINGUX_APPEARANCE_SOURCES` (for
//! example `config,gsettings`) restricts which sources are read, for testing
//! one layer on a machine that has all of them.

mod config;
mod environment;
mod gsettings;
mod gtk;
mod portal;
mod watch;

pub use watch::{Watch, watch};

use crate::theme::Scheme;

/// Where a value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A `BINGUX_*` variable.
    Environment,
    /// `org.freedesktop.portal.Settings`.
    Portal,
    /// `[appearance]` in `bingux.toml`.
    Config,
    /// GTK's `settings.ini` files.
    GtkSettings,
    /// `gsettings`, when installed.
    Gsettings,
    /// No source had an opinion.
    Default,
}

impl Source {
    /// The sources in the order they are asked, strongest first.
    pub const ORDER: [Source; 5] = [
        Source::Environment,
        Source::Portal,
        Source::Config,
        Source::GtkSettings,
        Source::Gsettings,
    ];

    /// The name `BINGUX_APPEARANCE_SOURCES` and diagnostics use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Portal => "portal",
            Self::Config => "config",
            Self::GtkSettings => "gtk",
            Self::Gsettings => "gsettings",
            Self::Default => "default",
        }
    }
}

/// What one source says. A field it has no opinion on is `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub scheme: Option<Scheme>,
    /// A name from [`crate::theme::accent_names`].
    pub accent: Option<&'static str>,
    pub high_contrast: Option<bool>,
    pub reduced_motion: Option<bool>,
    /// 1.0 is the system's own size.
    pub text_scale: Option<f32>,
    pub icon_theme: Option<String>,
}

/// Which source decided each value of an [`Appearance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub scheme: Source,
    pub accent: Source,
    pub high_contrast: Source,
    pub reduced_motion: Source,
    pub text_scale: Source,
    pub icon_theme: Source,
}

/// The appearance a window takes: every value is decided.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub scheme: Scheme,
    pub accent: &'static str,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub text_scale: f32,
    /// The icon theme to look names up in, when a source names one. `None`
    /// lets the lookup use Adwaita and then hicolor.
    pub icon_theme: Option<String>,
    pub from: Provenance,
}

/// The text scales the desktop may ask for: outside this, a value is a typing
/// mistake, not a preference.
pub const TEXT_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=3.0;

/// The icon theme used when no source names one.
pub const DEFAULT_ICON_THEME: &str = "Adwaita";

/// Settle every value from `layers`, strongest first: the first layer with an
/// opinion on a value decides it, and a value no layer has an opinion on takes
/// its default. Pure: the order rule is tested here.
pub fn resolve(layers: &[(Source, Reading)]) -> Appearance {
    fn pick<T: Clone>(
        layers: &[(Source, Reading)],
        field: impl Fn(&Reading) -> Option<T>,
    ) -> Option<(T, Source)> {
        layers
            .iter()
            .find_map(|(source, reading)| field(reading).map(|value| (value, *source)))
    }
    let (scheme, scheme_from) =
        pick(layers, |r| r.scheme).unwrap_or((Scheme::PreferDark, Source::Default));
    let (accent, accent_from) =
        pick(layers, |r| r.accent).unwrap_or((crate::theme::DEFAULT_ACCENT, Source::Default));
    let (high_contrast, high_contrast_from) =
        pick(layers, |r| r.high_contrast).unwrap_or((false, Source::Default));
    let (reduced_motion, reduced_motion_from) =
        pick(layers, |r| r.reduced_motion).unwrap_or((false, Source::Default));
    let (text_scale, text_scale_from) = pick(layers, |r| {
        r.text_scale
            .filter(|scale| TEXT_SCALE_RANGE.contains(scale))
    })
    .unwrap_or((1.0, Source::Default));
    let (icon_theme, icon_theme_from) = match pick(layers, |r| {
        r.icon_theme.clone().filter(|name| !name.is_empty())
    }) {
        Some((name, source)) => (Some(name), source),
        None => (None, Source::Default),
    };
    Appearance {
        scheme,
        accent,
        high_contrast,
        reduced_motion,
        text_scale,
        icon_theme,
        from: Provenance {
            scheme: scheme_from,
            accent: accent_from,
            high_contrast: high_contrast_from,
            reduced_motion: reduced_motion_from,
            text_scale: text_scale_from,
            icon_theme: icon_theme_from,
        },
    }
}

/// The sources `BINGUX_APPEARANCE_SOURCES` leaves on. Unset: all of them.
/// Names it does not know are ignored; the environment is always read.
fn enabled(list: Option<&str>) -> Vec<Source> {
    let Some(list) = list else {
        return Source::ORDER.to_vec();
    };
    let wanted: Vec<&str> = list.split(',').map(str::trim).collect();
    Source::ORDER
        .into_iter()
        .filter(|source| *source == Source::Environment || wanted.contains(&source.name()))
        .collect()
}

/// The appearance of the first frame: the environment and the defaults, with
/// nothing read from the system. Cheap and non-blocking, so `render` may call
/// it; [`current`] follows on a worker.
pub fn startup() -> Appearance {
    resolve(&[(
        Source::Environment,
        environment::read(&|name: &str| std::env::var(name)),
    )])
}

/// Read every source and settle the appearance. Blocks (the portal, a
/// `gsettings` child, files): the longest a missing or hung source costs is its
/// own time limit. Call it off the UI thread.
pub fn current() -> Appearance {
    crate::runtime::assert_off_ui_thread("platform::appearance::current");
    let sources = enabled(std::env::var("BINGUX_APPEARANCE_SOURCES").ok().as_deref());
    let isolated = crate::runtime::system_settings_isolated();
    let mut layers: Vec<(Source, Reading)> = Vec::new();
    for source in sources {
        let reading = match source {
            Source::Environment => environment::read(&|name: &str| std::env::var(name)),
            // A capture or a test never asks the person's desktop.
            Source::Portal | Source::Gsettings if isolated => continue,
            Source::Portal => match portal::read() {
                Some(reading) => reading,
                None => continue,
            },
            Source::Config => config::read(),
            Source::GtkSettings => gtk::read(),
            Source::Gsettings => gsettings::read(),
            Source::Default => continue,
        };
        layers.push((source, reading));
    }
    resolve(&layers)
}

/// The interface font family `bingux.toml` names (`appearance.font_family`),
/// when it names one. Blocks like [`current`]; the font is read once at start.
pub fn configured_font_family() -> Option<String> {
    config::font_family()
}

/// The icon theme to look icons up in: the appearance's, else
/// [`DEFAULT_ICON_THEME`]. Blocks like [`current`].
pub fn icon_theme() -> String {
    current()
        .icon_theme
        .unwrap_or_else(|| DEFAULT_ICON_THEME.to_owned())
}

/// A scheme from the words sources use: `dark` and `prefer-dark`, `light` and
/// `prefer-light`, `default` and `no-preference`. Quotes (gsettings prints
/// them) and case are ignored. `None` for anything else.
pub fn parse_scheme(raw: &str) -> Option<Scheme> {
    match raw
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .to_ascii_lowercase()
        .as_str()
    {
        "dark" | "prefer-dark" => Some(Scheme::PreferDark),
        "light" | "prefer-light" => Some(Scheme::PreferLight),
        "default" | "no-preference" => Some(Scheme::Default),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(source: Source, build: impl FnOnce(&mut Reading)) -> (Source, Reading) {
        let mut reading = Reading::default();
        build(&mut reading);
        (source, reading)
    }

    #[test]
    fn with_no_source_the_defaults_apply() {
        let found = resolve(&[]);
        assert_eq!(found.scheme, Scheme::PreferDark);
        assert_eq!(found.accent, "blue");
        assert!(!found.high_contrast && !found.reduced_motion);
        assert_eq!(found.text_scale, 1.0);
        assert_eq!(found.icon_theme, None);
        assert_eq!(found.from.scheme, Source::Default);
    }

    #[test]
    fn the_strongest_source_with_an_opinion_decides_each_value() {
        let found = resolve(&[
            layer(Source::Environment, |_| {}),
            layer(Source::Portal, |r| r.scheme = Some(Scheme::PreferLight)),
            layer(Source::Config, |r| {
                r.scheme = Some(Scheme::PreferDark);
                r.accent = Some("purple");
            }),
            layer(Source::Gsettings, |r| {
                r.accent = Some("red");
                r.reduced_motion = Some(true);
            }),
        ]);
        assert_eq!(
            (found.scheme, found.from.scheme),
            (Scheme::PreferLight, Source::Portal)
        );
        assert_eq!(
            (found.accent, found.from.accent),
            ("purple", Source::Config)
        );
        assert_eq!(
            (found.reduced_motion, found.from.reduced_motion),
            (true, Source::Gsettings)
        );
    }

    #[test]
    fn the_order_is_environment_portal_config_gtk_gsettings() {
        let all = |value: &'static str| -> Reading {
            Reading {
                accent: Some(value),
                ..Reading::default()
            }
        };
        let layers: Vec<(Source, Reading)> = vec![
            (Source::Environment, all("teal")),
            (Source::Portal, all("green")),
            (Source::Config, all("yellow")),
            (Source::GtkSettings, all("orange")),
            (Source::Gsettings, all("red")),
        ];
        // Dropping the strongest layer one at a time walks down the chain.
        for (index, expect) in ["teal", "green", "yellow", "orange", "red"]
            .iter()
            .enumerate()
        {
            let found = resolve(&layers[index..]);
            assert_eq!(found.accent, *expect, "from layer {index}");
            assert_eq!(found.from.accent, layers[index].0);
        }
        assert_eq!(resolve(&[]).from.accent, Source::Default);
    }

    #[test]
    fn a_value_out_of_range_is_not_an_opinion() {
        let found = resolve(&[
            layer(Source::Portal, |r| r.text_scale = Some(40.0)),
            layer(Source::Config, |r| r.text_scale = Some(1.25)),
        ]);
        assert_eq!(
            (found.text_scale, found.from.text_scale),
            (1.25, Source::Config)
        );
        let empty_name = resolve(&[layer(Source::Portal, |r| {
            r.icon_theme = Some(String::new())
        })]);
        assert_eq!(empty_name.icon_theme, None);
    }

    #[test]
    fn the_sources_can_be_restricted_but_the_environment_is_always_read() {
        assert_eq!(enabled(None), Source::ORDER.to_vec());
        assert_eq!(
            enabled(Some("config, gsettings")),
            [Source::Environment, Source::Config, Source::Gsettings]
        );
        assert_eq!(enabled(Some("nonsense")), [Source::Environment]);
    }

    #[test]
    fn scheme_words_are_read_the_way_every_source_spells_them() {
        for (raw, scheme) in [
            ("dark", Scheme::PreferDark),
            ("'prefer-dark'", Scheme::PreferDark),
            ("Light", Scheme::PreferLight),
            ("prefer-light", Scheme::PreferLight),
            ("default", Scheme::Default),
            ("\"no-preference\"", Scheme::Default),
        ] {
            assert_eq!(parse_scheme(raw), Some(scheme), "{raw}");
        }
        assert_eq!(parse_scheme("sepia"), None);
        assert_eq!(parse_scheme(""), None);
    }
}
