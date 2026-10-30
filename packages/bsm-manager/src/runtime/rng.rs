//! Operating system randomness for session ids and generations.

use std::fs::File;
use std::io::Read;

use crate::core::Rng;

/// Reads `/dev/urandom`. Failure to read randomness is fatal: ids must not be guessable.
pub struct OsRng(File);

impl OsRng {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self(File::open("/dev/urandom")?))
    }
}

impl Rng for OsRng {
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.0.read_exact(&mut b).expect("/dev/urandom is readable");
        u64::from_le_bytes(b)
    }
}
