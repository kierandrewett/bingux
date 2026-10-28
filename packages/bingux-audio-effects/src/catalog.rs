//! The plugins the engine can put in a chain, and how to find them.
//!
//! A plugin's controls come from its own port metadata (LADSPA range hints:
//! bounds, default, logarithmic, integer, toggled), so every plugin found on
//! the system has controls with no hand-written description. Libraries are
//! scanned in the background and the result is cached by file modification
//! time, so a normal start reads one small JSON file.

use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::preset::{CONVOLVER, DELAY, EQUALIZER, GAIN};

/// The sample rate that LADSPA bounds given "per sample rate" are scaled by.
const SAMPLE_RATE: f64 = 48_000.0;

/// What a value measures, for its label and its editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    None,
    Db,
    Hz,
    Ms,
    Percent,
    Ratio,
}

impl Unit {
    /// The unit of a control, guessed from its name, since LADSPA has no units.
    pub fn of(name: &str) -> Self {
        let name = name.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
        if name == "feed" || has(&["(db)", "threshold", "makeup", "knee", "range", "level", "reduction", "target", "preamp", "gain"]) {
            Unit::Db
        } else if has(&["(hz)", "freq", "cutoff", "tone"]) {
            Unit::Hz
        } else if has(&["(ms)", "attack", "release", "hold", "delay", "lookahead"]) {
            Unit::Ms
        } else if has(&["ratio"]) {
            Unit::Ratio
        } else if has(&["(%)", "mix", "width", "balance", "amount", "damping", "feedback", "room", "wet", "dry"]) {
            Unit::Percent
        } else {
            Unit::None
        }
    }

    /// `value` with its unit: "-18 dB", "6 kHz", "4:1".
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
            Unit::Hz if value >= 1000.0 => format!("{} kHz", number(value / 1000.0)),
            Unit::Hz => format!("{} Hz", number(value)),
            Unit::Ms if value >= 1000.0 => format!("{} s", number(value / 1000.0)),
            Unit::Ms => format!("{} ms", number(value)),
            Unit::Percent => format!("{}%", number(value)),
            Unit::Ratio => format!("{}:1", number(value)),
        }
    }
}

/// One setting of a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// The value is best moved on a logarithmic scale.
    pub log: bool,
    pub integer: bool,
    /// An on or off switch.
    pub toggle: bool,
    pub unit: Unit,
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
}

/// Where a plugin is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Category {
    Dynamics,
    Equalizer,
    Filter,
    Space,
    Enhance,
    Stereo,
    Voice,
    Utility,
    Other,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Dynamics,
        Category::Equalizer,
        Category::Filter,
        Category::Space,
        Category::Enhance,
        Category::Stereo,
        Category::Voice,
        Category::Utility,
        Category::Other,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Dynamics => "Dynamics",
            Category::Equalizer => "Equalizers",
            Category::Filter => "Filters",
            Category::Space => "Space and delay",
            Category::Enhance => "Enhancement",
            Category::Stereo => "Stereo",
            Category::Voice => "Voice",
            Category::Utility => "Utility",
            Category::Other => "Other",
        }
    }

    /// The category a plugin falls in, from its label and name.
    pub fn of(label: &str, name: &str) -> Self {
        let text = format!("{label} {name}").to_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
        if has(&["compressor", "limiter", "gate", "expander", "de-esser", "deesser", "auto gain", "autogain", "dynamics", "maximizer", "leveler", "normaliz"]) {
            Category::Dynamics
        } else if has(&["equalizer", "equaliser", " eq", "_eq", "graphic", "parametric"]) {
            Category::Equalizer
        } else if has(&["denoise", "noise", "rnnoise", "speech", "voice", "echo cancel", "dereverb"]) {
            Category::Voice
        } else if has(&["reverb", "echo", "delay", "chorus", "flanger", "phaser", "room"]) {
            Category::Space
        } else if has(&["stereo", "crossfeed", "width", "panner", "pan", "balance", "mid/side", "mid-side", "ms "]) {
            Category::Stereo
        } else if has(&["exciter", "enhancer", "crystal", "saturat", "distort", "overdrive", "bass", "loudness", "harmonic", "tube"]) {
            Category::Enhance
        } else if has(&["filter", "lowpass", "highpass", "low-pass", "high-pass", "bandpass", "crossover", "shelf", "notch"]) {
            Category::Filter
        } else if has(&["gain", "amplifier", "mixer", "invert", "dc ", "dcblock", "meter", "analy", "spectrum", "sine", "noise generator"]) {
            Category::Utility
        } else {
            Category::Other
        }
    }
}

