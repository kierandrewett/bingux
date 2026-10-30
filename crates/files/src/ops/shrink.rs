//! Making pictures, films and songs smaller: [`estimate`] says how big the
//! result would be, [`reduce`] does it.
//!
//! # What it promises
//!
//! - Each file is made in a hidden folder beside the result and gets its name
//!   by a rename, only after the new file was opened again and looked right
//!   (it has a picture or a sound, a length close to the original's) and its
//!   bytes were written out (`fsync`, and `syncfs` on removable media and
//!   shares). A failure or a cancel removes the hidden folder: no partial file
//!   is ever left behind.
//! - A file that would not get smaller is kept as it is: the result says
//!   "Already small" and nothing is made.
//! - The result sits beside the original as `name (compressed).ext`, never
//!   over another file. With `replace_original` it takes the original's name
//!   and the original goes to the Trash, after the new file is in place and
//!   never before; if the rename fails the original is put back.
//! - The time and permissions of the original carry over to the result. The
//!   metadata (tags, EXIF, location) carries over unless the options strip
//!   it.
//! - The programs run at low priority, one or two at a time, and die with
//!   this process.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::Cancel;
use crate::mime;

use super::durable::{sync_dirs, sync_written};
use super::media_plan::{
    Beat, MediaKind, Plan, Probe, ReduceOptions, Tools, audio_plan,
    estimate_sizes, fraction_done, free_reduced_name, image_extension, image_plan, lossy_size,
    parse_probe, parse_progress, probe_spec, sample_window, scale_sample, split_name, video_plan,
};
use super::pack::Work;
use super::tool::{self, Stream};
use super::trash::{Trashed, trash, untrash};
use super::{OpError, Progress, rename_noreplace};

/// How long a sample of a film or song is, in seconds, for an estimate.
pub const SAMPLE_SECONDS: f64 = 4.0;

/// The kind of media the file is, from its name.
pub fn kind_of_name(name: &str) -> Option<MediaKind> {
    MediaKind::of(mime::for_name(name).category)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn probe(tools: &Tools, path: &Path, cancel: &Cancel) -> Option<Probe> {
    let ffprobe = tools.ffprobe.as_ref()?;
    let (finished, out) = tool::capture(&probe_spec(ffprobe, path), cancel, 64 * 1024).ok()?;
    finished.success().then(|| parse_probe(&String::from_utf8_lossy(&out)))
}

fn extension_for(kind: MediaKind, options: &ReduceOptions, name: &str) -> Option<String> {
    match kind {
        MediaKind::Image => image_extension(&options.image, name).map(str::to_string),
        MediaKind::Video => Some(options.video.codec.extension().to_string()),
        MediaKind::Audio => Some(options.audio.codec.extension().to_string()),
    }
}

/// The plan for `input` into `out`, and what was probed.
fn plan_for(
    tools: &Tools,
    kind: MediaKind,
    options: &ReduceOptions,
    input: &Path,
    out: &Path,
    window: Option<(f64, f64)>,
    cancel: &Cancel,
) -> Result<(Plan, Option<Probe>), OpError> {
    let unsupported = |reason: String| OpError::Unsupported(reason);
    match kind {
        MediaKind::Image => {
            let needs_probe = options.image.max_dimension.is_some();
            let probe = needs_probe.then(|| probe(tools, input, cancel)).flatten();
            let plan = image_plan(tools, &options.image, input, out, probe.as_ref()).map_err(unsupported)?;
            Ok((plan, probe))
        }
        MediaKind::Video => {
            let probe = probe(tools, input, cancel)
                .ok_or_else(|| OpError::Other("The video could not be read.".into()))?;
            if !probe.has_video {
                return Err(OpError::Other("The file has no picture to reduce.".into()));
            }
            let plan = video_plan(tools, &options.video, input, out, &probe, window).map_err(unsupported)?;
            Ok((plan, Some(probe)))
        }
        MediaKind::Audio => {
            let probe = probe(tools, input, cancel)
                .ok_or_else(|| OpError::Other("The audio could not be read.".into()))?;
            if !probe.has_audio {
                return Err(OpError::Other("The file has no sound to reduce.".into()));
            }
            let plan = audio_plan(tools, &options.audio, input, out, &probe, window).map_err(unsupported)?;
            Ok((plan, Some(probe)))
        }
    }
}

// ---------------------------------------------------------------------------
// Estimates
// ---------------------------------------------------------------------------

/// How big the result would be.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Estimate {
    /// One entry per file, in order: the estimated size, or `None` for a file
    /// that cannot be reduced.
    pub per_file: Vec<Option<u64>>,
    pub original: u64,
    pub estimated: u64,
}

fn temp_work() -> Result<Work, OpError> {
    Work::make(&std::env::temp_dir())
}

