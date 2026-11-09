//! Media's settings, kept in `$XDG_CONFIG_HOME/bingux-media/settings.json`.
//!
//! Nothing needs setting: a missing or unreadable file is the defaults, and a
//! file from a newer or older version keeps every key it understands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::player::Levelling;

/// What the person chose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folders added to the library, beyond Music and Videos.
    pub folders: Vec<PathBuf>,
    /// Music or Videos taken out of the library.
    pub removed_defaults: Vec<PathBuf>,
    /// `off`, `track`, `album` or `automatic`.
    pub levelling: String,
    /// Films start where they were left.
    pub resume: bool,
    /// Plays go to Last.fm (once an account is set up).
    pub scrobble: bool,
    /// 0 to 100.
    pub volume: f64,
    pub muted: bool,
    /// Subtitles in the person's language are shown when a film has them.
    pub subtitles: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            folders: Vec::new(),
            removed_defaults: Vec::new(),
            levelling: "track".into(),
            resume: true,
            scrobble: false,
            volume: 100.0,
            muted: false,
            subtitles: true,
        }
    }
}

impl Settings {
    pub fn levelling(&self) -> Levelling {
        match self.levelling.as_str() {
            "off" => Levelling::Off,
            "album" => Levelling::Album,
            "automatic" => Levelling::Automatic,
            _ => Levelling::Track,
        }
    }

    pub fn set_levelling(&mut self, levelling: Levelling) {
        self.levelling = match levelling {
            Levelling::Off => "off",
            Levelling::Track => "track",
            Levelling::Album => "album",
            Levelling::Automatic => "automatic",
        }
        .into();
    }

    /// The library's folders: Music and Videos (unless removed), then the added ones.
    pub fn library_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = crate::library::default_folders()
            .into_iter()
            .filter(|folder| !self.removed_defaults.contains(folder))
            .collect();
        for folder in &self.folders {
            if !folders.contains(folder) {
                folders.push(folder.clone());
            }
        }
        folders
    }
}

/// `$XDG_CONFIG_HOME/bingux-media`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::library::home().join(".config"))
        .join("bingux-media")
}

fn file() -> PathBuf {
    config_dir().join("settings.json")
}

/// The saved settings, or the defaults.
pub fn load() -> Settings {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Saves the settings (written to a new file, then moved over the old one).
pub fn save(settings: &Settings) -> std::io::Result<()> {
    let file = file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = file.with_extension("json.part");
    std::fs::write(
        &partial,
        serde_json::to_string_pretty(settings).unwrap_or_default(),
    )?;
    std::fs::rename(&partial, &file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_and_missing_keys_are_fine() {
        let settings: Settings = serde_json::from_str(r#"{"volume": 40, "future": true}"#).unwrap();
        assert_eq!(settings.volume, 40.0);
        assert!(settings.resume);
        assert_eq!(settings.levelling(), Levelling::Track);
    }
}
