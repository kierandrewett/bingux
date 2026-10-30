//! What the host gives up before it reads a byte of an untrusted file.

use std::time::Duration;

/// The address space a host may map, in bytes. GStreamer and its plugins map a
/// lot without touching it, so playback gets more than a document.
pub const MEDIA_ADDRESS_SPACE: u64 = 8 << 30;
pub const DOCUMENT_ADDRESS_SPACE: u64 = 4 << 30;
/// Open descriptors: the socket, the file, shared memory, GStreamer's pipes.
pub const OPEN_FILES: u64 = 256;
/// Processor time of a document host, in seconds; a playing film has none.
pub const DOCUMENT_CPU_SECONDS: u64 = 300;
/// How long opening a file may take before the host stops.
pub const OPEN_TIME: Duration = Duration::from_secs(20);
/// How long one page render or search step may take.
pub const JOB_TIME: Duration = Duration::from_secs(30);
/// The largest document the host opens, in bytes.
pub const MAX_DOCUMENT_BYTES: u64 = 512 << 20;

/// What a person reads for a file that is over a cap.
pub const TOO_LARGE: &str = "This file is too large to preview.";

fn set(resource: libc::__rlimit_resource_t, value: u64) {
    let limit = libc::rlimit { rlim_cur: value as libc::rlim_t, rlim_max: value as libc::rlim_t };
    // SAFETY: setrlimit with a valid struct; a failure leaves the limit as it was.
    unsafe {
        libc::setrlimit(resource, &limit);
    }
}

/// Apply the limits that hold for every session: no new privileges, no core
/// dumps, a bounded number of descriptors, and a life that ends with the viewer.
pub fn harden() {
    // SAFETY: prctl with documented arguments; getppid and _exit are async-signal-safe.
    unsafe {
        libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
        // Spawned by a parent that has already gone: nobody will ever signal.
        if libc::getppid() == 1 {
            libc::_exit(1);
        }
    }
    set(libc::RLIMIT_CORE, 0);
    set(libc::RLIMIT_NOFILE, OPEN_FILES);
}

pub fn limit_media() {
    set(libc::RLIMIT_AS, MEDIA_ADDRESS_SPACE);
}

pub fn limit_document() {
    set(libc::RLIMIT_AS, DOCUMENT_ADDRESS_SPACE);
    set(libc::RLIMIT_CPU, DOCUMENT_CPU_SECONDS);
}
