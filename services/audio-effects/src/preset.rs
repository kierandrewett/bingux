//! The preset file format.
//!
//! A preset is one JSON file, `<name>.json`, in
//! `$XDG_CONFIG_HOME/bingux/effects/<output|input>/presets/`. It holds the
//! ordered chain of plugins for one direction. `docs/audio-effects.md`
//! describes every field; the types here are the format.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The `format` value every preset carries.
pub const FORMAT: &str = "bingux-effects-preset";
/// The version this code writes. A newer version is refused, not guessed at.
pub const VERSION: u32 = 1;

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
        Self::ALL.into_iter().find(|direction| direction.name() == text)
    }
}

/// What a filter band does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandKind {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
}

impl BandKind {
    pub const ALL: [BandKind; 8] = [
        BandKind::Peaking,
        BandKind::LowShelf,
        BandKind::HighShelf,
        BandKind::LowPass,
        BandKind::HighPass,
        BandKind::BandPass,
        BandKind::Notch,
        BandKind::AllPass,
    ];

    pub fn title(self) -> &'static str {
        match self {
            BandKind::Peaking => "Bell",
            BandKind::LowShelf => "Low shelf",
            BandKind::HighShelf => "High shelf",
            BandKind::LowPass => "Low pass",
            BandKind::HighPass => "High pass",
            BandKind::BandPass => "Band pass",
            BandKind::Notch => "Notch",
            BandKind::AllPass => "All pass",
        }
    }

    /// The value in preset files.
    pub fn key(self) -> &'static str {
        match self {
            BandKind::Peaking => "peaking",
            BandKind::LowShelf => "low_shelf",
            BandKind::HighShelf => "high_shelf",
            BandKind::LowPass => "low_pass",
            BandKind::HighPass => "high_pass",
            BandKind::BandPass => "band_pass",
            BandKind::Notch => "notch",
            BandKind::AllPass => "all_pass",
        }
    }

    /// The label of the PipeWire built-in biquad that implements it.
    pub fn label(self) -> &'static str {
        match self {
            BandKind::Peaking => "bq_peaking",
            BandKind::LowShelf => "bq_lowshelf",
            BandKind::HighShelf => "bq_highshelf",
            BandKind::LowPass => "bq_lowpass",
            BandKind::HighPass => "bq_highpass",
            BandKind::BandPass => "bq_bandpass",
            BandKind::Notch => "bq_notch",
            BandKind::AllPass => "bq_allpass",
        }
    }

    /// Whether the band uses its gain.
    pub fn has_gain(self) -> bool {
        matches!(self, BandKind::Peaking | BandKind::LowShelf | BandKind::HighShelf)
    }
}

/// One filter of an equalizer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    #[serde(rename = "type")]
    pub kind: BandKind,
    /// Hz.
    pub frequency: f64,
    /// dB. Ignored by filters without gain.
    #[serde(default)]
    pub gain: f64,
    #[serde(default = "default_q")]
    pub q: f64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Band {
    pub fn peaking(frequency: f64, gain: f64, q: f64) -> Self {
        Self {
            kind: BandKind::Peaking,
            frequency,
            gain,
            q,
            enabled: true,
        }
    }
}

fn default_q() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

/// The plugin type of the parametric equalizer.
pub const EQUALIZER: &str = "equalizer";
/// The plugin type of the plain gain stage.
pub const GAIN: &str = "gain";
/// The plugin type of the delay.
pub const DELAY: &str = "delay";
/// The plugin type of the convolver (impulse response reverb and speaker correction).
pub const CONVOLVER: &str = "convolver";

