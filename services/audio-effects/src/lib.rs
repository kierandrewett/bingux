//! Sound effects for the Bingux desktop.
//!
//! The effects are a chain of plugins between the applications and the sound
//! device (output effects) or between a microphone and the applications
//! (input effects). The chain is a plain file, a *preset* (see `preset`);
//! the engine turns it into a PipeWire `filter-chain` configuration (see
//! `graph`) and runs it as a separate `pipewire` process (see `service`), so
//! the Settings app is only a front end: it edits preset files and tells the
//! engine to restart, or sets a control live with `pw-cli`.
//!
//! Plugins come from three places, all described by the `catalog`:
//!
//! - PipeWire's built-in filters: parametric equalizer, gain, delay, convolver.
//! - Any LADSPA plugin installed on the system, which includes the Linux
//!   Studio Plugins (`lsp-plugins-ladspa`).
//! - `bingux_dsp`, this project's own LADSPA library: compressor, limiter,
//!   gate, expander, de-esser, auto gain, stereo tools, loudness, bass
//!   enhancer, exciter, crossfeed, echo and reverb.
//!
//! `import` reads EasyEffects presets and Equalizer APO / AutoEq text.

pub mod catalog;
pub mod eq;
pub mod graph;
pub mod import;
pub mod preset;
pub mod service;
pub mod store;

pub use catalog::{Catalog, Category, Control, PluginInfo, Unit};
pub use preset::{Band, BandKind, Direction, PluginSlot, Preset};
