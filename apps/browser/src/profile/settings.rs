//! A profile's settings: `settings.json`, one small document read whole and
//! replaced whole. A private window reads it with a plain read, which writes
//! nothing (a database open would).
//!
//! Version 1 holds what the settings page has today: the address bar's search
//! engine, by its stable id. Fields this build does not know (a newer
//! browser's) are kept on write; a file from a newer version is never written.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::files::{self, Problem};
use crate::address;

/// The settings format this build reads and writes.
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    /// The id of the address bar's search engine (`address::ENGINES`). An id
    /// this build does not know is kept, and the default engine is used.
    pub search_engine: String,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            search_engine: address::ENGINES[0].id.to_string(),
            rest: Map::new(),
        }
    }
}

/// Why the settings on disk are not the ones in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsProblem {
    /// The file is not settings: what is wrong with it.
    Damaged(String),
    /// A newer browser wrote it.
    TooNew { found: u32, supported: u32 },
    /// It could not be read.
    Io(Problem),
}

impl Settings {
    /// Reads `path`. A missing file is the defaults; a file that cannot be
    /// used gives the defaults and says why, and is left as it is.
    pub fn read(path: &Path) -> (Self, Option<SettingsProblem>) {
        match files::read_if_present(path) {
            Ok(None) => (Self::default(), None),
            Ok(Some(bytes)) => match Self::parse(&bytes) {
                Ok(settings) => (settings, None),
                Err(problem) => (Self::default(), Some(problem)),
            },
            Err(error) => (
                Self::default(),
                Some(SettingsProblem::Io(Problem::of(&error))),
            ),
        }
    }

    fn parse(bytes: &[u8]) -> Result<Self, SettingsProblem> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|error| SettingsProblem::Damaged(error.to_string()))?;
        let found = value.get("version").and_then(Value::as_u64).unwrap_or(0);
        if found > u64::from(VERSION) {
            return Err(SettingsProblem::TooNew {
                found: u32::try_from(found).unwrap_or(u32::MAX),
                supported: VERSION,
            });
        }
        serde_json::from_value(value).map_err(|error| SettingsProblem::Damaged(error.to_string()))
    }

    /// The bytes of `settings.json`.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Where the address bar's search engine is in `address::ENGINES`: the
    /// default for an id this build does not know.
    pub fn engine_index(&self) -> usize {
        address::ENGINES
            .iter()
            .position(|engine| engine.id == self.search_engine)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::files::{remove_tree, replace, tests::temp};
    use super::*;

    #[test]
    fn no_file_is_the_defaults_and_settings_survive_a_write_and_a_read() {
        let dir = temp();
        let path = dir.join("settings.json");
        let (settings, problem) = Settings::read(&path);
        assert_eq!((settings.clone(), problem), (Settings::default(), None));
        assert_eq!(settings.search_engine, "duckduckgo");
        let chosen = Settings {
            search_engine: "wikipedia".to_string(),
            ..settings
        };
        replace(&path, &chosen.encode()).unwrap();
        let (read, problem) = Settings::read(&path);
        assert_eq!(problem, None);
        assert_eq!(read, chosen);
        assert_eq!(address::ENGINES[read.engine_index()].name, "Wikipedia");
        remove_tree(&dir).unwrap();
    }

    #[test]
    fn an_engine_this_build_does_not_know_is_kept_and_the_default_used() {
        let settings =
            Settings::parse(br#"{"version":1,"search_engine":"kagi","sidebar":"left"}"#).unwrap();
        assert_eq!(settings.engine_index(), 0);
        let written: Value = serde_json::from_slice(&settings.encode()).unwrap();
        assert_eq!(written["search_engine"], "kagi");
        assert_eq!(
            written["sidebar"], "left",
            "a newer browser's field survives"
        );
    }

    #[test]
    fn a_damaged_or_newer_file_gives_the_defaults_and_says_why() {
        let dir = temp();
        let path = dir.join("settings.json");
        fs::write(&path, b"{ nope").unwrap();
        let (settings, problem) = Settings::read(&path);
        assert_eq!(settings, Settings::default());
        assert!(matches!(problem, Some(SettingsProblem::Damaged(_))));
        fs::write(&path, br#"{"version":7,"search_engine":"google"}"#).unwrap();
        assert_eq!(
            Settings::read(&path).1,
            Some(SettingsProblem::TooNew {
                found: 7,
                supported: VERSION
            })
        );
        remove_tree(&dir).unwrap();
    }
}
