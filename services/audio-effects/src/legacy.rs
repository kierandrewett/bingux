//! Version 1 presets, which used Bingux's own `bingux_dsp` plugins, become
//! version 2 presets of the published plugins.
//!
//! The settings that have a counterpart move across with their units
//! converted (the old plugins took dB, the Linux Studio Plugins take a linear
//! gain). An effect with a counterpart whose settings do not map keeps its
//! place in the stack at the plugin's defaults, and an effect with no
//! counterpart is dropped; both are listed in the notes. The original file is
//! kept by the store, so nothing is lost.

use serde_json::Value;

use crate::eq::{self, Band, BandKind};
use crate::preset::{Direction, PluginSlot, Preset, PresetError, VERSION};

/// A migrated preset and what could not be carried across.
#[derive(Clone, Debug, PartialEq)]
pub struct Migrated {
    pub preset: Preset,
    /// One line for each effect that was dropped or reset, in plain language.
    pub notes: Vec<String>,
}

fn db(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn number(object: &Value, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)
}

fn band_kind(key: &str) -> BandKind {
    match key {
        "low_shelf" => BandKind::LowShelf,
        "high_shelf" => BandKind::HighShelf,
        "low_pass" => BandKind::LowPass,
        "high_pass" => BandKind::HighPass,
        "band_pass" => BandKind::BandPass,
        "notch" => BandKind::Notch,
        "all_pass" => BandKind::AllPass,
        _ => BandKind::Bell,
    }
}

/// Whether `text` is a version 1 preset.
pub fn is_legacy(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .is_some_and(|value| {
            value.get("version").and_then(Value::as_u64).unwrap_or(1) < u64::from(VERSION)
        })
}

