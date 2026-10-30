//! Tests of what moves and copies promise: atomic renames, nothing removed
//! before it is safe, part files, and the order of the flushes. The kernel's
//! flushes go to a recorder, which can also fail or interfere at any call.

use super::super::durable::recorder::{Call, Recorder, stick};
use super::super::testing::Temp;
use super::*;
use std::sync::{Arc, Mutex};

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap()
}

fn engine<'a>(
    recorder: &'a Recorder,
    policy: &'a Policy,
    workers: usize,
    cross_drive: bool,
) -> Engine<'a> {
    Engine {
        flusher: recorder,
        policy,
        force_cross_drive: cross_drive,
        workers: Some(workers),
    }
}

/// Every `.part` file under `dir`.
fn part_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).unwrap().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else if path.to_string_lossy().ends_with(".part") {
                found.push(path);
            }
        }
    }
    found
}

fn lazy(_: &Path) -> Level {
    Level::Lazy
}

fn run(
    engine: &Engine<'_>,
    moving: bool,
    paths: &[PathBuf],
    dest: &Path,
    policy: &ConflictPolicy,
) -> Result<Vec<PathBuf>, OpError> {
    let (progress, cancel) = (Progress::new(), Cancel::new());
    if moving {
        move_in(engine, paths, dest, policy, &progress, &cancel)
    } else {
        copy_in(engine, paths, dest, policy, &progress, &cancel)
    }
}

#[test]
fn a_move_on_one_drive_is_one_rename() {
    let temp = Temp::new("atomic-move");
    let file = temp.write("src/a.txt", "a");
    let dir = temp.dir("src/d");
    temp.write("src/d/inner.txt", "i");
    let (file_inode, dir_inode) = (
        fs::metadata(&file).unwrap().ino(),
        fs::metadata(&dir).unwrap().ino(),
    );
    let dest = temp.dir("dest");
    let recorder = Recorder::default();
    let done = run(
        &engine(&recorder, &lazy, 1, false),
        true,
        &[file, dir],
        &dest,
        &ConflictPolicy::Skip,
    )
    .unwrap();
    assert_eq!(done, [dest.join("a.txt"), dest.join("d")]);
    // The very same inodes: nothing was copied, nothing was written.
    assert_eq!(fs::metadata(dest.join("a.txt")).unwrap().ino(), file_inode);
    assert_eq!(fs::metadata(dest.join("d")).unwrap().ino(), dir_inode);
    assert!(part_files(temp.path()).is_empty());
    assert!(recorder.calls().is_empty(), "an internal disk is not flushed");
}

#[test]
fn a_move_on_a_device_writes_out_both_folders_it_changed() {
    let temp = Temp::new("atomic-device-move");
    let file = temp.write("stick/a/f.txt", "f");
    let to = temp.dir("stick/b");
    let recorder = Recorder::default();
    let level = stick(&temp.path().join("stick"));
    let policy = move |_: &Path| level.clone();
    run(
        &engine(&recorder, &policy, 1, false),
        true,
        &[file],
        &to,
        &ConflictPolicy::Skip,
    )
    .unwrap();
    let calls = recorder.calls();
    assert!(calls.contains(&Call::Dir(temp.path().join("stick/a"))), "{calls:?}");
    assert!(calls.contains(&Call::Dir(temp.path().join("stick/b"))), "{calls:?}");
    assert_eq!(calls.last(), Some(&Call::Filesystem(temp.path().join("stick"))));
    assert!(
        !calls.iter().any(|call| matches!(call, Call::File(_))),
        "no data was written, so no file is synced"
    );
}

#[test]
fn a_move_never_overwrites_a_name_it_was_not_told_to() {
    let temp = Temp::new("atomic-clash");
    let src = temp.write("src/a.txt", "new");
    let dest = temp.dir("dest");
    temp.write("dest/a.txt", "old");
    for cross in [false, true] {
        let recorder = Recorder::default();
        let done = run(
            &engine(&recorder, &lazy, 1, cross),
            true,
            &[src.clone()],
            &dest,
            &ConflictPolicy::Skip,
        )
        .unwrap();
        assert!(done.is_empty());
        assert_eq!(read(dest.join("a.txt")), "old", "cross: {cross}");
        assert_eq!(read(&src), "new", "cross: {cross}");
    }
}

