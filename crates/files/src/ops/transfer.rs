//! Copying, moving and deleting for good.
//!
//! Links are never followed: a link is copied, moved and deleted as a link.
//! The totals for [`Progress`] are counted by a scan on a second thread, so
//! the first byte moves before the scan ends.
//!
//! # What each operation guarantees
//!
//! **A move on one drive is one `renameat2(RENAME_NOREPLACE)`.** Atomic, no
//! copy, no partial state, and a name that is taken is never overwritten. A
//! folder moves whole, whatever is in it.
//!
//! **A move between drives** (or where the kernel refuses the rename, as
//! between two subvolumes) copies first and removes second:
//!
//! 1. copy the item, each file under a temporary name that becomes the real
//!    name only once it is whole;
//! 2. `fsync` every file and folder written, and, where the drive needs it
//!    (see [`mounts::write_target`](crate::mounts::write_target)), `syncfs`
//!    the drive;
//! 3. check that every copy is as long as what was read, and that no source
//!    changed while it was read;
//! 4. only then remove the sources, file by file, and the folders that are
//!    empty.
//!
//! A failure, a cancel, a full drive or a pulled cable before step 4 leaves
//! the source exactly as it was. A source that was skipped (a name clash
//! answered "Skip") is never removed.
//!
//! **A copy never shows half a file.** Data goes to `.bingux-*.part` beside
//! the destination and is renamed into place when it is complete (and, on
//! removable media, durable). A failed or cancelled copy removes its part
//! files. The data is moved by reflink, else `copy_file_range`, else a big
//! buffer; holes stay holes; many small files are copied by a few threads.
//!
//! **On removable media, a share, FAT, exFAT or NTFS, "done" means durable.**
//! See [`super::durable`]: every file is `fsync`ed, then its folders, then
//! the drive is written out, and [`Progress::finishing`] names the drive
//! while that happens.
//!
//! **Name clashes** follow [`names`](super::names): nothing is replaced
//! unless the [`ConflictPolicy`] says so.

use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use crate::listing::Cancel;

use super::data::{Hints, copy_data, copy_xattrs, tolerate};
use super::durable::{Durable, Flusher, Kernel, Level, Policy};
use super::names::keep_both_name;
use super::{OpError, Progress, rename_noreplace};

/// Two things with one name: the thing being copied or moved, and the thing
/// already at the destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub source: PathBuf,
    pub existing: PathBuf,
}

/// What to do about one [`Conflict`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Leave both as they are.
    Skip,
    /// Replace the existing file. A folder over a folder merges: the
    /// contents go through the policy again, one by one.
    Replace,
    /// Keep the existing one and give the new one a "(copy)" name.
    KeepBoth,
    /// Stop the whole operation.
    Cancel,
}

/// How [`copy`] and [`move_to`] settle a conflict.
///
/// `Ask` runs on the worker thread and may block (on a channel, say) while
/// a dialog is up. For "apply to all", let the closure remember the answer
/// and return it without asking again.
pub enum ConflictPolicy {
    Skip,
    Replace,
    KeepBoth,
    Ask(Box<dyn Fn(&Conflict) -> Resolution + Send + Sync>),
}

impl std::fmt::Debug for ConflictPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ConflictPolicy::Skip => "Skip",
            ConflictPolicy::Replace => "Replace",
            ConflictPolicy::KeepBoth => "KeepBoth",
            ConflictPolicy::Ask(_) => "Ask(..)",
        })
    }
}

/// What an operation runs on: the kernel and the mount table, or, in a test,
/// a recorder and a made-up mount table.
pub(super) struct Engine<'a> {
    pub flusher: &'a dyn Flusher,
    pub policy: &'a Policy,
    /// Treat every move as a move between drives.
    pub force_cross_drive: bool,
    /// Threads that copy small files. `None` picks by drive.
    pub workers: Option<usize>,
}

static KERNEL: Kernel = Kernel;

fn system_policy(path: &Path) -> Level {
    Level::of_path(path)
}

impl Engine<'static> {
    pub(super) fn system() -> Self {
        Engine {
            flusher: &KERNEL,
            policy: &system_policy,
            force_cross_drive: false,
            workers: None,
        }
    }
}

/// How one node travels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Copy; the source stays.
    Copy,
    /// Move on one drive: `rename(2)`, with a copy if it turns out not to be.
    Rename,
    /// Move across drives: copy, remember every source, and remove them
    /// only once everything is durable and checked.
    CopyDelete,
}

/// Files this big are copied by the walking thread, one at a time; smaller
/// ones go to the workers.
const BIG_FILE: u64 = 32 << 20;

/// Jobs a worker may have waiting before the walker slows down.
const QUEUE_PER_WORKER: usize = 16;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One small file for a worker.
struct Job {
    source: PathBuf,
    meta: Metadata,
    target: PathBuf,
    replace: bool,
    mode: Mode,
}

/// What a move has put at the destination, so it can be checked and the
/// source removed.
struct Copied {
    source: PathBuf,
    target: PathBuf,
    what: What,
}

enum What {
    File {
        source_len: u64,
        source_modified: Option<SystemTime>,
        copy_len: u64,
    },
    Link,
    Dir,
}

/// A folder made by the copy, whose mode and times are set once everything
/// in it has arrived.
struct Finish {
    path: PathBuf,
    meta: Metadata,
}

struct Ctx<'a> {
    conflicts: &'a ConflictPolicy,
    progress: &'a Progress,
    cancel: &'a Cancel,
    durable: Durable<'a>,
    force_cross_drive: bool,
    workers: usize,
    chunk: usize,
    hints: Hints,
    /// A worker failed: everything still queued gives up.
    stop: AtomicBool,
    failure: Mutex<Option<OpError>>,
    jobs: Mutex<Option<SyncSender<Job>>>,
    pending: Mutex<usize>,
    idle: Condvar,
    /// Names queued for a worker and so not on disk yet.
    reserved: Mutex<HashSet<PathBuf>>,
    journal: Mutex<Vec<Copied>>,
    finish: Mutex<Vec<Finish>>,
}

