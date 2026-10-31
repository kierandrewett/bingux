//! The bytes of one file, moved as fast as the file systems allow.
//!
//! In this order, each only when the one before cannot:
//!
//! 1. **`FICLONE`**: a reflink. Btrfs, XFS and bcachefs share the data
//!    blocks and the copy is instant and takes no space.
//! 2. **`copy_file_range(2)`** in chunks, until end of file. The kernel moves
//!    the data without it passing through this process (and between two file
//!    systems it still does the loop in the kernel).
//! 3. **`pread`/`pwrite`** through a reused 4 MiB buffer, for a kernel or a
//!    file system that refuses `copy_file_range`.
//!
//! A file with holes is copied by its extents (`SEEK_DATA`, `SEEK_HOLE`), so
//! a sparse disk image stays sparse and the holes cost no I/O. A file with no
//! holes is checked against the free space first (`fstatvfs`), which makes a
//! full drive fail before the first gigabyte instead of at the last.
//!
//! The cancel flag and the progress counters are touched once per chunk, an
//! atomic load and an atomic add, so neither slows the fast path.

use std::cell::RefCell;
use std::ffi::CString;
use std::fs::{File, Metadata};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{OpError, Progress};

/// Files smaller than this skip the free-space check: it saves nothing.
const SPACE_CHECK_FROM: u64 = 1 << 20;

/// The size of the fallback buffer. It stays small enough for the cache: a
/// 4 MiB buffer measured 40% slower than this one between two file systems.
const BUFFER: usize = 256 << 10;

/// What a copy has learned about the file systems it moves data between, so
/// the second file does not ask the question the first one was refused. Share
/// one per operation. A refusal that comes from one file (a busy one, say) is
/// not remembered; one that comes from the pair of file systems is.
pub(super) struct Hints {
    reflink: AtomicBool,
    kernel: AtomicBool,
}

impl Hints {
    pub(super) fn new() -> Self {
        Hints {
            reflink: AtomicBool::new(true),
            kernel: AtomicBool::new(true),
        }
    }

    /// For a test of the buffer path: neither is tried.
    #[cfg(test)]
    fn memory_only() -> Self {
        Hints {
            reflink: AtomicBool::new(false),
            kernel: AtomicBool::new(false),
        }
    }
}

/// This `errno` says the two file systems cannot do it, whatever the file.
fn refused_for_good(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::EXDEV | libc::EOPNOTSUPP | libc::ENOSYS | libc::ENOTTY)
    )
}

thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// What `errno` values mean "this call is not for this file system", as
/// opposed to a real failure of the drive.
fn unsupported(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(
            libc::EXDEV
                | libc::ENOSYS
                | libc::EINVAL
                | libc::EOPNOTSUPP
                | libc::EPERM
                | libc::EBADF
                | libc::ETXTBSY
                | libc::EISDIR
        )
    )
}

/// Run a metadata step the file system may not have (permissions, times and
/// extended attributes on FAT, say): "not supported" is fine, a real error
/// is not.
pub(super) fn tolerate(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if unsupported(&error) => Ok(()),
        other => other,
    }
}

/// Copy the contents of `input` into `output`, which is empty and open for
/// writing. `check` is asked between chunks and stops the copy by returning
/// an error; `progress` hears every byte.
pub(super) fn copy_data(
    input: &File,
    output: &File,
    meta: &Metadata,
    chunk: usize,
    progress: &Progress,
    check: &dyn Fn() -> Result<(), OpError>,
    hints: &Hints,
) -> Result<(), OpError> {
    let length = meta.len();
    if length == 0 {
        return Ok(());
    }
    if hints.reflink.load(Ordering::Relaxed) {
        if reflink(input, output) {
            progress.add_bytes(length);
            return Ok(());
        }
        if refused_for_good(&io::Error::last_os_error()) {
            hints.reflink.store(false, Ordering::Relaxed);
        }
    }
    // `st_blocks` counts 512-byte units actually allocated.
    let sparse = meta.blocks().saturating_mul(512) < length;
    if !sparse && length >= SPACE_CHECK_FROM {
        check_space(output, length)?;
    }
    let mut copier = Copier {
        input,
        output,
        kernel: hints.kernel.load(Ordering::Relaxed),
        hints,
        chunk: chunk.max(4096),
        progress,
        check,
    };
    if sparse {
        copier.extents(length)?;
        // A trailing hole is only a length.
        output.set_len(length)?;
    } else {
        let written = copier.span(0, None)?;
        if written != length {
            // The source changed size while it was read: the copy is what
            // was read, no more and no less.
            output.set_len(written)?;
        }
    }
    Ok(())
}

