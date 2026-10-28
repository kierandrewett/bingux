//! A small, bounded, on-disk cache of folder listings.
//!
//! Opening a folder should paint at once, even when its drive is a network
//! share a hundred milliseconds away. So the [`loader`](crate::loader) keeps
//! what it read: one file per folder under
//! `$XDG_CACHE_HOME/bingux/files/v1/`, holding each entry's name, type, size
//! and modified time. The next time the folder is opened the cache is shown
//! first and the folder is checked afterwards (stale while revalidate).
//!
//! # Is a cached folder still right?
//!
//! The file stores the folder's [`DirStamp`]: device, inode, modified time
//! and changed time, as `statx` reports them. A folder whose stamp is the
//! same has had no entry added, removed or renamed, on a local disk and on
//! NFS alike, so nothing needs to be read. (A file rewritten in place does
//! not change the folder's stamp; the sizes of the rows on screen are
//! checked separately.) A different stamp means: read the folder and show
//! only what differs.
//!
//! The key is the folder's path as written, never canonicalised: resolving
//! links walks every parent, and on a network mount each step is a round
//! trip.
//!
//! # Size
//!
//! Names are sorted and stored as the part that differs from the previous
//! name, so `IMG_00001.png`, `IMG_00002.png`, ... cost a few bytes each.
//! About 15 bytes an entry: a 20,000-entry folder is about 300 KB. The cache
//! as a whole holds at most [`DEFAULT_BUDGET`] bytes (32 MiB) and
//! [`DEFAULT_FOLDERS`] folders (512); past either, the folders opened least
//! recently are deleted. A single folder over a quarter of the budget is not
//! cached. Files are written to a temporary name and renamed, so a crash
//! leaves no half-written entry, and a file that fails any check is deleted
//! and treated as a miss.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::entry::{Entry, Kind};
use crate::stat::DirStamp;

/// The most bytes the cache keeps.
pub const DEFAULT_BUDGET: u64 = 32 * 1024 * 1024;
/// The most folders the cache keeps.
pub const DEFAULT_FOLDERS: usize = 512;

/// `BFC1` files stored a name that is not valid UTF-8 as replacement
/// characters and so named no file on disk; they are misses now and are deleted.
const MAGIC: &[u8; 4] = b"BFC2";
const KIND_FILE: u8 = 0;
const KIND_DIR: u8 = 1;
const KIND_OTHER: u8 = 2;
const KIND_MASK: u8 = 0b11;
const SYMLINK: u8 = 1 << 2;
const HIDDEN: u8 = 1 << 3;
const SIZED: u8 = 1 << 4;
const HAS_TIME: u8 = 1 << 5;

/// A folder as the cache remembered it.
#[derive(Clone, Debug)]
pub struct Cached {
    /// The entries, in name order. `sized` is true for the ones whose size
    /// and time were known; the rest of their fields are empty.
    pub entries: Vec<Entry>,
    /// The folder's stamp when the entries were read.
    pub stamp: DirStamp,
}

/// The cache. Cheap to create; holds no open files.
#[derive(Debug)]
pub struct Cache {
    root: PathBuf,
    budget: u64,
    max_folders: usize,
    /// Bytes stored since the last sweep, so a sweep runs when the cache may
    /// have grown by an eighth of its budget, not on every store.
    since_sweep: AtomicU64,
    swept_once: std::sync::atomic::AtomicBool,
}

impl Cache {
    /// A cache in `root`, bounded by `budget` bytes and `max_folders` files.
    pub fn at(root: impl Into<PathBuf>, budget: u64, max_folders: usize) -> Cache {
        Cache {
            root: root.into(),
            budget,
            max_folders,
            since_sweep: AtomicU64::new(0),
            swept_once: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The cache of this user: `$XDG_CACHE_HOME/bingux/files/v1`, with the
    /// default bounds.
    pub fn open_default() -> Cache {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("bingux-cache"));
        Cache::at(
            base.join("bingux/files/v1"),
            DEFAULT_BUDGET,
            DEFAULT_FOLDERS,
        )
    }

    /// The process-wide cache of this user.
    pub fn shared() -> Arc<Cache> {
        static SHARED: LazyLock<Arc<Cache>> = LazyLock::new(|| Arc::new(Cache::open_default()));
        SHARED.clone()
    }

    /// The stamp stored for `dir`, reading only the head of its file. Not
    /// checked against the folder's path: use [`Cache::load`] for the content.
    pub fn peek_stamp(&self, dir: &Path) -> Option<DirStamp> {
        let mut head = [0u8; 52];
        fs::File::open(self.file_for(dir)).ok()?.read_exact(&mut head).ok()?;
        if &head[..4] != MAGIC {
            return None;
        }
        let i128_at = |at: usize| i128::from_le_bytes(head[at..at + 16].try_into().unwrap_or([0; 16]));
        let u64_at = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().unwrap_or([0; 8]));
        Some(DirStamp {
            modified_ns: i128_at(4),
            changed_ns: i128_at(20),
            dev: u64_at(36),
            ino: u64_at(44),
        })
    }

