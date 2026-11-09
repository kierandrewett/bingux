//! A private session bus for tests that talk D-Bus for real.
//!
//! A test starts its own `dbus-daemon`, puts a fake service on it (a portal, a
//! notification daemon) and calls the platform code with a connection to it.
//! Nothing touches the person's session bus. A machine without `dbus-daemon`
//! skips these tests, saying so on stderr.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

/// A running `dbus-daemon`, stopped when dropped.
pub(crate) struct PrivateBus {
    child: Child,
    pub address: String,
}

impl PrivateBus {
    /// Start one, or `None` when there is no `dbus-daemon`.
    pub(crate) fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(child.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        let address = address.trim().to_owned();
        if address.is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        Some(Self { child, address })
    }

    /// A plain client connection.
    pub(crate) fn client(&self) -> Connection {
        Builder::address(self.address.as_str())
            .expect("the daemon's address")
            .build()
            .expect("a connection to the private bus")
    }

    /// A connection that owns `name` and serves `interface` at `path`.
    pub(crate) fn serve<I: zbus::object_server::Interface>(
        &self,
        name: &str,
        path: &str,
        interface: I,
    ) -> Connection {
        Builder::address(self.address.as_str())
            .expect("the daemon's address")
            .name(name.to_owned())
            .expect("a bus name")
            .serve_at(path.to_owned(), interface)
            .expect("an object path")
            .build()
            .expect("a service on the private bus")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start a private bus or skip the test.
macro_rules! private_bus {
    () => {
        match $crate::platform::testing::PrivateBus::start() {
            Some(bus) => bus,
            None => {
                eprintln!("skipped: dbus-daemon is not installed");
                return;
            }
        }
    };
}
pub(crate) use private_bus;