/// One plugin that can be put in a chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// What a preset's `type` says: `equalizer`, `gain`, `delay`, `convolver` or
    /// `ladspa:<library>:<label>`.
    pub id: String,
    pub name: String,
    pub maker: String,
    pub category: Category,
    /// The library file, for a LADSPA plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<PathBuf>,
    /// The LADSPA label.
    #[serde(default)]
    pub label: String,
    /// Audio inputs and outputs. The engine runs a plugin with one of each on
    /// both channels, and one with two of each as a stereo plugin.
    pub inputs: usize,
    pub outputs: usize,
    /// The names of the audio ports, in port order.
    #[serde(default)]
    pub input_ports: Vec<String>,
    #[serde(default)]
    pub output_ports: Vec<String>,
    pub controls: Vec<Control>,
    /// Output control ports: readings such as gain reduction.
    #[serde(default)]
    pub meters: Vec<String>,
}

impl PluginInfo {
    /// Whether the engine can run this plugin, or why not.
    pub fn unusable(&self) -> Option<String> {
        match (self.inputs, self.outputs) {
            (1, 1) | (2, 2) => None,
            (0, _) => Some("It makes sound and takes none in.".to_string()),
            (inputs, _) if inputs > 2 => Some("It needs a side-chain input, which chains do not have yet.".to_string()),
            _ => Some("Its audio ports do not match one or two channels.".to_string()),
        }
    }

    pub fn control(&self, name: &str) -> Option<&Control> {
        self.controls.iter().find(|control| control.name == name)
    }

    /// Whether the plugin publishes its meters in the meter file (see
    /// `bingux_dsp::meters`).
    pub fn publishes_meters(&self) -> bool {
        self.control("Meter slot").is_some()
    }

    /// The controls to show a person: all but the engine's own "Meter slot".
    pub fn visible_controls(&self) -> impl Iterator<Item = &Control> {
        self.controls.iter().filter(|control| control.name != "Meter slot")
    }

    /// The plugin's library file name without `.so` or `lib`, such as `bingux_dsp`.
    pub fn library_stem(&self) -> Option<&str> {
        self.id.strip_prefix("ladspa:")?.split(':').next()
    }
}

/// Every plugin the engine knows of.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub plugins: Vec<PluginInfo>,
}

fn builtin(id: &str, name: &str, category: Category, controls: Vec<Control>) -> PluginInfo {
    PluginInfo {
        id: id.to_string(),
        name: name.to_string(),
        maker: "PipeWire".to_string(),
        category,
        library: None,
        label: String::new(),
        inputs: 2,
        outputs: 2,
        input_ports: Vec::new(),
        output_ports: Vec::new(),
        controls,
        meters: Vec::new(),
    }
}

fn control(name: &str, min: f64, max: f64, default: f64, unit: Unit) -> Control {
    Control {
        name: name.to_string(),
        min,
        max,
        default,
        log: false,
        integer: false,
        toggle: false,
        unit,
    }
}

impl Catalog {
    /// The plugins PipeWire provides itself.
    pub fn builtin() -> Self {
        Self {
            plugins: vec![
                builtin(EQUALIZER, "Parametric equalizer", Category::Equalizer, Vec::new()),
                builtin(
                    GAIN,
                    "Gain",
                    Category::Utility,
                    vec![control("Gain", -40.0, 40.0, 0.0, Unit::Db)],
                ),
                builtin(
                    DELAY,
                    "Delay",
                    Category::Space,
                    vec![control("Delay", 0.0, 1000.0, 0.0, Unit::Ms)],
                ),
                builtin(
                    CONVOLVER,
                    "Convolver",
                    Category::Space,
                    vec![control("Gain", -40.0, 20.0, 0.0, Unit::Db)],
                ),
            ],
        }
    }

    pub fn find(&self, id: &str) -> Option<&PluginInfo> {
        self.plugins.iter().find(|plugin| plugin.id == id)
    }

    /// The plugins of `category`, by name.
    pub fn in_category(&self, category: Category) -> Vec<&PluginInfo> {
        let mut found: Vec<&PluginInfo> = self
            .plugins
            .iter()
            .filter(|plugin| plugin.category == category)
            .collect();
        found.sort_by_key(|plugin| plugin.name.to_lowercase());
        found
    }

