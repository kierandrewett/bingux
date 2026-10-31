//! Copy throughput, before and after the transfer engine was rewritten.
//!
//! ```sh
//! cargo run --release --example copy_bench -- [DIR] [--to DEST] [--big-mib 1024] [--files 20000] [--runs 3]
//! ```
//!
//! `DIR` (default `/dev/shm`) is where the test data is made and copied; use a
//! folder on the file system you want to measure; `--to DEST` copies to another
//! folder (another drive, to measure a copy between drives). Two cases, each run a few
//! times with the destination emptied in between:
//!
//! 1. one big file (`--big-mib`, default 1 GiB);
//! 2. many small files (`--files`, default 20 000, 1 to 16 KiB each, 200 per
//!    folder).
//!
//! "old" is the previous engine, kept here as a baseline: one thread, data
//! written straight to the final name, `FICLONE` then `copy_file_range` in
//! 4 MiB steps. "new" is [`bingux_files::ops::copy`]: part file then rename,
//! `FICLONE` / `copy_file_range` / `fallocate`, and a few threads for small
//! files. Neither flushes: this measures an ordinary disk. The flush on
//! removable media is a drive operation and cannot be measured here.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bingux_files::Cancel;
use bingux_files::ops::{ConflictPolicy, Progress, copy};

// ---- the old engine, as it was -------------------------------------------------

fn old_copy_file(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::metadata(from)?;
    let mut input = File::open(from)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(meta.mode() & 0o777)
        .open(to)?;
    let mut done = false;
    if meta.len() > 0 {
        // SAFETY: both descriptors are open for the whole call.
        done = unsafe { libc::ioctl(output.as_raw_fd(), libc::FICLONE, input.as_raw_fd()) } == 0;
    }
    if !done {
        let mut kernel = true;
        let mut buffer = vec![0u8; 256 << 10];
        loop {
            if kernel {
                // SAFETY: null offsets make the kernel use the descriptors' own.
                let copied = unsafe {
                    libc::copy_file_range(
                        input.as_raw_fd(),
                        std::ptr::null_mut(),
                        output.as_raw_fd(),
                        std::ptr::null_mut(),
                        4 << 20,
                        0,
                    )
                };
                if copied < 0 {
                    kernel = false;
                    continue;
                }
                if copied == 0 {
                    break;
                }
            } else {
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                output.write_all(&buffer[..read])?;
            }
        }
    }
    output.set_permissions(fs::Permissions::from_mode(meta.mode() & 0o7777))?;
    output.set_times(
        fs::FileTimes::new()
            .set_accessed(meta.accessed()?)
            .set_modified(meta.modified()?),
    )?;
    Ok(())
}

fn old_copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    if from.symlink_metadata()?.is_dir() {
        fs::create_dir(to)?;
        let mut children: Vec<PathBuf> = fs::read_dir(from)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<_>>()?;
        children.sort();
        for child in children {
            old_copy_tree(&child, &to.join(child.file_name().unwrap()))?;
        }
        Ok(())
    } else {
        old_copy_file(from, to)
    }
}

// ---- the measurement ---------------------------------------------------------------

fn new_copy(from: &Path, into: &Path) {
    copy(
        &[from.to_path_buf()],
        into,
        &ConflictPolicy::Skip,
        &Progress::new(),
        &Cancel::new(),
    )
    .unwrap();
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn fill(path: &Path, bytes: u64) {
    let mut file = File::create(path).unwrap();
    // Not compressible and not all zeros, so no file system can cheat.
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut block = vec![0u8; 1 << 20];
    let mut left = bytes;
    while left > 0 {
        for chunk in block.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            chunk.copy_from_slice(&state.to_le_bytes()[..chunk.len()]);
        }
        let take = left.min(block.len() as u64) as usize;
        file.write_all(&block[..take]).unwrap();
        left -= take as u64;
    }
}

