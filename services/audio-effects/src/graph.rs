//! One stage of the chain as a PipeWire `filter-chain` configuration.
//!
//! Every effect runs in a `pipewire` process of its own, with one
//! `filter-chain` module: a capture stream that takes the sound in, the
//! plugin, and a playback stream that sends it out. The engine joins the
//! stages with links (see `chain`), so putting an effect in, taking it out or
//! changing the order is a matter of links, and the other effects keep
//! running with their state. A plugin that crashes takes only its own stage
//! with it.
//!
//! The two ends of the chain are stages too: the head takes the sound in
//! (a virtual sink the applications play to, or the microphone) and the tail
//! sends it on (the speakers, or a virtual microphone for the applications).
//! They are plain `copy` stages that stay up while effects come and go, so
//! the device applications see never disappears.
//!
//! A stereo plugin is one filter node; a mono plugin runs once for each
//! side. Plugin controls are set from the preset when the stage starts and
//! live afterwards (see `service::set_params`).

use std::fmt::Write;
use std::path::Path;

use crate::catalog::{Backend, PluginInfo};
use crate::effects::{Channels, Effect, Source};
use crate::preset::{Direction, PluginSlot};

/// The node name of the virtual output device (a sink applications play to).
pub const OUTPUT_NODE: &str = "bingux_effects_output";
/// The node name of the virtual input device (a source applications record from).
pub const INPUT_NODE: &str = "bingux_effects_input";

/// What a stage is in the chain.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Where the sound enters: the virtual sink (output) or the microphone (input).
    Head,
    /// Where it leaves: the speakers (output) or the virtual microphone (input).
    Tail,
    /// An effect, by the id of its slot in the preset.
    Effect(String),
}

/// The real device the chain works on.
#[derive(Clone, Debug, Default)]
pub struct Target {
    /// The `node.name` of the speakers the output chain plays to, or the
    /// microphone the input chain records.
    pub device: Option<String>,
}

/// A stage's nodes and the ports the chain links to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    /// The capture node: `bingux_fx.<direction>.<id>`, or the virtual sink's own name.
    pub capture: String,
    /// The playback node: the capture name and `.out`, or the virtual source's own name.
    pub playback: String,
}

/// The names of a stage's nodes.
pub fn names(direction: Direction, role: &Role) -> Names {
    let plain = |tail: &str| {
        let capture = format!("bingux_fx.{}.{tail}", direction.name());
        Names {
            playback: format!("{capture}.out"),
            capture,
        }
    };
    match (direction, role) {
        (Direction::Output, Role::Head) => Names {
            capture: OUTPUT_NODE.to_string(),
            playback: format!("{OUTPUT_NODE}.out"),
        },
        (Direction::Input, Role::Tail) => Names {
            capture: format!("bingux_fx.{}.tail", direction.name()),
            playback: INPUT_NODE.to_string(),
        },
        (_, Role::Head) => plain("head"),
        (_, Role::Tail) => plain("tail"),
        (_, Role::Effect(id)) => plain(id),
    }
}

/// The `node.name` of the virtual device a direction's chain creates.
pub fn node_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Output => OUTPUT_NODE,
        Direction::Input => INPUT_NODE,
    }
}

/// Whether the stage's capture node is a sink, so its ports are `playback_*`
/// rather than `input_*`.
fn capture_is_sink(direction: Direction, role: &Role) -> bool {
    matches!((direction, role), (Direction::Output, Role::Head))
}

/// The two ports sound enters a stage by, left then right, as `node:port`.
pub fn input_ports(direction: Direction, role: &Role) -> [String; 2] {
    let node = names(direction, role).capture;
    let prefix = if capture_is_sink(direction, role) {
        "playback"
    } else {
        "input"
    };
    [format!("{node}:{prefix}_FL"), format!("{node}:{prefix}_FR")]
}

/// The two ports sound leaves a stage by.
pub fn output_ports(direction: Direction, role: &Role) -> [String; 2] {
    let node = names(direction, role).playback;
    // A virtual microphone is a source, whose ports are named for capturing.
    let prefix = if matches!((direction, role), (Direction::Input, Role::Tail)) {
        "capture"
    } else {
        "output"
    };
    [format!("{node}:{prefix}_FL"), format!("{node}:{prefix}_FR")]
}

/// Why an effect has no stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
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