    /// Scan the LADSPA directories, using the cache where a library has not
    /// changed, and save the cache again when anything did.
    pub fn load() -> Self {
        let cache_path = crate::store::cache_dir().join("plugins.json");
        let cached: Vec<Library> = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let (libraries, changed) = scan_with_cache(&ladspa_paths(), cached);
        if changed {
            let _ = std::fs::create_dir_all(cache_path.parent().unwrap_or(Path::new(".")));
            if let Ok(text) = serde_json::to_string(&libraries) {
                let _ = std::fs::write(&cache_path, text);
            }
        }
        Self::from_libraries(libraries)
    }

    /// Scan `directories` with no cache.
    pub fn scan(directories: &[PathBuf]) -> Self {
        Self::from_libraries(scan_with_cache(directories, Vec::new()).0)
    }

    fn from_libraries(libraries: Vec<Library>) -> Self {
        let mut catalog = Self::builtin();
        for library in libraries {
            for plugin in library.plugins {
                if catalog.find(&plugin.id).is_none() {
                    catalog.plugins.push(plugin);
                }
            }
        }
        catalog
    }
}

/// A scanned library and when it was scanned.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Library {
    path: PathBuf,
    modified: u64,
    plugins: Vec<PluginInfo>,
}

/// The directories LADSPA plugins are looked for in: `LADSPA_PATH` when set,
/// otherwise the usual system and user directories, and always the Bingux
/// directories (`BINGUX_LADSPA_PATH`, the user's data dir, and `lib/ladspa`
/// beside the installed program).
pub fn ladspa_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let split = |value: std::ffi::OsString| std::env::split_paths(&value).collect::<Vec<_>>();
    if let Some(value) = std::env::var_os("LADSPA_PATH") {
        paths.extend(split(value));
    } else {
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
    }
    if let Some(value) = std::env::var_os("BINGUX_LADSPA_PATH") {
        paths.extend(split(value));
    }
    paths.push(crate::store::data_dir().join("ladspa"));
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        paths.push(prefix.join("lib/ladspa"));
        paths.push(prefix.join("lib64/ladspa"));
    }
    let mut unique: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

fn scan_with_cache(directories: &[PathBuf], cached: Vec<Library>) -> (Vec<Library>, bool) {
    let mut libraries: Vec<Library> = Vec::new();
    let mut changed = false;
    let mut files: Vec<PathBuf> = Vec::new();
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "so"))
            .collect();
        found.sort();
        files.extend(found);
    }
    for path in files {
        let modified = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        if libraries.iter().any(|library| library.path == path) {
            continue;
        }
        match cached.iter().find(|library| library.path == path && library.modified == modified) {
            Some(library) => libraries.push(library.clone()),
            None => {
                changed = true;
                libraries.push(Library {
                    plugins: read_library(&path),
                    path,
                    modified,
                });
            }
        }
    }
    if cached.len() != libraries.len() {
        changed = true;
    }
    (libraries, changed)
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
fn read_library(path: &Path) -> Vec<PluginInfo> {
    let Ok(c_path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
        return Vec::new();
    };
    let stem = library_stem(path);
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
            let entry: unsafe extern "C" fn(c_ulong) -> *const Descriptor = std::mem::transmute(symbol);
            for index in 0.. {
                let descriptor = entry(index);
                if descriptor.is_null() {
                    break;
                }
                plugins.extend(read_descriptor(&*descriptor, path, &stem));
            }
        }
        libc::dlclose(handle);
    }
    plugins
}

fn library_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("plugin");
    stem.strip_prefix("lib").unwrap_or(stem).to_string()
}

unsafe fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
    }
}

