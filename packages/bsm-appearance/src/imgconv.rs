//! The `bsm-imgconv` program.
//!
//! ```text
//! bsm-imgconv convert --kind avatar|wallpaper --source-fd N --out-fd N
//!                     [--blur-fd N] [--if-unchanged SHA256_HEX]
//!                     [--max-input-bytes N] [--max-pixels N] [--max-side N]
//!                     [--memory-limit BYTES] [--cpu-seconds N] [--max-output-bytes N]
//!                     [--require-seccomp]
//! bsm-imgconv selftest socket|open|exec|write|exec-memory|memory
//! ```
//!
//! It never opens a path. The source image, the output file and (for wallpapers)
//! the blurred output are inherited descriptors. It prints one JSON line
//! (`convert::ChildOutput`) on stdout; exit codes are in `convert::exit`. The step
//! order is the security design:
//!
//! 1. parse and validate arguments; refuse root; apply rlimits;
//! 2. check the descriptors (regular files), check the source size, read the source
//!    into memory with `pread` and hash it;
//! 3. close every other descriptor, then lock down (netns, no_new_privs, seccomp);
//! 4. only then decode, resize, blur, encode and write.

use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::FileExt;

use crate::convert::{
    ChildFailure, ChildOutput, ConvertLimits, ConvertReport, ImageKind, OutputInfo, SandboxReport, exit, sha256_hex,
};
use crate::image_ops::{self, DecodeLimits, Encoded, ImageError};
use crate::{sandbox, sys};

struct Failure {
    code: i32,
    name: &'static str,
    detail: String,
}

impl Failure {
    fn new(code: i32, name: &'static str, detail: impl Into<String>) -> Self {
        Self { code, name, detail: detail.into() }
    }
}

struct Arguments {
    kind: ImageKind,
    source_fd: RawFd,
    out_fd: RawFd,
    blur_fd: Option<RawFd>,
    if_unchanged: Option<String>,
    limits: ConvertLimits,
    require_seccomp: bool,
}

fn usage(detail: impl Into<String>) -> Failure {
    Failure::new(exit::USAGE, "usage", detail)
}

fn parse_number<T: std::str::FromStr>(flag: &str, value: Option<OsString>) -> Result<T, Failure> {
    value
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| usage(format!("{flag} needs a number")))
}

fn parse_arguments(mut args: impl Iterator<Item = OsString>) -> Result<Arguments, Failure> {
    let mut kind = None;
    let mut source_fd = None;
    let mut out_fd = None;
    let mut blur_fd = None;
    let mut if_unchanged = None;
    let mut limits = ConvertLimits::default();
    let mut require_seccomp = false;
    while let Some(flag) = args.next() {
        let name = flag.to_string_lossy().into_owned();
        match name.as_str() {
            "--kind" => {
                kind = args.next().and_then(|value| ImageKind::parse(&value.to_string_lossy()));
                if kind.is_none() {
                    return Err(usage("--kind is avatar or wallpaper"));
                }
            }
            "--source-fd" => source_fd = Some(parse_number::<RawFd>(&name, args.next())?),
            "--out-fd" => out_fd = Some(parse_number::<RawFd>(&name, args.next())?),
            "--blur-fd" => blur_fd = Some(parse_number::<RawFd>(&name, args.next())?),
            "--if-unchanged" => if_unchanged = args.next().and_then(|value| value.into_string().ok()),
            "--max-input-bytes" => limits.max_input_bytes = parse_number(&name, args.next())?,
            "--max-pixels" => limits.max_pixels = parse_number(&name, args.next())?,
            "--max-side" => limits.max_side = parse_number(&name, args.next())?,
            "--memory-limit" => limits.memory_bytes = parse_number(&name, args.next())?,
            "--cpu-seconds" => limits.cpu_seconds = parse_number(&name, args.next())?,
            "--max-output-bytes" => limits.max_output_bytes = parse_number(&name, args.next())?,
            "--require-seccomp" => require_seccomp = true,
            other => return Err(usage(format!("unknown argument {other}"))),
        }
    }
    let kind = kind.ok_or_else(|| usage("--kind is required"))?;
    let source_fd = source_fd.ok_or_else(|| usage("--source-fd is required"))?;
    let out_fd = out_fd.ok_or_else(|| usage("--out-fd is required"))?;
    if (kind == ImageKind::Wallpaper) != blur_fd.is_some() {
        return Err(usage("--blur-fd is required for a wallpaper and not allowed for an avatar"));
    }
    let mut descriptors = vec![source_fd, out_fd];
    descriptors.extend(blur_fd);
    descriptors.sort_unstable();
    descriptors.dedup();
    if descriptors.len() != 2 + usize::from(blur_fd.is_some()) || descriptors[0] < 3 {
        return Err(usage("descriptors must be distinct and above stderr"));
    }
    if limits.max_input_bytes == 0 || limits.max_pixels == 0 || limits.memory_bytes == 0 || limits.cpu_seconds == 0 {
        return Err(usage("limits must not be zero"));
    }
    Ok(Arguments { kind, source_fd, out_fd, blur_fd, if_unchanged, limits, require_seccomp })
}

