//! Reading LV2 plugin metadata through `liblilv`.
//!
//! The engine never runs an LV2 plugin itself: PipeWire's `filter-chain` does
//! (the `pipewire-module-filter-chain-lv2` package on Fedora, part of
//! `pipewire` on Arch). What the engine needs from the plugin is its
//! description: the audio ports, and every control port with its range,
//! default, scale and the choices of an enumeration, which is what makes a
//! control page for a plugin with no hand-written description.
//!
//! `lilv` is the reference LV2 library. Its development files are not needed:
//! it is loaded with `dlopen` as the shared library PipeWire's LV2 loader
//! already requires. When it cannot be loaded the catalog has no LV2 plugins,
//! which is also true of a machine where PipeWire has no LV2 loader.

use std::ffi::{CStr, CString, c_char, c_float, c_void};
use std::ptr::null;

use crate::catalog::{AudioPort, Backend, Choice, Control, PluginInfo, Unit};

const LV2: &str = "http://lv2plug.in/ns/lv2core#";
const PORT_PROPS: &str = "http://lv2plug.in/ns/ext/port-props#";
const UNITS: &str = "http://lv2plug.in/ns/extensions/units#";

type Node = c_void;

/// The `lilv` entry points the scanner calls, resolved once.
struct Lilv {
    handle: *mut c_void,
    world_new: unsafe extern "C" fn() -> *mut c_void,
    world_load_all: unsafe extern "C" fn(*mut c_void),
    world_free: unsafe extern "C" fn(*mut c_void),
    world_get_all_plugins: unsafe extern "C" fn(*mut c_void) -> *const c_void,
    world_get:
        unsafe extern "C" fn(*mut c_void, *const Node, *const Node, *const Node) -> *mut Node,
    new_uri: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut Node,
    node_free: unsafe extern "C" fn(*mut Node),
    node_as_string: unsafe extern "C" fn(*const Node) -> *const c_char,
    node_as_uri: unsafe extern "C" fn(*const Node) -> *const c_char,
    node_as_float: unsafe extern "C" fn(*const Node) -> c_float,
    node_is_uri: unsafe extern "C" fn(*const Node) -> bool,
    plugins_get_by_uri: unsafe extern "C" fn(*const c_void, *const Node) -> *const c_void,
    plugin_get_name: unsafe extern "C" fn(*const c_void) -> *mut Node,
    plugin_get_author_name: unsafe extern "C" fn(*const c_void) -> *mut Node,
    plugin_get_num_ports: unsafe extern "C" fn(*const c_void) -> u32,
    plugin_get_port_by_index: unsafe extern "C" fn(*const c_void, u32) -> *const c_void,
    port_get_symbol: unsafe extern "C" fn(*const c_void, *const c_void) -> *const Node,
    port_get_name: unsafe extern "C" fn(*const c_void, *const c_void) -> *mut Node,
    port_is_a: unsafe extern "C" fn(*const c_void, *const c_void, *const Node) -> bool,
    port_has_property: unsafe extern "C" fn(*const c_void, *const c_void, *const Node) -> bool,
    port_get_range: unsafe extern "C" fn(
        *const c_void,
        *const c_void,
        *mut *mut Node,
        *mut *mut Node,
        *mut *mut Node,
    ),
    port_get_value: unsafe extern "C" fn(*const c_void, *const c_void, *const Node) -> *mut c_void,
    port_get_scale_points: unsafe extern "C" fn(*const c_void, *const c_void) -> *mut c_void,
    scale_points_begin: unsafe extern "C" fn(*const c_void) -> *mut c_void,
    scale_points_is_end: unsafe extern "C" fn(*const c_void, *mut c_void) -> bool,
    scale_points_next: unsafe extern "C" fn(*const c_void, *mut c_void) -> *mut c_void,
    scale_points_get: unsafe extern "C" fn(*const c_void, *mut c_void) -> *const c_void,
    scale_points_free: unsafe extern "C" fn(*mut c_void),
    scale_point_get_label: unsafe extern "C" fn(*const c_void) -> *const Node,
    scale_point_get_value: unsafe extern "C" fn(*const c_void) -> *const Node,
    nodes_get_first: unsafe extern "C" fn(*const c_void) -> *const Node,
    nodes_free: unsafe extern "C" fn(*mut c_void),
}