/// Encode a sample of `path` (the whole of a picture, a few seconds of a film
/// or song) at the chosen settings and return its size and the seconds it
/// covers.
fn sample(
    tools: &Tools,
    kind: MediaKind,
    options: &ReduceOptions,
    path: &Path,
    cancel: &Cancel,
) -> Result<(u64, f64, Option<f64>), OpError> {
    let name = name_of(path);
    let extension = extension_for(kind, options, &name)
        .ok_or_else(|| OpError::Unsupported("This kind of file cannot be reduced here.".into()))?;
    let work = temp_work()?;
    let out = work.0.join(format!("sample.{extension}"));
    let full = probe(tools, path, cancel);
    let window = full
        .as_ref()
        .and_then(|probe| probe.duration)
        .and_then(|duration| sample_window(duration, SAMPLE_SECONDS));
    let (plan, _) = plan_for(tools, kind, options, path, &out, window, cancel)?;
    let finished = tool::run(&plan.spec, cancel, &mut |_, _| {})?;
    if !finished.success() {
        return Err(finished.sentence("The size could not be estimated"));
    }
    let bytes = fs::metadata(&out)?.len();
    let seconds = window.map_or_else(|| full.as_ref().and_then(|p| p.duration).unwrap_or(0.0), |(_, length)| length);
    Ok((bytes, seconds, full.and_then(|p| p.duration)))
}

/// Estimate the size of every file once reduced. Encodes the first picture
/// at the chosen quality, and a few seconds of the first film; a song's size
/// follows from its bitrate. Blocks and takes a moment: call it from a worker
/// and stop it with `cancel` when the settings change.
pub fn estimate(files: &[PathBuf], options: &ReduceOptions, cancel: &Cancel) -> Result<Estimate, OpError> {
    let tools = Tools::get();
    let sizes: Vec<u64> = files
        .iter()
        .map(|path| fs::metadata(path).map_or(0, |meta| meta.len()))
        .collect();
    let kinds: Vec<Option<MediaKind>> = files.iter().map(|path| kind_of_name(&name_of(path))).collect();
    let mut per_file: Vec<Option<u64>> = vec![None; files.len()];

    // Pictures: one sample sets the ratio for all.
    let pictures: Vec<usize> = (0..files.len()).filter(|&i| kinds[i] == Some(MediaKind::Image)).collect();
    let reducible = |i: usize| super::media_plan::image_format_of(&name_of(&files[i])).is_some();
    let pictures: Vec<usize> = pictures.into_iter().filter(|&i| reducible(i)).collect();
    if let Some(&first) = pictures.first() {
        let (out, _, _) = sample(tools, MediaKind::Image, options, &files[first], cancel)?;
        let wanted: Vec<u64> = pictures.iter().map(|&i| sizes[i]).collect();
        let estimated = estimate_sizes(&wanted, sizes[first], out);
        for (&i, size) in pictures.iter().zip(estimated) {
            per_file[i] = Some(size);
        }
    }
    // Films: a sample of the first, scaled by length, sets a bitrate ratio.
    let films: Vec<usize> = (0..files.len()).filter(|&i| kinds[i] == Some(MediaKind::Video)).collect();
    if let Some(&first) = films.first() {
        let (bytes, seconds, duration) = sample(tools, MediaKind::Video, options, &files[first], cancel)?;
        let whole = match duration {
            Some(duration) if seconds > 0.0 => scale_sample(bytes, seconds, duration),
            _ => bytes,
        };
        let wanted: Vec<u64> = films.iter().map(|&i| sizes[i]).collect();
        let estimated = estimate_sizes(&wanted, sizes[first], whole);
        for (&i, size) in films.iter().zip(estimated) {
            per_file[i] = Some(size);
        }
    }
    // Songs: a bitrate is a size; lossless ones are sampled.
    for i in (0..files.len()).filter(|&i| kinds[i] == Some(MediaKind::Audio)) {
        let duration = probe(tools, &files[i], cancel).and_then(|probe| probe.duration);
        per_file[i] = Some(match (options.audio.codec.has_bitrate(), duration) {
            (true, Some(duration)) => lossy_size(options.audio.kbps, duration),
            _ => {
                let (bytes, seconds, duration) = sample(tools, MediaKind::Audio, options, &files[i], cancel)?;
                match duration {
                    Some(duration) if seconds > 0.0 => scale_sample(bytes, seconds, duration),
                    _ => bytes,
                }
            }
        });
    }
    let original = sizes.iter().sum();
    // A file the reduction would not shrink stays as it is.
    let estimated = per_file
        .iter()
        .zip(&sizes)
        .map(|(estimate, size)| estimate.map_or(*size, |estimate| estimate.min(*size)))
        .sum();
    Ok(Estimate {
        per_file,
        original,
        estimated,
    })
}

// ---------------------------------------------------------------------------
// Reducing
// ---------------------------------------------------------------------------

/// What to reduce.
#[derive(Clone, Debug)]
pub struct ReduceJob {
    /// Absolute paths of the files.
    pub files: Vec<PathBuf>,
    /// Where the results go, or beside each original.
    pub dest_dir: Option<PathBuf>,
    pub options: ReduceOptions,
    /// Files worked on at once: one or two.
    pub parallel: usize,
}

/// How one file ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Reduced {
        output: PathBuf,
        from: u64,
        to: u64,
        /// The original, now in the Trash, when it was replaced.
        original: Option<Trashed>,
        /// Something the person should know, such as why the original was
        /// not moved to the Trash.
        note: Option<String>,
    },
    /// The file would not get smaller.
    Kept { from: u64 },
    /// Not something this can reduce.
    Skipped { reason: String },
    Failed { error: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReport {
    pub source: PathBuf,
    pub outcome: Outcome,
}

/// What [`reduce`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reduced {
    pub files: Vec<FileReport>,
}