fn node_text(
    kind: &str,
    name: &str,
    plugin: Option<&str>,
    label: Option<&str>,
    controls: &str,
    config: &str,
) -> String {
    let mut text = format!("{{ type = {kind} name = {}", quote(name));
    if let Some(plugin) = plugin {
        let _ = write!(text, " plugin = {}", quote(plugin));
    }
    if let Some(label) = label {
        let _ = write!(text, " label = {label}");
    }
    if !config.is_empty() {
        let _ = write!(text, " config = {{ {config} }}");
    }
    if !controls.is_empty() {
        let _ = write!(text, " control = {{{controls} }}");
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
    if std::io::Read::read_exact(&mut file, &mut header).is_err()
        || &header[..4] != b"RIFF"
        || &header[8..12] != b"WAVE"
    {
        return 1;
    }
    usize::from(u16::from_le_bytes([header[22], header[23]])).max(1)
}

/// The `control = { ... }` body for `slot`: its stored values that the plugin
/// has a control for, kept inside the control's range.
fn control_values(slot: &PluginSlot, plugin: &PluginInfo) -> String {
    let mut text = String::new();
    for (symbol, value) in &slot.controls {
        let Some(control) = plugin.control(symbol) else {
            continue;
        };
        let value = if control.integer {
            value.round()
        } else {
            *value
        };
        let _ = write!(
            text,
            " {} = {}",
            quote(symbol),
            number(value.clamp(control.min, control.max))
        );
    }
    text
}

/// What `render_effect` makes: the file and the two ports of the graph.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub config: String,
}

fn config_text(
    direction: Direction,
    role: &Role,
    description: &str,
    nodes: &[String],
    inputs: &[String],
    outputs: &[String],
    capture: &str,
    playback: &str,
) -> String {
    let id = match role {
        Role::Head => "head".to_string(),
        Role::Tail => "tail".to_string(),
        Role::Effect(id) => id.clone(),
    };
    let mut config = String::new();
    let _ = write!(
        config,
        "# Written by the Bingux effects engine. Edit the preset in Settings, not this file.\n\
         context.properties = {{ core.name = bingux-effects-{direction}-{id} }}\n\
         context.spa-libs = {{\n  audio.convert.* = audioconvert/libspa-audioconvert\n  support.* = support/libspa-support\n}}\n\
         context.modules = [\n\
         \x20 {{ name = libpipewire-module-rt flags = [ ifexists nofail ] }}\n\
         \x20 {{ name = libpipewire-module-protocol-native }}\n\
         \x20 {{ name = libpipewire-module-client-node }}\n\
         \x20 {{ name = libpipewire-module-adapter }}\n\
         \x20 {{ name = libpipewire-module-filter-chain\n    args = {{\n      node.description = {}\n      media.name = {}\n      filter.graph = {{\n        nodes = [\n",
        quote(description),
        quote(description),
        direction = direction.name(),
    );
    for node in nodes {
        let _ = writeln!(config, "          {node}");
    }
    config.push_str("        ]\n");
    let _ = write!(
        config,
        "        inputs = [ {} ]\n        outputs = [ {} ]\n      }}\n      audio.channels = 2\n      audio.position = [ FL FR ]\n      capture.props = {capture}\n      playback.props = {playback}\n    }}\n  }}\n]\n",
        inputs
            .iter()
            .map(|port| quote(port))
            .collect::<Vec<_>>()
            .join(" "),
        outputs
            .iter()
            .map(|port| quote(port))
            .collect::<Vec<_>>()
            .join(" "),
    );
    config
}

