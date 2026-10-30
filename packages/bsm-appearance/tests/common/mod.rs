//! Shared helpers: real image files and the built converter.

#![allow(dead_code)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bsm_appearance::convert::Converter;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};

pub fn converter() -> Converter {
    let mut converter = Converter::new(env!("CARGO_BIN_EXE_bsm-imgconv"));
    converter.timeout = Duration::from_secs(120);
    converter
}

/// A gradient with a diagonal stripe; `seed` changes the colours so two seeds never match.
pub fn picture(width: u32, height: u32, seed: u8) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        let stripe = ((x + y) / 40) % 2 == 0;
        Rgb([
            ((x * 255) / width.max(1)) as u8 ^ seed,
            ((y * 255) / height.max(1)) as u8,
            if stripe { 200u8.wrapping_add(seed) } else { 40 },
        ])
    })
}

pub fn png_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let picture = picture(width, height, seed);
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes).write_image(picture.as_raw(), width, height, ExtendedColorType::Rgb8).unwrap();
    bytes
}

pub fn jpeg_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let picture = picture(width, height, seed);
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 90)
        .write_image(picture.as_raw(), width, height, ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

/// Writes `bytes` to `dir/name` and returns the path.
pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    File::create(&path).unwrap().write_all(bytes).unwrap();
    path
}

pub fn open(path: &Path) -> File {
    File::open(path).unwrap()
}
