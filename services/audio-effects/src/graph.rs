//! A preset as a PipeWire `filter-chain` configuration.
//!
//! The chain becomes one filter graph with the plugins linked in order, left
//! and right channel each. A plugin with one audio input and one output is
//! run twice, once per channel; a plugin with two of each runs once as a
//! stereo plugin. The parametric equalizer is one PipeWire biquad per band,
//! so every band's frequency, gain and Q can be changed while sound plays.

use std::fmt::Write;
use std::path::Path;

use crate::catalog::{Catalog, PluginInfo};
use crate::preset::{CONVOLVER, DELAY, Direction, EQUALIZER, GAIN, PluginSlot, Preset};

/// The node name of the virtual output device (a sink applications play to).
pub const OUTPUT_NODE: &str = "bingux_effects_output";
/// The node name of the virtual input device (a source applications record from).
pub const INPUT_NODE: &str = "bingux_effects_input";
/// The catalog id of the analyser plugin the engine taps the chain's end with.
pub const ANALYSER: &str = "ladspa:bingux_dsp:bingux_analyser";

/// Where the chain's sound comes from and goes to.
#[derive(Clone, Debug, Default)]
pub struct Target {
    /// The `node.name` of the real device: the speakers the output chain plays
    /// to, or the microphone the input chain records.
    pub device: Option<String>,
    /// A shown name for the virtual device.
    pub description: Option<String>,
}

/// A plugin the chain could not include, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
}

/// The configuration and what became of each plugin.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub config: String,
    pub skipped: Vec<Skipped>,
}

/// The meter slot (see `bingux_dsp::meters`) of every plugin that publishes
/// one, in chain order. Slots start at 1.
pub fn meter_slots(preset: &Preset, catalog: &Catalog) -> Vec<(String, u32)> {
    let mut slots = Vec::new();
    for slot in &preset.plugins {
        if let Some(info) = usable(slot, catalog)
            && info.publishes_meters()
            && slots.len() < bingux_dsp::meters::SLOTS - 1
        {
            slots.push((slot.id.clone(), slots.len() as u32 + 1));
        }
    }
    slots
}

/// The plugin of `slot` when it is on and can run.
fn usable<'a>(slot: &PluginSlot, catalog: &'a Catalog) -> Option<&'a PluginInfo> {
    if !slot.enabled {
        return None;
    }
    catalog.find(&slot.kind).filter(|info| info.unusable().is_none())
}

/// A number in the form the configuration parser reads as a float.
fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:?}")
    } else {
        "0.0".to_string()
    }
}

/// A string in quotes.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `db` as the linear gain a mixer takes.
fn linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The left and right ports sound enters and leaves a stage by.
#[derive(Clone)]
struct Stage {
    inputs: [String; 2],
    outputs: [String; 2],
}

#[derive(Default)]
struct Graph {
    nodes: Vec<String>,
    links: Vec<String>,
    last: Option<Stage>,
    first_inputs: Option<[String; 2]>,
}

impl Graph {
    fn node(&mut self, text: String) {
        self.nodes.push(text);
    }

    /// Put `stage` after the last one.
    fn chain(&mut self, stage: Stage) {
        match self.last.take() {
            None => self.first_inputs = Some(stage.inputs.clone()),
            Some(previous) => {
                for channel in 0..2 {
                    self.links.push(format!(
                        "{{ output = {} input = {} }}",
                        quote(&previous.outputs[channel]),
                        quote(&stage.inputs[channel])
                    ));
                }
            }
        }
        self.last = Some(stage);
    }

    /// A built-in plugin once for each channel.
    fn builtin_pair(&mut self, name: &str, label: &str, controls: &[(&str, f64)], config: &str) -> Stage {
        let mut stage = Stage {
            inputs: [String::new(), String::new()],
            outputs: [String::new(), String::new()],
        };
        for (channel, side) in ["l", "r"].into_iter().enumerate() {
            let node = format!("{name}_{side}");
            self.node(node_text("builtin", &node, label, None, controls, config));
            stage.inputs[channel] = format!("{node}:{}", input_port(label));
            stage.outputs[channel] = format!("{node}:Out");
        }
        stage
    }
}

/// The audio input port of a built-in plugin.
fn input_port(label: &str) -> &'static str {
    if label == "mixer" { "In 1" } else { "In" }
}