/// Sources, and a destination called the stick, for the tests of a move
/// between drives. They run on one drive and are told it is two.
struct Cross {
    temp: Temp,
    sources: Vec<PathBuf>,
    dest: PathBuf,
}

fn cross() -> Cross {
    let temp = Temp::new("xdrive");
    let dir = temp.dir("src/d");
    temp.write("src/d/a.txt", "aaaa");
    temp.write("src/d/sub/b.txt", "bb");
    let file = temp.write("src/f.txt", "ffff");
    let dest = temp.dir("stick");
    Cross {
        sources: vec![dir, file],
        dest,
        temp,
    }
}

#[test]
fn a_move_between_drives_flushes_then_checks_then_removes_the_source() {
    let case = cross();
    let recorder = Recorder::default();
    let source_file = case.temp.path().join("src/f.txt");
    let source_inner = case.temp.path().join("src/d/a.txt");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    recorder.on_call(move |call| {
        if matches!(call, Call::Filesystem(_)) {
            sink.lock()
                .unwrap()
                .push(source_file.exists() && source_inner.exists());
        }
        Ok(())
    });
    let level = stick(&case.dest);
    let policy = move |_: &Path| level.clone();
    run(
        &engine(&recorder, &policy, 1, true),
        true,
        &case.sources,
        &case.dest,
        &ConflictPolicy::Skip,
    )
    .unwrap();

    // The drive was written out, once for each item, with the sources all
    // still there.
    {
        let seen = seen.lock().unwrap();
        // The first write-out of the first item happens with its sources in
        // place; the last, after every source was removed, finds them gone.
        assert_eq!(seen.first(), Some(&true));
        assert_eq!(seen.last(), Some(&false));
    }
    let calls = recorder.calls();
    let first_dir = calls.iter().position(|c| matches!(c, Call::Dir(_))).unwrap();
    let last_file_before_sync = calls
        .iter()
        .position(|c| matches!(c, Call::Filesystem(_)))
        .map(|sync| {
            calls[..sync]
                .iter()
                .rposition(|c| matches!(c, Call::File(_)))
                .unwrap()
        })
        .unwrap();
    assert!(last_file_before_sync < first_dir, "{calls:?}");
    assert_eq!(read(case.dest.join("d/sub/b.txt")), "bb");
    assert_eq!(read(case.dest.join("f.txt")), "ffff");
    assert!(!case.sources[0].exists() && !case.sources[1].exists());
    assert!(part_files(case.temp.path()).is_empty());
}

#[test]
fn a_drive_that_cannot_be_written_out_keeps_every_source() {
    let case = cross();
    let recorder = Recorder::default();
    recorder.on_call(|call| match call {
        Call::Filesystem(_) => Err(io::Error::from_raw_os_error(libc::EIO)),
        _ => Ok(()),
    });
    let level = stick(&case.dest);
    let policy = move |_: &Path| level.clone();
    let error = run(
        &engine(&recorder, &policy, 1, true),
        true,
        &case.sources,
        &case.dest,
        &ConflictPolicy::Skip,
    )
    .unwrap_err();
    assert!(error.to_string().contains("STICK"), "{error}");
    assert_eq!(read(case.temp.path().join("src/d/a.txt")), "aaaa");
    assert_eq!(read(case.temp.path().join("src/d/sub/b.txt")), "bb");
    assert!(case.sources[0].is_dir(), "the folder stays too");
    assert!(part_files(case.temp.path()).is_empty());
}

