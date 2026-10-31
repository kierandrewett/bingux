//! Where presets and settings live, and the factory presets.
//!
//! ```text
//! $XDG_CONFIG_HOME/bingux/effects/
//!     state.json                  what is on, which preset, which device
//!     output/presets/Night.json   one file per preset
//!     input/presets/Voice.json
//! ```
//!
//! Files are written whole to a temporary name and renamed, so the engine
//! never reads half a preset.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eq::{self, Band, BandKind};
use crate::preset::{Direction, PluginSlot, Preset, file_stem};

fn home_dir(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var).filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(&std::env::var_os("HOME").unwrap_or_default()).join(fallback),
    }
}

/// `$XDG_CONFIG_HOME/bingux/effects`.
pub fn config_dir() -> PathBuf {
    home_dir("XDG_CONFIG_HOME", ".config").join("bingux/effects")
}

/// `$XDG_CACHE_HOME/bingux/effects`.
pub fn cache_dir() -> PathBuf {
    home_dir("XDG_CACHE_HOME", ".cache").join("bingux/effects")
}

/// `$XDG_DATA_HOME/bingux`.
pub fn data_dir() -> PathBuf {
    home_dir("XDG_DATA_HOME", ".local/share").join("bingux")
}

/// `$XDG_RUNTIME_DIR/bingux/effects`, where the generated configuration, the
/// meter file and process ids live.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("bingux/effects"),
        None => std::env::temp_dir().join(format!("bingux-effects-{}", unsafe { libc::getuid() })),
    }
}

/// What one direction is doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectionState {
    /// Whether the effects run.
    #[serde(default)]
    pub enabled: bool,
    /// The preset in use.
    #[serde(default = "flat")]
    pub preset: String,
    /// The real device: the `node.name` of the speakers or microphone the
    /// effects use. `None` means whatever the default was when they started.
    #[serde(default)]
    pub device: Option<String>,
    /// A preset to use for a device, by `node.name`.
    #[serde(default)]
    pub autoload: BTreeMap<String, String>,
}

fn flat() -> String {
    "Flat".to_string()
}

impl Default for DirectionState {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: flat(),
            device: None,
            autoload: BTreeMap::new(),
        }
    }
}

/// The saved settings of both directions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub output: DirectionState,
    #[serde(default)]
    pub input: DirectionState,
    /// Factory presets already written (`output/Flat`), so one the person
    /// deleted does not come back.
    #[serde(default)]
    pub seeded: Vec<String>,
}

impl State {
    pub fn of(&self, direction: Direction) -> &DirectionState {
        match direction {
            Direction::Output => &self.output,
            Direction::Input => &self.input,
        }
    }

    pub fn of_mut(&mut self, direction: Direction) -> &mut DirectionState {
        match direction {
            Direction::Output => &mut self.output,
            Direction::Input => &mut self.input,
        }
    }
}

/// A failure reading or writing the store.
#[derive(Debug, PartialEq, Eq)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::preset::PresetError> for StoreError {
    fn from(error: crate::preset::PresetError) -> Self {
        Self(error.0)
    }
}

/// Write `text` to `path` through a temporary file in the same directory.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), StoreError> {
    let directory = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

/// The presets and settings in one directory tree.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Default for Store {
    fn default() -> Self {
        Self::at(config_dir())
    }
}

impl Store {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn presets_dir(&self, direction: Direction) -> PathBuf {
        self.root.join(direction.name()).join("presets")
    }

    pub fn preset_path(&self, direction: Direction, name: &str) -> PathBuf {
        self.presets_dir(direction)
            .join(format!("{}.json", file_stem(name)))
    }