impl<'a> Ctx<'a> {
    fn new(
        engine: &Engine<'a>,
        destination: Option<&Path>,
        conflicts: &'a ConflictPolicy,
        progress: &'a Progress,
        cancel: &'a Cancel,
    ) -> Self {
        let durable = match destination {
            Some(dir) => Durable::new(engine.flusher, engine.policy, dir),
            None => Durable::without_destination(engine.flusher, engine.policy),
        };
        let level = durable.destination(Level::Lazy);
        let device = matches!(level, Level::Device(_));
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        Ctx {
            conflicts,
            progress,
            cancel,
            durable,
            force_cross_drive: engine.force_cross_drive,
            // A stick or a share gains from a few writes in flight, and loses
            // from many.
            workers: engine
                .workers
                .unwrap_or(if device { 4 } else { cpus.clamp(1, 8) }),
            // Smaller chunks where a chunk takes a while, so cancel is quick.
            chunk: if device { 4 << 20 } else { 16 << 20 },
            hints: Hints::new(),
            stop: AtomicBool::new(false),
            failure: Mutex::new(None),
            jobs: Mutex::new(None),
            pending: Mutex::new(0),
            idle: Condvar::new(),
            reserved: Mutex::new(HashSet::new()),
            journal: Mutex::new(Vec::new()),
            finish: Mutex::new(Vec::new()),
        }
    }