impl Reduced {
    /// The files made.
    pub fn created(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter_map(|file| match &file.outcome {
                Outcome::Reduced { output, .. } => Some(output.clone()),
                _ => None,
            })
            .collect()
    }

    /// The originals that were replaced, now in the Trash.
    pub fn trashed(&self) -> Vec<Trashed> {
        self.files
            .iter()
            .filter_map(|file| match &file.outcome {
                Outcome::Reduced { original, .. } => original.clone(),
                _ => None,
            })
            .collect()
    }

    /// How many files came out smaller.
    pub fn reduced(&self) -> usize {
        self.created().len()
    }

    /// The bytes of the originals that were reduced, and of what they became.
    pub fn sizes(&self) -> (u64, u64) {
        self.files.iter().fold((0, 0), |(from, to), file| match &file.outcome {
            Outcome::Reduced { from: a, to: b, .. } => (from + a, to + b),
            _ => (from, to),
        })
    }

    pub fn kept(&self) -> usize {
        self.files.iter().filter(|file| matches!(file.outcome, Outcome::Kept { .. })).count()
    }

    pub fn failures(&self) -> Vec<&FileReport> {
        self.files
            .iter()
            .filter(|file| matches!(file.outcome, Outcome::Failed { .. } | Outcome::Skipped { .. }))
            .collect()
    }
}

/// Where a file in a running batch is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Waiting,
    Working,
    Done,
}

/// A running batch, as the window reads it: one line a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub name: String,
    pub stage: Stage,
    /// 0 to 1, or `None` when not known.
    pub fraction: Option<f32>,
}

/// Shared with the worker: clones see the same lines.
#[derive(Clone, Debug, Default)]
pub struct Batch(Arc<Mutex<Vec<Live>>>);

impl Batch {
    pub fn new() -> Batch {
        Batch::default()
    }

    pub fn lines(&self) -> Vec<Live> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set_all(&self, lines: Vec<Live>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = lines;
    }

    fn update(&self, index: usize, change: impl FnOnce(&mut Live)) {
        if let Some(line) = self.0.lock().unwrap_or_else(PoisonError::into_inner).get_mut(index) {
            change(line);
        }
    }
}

fn exists(dir: &Path, name: &str) -> bool {
    dir.join(name).symlink_metadata().is_ok()
}

/// Give `path` the time and permissions of `like`.
fn carry_over(path: &Path, like: &fs::Metadata) -> Result<(), OpError> {
    fs::set_permissions(path, fs::Permissions::from_mode(like.mode() & 0o7777))?;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| OpError::Other("The path holds a null character.".into()))?;
    let times = [
        libc::timespec {
            tv_sec: like.atime() as libc::time_t,
            tv_nsec: like.atime_nsec() as _,
        },
        libc::timespec {
            tv_sec: like.mtime() as libc::time_t,
            tv_nsec: like.mtime_nsec() as _,
        },
    ];
    // SAFETY: the path is NUL-terminated and `times` holds two entries.
    let status = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// Reduce every file. Blocks; call it from a worker. A file that fails does
/// not stop the others: its report says why. [`Progress`] counts files and
/// the share of each original's bytes done; `batch` has a line a file.
pub fn reduce(job: &ReduceJob, progress: &Progress, batch: &Batch, cancel: &Cancel) -> Result<Reduced, OpError> {
    if job.files.is_empty() {
        return Err(OpError::Other("There is nothing to reduce.".into()));
    }
    if job.files.iter().any(|path| !path.is_absolute()) {
        return Err(OpError::Other("The paths must be absolute.".into()));
    }
    if let Some(dest) = &job.dest_dir
        && !fs::metadata(dest)?.is_dir()
    {
        return Err(OpError::Other("The destination is not a folder.".into()));
    }
    let tools = Tools::get();
    let sizes: Vec<u64> = job
        .files
        .iter()
        .map(|path| fs::metadata(path).map_or(0, |meta| meta.len()))
        .collect();
    progress.add_total(sizes.iter().sum(), job.files.len() as u64);
    batch.set_all(
        job.files
            .iter()
            .map(|path| Live {
                name: name_of(path),
                stage: Stage::Waiting,
                fraction: None,
            })
            .collect(),
    );
    let next = AtomicUsize::new(0);
    let results: Mutex<HashMap<usize, FileReport>> = Mutex::new(HashMap::new());
    let stopped = Mutex::new(false);
    let workers = job.parallel.clamp(1, 2).min(job.files.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= job.files.len() || cancel.is_cancelled() {
                        return;
                    }
                    let path = &job.files[index];
                    batch.update(index, |line| line.stage = Stage::Working);
                    let outcome = match reduce_one(tools, job, index, path, sizes[index], progress, batch, cancel) {
                        Ok(outcome) => outcome,
                        Err(OpError::Cancelled) => {
                            *stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
                            return;
                        }
                        Err(error) => Outcome::Failed { error: error.to_string() },
                    };
                    batch.update(index, |line| {
                        line.stage = Stage::Done;
                        line.fraction = Some(1.0);
                    });
                    progress.add_files(1);
                    results
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(index, FileReport { source: path.clone(), outcome });
                }
            });
        }
    });
    if cancel.is_cancelled() || *stopped.lock().unwrap_or_else(PoisonError::into_inner) {
        // What was finished stays; the person asked to stop, so say so.
        return Err(OpError::Cancelled);
    }
    let mut results = results.into_inner().unwrap_or_else(PoisonError::into_inner);
    let files = (0..job.files.len()).filter_map(|index| results.remove(&index)).collect();
    progress.settle();
    Ok(Reduced { files })
}

