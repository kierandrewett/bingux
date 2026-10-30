//! The host's end of the session: sending packets from any thread, and the
//! watchdog that ends a job that never finishes.

use std::os::fd::RawFd;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bingux_preview_core::host::channel::Channel;
use bingux_preview_core::host::protocol::{ErrorCode, Message, Packet};

/// The socket to the viewer. A failed send means the viewer is gone, and the
/// host's life is the viewer's: it ends.
pub struct Link {
    channel: Channel,
    send: Mutex<()>,
}

impl Link {
    pub fn new(channel: Channel) -> Link {
        Link { channel, send: Mutex::new(()) }
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn send(&self, request: u32, message: Message) {
        self.send_with(request, message, &[]);
    }

    pub fn send_with(&self, request: u32, message: Message, fds: &[RawFd]) {
        let _one_at_a_time = self.send.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.channel.send(&Packet { request, message }, fds).is_err() {
            std::process::exit(0);
        }
    }

    pub fn error(&self, request: u32, code: ErrorCode, message: &str) {
        self.send(request, Message::Error { code, message: message.to_owned() });
    }
}

/// Ends the process when one job runs longer than its limit.
///
/// A decoder or renderer cannot be interrupted from outside, so a file that
/// sends one into a loop is answered by exiting: the viewer sees the session
/// end and shows an error state, and the next file starts a new host.
pub struct Watchdog {
    start: Instant,
    /// Milliseconds since `start` at which the current job began, plus one;
    /// zero when idle.
    since: AtomicU64,
    limit: Duration,
}

impl Watchdog {
    pub fn start(link: std::sync::Arc<Link>, limit: Duration) -> std::sync::Arc<Watchdog> {
        let dog = std::sync::Arc::new(Watchdog { start: Instant::now(), since: AtomicU64::new(0), limit });
        let watching = std::sync::Arc::clone(&dog);
        let spawned = std::thread::Builder::new().name("watchdog".into()).spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let since = watching.since.load(Ordering::Acquire);
                if since == 0 {
                    continue;
                }
                let began = Duration::from_millis(since - 1);
                if watching.start.elapsed().saturating_sub(began) > watching.limit {
                    link.error(0, ErrorCode::Internal, "The preview took too long and was stopped.");
                    std::process::exit(124);
                }
            }
        });
        if spawned.is_err() {
            eprintln!("bingux-preview-host: no watchdog thread");
        }
        dog
    }

    /// A job begins; [`Busy`] ends it when dropped.
    pub fn busy(&self) -> Busy<'_> {
        self.since.store(self.start.elapsed().as_millis() as u64 + 1, Ordering::Release);
        Busy(self)
    }
}

pub struct Busy<'a>(&'a Watchdog);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.since.store(0, Ordering::Release);
    }
}
