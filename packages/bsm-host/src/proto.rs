//! Client bindings for the private `bsm_session_host_v1` protocol.
//!
//! Generated at build time from the single source of truth,
//! `packages/bsm/protocol/bsm-session-host-v1.xml`.

#![allow(clippy::all, non_camel_case_types, unused_imports, missing_docs)]

use wayland_client;
use wayland_client::protocol::*;

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("../bsm/protocol/bsm-session-host-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("../bsm/protocol/bsm-session-host-v1.xml");
