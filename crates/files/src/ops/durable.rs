//! Making "done" mean "on the device".
//!
//! Writing a file returns as soon as the kernel has the data. On an internal
//! disk that is fine: the kernel writes it back within seconds and a journal
//! keeps the file system whole. On a USB stick, a card or a network share it
//! is not: a file manager that says "done" there while the data still sits in
//! memory lets the person pull the stick out and lose it.
//!
//! [`Level`] says how hard an operation must flush, decided per drive from
//! [`mounts::write_target`]. [`Durable`] does the flushing for one operation
//! and keeps the order that matters:
//!
//! 1. every file written is `fsync`ed before it gets its final name
//!    ([`Durable::file_written`]);
//! 2. every folder that gained or lost a name is `fsync`ed;
//! 3. a drive that needs it is written out whole with `syncfs(2)`, while the
//!    [`Progress`] says "Finishing, writing to <drive>";
//! 4. only then does the operation report that it is done, and only then does
//!    a move remove its source.
//!
//! The kernel calls go through [`Flusher`] so a test can record their order.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::mounts::{self, WriteTarget};

use super::{OpError, Progress};

/// How hard to flush what is written to one drive. Ordered: a bigger level
/// does everything a smaller one does.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    /// The kernel's own write-back is enough: an internal disk.
    Lazy,
    /// `fsync` every file and folder written, which makes each one durable
    /// without waiting for the whole drive. Used for a move between drives,
    /// where the source is removed afterwards.
    Files,
    /// `fsync` as above, then `syncfs` the drive and say so in the
    /// progress: removable media, network shares, FAT, exFAT and NTFS.
    Device(Device),
}

/// The drive of a [`Level::Device`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Device {
    pub mount_point: PathBuf,
    /// What to call it in a sentence.
    pub label: String,
}

impl Level {
    /// The level for the drive `path` is on, from the mount table.
    pub(crate) fn of_path(path: &Path) -> Level {
        Level::from_target(mounts::write_target(path))
    }

    pub(crate) fn from_target(target: Option<WriteTarget>) -> Level {
        match target {
            Some(target) => Level::Device(Device {
                mount_point: target.mount_point,
                label: target.label,
            }),
            None => Level::Lazy,
        }
    }

    fn flushes(&self) -> bool {
        *self != Level::Lazy
    }

    /// The bigger of two levels.
    pub(crate) fn or(self, other: Level) -> Level {
        self.max(other)
    }
}

/// The kernel calls behind a flush.
pub(crate) trait Flusher: Send + Sync {
    /// `fsync` one written file.
    fn file(&self, file: &File) -> io::Result<()>;
    /// `fsync` a folder, so the names in it survive.
    fn dir(&self, dir: &Path) -> io::Result<()>;
    /// Write out the whole drive mounted at `mount_point`.
    fn filesystem(&self, mount_point: &Path) -> io::Result<()>;
}

/// The real thing.
pub(crate) struct Kernel;

impl Flusher for Kernel {
    fn file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    fn filesystem(&self, mount_point: &Path) -> io::Result<()> {
        mounts::sync_filesystem(mount_point)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Where the flush level of a folder comes from: the mount table, or a test.
pub(crate) type Policy = dyn Fn(&Path) -> Level + Send + Sync;

/// The flushing of one operation.
pub(crate) struct Durable<'a> {
    flusher: &'a dyn Flusher,
    policy: &'a Policy,
    /// What files written to the destination need.
    destination: Level,
    /// The level of each drive seen so far, by device number, so a tree of
    /// many folders asks the mount table once.
    drives: Mutex<HashMap<u64, Level>>,
    /// Folders waiting to be `fsync`ed, with the level of their drive.
    dirty: Mutex<BTreeMap<PathBuf, Level>>,
}

impl<'a> Durable<'a> {
    /// For an operation that writes into `destination`.
    pub(crate) fn new(flusher: &'a dyn Flusher, policy: &'a Policy, destination: &Path) -> Self {
        let level = policy(destination);
        Durable {
            flusher,
            policy,
            destination: level,
            drives: Mutex::new(HashMap::new()),
            dirty: Mutex::new(BTreeMap::new()),
        }
    }

