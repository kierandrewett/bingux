//! What Weather keeps on disk: the settings and saved places, and the last
//! forecast of each place so the app opens on weather it already has and
//! shows it, with its age, when there is no network.
//!
//! Settings are `$XDG_CONFIG_HOME/bingux/weather/settings.json`; forecasts are
//! `$XDG_CACHE_HOME/bingux/weather/forecast-<place>.json`. `BINGUX_WEATHER_DIR`
//! puts both under one folder (`config/` and `cache/`), for a test or a
//! screenshot that must not touch the person's own.
//!
//! Every call touches the disk: run it on a worker.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{Forecast, Place};
use crate::units::Units;

/// How long a forecast counts as current. Open-Meteo refreshes its current
/// conditions every 15 minutes, so a younger one is not fetched again.
pub const FRESH_FOR: i64 = 10 * 60;
/// The `selected` value for the place the computer is in.
pub const HERE: &str = "here";

/// Whether `forecast`, fetched at `forecast.fetched_at`, is still current at `now`.
pub fn is_fresh(forecast: &Forecast, now: i64) -> bool {
    now - forecast.fetched_at < FRESH_FOR
}

/// Where the computer is, as last found. Kept in `here.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Here {
    pub place: Place,
    /// How it was found: `portal`, `network` or `fixed`.
    pub source: String,
    /// When, in Unix seconds.
    pub at: i64,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The units the person chose; none follows the locale.
    #[serde(default)]
    pub units: Option<Units>,
    /// Whether "My Location" is a place in the list.
    #[serde(default = "yes")]
    pub use_location: bool,
    /// The saved places, in the order the person put them.
    #[serde(default)]
    pub saved: Vec<Place>,
    /// The place on show: a place's key, or [`HERE`].
    #[serde(default)]
    pub selected: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            units: None,
            use_location: true,
            saved: Vec::new(),
            selected: None,
        }
    }
}

impl Settings {
    /// The units to show: the person's choice, or the locale's.
    pub fn units(&self) -> Units {
        self.units.unwrap_or_else(Units::auto)
    }
}

/// The folders Weather writes to.
#[derive(Clone, Debug)]
pub struct Store {
    config: PathBuf,
    cache: PathBuf,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn xdg(name: &str, fallback: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

impl Store {
    pub fn open() -> Store {
        if let Some(dir) = std::env::var_os("BINGUX_WEATHER_DIR").map(PathBuf::from) {
            return Store {
                config: dir.join("config"),
                cache: dir.join("cache"),
            };
        }
        Store {
            config: xdg("XDG_CONFIG_HOME", ".config").join("bingux/weather"),
            cache: xdg("XDG_CACHE_HOME", ".cache").join("bingux/weather"),
        }
    }

    /// A store in `dir`, for tests.
    pub fn in_dir(dir: &Path) -> Store {
        Store {
            config: dir.join("config"),
            cache: dir.join("cache"),
        }
    }

    fn settings_path(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    fn here_path(&self) -> PathBuf {
        self.config.join("here.json")
    }

    /// Where the computer was last found, kept apart from the settings so a
    /// lookup finishing never overwrites a change the person just made.
    pub fn here(&self) -> Option<Here> {
        let text = fs::read_to_string(self.here_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save_here(&self, here: &Here) -> io::Result<()> {
        write_atomic(
            &self.here_path(),
            &serde_json::to_vec(here).map_err(io::Error::other)?,
        )
    }

    fn forecast_path(&self, key: &str) -> PathBuf {
        let safe: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.cache.join(format!("forecast-{safe}.json"))
    }

    /// The settings, or the defaults when there are none or they cannot be read.
    pub fn settings(&self) -> Settings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        write_atomic(
            &self.settings_path(),
            &serde_json::to_vec_pretty(settings).map_err(io::Error::other)?,
        )
    }

    /// The last forecast saved for the place with `key`.
    pub fn forecast(&self, key: &str) -> Option<Forecast> {
        let text = fs::read_to_string(self.forecast_path(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save_forecast(&self, key: &str, forecast: &Forecast) -> io::Result<()> {
        write_atomic(
            &self.forecast_path(key),
            &serde_json::to_vec(forecast).map_err(io::Error::other)?,
        )
    }

    /// Deletes the saved forecasts of places that are not in `keep`.
    pub fn prune(&self, keep: &[String]) {
        let wanted: Vec<PathBuf> = keep.iter().map(|key| self.forecast_path(key)).collect();
        let Ok(entries) = fs::read_dir(&self.cache) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("forecast-") && name.ends_with(".json") && !wanted.contains(&path) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

/// Writes to a sibling file and renames it into place, so a crash never
/// leaves half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-weather-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_round_trip_and_default() {
        let dir = scratch("settings");
        let store = Store::in_dir(&dir);
        assert_eq!(store.settings(), Settings::default());
        assert!(store.settings().use_location);
        let mut settings = Settings::default();
        settings.saved.push(Place::new("Leeds", 53.8, -1.55));
        settings.selected = Some("53.80,-1.55".into());
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings(), settings);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn forecasts_are_kept_per_place_and_pruned() {
        let dir = scratch("forecast");
        let store = Store::in_dir(&dir);
        let forecast = Forecast {
            fetched_at: 1_000,
            ..Forecast::default()
        };
        store.save_forecast("1.00,2.00", &forecast).unwrap();
        store.save_forecast("3.00,-4.00", &forecast).unwrap();
        assert_eq!(store.forecast("1.00,2.00"), Some(forecast));
        assert!(store.forecast("9.00,9.00").is_none());
        store.prune(&["1.00,2.00".to_string()]);
        assert!(store.forecast("1.00,2.00").is_some());
        assert!(store.forecast("3.00,-4.00").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn freshness() {
        let forecast = Forecast {
            fetched_at: 1_000,
            ..Forecast::default()
        };
        assert!(is_fresh(&forecast, 1_000 + FRESH_FOR - 1));
        assert!(!is_fresh(&forecast, 1_000 + FRESH_FOR));
    }
}
