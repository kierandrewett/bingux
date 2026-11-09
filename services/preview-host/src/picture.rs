//! Decoding the pictures a viewer's own codecs do not read, and showing them.
//!
//! The viewer in the UI process draws PNG, JPEG, WebP, GIF, BMP, ICO and SVG
//! itself. Everything else goes here, so a codec bug in a decoder takes this
//! process and not the window: HEIF and AVIF (libheif), TIFF, Radiance HDR and
//! OpenEXR (the `image` crate), and the preview a camera RAW file carries
//! ([`crate::raw`]). A picture is decoded whole, turned upright by the file's own
//! orientation, brought to 8-bit sRGB (high dynamic range pictures are tone
//! mapped, [`crate::tonemap`]), laid over a quiet chequerboard where it is
//! transparent, and kept. The viewer then asks for it at the size it needs.

use std::fs::File;
use std::io::{Cursor, Read};

use bingux_preview_core::host::protocol::ErrorCode;
use bingux_preview_core::host::shm::{MAX_BYTES, Shared};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};

use crate::limits::{MAX_DOCUMENT_BYTES, TOO_LARGE};
use crate::raw;
use crate::tonemap::{self, Primaries, Transfer};

/// The most pixels a picture may have: 400 megapixels.
const MAX_PIXELS: u64 = 400_000_000;
/// The longest side a decoder is allowed.
const MAX_SIDE: u32 = 65_536;
/// A RAW preview shorter than this on its long side is a thumbnail, not the picture.
const RAW_SMALLEST: u32 = 600;

/// The two greys of the chequerboard behind transparent pixels, and its square.
const CHECK_LIGHT: u8 = 0xEE;
const CHECK_DARK: u8 = 0xD8;
const CHECK_SIZE: u32 = 8;

type Failure = (ErrorCode, String);

/// A picture, ready to be drawn at any size.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, opaque RGBA.
    pub rgba: Vec<u8>,
}

fn corrupt() -> Failure {
    (
        ErrorCode::Corrupt,
        "This picture is damaged and cannot be shown.".to_owned(),
    )
}

fn unsupported(what: &str) -> Failure {
    (ErrorCode::Unsupported, what.to_owned())
}

/// Read `file`, at most the size cap, into memory.
fn read_all(mut file: File) -> Result<Vec<u8>, Failure> {
    let size = file.metadata().map_or(0, |metadata| metadata.len());
    if size > MAX_DOCUMENT_BYTES {
        return Err((ErrorCode::Unsupported, TOO_LARGE.to_owned()));
    }
    let mut data = Vec::with_capacity(size as usize);
    file.read_to_end(&mut data).map_err(|_| {
        (
            ErrorCode::NotFound,
            "The file could not be read.".to_owned(),
        )
    })?;
    Ok(data)
}

/// Decode the picture in `file`. `raw` says it is a camera file.
pub fn decode(file: File, raw: bool) -> Result<Picture, Failure> {
    let data = read_all(file)?;
    let flat = if raw {
        decode_raw(&data)?
    } else if is_heif(&data) {
        decode_heif(&data)?
    } else {
        decode_with_image(&data, None)?
    };
    finish(flat)
}

fn is_heif(data: &[u8]) -> bool {
    data.get(4..8) == Some(b"ftyp")
        && matches!(
            data.get(8..12),
            Some(
                b"heic"
                    | b"heix"
                    | b"hevc"
                    | b"hevx"
                    | b"mif1"
                    | b"msf1"
                    | b"avif"
                    | b"avis"
                    | b"heim"
                    | b"heis"
            )
        )
}

/// What a decoder produced, before it is finished: 8-bit sRGB with or without
/// alpha.
struct Flat {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(2 << 30);
    limits
}

/// The `image` crate: PNG, JPEG, WebP, GIF, BMP, ICO, TIFF, Radiance HDR, EXR.
/// `orientation` overrides what the file says (a RAW's own).
fn decode_with_image(data: &[u8], orientation: Option<Orientation>) -> Result<Flat, Failure> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| corrupt())?;
    reader.limits(limits());
    if reader.format().is_none() {
        return Err(unsupported("This picture format is not supported."));
    }
    let mut decoder = reader.into_decoder().map_err(|error| match error {
        image::ImageError::Unsupported(_) => unsupported("This picture format is not supported."),
        image::ImageError::Limits(_) => (ErrorCode::Unsupported, TOO_LARGE.to_owned()),
        _ => corrupt(),
    })?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err((ErrorCode::Unsupported, TOO_LARGE.to_owned()));
    }
    let orientation = orientation
        .or_else(|| decoder.orientation().ok())
        .unwrap_or(Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|_| corrupt())?;
    picture.apply_orientation(orientation);
    Ok(flatten(picture))
}