fn node_text(kind: &str, name: &str, label: &str, plugin: Option<&Path>, controls: &[(&str, f64)], config: &str) -> String {
    let mut text = format!("{{ type = {kind} name = {} label = {label}", quote(name));
    if let Some(path) = plugin {
        let _ = write!(text, " plugin = {}", quote(&path.to_string_lossy()));
    }
    if !config.is_empty() {
        let _ = write!(text, " config = {{ {config} }}");
    }
    if !controls.is_empty() {
        text.push_str(" control = {");
        for (control, value) in controls {
            let _ = write!(text, " {} = {}", quote(control), number(*value));
        }
        text.push_str(" }");
    }
    text.push_str(" }");
    text
}

/// The channel count of a WAV file, or 1 when it is not one.
fn wav_channels(path: &str) -> usize {
    let Ok(mut file) = std::fs::File::open(path) else {
        return 1;
    };
    let mut header = [0u8; 24];
    if std::io::Read::read_exact(&mut file, &mut header).is_err() || &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return 1;
    }
    usize::from(u16::from_le_bytes([header[22], header[23]])).max(1)
}

/// The equalizer's nodes: a gain for the preamp, then a biquad per enabled band.
fn equalizer(graph: &mut Graph, slot: &PluginSlot) {
    let id = &slot.id;
    let preamp = slot.preamp.unwrap_or(0.0);
    if preamp != 0.0 {
        let stage = graph.builtin_pair(&format!("{id}_pre"), "mixer", &[("Gain 1", linear(preamp))], "");
        graph.chain(stage);
    }
    for (index, band) in slot.bands.iter().enumerate().filter(|(_, band)| band.enabled) {
        let gain = if band.kind.has_gain() { band.gain } else { 0.0 };
        let stage = graph.builtin_pair(
            &format!("{id}_b{index}"),
            band.kind.label(),
            &[("Freq", band.frequency), ("Q", band.q), ("Gain", gain)],
            "",
        );
        graph.chain(stage);
    }
}

