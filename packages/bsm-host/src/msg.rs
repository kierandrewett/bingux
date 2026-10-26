//! Messages from the worker threads (D-Bus, PipeWire, layer list) into the
//! bridge's main event loop.

use std::os::fd::OwnedFd;
use std::sync::Arc;

use crate::frames::BufferSet;
use crate::readiness::LayerSurface;

/// A rectangle in buffer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// State of a PipeWire stream as seen by the stream thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStatus {
    /// Format negotiated at this size; buffers flow.
    Streaming { width: u32, height: u32 },
    /// The stream ended (monitor gone or Gnoblin lost).
    Ended,
    Failed(String),
}

/// A cursor image taken from the stream's cursor metadata (ARGB32 premultiplied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub pixels: Vec<u8>,
}

pub enum Msg {
    /// Mutter's ScreenCast, RemoteDesktop and DisplayConfig names are owned.
    GnoblinBus,
    /// `org.gnoblin.Hosted1.Ready` changed.
    GnoblinReady(bool),
    /// Gnoblin or its D-Bus services are gone: the session is lost.
    GnoblinLost(String),
    /// A fresh EIS connection from a RemoteDesktop session.
    Eis(OwnedFd),
    /// A virtual monitor stream exists with this PipeWire node.
    MonitorStream { output: String, epoch: u64, node_id: u32 },
    /// The monitor is laid out as requested.
    MonitorApplied { output: String, epoch: u64, connector: String },
    /// A monitor request failed.
    MonitorFailed { output: String, epoch: u64, reason: String },
    /// Gnoblin's layer-surface list.
    Layers(Vec<LayerSurface>),
    /// A stream changed state.
    Stream { output: String, epoch: u64, status: StreamStatus },
    /// A frame was copied into slot `slot` of `set`.
    Frame { output: String, epoch: u64, set: Arc<BufferSet>, slot: usize, damage: Vec<Rect>, seq: u64 },
    /// A new cursor image for the output.
    Cursor { output: String, epoch: u64, image: CursorImage },
    /// The cursor is hidden.
    CursorHidden { output: String, epoch: u64 },
    /// Gnoblin asked for the session to be locked (forwarded to bsm).
    LockRequested,
    /// Gnoblin asked for a session action (logout, reboot, ...).
    Action(String),
    /// SIGTERM or SIGINT.
    Terminate,
}
