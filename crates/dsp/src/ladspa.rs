//! The LADSPA face of the library: descriptors for a host, and the glue that
//! hands a host's buffers to a [`Dsp`]. This is the only unsafe code here.

use std::ffi::{CString, c_char, c_int, c_ulong, c_void};
use std::sync::LazyLock;

use crate::dsp::{Dsp, Kind, LOG, PortSpec, Spec};
use crate::plugins;

const PORT_INPUT: c_int = 0x1;
const PORT_OUTPUT: c_int = 0x2;
const PORT_CONTROL: c_int = 0x4;
const PORT_AUDIO: c_int = 0x8;

const HINT_BOUNDED_BELOW: c_int = 0x1;
const HINT_BOUNDED_ABOVE: c_int = 0x2;
const HINT_DEFAULT_MINIMUM: c_int = 0x40;
const HINT_DEFAULT_LOW: c_int = 0x80;
const HINT_DEFAULT_MIDDLE: c_int = 0xC0;
const HINT_DEFAULT_HIGH: c_int = 0x100;
const HINT_DEFAULT_MAXIMUM: c_int = 0x140;
const HINT_DEFAULT_0: c_int = 0x200;
const HINT_DEFAULT_1: c_int = 0x240;

const PROPERTY_HARD_RT_CAPABLE: c_int = 0x4;

#[repr(C)]
pub struct RangeHint {
    descriptor: c_int,
    lower: f32,
    upper: f32,
}

type Handle = *mut c_void;

#[repr(C)]
pub struct Descriptor {
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
    instantiate: Option<unsafe extern "C" fn(*const Descriptor, c_ulong) -> Handle>,
    connect_port: Option<unsafe extern "C" fn(Handle, c_ulong, *mut f32)>,
    activate: Option<unsafe extern "C" fn(Handle)>,
    run: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    run_adding: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    set_run_adding_gain: Option<unsafe extern "C" fn(Handle, f32)>,
    deactivate: Option<unsafe extern "C" fn(Handle)>,
    cleanup: Option<unsafe extern "C" fn(Handle)>,
}

/// What one plugin's ports are, kept beside its descriptor.
struct Layout {
    make: fn(f32) -> Box<dyn Dsp>,
    ports: Vec<PortSpec>,
}

/// A descriptor and the strings and arrays it points into.
struct Holder {
    descriptor: Descriptor,
    _label: CString,
    _name: CString,
    _maker: CString,
    _copyright: CString,
    _names: Vec<CString>,
    _name_pointers: Vec<*const c_char>,
    _kinds: Vec<c_int>,
    _hints: Vec<RangeHint>,
    _layout: Box<Layout>,
}

struct Table(Vec<Box<Holder>>);

// The table is built once and never changes; the pointers inside point into
// the table itself.
unsafe impl Sync for Table {}
unsafe impl Send for Table {}

fn table() -> &'static Table {
    static TABLE: LazyLock<Table> =
        LazyLock::new(|| Table(plugins().into_iter().map(holder).collect()));
    &TABLE
}

fn cstring(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}

