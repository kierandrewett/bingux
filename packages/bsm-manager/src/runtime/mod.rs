//! The I/O runtime around the pure core: one driver thread owns the `Manager`; workers
//! (helper client, greeter control server, D-Bus, system services, appearance) talk to it
//! through a channel and never share its state.

pub mod api;