#[test]
fn a_write_that_fails_part_way_keeps_the_source_and_leaves_no_part_file() {
    let case = cross();
    let recorder = Recorder::default();
    let count = std::sync::atomic::AtomicU32::new(0);
    recorder.on_call(move |call| match call {
        // The second file to be made durable fails, as a full drive would.
        Call::File(_) if count.fetch_add(1, Ordering::SeqCst) == 1 => {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }
        _ => Ok(()),
    });
    let level = stick(&case.dest);
    let policy = move |_: &Path| level.clone();
    let error = run(
        &engine(&recorder, &policy, 1, true),
        true,
        &case.sources,
        &case.dest,
        &ConflictPolicy::Skip,
    )
    .unwrap_err();
    assert!(matches!(error, OpError::Other(_)), "{error:?}");
    assert_eq!(read(case.temp.path().join("src/d/a.txt")), "aaaa");
    assert_eq!(read(case.temp.path().join("src/d/sub/b.txt")), "bb");
    assert_eq!(read(case.temp.path().join("src/f.txt")), "ffff");
    assert!(part_files(case.temp.path()).is_empty());
}

#[test]
fn a_short_copy_is_caught_before_the_source_is_removed() {
    let case = cross();
    let recorder = Recorder::default();
    let victim = case.dest.join("f.txt");
    recorder.on_call(move |call| {
        if matches!(call, Call::Filesystem(_)) && victim.exists() {
            // The drive lost the tail of a file.
            fs::OpenOptions::new()
                .write(true)
                .open(&victim)
                .unwrap()
                .set_len(1)
                .unwrap();
        }
        Ok(())
    });
    let level = stick(&case.dest);
    let policy = move |_: &Path| level.clone();
    let error = run(
        &engine(&recorder, &policy, 1, true),
        true,
        &case.sources,
        &case.dest,
        &ConflictPolicy::Skip,
    )
    .unwrap_err();
    assert!(error.to_string().contains("did not arrive complete"), "{error}");
    assert_eq!(read(case.temp.path().join("src/f.txt")), "ffff");
}

#[test]
fn a_source_that_changes_while_it_is_moved_is_kept() {
    let case = cross();
    let recorder = Recorder::default();
    let touched = case.temp.path().join("src/f.txt");
    recorder.on_call(move |call| {
        if matches!(call, Call::Filesystem(_)) && touched.exists() {
            fs::write(&touched, "ffff and more, written meanwhile").unwrap();
        }
        Ok(())
    });
    let level = stick(&case.dest);
    let policy = move |_: &Path| level.clone();
    let error = run(
        &engine(&recorder, &policy, 1, true),
        true,
        &case.sources,
        &case.dest,
        &ConflictPolicy::Skip,
    )
    .unwrap_err();
    assert!(error.to_string().contains("changed while"), "{error}");
    assert_eq!(
        read(case.temp.path().join("src/f.txt")),
        "ffff and more, written meanwhile"
    );
}

#[test]
fn a_move_between_drives_keeps_what_was_skipped() {
    let temp = Temp::new("xdrive-merge");
    let src = temp.dir("src/d");
    temp.write("src/d/clash.txt", "src clash");
    temp.write("src/d/fresh.txt", "fresh");
    let dest = temp.dir("dest");
    temp.write("dest/d/clash.txt", "dest clash");
    let policy = ConflictPolicy::Ask(Box::new(|conflict| {
        if conflict.existing.is_dir() {
            Resolution::Replace
        } else {
            Resolution::Skip
        }
    }));
    let recorder = Recorder::default();
    run(&engine(&recorder, &lazy, 1, true), true, &[src.clone()], &dest, &policy).unwrap();
    assert_eq!(read(dest.join("d/fresh.txt")), "fresh");
    assert_eq!(read(dest.join("d/clash.txt")), "dest clash");
    assert!(!src.join("fresh.txt").exists());
    assert_eq!(read(src.join("clash.txt")), "src clash", "skipped, so kept");
}

