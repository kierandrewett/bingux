//! The few facts the bridge reads from the user's Bingux configuration.

use std::path::{Path, PathBuf};

/// Where the user's configuration lives: `$XDG_CONFIG_HOME/bingux/bingux.toml`
/// or `$HOME/.config/bingux/bingux.toml`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("bingux/bingux.toml"))
}

/// Is the dock enabled (`[desktop] dock`)? Absent or unreadable means enabled,
/// which is Bingux's default and the stricter reading (the dock is required).
pub fn dock_enabled(toml: &str) -> bool {
    let mut in_desktop = false;
    for raw in toml.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(table) = line.strip_prefix('[') {
            in_desktop = table.trim_end_matches(']').trim() == "desktop";
            continue;
        }
        if in_desktop {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "dock" {
                    return value.trim() != "false";
                }
            }
        }
    }
    true
}

pub fn read_dock_enabled() -> bool {
    config_path().and_then(|p| std::fs::read_to_string(p).ok()).map_or(true, |t| dock_enabled(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_flag_is_read_from_the_desktop_table_only() {
        assert!(dock_enabled(""));
        assert!(dock_enabled("[desktop]\ndock = true\n"));
        assert!(!dock_enabled("[desktop]\nsidebar = true\ndock = false # off\n"));
        assert!(dock_enabled("[other]\ndock = false\n[desktop]\nsidebar = true\n"));
        assert!(dock_enabled("[desktop.layout]\ndock = []\n"), "other tables named desktop.* do not count");
        assert!(!dock_enabled("[desktop]\n dock=false\n"));
    }
}