    /// The names of the presets of `direction`, sorted.
    pub fn list(&self, direction: Direction) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.presets_dir(direction))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "json" {
                    return None;
                }
                // The name inside the file is what is shown; a file that
                // cannot be read is listed by its file name so it can be deleted.
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some(
                    std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|text| Preset::parse(&text).ok())
                        .map_or(stem, |preset| preset.name),
                )
            })
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    pub fn load(&self, direction: Direction, name: &str) -> Result<Preset, StoreError> {
        let text = std::fs::read_to_string(self.preset_path(direction, name))?;
        let preset = Preset::parse(&text)?;
        if preset.direction != direction {
            return Err(StoreError(format!(
                "\"{name}\" is not an {} preset.",
                direction.name()
            )));
        }
        Ok(preset)
    }

    pub fn save(&self, preset: &Preset) -> Result<(), StoreError> {
        write_atomic(
            &self.preset_path(preset.direction, &preset.name),
            &preset.to_json(),
        )
    }

    pub fn exists(&self, direction: Direction, name: &str) -> bool {
        self.preset_path(direction, name).exists()
    }

    pub fn delete(&self, direction: Direction, name: &str) -> Result<(), StoreError> {
        std::fs::remove_file(self.preset_path(direction, name))?;
        Ok(())
    }

    /// A name like `Night copy` or `Night copy 2` that no preset has.
    pub fn free_name(&self, direction: Direction, wanted: &str) -> String {
        if !self.exists(direction, wanted) {
            return wanted.to_string();
        }
        (2..)
            .map(|n| format!("{wanted} {n}"))
            .find(|name| !self.exists(direction, name))
            .unwrap_or_default()
    }

    /// Copy a preset under a new name and return the name.
    pub fn duplicate(&self, direction: Direction, name: &str) -> Result<String, StoreError> {
        let mut preset = self.load(direction, name)?;
        let copy = self.free_name(direction, &format!("{name} copy"));
        preset.name = copy.clone();
        self.save(&preset)?;
        Ok(copy)
    }

    /// Give a preset a new name. The new name must be free.
    pub fn rename(
        &self,
        direction: Direction,
        name: &str,
        new_name: &str,
    ) -> Result<(), StoreError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(StoreError("The name is empty.".to_string()));
        }
        if new_name != name && self.exists(direction, new_name) {
            return Err(StoreError(format!(
                "A preset called \"{new_name}\" already exists."
            )));
        }
        let mut preset = self.load(direction, name)?;
        preset.name = new_name.to_string();
        self.save(&preset)?;
        if file_stem(new_name) != file_stem(name) {
            self.delete(direction, name)?;
        }
        Ok(())
    }

    /// Copy a preset's file to `destination`.
    pub fn export(
        &self,
        direction: Direction,
        name: &str,
        destination: &Path,
    ) -> Result<(), StoreError> {
        let preset = self.load(direction, name)?;
        write_atomic(destination, &preset.to_json())
    }

    pub fn state(&self) -> State {
        std::fs::read_to_string(self.root.join("state.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_state(&self, state: &State) -> Result<(), StoreError> {
        let mut text = serde_json::to_string_pretty(state).unwrap_or_default();
        text.push('\n');
        write_atomic(&self.root.join("state.json"), &text)
    }

    /// Change the state and save it.
    pub fn update_state(&self, change: impl FnOnce(&mut State)) -> Result<State, StoreError> {
        let mut state = self.state();
        change(&mut state);
        self.save_state(&state)?;
        Ok(state)
    }

    /// Move every version 1 preset to version 2 (see `legacy`). The original
    /// file is kept in `presets/legacy/` with a `.notes.txt` beside it that
    /// says what could not be carried across. Returns the number migrated.
    pub fn migrate_legacy(&self) -> usize {
        let mut migrated = 0;
        for direction in Direction::ALL {
            let directory = self.presets_dir(direction);
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if !crate::legacy::is_legacy(&text) {
                    continue;
                }
                let Ok(result) = crate::legacy::migrate(&text) else {
                    continue;
                };
                let keep = directory.join("legacy");
                let Some(file_name) = path.file_name() else {
                    continue;
                };
                if std::fs::create_dir_all(&keep).is_err()
                    || write_atomic(&keep.join(file_name), &text).is_err()
                    || write_atomic(&path, &result.preset.to_json()).is_err()
                {
                    continue;
                }
                if !result.notes.is_empty() {
                    let _ = write_atomic(
                        &keep.join(format!(
                            "{}.notes.txt",
                            path.file_stem().unwrap_or_default().to_string_lossy()
                        )),
                        &(result.notes.join("\n") + "\n"),
                    );
                }
                migrated += 1;
            }
        }
        migrated
    }

    /// Write the factory presets that were never written. One the person has
    /// since deleted or changed is left alone. Presets in the old format
    /// are migrated first.
    pub fn ensure_factory(&self) -> Result<(), StoreError> {
        self.migrate_legacy();
        let mut state = self.state();
        let mut changed = false;
        for preset in factory_presets() {
            let key = format!("{}/{}", preset.direction.name(), preset.name);
            if state.seeded.contains(&key) {
                continue;
            }
            if !self.exists(preset.direction, &preset.name) {
                self.save(&preset)?;
            }
            state.seeded.push(key);
            changed = true;
        }
        if changed {
            self.save_state(&state)?;
        }
        Ok(())
    }
}

fn band(kind: BandKind, frequency: f64, gain: f64, q: f64) -> Band {
    Band::new(kind, frequency, gain, q)
}

