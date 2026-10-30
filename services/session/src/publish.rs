//! The user-side appearance publisher for bsm.
//!
//! bsm cannot read home directories and never opens a path a user names. It
//! takes the wallpaper and the avatar as open files from the account's own
//! process, plus a small whitelisted record. This module works out what this
//! account's login screen should show, the same way the lock screen and the
//! shell do, and hands it to `bsmctl publish-appearance`.
//!
//! Sources, in order:
//!
//! * wallpaper: the Bingux configuration (`desktop.wallpaper`, then the GNOME
//!   background), exactly what `bingux-session export` resolves
//!   (`snapshot::configured_wallpaper`);
//! * avatar: the AccountsService icon `/var/lib/AccountsService/icons/<user>`,
//!   then `~/.face`, then `~/.face.icon`;
//! * colour scheme and accent: gsettings `org.gnome.desktop.interface`
//!   `color-scheme` and `accent-color`, as `Theme.qml` reads them, including
//!   its lighter dark-scheme accent;
//! * clock format: gsettings `clock-format`, else the locale's time format;
//! * reduced motion: `BINGUX_REDUCED_MOTION=1` or `BINGUX_SOFTWARE_RENDERING=1`
//!   (what the shell honours), or gsettings `enable-animations` false;
//! * personal images: `privacy.show_personal_images` in `bingux.toml`, on by
//!   default. Off sends no images and bsm removes the published ones.
//!
//! Everything external goes through [`Sources`], so the rules are tested
//! without a desktop.

use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Largest image handed over: `bsmctl publish-appearance` refuses anything
/// bigger, and one refused image would fail the whole publish.
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const DEFAULT_ACCENT: &str = "blue";

/// GNOME's accent colours, the values `Theme.qml` uses.
const ACCENTS: [(&str, [f64; 3]); 9] = [
    ("blue", [0.208, 0.518, 0.894]),
    ("teal", [0.129, 0.565, 0.643]),
    ("green", [0.227, 0.580, 0.290]),
    ("yellow", [0.784, 0.533, 0.000]),
    ("orange", [0.929, 0.357, 0.000]),
    ("red", [0.902, 0.176, 0.259]),
    ("pink", [0.835, 0.380, 0.600]),
    ("purple", [0.569, 0.255, 0.675]),
    ("slate", [0.435, 0.514, 0.588]),
];

/// Where the facts come from. The real implementation is [`System`].
pub trait Sources {
    /// A `bingux-config get KEY` value, trimmed, or `None` when unavailable.
    fn config(&self, key: &str) -> Option<String>;
    /// The wallpaper path Bingux resolves for this account, or empty.
    fn wallpaper(&self) -> String;
    /// A gsettings value on `org.gnome.desktop.interface`, or `None`.
    fn gsettings(&self, key: &str) -> Option<String>;
    /// An environment variable.
    fn env(&self, key: &str) -> Option<String>;
    /// The locale's `t_fmt` time format, or `None`.
    fn locale_time_format(&self) -> Option<String>;
    /// The account's login name and home directory.
    fn user(&self) -> Option<(String, PathBuf)>;
    /// The AccountsService icons directory.
    fn icons_dir(&self) -> PathBuf;
}

pub struct System;