/// Render `preset` for `target`. A plugin that cannot run is left out and
/// listed in `skipped`; the rest of the chain still plays.
pub fn render(preset: &Preset, catalog: &Catalog, target: &Target) -> Rendered {
    let mut graph = Graph::default();
    let mut skipped = Vec::new();
    let slots = meter_slots(preset, catalog);
    for slot in preset.plugins.iter().filter(|slot| slot.enabled) {
        let id = slot.id.as_str();
        let Some(info) = catalog.find(&slot.kind) else {
            skipped.push(Skipped {
                id: slot.id.clone(),
                reason: "The plugin is not installed.".to_string(),
            });
            continue;
        };
        if let Some(reason) = info.unusable() {
            skipped.push(Skipped { id: slot.id.clone(), reason });
            continue;
        }
        let value = |name: &str| {
            slot.controls
                .get(name)
                .copied()
                .or_else(|| info.control(name).map(|control| control.default))
                .unwrap_or(0.0)
        };
        match slot.kind.as_str() {
            EQUALIZER => equalizer(&mut graph, slot),
            GAIN => {
                let stage = graph.builtin_pair(id, "mixer", &[("Gain 1", linear(value("Gain")))], "");
                graph.chain(stage);
            }
            DELAY => {
                let seconds = value("Delay") / 1000.0;
                let stage = graph.builtin_pair(id, "delay", &[("Delay (s)", seconds)], "max-delay = 1.0");
                graph.chain(stage);
            }
            CONVOLVER => {
                let Some(path) = slot.path.as_deref().filter(|path| Path::new(path).is_file()) else {
                    skipped.push(Skipped {
                        id: slot.id.clone(),
                        reason: "There is no impulse response file.".to_string(),
                    });
                    continue;
                };
                let stereo = wav_channels(path) >= 2;
                let gain = linear(value("Gain"));
                let mut stage = Stage {
                    inputs: [String::new(), String::new()],
                    outputs: [String::new(), String::new()],
                };
                for (channel, side) in ["l", "r"].into_iter().enumerate() {
                    let node = format!("{id}_{side}");
                    let config = format!(
                        "filename = {} channel = {} gain = {}",
                        quote(path),
                        if stereo { channel } else { 0 },
                        number(gain)
                    );
                    graph.node(node_text("builtin", &node, "convolver", None, &[], &config));
                    stage.inputs[channel] = format!("{node}:In");
                    stage.outputs[channel] = format!("{node}:Out");
                }
                graph.chain(stage);
            }
            _ => {
                let Some(library) = info.library.as_deref() else {
                    continue;
                };
                let mut controls: Vec<(&str, f64)> = info
                    .controls
                    .iter()
                    .map(|control| (control.name.as_str(), value(&control.name).clamp(control.min, control.max)))
                    .collect();
                if let Some(at) = controls.iter().position(|(name, _)| *name == "Meter slot") {
                    controls[at].1 = slots.iter().find(|(slot_id, _)| slot_id == id).map_or(0.0, |(_, slot)| f64::from(*slot));
                }
                let ladspa_inputs = audio_ports(info, true);
                let ladspa_outputs = audio_ports(info, false);
                if info.inputs == 2 {
                    graph.node(node_text("ladspa", id, &info.label, Some(library), &controls, ""));
                    graph.chain(Stage {
                        inputs: [format!("{id}:{}", ladspa_inputs[0]), format!("{id}:{}", ladspa_inputs[1])],
                        outputs: [format!("{id}:{}", ladspa_outputs[0]), format!("{id}:{}", ladspa_outputs[1])],
                    });
                } else {
                    let mut stage = Stage {
                        inputs: [String::new(), String::new()],
                        outputs: [String::new(), String::new()],
                    };
                    for (channel, side) in ["l", "r"].into_iter().enumerate() {
                        let node = format!("{id}_{side}");
                        graph.node(node_text("ladspa", &node, &info.label, Some(library), &controls, ""));
                        stage.inputs[channel] = format!("{node}:{}", ladspa_inputs[0]);
                        stage.outputs[channel] = format!("{node}:{}", ladspa_outputs[0]);
                    }
                    graph.chain(stage);
                }
            }
        }
    }
    if graph.last.is_none() {
        let stage = graph.builtin_pair("bypass", "copy", &[], "");
        graph.chain(stage);
    }
    // The analyser is a pass-through tap on the end of the chain.
    if let Some(info) = catalog.find(ANALYSER)
        && let Some(library) = info.library.as_deref()
        && info.input_ports.len() == 2
        && info.output_ports.len() == 2
    {
        graph.node(node_text("ladspa", "analyser", &info.label, Some(library), &[("Publish", 1.0)], ""));
        graph.chain(Stage {
            inputs: [format!("analyser:{}", info.input_ports[0]), format!("analyser:{}", info.input_ports[1])],
            outputs: [format!("analyser:{}", info.output_ports[0]), format!("analyser:{}", info.output_ports[1])],
        });
    }
    let first = graph.first_inputs.clone().unwrap_or_default();
    let last = graph.last.clone().unwrap_or(Stage {
        inputs: [String::new(), String::new()],
        outputs: [String::new(), String::new()],
    });
    let mut config = String::new();
    let (capture, playback) = properties(preset.direction, target);
    let _ = write!(
        config,
        "# Written by Bingux Settings from the preset \"{}\". Edit the preset, not this file.\n\
         context.properties = {{ core.name = bingux-effects-{} }}\n\
         context.spa-libs = {{\n  audio.convert.* = audioconvert/libspa-audioconvert\n  support.* = support/libspa-support\n}}\n\
         context.modules = [\n\
         \x20 {{ name = libpipewire-module-rt flags = [ ifexists nofail ] }}\n\
         \x20 {{ name = libpipewire-module-protocol-native }}\n\
         \x20 {{ name = libpipewire-module-client-node }}\n\
         \x20 {{ name = libpipewire-module-adapter }}\n\
         \x20 {{ name = libpipewire-module-filter-chain\n    args = {{\n      node.description = {}\n      media.name = {}\n      filter.graph = {{\n        nodes = [\n",
        preset.name.replace('\n', " "),
        preset.direction.name(),
        quote(target.description.as_deref().unwrap_or(default_description(preset.direction))),
        quote(target.description.as_deref().unwrap_or(default_description(preset.direction))),
    );
    for node in &graph.nodes {
        let _ = writeln!(config, "          {node}");
    }
    config.push_str("        ]\n        links = [\n");
    for link in &graph.links {
        let _ = writeln!(config, "          {link}");
    }
    let _ = write!(
        config,
        "        ]\n        inputs = [ {} {} ]\n        outputs = [ {} {} ]\n      }}\n      audio.channels = 2\n      audio.position = [ FL FR ]\n      capture.props = {capture}\n      playback.props = {playback}\n    }}\n  }}\n]\n",
        quote(&first[0]),
        quote(&first[1]),
        quote(&last.outputs[0]),
        quote(&last.outputs[1]),
    );
    Rendered { config, skipped }
}

fn default_description(direction: Direction) -> &'static str {
    match direction {
        Direction::Output => "Effects (Output)",
        Direction::Input => "Effects (Input)",
    }
}

/// The names of a plugin's audio input or output ports, in port order.
fn audio_ports(info: &PluginInfo, inputs: bool) -> Vec<String> {
    if inputs { info.input_ports.clone() } else { info.output_ports.clone() }
}