fn holder(spec: Spec) -> Box<Holder> {
    let names: Vec<CString> = spec.ports.iter().map(|port| cstring(port.name)).collect();
    let name_pointers: Vec<*const c_char> = names.iter().map(|name| name.as_ptr()).collect();
    let kinds: Vec<c_int> = spec
        .ports
        .iter()
        .map(|port| match port.kind {
            Kind::AudioIn => PORT_INPUT | PORT_AUDIO,
            Kind::AudioOut => PORT_OUTPUT | PORT_AUDIO,
            Kind::ControlIn => PORT_INPUT | PORT_CONTROL,
            Kind::ControlOut => PORT_OUTPUT | PORT_CONTROL,
        })
        .collect();
    let hints: Vec<RangeHint> = spec.ports.iter().map(range_hint).collect();
    let mut layout = Box::new(Layout {
        make: spec.make,
        ports: spec.ports,
    });
    let label = cstring(spec.label);
    let name = cstring(spec.name);
    let maker = cstring("Bingux");
    let copyright = cstring("GPL-3.0-or-later");
    let descriptor = Descriptor {
        unique_id: c_ulong::from(spec.id),
        label: label.as_ptr(),
        properties: PROPERTY_HARD_RT_CAPABLE,
        name: name.as_ptr(),
        maker: maker.as_ptr(),
        copyright: copyright.as_ptr(),
        port_count: layout.ports.len() as c_ulong,
        port_descriptors: kinds.as_ptr(),
        port_names: name_pointers.as_ptr(),
        port_range_hints: hints.as_ptr(),
        implementation_data: (&mut *layout as *mut Layout).cast(),
        instantiate: Some(instantiate),
        connect_port: Some(connect_port),
        activate: None,
        run: Some(run),
        run_adding: None,
        set_run_adding_gain: None,
        deactivate: None,
        cleanup: Some(cleanup),
    };
    Box::new(Holder {
        descriptor,
        _label: label,
        _name: name,
        _maker: maker,
        _copyright: copyright,
        _names: names,
        _name_pointers: name_pointers,
        _kinds: kinds,
        _hints: hints,
        _layout: layout,
    })
}

fn range_hint(port: &PortSpec) -> RangeHint {
    if matches!(port.kind, Kind::AudioIn | Kind::AudioOut) {
        return RangeHint {
            descriptor: 0,
            lower: 0.0,
            upper: 0.0,
        };
    }
    let mut descriptor = HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | (port.hint & !LOG) as c_int;
    if port.hint & LOG != 0 {
        descriptor |= 0x10;
    }
    // LADSPA can only name a default as a position in the range, or 0 or 1.
    descriptor |= if port.default == 0.0 {
        HINT_DEFAULT_0
    } else if port.default == 1.0 {
        HINT_DEFAULT_1
    } else if port.default <= port.min {
        HINT_DEFAULT_MINIMUM
    } else if port.default >= port.max {
        HINT_DEFAULT_MAXIMUM
    } else {
        let at = if port.hint & LOG != 0 {
            (port.default / port.min).ln() / (port.max / port.min).ln()
        } else {
            (port.default - port.min) / (port.max - port.min)
        };
        match at {
            at if at < 0.25 => HINT_DEFAULT_LOW,
            at if at < 0.75 => HINT_DEFAULT_MIDDLE,
            _ => HINT_DEFAULT_HIGH,
        }
    };
    RangeHint {
        descriptor,
        lower: port.min,
        upper: port.max,
    }
}

/// One running plugin.
struct Instance {
    dsp: Box<dyn Dsp>,
    layout: *const Layout,
    pointers: Vec<*mut f32>,
    controls: Vec<f32>,
    meters: Vec<f32>,
    copies: [Vec<f32>; 2],
}

unsafe extern "C" fn instantiate(descriptor: *const Descriptor, rate: c_ulong) -> Handle {
    // SAFETY: the host passes back a descriptor this library made.
    let layout = unsafe { &*((*descriptor).implementation_data as *const Layout) };
    let controls = layout
        .ports
        .iter()
        .filter(|port| port.kind == Kind::ControlIn)
        .map(|port| port.default)
        .collect();
    let meters = vec![
        0.0;
        layout
            .ports
            .iter()
            .filter(|port| port.kind == Kind::ControlOut)
            .count()
    ];
    let instance = Box::new(Instance {
        dsp: (layout.make)(rate as f32),
        layout,
        pointers: vec![std::ptr::null_mut(); layout.ports.len()],
        controls,
        meters,
        copies: [Vec::new(), Vec::new()],
    });
    Box::into_raw(instance).cast()
}

unsafe extern "C" fn connect_port(handle: Handle, port: c_ulong, data: *mut f32) {
    // SAFETY: `handle` came from `instantiate`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    if let Some(slot) = instance.pointers.get_mut(port as usize) {
        *slot = data;
    }
}

