//! Reading presets from other programs: EasyEffects JSON and Equalizer APO /
//! AutoEq text.
//!
//! EasyEffects hosts the same plugins as Bingux, so most of its settings map
//! one to one onto a port (see `import_table`). What does not map is listed
//! in [`Imported::unmapped`], so the person knows what was dropped.

use serde_json::Value;

use crate::catalog::Catalog;
use crate::eq::{self, Band, BandKind};
use crate::import_table::{self, Conv};
use crate::preset::{Direction, PluginSlot, Preset};

/// A preset read from another format.
#[derive(Clone, Debug)]
pub struct Imported {
    pub preset: Preset,
    /// What was turned into an effect or setting, one line each.
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
pub fn read(
    text: &str,
    name: &str,
    direction: Direction,
    catalog: &Catalog,
) -> Result<Vec<Imported>, ImportError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let value: Value =
            serde_json::from_str(text).map_err(|e| ImportError(format!("Not valid JSON: {e}")))?;
        if value.get("format").and_then(Value::as_str) == Some(crate::preset::FORMAT) {
            let (preset, notes) = if crate::legacy::is_legacy(text) {
                let migrated = crate::legacy::migrate(text).map_err(|e| ImportError(e.0))?;
                (migrated.preset, migrated.notes)
            } else {
                (
                    Preset::parse(text).map_err(|e| ImportError(e.0))?,
                    Vec::new(),
                )
            };
            return Ok(vec![Imported {
                mapped: vec![format!("{} effects", preset.plugins.len())],
                unmapped: notes,
                preset,
            }]);
        }
        if value.get("output").is_some() || value.get("input").is_some() {
            return easyeffects(&value, name, catalog);
        }
        return Err(ImportError(
            "This JSON file is neither a Bingux preset nor an EasyEffects preset.".to_string(),
        ));
    }
    let imported = equalizer_apo(text, name, direction);
    if imported.preset.plugins.is_empty() {
        return Err(ImportError(
            "No filters were found in this file.".to_string(),
        ));
    }
    Ok(vec![imported])
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn db_to_linear(db: f64) -> f64 {
    if db <= -100.0 {
        0.0
    } else {
        10f64.powf(db / 20.0)
    }
}

/// An EasyEffects preset file. It holds an `output` section, an `input`
/// section, or both; each becomes a preset.
fn easyeffects(value: &Value, name: &str, catalog: &Catalog) -> Result<Vec<Imported>, ImportError> {
    let mut found = Vec::new();
    for direction in Direction::ALL {
        let Some(section) = value
            .get(direction.name())
            .filter(|section| section.is_object())
        else {
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
            map_plugin(&mut imported, key, settings, catalog);
        }
        found.push(imported);
    }
    if found.is_empty() {
        return Err(ImportError(
            "The EasyEffects preset has no output or input section.".to_string(),
        ));
    }
    Ok(found)
}

fn ee_band_kind(kind: &str) -> Option<BandKind> {
    Some(match kind {
        "Bell" => BandKind::Bell,
        "Hi-pass" => BandKind::HighPass,
        "Hi-shelf" => BandKind::HighShelf,
        "Lo-pass" => BandKind::LowPass,
        "Lo-shelf" => BandKind::LowShelf,
        "Notch" => BandKind::Notch,
        "Allpass" => BandKind::AllPass,
        "Bandpass" => BandKind::BandPass,
        "Resonance" => BandKind::Resonance,
        _ => return None,
    })
}