#[test]
fn a_file_has_no_real_name_until_it_is_whole() {
    let temp = Temp::new("temp-name");
    let a = temp.write("src/a.txt", "alpha");
    let dest = temp.dir("stick");
    let recorder = Recorder::default();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (sink, watched) = (seen.clone(), dest.join("a.txt"));
    recorder.on_call(move |call| {
        if let Call::File(name) = call {
            sink.lock()
                .unwrap()
                .push((name.clone(), watched.symlink_metadata().is_ok()));
        }
        Ok(())
    });
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    run(
        &engine(&recorder, &policy, 1, false),
        false,
        &[a],
        &dest,
        &ConflictPolicy::Skip,
    )
    .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let (name, real_name_existed) = &seen[0];
    assert!(name.starts_with(".bingux-") && name.ends_with(".part"), "{name}");
    assert!(!real_name_existed, "the real name must not exist while it is written");
    assert_eq!(read(dest.join("a.txt")), "alpha");
    assert!(part_files(&dest).is_empty());
}

#[test]
fn replacing_a_file_swaps_it_in_whole() {
    let temp = Temp::new("replace-atomic");
    let src = temp.write("src/a.txt", "new contents");
    let dest = temp.dir("dest");
    let old = temp.write("dest/a.txt", "old");
    let inode = fs::metadata(&old).unwrap().ino();
    let recorder = Recorder::default();
    let watched = old.clone();
    let during = Arc::new(Mutex::new(Vec::new()));
    let sink = during.clone();
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    recorder.on_call(move |call| {
        if matches!(call, Call::File(_)) {
            // While the new data is being made durable the old file is intact.
            sink.lock().unwrap().push(fs::read_to_string(&watched).unwrap());
        }
        Ok(())
    });
    run(
        &engine(&recorder, &policy, 1, false),
        false,
        &[src],
        &dest,
        &ConflictPolicy::Replace,
    )
    .unwrap();
    assert_eq!(*during.lock().unwrap(), ["old"]);
    assert_eq!(read(dest.join("a.txt")), "new contents");
    assert_ne!(fs::metadata(dest.join("a.txt")).unwrap().ino(), inode);
    assert!(part_files(&dest).is_empty());
}

#[test]
fn copying_to_a_device_writes_files_then_folders_then_the_drive() {
    let temp = Temp::new("device-copy");
    let dir = temp.dir("src/d");
    for n in 0..5 {
        temp.write(&format!("src/d/f{n}.txt"), "x");
    }
    temp.write("src/d/sub/g.txt", "g");
    let dest = temp.dir("stick");
    let recorder = Recorder::default();
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    let progress = Progress::new();
    copy_in(
        &engine(&recorder, &policy, 1, false),
        &[dir],
        &dest,
        &ConflictPolicy::Skip,
        &progress,
        &Cancel::new(),
    )
    .unwrap();
    let calls = recorder.calls();
    let files = calls.iter().filter(|c| matches!(c, Call::File(_))).count();
    assert_eq!(files, 6, "every file is fsynced: {calls:?}");
    let last_file = calls.iter().rposition(|c| matches!(c, Call::File(_))).unwrap();
    let first_dir = calls.iter().position(|c| matches!(c, Call::Dir(_))).unwrap();
    assert!(last_file < first_dir, "folders after their files: {calls:?}");
    assert_eq!(
        calls.last(),
        Some(&Call::Filesystem(dest.clone())),
        "the drive is written out last, once: {calls:?}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::Filesystem(_)))
            .count(),
        1
    );
    assert!(calls.contains(&Call::Dir(dest.join("d"))));
    assert!(calls.contains(&Call::Dir(dest.join("d/sub"))));
    assert!(calls.contains(&Call::Dir(dest.clone())));
    assert_eq!(progress.flushed_to(), Some("STICK".to_owned()));
    assert_eq!(progress.finishing(), None);
}

