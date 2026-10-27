//! `bsm-imgconv` run for real, with real files and real descriptors.

mod common;

use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::time::{Duration, Instant};

use bsm_appearance::convert::{ConvertError, ConvertLimits, ConvertReport, ConvertRequest, Converter, ImageKind};
use bsm_appearance::image_ops::OutputFormat;
use common::*;

fn run(
    converter: &Converter,
    kind: ImageKind,
    source: &File,
    if_unchanged: Option<&str>,
) -> (Result<ConvertReport, ConvertError>, tempfile::TempDir, File, Option<File>) {
    let dir = tempfile::tempdir().unwrap();
    let out = File::create(dir.path().join("out")).unwrap();
    let blur = (kind == ImageKind::Wallpaper).then(|| File::create(dir.path().join("blur")).unwrap());
    let result = converter.convert(&ConvertRequest { kind, source, out: &out, blur_out: blur.as_ref(), if_unchanged });
    (result, dir, out, blur)
}

#[test]
fn wallpaper_conversion_reports_every_sandbox_layer() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_file(dir.path(), "wall.jpg", &jpeg_bytes(1600, 900, 1));
    let (result, out_dir, _out, _blur) = run(&converter(), ImageKind::Wallpaper, &open(&source), None);
    let report = result.expect("conversion succeeds");
    assert!(report.sandbox.rlimits && report.sandbox.no_new_privs && report.sandbox.seccomp, "{:?}", report.sandbox);
    let primary = report.primary.unwrap();
    assert_eq!((primary.width, primary.height, primary.format), (1600, 900, OutputFormat::Jpeg));
    let blurred = report.blurred.unwrap();
    assert!(blurred.width <= 960 && blurred.height <= 600);
    let written = std::fs::read(out_dir.path().join("out")).unwrap();
    assert_eq!(written.len() as u64, primary.bytes);
    let decoded = image::load_from_memory(&written).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1600, 900));
    assert!(image::load_from_memory(&std::fs::read(out_dir.path().join("blur")).unwrap()).is_ok());
}

#[test]
fn avatar_conversion_is_a_square_png() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_file(dir.path(), "face.png", &png_bytes(900, 600, 2));
    let (result, out_dir, _out, _blur) = run(&converter(), ImageKind::Avatar, &open(&source), None);
    let report = result.unwrap();
    let info = report.primary.unwrap();
    assert_eq!((info.width, info.height, info.format), (512, 512, OutputFormat::Png));
    assert!(report.blurred.is_none());
    let written = std::fs::read(out_dir.path().join("out")).unwrap();
    assert_eq!(image::guess_format(&written).unwrap(), image::ImageFormat::Png);
}

#[test]
fn the_shared_file_offset_is_irrelevant() {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let source = write_file(dir.path(), "face.png", &png_bytes(200, 200, 3));
    let mut file = open(&source);
    let mut sink = Vec::new();
    file.read_to_end(&mut sink).unwrap(); // offset now at the end
    assert!(run(&converter(), ImageKind::Avatar, &file, None).0.is_ok());
}

#[test]
fn unchanged_source_is_skipped_by_hash() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_file(dir.path(), "face.png", &png_bytes(120, 120, 4));
    let first = run(&converter(), ImageKind::Avatar, &open(&source), None).0.unwrap();
    assert!(!first.unchanged);
    let (second, out_dir, ..) = run(&converter(), ImageKind::Avatar, &open(&source), Some(&first.source_sha256));
    let second = second.unwrap();
    assert!(second.unchanged && second.primary.is_none());
    assert_eq!(std::fs::metadata(out_dir.path().join("out")).unwrap().len(), 0, "nothing written");
    let (other, ..) = run(&converter(), ImageKind::Avatar, &open(&source), Some(&"0".repeat(64)));
    assert!(!other.unwrap().unchanged);
}

#[test]
fn malformed_images_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut truncated = png_bytes(300, 300, 5);
    truncated.truncate(truncated.len() / 2);
    let mut corrupt = jpeg_bytes(300, 300, 5);
    for byte in corrupt.iter_mut().skip(200).take(400) {
        *byte = 0x55;
    }
    let cases: [(&str, Vec<u8>); 5] = [
        ("text", b"#!/bin/sh\necho hello\n".to_vec()),
        ("truncated png", truncated),
        ("empty", Vec::new()),
        ("gif", b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;".to_vec()),
        ("svg", b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>".to_vec()),
    ];
    for (name, bytes) in cases {
        let path = write_file(dir.path(), name, &bytes);
        let (result, ..) = run(&converter(), ImageKind::Avatar, &open(&path), None);
        assert!(matches!(result, Err(ConvertError::Malformed(_))), "{name}: {result:?}");
    }
    // A corrupted JPEG body may decode with errors or fail; it must never crash the converter
    // or produce an unchecked output.
    let path = write_file(dir.path(), "corrupt.jpg", &corrupt);
    let (result, ..) = run(&converter(), ImageKind::Avatar, &open(&path), None);
    assert!(matches!(result, Ok(_) | Err(ConvertError::Malformed(_))), "{result:?}");
}

#[test]
fn oversized_images_are_rejected_by_their_declared_size() {
    let dir = tempfile::tempdir().unwrap();
    // 9000x9000 = 81 MP: valid and small on disk (flat colour), over the 64 MP limit.
    let flat = RgbFlat::png(9000, 9000);
    let path = write_file(dir.path(), "bomb.png", &flat);
    assert!(std::fs::metadata(&path).unwrap().len() < 2 * 1024 * 1024);
    let started = Instant::now();
    let (result, ..) = run(&converter(), ImageKind::Wallpaper, &open(&path), None);
    assert!(matches!(result, Err(ConvertError::LimitExceeded(_))), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10), "rejected from the header, not after decoding");
    // One side beyond the side limit, few pixels.
    let long = write_file(dir.path(), "long.png", &RgbFlat::png(20_000, 2));
    let (result, ..) = run(&converter(), ImageKind::Wallpaper, &open(&long), None);
    assert!(matches!(result, Err(ConvertError::LimitExceeded(_))), "{result:?}");
}