impl Drop for Lilv {
    fn drop(&mut self) {
        // SAFETY: the handle came from `dlopen` and nothing from it outlives `Lilv`.
        unsafe { libc::dlclose(self.handle) };
    }
}

impl Lilv {
    fn open() -> Option<Self> {
        // SAFETY: plain `dlopen`/`dlsym`; every symbol is cast to the signature
        // it has in lilv.h (lilv 0.24 and later, ABI-stable since 0.x).
        unsafe {
            let mut handle = std::ptr::null_mut();
            for name in [c"liblilv-0.so.0", c"liblilv-0.so"] {
                handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if !handle.is_null() {
                    break;
                }
            }
            if handle.is_null() {
                return None;
            }
            macro_rules! symbol {
                ($name:literal) => {{
                    let pointer = libc::dlsym(handle, concat!($name, "\0").as_ptr().cast());
                    if pointer.is_null() {
                        libc::dlclose(handle);
                        return None;
                    }
                    std::mem::transmute(pointer)
                }};
            }
            Some(Self {
                handle,
                world_new: symbol!("lilv_world_new"),
                world_load_all: symbol!("lilv_world_load_all"),
                world_free: symbol!("lilv_world_free"),
                world_get_all_plugins: symbol!("lilv_world_get_all_plugins"),
                world_get: symbol!("lilv_world_get"),
                new_uri: symbol!("lilv_new_uri"),
                node_free: symbol!("lilv_node_free"),
                node_as_string: symbol!("lilv_node_as_string"),
                node_as_uri: symbol!("lilv_node_as_uri"),
                node_as_float: symbol!("lilv_node_as_float"),
                node_is_uri: symbol!("lilv_node_is_uri"),
                plugins_get_by_uri: symbol!("lilv_plugins_get_by_uri"),
                plugin_get_name: symbol!("lilv_plugin_get_name"),
                plugin_get_author_name: symbol!("lilv_plugin_get_author_name"),
                plugin_get_num_ports: symbol!("lilv_plugin_get_num_ports"),
                plugin_get_port_by_index: symbol!("lilv_plugin_get_port_by_index"),
                port_get_symbol: symbol!("lilv_port_get_symbol"),
                port_get_name: symbol!("lilv_port_get_name"),
                port_is_a: symbol!("lilv_port_is_a"),
                port_has_property: symbol!("lilv_port_has_property"),
                port_get_range: symbol!("lilv_port_get_range"),
                port_get_value: symbol!("lilv_port_get_value"),
                port_get_scale_points: symbol!("lilv_port_get_scale_points"),
                scale_points_begin: symbol!("lilv_scale_points_begin"),
                scale_points_is_end: symbol!("lilv_scale_points_is_end"),
                scale_points_next: symbol!("lilv_scale_points_next"),
                scale_points_get: symbol!("lilv_scale_points_get"),
                scale_points_free: symbol!("lilv_scale_points_free"),
                scale_point_get_label: symbol!("lilv_scale_point_get_label"),
                scale_point_get_value: symbol!("lilv_scale_point_get_value"),
                nodes_get_first: symbol!("lilv_nodes_get_first"),
                nodes_free: symbol!("lilv_nodes_free"),
            })
        }
    }
}

/// Controls that only steer a plugin's own graphs and analysers: nobody needs
/// them in a settings page.
fn gui_only(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "visibility",
        "overlay",
        "graph",
        "hue",
        "zoom",
        "inspect",
        "fft reactivity",
        "shift gain",
        "show ",
        "quantize",
        "mix overlay",
    ]
    .iter()
    .any(|word| name.contains(word))
}

/// An owned `LilvNode`.
struct Owned<'a> {
    lilv: &'a Lilv,
    node: *mut Node,
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        if !self.node.is_null() {
            // SAFETY: the node was returned to the caller to free.
            unsafe { (self.lilv.node_free)(self.node) };
        }
    }
}