/// The capture and playback properties of the filter-chain stream.
fn properties(direction: Direction, target: &Target) -> (String, String) {
    let device = target
        .device
        .as_deref()
        .map(|device| format!(" target.object = {}", quote(device)))
        .unwrap_or_default();
    match direction {
        Direction::Output => (
            format!(
                "{{ node.name = {} media.class = Audio/Sink audio.position = [ FL FR ] }}",
                quote(OUTPUT_NODE)
            ),
            format!(
                "{{ node.name = {} node.passive = true audio.position = [ FL FR ]{device} }}",
                quote(&format!("{OUTPUT_NODE}.out"))
            ),
        ),
        Direction::Input => (
            format!(
                "{{ node.name = {} node.passive = true audio.position = [ FL FR ]{device} }}",
                quote(&format!("{INPUT_NODE}.in"))
            ),
            format!(
                "{{ node.name = {} media.class = Audio/Source audio.position = [ FL FR ] }}",
                quote(INPUT_NODE)
            ),
        ),
    }
}

/// The `node.name` of the virtual device a direction's chain creates.
pub fn node_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Output => OUTPUT_NODE,
        Direction::Input => INPUT_NODE,
    }
}

/// A control changed while the chain runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Live {
    /// A plugin's control by name.
    Control { slot: String, name: String, value: f64 },
    /// One field of an equalizer band (`frequency`, `gain` or `q`).
    Band { slot: String, index: usize, field: BandField, value: f64 },
    /// The equalizer's preamp, in dB.
    Preamp { slot: String, db: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandField {
    Frequency,
    Gain,
    Q,
}

/// The node parameters (`node:Control`, value) that set `change` in a running
/// chain, or none when the chain has no node for it (a plugin that is off, a
/// disabled band, an equalizer without a preamp stage).
pub fn live_params(preset: &Preset, catalog: &Catalog, change: &Live) -> Vec<(String, f64)> {
    match change {
        Live::Preamp { slot, db } => match preset.slot(slot) {
            Some(plugin) if plugin.enabled && plugin.preamp.unwrap_or(0.0) != 0.0 => ["l", "r"]
                .iter()
                .map(|side| (format!("{slot}_pre_{side}:Gain 1"), linear(*db)))
                .collect(),
            _ => Vec::new(),
        },
        Live::Band { slot, index, field, value } => {
            let Some(plugin) = preset.slot(slot).filter(|plugin| plugin.enabled) else {
                return Vec::new();
            };
            let Some(band) = plugin.bands.get(*index).filter(|band| band.enabled) else {
                return Vec::new();
            };
            let control = match field {
                BandField::Frequency => "Freq",
                BandField::Q => "Q",
                BandField::Gain if band.kind.has_gain() => "Gain",
                BandField::Gain => return Vec::new(),
            };
            ["l", "r"]
                .iter()
                .map(|side| (format!("{slot}_b{index}_{side}:{control}"), *value))
                .collect()
        }
        Live::Control { slot, name, value } => {
            let Some(plugin) = preset.slot(slot).filter(|plugin| plugin.enabled) else {
                return Vec::new();
            };
            let Some(info) = usable(plugin, catalog) else {
                return Vec::new();
            };
            let (port, value) = match plugin.kind.as_str() {
                GAIN => ("Gain 1".to_string(), linear(*value)),
                DELAY => ("Delay (s)".to_string(), value / 1000.0),
                CONVOLVER | EQUALIZER => return Vec::new(),
                _ => (name.clone(), *value),
            };
            if info.library.is_some() && info.control(name).is_none() {
                return Vec::new();
            }
            if info.inputs == 2 && info.library.is_some() {
                vec![(format!("{slot}:{port}"), value)]
            } else {
                ["l", "r"]
                    .iter()
                    .map(|side| (format!("{slot}_{side}:{port}"), value))
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{Band, Direction};

    fn catalog_with_dsp() -> Catalog {
        let mut catalog = Catalog::builtin();
        let control = |name: &str, min, max, default| crate::catalog::Control {
            name: name.to_string(),
            min,
            max,
            default,
            log: false,
            integer: false,
            toggle: false,
            unit: crate::catalog::Unit::of(name),
        };
        catalog.plugins.push(PluginInfo {
            id: "ladspa:bingux_dsp:bingux_compressor".into(),
            name: "Bingux Compressor".into(),
            maker: "Bingux".into(),
            category: crate::catalog::Category::Dynamics,
            library: Some("/usr/lib64/ladspa/bingux_dsp.so".into()),
            label: "bingux_compressor".into(),
            inputs: 2,
            outputs: 2,
            input_ports: vec!["Input L".into(), "Input R".into()],
            output_ports: vec!["Output L".into(), "Output R".into()],
            controls: vec![control("Threshold", -60.0, 0.0, -18.0), control("Meter slot", 0.0, 63.0, 0.0)],
            meters: vec!["Gain reduction".into()],
        });
        catalog
    }

    #[test]
    fn a_stereo_ladspa_plugin_is_one_node_with_its_controls_and_meter_slot() {
        let preset = Preset::new("Night", Direction::Output)
            .with(PluginSlot::ladspa("comp-1", "bingux_dsp", "bingux_compressor").control("Threshold", -30.0));
        let catalog = catalog_with_dsp();
        let rendered = render(&preset, &catalog, &Target::default());
        assert!(rendered.config.contains("type = ladspa name = \"comp-1\" label = bingux_compressor plugin = \"/usr/lib64/ladspa/bingux_dsp.so\""));
        assert!(rendered.config.contains("\"Threshold\" = -30.0"));
        assert!(rendered.config.contains("\"Meter slot\" = 1.0"));
        assert!(rendered.config.contains("inputs = [ \"comp-1:Input L\" \"comp-1:Input R\" ]"));
        let live = live_params(&preset, &catalog, &Live::Control { slot: "comp-1".into(), name: "Threshold".into(), value: -20.0 });
        assert_eq!(live, vec![("comp-1:Threshold".to_string(), -20.0)]);
    }

    #[test]
    fn an_empty_chain_is_a_pass_through() {
        let preset = Preset::new("Flat", Direction::Output);
        let rendered = render(&preset, &Catalog::builtin(), &Target::default());
        assert!(rendered.config.contains("label = copy"));
        assert!(rendered.config.contains("bingux_effects_output"));
        assert!(rendered.skipped.is_empty());
    }

    #[test]
    fn each_enabled_band_is_a_biquad_pair_and_a_disabled_one_is_left_out() {
        let mut off = Band::peaking(500.0, 2.0, 1.0);
        off.enabled = false;
        let preset = Preset::new("EQ", Direction::Output).with(PluginSlot::equalizer(
            "eq-1",
            -3.0,
            vec![Band::peaking(1000.0, 6.0, 1.0), off],
        ));
        let rendered = render(&preset, &Catalog::builtin(), &Target::default());
        assert!(rendered.config.contains("name = \"eq-1_b0_l\" label = bq_peaking"));
        assert!(rendered.config.contains("name = \"eq-1_b0_r\""));
        assert!(!rendered.config.contains("eq-1_b1"));
        assert!(rendered.config.contains("eq-1_pre_l"));
        // preamp -> band, left and right.
        assert!(rendered.config.contains("\"eq-1_pre_l:Out\" input = \"eq-1_b0_l:In\""));
    }

    #[test]
    fn the_input_chain_records_the_device_and_offers_a_source() {
        let preset = Preset::new("Mic", Direction::Input);
        let target = Target {
            device: Some("alsa_input.usb-mic".into()),
            description: None,
        };
        let rendered = render(&preset, &Catalog::builtin(), &target);
        assert!(rendered.config.contains("media.class = Audio/Source"));
        assert!(rendered.config.contains("target.object = \"alsa_input.usb-mic\""));
    }

    #[test]
    fn missing_plugins_are_skipped_with_a_reason() {
        let preset = Preset::new("x", Direction::Output).with(PluginSlot::new("a-1", "ladspa:nothing:here"));
        let rendered = render(&preset, &Catalog::builtin(), &Target::default());
        assert_eq!(rendered.skipped.len(), 1);
        assert!(rendered.skipped[0].reason.contains("not installed"));
    }

    #[test]
    fn live_changes_name_the_nodes_the_graph_has() {
        let preset = Preset::new("EQ", Direction::Output).with(PluginSlot::equalizer(
            "eq-1",
            -3.0,
            vec![Band::peaking(1000.0, 6.0, 1.0)],
        ));
        let catalog = Catalog::builtin();
        let params = live_params(
            &preset,
            &catalog,
            &Live::Band { slot: "eq-1".into(), index: 0, field: BandField::Gain, value: 3.0 },
        );
        assert_eq!(params[0].0, "eq-1_b0_l:Gain");
        assert_eq!(params.len(), 2);
        let preamp = live_params(&preset, &catalog, &Live::Preamp { slot: "eq-1".into(), db: -6.0 });
        assert_eq!(preamp[0].0, "eq-1_pre_l:Gain 1");
        let _ = catalog_with_dsp();
    }
}