fn take_descriptor(fd: RawFd, name: &str) -> Result<File, Failure> {
    // SAFETY: the parent passed this descriptor to us and nothing else in this process uses it.
    let file = unsafe { File::from_raw_fd(fd) };
    let stat = sys::fstat(fd).map_err(|error| Failure::new(exit::OUTPUT, "descriptor", format!("{name}: {error}")))?;
    if !sys::is_regular(stat.mode) {
        // A source that is a directory, pipe, socket or device is a refusal, not an outage.
        let code = if name == "--source-fd" { exit::REJECTED } else { exit::OUTPUT };
        return Err(Failure::new(code, "not_regular", format!("{name} is not a regular file")));
    }
    Ok(file)
}

fn check_output(file: &File, name: &str) -> Result<(), Failure> {
    let stat = sys::fstat(file.as_raw_fd()).map_err(|error| Failure::new(exit::OUTPUT, "output", format!("{name}: {error}")))?;
    if stat.size != 0 {
        return Err(Failure::new(exit::OUTPUT, "output", format!("{name} must be a fresh, empty file")));
    }
    Ok(())
}

fn image_failure(error: ImageError) -> Failure {
    match &error {
        ImageError::TooLarge { .. } | ImageError::OverLimit(_) => Failure::new(exit::LIMITS, "limits", error.to_string()),
        ImageError::Encode(_) => Failure::new(exit::OUTPUT, "encode", error.to_string()),
        _ => Failure::new(exit::DECODE, "decode", error.to_string()),
    }
}

fn write_output(file: &mut File, encoded: &Encoded) -> Result<OutputInfo, Failure> {
    file.write_all(&encoded.bytes)
        .map_err(|error| Failure::new(exit::OUTPUT, "output", format!("writing the result: {error}")))?;
    Ok(OutputInfo { width: encoded.width, height: encoded.height, bytes: encoded.bytes.len() as u64, format: encoded.format })
}

/// Reads the whole source with `pread` from offset 0, at most `limit` bytes. The shared file
/// offset is neither used nor changed. `None` when the file is longer than `limit`.
fn read_source(file: &File, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 256 * 1024];
    loop {
        let read = file.read_at(&mut chunk, bytes.len() as u64)?;
        if read == 0 {
            return Ok(Some(bytes));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() as u64 > limit {
            return Ok(None);
        }
    }
}

