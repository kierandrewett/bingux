//! What the installed plugins are, read from their own metadata.
//!
//! [`effects`](crate::effects) says which plugins Bingux offers; this module
//! finds each one on the system and describes its ports: the audio ports and
//! every control with its range, default, scale, unit and the choices of an
//! enumeration. A page for a plugin is drawn from that, so a plugin needs no
//! hand-written description. The scan is cached by the modification time of
//! the plugin bundles, so a normal start reads one small JSON file.

use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::effects::{self, Source};

/// The sample rate that LADSPA bounds given "per sample rate" are scaled by.
const SAMPLE_RATE: f64 = 48_000.0;

/// What a value measures, for its label and its editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    None,
    /// A value in decibels.
    Db,
    /// A linear gain (1.0 is unity) that people read in decibels: LSP ports
    /// declare it as `G`.
    Gain,
    Hz,
    Ms,
    Seconds,
    Percent,
    Semitone,
    Octave,
    Degree,
}

impl Unit {
    /// The unit an LV2 `units:` term (`db`, `hz`, `G`, ...) names.
    pub fn from_symbol(symbol: &str) -> Self {
        match symbol {
            "db" | "dB" => Unit::Db,
            "G" => Unit::Gain,
            "hz" | "Hz" => Unit::Hz,
            "ms" => Unit::Ms,
            "s" => Unit::Seconds,
            "pc" | "%" => Unit::Percent,
            "semitone" | "st" => Unit::Semitone,
            "oct" => Unit::Octave,
            "degree" => Unit::Degree,
            _ => Unit::None,
        }
    }

    /// The decibels a stored value is shown as: itself for a `Db` control, the
    /// linear gain's level for a `Gain` one.
    pub fn shown(self, stored: f64) -> f64 {
        match self {
            Unit::Gain => 20.0 * stored.max(1e-6).log10(),
            _ => stored,
        }
    }

    /// The stored value for what is shown (the inverse of [`Unit::shown`]).
    pub fn stored(self, shown: f64) -> f64 {
        match self {
            Unit::Gain => 10f64.powf(shown / 20.0),
            _ => shown,
        }
    }

    /// `value` (as stored) with its unit: "-18 dB", "6 kHz", "100 ms".
    pub fn format(self, value: f64) -> String {
        let number = |value: f64| {
            let text = if value.abs() >= 100.0 {
                format!("{value:.0}")
            } else if value.abs() >= 10.0 {
                format!("{value:.1}")
            } else {
                format!("{value:.2}")
            };
            if text.contains('.') {
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                text
            }
        };
        match self {
            Unit::None => number(value),
            Unit::Db => format!("{} dB", number(value)),
            Unit::Gain if value <= 1e-4 => "-inf dB".to_string(),
            Unit::Gain => format!("{} dB", number(self.shown(value))),
            Unit::Hz if value >= 1000.0 => format!("{} kHz", number(value / 1000.0)),
            Unit::Hz => format!("{} Hz", number(value)),
            Unit::Ms if value >= 1000.0 => format!("{} s", number(value / 1000.0)),
            Unit::Ms => format!("{} ms", number(value)),
            Unit::Seconds => format!("{} s", number(value)),
            Unit::Percent => format!("{}%", number(value)),
            Unit::Semitone => format!("{} st", number(value)),
            Unit::Octave => format!("{} oct", number(value)),
            Unit::Degree => format!("{}°", number(value)),
        }
    }
}

/// One choice of an enumerated control.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub value: f64,
    pub label: String,
}

/// One setting of a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Control {
    /// The port's own name (an LV2 symbol, a LADSPA port name): the key a
    /// preset stores the value under.
    pub symbol: String,
    /// The name to show.
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// The value is best moved on a logarithmic scale.
    pub log: bool,
    pub integer: bool,
    /// An on or off switch.
    pub toggle: bool,
    /// The named values of an enumeration, in order; empty for any other control.
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub unit: Unit,
    /// The plugin's own interface leaves it out (`notOnGUI`).
    #[serde(default)]
    pub hidden: bool,
}

impl Control {
    /// The value at `position` from 0 to 1 along the control's scale.
    pub fn value_at(&self, position: f64) -> f64 {
        let position = position.clamp(0.0, 1.0);
        let value = if self.log && self.min > 0.0 {
            self.min * (self.max / self.min).powf(position)
        } else {
            self.min + (self.max - self.min) * position
        };
        if self.integer { value.round() } else { value }
    }