unsafe extern "C" fn run(handle: Handle, samples: c_ulong) {
    // SAFETY: `handle` came from `instantiate`, and the host connected every
    // port to a buffer of `samples` floats (or a single float for a control).
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    let layout = unsafe { &*instance.layout };
    let samples = samples as usize;
    let mut control = 0;
    let mut slot = 0;
    for (index, port) in layout.ports.iter().enumerate() {
        let pointer = instance.pointers[index];
        match port.kind {
            Kind::ControlIn => {
                let value = if pointer.is_null() {
                    port.default
                } else {
                    unsafe { *pointer }
                };
                instance.controls[control] = if value.is_finite() {
                    value.clamp(port.min, port.max)
                } else {
                    port.default
                };
                if port.name == "Meter slot" {
                    slot = instance.controls[control] as usize;
                }
                control += 1;
            }
            _ => {}
        }
    }
    let [in_l, in_r, out_l, out_r] = [0, 1, 2, 3].map(|index| instance.pointers[index]);
    if [in_l, in_r, out_l, out_r].iter().any(|pointer| pointer.is_null()) {
        return;
    }
    // Copy the inputs, so an output may share a buffer with an input.
    for (copy, pointer) in instance.copies.iter_mut().zip([in_l, in_r]) {
        copy.clear();
        copy.extend_from_slice(unsafe { std::slice::from_raw_parts(pointer, samples) });
    }
    let outputs = unsafe {
        [
            std::slice::from_raw_parts_mut(out_l, samples),
            std::slice::from_raw_parts_mut(out_r, samples),
        ]
    };
    let [copy_l, copy_r] = &instance.copies;
    instance.dsp.process(
        [copy_l.as_slice(), copy_r.as_slice()],
        outputs,
        &instance.controls,
        &mut instance.meters,
    );
    let mut meter_at = 0;
    for (index, port) in layout.ports.iter().enumerate() {
        if port.kind == Kind::ControlOut {
            let pointer = instance.pointers[index];
            if !pointer.is_null() {
                unsafe { *pointer = instance.meters[meter_at] };
            }
            meter_at += 1;
        }
    }
    crate::meters::publish(slot, &instance.meters);
}

unsafe extern "C" fn cleanup(handle: Handle) {
    // SAFETY: `handle` came from `instantiate` and is not used again.
    drop(unsafe { Box::from_raw(handle.cast::<Instance>()) });
}

/// The entry point a LADSPA host looks up: the descriptor at `index`, or null
/// past the last one.
#[unsafe(no_mangle)]
pub extern "C" fn ladspa_descriptor(index: c_ulong) -> *const Descriptor {
    table()
        .0
        .get(index as usize)
        .map_or(std::ptr::null(), |holder| &holder.descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_sees_every_plugin_with_its_ports() {
        let mut count = 0;
        loop {
            let descriptor = ladspa_descriptor(count);
            if descriptor.is_null() {
                break;
            }
            let descriptor = unsafe { &*descriptor };
            assert!(descriptor.port_count >= 4);
            count += 1;
        }
        assert_eq!(count as usize, plugins().len());
        assert!(count >= 12);
    }

    #[test]
    fn a_plugin_runs_through_the_host_interface() {
        let descriptor = unsafe { &*ladspa_descriptor(0) };
        let handle = unsafe { (descriptor.instantiate.unwrap())(descriptor, 48_000) };
        let input = vec![0.5f32; 256];
        let mut output_l = vec![0.0f32; 256];
        let mut output_r = vec![0.0f32; 256];
        let mut controls: Vec<f32> = plugins()[0]
            .ports
            .iter()
            .map(|port| port.default)
            .collect();
        unsafe {
            let connect = descriptor.connect_port.unwrap();
            connect(handle, 0, input.as_ptr() as *mut f32);
            connect(handle, 1, input.as_ptr() as *mut f32);
            connect(handle, 2, output_l.as_mut_ptr());
            connect(handle, 3, output_r.as_mut_ptr());
            for index in 4..descriptor.port_count as usize {
                connect(handle, index as c_ulong, &mut controls[index]);
            }
            (descriptor.run.unwrap())(handle, 256);
            (descriptor.cleanup.unwrap())(handle);
        }
        assert!(output_l[255] > 0.0 && output_l[255] <= 0.5);
    }
}