fn convert(arguments: Arguments) -> Result<(ConvertReport, i32), Failure> {
    let fail_sandbox = |detail: String| Failure::new(exit::SANDBOX, "sandbox", detail);
    let mut report = SandboxReport::default();
    sandbox::refuse_root().map_err(fail_sandbox)?;
    sandbox::apply_rlimits(&arguments.limits).map_err(fail_sandbox)?;
    report.rlimits = true;

    let source = take_descriptor(arguments.source_fd, "--source-fd")?;
    let mut out = take_descriptor(arguments.out_fd, "--out-fd")?;
    check_output(&out, "--out-fd")?;
    let mut blur_out = match arguments.blur_fd {
        Some(fd) => {
            let file = take_descriptor(fd, "--blur-fd")?;
            check_output(&file, "--blur-fd")?;
            Some(file)
        }
        None => None,
    };

    let size = source.metadata().map_err(|error| Failure::new(exit::UNREADABLE, "unreadable", error.to_string()))?.len();
    if size > arguments.limits.max_input_bytes {
        return Err(Failure::new(
            exit::REJECTED,
            "too_big",
            format!("source is {size} bytes, the limit is {}", arguments.limits.max_input_bytes),
        ));
    }
    // The size can change between fstat and read (a memfd the caller still holds writable):
    // the read itself is capped, and the hash is of exactly the bytes decoded below.
    let bytes = read_source(&source, arguments.limits.max_input_bytes)
        .map_err(|error| Failure::new(exit::UNREADABLE, "unreadable", format!("reading the source: {error}")))?
        .ok_or_else(|| Failure::new(exit::REJECTED, "too_big", "source grew past the limit while it was read"))?;
    drop(source);
    let source_sha256 = sha256_hex(&bytes);
    if arguments.if_unchanged.as_deref() == Some(source_sha256.as_str()) {
        return Ok((
            ConvertReport {
                kind: arguments.kind,
                source_sha256,
                unchanged: true,
                primary: None,
                blurred: None,
                sandbox: report,
            },
            exit::UNCHANGED,
        ));
    }

    let mut keep = vec![arguments.out_fd];
    keep.extend(arguments.blur_fd);
    sandbox::close_other_descriptors(&keep);
    let report = sandbox::lock_down(&keep, arguments.require_seccomp, report).map_err(fail_sandbox)?;

    // Everything below runs inside the sandbox.
    let limits = DecodeLimits {
        max_pixels: arguments.limits.max_pixels,
        max_side: arguments.limits.max_side,
        max_alloc: (arguments.limits.memory_bytes / 2).max(64 * 1024 * 1024),
    };
    let decoded = image_ops::decode(&bytes, limits).map_err(image_failure)?;
    drop(bytes);
    let (primary, blurred) = match arguments.kind {
        ImageKind::Avatar => {
            let avatar = image_ops::make_avatar(&decoded).map_err(image_failure)?;
            (write_output(&mut out, &avatar)?, None)
        }
        ImageKind::Wallpaper => {
            let (wallpaper, rgb) = image_ops::make_wallpaper(&decoded).map_err(image_failure)?;
            drop(decoded);
            let blurred = image_ops::make_blurred(&rgb).map_err(image_failure)?;
            let blur_file = blur_out.as_mut().expect("parse_arguments ties blur-fd to wallpapers");
            (write_output(&mut out, &wallpaper)?, Some(write_output(blur_file, &blurred)?))
        }
    };
    Ok((
        ConvertReport {
            kind: arguments.kind,
            source_sha256,
            unchanged: false,
            primary: Some(primary),
            blurred,
            sandbox: report,
        },
        exit::OK,
    ))
}

