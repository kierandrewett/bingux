//! The browser as a program: its command line and how it starts.

mod command_line;
mod launch;

pub use command_line::CommandLine;
pub use launch::Launch;

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod structure_tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
