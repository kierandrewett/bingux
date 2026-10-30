//! The converter contract: command line, report and exit codes of
//! `bsm-imgconv`, and [`Converter`], the parent side that spawns it.
//!
//! The child does the dangerous work (reading an untrusted path, decoding an
//! untrusted image) inside a sandbox, see [`crate::sandbox`]. The parent never
//! decodes anything. The parent creates the output files and passes them as
//! open descriptors, so the child needs no write access to any directory and
//! can run as the unprivileged owner of the source image.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::image_ops::OutputFormat;
use crate::sys;

/// Where the packages install the converter.
pub const DEFAULT_PROGRAM: &str = "/usr/libexec/bsm/bsm-imgconv";

/// Exit codes of `bsm-imgconv`. Anything else, or death by signal, is a crash.
pub mod exit {
    pub const OK: i32 = 0;
    /// The source hash equals `--if-unchanged`; nothing was decoded or written.
    pub const UNCHANGED: i32 = 10;
    pub const USAGE: i32 = 20;
    /// The source could not be read (I/O error on the descriptor).
    pub const UNREADABLE: i32 = 21;
    /// The source is not acceptable: not a regular file, or too big.
    pub const REJECTED: i32 = 22;
    /// The file is not a valid JPEG, PNG or WebP image.
    pub const DECODE: i32 = 23;
    /// The image exceeds the pixel, dimension or memory limits.
    pub const LIMITS: i32 = 24;
    /// A required sandbox layer could not be applied. Nothing was read.
    pub const SANDBOX: i32 = 25;
    /// The output descriptors are unusable.
    pub const OUTPUT: i32 = 26;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    Avatar,
    Wallpaper,
}

impl ImageKind {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Wallpaper => "wallpaper",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "avatar" => Some(Self::Avatar),
            "wallpaper" => Some(Self::Wallpaper),
            _ => None,
        }
    }
}

impl fmt::Display for ImageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_arg())
    }
}

/// Resource limits for one conversion. The child applies them to itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvertLimits {
    /// Largest source file in bytes.
    pub max_input_bytes: u64,
    /// Largest source image in pixels (declared in the header, checked before decoding).
    pub max_pixels: u64,
    /// Longest side of the source image.
    pub max_side: u32,
    /// `RLIMIT_AS` of the child in bytes.
    pub memory_bytes: u64,
    /// `RLIMIT_CPU` of the child in seconds.
    pub cpu_seconds: u64,
    /// `RLIMIT_FSIZE` of the child: the most it can write to an output.
    pub max_output_bytes: u64,
}

impl Default for ConvertLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 48 * 1024 * 1024,
            max_pixels: 64 * 1024 * 1024,
            max_side: 16_384,
            memory_bytes: 1536 * 1024 * 1024,
            cpu_seconds: 20,
            max_output_bytes: 32 * 1024 * 1024,
        }
    }
}

impl ConvertLimits {
    /// Avatars are small; a smaller cap keeps a wallpaper-sized file from being used as one.
    pub fn for_kind(kind: ImageKind) -> Self {
        let defaults = Self::default();
        match kind {
            ImageKind::Wallpaper => defaults,
            ImageKind::Avatar => Self { max_input_bytes: 16 * 1024 * 1024, max_pixels: 32 * 1024 * 1024, ..defaults },
        }
    }
}

/// Which sandbox layers the child really applied. A layer is `true` only after
/// the kernel accepted it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxReport {
    pub rlimits: bool,
    pub no_new_privs: bool,
    /// A network namespace without interfaces. Best effort: it needs privilege.
    pub network_namespace: bool,
    /// The seccomp-bpf allow list is installed.
    pub seccomp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub format: OutputFormat,
}