/// The classes and properties the scan asks about, as nodes.
struct Terms<'a> {
    input: Owned<'a>,
    output: Owned<'a>,
    control: Owned<'a>,
    audio: Owned<'a>,
    toggled: Owned<'a>,
    integer: Owned<'a>,
    enumeration: Owned<'a>,
    logarithmic: Owned<'a>,
    not_on_gui: Owned<'a>,
    optional: Owned<'a>,
    side_chain: Owned<'a>,
    sample_rate: Owned<'a>,
    unit: Owned<'a>,
    unit_symbol: Owned<'a>,
}

/// An LV2 scan: loads every bundle once, then answers for plugins by URI.
pub struct Scan {
    lilv: Lilv,
    world: *mut c_void,
}

impl Drop for Scan {
    fn drop(&mut self) {
        // SAFETY: the world was made by `world_new` and is freed once.
        unsafe { (self.lilv.world_free)(self.world) };
    }
}

fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        // SAFETY: lilv returns NUL-terminated strings that live as long as their node.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Scan {
    /// Load every LV2 bundle on the search path (`LV2_PATH`, else the system
    /// directories), or `None` when `lilv` is not installed.
    pub fn new() -> Option<Self> {
        let lilv = Lilv::open()?;
        // SAFETY: a fresh world, loaded once.
        let world = unsafe { (lilv.world_new)() };
        if world.is_null() {
            return None;
        }
        unsafe { (lilv.world_load_all)(world) };
        Some(Self { lilv, world })
    }

    fn uri(&self, uri: &str) -> Owned<'_> {
        let c = CString::new(uri).unwrap_or_default();
        // SAFETY: `world` is live and `c` is NUL-terminated.
        Owned {
            lilv: &self.lilv,
            node: unsafe { (self.lilv.new_uri)(self.world, c.as_ptr()) },
        }
    }

    fn terms(&self) -> Terms<'_> {
        Terms {
            input: self.uri(&format!("{LV2}InputPort")),
            output: self.uri(&format!("{LV2}OutputPort")),
            control: self.uri(&format!("{LV2}ControlPort")),
            audio: self.uri(&format!("{LV2}AudioPort")),
            toggled: self.uri(&format!("{LV2}toggled")),
            integer: self.uri(&format!("{LV2}integer")),
            enumeration: self.uri(&format!("{LV2}enumeration")),
            logarithmic: self.uri(&format!("{PORT_PROPS}logarithmic")),
            not_on_gui: self.uri(&format!("{PORT_PROPS}notOnGUI")),
            optional: self.uri(&format!("{LV2}connectionOptional")),
            side_chain: self.uri(&format!("{LV2}isSideChain")),
            sample_rate: self.uri(&format!("{LV2}sampleRate")),
            unit: self.uri(&format!("{UNITS}unit")),
            unit_symbol: self.uri(&format!("{UNITS}symbol")),
        }
    }

    /// What the plugin at `uri` is, or `None` when it is not installed.
    pub fn plugin(&self, uri: &str) -> Option<PluginInfo> {
        let lilv = &self.lilv;
        let terms = self.terms();
        let wanted = self.uri(uri);
        // SAFETY: every pointer below is returned by lilv for this live world and
        // used before the node or collection it belongs to is freed.
        unsafe {
            let plugins = (lilv.world_get_all_plugins)(self.world);
            let plugin = (lilv.plugins_get_by_uri)(plugins, wanted.node);
            if plugin.is_null() {
                return None;
            }
            let own = |node: *mut Node| Owned { lilv, node };
            let name = text((lilv.node_as_string)(
                own((lilv.plugin_get_name)(plugin)).node,
            ));
            let maker_node = own((lilv.plugin_get_author_name)(plugin));
            let maker = if maker_node.node.is_null() {
                String::new()
            } else {
                text((lilv.node_as_string)(maker_node.node))
            };
            let mut info = PluginInfo {
                id: uri.to_string(),
                name,
                maker,
                backend: Backend::Lv2,
                library: None,
                label: String::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                controls: Vec::new(),
                meters: Vec::new(),
                latency: None,
            };
            for index in 0..(lilv.plugin_get_num_ports)(plugin) {
                let port = (lilv.plugin_get_port_by_index)(plugin, index);
                let symbol = text((lilv.node_as_string)((lilv.port_get_symbol)(plugin, port)));
                let port_name = {
                    let node = own((lilv.port_get_name)(plugin, port));
                    text((lilv.node_as_string)(node.node))
                };
                let is = |class: &Owned| (lilv.port_is_a)(plugin, port, class.node);
                let has = |property: &Owned| (lilv.port_has_property)(plugin, port, property.node);
                if is(&terms.audio) {
                    let audio = AudioPort {
                        symbol,
                        name: port_name,
                        optional: has(&terms.optional),
                        side_chain: has(&terms.side_chain),
                    };
                    if is(&terms.input) {
                        info.inputs.push(audio);
                    } else if is(&terms.output) {
                        info.outputs.push(audio);
                    }
                    continue;
                }
                if !is(&terms.control) {
                    continue;
                }
                let (mut default, mut min, mut max) = (
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (lilv.port_get_range)(plugin, port, &mut default, &mut min, &mut max);
                let (default, min, max) = (own(default), own(min), own(max));
                let float = |node: &Owned, fallback: f64| {
                    if node.node.is_null() {
                        fallback
                    } else {
                        f64::from((lilv.node_as_float)(node.node))
                    }
                };
                let toggle = has(&terms.toggled);
                let integer = has(&terms.integer) || toggle;
                let (mut low, mut high) = (float(&min, 0.0), float(&max, 1.0));
                let scale = if has(&terms.sample_rate) {
                    48_000.0
                } else {
                    1.0
                };
                low *= scale;
                high *= scale;
                if toggle {
                    (low, high) = (0.0, 1.0);
                }
                if high <= low {
                    high = low + 1.0;
                }
                let default = float(&default, low).clamp(low, high);
                let unit = self.unit_of(plugin, port, &terms);
                let mut choices = Vec::new();
                if has(&terms.enumeration) {
                    let points = (lilv.port_get_scale_points)(plugin, port);
                    if !points.is_null() {
                        let mut iter = (lilv.scale_points_begin)(points);
                        while !(lilv.scale_points_is_end)(points, iter) {
                            let point = (lilv.scale_points_get)(points, iter);
                            choices.push(Choice {
                                value: f64::from((lilv.node_as_float)((lilv
                                    .scale_point_get_value)(
                                    point
                                ))),
                                label: text((lilv.node_as_string)((lilv.scale_point_get_label)(
                                    point,
                                ))),
                            });
                            iter = (lilv.scale_points_next)(points, iter);
                        }
                        (lilv.scale_points_free)(points);
                    }
                    choices.sort_by(|a, b| a.value.total_cmp(&b.value));
                }
                let hidden = has(&terms.not_on_gui) || gui_only(&port_name);
                let control = Control {
                    symbol,
                    name: port_name,
                    min: low,
                    max: high,
                    default,
                    log: has(&terms.logarithmic) && low > 0.0,
                    integer,
                    toggle,
                    choices,
                    unit,
                    hidden,
                };
                if is(&terms.output) {
                    info.meters.push(control);
                } else {
                    info.controls.push(control);
                }
            }
            info.latency = info
                .meters
                .iter()
                .find(|m| m.symbol.contains("latency"))
                .map(|m| m.symbol.clone());
            Some(info)
        }
    }

    /// The unit a control port declares (`units:unit`), if it names one.
    fn unit_of(&self, plugin: *const c_void, port: *const c_void, terms: &Terms<'_>) -> Unit {
        let lilv = &self.lilv;
        // SAFETY: as in `plugin`; the node list is freed before returning.
        unsafe {
            let values = (lilv.port_get_value)(plugin, port, terms.unit.node);
            if values.is_null() {
                return Unit::None;
            }
            let first = (lilv.nodes_get_first)(values);
            let mut unit = Unit::None;
            if !first.is_null() {
                if (lilv.node_is_uri)(first) {
                    unit = Unit::from_symbol(
                        text((lilv.node_as_uri)(first))
                            .rsplit(['#', '/'])
                            .next()
                            .unwrap_or(""),
                    );
                } else {
                    let symbol =
                        (lilv.world_get)(self.world, first, terms.unit_symbol.node, null());
                    if !symbol.is_null() {
                        unit = Unit::from_symbol(&text((lilv.node_as_string)(symbol)));
                        (lilv.node_free)(symbol);
                    }
                }
            }
            (lilv.nodes_free)(values);
            unit
        }
    }
}
