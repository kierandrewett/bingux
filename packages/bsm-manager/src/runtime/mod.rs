//! The I/O runtime around the pure core: one driver thread owns the `Manager`; workers
//! (helper client, greeter control server, D-Bus, system services, appearance) talk to it
//! through a channel and never share its state.

pub mod api;
pub mod driver;
pub mod greeter_server;
pub mod helper_client;
pub mod persist;
pub mod recovery;
pub mod rng;

pub use api::*;
pub use driver::{CALL_DEADLINE, RuntimeConfig, RuntimeHandle, channel, start};
pub use greeter_server::ControlConfig;
pub use helper_client::HelperEndpoint;