/// The equalizer's bands from one side's `bandN` objects.
fn ee_bands(side: &Value, count: u64, notes: &mut Vec<String>) -> Vec<Band> {
    let mut bands = Vec::new();
    for index in 0..count {
        let Some(band) = side.get(format!("band{index}")) else {
            continue;
        };
        let kind = band.get("type").and_then(Value::as_str).unwrap_or("Off");
        let Some(mapped) = ee_band_kind(kind) else {
            if kind != "Off" {
                notes.push(format!(
                    "band {index}: filter type \"{kind}\" has no equivalent"
                ));
            }
            continue;
        };
        let slope = band
            .get("slope")
            .and_then(Value::as_str)
            .and_then(|slope| slope.strip_prefix('x'))
            .and_then(|n| n.parse::<u8>().ok())
            .unwrap_or(1);
        bands.push(Band {
            kind: mapped,
            frequency: number(band, "frequency").unwrap_or(1000.0),
            gain: number(band, "gain").unwrap_or(0.0),
            q: number(band, "q").unwrap_or(1.0),
            slope: slope.clamp(1, 4) - 1,
            mute: band.get("mute").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    bands
}

/// The value of an Easy Effects setting that may sit in a nested object:
/// `sidechain-type` is `sidechain.type` in newer presets.
fn setting<'a>(settings: &'a Value, key: &str) -> Option<&'a Value> {
    settings.get(key).or_else(|| {
        key.split_once('-')
            .and_then(|(group, rest)| settings.get(group)?.get(rest))
    })
}

/// One setting as a number for the port: switches are 0 and 1, decibels
/// become a linear gain, and a named choice becomes its number.
fn port_value(raw: &Value, conv: Conv, choices: &[crate::catalog::Choice]) -> Option<f64> {
    let number = match raw {
        Value::Bool(on) => f64::from(u8::from(*on != (conv == Conv::Inverted))),
        Value::Number(number) => number.as_f64()?,
        Value::String(label) => {
            let label = label.to_lowercase();
            choices
                .iter()
                .find(|choice| choice.label.to_lowercase() == label)
                .or_else(|| {
                    choices.iter().find(|choice| {
                        choice.label.to_lowercase().starts_with(&label)
                            || label.starts_with(&choice.label.to_lowercase())
                    })
                })?
                .value
        }
        _ => return None,
    };
    Some(if conv == Conv::Db && raw.is_number() {
        db_to_linear(number)
    } else {
        number
    })
}

/// Map one EasyEffects plugin (`compressor#0`) to an effect of the stack.
fn map_plugin(imported: &mut Imported, key: &str, settings: &Value, catalog: &Catalog) {
    let base = key.split('#').next().unwrap_or(key);
    let bypass = settings
        .get("bypass")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(effect) = crate::effects::find(base) else {
        imported
            .unmapped
            .push(format!("{base}: Bingux has no equivalent effect"));
        return;
    };
    let id = imported.preset.fresh_id(&base.replace('_', "-"));
    let mut slot = PluginSlot::new(id, base);
    let plugin = catalog.plugin_of(effect);
    let mut notes: Vec<String> = Vec::new();
    match base {
        "equalizer" | "midside_equalizer" => {
            let count = settings
                .get("num-bands")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let mid_side = base == "midside_equalizer";
            let sides = if mid_side {
                ["mid", "side"]
            } else {
                ["left", "right"]
            };
            let first = settings
                .get(sides[0])
                .map(|side| ee_bands(side, count, &mut notes))
                .unwrap_or_default();
            let split = mid_side
                || settings
                    .get("split-channels")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            let second = if split {
                settings
                    .get(sides[1])
                    .map(|side| ee_bands(side, count, &mut notes))
                    .unwrap_or_default()
            } else {
                first.clone()
            };
            slot.controls = eq::controls_for(&first, &second, mid_side);
            if let Some(mode) = settings.get("mode").and_then(Value::as_str) {
                let value = match mode {
                    "FIR" => Some(1.0),
                    "FFT" => Some(2.0),
                    "SPM" => Some(3.0),
                    _ => None,
                };
                if let Some(value) = value {
                    slot.controls.insert("mode".to_string(), value);
                }
            }
        }
        "convolver" => match settings
            .get("kernel-path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        {
            Some(path) => slot.path = Some(path.to_string()),
            None => {
                imported
                    .unmapped
                    .push("convolver: it had no impulse response".to_string());
                return;
            }
        },
        "pitch" => {
            for (from, to) in [
                ("cents", "Cents"),
                ("semitones", "Semitones"),
                ("octaves", "Octaves"),
            ] {
                if let Some(value) = number(settings, from) {
                    slot.controls.insert(to.to_string(), value);
                }
            }
            notes.push(
                "pitch is done by a different library, so it sounds a little different".to_string(),
            );
        }
        "autogain" => {
            if let Some(target) = number(settings, "target") {
                slot.controls
                    .insert("level".to_string(), target.clamp(-60.0, 0.0));
            }
            notes.push(
                "the plugin is a different one, so only the target loudness carries over"
                    .to_string(),
            );
        }
        _ => {}
    }
    // The settings every plugin names after one of its ports.
    for (name, symbol, conv) in import_table::ports(base) {
        let Some(raw) = setting(settings, name) else {
            continue;
        };
        let control = plugin.and_then(|plugin| plugin.control(symbol));
        if plugin.is_some() && control.is_none() {
            continue;
        }
        let choices = control.map_or(&[][..], |control| control.choices.as_slice());
        match port_value(raw, *conv, choices) {
            Some(mut value) => {
                if let Some(control) = control {
                    value = value.clamp(control.min, control.max);
                }
                slot.controls.insert((*symbol).to_string(), value);
            }
            None => notes.push(format!("{name}: \"{raw}\" has no equivalent")),
        }
    }
    if let Some(symbol) = effect.input_gain
        && let Some(db) = number(settings, "input-gain")
    {
        slot.controls.insert(symbol.to_string(), db_to_linear(db));
    }
    if let Some(symbol) = effect.output_gain
        && let Some(db) = number(settings, "output-gain")
    {
        slot.controls.insert(symbol.to_string(), db_to_linear(db));
    }
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
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let lower = line.to_lowercase();
        if let Some(rest) = lower.strip_prefix("preamp:") {
            preamp = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
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
                    bands.push(Band::bell(*frequency, *gain, 2.0));
                }
            }
            unmapped
                .push("GraphicEQ: drawn as bell filters, so the curve is approximate".to_string());
        } else {
            let keyword = line.split(':').next().unwrap_or(line);
            unmapped.push(format!("{keyword}: not supported"));
        }
    }
    if bands.len() > eq::BANDS {
        unmapped.push(format!(
            "only the first {} filters fit; {} were left out",
            eq::BANDS,
            bands.len() - eq::BANDS
        ));
        bands.truncate(eq::BANDS);
    }
    let mapped = vec![format!("{} filters, preamp {preamp} dB", bands.len())];
    let mut preset = Preset::new(name, direction);
    if !bands.is_empty() {
        let mut slot = PluginSlot::new("equalizer-1", "equalizer");
        slot.controls = eq::controls(&bands);
        if preamp != 0.0 {
            slot.controls
                .insert("g_in".to_string(), db_to_linear(preamp).clamp(0.0, 10.0));
        }
        preset.plugins.push(slot);
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
    let state = tokens
        .first()
        .map(|token| token.to_uppercase())
        .unwrap_or_default();
    if state == "OFF" {
        return Ok(None);
    }
    let kind = tokens
        .get(1)
        .map(|token| token.to_uppercase())
        .unwrap_or_default();
    let (kind, default_q) = match kind.as_str() {
        "PK" | "PEQ" | "MODAL" => (BandKind::Bell, 1.0),
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
        let at = tokens
            .iter()
            .position(|token| token.eq_ignore_ascii_case(key))?;
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
    Ok(Some(Band::new(
        kind,
        frequency,
        value_after("Gain").unwrap_or(0.0),
        q,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AudioPort, Backend, Choice, Control, PluginInfo, Unit};

    const AUTOEQ: &str = "Preamp: -6.2 dB\n\
        Filter 1: ON PK Fc 31 Hz Gain 3.5 dB Q 1.41\n\
        Filter 2: ON LSC Fc 105 Hz Gain 4.0 dB\n\
        Filter 3: ON HSC Fc 8000 Hz Gain -2.5 dB Q 0.7\n\
        Filter 4: OFF PK Fc 500 Hz Gain 1 dB Q 1\n\
        Filter 5: ON LS 6dB Fc 50 Hz Gain 1 dB\n\
        Convolution: x.wav\n";

    fn compressor_catalog() -> Catalog {
        let effect = crate::effects::find("compressor").unwrap();
        let control = |symbol: &str, min: f64, max: f64, choices: Vec<Choice>| Control {
            symbol: symbol.into(),
            name: symbol.into(),
            min,
            max,
            default: min,
            log: false,
            integer: !choices.is_empty(),
            toggle: false,
            choices,
            unit: Unit::None,
            hidden: false,
        };
        Catalog {
            plugins: vec![PluginInfo {
                id: effect.source.plugin_id(),
                name: "c".into(),
                maker: String::new(),
                backend: Backend::Lv2,
                library: None,
                label: String::new(),
                inputs: vec![AudioPort {
                    symbol: "in_l".into(),
                    name: String::new(),
                    optional: false,
                    side_chain: false,
                }],
                outputs: Vec::new(),
                controls: vec![
                    control("al", 0.001, 1.0, Vec::new()),
                    control("cr", 1.0, 100.0, Vec::new()),
                    control("at", 0.0, 2000.0, Vec::new()),
                    control(
                        "cm",
                        0.0,
                        2.0,
                        vec![
                            Choice {
                                value: 0.0,
                                label: "Down".into(),
                            },
                            Choice {
                                value: 1.0,
                                label: "Up".into(),
                            },
                        ],
                    ),
                    control("g_in", 0.0, 1000.0, Vec::new()),
                ],
                meters: Vec::new(),
                latency: None,
            }],
        }
    }

    #[test]
    fn autoeq_text_becomes_one_equalizer() {
        let imported = equalizer_apo(AUTOEQ, "Headphones", Direction::Output);
        let slot = &imported.preset.plugins[0];
        assert!((slot.controls["g_in"] - 10f64.powf(-6.2 / 20.0)).abs() < 1e-9);
        assert_eq!(slot.controls["ftl_0"], BandKind::Bell.code());
        assert_eq!(slot.controls["fl_0"], 31.0);
        assert_eq!(slot.controls["ql_0"], 1.41);
        assert_eq!(slot.controls["ftl_1"], BandKind::LowShelf.code());
        assert_eq!(slot.controls["ql_1"], 0.707);
        assert_eq!(
            slot.controls["ftl_3"],
            BandKind::LowShelf.code(),
            "the filter that is off is skipped"
        );
        assert_eq!(slot.controls["ftl_4"], 0.0);
        // Convolution: is not read.
        assert_eq!(imported.unmapped.len(), 1, "{:?}", imported.unmapped);
    }

    #[test]
    fn a_bandwidth_becomes_a_q() {
        let band = apo_filter("Filter: ON PK Fc 1000 Hz Gain 3 dB BW Oct 1.0")
            .unwrap()
            .unwrap();
        assert!((band.q - 1.414).abs() < 0.01, "{}", band.q);
    }

    #[test]
    fn easyeffects_presets_map_through_the_port_table() {
        let text = r#"{ "output": {
            "blocklist": [],
            "plugins_order": ["equalizer#0", "compressor#0", "crossfeed#0", "limiter#0"],
            "equalizer#0": { "bypass": false, "mode": "IIR", "num-bands": 3, "input-gain": -1.0, "output-gain": 0.0,
                "split-channels": false,
                "left": {
                    "band0": { "type": "Lo-shelf", "frequency": 80.0, "gain": 4.0, "q": 0.7, "mute": false, "slope": "x1" },
                    "band1": { "type": "Bell", "frequency": 1000.0, "gain": -3.0, "q": 1.2, "mute": false, "slope": "x2" },
                    "band2": { "type": "Off", "frequency": 5000.0, "gain": 0.0, "q": 1.0, "mute": false, "slope": "x1" } } },
            "compressor#0": { "bypass": false, "mode": "Down", "threshold": -20.0, "ratio": 3.0, "attack": 15.0,
                "input-gain": 3.0, "sidechain": { "type": "Feed-forward" } },
            "crossfeed#0": { "bypass": false },
            "limiter#0": { "bypass": true, "threshold": -2.0 } } }"#;
        let imported = read(text, "Imported", Direction::Output, &compressor_catalog()).unwrap();
        let preset = &imported[0].preset;
        assert_eq!(
            preset
                .plugins
                .iter()
                .map(|slot| slot.kind.as_str())
                .collect::<Vec<_>>(),
            ["equalizer", "compressor", "limiter"]
        );
        let eq = &preset.plugins[0];
        assert_eq!(eq.controls["ftl_0"], BandKind::LowShelf.code());
        assert_eq!(eq.controls["fl_1"], 1000.0);
        assert_eq!(eq.controls["sl_1"], 1.0, "x2 is the plugin's second slope");
        assert_eq!(
            eq.controls["ftr_0"],
            BandKind::LowShelf.code(),
            "unsplit bands drive both sides"
        );
        let comp = &preset.plugins[1];
        assert!(
            (comp.controls["al"] - 0.1).abs() < 1e-9,
            "-20 dB threshold is a 0.1 gain"
        );
        assert_eq!(comp.controls["cr"], 3.0);
        assert_eq!(comp.controls["at"], 15.0);
        assert_eq!(
            comp.controls["cm"], 0.0,
            "\"Down\" is the plugin's first mode"
        );
        assert!((comp.controls["g_in"] - 10f64.powf(3.0 / 20.0)).abs() < 1e-9);
        assert!(!preset.plugins[2].enabled);
        assert!(
            imported[0]
                .unmapped
                .iter()
                .any(|line| line.starts_with("crossfeed:"))
        );
    }

    #[test]
    fn a_native_preset_is_read_as_is_and_junk_is_refused() {
        let preset = Preset::new("Mine", Direction::Input);
        let catalog = Catalog::default();
        assert_eq!(
            read(&preset.to_json(), "x", Direction::Output, &catalog).unwrap()[0].preset,
            preset
        );
        assert!(read("hello world", "x", Direction::Output, &catalog).is_err());
        assert!(read("{\"a\": 1}", "x", Direction::Output, &catalog).is_err());
    }
}