/// One plugin in the chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginSlot {
    /// Names the plugin in this preset, such as `compressor-1`. Letters, digits and `-`.
    pub id: String,
    /// `equalizer`, `gain`, `delay`, `convolver` or `ladspa:<library>:<label>`.
    #[serde(rename = "type")]
    pub kind: String,
    /// A plugin that is off is left out of the audio path.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Control values by port name. A control that is missing has its default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub controls: BTreeMap<String, f64>,
    /// Equalizer only: gain before the bands, in dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preamp: Option<f64>,
    /// Equalizer only: the filters, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bands: Vec<Band>,
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
            preamp: None,
            bands: Vec::new(),
            path: None,
        }
    }

    pub fn control(mut self, name: &str, value: f64) -> Self {
        self.controls.insert(name.to_string(), value);
        self
    }

    pub fn equalizer(id: impl Into<String>, preamp: f64, bands: Vec<Band>) -> Self {
        let mut slot = Self::new(id, EQUALIZER);
        slot.preamp = Some(preamp);
        slot.bands = bands;
        slot
    }

    /// `ladspa:<library>:<label>` for a LADSPA plugin.
    pub fn ladspa(id: impl Into<String>, library: &str, label: &str) -> Self {
        Self::new(id, format!("ladspa:{library}:{label}"))
    }
}

/// A named chain of plugins for one direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default = "format_name")]
    pub format: String,
    #[serde(default = "version")]
    pub version: u32,
    pub name: String,
    pub direction: Direction,
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

    /// Read a preset from JSON text and check it.
    pub fn parse(text: &str) -> Result<Self, PresetError> {
        let preset: Preset =
            serde_json::from_str(text).map_err(|e| PresetError(format!("Not a preset: {e}")))?;
        preset.check()?;
        Ok(preset)
    }

    /// The preset as pretty JSON, one plugin per block.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    fn check(&self) -> Result<(), PresetError> {
        if self.format != FORMAT {
            return Err(PresetError(format!("Not a Bingux preset (format \"{}\").", self.format)));
        }
        if self.version > VERSION {
            return Err(PresetError(format!(
                "Preset version {} is newer than this app understands ({VERSION}).",
                self.version
            )));
        }
        if self.name.trim().is_empty() {
            return Err(PresetError("The preset has no name.".to_string()));
        }
        let mut seen = Vec::new();
        for slot in &self.plugins {
            if slot.id.is_empty() || !slot.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(PresetError(format!(
                    "Plugin id \"{}\" must be letters, digits and dashes.",
                    slot.id
                )));
            }
            if seen.contains(&slot.id) {
                return Err(PresetError(format!("Plugin id \"{}\" is used twice.", slot.id)));
            }
            seen.push(slot.id.clone());
        }
        Ok(())
    }

    /// An id like `compressor-3` that no plugin in the preset uses.
    pub fn fresh_id(&self, stem: &str) -> String {
        let stem: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
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
}

/// A file name for a preset name: the name with anything unsafe in a path
/// replaced, never empty and never starting with a dot.
pub fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim_start_matches('.').trim().to_string();
    if cleaned.is_empty() { "Preset".to_string() } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_survives_a_round_trip() {
        let preset = Preset::new("Night", Direction::Output)
            .with(PluginSlot::equalizer("eq-1", -2.0, vec![Band::peaking(100.0, 3.0, 1.0)]))
            .with(PluginSlot::ladspa("comp-1", "bingux_dsp", "bingux_compressor").control("Threshold", -24.0));
        assert_eq!(Preset::parse(&preset.to_json()).unwrap(), preset);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let preset = Preset::parse(
            r#"{ "name": "Mine", "direction": "input",
                 "plugins": [ { "id": "g", "type": "gain" } ] }"#,
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
        let twice = r#"{"name":"x","direction":"output","plugins":[{"id":"a","type":"gain"},{"id":"a","type":"gain"}]}"#;
        assert!(Preset::parse(twice).unwrap_err().0.contains("twice"));
    }

    #[test]
    fn fresh_ids_do_not_collide() {
        let preset = Preset::new("x", Direction::Output).with(PluginSlot::new("gain-1", GAIN));
        assert_eq!(preset.fresh_id("Gain"), "gain-2");
        assert_eq!(preset.fresh_id("Bingux Compressor"), "bingux-compressor-1");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("a/b"), "a_b");
        assert_eq!(file_stem("..hidden"), "hidden");
        assert_eq!(file_stem("  "), "Preset");
    }
}