/// SAFETY: `descriptor` must be a valid LADSPA descriptor.
unsafe fn read_descriptor(descriptor: &Descriptor, path: &Path, stem: &str) -> Option<PluginInfo> {
    let label = unsafe { text(descriptor.label) };
    if label.is_empty() {
        return None;
    }
    let name = unsafe { text(descriptor.name) };
    let (mut input_ports, mut output_ports) = (Vec::new(), Vec::new());
    let mut controls = Vec::new();
    let mut meters = Vec::new();
    for port in 0..descriptor.port_count as usize {
        let kind = unsafe { *descriptor.port_descriptors.add(port) };
        let port_name = unsafe { text(*descriptor.port_names.add(port)) };
        let hint = unsafe { &*descriptor.port_range_hints.add(port) };
        if kind & PORT_AUDIO != 0 {
            if kind & PORT_INPUT != 0 {
                input_ports.push(port_name);
            } else if kind & PORT_OUTPUT != 0 {
                output_ports.push(port_name);
            }
        } else if kind & PORT_CONTROL != 0 {
            if kind & PORT_OUTPUT != 0 {
                meters.push(port_name);
            } else {
                controls.push(read_control(port_name, hint));
            }
        }
    }
    let maker = unsafe { text(descriptor.maker) };
    Some(PluginInfo {
        id: format!("ladspa:{stem}:{label}"),
        category: Category::of(&label, &name),
        name: if name.is_empty() { label.clone() } else { name },
        maker,
        library: Some(path.to_path_buf()),
        label,
        inputs: input_ports.len(),
        outputs: output_ports.len(),
        input_ports,
        output_ports,
        controls,
        meters,
    })
}

/// A control's range, default and scale from its LADSPA range hint.
fn read_control(name: String, hint: &RangeHint) -> Control {
    let flags = hint.descriptor;
    let scale = if flags & SAMPLE_RATE_HINT != 0 { SAMPLE_RATE } else { 1.0 };
    let below = flags & BOUNDED_BELOW != 0;
    let above = flags & BOUNDED_ABOVE != 0;
    let log = flags & LOGARITHMIC != 0;
    let toggle = flags & TOGGLED != 0;
    let integer = flags & INTEGER != 0;
    let (mut min, mut max) = match (below, above) {
        (true, true) => (f64::from(hint.lower) * scale, f64::from(hint.upper) * scale),
        (true, false) => (f64::from(hint.lower) * scale, f64::from(hint.lower) * scale + 100.0),
        (false, true) => (f64::from(hint.upper) * scale - 100.0, f64::from(hint.upper) * scale),
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
    Control {
        unit: Unit::of(&name),
        name,
        min,
        max,
        default,
        log: log && min > 0.0,
        integer,
        toggle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_are_guessed_from_names() {
        assert_eq!(Unit::of("Threshold"), Unit::Db);
        assert_eq!(Unit::of("Frequency"), Unit::Hz);
        assert_eq!(Unit::of("Attack time (ms)"), Unit::Ms);
        assert_eq!(Unit::of("Ratio"), Unit::Ratio);
        assert_eq!(Unit::of("Mix"), Unit::Percent);
        assert_eq!(Unit::of("Drive"), Unit::None);
        assert_eq!(Unit::Hz.format(6000.0), "6 kHz");
        assert_eq!(Unit::Db.format(-18.0), "-18 dB");
        assert_eq!(Unit::Ratio.format(4.0), "4:1");
        assert_eq!(Unit::Ms.format(150.0), "150 ms");
    }

    #[test]
    fn categories_group_plugins() {
        assert_eq!(Category::of("sc_compressor_stereo", "Sidechain Compressor Stereo"), Category::Dynamics);
        assert_eq!(Category::of("para_equalizer_x16_lr", "Parametric Equalizer x16 LeftRight"), Category::Equalizer);
        assert_eq!(Category::of("bingux_reverb", "Bingux Reverb"), Category::Space);
        assert_eq!(Category::of("bingux_stereo_tools", "Bingux Stereo Tools"), Category::Stereo);
    }

    #[test]
    fn a_log_control_maps_positions_to_values_and_back() {
        let control = Control {
            name: "Frequency".into(),
            min: 20.0,
            max: 20_000.0,
            default: 1000.0,
            log: true,
            integer: false,
            toggle: false,
            unit: Unit::Hz,
        };
        assert!((control.value_at(0.5) - 632.5).abs() < 1.0);
        assert!((control.position_of(control.value_at(0.3)) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn hints_give_bounds_and_defaults() {
        let hint = RangeHint {
            descriptor: BOUNDED_BELOW | BOUNDED_ABOVE | LOGARITHMIC | 0xC0,
            lower: 10.0,
            upper: 1000.0,
        };
        let control = read_control("Release".into(), &hint);
        assert_eq!((control.min, control.max), (10.0, 1000.0));
        assert!((control.default - 100.0).abs() < 1e-6);
        assert!(control.log);
        let toggled = read_control("Mono".into(), &RangeHint { descriptor: TOGGLED, lower: 0.0, upper: 0.0 });
        assert!(toggled.toggle);
        assert_eq!((toggled.min, toggled.max), (0.0, 1.0));
    }
}
