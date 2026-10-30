//! Shared memory for frames: a sealed `memfd` the host writes and the viewer
//! maps read-only.
//!
//! The host creates a [`Shared`] of a fixed size, seals it against growing and
//! shrinking (so a map of it can never fault past the end), fills it and sends
//! its descriptor. The viewer calls [`Shared::map`], which refuses a descriptor
//! that is not sealed or is smaller than the message promised, copies the
//! pixels out with [`Shared::copy_to`] and drops it.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// The largest area one frame or page may hold, in bytes: 8192 by 8192 pixels
/// of four bytes.
pub const MAX_BYTES: usize = 8192 * 8192 * 4;

/// A memory-mapped `memfd`.
pub struct Shared {
    fd: OwnedFd,
    ptr: *mut u8,
    len: usize,
    writable: bool,
}

// SAFETY: the mapping is owned by this value and unmapped on drop; the bytes
// are only touched through `&mut self` (writer) or copied out (reader).
unsafe impl Send for Shared {}

impl Shared {
    /// A new zeroed region of `len` bytes, mapped read-write and sealed.
    pub fn create(name: &str, len: usize) -> io::Result<Shared> {
        if len == 0 || len > MAX_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a size out of range"));
        }
        let name = std::ffi::CString::new(name).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: a valid C string and flags.
        let raw = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: memfd_create returned a descriptor nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: ftruncate and fcntl on our own descriptor.
        unsafe {
            if libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::fcntl(fd.as_raw_fd(), libc::F_ADD_SEALS, libc::F_SEAL_SHRINK | libc::F_SEAL_GROW) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Shared::mapped(fd, len, true)
    }

    /// Map the region `fd` for reading. Refused when the descriptor is not a
    /// sealed `memfd`, or holds fewer than `len` bytes.
    pub fn map(fd: OwnedFd, len: usize) -> io::Result<Shared> {
        if len == 0 || len > MAX_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a size out of range"));
        }
        // SAFETY: fcntl and fstat on a descriptor we own; `stat` is plain data.
        let (seals, size) = unsafe {
            let seals = libc::fcntl(fd.as_raw_fd(), libc::F_GET_SEALS);
            let mut stat: libc::stat = std::mem::zeroed();
            if libc::fstat(fd.as_raw_fd(), &mut stat) != 0 {
                return Err(io::Error::last_os_error());
            }
            (seals, stat.st_size)
        };
        let needed = libc::F_SEAL_SHRINK | libc::F_SEAL_GROW;
        if seals < 0 || seals & needed != needed {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the memory is not sealed"));
        }
        if (size as usize) < len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the memory is smaller than promised"));
        }
        Shared::mapped(fd, len, false)
    }

    fn mapped(fd: OwnedFd, len: usize, writable: bool) -> io::Result<Shared> {
        let protection = if writable { libc::PROT_READ | libc::PROT_WRITE } else { libc::PROT_READ };
        // SAFETY: mapping `len` bytes of a descriptor that holds at least that
        // many and cannot shrink.
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, protection, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Shared { fd, ptr: ptr.cast(), len, writable })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The descriptor, to send beside a message.
    pub fn raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    pub fn into_fd(self) -> OwnedFd {
        // Dropping `self` unmaps; the descriptor is duplicated out first.
        let fd = self.fd.try_clone().expect("duplicating a memfd");
        drop(self);
        fd
    }

    /// The writable bytes. Only a region made with [`Shared::create`] has them.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        assert!(self.writable, "a read-only mapping");
        // SAFETY: `ptr` maps `len` writable bytes owned by `self`; the viewer
        // is not told of the region until it is filled.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    /// Copy the first `len` bytes into a vector. The other process may be
    /// writing the region, so the bytes are read with a plain copy and never
    /// held as a slice.
    pub fn copy_to(&self, len: usize) -> Vec<u8> {
        let len = len.min(self.len);
        let mut out = Vec::<u8>::with_capacity(len);
        // SAFETY: `ptr` maps at least `len` readable bytes; the vector has room
        // for them and they are initialised by the copy.
        unsafe {
            std::ptr::copy_nonoverlapping(self.ptr, out.as_mut_ptr(), len);
            out.set_len(len);
        }
        out
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: unmapping exactly what `mapped` mapped.
        unsafe {
            libc::munmap(self.ptr.cast(), self.len);
        }
    }
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").field("len", &self.len).field("writable", &self.writable).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_written_by_one_side_is_read_by_the_other() {
        let mut host = Shared::create("frame", 64).unwrap();
        host.bytes_mut()[..4].copy_from_slice(&[1, 2, 3, 4]);
        let fd = host.fd.try_clone().unwrap();
        let viewer = Shared::map(fd, 64).unwrap();
        assert_eq!(&viewer.copy_to(4), &[1, 2, 3, 4]);
        // Later writes show through the same pages.
        host.bytes_mut()[0] = 9;
        assert_eq!(viewer.copy_to(1), vec![9]);
    }

    #[test]
    fn an_unsealed_or_short_region_is_refused() {
        let raw = unsafe { libc::memfd_create(c"bare".as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        unsafe { libc::ftruncate(fd.as_raw_fd(), 64) };
        assert!(Shared::map(fd, 64).is_err(), "no seals");
        let host = Shared::create("frame", 16).unwrap();
        let fd = host.fd.try_clone().unwrap();
        assert!(Shared::map(fd, 64).is_err(), "smaller than promised");
    }

    #[test]
    fn sizes_out_of_range_are_refused() {
        assert!(Shared::create("zero", 0).is_err());
        assert!(Shared::create("huge", MAX_BYTES + 1).is_err());
    }
}
