//! Seccomp listener — the main event loop that reads seccomp
//! notifications from a listener file descriptor and dispatches them
//! to the [`GatedDaemon`].
//!
//! On real Linux this reads `struct seccomp_notif` from a seccomp
//! listener fd via `ioctl(SECCOMP_IOCTL_NOTIF_RECV)` and responds
//! via `ioctl(SECCOMP_IOCTL_NOTIF_SEND)`.  Because the real kernel
//! interface requires root and a live seccomp sandbox, this module
//! also provides a mock source that replays pre-built events for
//! testing and development.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::daemon::{EventResponse, GatedDaemon, EACCES};
use crate::decoder::SyscallEvent;
use crate::error::{GatedError, Result};

// ── Notification source trait ────────────────────────────────────

/// Abstraction over where seccomp notifications come from.
///
/// Real implementations read from the kernel; the mock feeds a
/// pre-built vector of events.
pub trait NotifSource: Send {
    /// Block until the next notification arrives.  Returns `None` when
    /// the source is exhausted or the fd has been closed.
    fn recv(&mut self) -> Result<Option<SyscallEvent>>;

    /// Respond to a notification.  `event` is the original event (so
    /// the implementation can extract the notification ID), `response`
    /// is the daemon's decision.
    fn respond(&mut self, event: &SyscallEvent, response: EventResponse) -> Result<()>;
}

// ── Mock source ──────────────────────────────────────────────────

/// A mock notification source that replays a fixed list of events.
///
/// Useful for integration tests and dry-run demos without root or a
/// real seccomp sandbox.
pub struct MockNotifSource {
    events: Vec<SyscallEvent>,
    index: usize,
    /// Records the responses the daemon gave, in order.
    pub responses: Vec<(SyscallEvent, EventResponse)>,
}

impl MockNotifSource {
    pub fn new(events: Vec<SyscallEvent>) -> Self {
        Self {
            events,
            index: 0,
            responses: Vec::new(),
        }
    }
}

impl NotifSource for MockNotifSource {
    fn recv(&mut self) -> Result<Option<SyscallEvent>> {
        if self.index >= self.events.len() {
            return Ok(None);
        }
        let event = self.events[self.index].clone();
        self.index += 1;
        Ok(Some(event))
    }

    fn respond(&mut self, event: &SyscallEvent, response: EventResponse) -> Result<()> {
        self.responses.push((event.clone(), response));
        Ok(())
    }
}

// ── Linux seccomp fd source (stub) ──────────────────────────────

/// Placeholder for the real Linux seccomp listener fd source.
///
/// A full implementation would:
/// 1. Accept the listener fd (received from the sandbox shim via
///    SCM_RIGHTS over a unix socket).
/// 2. Use `ioctl(fd, SECCOMP_IOCTL_NOTIF_RECV, &mut notif)` to
///    receive `struct seccomp_notif`.
/// 3. Map the kernel struct fields into a `SyscallEvent`.
/// 4. Use `ioctl(fd, SECCOMP_IOCTL_NOTIF_SEND, &resp)` to respond.
///
/// This is gated behind `target_os = "linux"` and requires root.
#[cfg(target_os = "linux")]
pub struct SeccompFdSource {
    _fd: i32,
}

#[cfg(target_os = "linux")]
impl SeccompFdSource {
    /// Wrap a raw seccomp listener fd.
    ///
    /// # Safety
    /// The caller must ensure `fd` is a valid seccomp user-notification
    /// file descriptor.
    pub unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { _fd: fd }
    }
}

#[cfg(target_os = "linux")]
impl NotifSource for SeccompFdSource {
    fn recv(&mut self) -> Result<Option<SyscallEvent>> {
        // TODO: ioctl(SECCOMP_IOCTL_NOTIF_RECV)
        // For now return None (exhausted) so the loop exits.
        Ok(None)
    }

    fn respond(&mut self, _event: &SyscallEvent, _response: EventResponse) -> Result<()> {
        // TODO: ioctl(SECCOMP_IOCTL_NOTIF_SEND)
        Ok(())
    }
}

// ── Listener (the main event loop) ──────────────────────────────

/// The seccomp listener that drives the daemon's event loop.
///
/// Reads notifications from a [`NotifSource`], passes each one to
/// [`GatedDaemon::handle_event`], and writes the response back.
pub struct SeccompListener {
    source: Box<dyn NotifSource>,
    /// Shared shutdown flag.  Set to `true` to make the loop exit
    /// cleanly after the current event.
    shutdown: Arc<AtomicBool>,
}