/// The stream properties of the two ends of a stage.
fn stream_properties(direction: Direction, role: &Role, target: &Target) -> (String, String) {
    let device = |_: ()| {
        target
            .device
            .as_deref()
            .map(|device| format!(" target.object = {}", quote(device)))
            .unwrap_or_default()
    };
    let free = "node.autoconnect = false node.dont-reconnect = true";
    let own = names(direction, role);
    let capture = match (direction, role) {
        (Direction::Output, Role::Head) => {
            format!(
                "{{ node.name = {} media.class = Audio/Sink audio.position = [ FL FR ] }}",
                quote(&own.capture)
            )
        }
        (Direction::Input, Role::Head) => format!(
            "{{ node.name = {} node.passive = true audio.position = [ FL FR ]{} }}",
            quote(&own.capture),
            device(())
        ),
        _ => format!(
            "{{ node.name = {} media.class = Stream/Input/Audio audio.position = [ FL FR ] {free} }}",
            quote(&own.capture)
        ),
    };
    let playback = match (direction, role) {
        (Direction::Output, Role::Tail) => format!(
            "{{ node.name = {} node.passive = true audio.position = [ FL FR ]{} }}",
            quote(&own.playback),
            device(())
        ),
        (Direction::Input, Role::Tail) => {
            format!(
                "{{ node.name = {} media.class = Audio/Source audio.position = [ FL FR ] }}",
                quote(&own.playback)
            )
        }
        _ => format!(
            "{{ node.name = {} node.passive = true audio.position = [ FL FR ] {free} }}",
            quote(&own.playback)
        ),
    };
    (capture, playback)
}

fn copy_nodes() -> Vec<String> {
    ["l", "r"]
        .iter()
        .map(|side| {
            node_text(
                "builtin",
                &format!("copy_{side}"),
                None,
                Some("copy"),
                "",
                "",
            )
        })
        .collect()
}

/// The configuration of the head or the tail: a plain copy of both channels.
pub fn render_end(direction: Direction, role: &Role, target: &Target) -> Rendered {
    let description = match (direction, role) {
        (Direction::Output, Role::Head) => "Effects (Output)",
        (Direction::Input, Role::Tail) => "Effects (Input)",
        (Direction::Output, _) => "Effects (Output) tail",
        (Direction::Input, _) => "Effects (Input) head",
    };
    let (capture, playback) = stream_properties(direction, role, target);
    Rendered {
        config: config_text(
            direction,
            role,
            description,
            &copy_nodes(),
            &["copy_l:In".to_string(), "copy_r:In".to_string()],
            &["copy_l:Out".to_string(), "copy_r:Out".to_string()],
            &capture,
            &playback,
        ),
    }
}

/// The configuration of an effect's stage, or why it cannot run.
pub fn render_effect(
    direction: Direction,
    slot: &PluginSlot,
    effect: &Effect,
    plugin: Option<&PluginInfo>,
) -> Result<Rendered, Skipped> {
    let skip = |reason: &str| Skipped {
        id: slot.id.clone(),
        reason: reason.to_string(),
    };
    let Some(plugin) = plugin else {
        return Err(skip(&format!(
            "{} is not installed. It comes with {} ({}).",
            effect.name,
            effect.package.project(),
            effect.package.fedora()
        )));
    };
    let role = Role::Effect(slot.id.clone());
    let controls = control_values(slot, plugin);
    let mut nodes = Vec::new();
    let (inputs, outputs): (Vec<String>, Vec<String>);
    match (effect.source, effect.channels) {
        (Source::Convolver, _) => {
            let path = slot
                .path
                .as_deref()
                .filter(|path| Path::new(path).is_file())
                .ok_or_else(|| skip("Choose an impulse response file."))?;
            let stereo = wav_channels(path) >= 2;
            let gain = slot.value("gain", 1.0).clamp(0.0, 4.0);
            let mut ins = Vec::new();
            let mut outs = Vec::new();
            for (channel, side) in ["l", "r"].into_iter().enumerate() {
                let config = format!(
                    "filename = {} channel = {} gain = {}",
                    quote(path),
                    if stereo { channel } else { 0 },
                    number(gain)
                );
                nodes.push(node_text(
                    "builtin",
                    &format!("fx_{side}"),
                    None,
                    Some("convolver"),
                    "",
                    &config,
                ));
                ins.push(format!("fx_{side}:In"));
                outs.push(format!("fx_{side}:Out"));
            }
            (inputs, outputs) = (ins, outs);
        }
        (_, Channels::Stereo) => {
            let ports_in = plugin.main_inputs();
            let ports_out = plugin.main_outputs();
            if ports_in.len() < 2 || ports_out.len() < 2 {
                return Err(skip(
                    "The plugin does not have a left and a right audio port.",
                ));
            }
            nodes.push(plugin_node(plugin, effect, "fx", &controls));
            inputs = ports_in
                .iter()
                .take(2)
                .map(|port| format!("fx:{}", port.symbol))
                .collect();
            outputs = ports_out
                .iter()
                .take(2)
                .map(|port| format!("fx:{}", port.symbol))
                .collect();
        }
        (_, Channels::Mono) => {
            let (Some(port_in), Some(port_out)) = (
                plugin.main_inputs().first().copied(),
                plugin.main_outputs().first().copied(),
            ) else {
                return Err(skip("The plugin has no audio port."));
            };
            let (mut ins, mut outs) = (Vec::new(), Vec::new());
            for side in ["l", "r"] {
                let name = format!("fx_{side}");
                nodes.push(plugin_node(plugin, effect, &name, &controls));
                ins.push(format!("{name}:{}", port_in.symbol));
                outs.push(format!("{name}:{}", port_out.symbol));
            }
            (inputs, outputs) = (ins, outs);
        }
    }
    let (capture, playback) = stream_properties(direction, &role, &Target::default());
    Ok(Rendered {
        config: config_text(
            direction,
            &role,
            effect.name,
            &nodes,
            &inputs,
            &outputs,
            &capture,
            &playback,
        ),
    })
}

