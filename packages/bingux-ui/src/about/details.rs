//! The plain-text block "Copy details" puts on the clipboard, and the facts
//! it is made of.
//!
//! [`details_text`] is a pure function of an [`AppInfo`] and an
//! [`Environment`], so what a bug report carries is tested without a window.
//! [`Facts`] are the parts of the environment that are read from the system:
//! they are asked once, by `Application::run`, before the first window opens.

use std::sync::OnceLock;

use super::info::{AppInfo, short_commit};

/// The Rust toolchain bingux-ui was compiled with, from `rustc --version`
/// (set by this crate's `build.rs`).
pub(crate) const RUSTC: &str = match option_env!("BINGUX_UI_RUSTC") {
    Some(version) => version,
    None => "",
};
/// The Freya version bingux-ui asks for in its `Cargo.toml`.
pub(crate) const FREYA: &str = match option_env!("BINGUX_UI_FREYA") {
    Some(version) => version,
    None => "",
};
/// This crate's version.
pub(crate) const BINGUX_UI: &str = env!("CARGO_PKG_VERSION");

/// What is read from the system once, at launch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// `PRETTY_NAME` of `/etc/os-release`: "Fedora Linux 43 (Workstation)".
    pub os: String,
    /// The kernel release, `uname -r`.
    pub kernel: String,
    /// The session: "wayland, sway" (`XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`).
    pub session: String,
    /// The graphics driver and PCI id of the first display adapter:
    /// "amdgpu (1002:73bf)". Empty when the kernel does not say.
    pub graphics: String,
}

static FACTS: OnceLock<Facts> = OnceLock::new();

impl Facts {
    /// Read the facts from the system. Blocking: call it before the window
    /// opens, never from a render.
    pub(crate) fn read() -> Self {
        let text = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let os = text("/etc/os-release")
            .lines()
            .find_map(|line| line.strip_prefix("PRETTY_NAME="))
            .map(|value| value.trim().trim_matches('"').to_owned())
            .unwrap_or_default();
        let env = |name: &str| std::env::var(name).unwrap_or_default();
        let session = [env("XDG_SESSION_TYPE"), env("XDG_CURRENT_DESKTOP")]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            os,
            kernel: text("/proc/sys/kernel/osrelease").trim().to_owned(),
            session,
            graphics: graphics(),
        }
    }
}

/// The first display adapter's driver and PCI id, from sysfs.
fn graphics() -> String {
    let Ok(cards) = std::fs::read_dir("/sys/class/drm") else {
        return String::new();
    };
    let mut names: Vec<String> = cards
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.strip_prefix("card").is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit())))
        .collect();
    names.sort();
    for card in names {
        let device = format!("/sys/class/drm/{card}/device");
        let read = |file: &str| {
            std::fs::read_to_string(format!("{device}/{file}"))
                .map(|text| text.trim().trim_start_matches("0x").to_owned())
                .unwrap_or_default()
        };
        let driver = std::fs::read_link(format!("{device}/driver"))
            .ok()
            .and_then(|link| link.file_name().map(|name| name.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let (vendor, id) = (read("vendor"), read("device"));
        let pci = if vendor.is_empty() || id.is_empty() { String::new() } else { format!("{vendor}:{id}") };
        match (driver.is_empty(), pci.is_empty()) {
            (true, true) => continue,
            (false, true) => return driver,
            (true, false) => return pci,
            (false, false) => return format!("{driver} ({pci})"),
        }
    }
    String::new()
}

/// Read the facts now and keep them for the process.
pub(crate) fn read_facts() {
    let _ = FACTS.set(Facts::read());
}

/// The facts read at launch; empty when nothing read them (a test).
pub(crate) fn facts() -> Facts {
    FACTS.get().cloned().unwrap_or_default()
}

/// Everything the details block says that is not about the application
/// itself: the system's facts and how this window looks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Environment {
    pub facts: Facts,
    /// The window's display scale: 1.0, 1.25, 2.0.
    pub scale: f64,
    /// "Light" or "Dark".
    pub theme: String,
    /// The accent colour's name.
    pub accent: String,
    /// "Regular" or "Compact".
    pub density: String,
}

