//! LADSPA effect plugins for the Bingux sound effects.
//!
//! PipeWire's filter-chain module loads LADSPA plugins on any distribution,
//! so the effects the Settings app offers need no extra packages: this
//! library is installed as `bingux_dsp.so` in a LADSPA directory and holds the
//! plugins listed in [`plugins`]. Every plugin is stereo (two audio inputs,
//! two audio outputs) and exposes its meters, such as gain reduction, as
//! output control ports.
//!
//! The DSP in `dynamics`, `tone` and `space` is plain safe Rust over slices.
//! `ladspa` is the only unsafe part: it describes the plugins to the host and
//! hands the host's buffers to them.

mod analyser;
mod dsp;
mod dynamics;
mod ladspa;
pub mod meters;
mod multiband;
mod space;
mod tone;

pub use dsp::{Dsp, PortSpec, Spec};
pub use ladspa::ladspa_descriptor;

/// Every plugin in the library, in the order the host sees them.
pub fn plugins() -> Vec<Spec> {
    let mut all = Vec::new();
    all.extend(dynamics::plugins());
    all.extend(multiband::plugins());
    all.extend(analyser::plugins());
    all.extend(tone::plugins());
    all.extend(space::plugins());
    all
}
