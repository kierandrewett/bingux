//! The helper process that decodes media and renders documents, and how a
//! viewer talks to it.
//!
//! | Module | What it holds |
//! | ------ | ------------- |
//! | [`protocol`] | The wire format, version 1: a [`Packet`](protocol::Packet) per datagram |
//! | [`channel`] | The `SOCK_SEQPACKET` socket pair that carries packets and descriptors |
//! | [`shm`] | Sealed `memfd` regions: how frames and pages travel |
//! | [`player`] | The player's state machine, fed by packets, answering with packets |
//!
//! The helper itself is the `bingux-preview-host` crate; `bingux_ui::preview`
//! spawns it. See `design/preview.md`.

pub mod channel;
pub mod player;
pub mod protocol;
pub mod shm;
