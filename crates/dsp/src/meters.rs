//! Meters a host cannot read from the plugin's ports.
//!
//! PipeWire's filter-chain does not expose output control ports, so a plugin
//! such as the compressor publishes its readings (gain reduction) in a small
//! shared file instead. The host sets `BINGUX_DSP_METERS` to the file's path
//! before it loads the plugins, and gives every plugin it wants to watch a
//! different "Meter slot" control from 1 to 63. Slot 0 publishes nothing.
//!
//! The file is a 16 byte header (`BGXM`, then the slot count as a little
//! endian u32) followed by one 16 byte record per slot of four little endian
//! f32 readings, in the order of the plugin's meter ports. A reader just reads
//! the file; there is no locking, and a reading is always a whole f32.

use std::ffi::CString;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};

/// The environment variable that names the file.
pub const ENV: &str = "BINGUX_DSP_METERS";
/// Records in the file, slot 0 included.
pub const SLOTS: usize = 64;
/// Readings in a record.
pub const VALUES: usize = 4;
/// Bytes before the first record.
pub const HEADER: usize = 16;
/// The magic number at the start of the file.
pub const MAGIC: [u8; 4] = *b"BGXM";

/// Bands in the spectrum record that follows the slots: levels in dBFS of
/// 64 log-spaced bands from 20 Hz to 20 kHz, written by the analyser plugin.
pub const BANDS: usize = 64;

/// Where the spectrum record starts.
pub const SPECTRUM_AT: usize = HEADER + SLOTS * VALUES * 4;

/// The size of a meter file in bytes.
pub const fn file_len() -> usize {
    SPECTRUM_AT + BANDS * 4
}

/// The spectrum in a meter file read into `bytes`, or `None` if it holds none.
/// A file the analyser has not written has every band at exactly zero.
pub fn read_spectrum(bytes: &[u8]) -> Option<[f32; BANDS]> {
    if bytes.len() < file_len() || bytes[..4] != MAGIC {
        return None;
    }
    let mut bands = [0.0; BANDS];
    for (index, band) in bands.iter_mut().enumerate() {
        let at = SPECTRUM_AT + index * 4;
        *band = f32::from_le_bytes(bytes[at..at + 4].try_into().ok()?);
    }
    bands.iter().any(|band| *band != 0.0).then_some(bands)
}

/// A new, empty meter file's contents.
pub fn empty_file() -> Vec<u8> {
    let mut bytes = vec![0u8; file_len()];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4..8].copy_from_slice(&(SLOTS as u32).to_le_bytes());
    bytes
}

/// The readings in `slot` of a meter file read into `bytes`.
pub fn read(bytes: &[u8], slot: usize) -> Option<[f32; VALUES]> {
    if slot >= SLOTS || bytes.len() < file_len() || bytes[..4] != MAGIC {
        return None;
    }
    let at = HEADER + slot * VALUES * 4;
    let mut values = [0.0; VALUES];
    for (index, value) in values.iter_mut().enumerate() {
        let start = at + index * 4;
        *value = f32::from_le_bytes(bytes[start..start + 4].try_into().ok()?);
    }
    Some(values)
}

struct Map(*mut u8);

// The mapping is shared memory written with atomic stores.
unsafe impl Sync for Map {}
unsafe impl Send for Map {}

static MAP: LazyLock<Option<Map>> = LazyLock::new(open);

fn open() -> Option<Map> {
    let path = CString::new(std::env::var_os(ENV)?.to_str()?).ok()?;
    // SAFETY: plain libc calls on a path and a descriptor this function owns.
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        if fd < 0 {
            return None;
        }
        let mut stat: libc::stat = std::mem::zeroed();
        let usable = libc::fstat(fd, &mut stat) == 0 && stat.st_size as usize >= file_len();
        let pointer = if usable {
            libc::mmap(
                std::ptr::null_mut(),
                file_len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        } else {
            libc::MAP_FAILED
        };
        libc::close(fd);
        (pointer != libc::MAP_FAILED).then(|| Map(pointer.cast()))
    }
}

/// Publish `values` (at most four) in `slot`. Does nothing for slot 0, an
/// unknown slot or when no meter file was given.
pub(crate) fn publish(slot: usize, values: &[f32]) {
    if slot == 0 || slot >= SLOTS {
        return;
    }
    let Some(Map(base)) = MAP.as_ref() else {
        return;
    };
    for (index, value) in values.iter().take(VALUES).enumerate() {
        let at = HEADER + slot * VALUES * 4 + index * 4;
        // SAFETY: `at` is inside the mapping, aligned for a u32, and the
        // mapping lives for the whole process.
        let cell = unsafe { &*base.add(at).cast::<AtomicU32>() };
        cell.store(value.to_bits().to_le(), Ordering::Relaxed);
    }
}

/// Publish the analyser's spectrum.
pub(crate) fn publish_spectrum(bands: &[f32; BANDS]) {
    let Some(Map(base)) = MAP.as_ref() else {
        return;
    };
    for (index, value) in bands.iter().enumerate() {
        // SAFETY: inside the mapping (see `publish`).
        let cell = unsafe { &*base.add(SPECTRUM_AT + index * 4).cast::<AtomicU32>() };
        cell.store(value.to_bits().to_le(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_round_trips_through_the_file_layout() {
        let mut bytes = empty_file();
        let at = HEADER + 5 * VALUES * 4;
        bytes[at..at + 4].copy_from_slice(&(-7.5f32).to_le_bytes());
        assert_eq!(read(&bytes, 5).unwrap()[0], -7.5);
        assert!(read(&bytes, SLOTS).is_none());
        assert!(read(&bytes[..8], 1).is_none());
    }
}