    fn file_for(&self, dir: &Path) -> PathBuf {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in dir.as_os_str().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.root.join(format!("{hash:016x}.bfc"))
    }

    /// The remembered entries of `dir`, or `None` when there are none (or
    /// they are damaged: such a file is deleted).
    pub fn load(&self, dir: &Path) -> Option<Cached> {
        let bytes = self.read(dir)?;
        let cached = decode(&bytes, dir);
        if cached.is_none() {
            self.discard_if_damaged(dir, &bytes);
        }
        cached
    }

    /// The raw file of `dir`, for [`Cache::decode`]. Lets a caller decode the
    /// same bytes twice instead of copying the entries.
    pub fn read(&self, dir: &Path) -> Option<Vec<u8>> {
        let file = self.file_for(dir);
        let mut handle = fs::File::open(&file).ok()?;
        let meta = handle.metadata().ok()?;
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        handle.read_to_end(&mut bytes).ok()?;
        // Opened recently: move it to the safe end of the eviction order, at
        // most every few minutes.
        let stale = meta
            .modified()
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_none_or(|age| age > Duration::from_secs(300));
        if stale {
            let _ = handle.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// The entries in `bytes`, as [`Cache::read`] returned them for `dir`.
    pub fn decode(&self, bytes: &[u8], dir: &Path) -> Option<Cached> {
        decode(bytes, dir)
    }

    /// Delete the file of `dir` when `bytes` is not a file for it that is
    /// merely a hash neighbour's.
    pub fn discard_if_damaged(&self, dir: &Path, bytes: &[u8]) {
        // A file for another folder that hashes alike is left alone.
        let other = stored_path(bytes).is_some_and(|path| path != dir);
        if !other {
            let _ = fs::remove_file(self.file_for(dir));
        }
    }

    /// Remember `entries` as the content of `dir` at `stamp`. Failures are
    /// ignored: a cache that cannot be written is only a slower start.
    pub fn store(&self, dir: &Path, stamp: &DirStamp, entries: &[Entry]) {
        let bytes = encode(dir, stamp, entries);
        if bytes.len() as u64 > self.budget / 4 {
            self.forget(dir);
            return;
        }
        if fs::create_dir_all(&self.root).is_err() {
            return;
        }
        let file = self.file_for(dir);
        let temporary = file.with_extension(format!("tmp{}", std::process::id()));
        let written = fs::File::create(&temporary).and_then(|mut handle| handle.write_all(&bytes));
        if written.is_err() || fs::rename(&temporary, &file).is_err() {
            let _ = fs::remove_file(&temporary);
            return;
        }
        let total = self.since_sweep.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
        if total > self.budget / 8 || !self.swept_once.swap(true, Ordering::Relaxed) {
            self.since_sweep.store(0, Ordering::Relaxed);
            self.sweep();
        }
    }

    /// Delete the remembered content of `dir`.
    pub fn forget(&self, dir: &Path) {
        let _ = fs::remove_file(self.file_for(dir));
    }

    /// Delete everything.
    pub fn clear(&self) {
        if let Ok(reader) = fs::read_dir(&self.root) {
            for item in reader.flatten() {
                let _ = fs::remove_file(item.path());
            }
        }
    }

    /// How many bytes and files the cache holds now.
    pub fn disk_usage(&self) -> (u64, usize) {
        let mut bytes = 0;
        let mut files = 0;
        if let Ok(reader) = fs::read_dir(&self.root) {
            for item in reader.flatten() {
                if item.path().extension().is_some_and(|ext| ext == "bfc")
                    && let Ok(meta) = item.metadata()
                {
                    bytes += meta.len();
                    files += 1;
                }
            }
        }
        (bytes, files)
    }

    /// Delete the least recently used folders until the cache is inside its
    /// bounds, and any temporary file left by a crash.
    pub fn sweep(&self) {
        let Ok(reader) = fs::read_dir(&self.root) else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for item in reader.flatten() {
            let path = item.path();
            let Ok(meta) = item.metadata() else { continue };
            let modified = meta.modified().unwrap_or(UNIX_EPOCH);
            if path.extension().is_some_and(|ext| ext == "bfc") {
                files.push((modified, meta.len(), path));
            } else if modified.elapsed().is_ok_and(|age| age > Duration::from_secs(3600)) {
                let _ = fs::remove_file(path);
            }
        }
        files.sort_by_key(|(modified, _, _)| *modified);
        let mut bytes: u64 = files.iter().map(|(_, len, _)| len).sum();
        let mut count = files.len();
        for (_, len, path) in files {
            if bytes <= self.budget && count <= self.max_folders {
                break;
            }
            if fs::remove_file(path).is_ok() {
                bytes -= len;
                count -= 1;
            }
        }
    }
}

// ---- encoding --------------------------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

fn seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

/// The bytes of the file an entry names: the last part of its path. Not
/// `Entry::name`, the text it is shown as, which writes a byte that is not
/// valid UTF-8 as U+FFFD: two files could share it, and the cache would hand
/// back a path that is on no disk.
fn name_bytes(entry: &Entry) -> &[u8] {
    entry
        .path
        .file_name()
        .map_or_else(|| entry.name.as_bytes(), |name| name.as_bytes())
}

fn encode(dir: &Path, stamp: &DirStamp, entries: &[Entry]) -> Vec<u8> {
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_unstable_by(|a, b| name_bytes(a).cmp(name_bytes(b)));
    let mut out = Vec::with_capacity(64 + entries.len() * 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&stamp.modified_ns.to_le_bytes());
    out.extend_from_slice(&stamp.changed_ns.to_le_bytes());
    out.extend_from_slice(&stamp.dev.to_le_bytes());
    out.extend_from_slice(&stamp.ino.to_le_bytes());
    let path = dir.as_os_str().as_bytes();
    put_varint(&mut out, path.len() as u64);
    out.extend_from_slice(path);
    put_varint(&mut out, order.len() as u64);
    let base = (stamp.modified_ns / 1_000_000_000) as i64;
    let mut previous: &[u8] = &[];
    for entry in order {
        let name = name_bytes(entry);
        let shared = previous
            .iter()
            .zip(name)
            .take_while(|(a, b)| a == b)
            .count();
        put_varint(&mut out, shared as u64);
        put_varint(&mut out, (name.len() - shared) as u64);
        out.extend_from_slice(&name[shared..]);
        let mut flags = match entry.kind {
            Kind::File => KIND_FILE,
            Kind::Directory => KIND_DIR,
            Kind::Other => KIND_OTHER,
        };
        if entry.symlink {
            flags |= SYMLINK;
        }
        if entry.hidden {
            flags |= HIDDEN;
        }
        if entry.sized {
            flags |= SIZED;
            if entry.modified.is_some() {
                flags |= HAS_TIME;
            }
        }
        out.push(flags);
        if entry.sized {
            put_varint(&mut out, entry.size);
            if let Some(modified) = entry.modified {
                put_varint(&mut out, zigzag(seconds(modified) - base));
            }
        }
        previous = name;
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

/// The folder path a cache file says it is for.
fn stored_path(bytes: &[u8]) -> Option<PathBuf> {
    let mut reader = Reader { bytes, at: 4 + 16 + 16 + 8 + 8 };
    let length = reader.varint()? as usize;
    let path = reader.take(length)?;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(path)))
}

fn decode(bytes: &[u8], dir: &Path) -> Option<Cached> {
    use std::ffi::OsStr;
    if bytes.get(..4)? != MAGIC {
        return None;
    }
    let mut reader = Reader { bytes, at: 4 };
    let i128_at = |reader: &mut Reader<'_>| -> Option<i128> {
        Some(i128::from_le_bytes(reader.take(16)?.try_into().ok()?))
    };
    let u64_at = |reader: &mut Reader<'_>| -> Option<u64> {
        Some(u64::from_le_bytes(reader.take(8)?.try_into().ok()?))
    };
    let stamp = DirStamp {
        modified_ns: i128_at(&mut reader)?,
        changed_ns: i128_at(&mut reader)?,
        dev: u64_at(&mut reader)?,
        ino: u64_at(&mut reader)?,
    };
    let path_length = reader.varint()? as usize;
    if reader.take(path_length)? != dir.as_os_str().as_bytes() {
        return None;
    }
    let count = reader.varint()? as usize;
    if count > bytes.len() {
        return None;
    }
    let base = (stamp.modified_ns / 1_000_000_000) as i64;
    let special = crate::places::system_user_dirs().children_of(dir);
    let mut entries = Vec::with_capacity(count);
    let mut name: Vec<u8> = Vec::new();
    for _ in 0..count {
        let shared = reader.varint()? as usize;
        let extra = reader.varint()? as usize;
        if shared > name.len() {
            return None;
        }
        name.truncate(shared);
        name.extend_from_slice(reader.take(extra)?);
        let flags = reader.byte()?;
        let kind = match flags & KIND_MASK {
            KIND_FILE => Kind::File,
            KIND_DIR => Kind::Directory,
            KIND_OTHER => Kind::Other,
            _ => return None,
        };
        let text = String::from_utf8_lossy(&name).into_owned();
        let os_name = OsStr::from_bytes(&name);
        let mut entry = Entry::named(
            dir.join(os_name),
            text,
            kind,
            flags & SYMLINK != 0,
            flags & HIDDEN != 0,
        );
        if !special.is_empty() {
            entry.special = special
                .iter()
                .find(|(special_name, _)| special_name.as_bytes() == name.as_slice())
                .map(|(_, kind)| *kind);
        }
        if flags & SIZED != 0 {
            entry.size = reader.varint()?;
            if flags & HAS_TIME != 0 {
                let secs = base.checked_add(unzigzag(reader.varint()?))?;
                entry.modified = Some(if secs >= 0 {
                    UNIX_EPOCH + Duration::from_secs(secs as u64)
                } else {
                    UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))?
                });
            }
            entry.sized = true;
        }
        entries.push(entry);
    }
    Some(Cached { entries, stamp })
}