#[test]
fn the_progress_says_finishing_until_the_drive_has_the_data() {
    let temp = Temp::new("device-finishing");
    let a = temp.write("src/a.txt", "a");
    let dest = temp.dir("stick");
    let recorder = Recorder::default();
    let progress = Progress::new();
    let watcher = progress.clone();
    let during = Arc::new(Mutex::new(Vec::new()));
    let sink = during.clone();
    recorder.on_call(move |call| {
        if matches!(call, Call::Filesystem(_)) {
            sink.lock()
                .unwrap()
                .push((watcher.finishing(), watcher.flushed_to()));
        }
        Ok(())
    });
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    copy_in(
        &engine(&recorder, &policy, 1, false),
        &[a],
        &dest,
        &ConflictPolicy::Skip,
        &progress,
        &Cancel::new(),
    )
    .unwrap();
    assert_eq!(*during.lock().unwrap(), [(Some("STICK".to_owned()), None)]);
    assert_eq!(progress.finishing(), None);
    assert_eq!(progress.flushed_to(), Some("STICK".to_owned()));
}

#[test]
fn copying_on_an_internal_disk_does_not_wait_for_the_disk() {
    let temp = Temp::new("internal-copy");
    let a = temp.write("src/a.txt", "a");
    let dest = temp.dir("dest");
    let recorder = Recorder::default();
    run(&engine(&recorder, &lazy, 2, false), false, &[a], &dest, &ConflictPolicy::Skip).unwrap();
    assert!(recorder.calls().is_empty());
    assert_eq!(read(dest.join("a.txt")), "a");
}

#[test]
fn a_cancelled_or_failed_copy_leaves_no_part_file() {
    let temp = Temp::new("copy-fail");
    temp.write("src/a.txt", "a");
    temp.write("src/b.txt", "b");
    temp.write("src/c.txt", "c");
    let sources = vec![
        temp.path().join("src/a.txt"),
        temp.path().join("src/b.txt"),
        temp.path().join("src/c.txt"),
    ];
    let dest = temp.dir("stick");
    let recorder = Recorder::default();
    let count = std::sync::atomic::AtomicU32::new(0);
    recorder.on_call(move |call| match call {
        Call::File(_) if count.fetch_add(1, Ordering::SeqCst) == 1 => {
            Err(io::Error::from_raw_os_error(libc::EIO))
        }
        _ => Ok(()),
    });
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    let result = run(
        &engine(&recorder, &policy, 1, false),
        false,
        &sources,
        &dest,
        &ConflictPolicy::Skip,
    );
    assert!(result.is_err());
    assert!(part_files(&dest).is_empty(), "{:?}", part_files(&dest));
    assert_eq!(read(dest.join("a.txt")), "a", "what was whole stays");
    assert!(!dest.join("b.txt").exists());
    assert!(!dest.join("c.txt").exists());

    // Cancelled from outside while a file is being made durable.
    let cancel = Cancel::new();
    let stop = cancel.clone();
    let again = Recorder::default();
    again.on_call(move |call| {
        if matches!(call, Call::File(_)) {
            stop.cancel();
        }
        Ok(())
    });
    let dest2 = temp.dir("stick2");
    let level2 = stick(&dest2);
    let policy2 = move |_: &Path| level2.clone();
    let result = copy_in(
        &engine(&again, &policy2, 1, false),
        &sources,
        &dest2,
        &ConflictPolicy::Skip,
        &Progress::new(),
        &cancel,
    );
    assert_eq!(result, Err(OpError::Cancelled));
    assert!(part_files(&dest2).is_empty());
}

#[test]
fn many_small_files_are_copied_whole_by_several_threads() {
    let temp = Temp::new("many");
    let mut expected = Vec::new();
    for d in 0..5 {
        for n in 0..120 {
            let text = format!("file {d}/{n}");
            temp.write(&format!("src/tree/d{d}/f{n}.txt"), &text);
            expected.push((format!("tree/d{d}/f{n}.txt"), text));
        }
    }
    let dest = temp.dir("dest");
    let recorder = Recorder::default();
    let progress = Progress::new();
    copy_in(
        &engine(&recorder, &lazy, 4, false),
        &[temp.path().join("src/tree")],
        &dest,
        &ConflictPolicy::Skip,
        &progress,
        &Cancel::new(),
    )
    .unwrap();
    for (path, text) in &expected {
        assert_eq!(read(dest.join(path)), *text, "{path}");
    }
    assert!(part_files(&dest).is_empty());
    // The tree, its five folders and 600 files.
    assert_eq!(progress.files_done(), 606);
    assert_eq!(progress.fraction(), Some(1.0));
}