    /// For an operation with no destination (a delete).
    pub(crate) fn without_destination(flusher: &'a dyn Flusher, policy: &'a Policy) -> Self {
        Durable {
            flusher,
            policy,
            destination: Level::Lazy,
            drives: Mutex::new(HashMap::new()),
            dirty: Mutex::new(BTreeMap::new()),
        }
    }

    /// The level files written to the destination need. `at_least` raises it
    /// for a move between drives.
    pub(crate) fn destination(&self, at_least: Level) -> Level {
        self.destination.clone().or(at_least)
    }

    /// The level of the drive `dir` is on.
    fn level_of(&self, dir: &Path) -> Level {
        let Ok(meta) = fs::metadata(dir) else {
            return Level::Lazy;
        };
        let mut drives = lock(&self.drives);
        drives
            .entry(meta.dev())
            .or_insert_with(|| (self.policy)(dir))
            .clone()
    }

    /// `file` has all its data. Make it durable before it is given its name.
    pub(crate) fn file_written(&self, file: &File, level: &Level) -> Result<(), OpError> {
        if level.flushes() {
            self.flusher
                .file(file)
                .map_err(|error| write_error(level, &error))?;
        }
        Ok(())
    }

    /// A name appeared in `dir`, in the destination or at `level`.
    pub(crate) fn dir_changed(&self, dir: &Path, level: &Level) {
        if !level.flushes() {
            return;
        }
        let mut dirty = lock(&self.dirty);
        let entry = dirty.entry(dir.to_path_buf()).or_insert(Level::Lazy);
        if *entry < *level {
            *entry = level.clone();
        }
    }

    /// A name appeared in `dir` of the destination, written at `level`.
    pub(crate) fn dir_changed_in_destination(&self, dir: &Path, level: &Level) {
        let level = self.destination(level.clone());
        self.dir_changed(dir, &level);
    }

    /// A name went from, or appeared in, `dir`, and `dir` is wherever it is:
    /// the drive's own level decides. This is the source side of a move, a
    /// rename or a delete.
    pub(crate) fn dir_changed_here(&self, dir: &Path) {
        let level = self.level_of(dir);
        self.dir_changed(dir, &level);
    }

    /// Flush every folder that changed and every drive that needs it. Call it
    /// once the work is done and before reporting so, and, for a move, before
    /// removing the source.
    pub(crate) fn finish(&self, progress: &Progress) -> Result<(), OpError> {
        let dirty = std::mem::take(&mut *lock(&self.dirty));
        let mut devices: BTreeMap<PathBuf, String> = BTreeMap::new();
        for level in dirty.values() {
            if let Level::Device(device) = level {
                devices.insert(device.mount_point.clone(), device.label.clone());
            }
        }
        if let Some(label) = devices.values().next() {
            progress.set_finishing(Some(label));
        }
        let outcome = self.flush(&dirty, &devices);
        progress.set_finishing(None);
        outcome?;
        if let Some(label) = devices.values().next() {
            progress.set_flushed(label);
        }
        Ok(())
    }

