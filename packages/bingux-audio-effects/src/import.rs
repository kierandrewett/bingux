//! Reading presets from other programs: EasyEffects JSON and Equalizer APO /
//! AutoEq text.
//!
//! What maps cleanly becomes a plugin of the chain. What does not is listed
//! in [`Imported::unmapped`], so the person knows what was dropped or
//! approximated.

use serde_json::Value;

use crate::preset::{Band, BandKind, CONVOLVER, DELAY, Direction, PluginSlot, Preset};

/// A preset read from another format.
#[derive(Clone, Debug)]
pub struct Imported {
    pub preset: Preset,
    /// What was turned into a plugin or setting, one line each.
    pub mapped: Vec<String>,
    /// What was left out or only approximated, one line each.
    pub unmapped: Vec<String>,
}

/// Why a file could not be read.
#[derive(Debug, PartialEq, Eq)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImportError {}

/// Read `text`, whatever format it is in: a Bingux preset, an EasyEffects
/// preset or Equalizer APO text. `name` names presets the file does not
/// name itself, and `direction` is used when the file does not say.
pub fn read(text: &str, name: &str, direction: Direction) -> Result<Vec<Imported>, ImportError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let value: Value = serde_json::from_str(text).map_err(|e| ImportError(format!("Not valid JSON: {e}")))?;
        if value.get("format").and_then(Value::as_str) == Some(crate::preset::FORMAT) {
            let preset = Preset::parse(text).map_err(|e| ImportError(e.0))?;
            return Ok(vec![Imported {
                mapped: vec![format!("{} plugins", preset.plugins.len())],
                unmapped: Vec::new(),
                preset,
            }]);
        }
        if value.get("output").is_some() || value.get("input").is_some() {
            return easyeffects(&value, name);
        }
        return Err(ImportError("This JSON file is neither a Bingux preset nor an EasyEffects preset.".to_string()));
    }
    let imported = equalizer_apo(text, name, direction);
    if imported.preset.plugins.is_empty() {
        return Err(ImportError("No filters were found in this file.".to_string()));
    }
    Ok(vec![imported])
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// An EasyEffects preset file. It holds an `output` section, an `input`
/// section, or both; each becomes a preset.
fn easyeffects(value: &Value, name: &str) -> Result<Vec<Imported>, ImportError> {
    let mut found = Vec::new();
    for direction in Direction::ALL {
        let Some(section) = value.get(direction.name()).filter(|section| section.is_object()) else {
            continue;
        };
        let mut imported = Imported {
            preset: Preset::new(name, direction),
            mapped: Vec::new(),
            unmapped: Vec::new(),
        };
        let order: Vec<&str> = section
            .get("plugins_order")
            .and_then(Value::as_array)
            .map(|order| order.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for key in order {
            let Some(settings) = section.get(key) else {
                continue;
            };
            map_plugin(&mut imported, key, settings);
        }
        found.push(imported);
    }
    if found.is_empty() {
        return Err(ImportError("The EasyEffects preset has no output or input section.".to_string()));
    }
    Ok(found)
}

fn ee_band_kind(kind: &str) -> Option<BandKind> {
    Some(match kind {
        "Bell" => BandKind::Peaking,
        "Hi-pass" => BandKind::HighPass,
        "Hi-shelf" => BandKind::HighShelf,
        "Lo-pass" => BandKind::LowPass,
        "Lo-shelf" => BandKind::LowShelf,
        "Notch" => BandKind::Notch,
        "Allpass" => BandKind::AllPass,
        "Bandpass" => BandKind::BandPass,
        _ => return None,
    })
}

/// Map one EasyEffects plugin (`compressor#0`) to a plugin of the chain.
fn map_plugin(imported: &mut Imported, key: &str, settings: &Value) {
    let base = key.split('#').next().unwrap_or(key);
    let bypass = settings.get("bypass").and_then(Value::as_bool).unwrap_or(false);
    let id = imported.preset.fresh_id(&base.replace('_', "-"));
    let dsp = |label: &str| PluginSlot::ladspa(id.clone(), "bingux_dsp", label);
    let mut notes: Vec<String> = Vec::new();
    let get = |name: &str| number(settings, name);
    let mut slot = match base {
        "equalizer" => {
            let mut bands = Vec::new();
            let count = settings.get("num-bands").and_then(Value::as_u64).unwrap_or(0);
            if let Some(left) = settings.get("left") {
                for index in 0..count {
                    let Some(band) = left.get(format!("band{index}")) else {
                        continue;
                    };
                    let kind = band.get("type").and_then(Value::as_str).unwrap_or("Off");
                    let Some(mapped) = ee_band_kind(kind) else {
                        if kind != "Off" {
                            notes.push(format!("band {index}: filter type \"{kind}\" has no equivalent"));
                        }
                        continue;
                    };
                    if band.get("slope").and_then(Value::as_str).is_some_and(|slope| slope != "x1") {
                        notes.push(format!("band {index}: steeper slopes became a single filter"));
                    }
                    bands.push(Band {
                        kind: mapped,
                        frequency: number(band, "frequency").unwrap_or(1000.0),
                        gain: number(band, "gain").unwrap_or(0.0),
                        q: number(band, "q").unwrap_or(1.0),
                        enabled: !band.get("mute").and_then(Value::as_bool).unwrap_or(false),
                    });
                }
            }
            if settings.get("split-channels").and_then(Value::as_bool) == Some(true) {
                notes.push("the right channel had its own bands; the left ones are used for both".to_string());
            }
            if settings.get("mode").and_then(Value::as_str).is_some_and(|mode| mode != "IIR") {
                notes.push("FIR and FFT modes became ordinary filters".to_string());
            }
            let preamp = get("input-gain").unwrap_or(0.0) + get("output-gain").unwrap_or(0.0);
            Some(PluginSlot::equalizer(id.clone(), preamp, bands))
        }
        "compressor" => {
            if settings.get("mode").and_then(Value::as_str).is_some_and(|mode| mode != "Downward") {
                notes.push("only the downward mode exists; upward and boosting compression were not mapped".to_string());
            }
            if settings.get("hpf-mode").and_then(Value::as_str).is_some_and(|mode| mode != "off") {
                notes.push("the side-chain filters were dropped".to_string());
            }
            let dry = db_to_linear(get("dry").unwrap_or(-100.0).max(-100.0));
            let wet = db_to_linear(get("wet").unwrap_or(0.0));
            let mut slot = dsp("bingux_compressor");
            for (from, to) in [("threshold", "Threshold"), ("ratio", "Ratio"), ("attack", "Attack"), ("release", "Release"), ("makeup", "Makeup")] {
                if let Some(value) = get(from) {
                    slot = slot.control(to, value);
                }
            }
            if let Some(knee) = get("knee") {
                slot = slot.control("Knee", knee.abs().min(24.0));
            }
            slot = slot.control("Mix", 100.0 * wet / (wet + dry));
            Some(slot)
        }
        "limiter" => {
            let mut slot = dsp("bingux_limiter");
            if let Some(value) = get("threshold") {
                slot = slot.control("Threshold", value);
            }
            if let Some(value) = get("release") {
                slot = slot.control("Release", value);
            }
            notes.push("look-ahead is fixed at 3 ms and oversampling is not used".to_string());
            Some(slot)
        }
        "gate" => {
            let mut slot = dsp("bingux_gate");
            if let Some(value) = get("curve-threshold").or_else(|| get("threshold")) {
                slot = slot.control("Threshold", value);
            }
            for (from, to) in [("attack", "Attack"), ("release", "Release")] {
                if let Some(value) = get(from) {
                    slot = slot.control(to, value);
                }
            }
            if let Some(value) = get("reduction") {
                slot = slot.control("Range", -value.abs());
            }
            Some(slot)
        }
        "expander" => {
            let mut slot = dsp("bingux_expander");
            for (from, to) in [("threshold", "Threshold"), ("ratio", "Ratio"), ("attack", "Attack"), ("release", "Release")] {
                if let Some(value) = get(from) {
                    slot = slot.control(to, value);
                }
            }
            Some(slot)
        }
        "deesser" => {
            let mut slot = dsp("bingux_deesser");
            for (from, to) in [("threshold", "Threshold"), ("ratio", "Ratio"), ("f1-freq", "Frequency")] {
                if let Some(value) = get(from) {
                    slot = slot.control(to, value);
                }
            }
            notes.push("the detection band and the split are simpler than the original".to_string());
            Some(slot)
        }
        "autogain" => {
            notes.push("the target was a loudness in LUFS; it is used as a level in dB".to_string());
            let mut slot = dsp("bingux_autogain");
            if let Some(target) = get("target") {
                slot = slot.control("Target", target.clamp(-40.0, -6.0));
            }
            Some(slot)
        }
        "crossfeed" => {
            let mut slot = dsp("bingux_crossfeed");
            if let Some(value) = get("fcut") {
                slot = slot.control("Cutoff", value);
            }
            if let Some(value) = get("feed") {
                slot = slot.control("Feed", value);
            }
            Some(slot)
        }
        "stereo_tools" => {
            let mut slot = dsp("bingux_stereo_tools");
            if settings.get("phasel").and_then(Value::as_bool) == Some(true) {
                slot = slot.control("Invert left", 1.0);
            }
            if settings.get("phaser").and_then(Value::as_bool) == Some(true) {
                slot = slot.control("Invert right", 1.0);
            }
            notes.push("only the polarity switches were mapped".to_string());
            Some(slot)
        }
        "bass_enhancer" | "exciter" => {
            let label = if base == "exciter" { "bingux_exciter" } else { "bingux_bass_enhancer" };
            let mut slot = dsp(label);
            if let Some(value) = get("scope") {
                slot = slot.control("Frequency", value);
            }
            if let Some(amount) = get("amount") {
                slot = slot.control("Amount", (amount / 12.0 * 100.0).clamp(0.0, 100.0));
            }
            notes.push("the amount is approximate; the harmonics are made differently".to_string());
            Some(slot)
        }
        "convolver" => match settings.get("kernel-path").and_then(Value::as_str).filter(|path| !path.is_empty()) {
            Some(path) => {
                let mut slot = PluginSlot::new(id.clone(), CONVOLVER);
                slot.path = Some(path.to_string());
                Some(slot)
            }
            None => {
                notes.push("it had no impulse response".to_string());
                None
            }
        },
        "delay" => {
            let mut slot = PluginSlot::new(id.clone(), DELAY);
            slot = slot.control("Delay", get("time-l").unwrap_or(0.0));
            notes.push("one delay for both channels (the left one)".to_string());
            Some(slot)
        }
        "filter" => {
            let mode = settings.get("mode").and_then(Value::as_str).unwrap_or("");
            let kind = if mode.contains("Low-pass") {
                Some(BandKind::LowPass)
            } else if mode.contains("High-pass") {
                Some(BandKind::HighPass)
            } else if mode.contains("Band-pass") {
                Some(BandKind::BandPass)
            } else if mode.contains("Band-reject") {
                Some(BandKind::Notch)
            } else {
                None
            };
            kind.map(|kind| {
                notes.push("the filter is one 12 dB per octave stage".to_string());
                PluginSlot::equalizer(
                    id.clone(),
                    0.0,
                    vec![Band {
                        kind,
                        frequency: get("frequency").unwrap_or(1000.0),
                        gain: 0.0,
                        q: get("resonance").map_or(0.707, |resonance| resonance.max(0.1)),
                        enabled: true,
                    }],
                )
            })
        }
        _ => None,
    };
    let Some(mut slot) = slot.take() else {
        imported.unmapped.push(format!("{base}: no equivalent plugin"));
        return;
    };
    slot.enabled = !bypass;
    imported.mapped.push(base.to_string());
    for note in notes {
        imported.unmapped.push(format!("{base}: {note}"));
    }
    imported.preset.plugins.push(slot);
}

/// Equalizer APO text, as AutoEq writes it: a `Preamp:` line and `Filter:`
/// lines, or a `GraphicEQ:` line.
pub fn equalizer_apo(text: &str, name: &str, direction: Direction) -> Imported {
    let mut preamp = 0.0;
    let mut bands = Vec::new();
    let mut unmapped = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let lower = line.to_lowercase();
        if let Some(rest) = lower.strip_prefix("preamp:") {
            preamp = rest.split_whitespace().next().and_then(|value| value.parse().ok()).unwrap_or(0.0);
        } else if lower.starts_with("filter") {
            match apo_filter(line) {
                Ok(Some(band)) => bands.push(band),
                Ok(None) => {}
                Err(reason) => unmapped.push(format!("{line}: {reason}")),
            }
        } else if let Some(rest) = lower.strip_prefix("graphiceq:") {
            let points: Vec<(f64, f64)> = rest
                .split(';')
                .filter_map(|pair| {
                    let mut parts = pair.split_whitespace();
                    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
                })
                .collect();
            // A graphic curve becomes a bell at each point whose level differs from its neighbour.
            for (index, (frequency, gain)) in points.iter().enumerate() {
                let previous = if index == 0 { 0.0 } else { points[index - 1].1 };
                if (gain - previous).abs() > 0.05 || *gain != 0.0 {
                    bands.push(Band::peaking(*frequency, *gain, 2.0));
                }
            }
            bands.truncate(31);
            unmapped.push("GraphicEQ: drawn as bell filters, so the curve is approximate".to_string());
        } else {
            let keyword = line.split(':').next().unwrap_or(line);
            unmapped.push(format!("{keyword}: not supported"));
        }
    }
    let mapped = vec![format!("{} filters, preamp {preamp} dB", bands.len())];
    let mut preset = Preset::new(name, direction);
    if !bands.is_empty() {
        preset.plugins.push(PluginSlot::equalizer("equalizer-1", preamp, bands));
    }
    Imported {
        preset,
        mapped,
        unmapped,
    }
}