impl Sources for System {
    fn config(&self, key: &str) -> Option<String> {
        let output = Command::new("bingux-config")
            .args(["get", key])
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn wallpaper(&self) -> String {
        crate::snapshot::configured_wallpaper()
    }

    fn gsettings(&self, key: &str) -> Option<String> {
        let output = Command::new("gsettings")
            .args(["get", INTERFACE_SCHEMA, key])
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn locale_time_format(&self) -> Option<String> {
        let output = Command::new("locale")
            .arg("t_fmt")
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn user(&self) -> Option<(String, PathBuf)> {
        let name = crate::accounts::current_user_name()?;
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some((name, home))
    }

    fn icons_dir(&self) -> PathBuf {
        PathBuf::from("/var/lib/AccountsService/icons")
    }
}

/// What this account's login screen should show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Appearance {
    pub color_scheme: &'static str,
    pub accent: String,
    pub clock_format: &'static str,
    pub reduce_motion: bool,
    pub show_personal_images: bool,
    /// Absent when none is configured, it cannot be read, or the account opted out.
    pub wallpaper: Option<PathBuf>,
    pub avatar: Option<PathBuf>,
}

pub fn resolve(sources: &dyn Sources) -> Appearance {
    let show_personal_images = sources
        .config("privacy.show_personal_images")
        .is_none_or(|value| value != "false");
    let color_scheme = color_scheme(sources.gsettings("color-scheme").as_deref());
    let accent = accent_hex(
        sources.gsettings("accent-color").as_deref(),
        color_scheme == "dark",
    );
    let (wallpaper, avatar) = if show_personal_images {
        (
            usable_image(&sources.wallpaper()),
            avatar(sources).filter(|path| usable_image(&path.display().to_string()).is_some()),
        )
    } else {
        (None, None)
    };
    Appearance {
        color_scheme,
        accent,
        clock_format: clock_format(sources),
        reduce_motion: reduce_motion(sources),
        show_personal_images,
        wallpaper,
        avatar,
    }
}

/// `prefer-dark` is dark. Bingux defaults to dark when the setting is
/// unreadable; `default` and `prefer-light` are light, as in `Theme.qml`.
fn color_scheme(value: Option<&str>) -> &'static str {
    match value.map(unquote) {
        Some("prefer-light" | "default") => "light",
        _ => "dark",
    }
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(['\'', '"']).trim()
}

/// `#rrggbb` for a GNOME accent name. An unknown or missing name is blue. The
/// dark scheme uses the lighter variant of each hue, as `Theme.accentColorFor`.
fn accent_hex(value: Option<&str>, dark: bool) -> String {
    let name = value.map(unquote).unwrap_or(DEFAULT_ACCENT);
    let base = ACCENTS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map_or(ACCENTS[0].1, |(_, rgb)| *rgb);
    let channel = |value: f64| {
        let shown = if dark { value + (1.0 - value) * 0.22 } else { value };
        (shown * 255.0).round() as u8
    };
    format!("#{:02x}{:02x}{:02x}", channel(base[0]), channel(base[1]), channel(base[2]))
}

fn clock_format(sources: &dyn Sources) -> &'static str {
    match sources.gsettings("clock-format").as_deref().map(unquote) {
        Some("12h") => return "12h",
        Some("24h") => return "24h",
        _ => {}
    }
    // The locale decides when the user never chose: a 12 hour pattern uses
    // %I, %l, %p, %P or %r.
    match sources.locale_time_format() {
        Some(format) if ["%I", "%l", "%p", "%P", "%r"].iter().any(|code| format.contains(code)) => {
            "12h"
        }
        _ => "24h",
    }
}

fn reduce_motion(sources: &dyn Sources) -> bool {
    let flag = |key: &str| sources.env(key).is_some_and(|value| value == "1");
    flag("BINGUX_REDUCED_MOTION")
        || flag("BINGUX_SOFTWARE_RENDERING")
        || sources.gsettings("enable-animations").as_deref().map(unquote) == Some("false")
}

/// A regular file a user can read, at most `MAX_IMAGE_BYTES`. A missing file
/// means "no image", not an error, so a removed wallpaper clears the copy.
fn usable_image(path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return None;
    }
    let metadata = fs::metadata(path).ok()?;
    (metadata.is_file() && metadata.len() <= MAX_IMAGE_BYTES && fs::File::open(path).is_ok())
        .then(|| path.to_path_buf())
}

fn avatar(sources: &dyn Sources) -> Option<PathBuf> {
    let (name, home) = sources.user()?;
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return None;
    }
    [
        sources.icons_dir().join(&name),
        home.join(".face"),
        home.join(".face.icon"),
    ]
    .into_iter()
    .find(|path| usable_image(&path.display().to_string()).is_some())
}

/// The arguments after `bsmctl`. Optional images are left out, never empty:
/// bsm treats a missing image as "show none".
pub fn bsmctl_arguments(appearance: &Appearance) -> Vec<String> {
    let mut arguments = vec![
        "publish-appearance".to_string(),
        "--color-scheme".to_string(),
        appearance.color_scheme.to_string(),
        "--accent".to_string(),
        appearance.accent.clone(),
        "--clock-format".to_string(),
        appearance.clock_format.to_string(),
        "--reduce-motion".to_string(),
        appearance.reduce_motion.to_string(),
        "--show-personal-images".to_string(),
        appearance.show_personal_images.to_string(),
    ];
    if let Some(path) = &appearance.wallpaper {
        arguments.push("--wallpaper".to_string());
        arguments.push(path.display().to_string());
    }
    if let Some(path) = &appearance.avatar {
        arguments.push("--avatar".to_string());
        arguments.push(path.display().to_string());
    }
    arguments
}

