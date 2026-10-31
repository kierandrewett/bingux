//! Sound effects for the Bingux desktop.
//!
//! The effects are a stack of plugins between the applications and the sound
//! device (output effects) or between a microphone and the applications
//! (input effects). The stack is a plain file, a *preset* (see `preset`); the
//! engine runs each effect as a PipeWire `filter-chain` of its own and joins
//! them with links (see `graph`, `chain` and `service`), so the Settings app
//! is only a front end: it edits preset files and tells the engine to read
//! them again.
//!
//! The effects are published plugins, the same set Easy Effects hosts: Linux
//! Studio Plugins, Calf Studio Gear, ZamAudio, MDA and x42 (LV2), Rubber Band
//! (LADSPA) and PipeWire's own convolver. `effects` lists them and `catalog`
//! describes each one from its own port metadata. Bingux writes no plugin of
//! its own.
//!
//! `import` reads EasyEffects presets and Equalizer APO / AutoEq text, and
//! `legacy` migrates presets written for Bingux's old plugins.

pub mod catalog;
pub mod chain;
pub mod effects;
pub mod eq;
pub mod graph;
pub mod import;
pub mod import_table;
pub mod legacy;
pub mod lv2;
pub mod preset;
pub mod service;
pub mod store;

pub use catalog::{Catalog, Choice, Control, PluginInfo, Unit};
pub use effects::{Category, Effect};
pub use preset::{Direction, PluginSlot, Preset};
