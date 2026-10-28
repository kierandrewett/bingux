//! A file's SHA-256, read in the background.
//!
//! [`Sha256`] is a streaming hasher: feed it any number of slices, then
//! [`finish`](Sha256::finish) it. [`Hashing`] reads one file through it on a
//! worker thread, reports how far it has got and stops when asked or when it
//! is dropped, so a dialog can show a progress bar and a Stop button and
//! never block.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How much of the file one read takes. A multiple of the block size, small
/// enough that Stop is heard within a few milliseconds on a slow drive.
const CHUNK: usize = 256 * 1024;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// A streaming SHA-256 (FIPS 180-4).
#[derive(Clone, Debug)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }

    /// Add `data` to what is hashed.
    pub fn update(&mut self, mut data: &[u8]) {
        self.length += data.len() as u64;
        if self.filled > 0 {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < 64 {
                return;
            }
            let block = self.block;
            self.compress(&block);
            self.filled = 0;
        }
        let mut blocks = data.chunks_exact(64);
        for block in &mut blocks {
            self.compress(block.try_into().expect("a chunk of 64 bytes"));
        }
        let rest = blocks.remainder();
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// The digest of everything added, as 64 lower case hex digits.
    pub fn finish(mut self) -> String {
        let bits = self.length.wrapping_mul(8);
        let mut pad = [0_u8; 72];
        pad[0] = 0x80;
        let zeros = (119 - self.filled) % 64;
        pad[zeros + 1..zeros + 9].copy_from_slice(&bits.to_be_bytes());
        // `update` would count the padding as length, which is already taken.
        let length = self.length;
        self.update(&pad[..zeros + 9]);
        self.length = length;
        debug_assert_eq!(self.filled, 0);
        self.state.iter().map(|word| format!("{word:08x}")).collect()
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0_u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("four bytes"));
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7) ^ w[index - 15].rotate_right(18) ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17) ^ w[index - 2].rotate_right(19) ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let first = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (state, add) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(add);
        }
    }
}

/// How a [`Hashing`] ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashOutcome {
    /// The digest, 64 hex digits.
    Done(String),
    /// The file could not be read; the sentence says why.
    Failed(String),
    /// Stopped by the caller.
    Cancelled,
}

/// How far a [`Hashing`] has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashProgress {
    pub done: u64,
    /// The size of the file when the read started.
    pub total: u64,
    /// `None` while it is running.
    pub outcome: Option<HashOutcome>,
}

impl HashProgress {
    /// Read so far, from 0 to 1. An empty file is whole at once.
    pub fn fraction(&self) -> f64 {
        match (self.total, &self.outcome) {
            (_, Some(HashOutcome::Done(_))) | (0, _) => 1.0,
            (total, _) => (self.done as f64 / total as f64).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Default)]
struct Shared {
    done: AtomicU64,
    total: AtomicU64,
    cancel: AtomicBool,
    outcome: Mutex<Option<HashOutcome>>,
}

/// A file's SHA-256 being worked out on its own thread. Dropping it stops
/// the read.
#[derive(Debug)]
pub struct Hashing {
    shared: Arc<Shared>,
}

impl Hashing {
    /// Start reading `path`. Returns at once.
    pub fn start(path: PathBuf) -> Hashing {
        let shared = Arc::new(Shared::default());
        let worker = shared.clone();
        std::thread::spawn(move || {
            let outcome = hash_file(&path, &worker);
            *worker
                .outcome
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome);
        });
        Hashing { shared }
    }

    pub fn progress(&self) -> HashProgress {
        // The outcome is read first: the counters never go backwards, so a
        // finished read shows its final count.
        let outcome = self
            .shared
            .outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        HashProgress {
            done: self.shared.done.load(Ordering::Relaxed),
            total: self.shared.total.load(Ordering::Relaxed),
            outcome,
        }
    }

    /// Stop reading. The outcome becomes [`HashOutcome::Cancelled`].
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Hashing {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn hash_file(path: &Path, shared: &Shared) -> HashOutcome {
    let failed = |error: std::io::Error| HashOutcome::Failed(super::OpError::from(error).to_string());
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return failed(error),
    };
    match file.metadata() {
        Ok(meta) if meta.is_file() => shared.total.store(meta.len(), Ordering::Relaxed),
        Ok(_) => return HashOutcome::Failed("Only a file has a checksum.".to_owned()),
        Err(error) => return failed(error),
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        if shared.cancel.load(Ordering::Relaxed) {
            return HashOutcome::Cancelled;
        }
        match file.read(&mut buffer) {
            Ok(0) => return HashOutcome::Done(hasher.finish()),
            Ok(read) => {
                hasher.update(&buffer[..read]);
                shared.done.fetch_add(read as u64, Ordering::Relaxed);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return failed(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::testing::Temp;
    use super::*;

    fn digest(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finish()
    }

    #[test]
    fn known_digests() {
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Two blocks, with the length spilling into the second.
        assert_eq!(
            digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn the_padding_edges_agree_with_the_reference() {
        // 55, 56 and 64 bytes are where the length no longer fits one block.
        assert_eq!(
            digest(&[b'a'; 55]),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            digest(&[b'a'; 56]),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            digest(&[b'a'; 64]),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }

    #[test]
    fn feeding_in_pieces_changes_nothing() {
        let data: Vec<u8> = (0..10_000_u32).map(|n| (n * 7 % 251) as u8).collect();
        let whole = digest(&data);
        for step in [1, 3, 63, 64, 65, 1000] {
            let mut hasher = Sha256::new();
            for piece in data.chunks(step) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finish(), whole, "pieces of {step}");
        }
    }

    fn wait(hashing: &Hashing) -> HashProgress {
        let started = Instant::now();
        loop {
            let progress = hashing.progress();
            if progress.outcome.is_some() {
                return progress;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "the hash did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_file_is_hashed_in_the_background() {
        let temp = Temp::new("digest-file");
        let file = temp.write("abc.txt", "abc");
        let progress = wait(&Hashing::start(file));
        assert_eq!(
            progress.outcome,
            Some(HashOutcome::Done(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned()
            ))
        );
        assert_eq!((progress.done, progress.total), (3, 3));
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn a_stopped_hash_says_so() {
        let temp = Temp::new("digest-stop");
        let file = temp.write("big", &"x".repeat(4 * CHUNK));
        let hashing = Hashing::start(file);
        hashing.cancel();
        let progress = wait(&hashing);
        // The thread may have finished before it heard the cancel.
        assert!(matches!(
            progress.outcome,
            Some(HashOutcome::Cancelled | HashOutcome::Done(_))
        ));
    }

    #[test]
    fn a_missing_file_and_a_folder_fail_with_a_sentence() {
        let temp = Temp::new("digest-fail");
        let missing = wait(&Hashing::start(temp.path().join("nope")));
        assert_eq!(
            missing.outcome,
            Some(HashOutcome::Failed("That file or folder no longer exists.".to_owned()))
        );
        let folder = wait(&Hashing::start(temp.path().to_path_buf()));
        assert!(matches!(folder.outcome, Some(HashOutcome::Failed(_))));
    }
}