    /// Where `value` sits from 0 to 1 along the control's scale.
    pub fn position_of(&self, value: f64) -> f64 {
        let value = value.clamp(self.min, self.max);
        if self.max <= self.min {
            0.0
        } else if self.log && self.min > 0.0 {
            (value / self.min).ln() / (self.max / self.min).ln()
        } else {
            (value - self.min) / (self.max - self.min)
        }
    }

    /// The value as a person reads it: a choice's label, "On" or "Off", or
    /// the number with its unit.
    pub fn display(&self, value: f64) -> String {
        if let Some(choice) = self
            .choices
            .iter()
            .find(|choice| (choice.value - value).abs() < 1e-6)
        {
            return choice.label.clone();
        }
        if self.toggle {
            return if value >= 0.5 { "On" } else { "Off" }.to_string();
        }
        if self.integer && self.unit == Unit::None {
            return format!("{value:.0}");
        }
        self.unit.format(value)
    }
}

/// An audio port of a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioPort {
    pub symbol: String,
    pub name: String,
    /// The plugin works with the port unconnected.
    #[serde(default)]
    pub optional: bool,
    /// A side-chain input.
    #[serde(default)]
    pub side_chain: bool,
}

/// How a plugin is hosted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    Lv2,
    Ladspa,
    /// One of PipeWire's own filters.
    Builtin,
}

/// One plugin found on the system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// The LV2 URI, `ladspa:<library>:<label>` or `builtin:<label>`.
    pub id: String,
    pub name: String,
    pub maker: String,
    pub backend: Backend,
    /// The library file, for a LADSPA plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<PathBuf>,
    /// The LADSPA label or PipeWire filter label.
    #[serde(default)]
    pub label: String,
    pub inputs: Vec<AudioPort>,
    pub outputs: Vec<AudioPort>,
    pub controls: Vec<Control>,
    /// Output control ports: readings such as gain reduction and latency.
    #[serde(default)]
    pub meters: Vec<Control>,
    /// The output port that reports the plugin's latency in samples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency: Option<String>,
}

impl PluginInfo {
    pub fn control(&self, symbol: &str) -> Option<&Control> {
        self.controls
            .iter()
            .find(|control| control.symbol == symbol)
    }

    /// The audio inputs a chain feeds: the side-chain and optional ones left out.
    pub fn main_inputs(&self) -> Vec<&AudioPort> {
        self.inputs
            .iter()
            .filter(|port| !port.side_chain && !port.optional)
            .collect()
    }

    pub fn main_outputs(&self) -> Vec<&AudioPort> {
        self.outputs.iter().filter(|port| !port.optional).collect()
    }

    /// The controls to show: all but the ones the plugin's own interface hides.
    pub fn visible_controls(&self) -> impl Iterator<Item = &Control> {
        self.controls.iter().filter(|control| !control.hidden)
    }
}

/// The plugins found for the effects Bingux offers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub plugins: Vec<PluginInfo>,
}

impl Catalog {
    pub fn find(&self, id: &str) -> Option<&PluginInfo> {
        self.plugins.iter().find(|plugin| plugin.id == id)
    }

    /// The plugin that implements `effect`, when it is installed.
    pub fn plugin_of(&self, effect: &effects::Effect) -> Option<&PluginInfo> {
        self.find(&effect.source.plugin_id())
    }

    /// Scan for the plugins of every effect, using the cache while the plugin
    /// bundles have not changed, and write the cache when they did.
    pub fn load() -> Self {
        let cache_path = crate::store::cache_dir().join("plugins.json");
        let fingerprint = fingerprint();
        let cached: Option<Cache> = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        if let Some(cache) = cached.filter(|cache| cache.fingerprint == fingerprint) {
            return cache.catalog;
        }
        let catalog = Self::scan();
        let cache = Cache {
            fingerprint,
            catalog,
        };
        let _ = std::fs::create_dir_all(cache_path.parent().unwrap_or(Path::new(".")));
        if let Ok(text) = serde_json::to_string(&cache) {
            let _ = std::fs::write(&cache_path, text);
        }
        cache.catalog
    }