/// Names of the entries of a cached folder, for completion: no entry is
/// built, so a long folder costs one pass over the bytes.
pub fn cached_names(cache: &Cache, dir: &Path) -> Option<(Vec<(String, bool)>, DirStamp)> {
    let cached = cache.load(dir)?;
    let names = cached
        .entries
        .into_iter()
        .map(|entry| {
            let is_dir = entry.is_dir();
            (entry.name, is_dir)
        })
        .collect();
    Some((names, cached.stamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-files-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn stamp(seed: i128) -> DirStamp {
        DirStamp {
            modified_ns: 1_700_000_000_000_000_000 + seed,
            changed_ns: 1_700_000_001_000_000_000,
            dev: 7,
            ino: 42,
        }
    }

    fn entries(dir: &Path, count: usize) -> Vec<Entry> {
        (0..count)
            .map(|i| {
                let name = format!("IMG_{i:05}.png");
                let mut entry = Entry::named(
                    dir.join(&name),
                    name,
                    if i % 10 == 0 { Kind::Directory } else { Kind::File },
                    i % 7 == 0,
                    i % 13 == 0,
                );
                entry.size = (i as u64) * 37;
                entry.modified = Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000 - (i as u64) * 91));
                entry.sized = true;
                entry
            })
            .collect()
    }

    #[test]
    fn round_trips_entries_and_stamp() {
        let root = scratch("roundtrip");
        let cache = Cache::at(&root, DEFAULT_BUDGET, DEFAULT_FOLDERS);
        let dir = Path::new("/data/Pictures");
        let mut written = entries(dir, 300);
        written[5].sized = false;
        written[5].size = 0;
        written[5].modified = None;
        cache.store(dir, &stamp(5), &written);
        let cached = cache.load(dir).unwrap();
        assert_eq!(cached.stamp, stamp(5));
        assert_eq!(cached.entries.len(), 300);
        let mut expected = written.clone();
        expected.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(cached.entries, expected);
        assert!(cache.load(Path::new("/data/Other")).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_changed_stamp_is_visible_to_the_caller() {
        let root = scratch("stamp");
        let cache = Cache::at(&root, DEFAULT_BUDGET, DEFAULT_FOLDERS);
        let dir = Path::new("/data/Docs");
        cache.store(dir, &stamp(1), &entries(dir, 3));
        assert_ne!(cache.load(dir).unwrap().stamp, stamp(2));
        // Storing again replaces the old content.
        cache.store(dir, &stamp(2), &entries(dir, 4));
        let cached = cache.load(dir).unwrap();
        assert_eq!((cached.stamp, cached.entries.len()), (stamp(2), 4));
        cache.forget(dir);
        assert!(cache.load(dir).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn damaged_files_are_misses_and_are_deleted() {
        let root = scratch("damaged");
        let cache = Cache::at(&root, DEFAULT_BUDGET, DEFAULT_FOLDERS);
        let dir = Path::new("/data/Broken");
        cache.store(dir, &stamp(1), &entries(dir, 50));
        let file = cache.file_for(dir);
        let mut bytes = fs::read(&file).unwrap();
        bytes.truncate(bytes.len() / 2);
        fs::write(&file, &bytes).unwrap();
        assert!(cache.load(dir).is_none());
        assert!(!file.exists());
        fs::write(&file, b"nonsense").unwrap();
        assert!(cache.load(dir).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn evicts_the_least_recently_used_folders_by_count() {
        let root = scratch("count");
        let cache = Cache::at(&root, DEFAULT_BUDGET, 1000);
        for index in 0..6 {
            let dir = PathBuf::from(format!("/data/folder{index}"));
            cache.store(&dir, &stamp(1), &entries(&dir, 10));
            // Distinct modification times decide the order.
            let file = cache.file_for(&dir);
            let time = SystemTime::now() - Duration::from_secs(100 - index as u64 * 10);
            fs::File::options().write(true).open(file).unwrap().set_modified(time).unwrap();
        }
        let cache = Cache::at(&root, DEFAULT_BUDGET, 3);
        cache.sweep();
        let (_, files) = cache.disk_usage();
        assert_eq!(files, 3);
        for index in 0..3 {
            assert!(cache.load(Path::new(&format!("/data/folder{index}"))).is_none());
        }
        for index in 3..6 {
            assert!(cache.load(Path::new(&format!("/data/folder{index}"))).is_some());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn evicts_by_size_and_refuses_a_folder_that_is_too_big() {
        let root = scratch("bytes");
        let probe = encode(Path::new("/data/a"), &stamp(1), &entries(Path::new("/data/a"), 200)).len() as u64;
        // Room for about two such folders, and a quarter of it is too small
        // for a folder ten times bigger.
        let cache = Cache::at(&root, probe * 100, DEFAULT_FOLDERS);
        for index in 0..5 {
            let dir = PathBuf::from(format!("/data/{}", (b'a' + index) as char));
            cache.store(&dir, &stamp(1), &entries(&dir, 200));
            let file = cache.file_for(&dir);
            let time = SystemTime::now() - Duration::from_secs(100 - u64::from(index) * 10);
            fs::File::options().write(true).open(file).unwrap().set_modified(time).unwrap();
        }
        let cache = Cache::at(&root, probe * 2 + probe / 2, DEFAULT_FOLDERS);
        cache.sweep();
        let (bytes, files) = cache.disk_usage();
        assert!(bytes <= probe * 2 + probe / 2, "{bytes} bytes");
        assert_eq!(files, 2);
        let huge = Path::new("/data/huge");
        cache.store(huge, &stamp(1), &entries(huge, 2000));
        assert!(cache.load(huge).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn twenty_thousand_entries_are_small() {
        let dir = Path::new("/data/big");
        let bytes = encode(dir, &stamp(1), &entries(dir, 20_000)).len();
        assert!(bytes < 400_000, "{bytes} bytes");
    }

    #[test]
    fn varints_round_trip() {
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            let mut out = Vec::new();
            put_varint(&mut out, value);
            assert_eq!(Reader { bytes: &out, at: 0 }.varint(), Some(value));
        }
        for value in [0i64, 1, -1, 1000, -1000, i64::MAX, i64::MIN] {
            assert_eq!(unzigzag(zigzag(value)), value);
        }
    }

    #[test]
    fn a_name_that_is_not_utf8_comes_back_as_the_same_file() {
        use std::ffi::OsStr;
        let root = scratch("odd-names");
        let cache = Cache::at(&root, DEFAULT_BUDGET, DEFAULT_FOLDERS);
        let dir = Path::new("/data/Odd");
        let named = |bytes: &[u8], kind: Kind| {
            let file = OsStr::from_bytes(bytes);
            Entry::named(dir.join(file), file.to_string_lossy().into_owned(), kind, false, false)
        };
        let written = vec![
            named(b"caf\xe9-\xff", Kind::Directory),
            named(b"caf\xe9-\xfe", Kind::Directory),
            named(b"plain", Kind::File),
        ];
        // The two folders are shown alike, and are two folders.
        assert_eq!(written[0].name, written[1].name);
        assert_ne!(written[0].path, written[1].path);
        cache.store(dir, &stamp(1), &written);
        let cached = cache.load(dir).unwrap();
        let mut found: Vec<PathBuf> = cached.entries.iter().map(|entry| entry.path.clone()).collect();
        let mut expected: Vec<PathBuf> = written.iter().map(|entry| entry.path.clone()).collect();
        found.sort();
        expected.sort();
        assert_eq!(found, expected);
        // Their names read back as the text they are shown as.
        for entry in &cached.entries {
            let shown = entry.path.file_name().unwrap().to_string_lossy();
            assert_eq!(entry.name, shown);
        }
        let _ = fs::remove_dir_all(&root);
    }
}