/// Read a version 1 preset as a version 2 one.
pub fn migrate(text: &str) -> Result<Migrated, PresetError> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| PresetError(format!("Not a preset: {e}")))?;
    if value.get("format").and_then(Value::as_str) != Some(crate::preset::FORMAT) {
        return Err(PresetError("Not a Bingux preset.".to_string()));
    }
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let direction = value
        .get("direction")
        .and_then(Value::as_str)
        .and_then(Direction::parse)
        .ok_or_else(|| PresetError("The preset has no direction.".to_string()))?;
    let mut preset = Preset::new(name, direction);
    let mut notes = Vec::new();
    for old in value
        .get("plugins")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = old
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("effect")
            .to_string();
        let kind = old.get("type").and_then(Value::as_str).unwrap_or("");
        let controls = old.get("controls").cloned().unwrap_or(Value::Null);
        let c = |key: &str, default: f64| number(&controls, key).unwrap_or(default);
        let mut slot = match kind {
            "equalizer" => {
                let bands: Vec<Band> = old
                    .get("bands")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|band| band.get("enabled").and_then(Value::as_bool).unwrap_or(true))
                    .take(eq::BANDS)
                    .map(|band| {
                        Band::new(
                            band_kind(
                                band.get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("peaking"),
                            ),
                            number(band, "frequency").unwrap_or(1000.0),
                            number(band, "gain").unwrap_or(0.0),
                            number(band, "q").unwrap_or(1.0),
                        )
                    })
                    .collect();
                let mut slot = PluginSlot::new(&id, "equalizer");
                slot.controls = eq::controls(&bands);
                if let Some(preamp) = number(old, "preamp").filter(|preamp| *preamp != 0.0) {
                    slot.controls
                        .insert("g_in".to_string(), db(preamp).clamp(0.0, 10.0));
                }
                slot
            }
            "gain" => {
                notes.push("The gain stage became an equalizer with no filters; its output gain holds the level.".to_string());
                let mut slot = PluginSlot::new(&id, "equalizer");
                slot.controls = eq::controls(&[]);
                slot.controls
                    .insert("g_out".to_string(), db(c("Gain", 0.0)).clamp(0.0, 10.0));
                slot
            }
            "delay" => PluginSlot::new(&id, "delay")
                .control("time_l", c("Delay", 0.0).clamp(0.0, 1000.0))
                .control("mode_l", 2.0)
                .control("time_r", c("Delay", 0.0).clamp(0.0, 1000.0))
                .control("mode_r", 2.0),
            "convolver" => {
                let mut slot = PluginSlot::new(&id, "convolver")
                    .control("gain", db(c("Gain", 0.0)).clamp(0.0, 4.0));
                slot.path = old.get("path").and_then(Value::as_str).map(str::to_string);
                slot
            }
            other => {
                match other.strip_prefix("ladspa:bingux_dsp:bingux_") {
                    Some("compressor") => PluginSlot::new(&id, "compressor")
                        .control("al", db(c("Threshold", -18.0)).clamp(0.001, 1.0))
                        .control("cr", c("Ratio", 4.0).clamp(1.0, 100.0))
                        .control("at", c("Attack", 10.0).clamp(0.0, 2000.0))
                        .control("rt", c("Release", 150.0).clamp(0.0, 5000.0))
                        .control("kn", db(-c("Knee", 6.0)).clamp(0.0631, 1.0))
                        .control("mk", db(c("Makeup", 0.0)).clamp(0.001, 1000.0))
                        .control("cdw", c("Mix", 100.0).clamp(0.0, 100.0)),
                    Some("limiter") => PluginSlot::new(&id, "limiter")
                        .control("th", db(c("Threshold", -1.0)).clamp(0.003_981, 1.0))
                        .control("rt", c("Release", 100.0).clamp(0.25, 20.0)),
                    Some("gate") => PluginSlot::new(&id, "gate")
                        .control("gt", db(c("Threshold", -45.0)).clamp(0.001, 1.0))
                        .control("at", c("Attack", 5.0).clamp(0.0, 2000.0))
                        .control("hold", c("Hold", 50.0).clamp(0.0, 1000.0))
                        .control("rt", c("Release", 200.0).clamp(0.0, 5000.0))
                        .control("gr", db(c("Range", -60.0)).clamp(0.000_251, 3981.0)),
                    Some("expander") => PluginSlot::new(&id, "expander")
                        .control("al", db(c("Threshold", -40.0)).clamp(0.001, 1.0))
                        .control("er", c("Ratio", 2.0).clamp(1.0, 100.0))
                        .control("at", c("Attack", 10.0).clamp(0.0, 2000.0))
                        .control("rt", c("Release", 200.0).clamp(0.0, 5000.0)),
                    Some("deesser") => PluginSlot::new(&id, "deesser")
                        .control(
                            "threshold",
                            db(c("Threshold", -30.0)).clamp(0.000_976_563, 1.0),
                        )
                        .control("f1_freq", c("Frequency", 6000.0).clamp(10.0, 18_000.0))
                        .control("ratio", c("Ratio", 4.0).clamp(1.0, 20.0)),
                    Some(
                        which @ ("autogain" | "loudness" | "bass_enhancer" | "exciter"
                        | "stereo_tools" | "reverb" | "maximizer"),
                    ) => {
                        let (kind, title) = match which {
                            "autogain" => ("autogain", "Auto gain"),
                            "loudness" => ("loudness", "Loudness"),
                            "bass_enhancer" => ("bass_enhancer", "Bass enhancer"),
                            "exciter" => ("exciter", "Exciter"),
                            "stereo_tools" => ("stereo_tools", "Stereo tools"),
                            "reverb" => ("reverb", "Reverb"),
                            _ => ("maximizer", "Maximizer"),
                        };
                        notes.push(format!(
                            "{title} is now a different plugin; its settings start at the defaults."
                        ));
                        PluginSlot::new(&id, kind)
                    }
                    Some("multiband_compressor") => {
                        notes.push("Multiband compressor is now a different plugin; its settings start at the defaults.".to_string());
                        PluginSlot::new(&id, "multiband_compressor")
                    }
                    Some("multiband_gate") => {
                        notes.push("Multiband gate is now a different plugin; its settings start at the defaults.".to_string());
                        PluginSlot::new(&id, "multiband_gate")
                    }
                    Some(gone @ ("crossfeed" | "echo")) => {
                        notes.push(format!(
                            "{} was removed: there is no published plugin to replace it.",
                            if gone == "echo" { "Echo" } else { "Crossfeed" }
                        ));
                        continue;
                    }
                    _ => {
                        notes.push(format!("An effect of type \"{other}\" was removed: it is not one Bingux offers."));
                        continue;
                    }
                }
            }
        };
        slot.enabled = old.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        preset.plugins.push(slot);
    }
    Ok(Migrated { preset, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = r#"{
      "format": "bingux-effects-preset", "version": 1, "name": "Night", "direction": "output",
      "plugins": [
        { "id": "equalizer-1", "type": "equalizer", "preamp": -2.0,
          "bands": [ { "type": "low_shelf", "frequency": 100.0, "gain": 6.0, "q": 0.707, "enabled": true },
                     { "type": "peaking", "frequency": 3000.0, "gain": 3.0, "q": 1.0, "enabled": false } ] },
        { "id": "compressor-1", "type": "ladspa:bingux_dsp:bingux_compressor", "enabled": false,
          "controls": { "Threshold": -20.0, "Ratio": 6.0, "Makeup": 6.0 } },
        { "id": "crossfeed-1", "type": "ladspa:bingux_dsp:bingux_crossfeed" },
        { "id": "reverb-1", "type": "ladspa:bingux_dsp:bingux_reverb" }
      ] }"#;

    #[test]
    fn a_version_1_preset_moves_to_the_published_plugins() {
        assert!(is_legacy(OLD));
        let migrated = migrate(OLD).unwrap();
        let preset = &migrated.preset;
        assert_eq!(preset.version, VERSION);
        assert_eq!(
            preset
                .plugins
                .iter()
                .map(|slot| slot.kind.as_str())
                .collect::<Vec<_>>(),
            ["equalizer", "compressor", "reverb"]
        );
        let eq = &preset.plugins[0];
        assert_eq!(eq.controls["ftl_0"], BandKind::LowShelf.code());
        assert_eq!(eq.controls["ftl_1"], 0.0, "a disabled band is off");
        assert!(
            (eq.controls["g_in"] - db(-2.0)).abs() < 1e-9,
            "the preamp became input gain"
        );
        let comp = &preset.plugins[1];
        assert!(!comp.enabled);
        assert!((comp.controls["al"] - 0.1).abs() < 1e-9);
        assert!((comp.controls["mk"] - db(6.0)).abs() < 1e-9);
        assert_eq!(comp.controls["cr"], 6.0);
        assert_eq!(migrated.notes.len(), 2);
        assert!(migrated.notes[0].contains("Crossfeed"));
        assert!(
            crate::preset::Preset::parse(&preset.to_json()).is_ok(),
            "the result is a valid version 2 file"
        );
    }

    #[test]
    fn a_current_preset_is_not_legacy() {
        assert!(!is_legacy(
            r#"{"format":"bingux-effects-preset","version":2,"name":"x","direction":"output"}"#
        ));
        assert!(migrate("[]").is_err());
    }
}