/// A decoded picture as 8-bit sRGB, tone mapping what is floating point.
fn flatten(picture: DynamicImage) -> Flat {
    match picture {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            let rgba = picture.into_rgba32f();
            let (width, height) = rgba.dimensions();
            let pixels: Vec<[f32; 3]> = rgba
                .pixels()
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            let exposure = tonemap::exposure_of(&pixels, None);
            let mut out = Vec::with_capacity(pixels.len() * 4);
            for (pixel, source) in pixels.iter().zip(rgba.pixels()) {
                let mapped = tonemap::map_pixel(*pixel, exposure);
                out.extend_from_slice(&[
                    to_byte(mapped[0]),
                    to_byte(mapped[1]),
                    to_byte(mapped[2]),
                    to_byte(source[3]),
                ]);
            }
            Flat {
                width,
                height,
                rgba: out,
            }
        }
        other => {
            let rgba = other.into_rgba8();
            let (width, height) = rgba.dimensions();
            Flat {
                width,
                height,
                rgba: rgba.into_raw(),
            }
        }
    }
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The largest JPEG a camera file carries, upright by the camera's own
/// orientation tag (which is in the RAW container, not in the JPEG).
fn decode_raw(data: &[u8]) -> Result<Flat, Failure> {
    let Some(preview) =
        raw::largest_preview(data, RAW_SMALLEST).or_else(|| raw::largest_preview(data, 1))
    else {
        return Err(unsupported("This camera file has no preview to show."));
    };
    let orientation = exif::Reader::new()
        .read_from_container(&mut Cursor::new(data))
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })
        .and_then(|value| u8::try_from(value).ok())
        .and_then(Orientation::from_exif);
    let slice = &data[preview.start..preview.end];
    let mut reader = ImageReader::with_format(Cursor::new(slice), ImageFormat::Jpeg);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(|_| corrupt())?;
    let orientation = orientation
        .or_else(|| decoder.orientation().ok())
        .unwrap_or(Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|_| corrupt())?;
    picture.apply_orientation(orientation);
    Ok(flatten(picture))
}