#[test]
fn two_sources_with_one_name_do_not_race_for_it() {
    let temp = Temp::new("race");
    let x = temp.write("x/a.txt", "from x");
    let y = temp.write("y/a.txt", "from y");
    let dest = temp.dir("dest");
    let recorder = Recorder::default();
    let done = run(
        &engine(&recorder, &lazy, 4, false),
        false,
        &[x, y],
        &dest,
        &ConflictPolicy::KeepBoth,
    )
    .unwrap();
    assert_eq!(done.len(), 2);
    let mut texts: Vec<String> = done.iter().map(read).collect();
    texts.sort();
    assert_eq!(texts, ["from x", "from y"]);
    assert_ne!(done[0], done[1]);
    assert!(part_files(&dest).is_empty());
}

#[test]
fn a_failure_on_one_thread_stops_the_rest_and_is_the_answer() {
    let temp = Temp::new("worker-fail");
    for n in 0..60 {
        temp.write(&format!("src/t/f{n}.txt"), "x");
    }
    let dest = temp.dir("stick");
    let recorder = Recorder::default();
    let count = std::sync::atomic::AtomicU32::new(0);
    recorder.on_call(move |call| match call {
        Call::File(_) if count.fetch_add(1, Ordering::SeqCst) == 5 => {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }
        _ => Ok(()),
    });
    let level = stick(&dest);
    let policy = move |_: &Path| level.clone();
    let error = run(
        &engine(&recorder, &policy, 4, false),
        false,
        &[temp.path().join("src/t")],
        &dest,
        &ConflictPolicy::Skip,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("STICK"),
        "the real failure, not Cancelled: {error}"
    );
    assert!(part_files(&dest).is_empty());
}

#[test]
fn a_sparse_file_stays_sparse_when_copied() {
    use std::os::unix::fs::FileExt;
    let temp = Temp::new("sparse-copy");
    let image = temp.path().join("disk.img");
    let length = 48u64 << 20;
    {
        let file = File::create(&image).unwrap();
        file.set_len(length).unwrap();
        file.write_all_at(b"boot", 0).unwrap();
        file.write_all_at(b"data", 20 << 20).unwrap();
    }
    if fs::metadata(&image).unwrap().blocks() * 512 >= length / 2 {
        eprintln!("skipped: this file system does not keep holes");
        return;
    }
    let dest = temp.dir("dest");
    let progress = Progress::new();
    copy(&[image], &dest, &ConflictPolicy::Skip, &progress, &Cancel::new()).unwrap();
    let copy = fs::metadata(dest.join("disk.img")).unwrap();
    assert_eq!(copy.len(), length);
    assert!(
        copy.blocks() * 512 < length / 4,
        "{} bytes allocated",
        copy.blocks() * 512
    );
    assert_eq!(progress.bytes_done(), length);
}

#[test]
fn delete_on_a_device_writes_out_the_folders() {
    let temp = Temp::new("device-delete");
    let doomed = temp.dir("stick/d");
    temp.write("stick/d/a.txt", "a");
    let recorder = Recorder::default();
    let level = stick(&temp.path().join("stick"));
    let policy = move |_: &Path| level.clone();
    let progress = Progress::new();
    delete_in(
        &engine(&recorder, &policy, 1, false),
        &[doomed.clone()],
        &progress,
        &Cancel::new(),
    )
    .unwrap();
    assert!(!doomed.exists());
    let calls = recorder.calls();
    assert!(calls.contains(&Call::Dir(temp.path().join("stick"))), "{calls:?}");
    assert_eq!(calls.last(), Some(&Call::Filesystem(temp.path().join("stick"))));
    assert_eq!(progress.flushed_to(), Some("STICK".to_owned()));
}