    fn check(&self) -> Result<(), OpError> {
        if self.cancel.is_cancelled() || self.stop.load(Ordering::Acquire) {
            Err(OpError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn resolve(&self, source: &Path, existing: &Path) -> Result<Resolution, OpError> {
        let resolution = match self.conflicts {
            ConflictPolicy::Skip => Resolution::Skip,
            ConflictPolicy::Replace => Resolution::Replace,
            ConflictPolicy::KeepBoth => Resolution::KeepBoth,
            ConflictPolicy::Ask(ask) => ask(&Conflict {
                source: source.to_path_buf(),
                existing: existing.to_path_buf(),
            }),
        };
        match resolution {
            Resolution::Cancel => Err(OpError::Cancelled),
            other => Ok(other),
        }
    }

    /// What a file written for `mode` must be flushed with.
    fn level_for(&self, mode: Mode) -> Level {
        self.durable.destination(if mode == Mode::CopyDelete {
            Level::Files
        } else {
            Level::Lazy
        })
    }

    /// A name appeared in `dir` of the destination.
    fn name_added(&self, dir: &Path, mode: Mode) {
        self.durable
            .dir_changed_in_destination(dir, &self.level_for(mode));
    }

    /// `from` was renamed to `to`, on one drive: both folders changed.
    fn renamed(&self, from: &Path, to: &Path) {
        for dir in [from.parent(), to.parent()].into_iter().flatten() {
            self.durable.dir_changed_here(dir);
        }
    }

    /// A name `dir/name` that is free on disk and not about to be made by a
    /// worker: `name` itself, else its next "(copy)" name.
    fn unique_target(&self, dir: &Path, name: &str) -> PathBuf {
        let reserved = lock(&self.reserved);
        let taken = |candidate: &str| {
            let path = dir.join(candidate);
            path.symlink_metadata().is_ok() || reserved.contains(&path)
        };
        if taken(name) {
            dir.join(keep_both_name(name, &taken))
        } else {
            dir.join(name)
        }
    }

    fn is_reserved(&self, path: &Path) -> bool {
        lock(&self.reserved).contains(path)
    }

    // ---- workers -------------------------------------------------------------

    /// Run `body`, which may queue small files, with the workers running. Waits
    /// for the workers, then sets the modes and times of the folders made. The
    /// first failure of any thread is the answer.
    fn with_workers<T>(&self, body: impl FnOnce() -> Result<T, OpError>) -> Result<T, OpError> {
        let mark = lock(&self.finish).len();
        let outcome = if self.workers <= 1 || lock(&self.jobs).is_some() {
            body()
        } else {
            let (sender, receiver) = sync_channel::<Job>(self.workers * QUEUE_PER_WORKER);
            let receiver = Mutex::new(receiver);
            *lock(&self.jobs) = Some(sender);
            std::thread::scope(|scope| {
                for _ in 0..self.workers {
                    scope.spawn(|| {
                        loop {
                            let next = lock(&receiver).recv();
                            match next {
                                Ok(job) => self.run_job(job),
                                Err(_) => break,
                            }
                        }
                    });
                }
                let outcome = body();
                // Dropping the sender ends the workers once the queue is empty.
                *lock(&self.jobs) = None;
                outcome
            })
        };
        let failure = lock(&self.failure).take();
        let outcome = match failure {
            Some(error) => Err(error),
            None => outcome,
        };
        // Folders that were made keep the mode they were given even when the
        // copy stopped half way.
        let finished = self.finish_folders(mark);
        if outcome.is_ok() {
            finished?;
        }
        outcome
    }

    /// Give `job` to a worker. Hands it back when there are none (the walker
    /// then does it itself).
    fn queue(&self, job: Job) -> Option<Job> {
        let Some(sender) = lock(&self.jobs).clone() else {
            return Some(job);
        };
        lock(&self.reserved).insert(job.target.clone());
        *lock(&self.pending) += 1;
        match sender.send(job) {
            Ok(()) => None,
            Err(returned) => {
                *lock(&self.pending) -= 1;
                Some(returned.0)
            }
        }
    }

    fn run_job(&self, job: Job) {
        struct Done<'c, 'a>(&'c Ctx<'a>);
        impl Drop for Done<'_, '_> {
            fn drop(&mut self) {
                let mut pending = lock(&self.0.pending);
                *pending -= 1;
                if *pending == 0 {
                    self.0.idle.notify_all();
                }
            }
        }
        let _done = Done(self);
        if self.check().is_err() {
            return;
        }
        match copy_file(
            self,
            &job.source,
            &job.meta,
            &job.target,
            job.replace,
            job.mode,
        ) {
            Ok(copy_len) => {
                self.progress.add_files(1);
                self.remember_file(&job.source, &job.meta, &job.target, copy_len, job.mode);
            }
            Err(error) => self.fail(error),
        }
    }

    /// A worker failed. The first real failure wins over the "cancelled" the
    /// others then report.
    fn fail(&self, error: OpError) {
        let mut failure = lock(&self.failure);
        if failure.is_none() || matches!(*failure, Some(OpError::Cancelled)) {
            *failure = Some(error);
        }
        self.stop.store(true, Ordering::Release);
    }

    /// Wait until every queued file is on disk.
    fn drain(&self) -> Result<(), OpError> {
        let mut pending = lock(&self.pending);
        while *pending > 0 {
            pending = self
                .idle
                .wait(pending)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(pending);
        self.check()
    }

    // ---- the journal of a move -----------------------------------------------

    fn remember_file(
        &self,
        source: &Path,
        meta: &Metadata,
        target: &Path,
        copy_len: u64,
        mode: Mode,
    ) {
        if mode == Mode::CopyDelete {
            lock(&self.journal).push(Copied {
                source: source.to_path_buf(),
                target: target.to_path_buf(),
                what: What::File {
                    source_len: meta.len(),
                    source_modified: meta.modified().ok(),
                    copy_len,
                },
            });
        }
    }

    fn remember(&self, source: &Path, target: &Path, what: What, mode: Mode) {
        if mode == Mode::CopyDelete {
            lock(&self.journal).push(Copied {
                source: source.to_path_buf(),
                target: target.to_path_buf(),
                what,
            });
        }
    }

    fn journal_len(&self) -> usize {
        lock(&self.journal).len()
    }

    fn finish_folders(&self, mark: usize) -> Result<(), OpError> {
        let folders = lock(&self.finish).split_off(mark);
        // Deepest first: a folder made read-only must be the last thing set
        // in its parent.
        for folder in folders.iter().rev() {
            tolerate(fs::set_permissions(
                &folder.path,
                fs::Permissions::from_mode(folder.meta.mode() & 0o7777),
            ))?;
            // A folder opens read-only; `futimens` does not need write access.
            if let Ok(handle) = File::open(&folder.path) {
                let _ = handle.set_times(copied_times(&folder.meta));
            }
        }
        Ok(())
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// A name for a file being written, in the folder it will end up in. Short
/// and fixed in length, so it works for any name the real one could have.
fn temp_path(dir: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    dir.join(format!(
        ".bingux-{}-{}.part",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

// ---- scanning ---------------------------------------------------------------

/// Run `body` while a second thread counts `roots` into `progress`.
fn with_scan<T>(
    progress: &Progress,
    cancel: &Cancel,
    roots: &[PathBuf],
    body: impl FnOnce() -> T,
) -> T {
    let stop = AtomicBool::new(false);
    progress.set_scanning(true);
    let out = std::thread::scope(|scope| {
        scope.spawn(|| {
            for root in roots {
                if !scan(root, progress, cancel, &stop) {
                    break;
                }
            }
            progress.set_scanning(false);
        });
        let out = body();
        stop.store(true, Ordering::Release);
        out
    });
    progress.settle();
    out
}

/// Add `root` and everything under it to the totals. False if stopped.
fn scan(root: &Path, progress: &Progress, cancel: &Cancel, stop: &AtomicBool) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        if cancel.is_cancelled() || stop.load(Ordering::Acquire) {
            return false;
        }
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if meta.is_dir() {
            progress.add_total(0, 1);
            if let Ok(children) = fs::read_dir(&path) {
                pending.extend(children.flatten().map(|child| child.path()));
            }
        } else {
            progress.add_total(if meta.is_file() { meta.len() } else { 0 }, 1);
        }
    }
    true
}

// ---- delete -----------------------------------------------------------------

/// Delete for good. Folders go with everything in them. Links are removed,
/// never followed. Stops at the first error; what was deleted stays deleted.
///
/// On removable media, a share, FAT, exFAT or NTFS the folders that lost a
/// name are `fsync`ed before this returns.
pub fn delete(paths: &[PathBuf], progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    delete_in(&Engine::system(), paths, progress, cancel)
}

pub(super) fn delete_in(
    engine: &Engine<'_>,
    paths: &[PathBuf],
    progress: &Progress,
    cancel: &Cancel,
) -> Result<(), OpError> {
    for path in paths {
        if path.parent().is_none() {
            return Err(OpError::Other("The root folder cannot be deleted.".into()));
        }
    }
    let durable = Durable::without_destination(engine.flusher, engine.policy);
    with_scan(progress, cancel, paths, || {
        for path in paths {
            remove_tree(path, progress, cancel, &durable)?;
        }
        Ok::<(), OpError>(())
    })?;
    durable.finish(progress)
}

fn remove_tree(
    path: &Path,
    progress: &Progress,
    cancel: &Cancel,
    durable: &Durable<'_>,
) -> Result<(), OpError> {
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let meta = path.symlink_metadata()?;
    progress.set_current(&name_of(path));
    if meta.is_dir() {
        let mut children: Vec<PathBuf> = fs::read_dir(path)?
            .map(|child| child.map(|child| child.path()))
            .collect::<io::Result<_>>()?;
        children.sort();
        for child in children {
            remove_tree(&child, progress, cancel, durable)?;
        }
        durable.dir_changed_here(path);
        fs::remove_dir(path)?;
        progress.add_files(1);
    } else {
        fs::remove_file(path)?;
        progress.add_bytes(if meta.is_file() { meta.len() } else { 0 });
        progress.add_files(1);
    }
    if let Some(parent) = path.parent() {
        durable.dir_changed_here(parent);
    }
    Ok(())
}

// ---- copy and move ----------------------------------------------------------

/// Where each of `paths` goes, checked once: the destination is a folder, and
/// no folder goes into itself.
struct Plan {
    dest: PathBuf,
    dest_canonical: PathBuf,
    dest_dev: u64,
}

fn plan(dest_dir: &Path) -> Result<Plan, OpError> {
    let meta = fs::metadata(dest_dir)?;
    if !meta.is_dir() {
        return Err(OpError::Other("The destination is not a folder.".into()));
    }
    Ok(Plan {
        dest: dest_dir.to_path_buf(),
        dest_canonical: fs::canonicalize(dest_dir)?,
        dest_dev: meta.dev(),
    })
}

impl Plan {
    /// The item is already in the destination folder.
    fn is_in_place(&self, source: &Path) -> bool {
        source
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            .is_some_and(|parent| parent == self.dest_canonical)
    }

    fn check_not_into_itself(&self, source: &Path, meta: &Metadata) -> Result<(), OpError> {
        if meta.is_dir() && self.dest_canonical.starts_with(fs::canonicalize(source)?) {
            return Err(OpError::Other(
                "A folder cannot be put inside itself.".into(),
            ));
        }
        Ok(())
    }
}

/// Copy items into `dest_dir`. Folders are copied whole; modes, modified
/// times and extended attributes are kept as far as the drive has them; links
/// stay links. The data moves by reflink where the file system has them, else
/// `copy_file_range`; holes stay holes.
///
/// Every file arrives under a temporary name and is renamed into place when
/// it is whole, so a half-copied file never has its real name.
///
/// Copying into the folder an item is already in makes a "(copy)" of it.
/// Returns the new paths, without the ones that were skipped. If it stops
/// early (cancelled, or an error) what was already copied stays, and no
/// part files do.
///
/// On removable media, a share, FAT, exFAT or NTFS the result is durable
/// when this returns: see [`Progress::finishing`].
pub fn copy(
    paths: &[PathBuf],
    dest_dir: &Path,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<PathBuf>, OpError> {
    copy_in(
        &Engine::system(),
        paths,
        dest_dir,
        conflicts,
        progress,
        cancel,
    )
}

pub(super) fn copy_in(
    engine: &Engine<'_>,
    paths: &[PathBuf],
    dest_dir: &Path,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<PathBuf>, OpError> {
    let plan = plan(dest_dir)?;
    let ctx = Ctx::new(engine, Some(&plan.dest), conflicts, progress, cancel);
    with_scan(progress, cancel, paths, || {
        let done = ctx.with_workers(|| {
            let mut done = Vec::with_capacity(paths.len());
            for source in paths {
                ctx.check()?;
                let meta = source.symlink_metadata()?;
                plan.check_not_into_itself(source, &meta)?;
                let placed = place(
                    &ctx,
                    source,
                    &plan.dest,
                    Mode::Copy,
                    plan.is_in_place(source),
                )?;
                done.extend(placed);
            }
            Ok(done)
        })?;
        ctx.durable.finish(progress)?;
        Ok(done)
    })
}

/// Move items into `dest_dir`: one `renameat2` on the same drive, and on
/// another drive a copy that is made durable and checked before each source
/// is removed (see the module documentation). Moving an item into the folder
/// it is in does nothing. Returns the new paths, without the ones that were
/// skipped.
pub fn move_to(
    paths: &[PathBuf],
    dest_dir: &Path,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<PathBuf>, OpError> {
    move_in(
        &Engine::system(),
        paths,
        dest_dir,
        conflicts,
        progress,
        cancel,
    )
}

pub(super) fn move_in(
    engine: &Engine<'_>,
    paths: &[PathBuf],
    dest_dir: &Path,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<PathBuf>, OpError> {
    let plan = plan(dest_dir)?;
    let ctx = Ctx::new(engine, Some(&plan.dest), conflicts, progress, cancel);
    let mut done = Vec::with_capacity(paths.len());
    for source in paths {
        ctx.check()?;
        let meta = source.symlink_metadata()?;
        if plan.is_in_place(source) {
            progress.count_instant();
            done.push(source.clone());
            continue;
        }
        plan.check_not_into_itself(source, &meta)?;
        let placed = if meta.dev() == plan.dest_dev && !ctx.force_cross_drive {
            place(&ctx, source, &plan.dest, Mode::Rename, false)?
        } else {
            let roots = [source.clone()];
            let mark = ctx.journal_len();
            let placed = with_scan(progress, cancel, &roots, || {
                ctx.with_workers(|| place(&ctx, source, &plan.dest, Mode::CopyDelete, false))
            })?;
            finish_move(&ctx, mark)?;
            placed
        };
        done.extend(placed);
    }
    // The folders the renames changed.
    ctx.durable.finish(progress)?;
    Ok(done)
}

/// The copies of a move are all made: make them durable, check them, and only
/// then remove what they were copied from.
fn finish_move(ctx: &Ctx<'_>, mark: usize) -> Result<(), OpError> {
    ctx.durable.finish(ctx.progress)?;
    let copied = lock(&ctx.journal).split_off(mark);
    verify(&copied)?;
    remove_sources(ctx, &copied)?;
    // The folders the sources were removed from, if they are on removable media.
    ctx.durable.finish(ctx.progress)
}

fn verify(copied: &[Copied]) -> Result<(), OpError> {
    let incomplete = |source: &Path| {
        OpError::Other(format!(
            "{} did not arrive complete, so the original was left where it is.",
            quoted(source)
        ))
    };
    for item in copied {
        match &item.what {
            What::File {
                source_len,
                source_modified,
                copy_len,
            } => {
                let copy = item
                    .target
                    .symlink_metadata()
                    .map_err(|_| incomplete(&item.source))?;
                if !copy.is_file() || copy.len() != *copy_len || copy_len != source_len {
                    return Err(incomplete(&item.source));
                }
                // Gone is fine: there is nothing left to lose.
                if let Ok(now) = item.source.symlink_metadata()
                    && (now.len() != *source_len || now.modified().ok() != *source_modified)
                {
                    return Err(OpError::Other(format!(
                        "{} changed while it was being moved, so the original was left where it is.",
                        quoted(&item.source)
                    )));
                }
            }
            What::Link => {
                let same = match (fs::read_link(&item.source), fs::read_link(&item.target)) {
                    (Ok(from), Ok(to)) => from == to,
                    (Err(error), _) => error.kind() == io::ErrorKind::NotFound,
                    _ => false,
                };
                if !same {
                    return Err(incomplete(&item.source));
                }
            }
            What::Dir => {
                if !item.target.is_dir() {
                    return Err(incomplete(&item.source));
                }
            }
        }
    }
    Ok(())
}

fn quoted(path: &Path) -> String {
    format!("“{}”", name_of(path))
}

/// Remove the files and links that were copied, then the folders that are
/// empty by now. A folder that still holds something (an item that was
/// skipped) stays.
fn remove_sources(ctx: &Ctx<'_>, copied: &[Copied]) -> Result<(), OpError> {
    let mut last: Option<&Path> = None;
    for item in copied.iter().filter(|item| !matches!(item.what, What::Dir)) {
        match fs::remove_file(&item.source) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if let Some(parent) = item.source.parent()
            && last != Some(parent)
        {
            ctx.durable.dir_changed_here(parent);
            last = Some(parent);
        }
    }
    for item in copied
        .iter()
        .rev()
        .filter(|item| matches!(item.what, What::Dir))
    {
        match fs::remove_dir(&item.source) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error.into()),
        }
        if let Some(parent) = item.source.parent() {
            ctx.durable.dir_changed_here(parent);
        }
    }
    Ok(())
}

/// Put `source` into `dir`, settling a name clash first. `None` if skipped.
fn place(
    ctx: &Ctx<'_>,
    source: &Path,
    dir: &Path,
    mode: Mode,
    force_keep_both: bool,
) -> Result<Option<PathBuf>, OpError> {
    ctx.check()?;
    let name = source
        .file_name()
        .ok_or_else(|| OpError::Other("That cannot be copied or moved.".into()))?;
    let meta = source.symlink_metadata()?;
    let target = dir.join(name);
    if ctx.is_reserved(&target) {
        // Another item of this very operation is about to take the name.
        ctx.drain()?;
    }
    let existing = match target.symlink_metadata() {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            transfer_node(ctx, source, &meta, &target, mode)?;
            return Ok(Some(target));
        }
        Err(error) => return Err(error.into()),
    };
    let resolution = if force_keep_both {
        Resolution::KeepBoth
    } else {
        ctx.resolve(source, &target)?
    };
    match resolution {
        Resolution::Skip | Resolution::Cancel => {
            if !meta.is_dir() {
                ctx.progress
                    .add_bytes(if meta.is_file() { meta.len() } else { 0 });
                ctx.progress.add_files(1);
            }
            Ok(None)
        }
        Resolution::KeepBoth => {
            let unique = ctx.unique_target(dir, &name.to_string_lossy());
            transfer_node(ctx, source, &meta, &unique, mode)?;
            Ok(Some(unique))
        }
        Resolution::Replace => {
            replace(ctx, source, &meta, &target, &existing, mode)?;
            Ok(Some(target))
        }
    }
}

fn replace(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    existing: &Metadata,
    mode: Mode,
) -> Result<(), OpError> {
    match (meta.is_dir(), existing.is_dir()) {
        // Folder over folder: merge. The existing folder keeps its own mode.
        (true, true) => copy_dir(ctx, source, meta, target, mode, false),
        (false, true) => Err(OpError::Other(format!(
            "A folder called {} is there already, so a file cannot replace it.",
            name_of(target)
        ))),
        (true, false) => {
            // Put the file aside, so a failure gives it back.
            let aside = temp_path(target.parent().unwrap_or(Path::new(".")));
            fs::rename(target, &aside)?;
            match transfer_node(ctx, source, meta, target, mode) {
                Ok(()) => {
                    let _ = fs::remove_file(&aside);
                    Ok(())
                }
                Err(error) => {
                    // Whatever is at `target` now is what this call made.
                    if target.symlink_metadata().is_ok() {
                        let _ = fs::remove_dir_all(target);
                    }
                    let _ = fs::rename(&aside, target);
                    Err(error)
                }
            }
        }
        (false, false) => {
            if mode == Mode::Rename {
                match fs::rename(source, target) {
                    Ok(()) => {
                        ctx.progress.count_instant();
                        ctx.renamed(source, target);
                        return Ok(());
                    }
                    Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                        return move_by_copy(ctx, source, meta, target, true);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            // The copy goes to a part file and is renamed over the old one,
            // so a failure keeps the old one.
            copy_node(ctx, source, meta, target, mode, true)
        }
    }
}

/// Bring `source` to `target`, which does not exist.
fn transfer_node(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    mode: Mode,
) -> Result<(), OpError> {
    if mode == Mode::Rename {
        return match rename_noreplace(source, target) {
            Ok(()) => {
                ctx.progress.count_instant();
                ctx.renamed(source, target);
                Ok(())
            }
            // Another drive after all (a bind mount, a subvolume).
            Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                move_by_copy(ctx, source, meta, target, false)
            }
            Err(error) => Err(error.into()),
        };
    }
    copy_node(ctx, source, meta, target, mode, false)
}

/// A move the kernel could not rename: copy it, make the copy durable, check
/// it, and only then remove the source.
fn move_by_copy(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    replace: bool,
) -> Result<(), OpError> {
    let mark = ctx.journal_len();
    ctx.with_workers(|| copy_node(ctx, source, meta, target, Mode::CopyDelete, replace))?;
    finish_move(ctx, mark)
}

/// Copy one file, link or folder to `target`. A `target` that exists is
/// replaced when `replace` is set (a file or link, never a folder).
fn copy_node(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    mode: Mode,
    replace: bool,
) -> Result<(), OpError> {
    ctx.check()?;
    let kind = meta.file_type();
    if kind.is_dir() {
        return copy_dir(ctx, source, meta, target, mode, true);
    }
    ctx.progress.set_current(&name_of(source));
    if kind.is_symlink() {
        copy_link(ctx, source, target, replace, mode)?;
        ctx.progress.add_files(1);
        ctx.remember(source, target, What::Link, mode);
        Ok(())
    } else if kind.is_file() {
        let job = Job {
            source: source.to_path_buf(),
            meta: meta.clone(),
            target: target.to_path_buf(),
            replace,
            mode,
        };
        let job = if meta.len() < BIG_FILE {
            match ctx.queue(job) {
                None => return Ok(()),
                Some(job) => job,
            }
        } else {
            job
        };
        let copy_len = copy_file(ctx, &job.source, &job.meta, &job.target, replace, mode)?;
        ctx.progress.add_files(1);
        ctx.remember_file(source, meta, target, copy_len, mode);
        Ok(())
    } else {
        Err(OpError::Other(format!(
            "{} is not a regular file, so it cannot be copied.",
            name_of(source)
        )))
    }
}

fn copy_link(
    ctx: &Ctx<'_>,
    source: &Path,
    target: &Path,
    replace: bool,
    mode: Mode,
) -> Result<(), OpError> {
    let destination = fs::read_link(source)?;
    if replace {
        let temp = temp_path(target.parent().unwrap_or(Path::new(".")));
        std::os::unix::fs::symlink(&destination, &temp)?;
        if let Err(error) = fs::rename(&temp, target) {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
    } else {
        std::os::unix::fs::symlink(&destination, target)?;
    }
    if let Some(dir) = target.parent() {
        ctx.name_added(dir, mode);
    }
    Ok(())
}

/// Copy a folder. `created` is false when merging into an existing folder,
/// which is then left with its own mode and time.
fn copy_dir(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    mode: Mode,
    created: bool,
) -> Result<(), OpError> {
    ctx.progress.set_current(&name_of(source));
    let permissions = meta.mode() & 0o7777;
    if created {
        // Owner can always write while the contents arrive; the real mode
        // is set when they all have.
        fs::DirBuilder::new()
            .mode(permissions | 0o700)
            .create(target)?;
        if let Some(parent) = target.parent() {
            ctx.name_added(parent, mode);
        }
        ctx.name_added(target, mode);
        lock(&ctx.finish).push(Finish {
            path: target.to_path_buf(),
            meta: meta.clone(),
        });
    }
    ctx.remember(source, target, What::Dir, mode);
    let mut children: Vec<PathBuf> = fs::read_dir(source)?
        .map(|child| child.map(|child| child.path()))
        .collect::<io::Result<_>>()?;
    children.sort();
    for child in children {
        place(ctx, &child, target, mode, false)?;
    }
    ctx.progress.add_files(1);
    Ok(())
}

fn copied_times(meta: &Metadata) -> fs::FileTimes {
    let mut times = fs::FileTimes::new();
    if let Ok(accessed) = meta.accessed() {
        times = times.set_accessed(accessed);
    }
    if let Ok(modified) = meta.modified() {
        times = times.set_modified(modified);
    }
    times
}

/// Copy one regular file: data, mode, times and extended attributes, to a
/// part file beside `target`; made durable where the drive needs it; then
/// renamed into place. A failed or cancelled copy leaves no part file.
/// Returns the length of the copy.
fn copy_file(
    ctx: &Ctx<'_>,
    source: &Path,
    meta: &Metadata,
    target: &Path,
    replace: bool,
    mode: Mode,
) -> Result<u64, OpError> {
    ctx.check()?;
    ctx.progress.set_current(&name_of(source));
    let level = ctx.level_for(mode);
    let dir = target
        .parent()
        .ok_or_else(|| OpError::Other("That cannot be copied or moved.".into()))?;
    let input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)?;
    let temp = temp_path(dir);
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(meta.mode() & 0o777)
        .open(&temp)?;
    let written = (|| -> Result<u64, OpError> {
        copy_data(
            &input,
            &output,
            meta,
            ctx.chunk,
            ctx.progress,
            &|| ctx.check(),
            &ctx.hints,
        )?;
        copy_xattrs(&input, &output);
        // FAT and friends have no modes, and say so with an error.
        tolerate(output.set_permissions(fs::Permissions::from_mode(meta.mode() & 0o7777)))?;
        tolerate(output.set_times(copied_times(meta)))?;
        ctx.durable.file_written(&output, &level)?;
        Ok(output.metadata()?.len())
    })();
    drop(output);
    let length = match written {
        Ok(length) => length,
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
    };
    let placed = if replace {
        fs::rename(&temp, target)
    } else {
        // Another program may have made the name since it was looked at.
        rename_noreplace(&temp, target)
    };
    if let Err(error) = placed {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    ctx.name_added(dir, mode);
    Ok(length)
}

#[cfg(test)]
mod durability;

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;
    use std::sync::{Arc, Mutex};

    fn run_copy(
        paths: &[PathBuf],
        dest: &Path,
        policy: &ConflictPolicy,
    ) -> Result<Vec<PathBuf>, OpError> {
        copy(paths, dest, policy, &Progress::new(), &Cancel::new())
    }

    fn run_move(
        paths: &[PathBuf],
        dest: &Path,
        policy: &ConflictPolicy,
    ) -> Result<Vec<PathBuf>, OpError> {
        move_to(paths, dest, policy, &Progress::new(), &Cancel::new())
    }

    fn read(path: impl AsRef<Path>) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn copies_a_tree_with_modes_times_and_links() {
        let temp = Temp::new("copy-tree");
        let src = temp.dir("src/tree");
        temp.write("src/tree/a.txt", "alpha");
        let script = temp.write("src/tree/sub/run.sh", "#!/bin/sh\n");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();
        std::os::unix::fs::symlink("a.txt", src.join("link")).unwrap();
        std::os::unix::fs::symlink("/nonexistent", src.join("dangling")).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        File::options()
            .write(true)
            .open(src.join("a.txt"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::create_dir(src.join("empty")).unwrap();
        let dest = temp.dir("dest");

        let progress = Progress::new();
        let done = copy(
            &[src.clone()],
            &dest,
            &ConflictPolicy::Skip,
            &progress,
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(done, vec![dest.join("tree")]);
        let copy_root = dest.join("tree");
        assert_eq!(read(copy_root.join("a.txt")), "alpha");
        assert_eq!(
            fs::metadata(copy_root.join("a.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            old
        );
        assert_eq!(
            fs::metadata(copy_root.join("sub/run.sh")).unwrap().mode() & 0o7777,
            0o750
        );
        assert_eq!(
            fs::read_link(copy_root.join("link")).unwrap(),
            Path::new("a.txt")
        );
        assert_eq!(
            fs::read_link(copy_root.join("dangling")).unwrap(),
            Path::new("/nonexistent")
        );
        assert!(copy_root.join("empty").is_dir());
        assert_eq!(read(src.join("a.txt")), "alpha", "the source stays");
        assert!(!progress.is_scanning());
        assert_eq!(progress.fraction(), Some(1.0));
        assert_eq!(progress.bytes_total(), 5 + 10);
        assert_eq!(progress.bytes_done(), 15);
        // tree, a.txt, sub, run.sh, link, dangling, empty.
        assert_eq!(progress.files_total(), 7);
        assert_eq!(progress.files_done(), 7);
    }

    #[test]
    fn a_read_only_folder_is_still_filled() {
        let temp = Temp::new("copy-ro");
        let src = temp.dir("ro");
        temp.write("ro/a.txt", "a");
        fs::set_permissions(&src, fs::Permissions::from_mode(0o555)).unwrap();
        let dest = temp.dir("dest");
        run_copy(&[src.clone()], &dest, &ConflictPolicy::Skip).unwrap();
        assert_eq!(read(dest.join("ro/a.txt")), "a");
        assert_eq!(fs::metadata(dest.join("ro")).unwrap().mode() & 0o777, 0o555);
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(dest.join("ro"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn copy_into_the_same_folder_makes_a_copy() {
        let temp = Temp::new("copy-same");
        let file = temp.write("a.txt", "a");
        let done = run_copy(&[file.clone()], temp.path(), &ConflictPolicy::Skip).unwrap();
        assert_eq!(done, vec![temp.path().join("a (copy).txt")]);
        let done = run_copy(&[file.clone()], temp.path(), &ConflictPolicy::Skip).unwrap();
        assert_eq!(done, vec![temp.path().join("a (another copy).txt")]);
        assert_eq!(read(&file), "a");
    }

    #[test]
    fn a_folder_cannot_go_into_itself() {
        let temp = Temp::new("copy-self");
        let dir = temp.dir("d");
        let inner = temp.dir("d/inner");
        for dest in [&dir, &inner] {
            let error = run_copy(&[dir.clone()], dest, &ConflictPolicy::KeepBoth).unwrap_err();
            assert_eq!(
                error,
                OpError::Other("A folder cannot be put inside itself.".into())
            );
            let error = run_move(&[dir.clone()], dest, &ConflictPolicy::KeepBoth);
            assert!(error.is_err() || dest == &dir, "{error:?}");
        }
        assert!(!inner.join("d").exists());
        assert!(dir.exists());
    }

    #[test]
    fn conflict_policies() {
        let temp = Temp::new("conflicts");
        let src = temp.write("src/a.txt", "new");
        let dest = temp.dir("dest");
        let existing = temp.write("dest/a.txt", "old");

        let done = run_copy(&[src.clone()], &dest, &ConflictPolicy::Skip).unwrap();
        assert!(done.is_empty());
        assert_eq!(read(&existing), "old");

        let done = run_copy(&[src.clone()], &dest, &ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(done, vec![dest.join("a (copy).txt")]);
        assert_eq!(read(&existing), "old");
        assert_eq!(read(dest.join("a (copy).txt")), "new");

        let done = run_copy(&[src.clone()], &dest, &ConflictPolicy::Replace).unwrap();
        assert_eq!(done, vec![existing.clone()]);
        assert_eq!(read(&existing), "new");
        let leftovers: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn ask_gets_the_paths_and_can_cancel() {
        let temp = Temp::new("ask");
        let src = temp.write("src/a.txt", "new");
        let dest = temp.dir("dest");
        let existing = temp.write("dest/a.txt", "old");
        let seen: Arc<Mutex<Vec<Conflict>>> = Arc::default();
        let sink = seen.clone();
        let policy = ConflictPolicy::Ask(Box::new(move |conflict| {
            sink.lock().unwrap().push(conflict.clone());
            Resolution::Cancel
        }));
        assert_eq!(
            run_copy(&[src.clone()], &dest, &policy),
            Err(OpError::Cancelled)
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![Conflict {
                source: src.clone(),
                existing: existing.clone()
            }]
        );
        assert_eq!(read(&existing), "old");
    }

    #[test]
    fn ask_can_remember_an_answer_for_all() {
        let temp = Temp::new("ask-all");
        let a = temp.write("src/a.txt", "A");
        let b = temp.write("src/b.txt", "B");
        let c = temp.write("src/c.txt", "C");
        let dest = temp.dir("dest");
        temp.write("dest/a.txt", "old a");
        temp.write("dest/b.txt", "old b");
        let asked = Arc::new(Mutex::new(0));
        let counter = asked.clone();
        let policy = ConflictPolicy::Ask(Box::new(move |_| {
            *counter.lock().unwrap() += 1;
            Resolution::Replace
        }));
        let done = run_copy(&[a, b, c], &dest, &policy).unwrap();
        assert_eq!(done.len(), 3);
        assert_eq!(*asked.lock().unwrap(), 2, "only the clashes are asked");
        assert_eq!(read(dest.join("a.txt")), "A");
        assert_eq!(read(dest.join("b.txt")), "B");
        assert_eq!(read(dest.join("c.txt")), "C");
    }

    #[test]
    fn replace_merges_folders_and_asks_again_for_the_contents() {
        let temp = Temp::new("merge");
        let src = temp.dir("src/d");
        temp.write("src/d/same.txt", "new same");
        temp.write("src/d/only-src.txt", "src");
        let dest = temp.dir("dest");
        temp.write("dest/d/same.txt", "old same");
        temp.write("dest/d/only-dest.txt", "dest");
        let asked = Arc::new(Mutex::new(Vec::new()));
        let sink = asked.clone();
        let policy = ConflictPolicy::Ask(Box::new(move |conflict| {
            sink.lock().unwrap().push(conflict.existing.clone());
            if conflict.existing.is_dir() {
                Resolution::Replace
            } else {
                Resolution::Skip
            }
        }));
        let done = run_copy(&[src.clone()], &dest, &policy).unwrap();
        assert_eq!(done, vec![dest.join("d")]);
        assert_eq!(
            *asked.lock().unwrap(),
            vec![dest.join("d"), dest.join("d/same.txt")]
        );
        assert_eq!(read(dest.join("d/same.txt")), "old same", "skipped");
        assert_eq!(read(dest.join("d/only-src.txt")), "src", "merged in");
        assert_eq!(read(dest.join("d/only-dest.txt")), "dest", "kept");
    }

    #[test]
    fn a_file_cannot_replace_a_folder() {
        let temp = Temp::new("file-over-dir");
        let src = temp.write("src/x", "file");
        let dest = temp.dir("dest");
        temp.write("dest/x/inner.txt", "keep");
        let error = run_copy(&[src], &dest, &ConflictPolicy::Replace).unwrap_err();
        assert!(matches!(error, OpError::Other(_)), "{error:?}");
        assert_eq!(read(dest.join("x/inner.txt")), "keep");
    }

    #[test]
    fn a_folder_can_replace_a_file() {
        let temp = Temp::new("dir-over-file");
        let src = temp.dir("src/x");
        temp.write("src/x/inner.txt", "dir");
        let dest = temp.dir("dest");
        temp.write("dest/x", "file");
        run_copy(&[src], &dest, &ConflictPolicy::Replace).unwrap();
        assert_eq!(read(dest.join("x/inner.txt")), "dir");
    }

    #[test]
    fn cancel_stops_and_leaves_no_half_file() {
        let temp = Temp::new("cancel");
        let a = temp.write("src/a.txt", "a");
        let b = temp.write("src/b.txt", "b");
        let dest = temp.dir("dest");
        let cancel = Cancel::new();
        cancel.cancel();
        let error = copy(
            &[a.clone(), b.clone()],
            &dest,
            &ConflictPolicy::Skip,
            &Progress::new(),
            &cancel,
        )
        .unwrap_err();
        assert_eq!(error, OpError::Cancelled);
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);

        // Cancelled by the question.
        let cancel = Cancel::new();
        let stop = cancel.clone();
        let temp2 = Temp::new("cancel-mid");
        let a = temp2.write("src/a.txt", "a");
        let b = temp2.write("src/b.txt", "b");
        let dest2 = temp2.dir("dest");
        temp2.write("dest/b.txt", "old");
        let policy = ConflictPolicy::Ask(Box::new(move |_| {
            stop.cancel();
            Resolution::Replace
        }));
        // `a` copies; the clash on `b` cancels the token, and Replace still
        // finishes `b`, but the next check stops anything further.
        let result = copy(&[a, b], &dest2, &policy, &Progress::new(), &cancel);
        assert!(matches!(result, Ok(_) | Err(OpError::Cancelled)));
        // `a` was queued for a worker, which gives up once the token is set:
        // it is whole or not there, never half.
        if dest2.join("a.txt").exists() {
            assert_eq!(read(dest2.join("a.txt")), "a");
        }
        assert!(
            fs::read_dir(&dest2)
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".part"))
        );
    }

    #[test]
    fn delete_removes_trees_without_following_links() {
        let temp = Temp::new("delete");
        let outside = temp.write("outside/keep.txt", "keep");
        let doomed = temp.dir("doomed");
        temp.write("doomed/a.txt", "a");
        temp.write("doomed/sub/b.txt", "bb");
        std::os::unix::fs::symlink(temp.path().join("outside"), doomed.join("link")).unwrap();
        let single = temp.write("single.txt", "s");
        let progress = Progress::new();
        delete(&[doomed.clone(), single.clone()], &progress, &Cancel::new()).unwrap();
        assert!(!doomed.exists());
        assert!(!single.exists());
        assert_eq!(read(&outside), "keep", "the link target is untouched");
        // doomed, a.txt, sub, b.txt, link, single.txt
        assert_eq!(progress.files_done(), 6);
        assert_eq!(progress.bytes_done(), 1 + 2 + 1);
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn delete_refuses_the_root_and_obeys_cancel() {
        let temp = Temp::new("delete-guard");
        let file = temp.write("a.txt", "a");
        let error = delete(&[PathBuf::from("/")], &Progress::new(), &Cancel::new());
        assert!(matches!(error, Err(OpError::Other(_))));
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(
            delete(&[file.clone()], &Progress::new(), &cancel),
            Err(OpError::Cancelled)
        );
        assert!(file.exists());
        assert_eq!(
            delete(
                &[temp.path().join("missing")],
                &Progress::new(),
                &Cancel::new()
            ),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn move_renames_on_one_drive() {
        let temp = Temp::new("move");
        let file = temp.write("src/a.txt", "a");
        let dir = temp.dir("src/d");
        temp.write("src/d/inner.txt", "i");
        let dest = temp.dir("dest");
        let done = run_move(&[file.clone(), dir.clone()], &dest, &ConflictPolicy::Skip).unwrap();
        assert_eq!(done, vec![dest.join("a.txt"), dest.join("d")]);
        assert!(!file.exists());
        assert!(!dir.exists());
        assert_eq!(read(dest.join("d/inner.txt")), "i");
    }

    #[test]
    fn move_into_the_same_folder_is_a_no_op() {
        let temp = Temp::new("move-same");
        let file = temp.write("a.txt", "a");
        let done = run_move(&[file.clone()], temp.path(), &ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(done, vec![file.clone()]);
        assert_eq!(read(&file), "a");
        assert!(!temp.path().join("a (copy).txt").exists());
    }

    #[test]
    fn move_conflicts() {
        let temp = Temp::new("move-conflict");
        let src = temp.write("src/a.txt", "new");
        let dest = temp.dir("dest");
        temp.write("dest/a.txt", "old");

        let done = run_move(&[src.clone()], &dest, &ConflictPolicy::Skip).unwrap();
        assert!(done.is_empty());
        assert!(src.exists(), "a skipped source stays");

        let done = run_move(&[src.clone()], &dest, &ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(done, vec![dest.join("a (copy).txt")]);
        assert!(!src.exists());
        assert_eq!(read(dest.join("a.txt")), "old");
        assert_eq!(read(dest.join("a (copy).txt")), "new");

        let src = temp.write("src/a.txt", "newest");
        let done = run_move(&[src.clone()], &dest, &ConflictPolicy::Replace).unwrap();
        assert_eq!(done, vec![dest.join("a.txt")]);
        assert!(!src.exists());
        assert_eq!(read(dest.join("a.txt")), "newest");
    }

    #[test]
    fn move_merges_folders_and_keeps_skipped_sources() {
        let temp = Temp::new("move-merge");
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
        run_move(&[src.clone()], &dest, &policy).unwrap();
        assert_eq!(read(dest.join("d/fresh.txt")), "fresh");
        assert_eq!(read(dest.join("d/clash.txt")), "dest clash");
        assert!(!src.join("fresh.txt").exists());
        assert_eq!(read(src.join("clash.txt")), "src clash", "skipped, so kept");
    }

    /// A folder on a different drive from the temp dir, if this machine has one.
    fn other_drive() -> Option<Temp> {
        let here = fs::metadata(std::env::temp_dir()).ok()?.dev();
        for candidate in ["/dev/shm", "/var/tmp", "/run/user"] {
            let path = Path::new(candidate);
            if fs::metadata(path).is_ok_and(|meta| meta.dev() != here && meta.is_dir()) {
                let probe = path.join(format!("bingux-files-ops-probe-{}", std::process::id()));
                if fs::create_dir(&probe).is_ok() {
                    let _ = fs::remove_dir(&probe);
                    return Some(Temp::in_dir(path, "other"));
                }
            }
        }
        None
    }

    #[test]
    fn move_across_drives_copies_then_deletes() {
        let Some(other) = other_drive() else {
            eprintln!("skipped: no second drive to test with");
            return;
        };
        let temp = Temp::new("move-xdev");
        let dir = temp.dir("src/d");
        temp.write("src/d/a.txt", "aaaa");
        temp.write("src/d/sub/b.txt", "bb");
        let file = temp.write("src/f.txt", "ffff");
        let dest = other.dir("dest");
        let progress = Progress::new();
        let done = move_to(
            &[dir.clone(), file.clone()],
            &dest,
            &ConflictPolicy::Skip,
            &progress,
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(done, vec![dest.join("d"), dest.join("f.txt")]);
        assert_eq!(read(dest.join("d/sub/b.txt")), "bb");
        assert_eq!(read(dest.join("f.txt")), "ffff");
        assert!(!dir.exists());
        assert!(!file.exists());
        assert_eq!(progress.bytes_done(), 4 + 2 + 4);
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn a_cancelled_cross_drive_move_keeps_the_source() {
        let Some(other) = other_drive() else {
            eprintln!("skipped: no second drive to test with");
            return;
        };
        let temp = Temp::new("move-xdev-cancel");
        let file = temp.write("src/f.txt", "ffff");
        let dest = other.dir("dest");
        let cancel = Cancel::new();
        cancel.cancel();
        let error = move_to(
            &[file.clone()],
            &dest,
            &ConflictPolicy::Skip,
            &Progress::new(),
            &cancel,
        )
        .unwrap_err();
        assert_eq!(error, OpError::Cancelled);
        assert_eq!(read(&file), "ffff");
        assert!(!dest.join("f.txt").exists());
    }

    #[test]
    fn a_destination_that_is_not_a_folder_is_refused() {
        let temp = Temp::new("bad-dest");
        let file = temp.write("a.txt", "a");
        let other = temp.write("b.txt", "b");
        assert!(matches!(
            run_copy(&[file.clone()], &other, &ConflictPolicy::Skip),
            Err(OpError::Other(_))
        ));
        assert_eq!(
            run_copy(&[file], &temp.path().join("missing"), &ConflictPolicy::Skip),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn a_missing_source_is_not_found() {
        let temp = Temp::new("missing-source");
        let dest = temp.dir("dest");
        assert_eq!(
            run_copy(&[temp.path().join("nope")], &dest, &ConflictPolicy::Skip),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn copies_large_files_in_chunks() {
        let temp = Temp::new("big");
        let big = temp.path().join("big.bin");
        let data: Vec<u8> = (0..((16 << 20) + 12345)).map(|i| (i % 251) as u8).collect();
        fs::write(&big, &data).unwrap();
        let dest = temp.dir("dest");
        let progress = Progress::new();
        copy(
            &[big.clone()],
            &dest,
            &ConflictPolicy::Skip,
            &progress,
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(fs::read(dest.join("big.bin")).unwrap(), data);
        assert_eq!(progress.bytes_done(), data.len() as u64);
    }
}