fn reflink(input: &File, output: &File) -> bool {
    // SAFETY: both descriptors are open for the whole call. FICLONE either
    // clones the whole file or fails without effect.
    unsafe { libc::ioctl(output.as_raw_fd(), libc::FICLONE, input.as_raw_fd()) == 0 }
}

/// Fail now, with "no space", when a whole file cannot fit on the drive,
/// rather than after copying most of it. This only asks the drive how much is
/// free (`fstatvfs`); preallocating would also do it, but on a memory file
/// system that means writing every page twice. A drive that reports nothing
/// (some network and FUSE file systems) is trusted.
fn check_space(output: &File, length: u64) -> Result<(), OpError> {
    // SAFETY: `statvfs` is plain data, filled in by the call on success.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the descriptor is open, and `stat` is a live `statvfs`.
    if unsafe { libc::fstatvfs(output.as_raw_fd(), &raw mut stat) } != 0 || stat.f_blocks == 0 {
        return Ok(());
    }
    let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    if free < length {
        return Err(io::Error::from_raw_os_error(libc::ENOSPC).into());
    }
    Ok(())
}

struct Copier<'a> {
    input: &'a File,
    output: &'a File,
    /// `copy_file_range` is still being tried, for this file.
    kernel: bool,
    hints: &'a Hints,
    chunk: usize,
    progress: &'a Progress,
    check: &'a dyn Fn() -> Result<(), OpError>,
}

impl Copier<'_> {
    /// Copy `start` up to `end` (or to end of file), to the same offsets.
    /// Returns how many bytes were copied.
    fn span(&mut self, start: u64, end: Option<u64>) -> Result<u64, OpError> {
        let mut position = start;
        loop {
            (self.check)()?;
            let want = match end {
                Some(end) if position >= end => break,
                Some(end) => (end - position).min(self.chunk as u64) as usize,
                None => self.chunk,
            };
            let copied = self.step(position, want)?;
            if copied == 0 {
                break;
            }
            position += copied as u64;
            self.progress.add_bytes(copied as u64);
        }
        Ok(position - start)
    }

    fn step(&mut self, position: u64, want: usize) -> Result<usize, OpError> {
        loop {
            if self.kernel {
                let mut from = position as libc::off64_t;
                let mut to = position as libc::off64_t;
                // SAFETY: both descriptors are open, and the offsets are
                // live locals the kernel reads and advances.
                let copied = unsafe {
                    libc::copy_file_range(
                        self.input.as_raw_fd(),
                        &raw mut from,
                        self.output.as_raw_fd(),
                        &raw mut to,
                        want,
                        0,
                    )
                };
                if copied >= 0 {
                    return Ok(copied as usize);
                }
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if !unsupported(&error) {
                    return Err(error.into());
                }
                self.kernel = false;
                if refused_for_good(&error) {
                    self.hints.kernel.store(false, Ordering::Relaxed);
                }
            }
            return self.step_through_memory(position, want);
        }
    }

    fn step_through_memory(&mut self, position: u64, want: usize) -> Result<usize, OpError> {
        SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            scratch.resize(want.min(BUFFER), 0);
            let read = loop {
                match self.input.read_at(&mut scratch, position) {
                    Ok(read) => break read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.into()),
                }
            };
            if read > 0 {
                self.output.write_all_at(&scratch[..read], position)?;
            }
            Ok(read)
        })
    }

    /// Copy the data extents of a sparse file; the holes stay holes, and
    /// count as done for the progress.
    fn extents(&mut self, length: u64) -> Result<(), OpError> {
        let mut position = 0u64;
        let mut written = 0u64;
        while position < length {
            let data = match seek(self.input, position, libc::SEEK_DATA) {
                Ok(data) => data,
                // Only a hole is left.
                Err(error) if error.raw_os_error() == Some(libc::ENXIO) => break,
                // This file system cannot say: copy the rest as it comes.
                Err(_) => {
                    written += self.span(position, None)?;
                    position = length;
                    break;
                }
            };
            let hole = seek(self.input, data, libc::SEEK_HOLE)
                .ok()
                .filter(|hole| *hole > data)
                .unwrap_or(length)
                .min(length);
            written += self.span(data, Some(hole))?;
            position = hole;
        }
        debug_assert!(position <= length || written > 0);
        self.progress.add_bytes(length.saturating_sub(written));
        Ok(())
    }
}