    /// Scan with no cache.
    pub fn scan() -> Self {
        let mut catalog = Self::default();
        let lv2 = crate::lv2::Scan::new();
        let ladspa = ladspa_paths();
        for effect in effects::ALL {
            let found = match effect.source {
                Source::Lv2(uri) => lv2.as_ref().and_then(|scan| scan.plugin(uri)),
                Source::Ladspa { library, label } => read_ladspa(&ladspa, library, label),
                Source::Convolver => Some(convolver()),
            };
            if let Some(mut plugin) = found
                && catalog.find(&plugin.id).is_none()
            {
                for control in &mut plugin.controls {
                    if effect.linear_gains().contains(&control.symbol.as_str()) {
                        control.unit = Unit::Gain;
                    }
                }
                catalog.plugins.push(plugin);
            }
        }
        catalog
    }
}

/// PipeWire's convolver: one gain, and the impulse response file chosen in the preset.
fn convolver() -> PluginInfo {
    PluginInfo {
        id: effects::Source::Convolver.plugin_id(),
        name: "Convolver".to_string(),
        maker: "PipeWire".to_string(),
        backend: Backend::Builtin,
        library: None,
        label: "convolver".to_string(),
        inputs: vec![AudioPort {
            symbol: "In".into(),
            name: "Input".into(),
            optional: false,
            side_chain: false,
        }],
        outputs: vec![AudioPort {
            symbol: "Out".into(),
            name: "Output".into(),
            optional: false,
            side_chain: false,
        }],
        controls: vec![Control {
            symbol: "gain".to_string(),
            name: "Gain".to_string(),
            min: 0.0,
            max: 4.0,
            default: 1.0,
            log: false,
            integer: false,
            toggle: false,
            choices: Vec::new(),
            unit: Unit::Gain,
            hidden: false,
        }],
        meters: Vec::new(),
        latency: None,
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    fingerprint: String,
    catalog: Catalog,
}

/// A line that changes when a plugin bundle or library is added, removed or
/// replaced, or when this scan's format changes.
fn fingerprint() -> String {
    let mut parts = vec![
        format!("v{}", env!("CARGO_PKG_VERSION")),
        "format-3".to_string(),
    ];
    for directory in lv2_paths().into_iter().chain(ladspa_paths()) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                let manifest = if path.is_dir() {
                    path.join("manifest.ttl")
                } else {
                    path.clone()
                };
                let modified = std::fs::metadata(&manifest)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |duration| duration.as_secs());
                format!("{}:{modified}", path.display())
            })
            .collect();
        names.sort();
        parts.extend(names);
    }
    parts.join("\n")
}

fn split(value: std::ffi::OsString) -> Vec<PathBuf> {
    std::env::split_paths(&value).collect()
}

/// The directories LV2 bundles are looked for in: `LV2_PATH` when set, else
/// the usual user and system directories (what `lilv` itself searches).
pub fn lv2_paths() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("LV2_PATH").filter(|value| !value.is_empty()) {
        return split(value);
    }
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(Path::new(&home).join(".lv2"));
    }
    for dir in [
        "/usr/local/lib64/lv2",
        "/usr/local/lib/lv2",
        "/usr/lib64/lv2",
        "/usr/lib/lv2",
        "/usr/lib/x86_64-linux-gnu/lv2",
    ] {
        paths.push(PathBuf::from(dir));
    }
    paths
}

/// The directories LADSPA plugins are looked for in: `LADSPA_PATH` when set,
/// otherwise the usual system and user directories.
pub fn ladspa_paths() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("LADSPA_PATH").filter(|value| !value.is_empty()) {
        return split(value);
    }
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(Path::new(&home).join(".ladspa"));
    }
    for dir in [
        "/usr/local/lib64/ladspa",
        "/usr/local/lib/ladspa",
        "/usr/lib64/ladspa",
        "/usr/lib/ladspa",
        "/usr/lib/x86_64-linux-gnu/ladspa",
    ] {
        paths.push(PathBuf::from(dir));
    }
    paths
}

/// The plugin `label` of the LADSPA library `library` (a file name without `.so`).
fn read_ladspa(directories: &[PathBuf], library: &str, label: &str) -> Option<PluginInfo> {
    for directory in directories {
        let path = directory.join(format!("{library}.so"));
        if path.is_file()
            && let Some(found) = read_library(&path, library)
                .into_iter()
                .find(|plugin| plugin.label == label)
        {
            return Some(found);
        }
    }
    None
}

#[repr(C)]
struct RangeHint {
    descriptor: c_int,
    lower: f32,
    upper: f32,
}