fn make_small_files(root: &Path, count: usize) -> u64 {
    let mut total = 0;
    let mut state = 88172645463325252u64;
    for n in 0..count {
        let dir = root.join(format!("d{:03}", n / 100));
        fs::create_dir_all(&dir).unwrap();
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let length = 1024 + (state % (15 * 1024)) as usize;
        let data: Vec<u8> = (0..length).map(|i| (i as u64 ^ state) as u8).collect();
        fs::write(dir.join(format!("f{n:05}.dat")), data).unwrap();
        total += length as u64;
    }
    total
}

fn report(case: &str, who: &str, times: &[Duration], bytes: u64, files: usize) {
    let best = *times.iter().min().unwrap();
    let mid = median(times.to_vec());
    println!(
        "{case:<22} {who:<4} median {:>8.1} ms  best {:>8.1} ms  {:>8.1} MiB/s  {:>9.0} files/s",
        mid.as_secs_f64() * 1e3,
        best.as_secs_f64() * 1e3,
        bytes as f64 / (1 << 20) as f64 / mid.as_secs_f64(),
        files as f64 / mid.as_secs_f64()
    );
}

fn compare(name: &str, source: &Path, bytes: u64, files: usize, dest: &Path, runs: usize) {
    let (mut old, mut new) = (Vec::new(), Vec::new());
    // Alternate, so neither always gets the warm cache.
    for _ in 0..runs {
        let target = dest.join("old");
        let _ = fs::remove_dir_all(&target);
        fs::create_dir(&target).unwrap();
        let start = Instant::now();
        old_copy_tree(source, &target.join(source.file_name().unwrap())).unwrap();
        old.push(start.elapsed());
        fs::remove_dir_all(&target).unwrap();

        let target = dest.join("new");
        let _ = fs::remove_dir_all(&target);
        fs::create_dir(&target).unwrap();
        let start = Instant::now();
        new_copy(source, &target);
        new.push(start.elapsed());
        // The copy is the same one.
        assert_eq!(
            fs::read_dir(&target).unwrap().count(),
            1,
            "one item arrives"
        );
        fs::remove_dir_all(&target).unwrap();
    }
    report(name, "old", &old, bytes, files);
    report(name, "new", &new, bytes, files);
    let (o, n) = (median(old).as_secs_f64(), median(new).as_secs_f64());
    println!(
        "{name:<22} new takes {:.0}% of the time of old",
        100.0 * n / o
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str, default: u64| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .and_then(|text| text.parse().ok())
            .unwrap_or(default)
    };
    let dir = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .map_or_else(|| PathBuf::from("/dev/shm"), PathBuf::from);
    let big_mib = value("--big-mib", 1024);
    let files = value("--files", 20_000) as usize;
    let runs = value("--runs", 3) as usize;

    let work = dir.join(format!("bingux-copy-bench-{}", std::process::id()));
    fs::create_dir_all(work.join("src")).unwrap();
    // Where the copies go: beside the source, or on another drive.
    let dest = args
        .iter()
        .position(|arg| arg == "--to")
        .and_then(|at| args.get(at + 1))
        .map_or_else(
            || work.clone(),
            |to| PathBuf::from(to).join(format!("bingux-copy-bench-{}-to", std::process::id())),
        );
    fs::create_dir_all(&dest).unwrap();
    println!("== {} -> {} (runs: {runs})", dir.display(), dest.display());

    let big = work.join("src/big.bin");
    fill(&big, big_mib << 20);
    compare(
        &format!("one {big_mib} MiB file"),
        &big,
        big_mib << 20,
        1,
        &dest,
        runs,
    );
    fs::remove_file(&big).unwrap();

    let tree = work.join("src/small");
    let bytes = make_small_files(&tree, files);
    compare(
        &format!("{files} small files"),
        &tree,
        bytes,
        files + files / 100 + 1,
        &dest,
        runs,
    );
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir_all(&dest);
}