fn db(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn equalizer(id: &str, input_gain_db: f64, bands: &[Band]) -> PluginSlot {
    let mut slot = PluginSlot::new(id, "equalizer");
    slot.controls = eq::controls(bands);
    if input_gain_db != 0.0 {
        slot.controls.insert("g_in".to_string(), db(input_gain_db));
    }
    slot
}

/// The presets that ship with Bingux.
pub fn factory_presets() -> Vec<Preset> {
    let output = |name: &str| Preset::new(name, Direction::Output);
    let input = |name: &str| Preset::new(name, Direction::Input);
    vec![
        output("Flat"),
        output("Voice clarity")
            .with(equalizer(
                "equalizer-1",
                -2.0,
                &[
                    band(BandKind::HighPass, 80.0, 0.0, 0.707),
                    band(BandKind::Bell, 250.0, -2.0, 1.2),
                    band(BandKind::Bell, 3000.0, 3.0, 1.0),
                    band(BandKind::HighShelf, 8000.0, 1.5, 0.707),
                ],
            ))
            .with(
                PluginSlot::new("compressor-1", "compressor")
                    .control("al", db(-24.0))
                    .control("cr", 2.5)
                    .control("mk", db(3.0)),
            ),
        output("Night")
            .with(
                PluginSlot::new("compressor-1", "compressor")
                    .control("al", db(-32.0))
                    .control("cr", 6.0)
                    .control("at", 5.0)
                    .control("rt", 250.0)
                    .control("kn", db(-8.0))
                    .control("mk", db(8.0)),
            )
            .with(
                PluginSlot::new("limiter-1", "limiter")
                    .control("th", db(-3.0))
                    .control("rt", 12.0),
            ),
        output("Bass boost")
            .with(equalizer(
                "equalizer-1",
                -5.0,
                &[
                    band(BandKind::LowShelf, 100.0, 6.0, 0.707),
                    band(BandKind::Bell, 60.0, 2.0, 1.0),
                ],
            ))
            .with(PluginSlot::new("limiter-1", "limiter").control("th", db(-1.0))),
        output("Loudness")
            .with(PluginSlot::new("loudness-1", "loudness"))
            .with(PluginSlot::new("limiter-1", "limiter").control("th", db(-1.0))),
        input("Flat"),
        input("Voice")
            .with(equalizer(
                "equalizer-1",
                0.0,
                &[
                    band(BandKind::HighPass, 90.0, 0.0, 0.707),
                    band(BandKind::Bell, 200.0, -2.0, 1.0),
                    band(BandKind::Bell, 4000.0, 2.0, 1.0),
                ],
            ))
            .with(
                PluginSlot::new("gate-1", "gate")
                    .control("gt", db(-50.0))
                    .control("gr", db(-40.0)),
            )
            .with(
                PluginSlot::new("compressor-1", "compressor")
                    .control("al", db(-22.0))
                    .control("cr", 3.0)
                    .control("mk", db(4.0)),
            )
            .with(
                PluginSlot::new("deesser-1", "deesser")
                    .control("threshold", db(-30.0))
                    .control("f1_freq", 6500.0),
            ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> Store {
        let root =
            std::env::temp_dir().join(format!("bingux-effects-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Store::at(root)
    }

    #[test]
    fn factory_presets_are_written_once() {
        let store = temp("factory");
        store.ensure_factory().unwrap();
        let names = store.list(Direction::Output);
        for wanted in ["Flat", "Voice clarity", "Night", "Bass boost", "Loudness"] {
            assert!(
                names.iter().any(|name| name == wanted),
                "{wanted} missing in {names:?}"
            );
        }
        store.delete(Direction::Output, "Night").unwrap();
        store.ensure_factory().unwrap();
        assert!(!store.exists(Direction::Output, "Night"));
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn presets_can_be_duplicated_renamed_and_deleted() {
        let store = temp("crud");
        store.ensure_factory().unwrap();
        let copy = store.duplicate(Direction::Output, "Night").unwrap();
        assert_eq!(copy, "Night copy");
        assert_eq!(
            store.duplicate(Direction::Output, "Night").unwrap(),
            "Night copy 2"
        );
        store
            .rename(Direction::Output, "Night copy", "Quiet")
            .unwrap();
        assert!(store.exists(Direction::Output, "Quiet"));
        assert!(!store.exists(Direction::Output, "Night copy"));
        assert!(store.rename(Direction::Output, "Quiet", "Night").is_err());
        assert_eq!(
            store.load(Direction::Output, "Quiet").unwrap().name,
            "Quiet"
        );
        store.delete(Direction::Output, "Quiet").unwrap();
        assert!(store.load(Direction::Output, "Quiet").is_err());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn state_round_trips_and_defaults_when_missing() {
        let store = temp("state");
        assert!(!store.state().output.enabled);
        store
            .update_state(|state| {
                state.output.enabled = true;
                state
                    .output
                    .autoload
                    .insert("alsa_output.usb".into(), "Night".into());
            })
            .unwrap();
        let state = store.state();
        assert!(state.output.enabled);
        assert_eq!(state.output.autoload["alsa_output.usb"], "Night");
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn exports_are_plain_preset_files() {
        let store = temp("export");
        store.ensure_factory().unwrap();
        let destination = store.root().join("out.json");
        store
            .export(Direction::Output, "Night", &destination)
            .unwrap();
        let text = std::fs::read_to_string(&destination).unwrap();
        assert_eq!(Preset::parse(&text).unwrap().name, "Night");
        let _ = std::fs::remove_dir_all(store.root());
    }
}