#[test]
fn oversized_files_are_rejected_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "big.png", &png_bytes(400, 400, 6));
    let mut small = converter();
    small.limits = Some(ConvertLimits { max_input_bytes: 1000, ..ConvertLimits::default() });
    let (result, ..) = run(&small, ImageKind::Avatar, &open(&path), None);
    assert!(matches!(result, Err(ConvertError::Rejected(_))), "{result:?}");
}

#[test]
fn memory_limit_stops_a_decode_that_would_use_too_much() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "wall.png", &png_bytes(3000, 2000, 7));
    let mut tight = converter();
    tight.limits = Some(ConvertLimits { memory_bytes: 64 * 1024 * 1024, ..ConvertLimits::default() });
    let (result, ..) = run(&tight, ImageKind::Wallpaper, &open(&path), None);
    assert!(result.is_err(), "a 64 MiB address space cannot hold a 3000x2000 decode: {result:?}");
}

#[test]
fn sources_that_are_not_regular_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let directory = File::open(dir.path()).unwrap();
    let (result, ..) = run(&converter(), ImageKind::Avatar, &directory, None);
    assert!(matches!(result, Err(ConvertError::Rejected(_))), "directory: {result:?}");
    let device = File::open("/dev/zero").unwrap();
    let (result, ..) = run(&converter(), ImageKind::Avatar, &device, None);
    assert!(matches!(result, Err(ConvertError::Rejected(_))), "device: {result:?}");
    let (read, _write) = std::io::pipe().unwrap();
    let pipe = File::from(std::os::fd::OwnedFd::from(read));
    let (result, ..) = run(&converter(), ImageKind::Avatar, &pipe, None);
    assert!(matches!(result, Err(ConvertError::Rejected(_))), "pipe: {result:?}");
}

#[test]
fn a_used_output_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_file(dir.path(), "face.png", &png_bytes(64, 64, 8));
    let out = write_file(dir.path(), "out", b"existing content");
    let out = std::fs::OpenOptions::new().write(true).open(out).unwrap();
    let result = converter().convert(&ConvertRequest {
        kind: ImageKind::Avatar,
        source: &open(&source),
        out: &out,
        blur_out: None,
        if_unchanged: None,
    });
    assert!(matches!(result, Err(ConvertError::Output(_))), "{result:?}");
}

#[test]
fn the_converter_cannot_be_pointed_at_a_path() {
    // Descriptors only: a path argument is a usage error, so a hostile path never reaches it.
    let output = Command::new(env!("CARGO_BIN_EXE_bsm-imgconv"))
        .args(["convert", "--kind", "avatar", "--input", "/etc/passwd", "--out-fd", "3"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(bsm_appearance::convert::exit::USAGE));
}

#[test]
fn sandbox_stops_what_it_must_stop() {
    // Each probe tries a forbidden syscall after the sandbox is up. The kernel must kill the
    // process with SIGSYS before the probe's next line runs.
    for probe in ["socket", "open", "exec", "write", "exec-memory", "kill-other", "kill-raw", "futex-shared"] {
        let output = Command::new(env!("CARGO_BIN_EXE_bsm-imgconv")).args(["selftest", probe]).output().unwrap();
        assert_eq!(output.status.signal(), Some(libc::SIGSYS), "probe {probe}: {:?} {}", output.status, String::from_utf8_lossy(&output.stdout));
        assert!(output.stdout.is_empty(), "probe {probe} must not get past the call");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_bsm-imgconv")).args(["selftest", "kill-self"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "allowed", "tgkill at itself stays possible");
    let output = Command::new(env!("CARGO_BIN_EXE_bsm-imgconv")).args(["selftest", "futex-private"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "allowed", "private futex ops stay possible");
    let output = Command::new(env!("CARGO_BIN_EXE_bsm-imgconv")).args(["selftest", "memory"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "denied");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_hung_converter_is_killed_at_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let script = write_file(dir.path(), "slow", b"#!/bin/sh\nexec sleep 30\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut slow = Converter::new(&script);
    slow.timeout = Duration::from_millis(300);
    let source = write_file(dir.path(), "face.png", &png_bytes(32, 32, 9));
    let started = Instant::now();
    let (result, ..) = run(&slow, ImageKind::Avatar, &open(&source), None);
    assert!(matches!(result, Err(ConvertError::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_missing_converter_is_a_spawn_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = Converter::new(dir.path().join("no-such-converter"));
    let source = write_file(dir.path(), "face.png", &png_bytes(32, 32, 9));
    let (result, ..) = run(&missing, ImageKind::Avatar, &open(&source), None);
    assert!(matches!(result, Err(ConvertError::Spawn(_))));
}

/// A flat-colour PNG of any size, written row by row so the test itself stays small.
struct RgbFlat;

impl RgbFlat {
    fn png(width: u32, height: u32) -> Vec<u8> {
        use image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use image::{ExtendedColorType, ImageEncoder};
        let mut bytes = Vec::new();
        let row = vec![30u8; width as usize * 3 * height as usize];
        PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::NoFilter)
            .write_image(&row, width, height, ExtendedColorType::Rgb8)
            .unwrap();
        bytes
    }
}
