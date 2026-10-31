//! The preset file format.
//!
//! A preset is one JSON file, `<name>.json`, in
//! `$XDG_CONFIG_HOME/bingux/effects/<output|input>/presets/`. It holds the
//! ordered stack of effects for one direction. `docs/audio-effects.md`
//! describes every field; the types here are the format.
//!
//! Version 2 names an effect by its kind (`compressor`, see `effects`) and
//! stores the plugin's controls under the plugin's own port symbols, so a
//! preset says exactly what the plugin was set to and nothing about how it is
//! hosted. Version 1 presets (Bingux's own plugins) are migrated by `legacy`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The `format` value every preset carries.
pub const FORMAT: &str = "bingux-effects-preset";
/// The version this code writes. A newer version is refused, not guessed at.
pub const VERSION: u32 = 2;

/// Which way the sound flows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// From applications to the speakers.
    Output,
    /// From a microphone to applications.
    Input,
}

impl Direction {
    pub const ALL: [Direction; 2] = [Direction::Output, Direction::Input];

    /// `output` or `input`, as in file names and commands.
    pub fn name(self) -> &'static str {
        match self {
            Direction::Output => "output",
            Direction::Input => "input",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|direction| direction.name() == text)
    }
}

fn yes() -> bool {
    true
}

/// One effect in the stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginSlot {
    /// Names the effect in this preset, such as `compressor-1`. Letters, digits and `-`.
    pub id: String,
    /// The kind of effect: `compressor`, `equalizer` and the rest of `effects::ALL`.
    #[serde(rename = "type")]
    pub kind: String,
    /// An effect that is off is left out of the audio path and keeps its settings.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Control values by the plugin's port symbol. A control that is missing
    /// has the plugin's default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub controls: BTreeMap<String, f64>,
    /// Convolver only: the impulse response file (WAV, FLAC and the like).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl PluginSlot {
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            enabled: true,
            controls: BTreeMap::new(),
            path: None,
        }
    }

    pub fn control(mut self, symbol: &str, value: f64) -> Self {
        self.controls.insert(symbol.to_string(), value);
        self
    }

    /// The value of `symbol`: the stored one, else `default`.
    pub fn value(&self, symbol: &str, default: f64) -> f64 {
        self.controls.get(symbol).copied().unwrap_or(default)
    }
}

/// A named stack of effects for one direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default = "format_name")]
    pub format: String,
    #[serde(default = "version")]
    pub version: u32,
    pub name: String,
    pub direction: Direction,
    /// The effects, in the order the sound passes through them.
    #[serde(default)]
    pub plugins: Vec<PluginSlot>,
}

fn format_name() -> String {
    FORMAT.to_string()
}

fn version() -> u32 {
    VERSION
}

/// Why a preset file was refused.
#[derive(Debug, PartialEq, Eq)]
pub struct PresetError(pub String);

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PresetError {}

impl Preset {
    pub fn new(name: impl Into<String>, direction: Direction) -> Self {
        Self {
            format: FORMAT.to_string(),
            version: VERSION,
            name: name.into(),
            direction,
            plugins: Vec::new(),
        }
    }

    pub fn with(mut self, slot: PluginSlot) -> Self {
        self.plugins.push(slot);
        self
    }

    /// Read a version 2 preset from JSON text and check it. A version 1 file
    /// is refused here: `legacy::migrate` reads those.
    pub fn parse(text: &str) -> Result<Self, PresetError> {
        let preset: Preset =
            serde_json::from_str(text).map_err(|e| PresetError(format!("Not a preset: {e}")))?;
        preset.check()?;
        Ok(preset)
    }

