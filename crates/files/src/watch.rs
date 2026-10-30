//! Telling a view that its folder changed.
//!
//! A [`Watcher`] puts one inotify watch on a folder and calls `on_change`
//! from its own thread when something in it is created, removed, renamed,
//! rewritten or has its attributes changed. Bursts (a download writing,
//! `git checkout`) are merged: the callback runs once the folder has been
//! quiet for [`QUIET`], or at least every [`LONGEST`] while it never is.
//! The callback only says "look again"; the owner lists the folder anew.
//!
//! Dropping the watcher stops the thread and closes the descriptors.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long the folder must be silent before the callback runs.
pub const QUIET: Duration = Duration::from_millis(150);
/// The longest the callback is held back by a folder that keeps changing.
pub const LONGEST: Duration = Duration::from_secs(1);

/// A running watch. See the module documentation.
pub struct Watcher {
    stop: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    /// Watch the folder `path`.
    pub fn new(path: &Path, on_change: impl Fn() + Send + 'static) -> io::Result<Watcher> {
        use std::os::unix::ffi::OsStrExt;
        // SAFETY: plain syscalls; each returned descriptor is checked and then
        // owned by exactly one `OwnedFd`.
        let (inotify, stop) = unsafe {
            let inotify = libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK);
            if inotify < 0 {
                return Err(io::Error::last_os_error());
            }
            let inotify = OwnedFd::from_raw_fd(inotify);
            let stop = libc::eventfd(0, libc::EFD_CLOEXEC);
            if stop < 0 {
                return Err(io::Error::last_os_error());
            }
            (inotify, OwnedFd::from_raw_fd(stop))
        };
        let mut name = path.as_os_str().as_bytes().to_vec();
        name.push(0);
        let mask = libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_CLOSE_WRITE
            | libc::IN_ATTRIB
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_ONLYDIR;
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let watch =
            unsafe { libc::inotify_add_watch(inotify.as_raw_fd(), name.as_ptr().cast(), mask) };
        if watch < 0 {
            return Err(io::Error::last_os_error());
        }
        let stop_for_thread = stop.as_raw_fd();
        // The thread owns the inotify descriptor; `stop` stays with the
        // watcher, which writes to it to wake the thread.
        let thread = thread::Builder::new()
            .name("bingux-files-watch".into())
            .spawn(move || run(inotify, stop_for_thread, on_change))?;
        Ok(Watcher {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let one: u64 = 1;
        // SAFETY: writing 8 bytes from a live u64 to an eventfd we own.
        unsafe {
            libc::write(self.stop.as_raw_fd(), (&raw const one).cast(), 8);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(inotify: OwnedFd, stop: i32, on_change: impl Fn()) {
    let mut pending_since: Option<Instant> = None;
    let mut last_event = Instant::now();
    let mut buffer = [0u8; 4096];
    loop {
        let mut fds = [
            libc::pollfd {
                fd: stop,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: inotify.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout = match pending_since {
            Some(first) => {
                let quiet_left = QUIET.saturating_sub(last_event.elapsed());
                let longest_left = LONGEST.saturating_sub(first.elapsed());
                quiet_left.min(longest_left).as_millis() as i32
            }
            None => -1,
        };
        // SAFETY: `fds` is a valid array of two pollfds.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[0].revents != 0 {
            return;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            // Drain; the events themselves do not matter, only that one came.
            loop {
                // SAFETY: reading into a live buffer of the stated length.
                let read = unsafe {
                    libc::read(
                        inotify.as_raw_fd(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                if read <= 0 {
                    break;
                }
            }
            last_event = Instant::now();
            pending_since.get_or_insert(last_event);
        }
        if let Some(first) = pending_since
            && (last_event.elapsed() >= QUIET || first.elapsed() >= LONGEST)
        {
            pending_since = None;
            on_change();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reports_a_new_file_once() {
        let dir = std::env::temp_dir().join(format!("bingux-files-watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let watcher = Watcher::new(&dir, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        fs::write(dir.join("a"), b"1").unwrap();
        fs::write(dir.join("b"), b"2").unwrap();
        thread::sleep(Duration::from_millis(600));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        drop(watcher);
        fs::write(dir.join("c"), b"3").unwrap();
        thread::sleep(Duration::from_millis(400));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_a_missing_folder() {
        assert!(Watcher::new(Path::new("/definitely/not/here"), || {}).is_err());
    }
}