/// What the child prints on success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvertReport {
    pub kind: ImageKind,
    /// SHA-256 (lower-case hex) of the source bytes the child read. The caller passes it back
    /// as `if_unchanged` to skip an unchanged source.
    pub source_sha256: String,
    /// True when the hash matched `if_unchanged` and nothing was produced.
    pub unchanged: bool,
    pub primary: Option<OutputInfo>,
    /// The blurred wallpaper twin, only for wallpapers.
    pub blurred: Option<OutputInfo>,
    pub sandbox: SandboxReport,
}

/// What the child prints on failure (and the exit code says which class).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildFailure {
    pub error: String,
    pub detail: String,
}

/// One line of JSON on the child's stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChildOutput {
    Ok(ConvertReport),
    Failed(ChildFailure),
}

#[derive(Debug)]
pub enum ConvertError {
    /// The converter could not be started.
    Spawn(io::Error),
    /// The source could not be read (I/O error).
    Unreadable(String),
    /// Source refused by policy: not a regular file, or too big.
    Rejected(String),
    /// Not a valid image.
    Malformed(String),
    /// Over the pixel, dimension or memory limits.
    LimitExceeded(String),
    /// The sandbox could not be established (or `require_seccomp` was not met).
    Sandbox(String),
    /// The output descriptors were unusable.
    Output(String),
    /// Wall-clock timeout; the child was killed.
    Timeout,
    /// The child died from a signal: sandbox violation (SIGSYS), memory or CPU limit, crash.
    Killed(i32),
    /// The child exited in a way this crate does not recognise.
    Protocol(String),
}

impl ConvertError {
    /// True when the source was merely unavailable, as opposed to bad or hostile.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unreadable(_))
    }
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "could not start the image converter: {error}"),
            Self::Unreadable(why) => write!(f, "source image is not readable: {why}"),
            Self::Rejected(why) => write!(f, "source image refused: {why}"),
            Self::Malformed(why) => write!(f, "source image is malformed: {why}"),
            Self::LimitExceeded(why) => write!(f, "source image exceeds the limits: {why}"),
            Self::Sandbox(why) => write!(f, "converter sandbox failed: {why}"),
            Self::Output(why) => write!(f, "converter output failed: {why}"),
            Self::Timeout => f.write_str("the image converter timed out"),
            Self::Killed(signal) => write!(f, "the image converter was killed by signal {signal}"),
            Self::Protocol(why) => write!(f, "unexpected converter result: {why}"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// One conversion: a source file descriptor and the files to write.
///
/// The converter never opens a path. The caller opens the source (the user's own
/// publisher process, or bsm itself for trusted system locations) and hands the
/// descriptor over, so a hostile path or symlink never reaches the converter.
pub struct ConvertRequest<'a> {
    pub kind: ImageKind,
    /// The source image. Must be a regular file (a `memfd` or an `O_RDONLY` file); the
    /// converter reads it with `pread` from offset 0 and ignores the shared file offset.
    pub source: &'a File,
    /// Receives the sanitised image. Must be a fresh, empty, regular file opened for writing.
    pub out: &'a File,
    /// Receives the blurred wallpaper twin. Required for wallpapers, forbidden for avatars.
    pub blur_out: Option<&'a File>,
    /// Skip the work when the source bytes hash to this SHA-256 (lower-case hex).
    pub if_unchanged: Option<&'a str>,
}

/// Spawns `bsm-imgconv`.
#[derive(Debug, Clone)]
pub struct Converter {
    pub program: PathBuf,
    /// Wall-clock budget, enforced by the parent with SIGKILL.
    pub timeout: Duration,
    /// Fail rather than convert when seccomp could not be installed.
    pub require_seccomp: bool,
    /// Overrides the per-kind limits when set.
    pub limits: Option<ConvertLimits>,
}