    /// The preset as pretty JSON, one effect per block.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    fn check(&self) -> Result<(), PresetError> {
        if self.format != FORMAT {
            return Err(PresetError(format!(
                "Not a Bingux preset (format \"{}\").",
                self.format
            )));
        }
        if self.version > VERSION {
            return Err(PresetError(format!(
                "Preset version {} is newer than this app understands ({VERSION}).",
                self.version
            )));
        }
        if self.version < VERSION {
            return Err(PresetError(format!(
                "Preset version {} is out of date and has not been migrated.",
                self.version
            )));
        }
        if self.name.trim().is_empty() {
            return Err(PresetError("The preset has no name.".to_string()));
        }
        let mut seen = Vec::new();
        for slot in &self.plugins {
            if slot.id.is_empty()
                || !slot
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(PresetError(format!(
                    "Effect id \"{}\" must be letters, digits and dashes.",
                    slot.id
                )));
            }
            if seen.contains(&slot.id) {
                return Err(PresetError(format!(
                    "Effect id \"{}\" is used twice.",
                    slot.id
                )));
            }
            seen.push(slot.id.clone());
        }
        Ok(())
    }

    /// An id like `compressor-3` that no effect in the preset uses.
    pub fn fresh_id(&self, stem: &str) -> String {
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let stem = stem.trim_matches('-');
        (1..)
            .map(|n| format!("{stem}-{n}"))
            .find(|id| self.plugins.iter().all(|slot| &slot.id != id))
            .unwrap_or_default()
    }

    pub fn slot(&self, id: &str) -> Option<&PluginSlot> {
        self.plugins.iter().find(|slot| slot.id == id)
    }

    pub fn slot_mut(&mut self, id: &str) -> Option<&mut PluginSlot> {
        self.plugins.iter_mut().find(|slot| slot.id == id)
    }

    /// Move the effect at `from` so it sits at `to`, both indexes into the
    /// stack before the move. Out of range or equal indexes change nothing.
    pub fn move_slot(&mut self, from: usize, to: usize) {
        if from == to || from >= self.plugins.len() || to >= self.plugins.len() {
            return;
        }
        let slot = self.plugins.remove(from);
        self.plugins.insert(to, slot);
    }

    /// Add a new effect of `kind` at the end with its initial settings, and
    /// return its id.
    pub fn add(&mut self, kind: &str) -> Option<String> {
        let effect = crate::effects::find(kind)?;
        let id = self.fresh_id(kind.replace('_', "-").as_str());
        let mut slot = PluginSlot::new(id.clone(), kind);
        for (symbol, value) in effect.initial_controls() {
            slot.controls.insert(symbol, value);
        }
        self.plugins.push(slot);
        Some(id)
    }

    /// Take the effect `id` out of the stack.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|slot| slot.id != id);
        self.plugins.len() != before
    }
}

/// A file name for a preset name: the name with anything unsafe in a path
/// replaced, never empty and never starting with a dot.
pub fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "Preset".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_survives_a_round_trip() {
        let preset = Preset::new("Night", Direction::Output)
            .with(
                PluginSlot::new("equalizer-1", "equalizer")
                    .control("ftl_0", 1.0)
                    .control("fl_0", 100.0),
            )
            .with(PluginSlot::new("compressor-1", "compressor").control("cr", 6.0));
        assert_eq!(Preset::parse(&preset.to_json()).unwrap(), preset);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let preset = Preset::parse(
            r#"{ "name": "Mine", "direction": "input",
                 "plugins": [ { "id": "g", "type": "limiter" } ] }"#,
        )
        .unwrap();
        assert!(preset.plugins[0].enabled);
        assert_eq!(preset.version, VERSION);
    }

    #[test]
    fn bad_presets_are_refused_with_a_reason() {
        assert!(Preset::parse("[]").is_err());
        assert!(Preset::parse(r#"{"format":"other","name":"x","direction":"output"}"#).is_err());
        assert!(Preset::parse(r#"{"version":99,"name":"x","direction":"output"}"#).is_err());
        assert!(
            Preset::parse(r#"{"version":1,"name":"x","direction":"output"}"#)
                .unwrap_err()
                .0
                .contains("migrated")
        );
        let twice = r#"{"name":"x","direction":"output","plugins":[{"id":"a","type":"gate"},{"id":"a","type":"gate"}]}"#;
        assert!(Preset::parse(twice).unwrap_err().0.contains("twice"));
    }

    #[test]
    fn fresh_ids_do_not_collide() {
        let preset = Preset::new("x", Direction::Output).with(PluginSlot::new("gate-1", "gate"));
        assert_eq!(preset.fresh_id("gate"), "gate-2");
        assert_eq!(preset.fresh_id("Bass enhancer"), "bass-enhancer-1");
    }

    #[test]
    fn effects_are_added_moved_and_removed_in_order() {
        let mut preset = Preset::new("x", Direction::Output);
        let eq = preset.add("equalizer").unwrap();
        let comp = preset.add("compressor").unwrap();
        let lim = preset.add("limiter").unwrap();
        assert_eq!(
            preset
                .plugins
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            [eq.as_str(), comp.as_str(), lim.as_str()]
        );
        preset.move_slot(2, 0);
        assert_eq!(preset.plugins[0].id, lim);
        preset.move_slot(0, 9);
        assert_eq!(preset.plugins[0].id, lim, "out of range moves nothing");
        assert!(preset.remove(&comp));
        assert!(!preset.remove(&comp));
        assert_eq!(preset.plugins.len(), 2);
        assert!(preset.add("not-an-effect").is_none());
        assert!(
            preset.slot(&eq).unwrap().controls.contains_key("ftl_0"),
            "an equalizer starts with its bands"
        );
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("a/b"), "a_b");
        assert_eq!(file_stem("..hidden"), "hidden");
        assert_eq!(file_stem("  "), "Preset");
    }
}