/// HEIF and AVIF through libheif, which turns the picture upright itself.
/// A picture deeper than 8 bits is high dynamic range as often as not: it is
/// linearised by the transfer function its colour box names and tone mapped.
fn decode_heif(data: &[u8]) -> Result<Flat, Failure> {
    use libheif_rs::{
        ColorPrimaries, ColorSpace, HeifContext, LibHeif, RgbChroma, TransferCharacteristics,
    };

    let library = LibHeif::new();
    let context = HeifContext::read_from_bytes(data).map_err(|_| corrupt())?;
    let handle = context.primary_image_handle().map_err(|_| corrupt())?;
    let (width, height) = (handle.width(), handle.height());
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err((ErrorCode::Unsupported, TOO_LARGE.to_owned()));
    }
    let depth = handle.luma_bits_per_pixel();
    let (transfer, primaries) = match handle.color_profile_nclx() {
        Some(profile) => (
            match profile.transfer_characteristics() {
                TransferCharacteristics::ITU_R_BT_2100_0_PQ => Transfer::Pq,
                TransferCharacteristics::ITU_R_BT_2100_0_HLG => Transfer::Hlg,
                TransferCharacteristics::Linear => Transfer::Linear,
                _ => Transfer::Srgb,
            },
            match profile.color_primaries() {
                ColorPrimaries::ITU_R_BT_2020_2_and_2100_0 => Primaries::Bt2020,
                ColorPrimaries::SMPTE_EG_432_1 | ColorPrimaries::SMPTE_RP_431_2 => {
                    Primaries::DisplayP3
                }
                _ => Primaries::Bt709,
            },
        ),
        None => (Transfer::Srgb, Primaries::Bt709),
    };
    if depth <= 8 && primaries == Primaries::Bt709 {
        let image = library
            .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
            .map_err(|_| corrupt())?;
        let plane = image.planes().interleaved.ok_or_else(corrupt)?;
        let row = width as usize * 4;
        let mut rgba = Vec::with_capacity(row * height as usize);
        for y in 0..height as usize {
            let start = y * plane.stride;
            rgba.extend_from_slice(plane.data.get(start..start + row).ok_or_else(corrupt)?);
        }
        return Ok(Flat {
            width,
            height,
            rgba,
        });
    }
    // Deeper, or wider than sRGB: 16-bit samples, linearised and mapped.
    let image = library
        .decode(&handle, ColorSpace::Rgb(RgbChroma::HdrRgbaLe), None)
        .map_err(|_| corrupt())?;
    let plane = image.planes().interleaved.ok_or_else(corrupt)?;
    let bits = u32::from(plane.bits_per_pixel.max(1)) / 4;
    let bits = if bits == 0 || bits > 16 {
        u32::from(depth.max(8))
    } else {
        bits
    };
    let peak = ((1u32 << bits) - 1) as f32;
    let row = width as usize * 8;
    let mut pixels: Vec<[f32; 3]> = Vec::with_capacity(width as usize * height as usize);
    let mut alphas: Vec<u8> = Vec::with_capacity(pixels.capacity());
    let mut fixed = None;
    for y in 0..height as usize {
        let start = y * plane.stride;
        let line = plane.data.get(start..start + row).ok_or_else(corrupt)?;
        for sample in line.chunks_exact(8) {
            let value =
                |at: usize| f32::from(u16::from_le_bytes([sample[at], sample[at + 1]])) / peak;
            let (linear, scale) =
                tonemap::linearise([value(0), value(2), value(4)], transfer, primaries);
            fixed = scale;
            pixels.push(linear);
            alphas.push(to_byte(value(6)));
        }
    }
    let exposure = tonemap::exposure_of(&pixels, fixed);
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for (pixel, alpha) in pixels.iter().zip(alphas) {
        let mapped = tonemap::map_pixel(*pixel, exposure);
        rgba.extend_from_slice(&[
            to_byte(mapped[0]),
            to_byte(mapped[1]),
            to_byte(mapped[2]),
            alpha,
        ]);
    }
    Ok(Flat {
        width,
        height,
        rgba,
    })
}

/// Lay a picture with transparent pixels over the chequerboard, so it reads
/// as transparent and is drawn opaque.
fn finish(flat: Flat) -> Result<Picture, Failure> {
    let Flat {
        width,
        height,
        mut rgba,
    } = flat;
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err(corrupt());
    }
    if rgba.chunks_exact(4).any(|pixel| pixel[3] != 255) {
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            if pixel[3] == 255 {
                continue;
            }
            let (x, y) = (index as u32 % width, index as u32 / width);
            let under = if (x / CHECK_SIZE + y / CHECK_SIZE) % 2 == 0 {
                CHECK_LIGHT
            } else {
                CHECK_DARK
            };
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + u32::from(under) * (255 - alpha) + 127)
                    / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
    Ok(Picture {
        width,
        height,
        rgba,
    })
}

/// How big a draw of a `width` by `height` picture is: `scale_milli`
/// thousandths of its own size, cut to fit `max_w` by `max_h` (0 is no limit),
/// never above its own size and never over the largest frame.
pub fn size_for(width: u32, height: u32, scale_milli: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let mut scale = (f64::from(scale_milli) / 1000.0).clamp(0.01, 1.0);
    for (limit, extent) in [(max_w, w), (max_h, h)] {
        if limit > 0 && extent * scale > f64::from(limit) {
            scale = f64::from(limit) / extent;
        }
    }
    let area = (MAX_BYTES / 4) as f64;
    if w * h * scale * scale > area {
        scale = (area / (w * h)).sqrt();
    }
    (
        ((w * scale).round() as u32).max(1),
        ((h * scale).round() as u32).max(1),
    )
}