#[repr(C)]
struct Descriptor {
    unique_id: c_ulong,
    label: *const c_char,
    properties: c_int,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: c_ulong,
    port_descriptors: *const c_int,
    port_names: *const *const c_char,
    port_range_hints: *const RangeHint,
    implementation_data: *mut c_void,
    // The function pointers follow; they are never read here.
}

const PORT_INPUT: c_int = 0x1;
const PORT_OUTPUT: c_int = 0x2;
const PORT_CONTROL: c_int = 0x4;
const PORT_AUDIO: c_int = 0x8;
const BOUNDED_BELOW: c_int = 0x1;
const BOUNDED_ABOVE: c_int = 0x2;
const TOGGLED: c_int = 0x4;
const SAMPLE_RATE_HINT: c_int = 0x8;
const LOGARITHMIC: c_int = 0x10;
const INTEGER: c_int = 0x20;
const DEFAULT_MASK: c_int = 0x3C0;

/// The plugins in one LADSPA library, or none when it cannot be loaded.
fn read_library(path: &Path, stem: &str) -> Vec<PluginInfo> {
    let Ok(c_path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
        return Vec::new();
    };
    let mut plugins = Vec::new();
    // SAFETY: a LADSPA library exports `ladspa_descriptor`, whose results are
    // plain data owned by the library, valid while it stays loaded. The
    // library is closed only after everything is copied out.
    unsafe {
        let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if handle.is_null() {
            return plugins;
        }
        let symbol = libc::dlsym(handle, c"ladspa_descriptor".as_ptr());
        if !symbol.is_null() {
            let entry: unsafe extern "C" fn(c_ulong) -> *const Descriptor =
                std::mem::transmute(symbol);
            for index in 0.. {
                let descriptor = entry(index);
                if descriptor.is_null() {
                    break;
                }
                plugins.extend(read_descriptor(&*descriptor, path, stem));
            }
        }
        libc::dlclose(handle);
    }
    plugins
}

unsafe fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

/// SAFETY: `descriptor` must be a valid LADSPA descriptor.
unsafe fn read_descriptor(descriptor: &Descriptor, path: &Path, stem: &str) -> Option<PluginInfo> {
    let label = unsafe { text(descriptor.label) };
    if label.is_empty() {
        return None;
    }
    let name = unsafe { text(descriptor.name) };
    let (mut inputs, mut outputs) = (Vec::new(), Vec::new());
    let mut controls = Vec::new();
    let mut meters = Vec::new();
    for port in 0..descriptor.port_count as usize {
        let kind = unsafe { *descriptor.port_descriptors.add(port) };
        let port_name = unsafe { text(*descriptor.port_names.add(port)) };
        let hint = unsafe { &*descriptor.port_range_hints.add(port) };
        if kind & PORT_AUDIO != 0 {
            let audio = AudioPort {
                symbol: port_name.clone(),
                name: port_name,
                optional: false,
                side_chain: false,
            };
            if kind & PORT_INPUT != 0 {
                inputs.push(audio);
            } else if kind & PORT_OUTPUT != 0 {
                outputs.push(audio);
            }
        } else if kind & PORT_CONTROL != 0 {
            if kind & PORT_OUTPUT != 0 {
                meters.push(read_control(port_name, hint));
            } else {
                controls.push(read_control(port_name, hint));
            }
        }
    }
    Some(PluginInfo {
        id: format!("ladspa:{stem}:{label}"),
        name: if name.is_empty() { label.clone() } else { name },
        maker: unsafe { text(descriptor.maker) },
        backend: Backend::Ladspa,
        library: Some(path.to_path_buf()),
        label,
        inputs,
        outputs,
        controls,
        meters,
        latency: None,
    })
}