impl Converter {
    pub fn system() -> Self {
        Self::new(DEFAULT_PROGRAM)
    }

    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self { program: program.into(), timeout: Duration::from_secs(30), require_seccomp: true, limits: None }
    }

    /// Runs one conversion and returns the child's report. On success the output
    /// files hold the images; on failure they may hold garbage and must be discarded.
    pub fn convert(&self, request: &ConvertRequest<'_>) -> Result<ConvertReport, ConvertError> {
        match (request.kind, request.blur_out) {
            (ImageKind::Wallpaper, None) => return Err(ConvertError::Output("a wallpaper needs a blur output".into())),
            (ImageKind::Avatar, Some(_)) => return Err(ConvertError::Output("an avatar has no blur output".into())),
            _ => {}
        }
        let limits = self.limits.unwrap_or_else(|| ConvertLimits::for_kind(request.kind));
        let mut command = Command::new(&self.program);
        command
            .arg("convert")
            .args(["--kind", request.kind.as_arg()])
            .args(["--source-fd", &request.source.as_raw_fd().to_string()])
            .args(["--out-fd", &request.out.as_raw_fd().to_string()])
            .args(["--max-input-bytes", &limits.max_input_bytes.to_string()])
            .args(["--max-pixels", &limits.max_pixels.to_string()])
            .args(["--max-side", &limits.max_side.to_string()])
            .args(["--memory-limit", &limits.memory_bytes.to_string()])
            .args(["--cpu-seconds", &limits.cpu_seconds.to_string()])
            .args(["--max-output-bytes", &limits.max_output_bytes.to_string()]);
        let mut inherited = vec![request.source.as_raw_fd(), request.out.as_raw_fd()];
        if let Some(blur) = request.blur_out {
            command.args(["--blur-fd", &blur.as_raw_fd().to_string()]);
            inherited.push(blur.as_raw_fd());
        }
        if let Some(sha256) = request.if_unchanged {
            command.args(["--if-unchanged", sha256]);
        }
        if self.require_seccomp {
            command.arg("--require-seccomp");
        }
        command.env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // SAFETY: the closure runs between fork and exec and only calls async-signal-safe
        // functions (fcntl, prctl) on integers; it allocates nothing.
        unsafe {
            command.pre_exec(move || {
                sys::die_with_parent()?;
                for fd in &inherited {
                    sys::clear_cloexec(*fd)?;
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(ConvertError::Spawn)?;
        let stdout = spawn_reader(child.stdout.take());
        let stderr = spawn_reader(child.stderr.take());
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ConvertError::Timeout);
                }
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => return Err(ConvertError::Spawn(error)),
            }
        };
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if let Some(signal) = status.signal() {
            return Err(ConvertError::Killed(signal));
        }
        let code = status.code().unwrap_or(-1);
        let output: Result<ChildOutput, _> = serde_json::from_slice(&stdout);
        let detail = |output: Result<ChildOutput, _>| match output {
            Ok(ChildOutput::Failed(failure)) => failure.detail,
            _ => String::from_utf8_lossy(&stderr).trim().to_string(),
        };
        match code {
            exit::OK | exit::UNCHANGED => match output {
                Ok(ChildOutput::Ok(report)) => Ok(report),
                _ => Err(ConvertError::Protocol(format!("exit {code} without a report"))),
            },
            exit::UNREADABLE => Err(ConvertError::Unreadable(detail(output))),
            exit::REJECTED => Err(ConvertError::Rejected(detail(output))),
            exit::DECODE => Err(ConvertError::Malformed(detail(output))),
            exit::LIMITS => Err(ConvertError::LimitExceeded(detail(output))),
            exit::SANDBOX => Err(ConvertError::Sandbox(detail(output))),
            exit::OUTPUT => Err(ConvertError::Output(detail(output))),
            other => Err(ConvertError::Protocol(format!("exit code {other}: {}", detail(output)))),
        }
    }
}

/// Reads a pipe to its end, keeping at most 64 KiB: a runaway child cannot grow our memory.
fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            let mut limited = pipe.take(64 * 1024);
            let _ = limited.read_to_end(&mut bytes);
            // Drain the rest so the child never blocks on a full pipe.
            let _ = io::copy(&mut limited.into_inner(), &mut io::sink());
        }
        bytes
    })
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