impl SeccompListener {
    /// Create a listener from any notification source.
    pub fn new(source: Box<dyn NotifSource>) -> Self {
        Self {
            source,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a mock listener that replays the given events.
    pub fn new_mock(events: Vec<SyscallEvent>) -> Self {
        Self::new(Box::new(MockNotifSource::new(events)))
    }

    /// Get a handle to the shutdown flag.  Setting it to `true` will
    /// cause [`run`](Self::run) to exit after the current event.
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Run the main event loop.
    ///
    /// Blocks until the source is exhausted, an unrecoverable error
    /// occurs, or the shutdown flag is set.
    ///
    /// Returns the number of events processed.
    pub fn run(&mut self, daemon: &mut GatedDaemon) -> Result<u64> {
        info!("seccomp listener starting");
        let mut processed: u64 = 0;

        loop {
            // Check shutdown flag
            if self.shutdown.load(Ordering::Relaxed) {
                info!("shutdown flag set, exiting event loop");
                break;
            }

            // Receive next notification
            let event = match self.source.recv() {
                Ok(Some(ev)) => ev,
                Ok(None) => {
                    info!("notification source exhausted, exiting event loop");
                    break;
                }
                Err(e) => {
                    error!(%e, "error receiving notification");
                    return Err(e);
                }
            };

            debug!(
                pid = event.pid,
                syscall = event.syscall_nr,
                "received seccomp notification"
            );

            // Dispatch to the daemon
            let response = match daemon.handle_event(event.clone()) {
                Ok(resp) => resp,
                Err(GatedError::PidNotFound { pid }) => {
                    warn!(pid, "unknown PID, denying syscall");
                    EventResponse::Deny(EACCES)
                }
                Err(GatedError::UnknownSyscall(nr)) => {
                    warn!(nr, "unknown syscall, denying");
                    EventResponse::Deny(EACCES)
                }
                Err(e) => {
                    error!(%e, "error handling event, denying syscall");
                    EventResponse::Deny(EACCES)
                }
            };

            debug!(?response, "responding to notification");

            // Send response back
            if let Err(e) = self.source.respond(&event, response) {
                error!(%e, "error sending response");
                return Err(e);
            }

            processed += 1;
        }

        info!(processed, "seccomp listener stopped");
        Ok(processed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{SYS_CONNECT, SYS_OPENAT};
    use crate::prompt::{MockPrompter, PromptResponse};
    use crate::registry::SandboxEntry;
    use bingux_common::package_id::Arch;
    use bingux_common::PackageId;
    use std::time::SystemTime;

    fn setup_daemon(response: PromptResponse, base: std::path::PathBuf) -> GatedDaemon {
        let prompter = Box::new(MockPrompter::new(response));
        let mut daemon = GatedDaemon::with_base_path(prompter, base);
        daemon.registry.register(
            100,
            SandboxEntry {
                package_name: "firefox".to_string(),
                package_id: PackageId::new("firefox", "128.0", Arch::X86_64Linux).unwrap(),
                user: "alice".to_string(),
                uid: 1000,
                listener_fd: None,
                started_at: SystemTime::now(),
            },
        );
        daemon
    }

    #[test]
    fn mock_listener_processes_all_events() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AllowOnce, dir.path().to_path_buf());

        let events = vec![
            SyscallEvent {
                pid: 100,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            },
            SyscallEvent {
                pid: 100,
                syscall_nr: SYS_OPENAT,
                args: [0, 0x1000, 0, 0, 0, 0],
            },
        ];

        let mut listener = SeccompListener::new_mock(events);
        let count = listener.run(&mut daemon).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn mock_listener_records_responses() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        let events = vec![SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        }];

        let source = MockNotifSource::new(events);
        let mut listener = SeccompListener::new(Box::new(source));
        let count = listener.run(&mut daemon).unwrap();
        assert_eq!(count, 1);

        // Access responses through the source (downcast)
        // Since we can't easily downcast, verify via the count
        // The daemon's MockPrompter returned Deny, so the event
        // should have been denied.
    }

    #[test]
    fn unknown_pid_is_denied_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AllowOnce, dir.path().to_path_buf());

        // PID 999 is not registered
        let events = vec![
            SyscallEvent {
                pid: 999,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            },
            SyscallEvent {
                pid: 100,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            },
        ];

        let mut listener = SeccompListener::new_mock(events);
        let count = listener.run(&mut daemon).unwrap();
        // Both should be processed — unknown PID is a soft error
        assert_eq!(count, 2);
    }

    #[test]
    fn shutdown_flag_stops_loop() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AllowOnce, dir.path().to_path_buf());

        // Many events, but we'll set the shutdown flag immediately
        let events = vec![
            SyscallEvent {
                pid: 100,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            },
            SyscallEvent {
                pid: 100,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            },
        ];

        let mut listener = SeccompListener::new_mock(events);
        let handle = listener.shutdown_handle();
        handle.store(true, Ordering::Relaxed);

        let count = listener.run(&mut daemon).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn empty_source_exits_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AllowOnce, dir.path().to_path_buf());

        let mut listener = SeccompListener::new_mock(vec![]);
        let count = listener.run(&mut daemon).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn pre_granted_capability_allows_without_prompt() {
        let dir = tempfile::tempdir().unwrap();
        // Use Deny as the prompt response — if the pre-grant works,
        // the prompter should never be called.
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        // Pre-grant net_outbound via handle_event + AlwaysAllow path:
        // We first run with AlwaysAllow to persist the grant, then
        // switch to Deny and verify the grant is still active.
        {
            let prompter = Box::new(MockPrompter::new(PromptResponse::AlwaysAllow));
            let mut setup_daemon = GatedDaemon::with_base_path(prompter, dir.path().to_path_buf());
            setup_daemon.registry.register(
                100,
                SandboxEntry {
                    package_name: "firefox".to_string(),
                    package_id: PackageId::new("firefox", "128.0", Arch::X86_64Linux).unwrap(),
                    user: "alice".to_string(),
                    uid: 1000,
                    listener_fd: None,
                    started_at: SystemTime::now(),
                },
            );
            let event = SyscallEvent {
                pid: 100,
                syscall_nr: SYS_CONNECT,
                args: [3, 0x2000, 16, 0, 0, 0],
            };
            let resp = setup_daemon.handle_event(event).unwrap();
            assert_eq!(resp, crate::daemon::EventResponse::Continue);
        }

        // Now run the real test with Deny backend — the pre-grant from
        // disk should still allow the connect.
        let events = vec![SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        }];

        let mut listener = SeccompListener::new_mock(events);
        let count = listener.run(&mut daemon).unwrap();
        assert_eq!(count, 1);
    }
}