#[allow(clippy::too_many_arguments)]
fn reduce_one(
    tools: &Tools,
    job: &ReduceJob,
    index: usize,
    path: &Path,
    size: u64,
    progress: &Progress,
    batch: &Batch,
    cancel: &Cancel,
) -> Result<Outcome, OpError> {
    let name = name_of(path);
    progress.set_current(&name);
    let Some(kind) = kind_of_name(&name) else {
        progress.add_bytes(size);
        return Ok(Outcome::Skipped { reason: "It is not a picture, a film or a song.".into() });
    };
    if kind == MediaKind::Image && super::media_plan::image_format_of(&name).is_none() {
        progress.add_bytes(size);
        return Ok(Outcome::Skipped { reason: "This kind of picture cannot be reduced here.".into() });
    }
    let source_meta = fs::metadata(path)?;
    if !source_meta.is_file() {
        return Ok(Outcome::Skipped { reason: "It is not a file.".into() });
    }
    let out_dir = match &job.dest_dir {
        Some(dest) => dest.clone(),
        None => path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| OpError::Other("The file has no folder.".into()))?,
    };
    let options = &job.options;
    let extension = extension_for(kind, options, &name)
        .ok_or_else(|| OpError::Unsupported("This kind of file cannot be reduced here.".into()))?;
    let work = Work::make(&out_dir)?;
    let made = work.0.join(format!("{index}.{extension}"));
    let (plan, source_probe) = plan_for(tools, kind, options, path, &made, None, cancel)?;
    debug_assert_eq!(plan.extension, extension, "the file is named for the plan that makes it");

    let duration = plan.duration;
    let mut reported = 0u64;
    let finished = tool::run(&plan.spec, cancel, &mut |stream, line| {
        if stream != Stream::Out {
            return;
        }
        if let Some(Beat::Time(seconds)) = parse_progress(line)
            && let Some(fraction) = fraction_done(seconds, duration)
        {
            let done = (size as f64 * f64::from(fraction)) as u64;
            if done > reported {
                progress.add_bytes(done - reported);
                reported = done;
            }
            batch.update(index, |live| live.fraction = Some(fraction));
        }
    })?;
    if !finished.success() {
        return Ok(Outcome::Failed { error: finished.sentence("It could not be reduced").to_string() });
    }
    if reported < size {
        progress.add_bytes(size - reported);
    }
    let out_meta = match fs::metadata(&made) {
        Ok(meta) if meta.len() > 0 => meta,
        _ => return Ok(Outcome::Failed { error: "It could not be reduced: the program wrote nothing.".into() }),
    };

    // Open the new file again before it replaces or sits beside anything.
    if let Some(problem) = verify(tools, kind, &made, source_probe.as_ref(), cancel)? {
        return Ok(Outcome::Failed { error: format!("The new file did not pass its check: {problem}") });
    }
    if out_meta.len() >= size {
        return Ok(Outcome::Kept { from: size });
    }
    carry_over(&made, &source_meta)?;
    sync_written(&made)?;

    let (stem, _) = split_name(&name);
    let mut original = None;
    let mut note = None;
    let output = if options.replace_original && job.dest_dir.is_none() {
        let wanted = format!("{stem}.{extension}");
        let target = out_dir.join(&wanted);
        if target == path {
            // The new file takes the old file's name: the old one goes first.
            let items = trash(std::slice::from_ref(&path.to_path_buf()))?;
            match rename_noreplace(&made, &target) {
                Ok(()) => original = items.into_iter().next(),
                Err(error) => {
                    let _ = untrash(&items);
                    return Err(error.into());
                }
            }
            target
        } else {
            let place = place_beside(&made, &out_dir, stem, &extension, Some(&wanted))?;
            match trash(std::slice::from_ref(&path.to_path_buf())) {
                Ok(items) => original = items.into_iter().next(),
                Err(error) => note = Some(format!("The original was not moved to the Trash: {error}")),
            }
            place
        }
    } else {
        place_beside(&made, &out_dir, stem, &extension, None)?
    };
    sync_dirs(&[&out_dir])?;
    Ok(Outcome::Reduced {
        output,
        from: size,
        to: out_meta.len(),
        original,
        note,
    })
}

