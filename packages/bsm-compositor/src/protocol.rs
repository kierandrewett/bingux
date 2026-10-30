//! Server side of the private `bsm_session_host_v1` protocol. The XML is the single
//! source of truth in `packages/bsm/protocol`; this module only runs the scanner.

#![allow(clippy::all, unused_imports, dead_code)]

use wayland_server::protocol::*;
use wayland_server::{self};

pub mod __interfaces {
    use wayland_server::backend as wayland_backend;
    use wayland_server::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("../bsm/protocol/bsm-session-host-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_server_code!("../bsm/protocol/bsm-session-host-v1.xml");