/// A control's range, default and scale from its LADSPA range hint.
fn read_control(name: String, hint: &RangeHint) -> Control {
    let flags = hint.descriptor;
    let scale = if flags & SAMPLE_RATE_HINT != 0 {
        SAMPLE_RATE
    } else {
        1.0
    };
    let below = flags & BOUNDED_BELOW != 0;
    let above = flags & BOUNDED_ABOVE != 0;
    let log = flags & LOGARITHMIC != 0;
    let toggle = flags & TOGGLED != 0;
    let integer = flags & INTEGER != 0;
    let (mut min, mut max) = match (below, above) {
        (true, true) => (f64::from(hint.lower) * scale, f64::from(hint.upper) * scale),
        (true, false) => (
            f64::from(hint.lower) * scale,
            f64::from(hint.lower) * scale + 100.0,
        ),
        (false, true) => (
            f64::from(hint.upper) * scale - 100.0,
            f64::from(hint.upper) * scale,
        ),
        (false, false) => (0.0, 1.0),
    };
    if toggle {
        (min, max) = (0.0, 1.0);
    }
    if max <= min {
        max = min + 1.0;
    }
    let between = |weight: f64| {
        if log && min > 0.0 {
            (min.ln() * (1.0 - weight) + max.ln() * weight).exp()
        } else {
            min * (1.0 - weight) + max * weight
        }
    };
    let default = match flags & DEFAULT_MASK {
        0x40 => min,
        0x80 => between(0.25),
        0xC0 => between(0.5),
        0x100 => between(0.75),
        0x140 => max,
        0x200 => 0.0,
        0x240 => 1.0,
        0x280 => 100.0,
        0x2C0 => 440.0,
        _ if min <= 0.0 && max >= 0.0 => 0.0,
        _ => min,
    }
    .clamp(min, max);
    let unit = unit_of_name(&name);
    Control {
        symbol: name.clone(),
        name,
        min,
        max,
        default,
        log: log && min > 0.0,
        integer,
        toggle,
        choices: Vec::new(),
        unit,
        hidden: false,
    }
}

/// The unit of a LADSPA control, guessed from its name: LADSPA has none.
fn unit_of_name(name: &str) -> Unit {
    let name = name.to_lowercase();
    if name.contains("semitone") {
        Unit::Semitone
    } else if name.contains("cent") {
        Unit::None
    } else if name.contains("(hz)") || name.contains("freq") {
        Unit::Hz
    } else if name.contains("(ms)") {
        Unit::Ms
    } else if name.contains("(db)") || name.contains("gain") {
        Unit::Db
    } else {
        Unit::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_come_from_lv2_terms_and_format_for_people() {
        assert_eq!(Unit::from_symbol("hz"), Unit::Hz);
        assert_eq!(Unit::from_symbol("G"), Unit::Gain);
        assert_eq!(Unit::from_symbol("pc"), Unit::Percent);
        assert_eq!(Unit::Hz.format(6000.0), "6 kHz");
        assert_eq!(Unit::Db.format(-18.0), "-18 dB");
        assert_eq!(Unit::Ms.format(150.0), "150 ms");
        assert_eq!(Unit::Gain.format(1.0), "0 dB");
        assert_eq!(Unit::Gain.format(0.5), "-6.02 dB");
        assert!((Unit::Gain.stored(Unit::Gain.shown(0.25)) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_log_control_maps_positions_to_values_and_back() {
        let control = Control {
            symbol: "freq".into(),
            name: "Frequency".into(),
            min: 20.0,
            max: 20_000.0,
            default: 1000.0,
            log: true,
            integer: false,
            toggle: false,
            choices: Vec::new(),
            unit: Unit::Hz,
            hidden: false,
        };
        assert!((control.value_at(0.5) - 632.5).abs() < 1.0);
        assert!((control.position_of(control.value_at(0.3)) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn controls_display_choices_toggles_and_units() {
        let mut control = Control {
            symbol: "mode".into(),
            name: "Mode".into(),
            min: 0.0,
            max: 2.0,
            default: 0.0,
            log: false,
            integer: true,
            toggle: false,
            choices: vec![
                Choice {
                    value: 0.0,
                    label: "Down".into(),
                },
                Choice {
                    value: 1.0,
                    label: "Up".into(),
                },
            ],
            unit: Unit::None,
            hidden: false,
        };
        assert_eq!(control.display(1.0), "Up");
        assert_eq!(control.display(2.0), "2");
        control.choices.clear();
        control.toggle = true;
        assert_eq!(control.display(1.0), "On");
    }

    #[test]
    fn ladspa_hints_give_bounds_and_defaults() {
        let hint = RangeHint {
            descriptor: BOUNDED_BELOW | BOUNDED_ABOVE | LOGARITHMIC | 0xC0,
            lower: 10.0,
            upper: 1000.0,
        };
        let control = read_control("Release".into(), &hint);
        assert_eq!((control.min, control.max), (10.0, 1000.0));
        assert!((control.default - 100.0).abs() < 1e-6);
        assert!(control.log);
        let toggled = read_control(
            "Mono".into(),
            &RangeHint {
                descriptor: TOGGLED,
                lower: 0.0,
                upper: 0.0,
            },
        );
        assert!(toggled.toggle);
        assert_eq!((toggled.min, toggled.max), (0.0, 1.0));
    }
}