impl Picture {
    /// The picture at the size [`size_for`] says, in shared memory.
    pub fn render(
        &self,
        scale_milli: u32,
        max_w: u32,
        max_h: u32,
    ) -> Result<(u32, u32, Shared), Failure> {
        let (width, height) = size_for(self.width, self.height, scale_milli, max_w, max_h);
        let internal = |what: &str| {
            (
                ErrorCode::Internal,
                format!("The picture could not be drawn ({what})."),
            )
        };
        let mut shared = Shared::create("bingux-picture", width as usize * height as usize * 4)
            .map_err(|_| internal("shared memory"))?;
        if (width, height) == (self.width, self.height) {
            shared.bytes_mut().copy_from_slice(&self.rgba);
        } else {
            let source = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
                .ok_or_else(|| internal("pixels"))?;
            // A box-like filter for a big step down (fast and without aliasing),
            // a Catmull-Rom for a small one (sharp).
            let filter = if u64::from(self.width) > u64::from(width) * 3 {
                image::imageops::FilterType::Triangle
            } else {
                image::imageops::FilterType::CatmullRom
            };
            let scaled = image::imageops::resize(&source, width, height, filter);
            shared.bytes_mut().copy_from_slice(scaled.as_raw());
        }
        Ok((width, height, shared))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba(pixel));
        let mut out = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn tiff(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([10, 200, 30]));
        let mut out = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Tiff)
            .unwrap();
        out
    }

    #[test]
    fn a_tiff_decodes_to_opaque_pixels_of_its_size() {
        let Flat {
            width,
            height,
            rgba,
        } = decode_with_image(&tiff(30, 20), None).unwrap();
        assert_eq!((width, height, rgba.len()), (30, 20, 30 * 20 * 4));
        assert_eq!(&rgba[..4], &[10, 200, 30, 255]);
    }

    #[test]
    fn a_picture_that_is_not_one_is_refused_in_words() {
        let error = decode_with_image(b"definitely not a picture", None)
            .err()
            .unwrap();
        assert_eq!(error.0, ErrorCode::Unsupported);
        let cut = &png(8, 8, [1, 2, 3, 255])[..40];
        assert_eq!(
            decode_with_image(cut, None).err().unwrap().0,
            ErrorCode::Corrupt
        );
    }

    #[test]
    fn transparency_is_laid_over_the_chequerboard() {
        let flat = decode_with_image(&png(16, 8, [0, 0, 0, 0]), None).unwrap();
        let picture = finish(flat).unwrap();
        assert!(picture.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        // The first square is light, the second dark.
        assert_eq!(picture.rgba[0], CHECK_LIGHT);
        assert_eq!(picture.rgba[(CHECK_SIZE as usize) * 4], CHECK_DARK);
        // A half-transparent black over light grey is darker than the grey.
        let half = finish(decode_with_image(&png(2, 2, [0, 0, 0, 128]), None).unwrap()).unwrap();
        assert!(half.rgba[0] > 100 && half.rgba[0] < 130, "{}", half.rgba[0]);
    }

    #[test]
    fn a_raw_file_shows_the_largest_jpeg_it_carries() {
        let mut jpeg_small = Vec::new();
        image::RgbImage::from_pixel(160, 120, image::Rgb([200, 0, 0]))
            .write_to(&mut Cursor::new(&mut jpeg_small), ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg_big = Vec::new();
        image::RgbImage::from_pixel(1200, 800, image::Rgb([0, 0, 200]))
            .write_to(&mut Cursor::new(&mut jpeg_big), ImageFormat::Jpeg)
            .unwrap();
        let mut file = b"II*\0\x08\0\0\0".to_vec();
        file.extend_from_slice(&jpeg_small);
        file.extend_from_slice(&[0u8; 64]);
        file.extend_from_slice(&jpeg_big);
        let Flat { width, height, .. } = decode_raw(&file).unwrap();
        assert_eq!((width, height), (1200, 800));
        assert_eq!(
            decode_raw(b"II*\0 no pictures in here").err().unwrap().0,
            ErrorCode::Unsupported
        );
    }

    #[test]
    fn a_draw_never_enlarges_and_keeps_the_aspect() {
        assert_eq!(size_for(4000, 3000, 1000, 0, 0), (4000, 3000));
        assert_eq!(size_for(4000, 3000, 5000, 0, 0), (4000, 3000));
        assert_eq!(size_for(4000, 3000, 1000, 640, 640), (640, 480));
        assert_eq!(size_for(4000, 3000, 500, 0, 0), (2000, 1500));
        assert_eq!(size_for(1, 1, 1000, 640, 640), (1, 1));
    }

    #[test]
    fn a_high_dynamic_range_float_picture_comes_out_inside_the_display_range() {
        let mut image = image::Rgb32FImage::new(4, 4);
        for (x, _, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgb([x as f32 * 3.0, 0.5, 0.01]);
        }
        let Flat { rgba, .. } = flatten(DynamicImage::ImageRgb32F(image));
        assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        // Brighter input is brighter output.
        assert!(rgba[0] < rgba[4 * 3]);
    }
}