/// Move the made file to its name in `dir`: `preferred` if given and free,
/// else the first free `name (compressed).ext`.
fn place_beside(made: &Path, dir: &Path, stem: &str, extension: &str, preferred: Option<&str>) -> Result<PathBuf, OpError> {
    for _ in 0..1000 {
        let name = match preferred {
            Some(name) if !exists(dir, name) => name.to_string(),
            _ => free_reduced_name(stem, extension, |name| exists(dir, name)),
        };
        let to = dir.join(&name);
        match rename_noreplace(made, &to) {
            Ok(()) => return Ok(to),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(OpError::Exists)
}

/// Open the new file with `ffprobe` (or, for a picture, ImageMagick) and say
/// what is wrong with it, if anything.
fn verify(
    tools: &Tools,
    kind: MediaKind,
    made: &Path,
    source: Option<&Probe>,
    cancel: &Cancel,
) -> Result<Option<String>, OpError> {
    let Some(found) = probe(tools, made, cancel) else {
        if tools.ffprobe.is_some() {
            return Ok(Some("it cannot be opened.".into()));
        }
        return Ok(None);
    };
    match kind {
        MediaKind::Image => {
            if found.width.unwrap_or(0) == 0 || found.height.unwrap_or(0) == 0 {
                return Ok(Some("it has no picture.".into()));
            }
        }
        MediaKind::Video => {
            if !found.has_video {
                return Ok(Some("it has no picture.".into()));
            }
        }
        MediaKind::Audio => {
            if !found.has_audio {
                return Ok(Some("it has no sound.".into()));
            }
        }
    }
    if matches!(kind, MediaKind::Video | MediaKind::Audio)
        && let (Some(before), Some(after)) = (source.and_then(|p| p.duration), found.duration)
        && after < before * 0.9 - 0.5
    {
        return Ok(Some(format!("it is {after:.1} s long, not {before:.1} s.")));
    }
    Ok(None)
}

/// The most files worth running at once: films one at a time (an encoder uses
/// every core), pictures and songs two.
pub fn parallelism(files: &[PathBuf]) -> usize {
    if files.iter().any(|path| kind_of_name(&name_of(path)) == Some(MediaKind::Video)) {
        1
    } else {
        2
    }
}

/// Is every file a picture, a film or a song?
pub fn all_media(files: &[PathBuf]) -> bool {
    !files.is_empty() && files.iter().all(|path| path.is_file() && kind_of_name(&name_of(path)).is_some())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::super::media_plan::{
        AudioCodec, AudioOptions, ImageFormat, ImageOptions, Sound, VideoCodec, VideoOptions, VideoQuality,
    };
    use super::super::pack_plan::Level;
    use super::super::testing::{Temp, with_data_home};
    use super::*;

    fn have(tool: &str) -> bool {
        tool::on_path(tool)
    }

    /// Tiny generated fixtures: a noisy picture, a short film with sound and a
    /// short song, made by ffmpeg and ImageMagick themselves.
    fn make(temp: &Temp) -> Option<(PathBuf, PathBuf, PathBuf)> {
        if !(have("ffmpeg") && have("ffprobe")) {
            return None;
        }
        let dir = temp.dir("media");
        let image = dir.join("photo.jpg");
        let film = dir.join("film.mp4");
        let song = dir.join("song.wav");
        let ok = |status: std::io::Result<std::process::ExitStatus>| status.map(|s| s.success()).unwrap_or(false);
        // A high-quality noisy JPEG compresses a lot at quality 40.
        if !ok(Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=1600x1200:rate=1", "-frames:v", "1", "-q:v", "1"])
            .arg(&image)
            .status())
        {
            return None;
        }
        if !ok(Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-c:v", "libx264", "-crf", "12", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "256k", "-metadata", "title=Fixture"])
            .arg(&film)
            .status())
        {
            return None;
        }
        if !ok(Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:duration=4", "-metadata", "artist=Nobody"])
            .arg(&song)
            .status())
        {
            return None;
        }
        Some((image, film, song))
    }

    fn run(job: &ReduceJob) -> Reduced {
        reduce(job, &Progress::new(), &Batch::new(), &Cancel::new()).unwrap()
    }

    fn job(files: Vec<PathBuf>, options: ReduceOptions) -> ReduceJob {
        ReduceJob { files, dest_dir: None, options, parallel: 2 }
    }

    fn probe_of(path: &Path) -> Probe {
        probe(Tools::get(), path, &Cancel::new()).expect("ffprobe opens it")
    }

    #[test]
    fn a_picture_shrinks_beside_the_original_and_opens() {
        let temp = Temp::new("shrink-image");
        let Some((image, ..)) = make(&temp) else { return };
        let mut options = ReduceOptions::default();
        options.image = ImageOptions { quality: 30, max_dimension: Some(800), ..ImageOptions::default() };
        let done = run(&job(vec![image.clone()], options));
        let [FileReport { outcome: Outcome::Reduced { output, from, to, original, .. }, .. }] = done.files.as_slice() else {
            panic!("{:?}", done.files);
        };
        assert_eq!(output, &image.with_file_name("photo (compressed).jpg"));
        assert!(to < from && original.is_none());
        let found = probe_of(output);
        assert_eq!((found.width, found.height), (Some(800), Some(600)));
        assert!(image.exists(), "the original is untouched");
        assert_eq!(fs::metadata(output).unwrap().mtime(), fs::metadata(&image).unwrap().mtime());
        let left: Vec<_> = fs::read_dir(image.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!left.iter().any(|name| name.starts_with(".bingux")), "{left:?}");
        // A second run does not overwrite the first.
        let again = run(&job(vec![image.clone()], ReduceOptions { image: ImageOptions { quality: 30, ..ImageOptions::default() }, ..ReduceOptions::default() }));
        let [FileReport { outcome: Outcome::Reduced { output, .. }, .. }] = again.files.as_slice() else { panic!() };
        assert_eq!(output, &image.with_file_name("photo (compressed 2).jpg"));
    }

    #[test]
    fn pictures_convert_between_formats() {
        let temp = Temp::new("shrink-convert");
        let Some((image, ..)) = make(&temp) else { return };
        for (format, extension) in [(ImageFormat::Webp, "webp"), (ImageFormat::Avif, "avif"), (ImageFormat::Png, "png")] {
            if Tools::get().image_format_unavailable(format).is_some() {
                eprintln!("skipped {format:?}");
                continue;
            }
            let options = ReduceOptions { image: ImageOptions { format, quality: 40, max_dimension: Some(600), ..ImageOptions::default() }, ..ReduceOptions::default() };
            let done = run(&job(vec![image.clone()], options));
            match &done.files[0].outcome {
                Outcome::Reduced { output, to, from, .. } => {
                    assert_eq!(output.extension().unwrap(), extension);
                    assert!(to < from, "{format:?}");
                    assert_eq!(probe_of(output).width, Some(600), "{format:?}");
                }
                // A lossless PNG of this picture may not be smaller than its JPEG.
                Outcome::Kept { .. } if format == ImageFormat::Png => {}
                other => panic!("{format:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn stripping_removes_location_and_camera_data() {
        if !(have("magick") && have("ffmpeg")) {
            return;
        }
        let temp = Temp::new("shrink-strip");
        let Some((image, ..)) = make(&temp) else { return };
        let tagged = image.with_file_name("tagged.jpg");
        // An EXIF block with a camera make and a GPS latitude, put right after the JPEG's start.
        let mut exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x02\x01\x0f\0\x02\0\0\0\x0a\0\0\0\x44\x88\x25\0\x04\0\0\0\x01\0\0\0\x26\0\0\0\0".to_vec();
        exif.extend_from_slice(b"\0\x02\0\x01\0\x02\0\0\0\x02N\0\0\0\0\x02\0\x05\0\0\0\x03\0\0\0\x4e\0\0\0\0");
        exif.extend_from_slice(b"BinguxCam\0");
        for number in [51u32, 1, 30, 1, 0, 1] {
            exif.extend_from_slice(&number.to_be_bytes());
        }
        let jpeg = fs::read(&image).unwrap();
        let mut spliced = jpeg[..2].to_vec();
        spliced.extend_from_slice(&[0xff, 0xe1]);
        spliced.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        spliced.extend_from_slice(&exif);
        spliced.extend_from_slice(&jpeg[2..]);
        fs::write(&tagged, spliced).unwrap();
        let exif = |path: &Path| {
            let out = Command::new("magick").arg("identify").arg("-format").arg("%[exif:Make]|%[exif:GPSLatitude]").arg(path).output().unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        assert!(exif(&tagged).contains("BinguxCam"), "the fixture has EXIF: {}", exif(&tagged));
        for (strip, expect_kept) in [(false, true), (true, false)] {
            let options = ReduceOptions { image: ImageOptions { quality: 30, strip_metadata: strip, ..ImageOptions::default() }, ..ReduceOptions::default() };
            let done = run(&job(vec![tagged.clone()], options));
            let Outcome::Reduced { output, .. } = &done.files[0].outcome else { panic!("{:?}", done.files) };
            let found = exif(output);
            assert_eq!(found.contains("BinguxCam"), expect_kept, "strip={strip}: {found}");
            assert_eq!(found.contains("51/1"), expect_kept, "strip={strip}: {found}");
            fs::remove_file(output).unwrap();
        }
    }

    #[test]
    fn a_film_shrinks_with_caps_and_keeps_its_sound_and_title() {
        let temp = Temp::new("shrink-film");
        let Some((_, film, _)) = make(&temp) else { return };
        let options = ReduceOptions {
            video: VideoOptions {
                quality: VideoQuality::Level(Level::Fastest),
                max_height: Some(180),
                max_fps: Some(15),
                sound: Sound::Kbps(64),
                ..VideoOptions::default()
            },
            ..ReduceOptions::default()
        };
        let progress = Progress::new();
        let batch = Batch::new();
        let done = reduce(&job(vec![film.clone()], options), &progress, &batch, &Cancel::new()).unwrap();
        let Outcome::Reduced { output, from, to, .. } = &done.files[0].outcome else { panic!("{:?}", done.files) };
        assert!(to < from);
        let found = probe_of(output);
        assert_eq!(found.height, Some(180));
        assert!(found.has_audio && found.has_video);
        assert!((found.fps.unwrap() - 15.0).abs() < 0.5, "{found:?}");
        assert!((found.duration.unwrap() - 3.0).abs() < 0.3);
        let tags = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags=title", "-of", "default=nw=1:nk=1"]).arg(output).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&tags.stdout).trim(), "Fixture", "metadata is kept");
        assert_eq!(progress.fraction(), Some(1.0));
        assert_eq!(batch.lines()[0].stage, Stage::Done);
    }

    #[test]
    fn a_film_without_sound_and_with_stripped_tags() {
        let temp = Temp::new("shrink-silent");
        let Some((_, film, _)) = make(&temp) else { return };
        let options = ReduceOptions {
            video: VideoOptions { sound: Sound::Remove, strip_metadata: true, max_height: Some(180), quality: VideoQuality::Crf(32), ..VideoOptions::default() },
            ..ReduceOptions::default()
        };
        let done = run(&job(vec![film], options));
        let Outcome::Reduced { output, .. } = &done.files[0].outcome else { panic!("{:?}", done.files) };
        assert!(!probe_of(output).has_audio);
        let tags = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags=title", "-of", "default=nw=1:nk=1"]).arg(output).output().unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).trim().is_empty());
    }

    #[test]
    fn each_available_film_codec_makes_a_file_that_opens() {
        let temp = Temp::new("shrink-codecs");
        let Some((_, film, _)) = make(&temp) else { return };
        for codec in VideoCodec::ALL {
            if Tools::get().video_codec_unavailable(codec).is_some() {
                eprintln!("skipped {codec:?}");
                continue;
            }
            let options = ReduceOptions {
                video: VideoOptions { codec, quality: VideoQuality::Level(Level::Fastest), max_height: Some(180), sound: Sound::Kbps(64), ..VideoOptions::default() },
                ..ReduceOptions::default()
            };
            let done = run(&job(vec![film.clone()], options));
            let Outcome::Reduced { output, .. } = &done.files[0].outcome else { panic!("{codec:?}: {:?}", done.files) };
            assert_eq!(output.extension().unwrap(), codec.extension());
            assert!(probe_of(output).has_video, "{codec:?}");
            fs::remove_file(output).unwrap();
        }
    }

    #[test]
    fn a_song_becomes_each_available_codec_and_keeps_its_tags() {
        let temp = Temp::new("shrink-song");
        let Some((_, _, song)) = make(&temp) else { return };
        for codec in AudioCodec::ALL {
            if Tools::get().audio_codec_unavailable(codec).is_some() {
                eprintln!("skipped {codec:?}");
                continue;
            }
            let options = ReduceOptions { audio: AudioOptions { codec, kbps: 64, strip_metadata: false }, ..ReduceOptions::default() };
            let done = run(&job(vec![song.clone()], options));
            let Outcome::Reduced { output, from, to, .. } = &done.files[0].outcome else { panic!("{codec:?}: {:?}", done.files) };
            assert!(to < from, "{codec:?}");
            assert!(probe_of(output).has_audio);
            let tags = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags=artist:stream_tags=artist", "-of", "default=nw=1:nk=1"]).arg(output).output().unwrap();
            assert!(String::from_utf8_lossy(&tags.stdout).contains("Nobody"), "{codec:?} keeps its tags");
            fs::remove_file(output).unwrap();
        }
    }

    #[test]
    fn a_file_that_would_grow_is_kept() {
        let temp = Temp::new("shrink-small");
        let Some((_, film, _)) = make(&temp) else { return };
        // A tiny clip at a very high quality grows.
        let tiny = film.with_file_name("tiny.mp4");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&film)
            .args(["-t", "0.2", "-vf", "scale=64:36", "-c:v", "libx264", "-crf", "45", "-preset", "veryslow", "-an"])
            .arg(&tiny)
            .status()
            .unwrap();
        assert!(made.success());
        let options = ReduceOptions {
            video: VideoOptions { quality: VideoQuality::Crf(16), sound: Sound::Remove, ..VideoOptions::default() },
            ..ReduceOptions::default()
        };
        let done = run(&job(vec![tiny.clone()], options));
        assert!(matches!(done.files[0].outcome, Outcome::Kept { .. }), "{:?}", done.files);
        let names: Vec<_> = fs::read_dir(tiny.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!names.iter().any(|name| name.contains("compressed")), "{names:?}");
        assert_eq!(done.reduced(), 0);
        assert_eq!(done.kept(), 1);
    }

    #[test]
    fn replacing_the_original_puts_it_in_the_trash_and_takes_its_name() {
        let temp = Temp::new("shrink-replace");
        let Some((image, ..)) = make(&temp) else { return };
        let data = temp.dir("data");
        let before = fs::read(&image).unwrap();
        let options = ReduceOptions {
            image: ImageOptions { quality: 30, ..ImageOptions::default() },
            replace_original: true,
            ..ReduceOptions::default()
        };
        let (done, restored) = with_data_home(&data, || {
            let done = run(&job(vec![image.clone()], options));
            let Outcome::Reduced { output, original, .. } = &done.files[0].outcome else { panic!("{:?}", done.files) };
            assert_eq!(output, &image, "the new file has the old name");
            assert!(fs::read(&image).unwrap().len() < before.len());
            let item = original.as_ref().expect("the original was binned");
            assert_eq!(fs::read(&item.in_trash).unwrap(), before, "and is whole in the Trash");
            // Undo: the new file goes, the original returns.
            trash(&done.created()).unwrap();
            untrash(&done.trashed()).unwrap();
            let restored = fs::read(&image).unwrap();
            (done, restored)
        });
        assert_eq!(done.reduced(), 1);
        assert_eq!(restored, before);
    }

    #[test]
    fn cancelling_leaves_no_partial_file() {
        let temp = Temp::new("shrink-cancel");
        let Some((_, film, _)) = make(&temp) else { return };
        // A slow, strong encode of a longer clip so the cancel lands mid-way.
        let long = film.with_file_name("long.mp4");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30:duration=60", "-c:v", "libx264", "-crf", "30", "-preset", "ultrafast"])
            .arg(&long)
            .status()
            .unwrap();
        assert!(made.success());
        let options = ReduceOptions {
            video: VideoOptions { quality: VideoQuality::Level(Level::Smallest), sound: Sound::Remove, ..VideoOptions::default() },
            ..ReduceOptions::default()
        };
        let cancel = Cancel::new();
        let stopper = cancel.clone();
        let watcher = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(700));
            stopper.cancel();
        });
        let outcome = reduce(&job(vec![long.clone()], options), &Progress::new(), &Batch::new(), &cancel);
        watcher.join().unwrap();
        assert_eq!(outcome.unwrap_err(), OpError::Cancelled);
        let names: Vec<_> = fs::read_dir(long.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!names.iter().any(|name| name.contains("compressed") || name.starts_with(".bingux")), "{names:?}");
    }

    #[test]
    fn estimates_follow_a_real_sample() {
        let temp = Temp::new("shrink-estimate");
        let Some((image, film, song)) = make(&temp) else { return };
        let files = vec![image.clone(), film.clone(), song.clone()];
        let options = ReduceOptions {
            image: ImageOptions { quality: 30, ..ImageOptions::default() },
            video: VideoOptions { quality: VideoQuality::Level(Level::Fastest), sound: Sound::Kbps(64), ..VideoOptions::default() },
            audio: AudioOptions { codec: AudioCodec::Opus, kbps: 64, strip_metadata: false },
            ..ReduceOptions::default()
        };
        let estimate = estimate(&files, &options, &Cancel::new()).unwrap();
        assert_eq!(estimate.per_file.len(), 3);
        assert!(estimate.estimated < estimate.original);
        // The estimate of the picture is its real size at those settings.
        let done = run(&job(vec![image], ReduceOptions { image: options.image.clone(), ..ReduceOptions::default() }));
        let Outcome::Reduced { to, .. } = &done.files[0].outcome else { panic!() };
        let guess = estimate.per_file[0].unwrap();
        assert!(guess.abs_diff(*to) * 10 <= *to, "estimated {guess}, made {to}");
        // A song at 64 kbit/s for 4 s is 32 000 bytes.
        assert_eq!(estimate.per_file[2], Some(32_000));
    }

    #[test]
    fn odd_names_are_carried_exactly() {
        let temp = Temp::new("shrink-names");
        let Some((image, ..)) = make(&temp) else { return };
        let odd = image.with_file_name("-odd: [0] 'x' *.jpg");
        fs::copy(&image, &odd).unwrap();
        let options = ReduceOptions { image: ImageOptions { quality: 30, ..ImageOptions::default() }, ..ReduceOptions::default() };
        let done = run(&job(vec![odd.clone()], options));
        let Outcome::Reduced { output, .. } = &done.files[0].outcome else { panic!("{:?}", done.files) };
        assert_eq!(output, &odd.with_file_name("-odd: [0] 'x' * (compressed).jpg"));
        assert!(probe_of(output).width.is_some());
    }

    #[test]
    fn things_that_are_not_reducible_are_skipped_with_a_reason() {
        let temp = Temp::new("shrink-skip");
        let text = temp.write("a.txt", "text");
        let gif = temp.write("a.gif", "GIF89a");
        let done = run(&job(vec![text, gif], ReduceOptions::default()));
        assert!(matches!(done.files[0].outcome, Outcome::Skipped { .. }));
        assert!(matches!(done.files[1].outcome, Outcome::Skipped { .. }));
        assert_eq!(done.failures().len(), 2);
        let progress = Progress::new();
        reduce(&job(vec![temp.path().join("a.txt")], ReduceOptions::default()), &progress, &Batch::new(), &Cancel::new()).unwrap();
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn bad_requests_are_refused() {
        let run = |job: &ReduceJob| reduce(job, &Progress::new(), &Batch::new(), &Cancel::new());
        assert!(run(&job(vec![], ReduceOptions::default())).is_err());
        assert!(run(&job(vec![PathBuf::from("rel.jpg")], ReduceOptions::default())).is_err());
        let temp = Temp::new("shrink-bad");
        let mut missing = job(vec![temp.path().join("a.jpg")], ReduceOptions::default());
        missing.dest_dir = Some(temp.path().join("nope"));
        assert!(run(&missing).is_err());
    }

    #[test]
    fn parallelism_and_kinds() {
        assert_eq!(parallelism(&[PathBuf::from("/a/x.mp4")]), 1);
        assert_eq!(parallelism(&[PathBuf::from("/a/x.jpg"), PathBuf::from("/a/y.mp3")]), 2);
        assert_eq!(kind_of_name("a.JPG"), Some(MediaKind::Image));
        assert_eq!(kind_of_name("a.mkv"), Some(MediaKind::Video));
        assert_eq!(kind_of_name("a.flac"), Some(MediaKind::Audio));
        assert_eq!(kind_of_name("a.zip"), None);
        assert!(!all_media(&[]));
    }
}