fn emit(output: &ChildOutput) {
    let mut line = serde_json::to_string(output).unwrap_or_else(|_| "{}".into());
    line.push('\n');
    let _ = std::io::stdout().write_all(line.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Sandbox self test: lock down, then attempt something the filter must stop. A probe
/// that is killed (SIGSYS) proves the filter; one that returns is reported and fails the test.
fn selftest(probe: &str) -> i32 {
    let limits = ConvertLimits { memory_bytes: 256 * 1024 * 1024, ..ConvertLimits::default() };
    if let Err(error) = sandbox::refuse_root().and_then(|()| sandbox::apply_rlimits(&limits)) {
        eprintln!("selftest: {error}");
        return exit::SANDBOX;
    }
    if let Err(error) = sandbox::lock_down(&[], true, SandboxReport::default()) {
        eprintln!("selftest: {error}");
        return exit::SANDBOX;
    }
    // SAFETY: every call below is deliberately forbidden by the filter; arguments are valid.
    unsafe {
        match probe {
            "socket" => {
                libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            }
            "open" => {
                libc::open(c"/etc/passwd".as_ptr(), libc::O_RDONLY);
            }
            "exec" => {
                libc::execve(c"/bin/true".as_ptr(), std::ptr::null(), std::ptr::null());
            }
            "write" => {
                // Descriptor 7 is not an output of this run.
                libc::write(7, b"x".as_ptr().cast(), 1);
            }
            "exec-memory" => {
                libc::mmap(
                    std::ptr::null_mut(),
                    4096,
                    libc::PROT_READ | libc::PROT_EXEC,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                );
            }
            "memory" => {
                // Over RLIMIT_AS: the allocation must fail cleanly, not be granted.
                let mut big: Vec<u8> = Vec::new();
                return if big.try_reserve(1024 * 1024 * 1024).is_err() {
                    println!("denied");
                    exit::OK
                } else {
                    println!("granted");
                    99
                };
            }
            other => {
                eprintln!("selftest: unknown probe {other}");
                return exit::USAGE;
            }
        }
    }
    println!("escaped");
    99
}

/// Entry point of the binary. Returns the process exit code.
pub fn main() -> i32 {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|command| command.to_str()) {
        Some("convert") => match parse_arguments(args).and_then(convert) {
            Ok((report, code)) => {
                emit(&ChildOutput::Ok(report));
                code
            }
            Err(failure) => {
                emit(&ChildOutput::Failed(ChildFailure { error: failure.name.into(), detail: failure.detail }));
                failure.code
            }
        },
        Some("selftest") => match args.next().and_then(|probe| probe.into_string().ok()) {
            Some(probe) => selftest(&probe),
            None => exit::USAGE,
        },
        _ => {
            eprintln!("usage: bsm-imgconv convert --kind avatar|wallpaper --source-fd N --out-fd N [...]");
            exit::USAGE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Arguments, Failure> {
        parse_arguments(args.iter().map(OsString::from))
    }

    #[test]
    fn wallpaper_needs_a_blur_descriptor_and_avatar_forbids_it() {
        let wallpaper = ["--kind", "wallpaper", "--source-fd", "3", "--out-fd", "4"];
        assert_eq!(parse(&wallpaper).err().map(|failure| failure.code), Some(exit::USAGE));
        let avatar = ["--kind", "avatar", "--source-fd", "3", "--out-fd", "4", "--blur-fd", "5"];
        assert_eq!(parse(&avatar).err().map(|failure| failure.code), Some(exit::USAGE));
        let fine = ["--kind", "wallpaper", "--source-fd", "3", "--out-fd", "4", "--blur-fd", "5"];
        assert!(parse(&fine).is_ok());
    }

    #[test]
    fn paths_and_identity_switches_are_not_accepted() {
        // The converter takes descriptors only; a path or a uid switch is a usage error.
        for flag in ["--input", "--root", "--uid", "--gid"] {
            let args = ["--kind", "avatar", "--source-fd", "3", "--out-fd", "4", flag, "1"];
            assert_eq!(parse(&args).err().map(|failure| failure.code), Some(exit::USAGE), "{flag}");
        }
    }

    #[test]
    fn descriptors_must_be_distinct_and_not_stdio() {
        let same = ["--kind", "avatar", "--source-fd", "4", "--out-fd", "4"];
        assert_eq!(parse(&same).err().map(|failure| failure.code), Some(exit::USAGE));
        let stdio = ["--kind", "avatar", "--source-fd", "1", "--out-fd", "4"];
        assert_eq!(parse(&stdio).err().map(|failure| failure.code), Some(exit::USAGE));
    }
}