fn plugin_node(plugin: &PluginInfo, effect: &Effect, name: &str, controls: &str) -> String {
    match plugin.backend {
        Backend::Lv2 => node_text("lv2", name, Some(&plugin.id), None, controls, ""),
        Backend::Ladspa => {
            let library = match effect.source {
                Source::Ladspa { library, .. } => library,
                _ => "",
            };
            node_text(
                "ladspa",
                name,
                Some(library),
                Some(&plugin.label),
                controls,
                "",
            )
        }
        Backend::Builtin => node_text("builtin", name, None, Some(&plugin.label), controls, ""),
    }
}

/// The filter-graph node names an effect's controls live under: `fx` for a
/// stereo plugin, `fx_l` and `fx_r` for a mono one.
pub fn control_nodes(effect: &Effect) -> &'static [&'static str] {
    match (effect.source, effect.channels) {
        (Source::Convolver, _) | (_, Channels::Mono) => &["fx_l", "fx_r"],
        _ => &["fx"],
    }
}

/// The node parameters (`node:symbol` and a value) that set `symbol` to
/// `value` in a running stage.
pub fn live_params(effect: &Effect, symbol: &str, value: f64) -> Vec<(String, f64)> {
    control_nodes(effect)
        .iter()
        .map(|node| (format!("{node}:{symbol}"), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AudioPort, Control, Unit};
    use crate::effects;

    fn port(symbol: &str) -> AudioPort {
        AudioPort {
            symbol: symbol.into(),
            name: symbol.into(),
            optional: false,
            side_chain: false,
        }
    }

    fn control(symbol: &str, min: f64, max: f64, default: f64) -> Control {
        Control {
            symbol: symbol.into(),
            name: symbol.into(),
            min,
            max,
            default,
            log: false,
            integer: false,
            toggle: false,
            choices: Vec::new(),
            unit: Unit::None,
            hidden: false,
        }
    }

    fn compressor() -> (PluginInfo, &'static Effect) {
        let effect = effects::find("compressor").unwrap();
        let plugin = PluginInfo {
            id: effect.source.plugin_id(),
            name: "LSP Compressor Stereo".into(),
            maker: "LSP".into(),
            backend: Backend::Lv2,
            library: None,
            label: String::new(),
            inputs: vec![port("in_l"), port("in_r")],
            outputs: vec![port("out_l"), port("out_r")],
            controls: vec![
                control("cr", 1.0, 100.0, 4.0),
                control("al", 0.001, 1.0, 0.25),
            ],
            meters: Vec::new(),
            latency: None,
        };
        (plugin, effect)
    }

    #[test]
    fn stage_names_and_ports_follow_the_role() {
        let head = names(Direction::Output, &Role::Head);
        assert_eq!(
            (head.capture.as_str(), head.playback.as_str()),
            (OUTPUT_NODE, "bingux_effects_output.out")
        );
        assert_eq!(
            input_ports(Direction::Output, &Role::Head)[0],
            "bingux_effects_output:playback_FL"
        );
        assert_eq!(
            output_ports(Direction::Output, &Role::Head)[1],
            "bingux_effects_output.out:output_FR"
        );
        let effect = Role::Effect("compressor-1".into());
        assert_eq!(
            input_ports(Direction::Output, &effect)[0],
            "bingux_fx.output.compressor-1:input_FL"
        );
        assert_eq!(names(Direction::Input, &Role::Tail).playback, INPUT_NODE);
        assert_eq!(
            output_ports(Direction::Input, &Role::Tail)[0],
            "bingux_effects_input:capture_FL"
        );
        assert_eq!(
            input_ports(Direction::Input, &Role::Tail)[1],
            "bingux_fx.input.tail:input_FR"
        );
    }

    #[test]
    fn a_stereo_lv2_effect_is_one_node_with_its_controls_in_range() {
        let (plugin, effect) = compressor();
        let slot = PluginSlot::new("compressor-1", "compressor")
            .control("cr", 500.0)
            .control("al", 0.1)
            .control("unknown", 3.0);
        let rendered = render_effect(Direction::Output, &slot, effect, Some(&plugin)).unwrap();
        let text = &rendered.config;
        assert!(text.contains("type = lv2 name = \"fx\" plugin = \"http://lsp-plug.in/plugins/lv2/compressor_stereo\""), "{text}");
        assert!(
            text.contains("\"cr\" = 100.0"),
            "a control is clamped to its range: {text}"
        );
        assert!(text.contains("\"al\" = 0.1"));
        assert!(
            !text.contains("unknown"),
            "a control the plugin lacks is not written"
        );
        assert!(text.contains("inputs = [ \"fx:in_l\" \"fx:in_r\" ]"));
        assert!(text.contains("outputs = [ \"fx:out_l\" \"fx:out_r\" ]"));
        assert!(text.contains(
            "node.name = \"bingux_fx.output.compressor-1\" media.class = Stream/Input/Audio"
        ));
        assert!(
            text.contains("node.autoconnect = false"),
            "the engine links effects itself"
        );
    }

    #[test]
    fn a_missing_plugin_is_skipped_with_the_package_that_has_it() {
        let effect = effects::find("compressor").unwrap();
        let slot = PluginSlot::new("compressor-1", "compressor");
        let skipped = render_effect(Direction::Output, &slot, effect, None).unwrap_err();
        assert!(
            skipped.reason.contains("lsp-plugins-lv2"),
            "{}",
            skipped.reason
        );
    }

    #[test]
    fn a_mono_plugin_runs_once_for_each_side() {
        let effect = effects::find("autotune").unwrap();
        let plugin = PluginInfo {
            id: effect.source.plugin_id(),
            name: "x42-Autotune".into(),
            maker: "x42".into(),
            backend: Backend::Lv2,
            library: None,
            label: String::new(),
            inputs: vec![port("in")],
            outputs: vec![port("out")],
            controls: vec![control("tuning", 400.0, 480.0, 440.0)],
            meters: Vec::new(),
            latency: None,
        };
        let slot = PluginSlot::new("autotune-1", "autotune").control("tuning", 432.0);
        let text = render_effect(Direction::Input, &slot, effect, Some(&plugin))
            .unwrap()
            .config;
        assert!(text.contains("name = \"fx_l\"") && text.contains("name = \"fx_r\""));
        assert!(text.contains("inputs = [ \"fx_l:in\" \"fx_r:in\" ]"));
        assert_eq!(
            live_params(effect, "tuning", 432.0),
            [
                ("fx_l:tuning".to_string(), 432.0),
                ("fx_r:tuning".to_string(), 432.0)
            ]
        );
    }

    #[test]
    fn the_ends_are_copies_and_the_devices_are_targets() {
        let target = Target {
            device: Some("alsa_output.usb-dac".into()),
        };
        let head = render_end(Direction::Output, &Role::Head, &target).config;
        assert!(head.contains("media.class = Audio/Sink"));
        assert!(head.contains("label = copy"));
        assert!(
            !head.contains("target.object"),
            "the head is linked, not routed"
        );
        let tail = render_end(Direction::Output, &Role::Tail, &target).config;
        assert!(tail.contains("target.object = \"alsa_output.usb-dac\""));
        let mic = render_end(
            Direction::Input,
            &Role::Head,
            &Target {
                device: Some("alsa_input.usb-mic".into()),
            },
        )
        .config;
        assert!(mic.contains("target.object = \"alsa_input.usb-mic\""));
        let virtual_mic = render_end(Direction::Input, &Role::Tail, &Target::default()).config;
        assert!(virtual_mic.contains("media.class = Audio/Source"));
    }
}