/// The block "Copy details" copies: one fact a line, `Name: value`, the
/// application first. A fact that is not known is left out, so the block never
/// says "unknown" about something the reader could not have found out.
///
/// ```text
/// Files 0.1.0
/// App ID: bingux-files
/// Commit: 444fb3b
/// Built: 2026-10-28
/// Licence: GPL-3.0-or-later
/// Rust: rustc 1.90.0 (1159e78c4 2025-09-14)
/// Freya: 0.5.0-rc.10
/// bingux-ui: 0.1.0
/// OS: Fedora Linux 43 (Workstation)
/// Kernel: 7.2.3
/// Session: wayland, sway
/// Display scale: 1.25
/// Graphics: amdgpu (1002:73bf)
/// Theme: Dark, blue accent
/// Density: Regular
/// ```
pub fn details_text(info: &AppInfo, env: &Environment) -> String {
    let mut lines: Vec<String> = Vec::new();
    let title = [info.name.as_str(), info.version.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    lines.push(title);
    let mut fact = |name: &str, value: &str| {
        if !value.is_empty() {
            lines.push(format!("{name}: {value}"));
        }
    };
    fact("App ID", &info.id);
    fact("Commit", short_commit(&info.commit));
    fact("Built", &info.built);
    fact("Licence", &info.licence);
    fact("Rust", RUSTC);
    fact("Freya", FREYA);
    fact("bingux-ui", BINGUX_UI);
    fact("OS", &env.facts.os);
    fact("Kernel", &env.facts.kernel);
    fact("Session", &env.facts.session);
    if env.scale > 0.0 {
        fact("Display scale", &scale_text(env.scale));
    }
    fact("Graphics", &env.facts.graphics);
    let theme = match (env.theme.is_empty(), env.accent.is_empty()) {
        (true, _) => String::new(),
        (false, true) => env.theme.clone(),
        (false, false) => format!("{}, {} accent", env.theme, env.accent),
    };
    fact("Theme", &theme);
    fact("Density", &env.density);
    lines.join("\n")
}

/// 1 as "1", 1.25 as "1.25", 1.5 as "1.5": no trailing zeros.
fn scale_text(scale: f64) -> String {
    let text = format!("{scale:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A licence or dependency list as paragraphs: the text split at blank lines,
/// with a paragraph's hard-wrapped lines joined into one, so it re-wraps to
/// the width it is shown at. A paragraph whose lines are indented (a table, a
/// list) keeps its lines.
pub(crate) fn paragraphs(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    let mut flush = |block: &mut Vec<&str>| {
        if block.is_empty() {
            return;
        }
        let verbatim = block.iter().any(|line| line.starts_with("  ") || line.starts_with('\t'));
        let joined = if verbatim {
            block.join("\n")
        } else {
            block.iter().map(|line| line.trim()).collect::<Vec<_>>().join(" ")
        };
        found.push(joined);
        block.clear();
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            flush(&mut block);
        } else {
            block.push(line.trim_end());
        }
    }
    flush(&mut block);
    found
}

/// The crates of a `scripts/cargo-licenses.py` file: the lines between the
/// summary and the licence texts that read `name version: expression`.
/// Returns the summary line (the licences together) and the crates.
pub(crate) fn dependency_list(text: &str) -> (String, Vec<String>) {
    let summary = text
        .lines()
        .find_map(|line| line.strip_prefix("Summary:"))
        .map(|rest| rest.trim().to_owned())
        .unwrap_or_default();
    let crates = text
        .lines()
        .take_while(|line| !line.starts_with("Licence texts") && !line.starts_with("License texts"))
        .filter(|line| {
            line.split_once(": ").is_some_and(|(head, _)| {
                let mut words = head.split(' ');
                matches!((words.next(), words.next(), words.next()), (Some(name), Some(version), None)
                    if !name.is_empty() && version.starts_with(|c: char| c.is_ascii_digit()))
            })
        })
        .map(str::to_owned)
        .collect();
    (summary, crates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> AppInfo {
        AppInfo::default()
            .name("Files")
            .id("bingux-files")
            .version("0.1.0")
            .licence("GPL-3.0-or-later")
            .commit("444fb3b9f2ac1d")
            .built("2026-10-28")
    }

    fn env() -> Environment {
        Environment {
            facts: Facts {
                os: "Fedora Linux 43".into(),
                kernel: "7.2.3".into(),
                session: "wayland, sway".into(),
                graphics: "amdgpu (1002:73bf)".into(),
            },
            scale: 1.25,
            theme: "Dark".into(),
            accent: "blue".into(),
            density: "Regular".into(),
        }
    }

    #[test]
    fn the_block_names_the_app_the_build_and_the_environment() {
        let text = details_text(&info(), &env());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Files 0.1.0");
        for expected in [
            "App ID: bingux-files",
            "Commit: 444fb3b",
            "Built: 2026-10-28",
            "Licence: GPL-3.0-or-later",
            "OS: Fedora Linux 43",
            "Kernel: 7.2.3",
            "Session: wayland, sway",
            "Display scale: 1.25",
            "Graphics: amdgpu (1002:73bf)",
            "Theme: Dark, blue accent",
            "Density: Regular",
        ] {
            assert!(lines.contains(&expected), "missing {expected:?} in\n{text}");
        }
        assert!(lines.contains(&format!("bingux-ui: {BINGUX_UI}").as_str()));
    }

    #[test]
    fn a_fact_that_is_not_known_is_left_out() {
        let text = details_text(&AppInfo::default().name("Files"), &Environment::default());
        assert!(text.starts_with("Files\n"), "{text}");
        for absent in ["App ID", "Commit", "Built", "Licence", "OS:", "Session", "Display scale", "Graphics", "Theme", "Density"] {
            assert!(!text.contains(absent), "{absent} should be left out of\n{text}");
        }
    }

    #[test]
    fn the_scale_has_no_trailing_zeros() {
        assert_eq!(scale_text(1.0), "1");
        assert_eq!(scale_text(1.25), "1.25");
        assert_eq!(scale_text(1.5), "1.5");
        assert_eq!(scale_text(2.0), "2");
    }

    #[test]
    fn a_theme_without_an_accent_is_just_the_theme() {
        let environment = Environment { accent: String::new(), ..env() };
        assert!(details_text(&info(), &environment).contains("\nTheme: Dark\n"));
    }

    #[test]
    fn hard_wrapped_lines_become_one_paragraph() {
        let text = "First line\nsecond line.\n\n\n  indented\n  table\n\nLast.";
        assert_eq!(paragraphs(text), ["First line second line.", "  indented\n  table", "Last."]);
    }

    #[test]
    fn the_dependency_list_is_the_crates_before_the_licence_texts() {
        let text = "Licences of the Rust crates linked into x\n(generated)\n\nSummary: MIT AND ISC\n\n\
                    ab_glyph 0.2.32: Apache-2.0\naccesskit 0.24.1: Apache-2.0 OR MIT\n\n\
                    Licence texts\n=============\n\nfoo 1.0: not a crate line\n";
        let (summary, crates) = dependency_list(text);
        assert_eq!(summary, "MIT AND ISC");
        assert_eq!(crates, ["ab_glyph 0.2.32: Apache-2.0", "accesskit 0.24.1: Apache-2.0 OR MIT"]);
    }
}