/// One `Filter 1: ON PK Fc 31 Hz Gain 3.5 dB Q 1.41` line. `Ok(None)` for a
/// filter that is off.
fn apo_filter(line: &str) -> Result<Option<Band>, String> {
    let body = line.split_once(':').map_or("", |(_, rest)| rest);
    let tokens: Vec<&str> = body.split_whitespace().collect();
    let state = tokens.first().map(|token| token.to_uppercase()).unwrap_or_default();
    if state == "OFF" {
        return Ok(None);
    }
    let kind = tokens.get(1).map(|token| token.to_uppercase()).unwrap_or_default();
    let (kind, default_q) = match kind.as_str() {
        "PK" | "PEQ" | "MODAL" => (BandKind::Peaking, 1.0),
        "LS" | "LSC" => (BandKind::LowShelf, 0.707),
        "HS" | "HSC" => (BandKind::HighShelf, 0.707),
        "LP" | "LPQ" => (BandKind::LowPass, 0.707),
        "HP" | "HPQ" => (BandKind::HighPass, 0.707),
        "BP" => (BandKind::BandPass, 1.0),
        "NO" => (BandKind::Notch, 1.0),
        "AP" => (BandKind::AllPass, 0.707),
        other => return Err(format!("filter type {other} is not supported")),
    };
    let value_after = |key: &str| -> Option<f64> {
        let at = tokens.iter().position(|token| token.eq_ignore_ascii_case(key))?;
        tokens.get(at + 1)?.parse().ok()
    };
    let frequency = value_after("Fc").ok_or("no frequency")?;
    let bandwidth = value_after("BW").or_else(|| value_after("Oct"));
    let q = match (value_after("Q"), bandwidth) {
        (Some(q), _) => q,
        // A bandwidth in octaves, as the cookbook relates it to Q.
        (None, Some(octaves)) => 1.0 / (2.0 * (std::f64::consts::LN_2 / 2.0 * octaves).sinh()),
        (None, None) => default_q,
    };
    Ok(Some(Band {
        kind,
        frequency,
        gain: value_after("Gain").unwrap_or(0.0),
        q,
        enabled: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTOEQ: &str = "Preamp: -6.2 dB\n\
        Filter 1: ON PK Fc 31 Hz Gain 3.5 dB Q 1.41\n\
        Filter 2: ON LSC Fc 105 Hz Gain 4.0 dB\n\
        Filter 3: ON HSC Fc 8000 Hz Gain -2.5 dB Q 0.7\n\
        Filter 4: OFF PK Fc 500 Hz Gain 1 dB Q 1\n\
        Filter 5: ON LS 6dB Fc 50 Hz Gain 1 dB\n\
        Convolution: x.wav\n";

    #[test]
    fn autoeq_text_becomes_one_equalizer() {
        let imported = equalizer_apo(AUTOEQ, "Headphones", Direction::Output);
        let slot = &imported.preset.plugins[0];
        assert_eq!(slot.preamp, Some(-6.2));
        assert_eq!(slot.bands.len(), 4);
        assert_eq!(slot.bands[0], Band::peaking(31.0, 3.5, 1.41));
        assert_eq!(slot.bands[1].kind, BandKind::LowShelf);
        assert_eq!(slot.bands[1].q, 0.707);
        assert_eq!(slot.bands[2].gain, -2.5);
        // LS 6dB keeps its corner and gain with the usual slope; Convolution: is not read.
        assert_eq!(imported.unmapped.len(), 1, "{:?}", imported.unmapped);
    }

    #[test]
    fn a_bandwidth_becomes_a_q() {
        let band = apo_filter("Filter: ON PK Fc 1000 Hz Gain 3 dB BW Oct 1.0").unwrap().unwrap();
        assert!((band.q - 1.414).abs() < 0.01, "{}", band.q);
    }

    #[test]
    fn easyeffects_presets_map_what_they_can() {
        let text = r#"{ "output": {
            "blocklist": [],
            "plugins_order": ["equalizer#0", "compressor#0", "reverb#0", "limiter#0"],
            "equalizer#0": { "bypass": false, "mode": "IIR", "num-bands": 3, "input-gain": -1.0, "output-gain": 0.0,
                "split-channels": false,
                "left": {
                    "band0": { "type": "Lo-shelf", "frequency": 80.0, "gain": 4.0, "q": 0.7, "mute": false, "slope": "x1" },
                    "band1": { "type": "Bell", "frequency": 1000.0, "gain": -3.0, "q": 1.2, "mute": false, "slope": "x1" },
                    "band2": { "type": "Off", "frequency": 5000.0, "gain": 0.0, "q": 1.0, "mute": false, "slope": "x1" } } },
            "compressor#0": { "bypass": false, "mode": "Downward", "threshold": -20.0, "ratio": 3.0, "attack": 15.0,
                "release": 200.0, "knee": -6.0, "makeup": 2.0, "dry": -100.0, "wet": 0.0, "hpf-mode": "off" },
            "reverb#0": { "bypass": false },
            "limiter#0": { "bypass": true, "threshold": -2.0, "release": 50.0 } } }"#;
        let imported = read(text, "Imported", Direction::Output).unwrap();
        let preset = &imported[0].preset;
        assert_eq!(preset.plugins.len(), 3);
        assert_eq!(preset.plugins[0].bands.len(), 2);
        assert_eq!(preset.plugins[0].preamp, Some(-1.0));
        assert_eq!(preset.plugins[1].controls["Threshold"], -20.0);
        assert_eq!(preset.plugins[1].controls["Knee"], 6.0);
        assert!((preset.plugins[1].controls["Mix"] - 100.0).abs() < 0.01);
        assert!(!preset.plugins[2].enabled);
        assert!(imported[0].unmapped.iter().any(|line| line.starts_with("reverb:")));
    }

    #[test]
    fn a_native_preset_is_read_as_is_and_junk_is_refused() {
        let preset = Preset::new("Mine", Direction::Input);
        assert_eq!(read(&preset.to_json(), "x", Direction::Output).unwrap()[0].preset, preset);
        assert!(read("hello world", "x", Direction::Output).is_err());
        assert!(read("{\"a\": 1}", "x", Direction::Output).is_err());
    }
}