/// Runs `bsmctl` (`BINGUX_BSMCTL` names another binary, for tests) and returns
/// its exit status as this command's.
pub fn publish(appearance: &Appearance) -> i32 {
    let program = std::env::var("BINGUX_BSMCTL").unwrap_or_else(|_| "bsmctl".to_string());
    match Command::new(&program).args(bsmctl_arguments(appearance)).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("bingux-session: cannot run {program}: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        config: HashMap<&'static str, &'static str>,
        gsettings: HashMap<&'static str, &'static str>,
        env: HashMap<&'static str, &'static str>,
        locale: Option<&'static str>,
        wallpaper: String,
        home: PathBuf,
        icons: PathBuf,
        name: &'static str,
    }

    impl Sources for Fake {
        fn config(&self, key: &str) -> Option<String> {
            self.config.get(key).map(|v| v.to_string())
        }
        fn wallpaper(&self) -> String {
            self.wallpaper.clone()
        }
        fn gsettings(&self, key: &str) -> Option<String> {
            self.gsettings.get(key).map(|v| v.to_string())
        }
        fn env(&self, key: &str) -> Option<String> {
            self.env.get(key).map(|v| v.to_string())
        }
        fn locale_time_format(&self) -> Option<String> {
            self.locale.map(str::to_string)
        }
        fn user(&self) -> Option<(String, PathBuf)> {
            Some((self.name.to_string(), self.home.clone()))
        }
        fn icons_dir(&self) -> PathBuf {
            self.icons.clone()
        }
    }

    fn directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bingux-publish-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake(root: &Path) -> Fake {
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("icons")).unwrap();
        Fake {
            home: root.join("home"),
            icons: root.join("icons"),
            name: "ada",
            ..Fake::default()
        }
    }

    #[test]
    fn defaults_without_any_setting_are_dark_blue_24h_with_images_on() {
        let root = directory("defaults");
        let found = resolve(&fake(&root));
        assert_eq!(found.color_scheme, "dark");
        assert_eq!(found.accent, "#619fea"); // blue, lightened for the dark scheme
        assert_eq!(found.clock_format, "24h");
        assert!(!found.reduce_motion);
        assert!(found.show_personal_images);
        assert_eq!((found.wallpaper, found.avatar), (None, None));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scheme_and_accent_follow_gsettings_like_the_shell_theme() {
        let root = directory("scheme");
        let mut sources = fake(&root);
        sources.gsettings.insert("color-scheme", "'prefer-light'");
        sources.gsettings.insert("accent-color", "'red'");
        let light = resolve(&sources);
        assert_eq!(light.color_scheme, "light");
        assert_eq!(light.accent, "#e62d42"); // the unlightened GNOME red
        sources.gsettings.insert("color-scheme", "'default'");
        assert_eq!(resolve(&sources).color_scheme, "light");
        sources.gsettings.insert("color-scheme", "'prefer-dark'");
        let dark = resolve(&sources);
        assert_eq!(dark.color_scheme, "dark");
        assert_ne!(dark.accent, light.accent);
        assert!(dark.accent.starts_with('#') && dark.accent.len() == 7);
        // An unknown accent is blue, never an arbitrary string.
        sources.gsettings.insert("accent-color", "'#ff0000; rm -rf'");
        assert_eq!(resolve(&sources).accent, accent_hex(Some("blue"), true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_accent_is_a_plain_rrggbb_in_both_schemes() {
        for (name, _) in ACCENTS {
            for dark in [false, true] {
                let hex = accent_hex(Some(&format!("'{name}'")), dark);
                assert_eq!(hex.len(), 7, "{name}");
                assert!(hex[1..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            }
        }
        assert_eq!(accent_hex(Some("'blue'"), false), "#3584e4");
    }

    #[test]
    fn clock_format_prefers_the_setting_then_the_locale() {
        let root = directory("clock");
        let mut sources = fake(&root);
        sources.gsettings.insert("clock-format", "'12h'");
        sources.locale = Some("%H:%M:%S");
        assert_eq!(resolve(&sources).clock_format, "12h");
        sources.gsettings.insert("clock-format", "'24h'");
        sources.locale = Some("%r");
        assert_eq!(resolve(&sources).clock_format, "24h");
        sources.gsettings.clear();
        assert_eq!(resolve(&sources).clock_format, "12h", "%r is a 12 hour pattern");
        sources.locale = Some("%l:%M %P");
        assert_eq!(resolve(&sources).clock_format, "12h");
        sources.locale = Some("%T");
        assert_eq!(resolve(&sources).clock_format, "24h");
        sources.locale = None;
        assert_eq!(resolve(&sources).clock_format, "24h");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reduced_motion_follows_the_shell_switches_and_gnome_animations() {
        let root = directory("motion");
        let mut sources = fake(&root);
        assert!(!resolve(&sources).reduce_motion);
        sources.gsettings.insert("enable-animations", "true");
        assert!(!resolve(&sources).reduce_motion);
        sources.gsettings.insert("enable-animations", "false");
        assert!(resolve(&sources).reduce_motion);
        sources.gsettings.clear();
        sources.env.insert("BINGUX_REDUCED_MOTION", "1");
        assert!(resolve(&sources).reduce_motion);
        sources.env.clear();
        sources.env.insert("BINGUX_SOFTWARE_RENDERING", "1");
        assert!(resolve(&sources).reduce_motion);
        sources.env.insert("BINGUX_SOFTWARE_RENDERING", "0");
        assert!(!resolve(&sources).reduce_motion);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn images_come_from_the_configured_wallpaper_and_the_account_icon_then_face() {
        let root = directory("images");
        let mut sources = fake(&root);
        let wallpaper = root.join("wall.png");
        fs::write(&wallpaper, b"png").unwrap();
        sources.wallpaper = wallpaper.display().to_string();
        let found = resolve(&sources);
        assert_eq!(found.wallpaper.as_deref(), Some(wallpaper.as_path()));
        assert_eq!(found.avatar, None);
        fs::write(root.join("home/.face"), b"face").unwrap();
        assert_eq!(resolve(&sources).avatar, Some(root.join("home/.face")));
        fs::write(root.join("icons/ada"), b"icon").unwrap();
        assert_eq!(
            resolve(&sources).avatar,
            Some(root.join("icons/ada")),
            "the AccountsService icon wins over ~/.face"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_unreadable_or_relative_images_are_left_out_so_bsm_clears_them() {
        let root = directory("missing");
        let mut sources = fake(&root);
        sources.wallpaper = root.join("gone.png").display().to_string();
        assert_eq!(resolve(&sources).wallpaper, None);
        sources.wallpaper = "relative/wall.png".into();
        assert_eq!(resolve(&sources).wallpaper, None);
        sources.wallpaper = root.display().to_string(); // a directory
        assert_eq!(resolve(&sources).wallpaper, None);
        sources.name = "../etc";
        fs::write(root.join("icons/ada"), b"icon").unwrap();
        assert_eq!(resolve(&sources).avatar, None, "a path-like user name is refused");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opting_out_sends_no_image_at_all() {
        let root = directory("optout");
        let mut sources = fake(&root);
        let wallpaper = root.join("wall.png");
        fs::write(&wallpaper, b"png").unwrap();
        fs::write(root.join("icons/ada"), b"icon").unwrap();
        sources.wallpaper = wallpaper.display().to_string();
        sources.config.insert("privacy.show_personal_images", "false");
        let found = resolve(&sources);
        assert!(!found.show_personal_images);
        assert_eq!((&found.wallpaper, &found.avatar), (&None, &None));
        let arguments = bsmctl_arguments(&found);
        assert!(!arguments.contains(&"--wallpaper".to_string()));
        assert!(!arguments.contains(&"--avatar".to_string()));
        let at = arguments.iter().position(|a| a == "--show-personal-images").unwrap();
        assert_eq!(arguments[at + 1], "false");
        // Anything but a plain "false", or no answer, keeps the default.
        sources.config.insert("privacy.show_personal_images", "true");
        assert!(resolve(&sources).show_personal_images);
        sources.config.clear();
        assert!(resolve(&sources).show_personal_images);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bsmctl_arguments_carry_every_field_and_only_existing_images() {
        let appearance = Appearance {
            color_scheme: "dark",
            accent: "#619fea".into(),
            clock_format: "12h",
            reduce_motion: true,
            show_personal_images: true,
            wallpaper: Some("/home/ada/wall.png".into()),
            avatar: None,
        };
        assert_eq!(
            bsmctl_arguments(&appearance),
            [
                "publish-appearance",
                "--color-scheme",
                "dark",
                "--accent",
                "#619fea",
                "--clock-format",
                "12h",
                "--reduce-motion",
                "true",
                "--show-personal-images",
                "true",
                "--wallpaper",
                "/home/ada/wall.png",
            ]
        );
    }

    #[test]
    fn publish_reports_the_exit_status_of_bsmctl_and_a_missing_binary() {
        let root = directory("exec");
        let script = root.join("fake-bsmctl");
        fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\nexit 3\n").unwrap();
        fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let appearance = resolve(&fake(&root));
        // SAFETY: tests in this module run on one thread.
        unsafe { std::env::set_var("BINGUX_BSMCTL", &script) };
        assert_eq!(publish(&appearance), 3);
        let recorded = fs::read_to_string(root.join("fake-bsmctl.args")).unwrap();
        assert_eq!(recorded.lines().next(), Some("publish-appearance"));
        unsafe { std::env::set_var("BINGUX_BSMCTL", root.join("missing")) };
        assert_eq!(publish(&appearance), 1);
        unsafe { std::env::remove_var("BINGUX_BSMCTL") };
        fs::remove_dir_all(root).unwrap();
    }
}