fn seek(file: &File, offset: u64, whence: libc::c_int) -> io::Result<u64> {
    // SAFETY: the descriptor is open; `lseek` only reads the offset.
    let at = unsafe { libc::lseek64(file.as_raw_fd(), offset as libc::off64_t, whence) };
    if at < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(at as u64)
    }
}

/// Copy the extended attributes (tags, ACLs, `user.*` data) as far as the
/// file systems on both sides take them. The SELinux label is left to the
/// destination's policy. Nothing here is an error: FAT has none.
pub(super) fn copy_xattrs(input: &File, output: &File) {
    let from = input.as_raw_fd();
    let to = output.as_raw_fd();
    // SAFETY (all calls below): the descriptors are open, and each buffer is
    // a live allocation of the length passed with it.
    let size = unsafe { libc::flistxattr(from, std::ptr::null_mut(), 0) };
    if size <= 0 {
        return;
    }
    let mut names = vec![0u8; size as usize];
    let size = unsafe { libc::flistxattr(from, names.as_mut_ptr().cast(), names.len()) };
    if size <= 0 {
        return;
    }
    names.truncate(size as usize);
    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        if name.starts_with(b"security.selinux") {
            continue;
        }
        let Ok(name) = CString::new(name) else {
            continue;
        };
        let length = unsafe { libc::fgetxattr(from, name.as_ptr(), std::ptr::null_mut(), 0) };
        if length < 0 {
            continue;
        }
        let mut value = vec![0u8; length as usize];
        let length =
            unsafe { libc::fgetxattr(from, name.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
        if length < 0 {
            continue;
        }
        unsafe {
            libc::fsetxattr(to, name.as_ptr(), value.as_ptr().cast(), length as usize, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;
    use std::fs::{self, OpenOptions};

    fn copy_with(source: &std::path::Path, target: &std::path::Path, kernel: bool) -> Progress {
        let input = File::open(source).unwrap();
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .unwrap();
        let meta = input.metadata().unwrap();
        let progress = Progress::new();
        // A small chunk makes the loops run more than once.
        let hints = if kernel {
            Hints::new()
        } else {
            Hints::memory_only()
        };
        copy_data(
            &input,
            &output,
            &meta,
            64 << 10,
            &progress,
            &|| Ok(()),
            &hints,
        )
        .unwrap();
        progress
    }

    fn pattern(length: usize) -> Vec<u8> {
        (0..length).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn both_paths_copy_every_byte() {
        let temp = Temp::new("data-dense");
        let data = pattern((3 << 20) + 12345);
        let source = temp.path().join("in");
        fs::write(&source, &data).unwrap();
        for (kernel, name) in [(true, "kernel"), (false, "memory")] {
            let target = temp.path().join(name);
            let progress = copy_with(&source, &target, kernel);
            assert_eq!(fs::read(&target).unwrap(), data, "{name}");
            assert_eq!(progress.bytes_done(), data.len() as u64, "{name}");
        }
    }

    #[test]
    fn an_empty_file_copies_as_empty() {
        let temp = Temp::new("data-empty");
        let source = temp.write("in", "");
        let target = temp.path().join("out");
        copy_with(&source, &target, true);
        assert_eq!(fs::metadata(&target).unwrap().len(), 0);
    }

    #[test]
    fn a_sparse_file_stays_sparse() {
        let temp = Temp::new("data-sparse");
        let source = temp.path().join("image");
        let length = 64u64 << 20;
        {
            let file = File::create(&source).unwrap();
            file.set_len(length).unwrap();
            file.write_all_at(b"head", 0).unwrap();
            file.write_all_at(b"middle", 30 << 20).unwrap();
            file.write_all_at(b"tail", length - 4).unwrap();
        }
        let meta = fs::metadata(&source).unwrap();
        if meta.blocks() * 512 >= length / 2 {
            eprintln!("skipped: this file system does not keep holes");
            return;
        }
        for (kernel, name) in [(true, "kernel"), (false, "memory")] {
            let target = temp.path().join(name);
            let progress = copy_with(&source, &target, kernel);
            let copy = fs::metadata(&target).unwrap();
            assert_eq!(copy.len(), length, "{name}: same length");
            assert!(
                copy.blocks() * 512 < length / 4,
                "{name}: {} bytes allocated for {length}",
                copy.blocks() * 512
            );
            let out = File::open(&target).unwrap();
            let mut seen = [0u8; 6];
            out.read_exact_at(&mut seen[..4], 0).unwrap();
            assert_eq!(&seen[..4], b"head");
            out.read_exact_at(&mut seen, 30 << 20).unwrap();
            assert_eq!(&seen, b"middle");
            out.read_exact_at(&mut seen[..4], length - 4).unwrap();
            assert_eq!(&seen[..4], b"tail");
            out.read_exact_at(&mut seen[..4], 10 << 20).unwrap();
            assert_eq!(&seen[..4], &[0, 0, 0, 0], "{name}: the hole reads as zeros");
            assert_eq!(progress.bytes_done(), length, "{name}: holes count as done");
        }
    }

    #[test]
    fn cancel_stops_between_chunks() {
        let temp = Temp::new("data-cancel");
        let source = temp.path().join("in");
        fs::write(&source, pattern(1 << 20)).unwrap();
        let input = File::open(&source).unwrap();
        let output = File::create(temp.path().join("out")).unwrap();
        let meta = input.metadata().unwrap();
        let calls = std::sync::atomic::AtomicU32::new(0);
        let check = || {
            if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 2 {
                Err(OpError::Cancelled)
            } else {
                Ok(())
            }
        };
        let progress = Progress::new();
        let result = copy_data(
            &input,
            &output,
            &meta,
            4096,
            &progress,
            &check,
            &Hints::memory_only(),
        );
        assert_eq!(result, Err(OpError::Cancelled));
        assert_eq!(progress.bytes_done(), 2 * 4096);
    }

    #[test]
    fn extended_attributes_follow_the_file_where_the_drive_has_them() {
        let temp = Temp::new("data-xattr");
        let source = temp.write("in", "x");
        let target = temp.write("out", "x");
        let input = File::open(&source).unwrap();
        let output = File::open(&target).unwrap();
        let name = CString::new("user.bingux.test").unwrap();
        // SAFETY: open descriptor, live buffers.
        let set = unsafe {
            libc::fsetxattr(
                input.as_raw_fd(),
                name.as_ptr(),
                b"tag".as_ptr().cast(),
                3,
                0,
            )
        };
        if set != 0 {
            eprintln!("skipped: this file system has no user xattrs");
            return;
        }
        copy_xattrs(&input, &output);
        let mut value = [0u8; 8];
        // SAFETY: as above.
        let got = unsafe {
            libc::fgetxattr(
                output.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        };
        assert_eq!(&value[..got.max(0) as usize], b"tag");
    }

    #[test]
    fn a_file_that_cannot_fit_fails_before_any_data_is_copied() {
        let temp = Temp::new("data-space");
        let output = File::create(temp.path().join("out")).unwrap();
        // More than any drive holds.
        let error = check_space(&output, u64::MAX / 2).unwrap_err();
        assert_eq!(error.to_string(), "There is no space left on the drive.");
        assert!(check_space(&output, 1).is_ok());
    }

    #[test]
    fn missing_support_is_tolerated_but_a_real_error_is_not() {
        assert!(tolerate(Err(io::Error::from_raw_os_error(libc::EPERM))).is_ok());
        assert!(tolerate(Err(io::Error::from_raw_os_error(libc::EOPNOTSUPP))).is_ok());
        assert!(tolerate(Err(io::Error::from_raw_os_error(libc::EIO))).is_err());
        assert!(tolerate(Err(io::Error::from_raw_os_error(libc::ENOSPC))).is_err());
    }
}
