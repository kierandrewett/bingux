//! The Bingux media engine: what plays music and films, with no UI of its own.
//!
//! - [`player`]: one libmpv player with its picture ([`render`]) and queue ([`queue`]).
//! - [`status`]: what the player is doing, for the window and the shell.
//! - [`library`]: the songs and films in the person's folders, indexed.
//! - [`thumbnail`]: frames of films, for the library and the seek preview.
//! - [`mpris`]: the player on the session bus (the shell's media controls).
//! - [`lastfm`]: scrobbling, when the person sets it up.
//! - [`settings`]: what the person chose.

pub mod lastfm;
pub mod library;
pub mod mpris;
pub mod mpv;
pub mod player;
pub mod queue;
pub mod render;
pub mod settings;
pub mod status;
pub mod thumbnail;

pub use player::{Change, Hooks, Levelling, NoHooks, Options, Player, Precision, Sleep};
pub use queue::{Item, Kind, Queue, Repeat};
pub use render::{Backend, Frame, FrameSink};
pub use status::{Chapter, PlayState, SleepLeft, Status, Track, TrackKind};