    fn flush(
        &self,
        dirty: &BTreeMap<PathBuf, Level>,
        devices: &BTreeMap<PathBuf, String>,
    ) -> Result<(), OpError> {
        // Deepest first: a path sorts after its parent.
        for (dir, level) in dirty.iter().rev() {
            match self.flusher.dir(dir) {
                Ok(()) => {}
                // Removed since, which is the end of a move.
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(write_error(level, &error)),
            }
        }
        for (mount_point, label) in devices {
            self.flusher.filesystem(mount_point).map_err(|error| {
                OpError::Other(format!(
                    "Could not finish writing to {label}: {}",
                    sentence(&error)
                ))
            })?;
        }
        Ok(())
    }
}

fn sentence(error: &io::Error) -> String {
    let text = error.to_string();
    let text = text.split(" (os error").next().unwrap_or(&text);
    let mut chars = text.chars();
    let mut out: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "The drive did not answer".into(),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn write_error(level: &Level, error: &io::Error) -> OpError {
    match level {
        Level::Device(device) => OpError::Other(format!(
            "Could not write to {}: {}",
            device.label,
            sentence(error)
        )),
        _ => OpError::Other(format!("Could not write to the drive: {}", sentence(error))),
    }
}

/// Make the names just changed in `dirs` durable, for operations that are a
/// single call (a rename, a new folder, binning). Does nothing on an
/// ordinary disk. On removable media, network shares and FAT it `fsync`s
/// each folder, which is what makes the new name survive a pulled cable.
pub(crate) fn sync_dirs(dirs: &[&Path]) -> Result<(), OpError> {
    sync_dirs_with(&Kernel, &Level::of_path, dirs)
}

pub(crate) fn sync_dirs_with(
    flusher: &dyn Flusher,
    policy: &Policy,
    dirs: &[&Path],
) -> Result<(), OpError> {
    let durable = Durable::without_destination(flusher, policy);
    for dir in dirs {
        durable.dir_changed_here(dir);
    }
    durable.finish(&Progress::new())
}

/// After an external tool wrote `path` (an archive, say): write it out the
/// way a copy would. Does nothing on an ordinary disk.
pub(crate) fn sync_written(path: &Path) -> Result<(), OpError> {
    let level = Level::of_path(path);
    if !level.flushes() {
        return Ok(());
    }
    let flusher = Kernel;
    if let Ok(file) = File::open(path) {
        let _ = flusher.file(&file);
    }
    let policy = move |_: &Path| level.clone();
    let durable = Durable::without_destination(&flusher, &policy);
    if let Some(parent) = path.parent() {
        durable.dir_changed_here(parent);
    }
    durable.finish(&Progress::new())
}

#[cfg(test)]
pub(crate) mod recorder {
    use super::*;

    /// One kernel call, as the order tests read it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Call {
        File(String),
        Dir(PathBuf),
        Filesystem(PathBuf),
    }

    type Hook = Box<dyn Fn(&Call) -> io::Result<()> + Send + Sync>;

    /// Records every flush and may fail or interfere with one.
    #[derive(Default)]
    pub struct Recorder {
        pub calls: Mutex<Vec<Call>>,
        hook: Mutex<Option<Hook>>,
    }

    impl Recorder {
        pub fn on_call(&self, hook: impl Fn(&Call) -> io::Result<()> + Send + Sync + 'static) {
            *lock(&self.hook) = Some(Box::new(hook));
        }

        pub fn calls(&self) -> Vec<Call> {
            lock(&self.calls).clone()
        }

        fn record(&self, call: Call) -> io::Result<()> {
            lock(&self.calls).push(call.clone());
            match lock(&self.hook).as_ref() {
                Some(hook) => hook(&call),
                None => Ok(()),
            }
        }
    }

    impl Flusher for Recorder {
        fn file(&self, file: &File) -> io::Result<()> {
            // The file has no name here: read it from the descriptor.
            use std::os::fd::AsRawFd;
            let name = fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .ok()
                .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            self.record(Call::File(name))
        }

        fn dir(&self, dir: &Path) -> io::Result<()> {
            self.record(Call::Dir(dir.to_path_buf()))
        }

        fn filesystem(&self, mount_point: &Path) -> io::Result<()> {
            self.record(Call::Filesystem(mount_point.to_path_buf()))
        }
    }

    pub fn stick(mount_point: &Path) -> Level {
        Level::Device(Device {
            mount_point: mount_point.to_path_buf(),
            label: "STICK".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::recorder::{Call, Recorder, stick};
    use super::*;

    #[test]
    fn levels_only_grow() {
        let device = stick(Path::new("/run/media/u/STICK"));
        assert!(Level::Lazy < Level::Files && Level::Files < device);
        assert_eq!(Level::Files.or(Level::Lazy), Level::Files);
        assert_eq!(Level::Lazy.or(device.clone()), device);
    }

    #[test]
    fn an_ordinary_disk_flushes_nothing() {
        let temp = Temp::new("durable-lazy");
        let recorder = Recorder::default();
        let policy = |_: &Path| Level::Lazy;
        let durable = Durable::new(&recorder, &policy, temp.path());
        durable.dir_changed_in_destination(temp.path(), &Level::Lazy);
        durable.dir_changed_here(temp.path());
        durable.finish(&Progress::new()).unwrap();
        assert!(recorder.calls().is_empty());
    }

    #[test]
    fn a_device_flushes_folders_deepest_first_then_the_whole_drive() {
        let temp = Temp::new("durable-order");
        let inner = temp.dir("a/b");
        let outer = temp.path().join("a");
        let recorder = Recorder::default();
        let level = stick(temp.path());
        let policy = {
            let level = level.clone();
            move |_: &Path| level.clone()
        };
        let durable = Durable::new(&recorder, &policy, temp.path());
        durable.dir_changed_in_destination(&outer, &Level::Lazy);
        durable.dir_changed_in_destination(&inner, &Level::Lazy);
        let progress = Progress::new();
        durable.finish(&progress).unwrap();
        assert_eq!(
            recorder.calls(),
            [
                Call::Dir(inner),
                Call::Dir(outer),
                Call::Filesystem(temp.path().to_path_buf())
            ]
        );
        assert_eq!(progress.finishing(), None, "the finishing note ends");
        assert_eq!(progress.flushed_to(), Some("STICK".to_owned()));
    }

    #[test]
    fn the_progress_says_finishing_while_the_drive_is_written() {
        let temp = Temp::new("durable-finishing");
        let recorder = Recorder::default();
        let progress = Progress::new();
        let seen = progress.clone();
        let during = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = during.clone();
        recorder.on_call(move |call| {
            if matches!(call, Call::Filesystem(_)) {
                lock(&sink).push(seen.finishing());
            }
            Ok(())
        });
        let level = stick(temp.path());
        let policy = move |_: &Path| level.clone();
        let durable = Durable::new(&recorder, &policy, temp.path());
        durable.dir_changed_in_destination(temp.path(), &Level::Lazy);
        durable.finish(&progress).unwrap();
        assert_eq!(*lock(&during), [Some("STICK".to_owned())]);
    }

    #[test]
    fn a_failed_drive_flush_is_an_error_not_done() {
        let temp = Temp::new("durable-fail");
        let recorder = Recorder::default();
        recorder.on_call(|call| match call {
            Call::Filesystem(_) => Err(io::Error::from_raw_os_error(libc::EIO)),
            _ => Ok(()),
        });
        let level = stick(temp.path());
        let policy = move |_: &Path| level.clone();
        let durable = Durable::new(&recorder, &policy, temp.path());
        durable.dir_changed_in_destination(temp.path(), &Level::Lazy);
        let progress = Progress::new();
        let error = durable.finish(&progress).unwrap_err();
        let OpError::Other(text) = error else {
            panic!("{error:?}")
        };
        assert!(
            text.starts_with("Could not finish writing to STICK"),
            "{text}"
        );
        assert_eq!(progress.flushed_to(), None);
        assert_eq!(progress.finishing(), None);
    }

    #[test]
    fn single_calls_sync_the_folder_on_a_device_only() {
        let temp = Temp::new("durable-single");
        let recorder = Recorder::default();
        let on_stick = {
            let level = stick(temp.path());
            move |_: &Path| level.clone()
        };
        sync_dirs_with(&recorder, &on_stick, &[temp.path()]).unwrap();
        assert_eq!(
            recorder.calls(),
            [
                Call::Dir(temp.path().to_path_buf()),
                Call::Filesystem(temp.path().to_path_buf())
            ]
        );
        let quiet = Recorder::default();
        sync_dirs_with(&quiet, &|_: &Path| Level::Lazy, &[temp.path()]).unwrap();
        assert!(quiet.calls().is_empty());
    }
}
