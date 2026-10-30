//! Decoding, resizing, blurring and re-encoding of greeter images.
//!
//! These are pure functions over bytes. The converter child runs them inside
//! its sandbox; nothing here opens a file. Re-encoding every image from decoded
//! pixels is what sanitises it: container tricks, trailing data, EXIF
//! (including GPS positions), ICC profiles and animation frames do not survive.

use std::fmt;
use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits, RgbImage};

/// Gaussian blur of the blurred wallpaper, as the standard deviation in pixels at a
/// reference width of [`BLUR_REFERENCE_WIDTH`]. Same value as
/// `bingux-session`'s `WALLPAPER_BLUR_RADIUS`, so the greeter looks like the lock screen.
pub const WALLPAPER_BLUR_RADIUS: f32 = 32.0;
/// Brightness of the blurred wallpaper: 1.0 keeps the picture, 0.0 is black.
/// Same value as `bingux-session`'s `WALLPAPER_DIM`.
pub const WALLPAPER_DIM: f32 = 0.65;
/// The screen width the blur radius is defined for.
pub const BLUR_REFERENCE_WIDTH: f32 = 1920.0;
/// The blurred copy is stored at most this big. A blur with sigma 32 at 1920 px has no
/// detail left at a quarter of that, so a smaller image is as sharp as a bigger one
/// would be and costs a fraction of the blur time and storage. QML scales it up.
pub const BLUR_MAX_WIDTH: u32 = 960;
pub const BLUR_MAX_HEIGHT: u32 = 600;
/// JPEG quality of the blurred copy (same as `bingux-session`).
const BLUR_JPEG_QUALITY: u8 = 88;
/// The published wallpaper fits in this box and is never upscaled.
pub const WALLPAPER_MAX_WIDTH: u32 = 3840;
pub const WALLPAPER_MAX_HEIGHT: u32 = 2400;
const WALLPAPER_JPEG_QUALITY: u8 = 90;
/// The published avatar is a square of at most this many pixels.
pub const AVATAR_MAX_SIZE: u32 = 512;
/// Shown behind transparent pixels of a wallpaper: the shell's base tone.
const WALLPAPER_BASE: [u8; 3] = [0x16, 0x16, 0x16];

/// Why an image was refused or could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Not a JPEG, PNG or WebP file.
    UnsupportedFormat,
    /// The header declares more pixels, or a longer side, than allowed.
    TooLarge { width: u32, height: u32 },
    /// The decoder refused the image because it would exceed a size or memory limit.
    OverLimit(String),
    /// The bytes are not a valid image of the detected format.
    Malformed(String),
    /// Encoding the result failed.
    Encode(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("not a JPEG, PNG or WebP image"),
            Self::TooLarge { width, height } => write!(f, "image is too large ({width}x{height})"),
            Self::OverLimit(why) => write!(f, "image exceeds the decoder limits: {why}"),
            Self::Malformed(why) => write!(f, "malformed image: {why}"),
            Self::Encode(why) => write!(f, "could not encode the result: {why}"),
        }
    }
}

impl std::error::Error for ImageError {}

/// Decoder limits. The converter also sets `RLIMIT_AS`, so these are the
/// polite, early checks and the rlimit is the hard backstop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_pixels: u64,
    pub max_side: u32,
    pub max_alloc: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self { max_pixels: 64 * 1024 * 1024, max_side: 16_384, max_alloc: 768 * 1024 * 1024 }
    }
}

/// The encoded result of one conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Png,
    Jpeg,
}

/// Decodes `bytes` after checking the declared size against `limits`. EXIF
/// orientation is applied, so the published pixels are upright.
pub fn decode(bytes: &[u8], limits: DecodeLimits) -> Result<DynamicImage, ImageError> {
    let format = image::guess_format(bytes).map_err(|_| ImageError::UnsupportedFormat)?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP) {
        return Err(ImageError::UnsupportedFormat);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut decoder_limits = Limits::default();
    decoder_limits.max_image_width = Some(limits.max_side);
    decoder_limits.max_image_height = Some(limits.max_side);
    decoder_limits.max_alloc = Some(limits.max_alloc);
    reader.limits(decoder_limits);
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > limits.max_side
        || height > limits.max_side
        || u64::from(width) * u64::from(height) > limits.max_pixels
    {
        return Err(ImageError::TooLarge { width, height });
    }
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    decoded.apply_orientation(orientation);
    Ok(decoded)
}

fn decode_error(error: image::ImageError) -> ImageError {
    match error {
        image::ImageError::Limits(why) => ImageError::OverLimit(why.to_string()),
        other => ImageError::Malformed(other.to_string()),
    }
}

fn fit_within(image: &DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    if image.width() <= max_width && image.height() <= max_height {
        return image.clone();
    }
    image.resize(max_width, max_height, FilterType::Lanczos3)
}

/// Flattens transparency onto the shell's base tone.
fn flatten(image: &DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.to_rgb8();
    }
    let rgba = image.to_rgba8();
    let mut flat = RgbImage::new(rgba.width(), rgba.height());
    for (source, target) in rgba.pixels().zip(flat.pixels_mut()) {
        let alpha = u32::from(source.0[3]);
        for channel in 0..3 {
            let over = u32::from(source.0[channel]) * alpha + u32::from(WALLPAPER_BASE[channel]) * (255 - alpha);
            target.0[channel] = ((over + 127) / 255) as u8;
        }
    }
    flat
}

fn encode_jpeg(picture: &RgbImage, quality: u8) -> Result<Vec<u8>, ImageError> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .write_image(picture.as_raw(), picture.width(), picture.height(), ExtendedColorType::Rgb8)
        .map_err(|error| ImageError::Encode(error.to_string()))?;
    Ok(bytes)
}

/// The published wallpaper: upright, flattened, fitted into 3840x2400 without
/// upscaling, as JPEG. Also returns the RGB pixels the blurred copy derives from.
pub fn make_wallpaper(image: &DynamicImage) -> Result<(Encoded, RgbImage), ImageError> {
    let fitted = flatten(&fit_within(image, WALLPAPER_MAX_WIDTH, WALLPAPER_MAX_HEIGHT));
    let bytes = encode_jpeg(&fitted, WALLPAPER_JPEG_QUALITY)?;
    let encoded = Encoded { bytes, width: fitted.width(), height: fitted.height(), format: OutputFormat::Jpeg };
    Ok((encoded, fitted))
}

/// The blurred, dimmed twin of the wallpaper (same blur and dim as the lock screen).
pub fn make_blurred(wallpaper: &RgbImage) -> Result<Encoded, ImageError> {
    let source = DynamicImage::ImageRgb8(wallpaper.clone());
    let small = fit_within(&source, BLUR_MAX_WIDTH, BLUR_MAX_HEIGHT).to_rgb8();
    let sigma = WALLPAPER_BLUR_RADIUS * small.width() as f32 / BLUR_REFERENCE_WIDTH;
    let mut blurred = imageops::blur(&small, sigma);
    for pixel in blurred.pixels_mut() {
        for channel in &mut pixel.0 {
            *channel = (f32::from(*channel) * WALLPAPER_DIM).round() as u8;
        }
    }
    let bytes = encode_jpeg(&blurred, BLUR_JPEG_QUALITY)?;
    Ok(Encoded { bytes, width: blurred.width(), height: blurred.height(), format: OutputFormat::Jpeg })
}

/// The published avatar: a centred square crop of at most 512 px, as PNG with alpha.
pub fn make_avatar(image: &DynamicImage) -> Result<Encoded, ImageError> {
    let side = image.width().min(image.height());
    let left = (image.width() - side) / 2;
    let top = (image.height() - side) / 2;
    let square = image.crop_imm(left, top, side, side);
    let square = if side > AVATAR_MAX_SIZE {
        square.resize_exact(AVATAR_MAX_SIZE, AVATAR_MAX_SIZE, FilterType::Lanczos3)
    } else {
        square
    };
    let rgba = square.to_rgba8();
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
        .map_err(|error| ImageError::Encode(error.to_string()))?;
    Ok(Encoded { bytes, width: rgba.width(), height: rgba.height(), format: OutputFormat::Png })
}

/// Which published file an output is, for [`inspect_output`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRole {
    Avatar,
    Wallpaper,
    WallpaperBlur,
}

/// Structural check of a file the converter wrote, run by the parent before it is renamed into
/// the cache. It does not decode pixels. It checks that:
///
/// * the container is the expected format (by signature) and ends with its terminator (JPEG
///   `EOI`, PNG `IEND`), so a truncated or padded file is refused;
/// * the format's own header parses and declares exactly the width and height the converter
///   reported;
/// * those dimensions fit the role (avatar: square, at most 512; wallpaper: at most 3840x2400;
///   blur: at most 960x600).
pub fn inspect_output(bytes: &[u8], role: OutputRole, format: OutputFormat, width: u32, height: u32) -> Result<(), String> {
    let (expected, image_format) = match (role, format) {
        (OutputRole::Avatar, OutputFormat::Png) => ((AVATAR_MAX_SIZE, AVATAR_MAX_SIZE), ImageFormat::Png),
        (OutputRole::Wallpaper, OutputFormat::Jpeg) => ((WALLPAPER_MAX_WIDTH, WALLPAPER_MAX_HEIGHT), ImageFormat::Jpeg),
        (OutputRole::WallpaperBlur, OutputFormat::Jpeg) => ((BLUR_MAX_WIDTH, BLUR_MAX_HEIGHT), ImageFormat::Jpeg),
        _ => return Err(format!("{role:?} has the wrong output format {format:?}")),
    };
    if image::guess_format(bytes).ok() != Some(image_format) {
        return Err("output has the wrong file signature".into());
    }
    let terminated = match format {
        OutputFormat::Jpeg => bytes.ends_with(&[0xff, 0xd9]),
        OutputFormat::Png => bytes.ends_with(b"IEND\xaeB`\x82"),
    };
    if !terminated {
        return Err("output is truncated or has trailing data".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image_format);
    let mut limits = Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|error| format!("output header: {error}"))?;
    let declared = decoder.dimensions();
    if declared != (width, height) {
        return Err(format!("output is {}x{}, the converter reported {width}x{height}", declared.0, declared.1));
    }
    let fits = width > 0 && height > 0 && width <= expected.0 && height <= expected.1;
    if !fits || (role == OutputRole::Avatar && width != height) {
        return Err(format!("output size {width}x{height} is not valid for {role:?}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, Rgba, RgbaImage};

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let picture = RgbImage::from_fn(width, height, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(picture.as_raw(), width, height, ExtendedColorType::Rgb8)
            .unwrap();
        bytes
    }

    #[test]
    fn rejects_garbage_and_foreign_formats() {
        assert_eq!(decode(b"definitely not an image", DecodeLimits::default()).unwrap_err(), ImageError::UnsupportedFormat);
        assert_eq!(decode(b"GIF89a\x01\x00\x01\x00\x00\x00\x00;", DecodeLimits::default()).unwrap_err(), ImageError::UnsupportedFormat);
    }

    #[test]
    fn rejects_declared_size_before_decoding_pixels() {
        let limits = DecodeLimits { max_pixels: 100 * 100, ..DecodeLimits::default() };
        let error = decode(&png_of(200, 200), limits).unwrap_err();
        assert_eq!(error, ImageError::TooLarge { width: 200, height: 200 });
        let narrow = DecodeLimits { max_side: 64, ..DecodeLimits::default() };
        assert!(decode(&png_of(65, 4), narrow).is_err());
    }

    #[test]
    fn truncated_image_is_malformed() {
        let mut bytes = png_of(64, 64);
        bytes.truncate(bytes.len() / 2);
        assert!(matches!(decode(&bytes, DecodeLimits::default()), Err(ImageError::Malformed(_))));
    }

    #[test]
    fn avatar_is_a_centred_square_without_upscaling() {
        let picture = decode(&png_of(300, 100), DecodeLimits::default()).unwrap();
        let avatar = make_avatar(&picture).unwrap();
        assert_eq!((avatar.width, avatar.height), (100, 100));
        let big = decode(&png_of(1200, 900), DecodeLimits::default()).unwrap();
        let avatar = make_avatar(&big).unwrap();
        assert_eq!((avatar.width, avatar.height), (512, 512));
        assert_eq!(image::guess_format(&avatar.bytes).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn wallpaper_fits_the_box_and_is_not_upscaled() {
        let huge = decode(&png_of(6000, 3000), DecodeLimits::default()).unwrap();
        let (wallpaper, _) = make_wallpaper(&huge).unwrap();
        assert_eq!((wallpaper.width, wallpaper.height), (3840, 1920));
        let small = decode(&png_of(800, 600), DecodeLimits::default()).unwrap();
        let (wallpaper, _) = make_wallpaper(&small).unwrap();
        assert_eq!((wallpaper.width, wallpaper.height), (800, 600));
        assert_eq!(image::guess_format(&wallpaper.bytes).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn blurred_copy_is_small_dimmed_and_smooth() {
        // A hard white/black split: after the blur the middle must be a gradient.
        let picture = RgbImage::from_fn(1920, 1080, |x, _| if x < 960 { Rgb([255, 255, 255]) } else { Rgb([0, 0, 0]) });
        let (_, rgb) = make_wallpaper(&DynamicImage::ImageRgb8(picture)).unwrap();
        let blurred = make_blurred(&rgb).unwrap();
        assert!(blurred.width <= BLUR_MAX_WIDTH && blurred.height <= BLUR_MAX_HEIGHT);
        let decoded = image::load_from_memory(&blurred.bytes).unwrap().to_rgb8();
        let far_white = decoded.get_pixel(5, decoded.height() / 2).0[0];
        let edge = decoded.get_pixel(decoded.width() / 2, decoded.height() / 2).0[0];
        assert!(far_white <= (255.0 * WALLPAPER_DIM) as u8 + 3 && far_white >= 150, "dimmed white, got {far_white}");
        assert!(edge > 20 && edge < far_white, "the edge is a gradient, got {edge}");
    }

    #[test]
    fn inspect_output_accepts_what_the_encoders_produce_and_nothing_else() {
        let picture = decode(&png_of(300, 200), DecodeLimits::default()).unwrap();
        let avatar = make_avatar(&picture).unwrap();
        let ok = |bytes: &[u8], role, format, w, h| inspect_output(bytes, role, format, w, h);
        assert_eq!(ok(&avatar.bytes, OutputRole::Avatar, OutputFormat::Png, 200, 200), Ok(()));
        // Wrong reported size, wrong role, wrong format.
        assert!(ok(&avatar.bytes, OutputRole::Avatar, OutputFormat::Png, 100, 100).is_err());
        assert!(ok(&avatar.bytes, OutputRole::Wallpaper, OutputFormat::Png, 200, 200).is_err());
        assert!(ok(&avatar.bytes, OutputRole::Avatar, OutputFormat::Jpeg, 200, 200).is_err());
        // Truncated, padded, signature only.
        assert!(ok(&avatar.bytes[..avatar.bytes.len() - 5], OutputRole::Avatar, OutputFormat::Png, 200, 200).is_err());
        let mut padded = avatar.bytes.clone();
        padded.extend_from_slice(b"trailing");
        assert!(ok(&padded, OutputRole::Avatar, OutputFormat::Png, 200, 200).is_err());
        assert!(ok(b"\x89PNG\r\n\x1a\n", OutputRole::Avatar, OutputFormat::Png, 1, 1).is_err());
        let (wallpaper, rgb) = make_wallpaper(&picture).unwrap();
        assert_eq!(ok(&wallpaper.bytes, OutputRole::Wallpaper, OutputFormat::Jpeg, 300, 200), Ok(()));
        assert!(ok(&wallpaper.bytes, OutputRole::Wallpaper, OutputFormat::Jpeg, 300, 201).is_err());
        assert!(ok(&wallpaper.bytes[..wallpaper.bytes.len() - 2], OutputRole::Wallpaper, OutputFormat::Jpeg, 300, 200).is_err());
        let blurred = make_blurred(&rgb).unwrap();
        assert_eq!(ok(&blurred.bytes, OutputRole::WallpaperBlur, OutputFormat::Jpeg, blurred.width, blurred.height), Ok(()));
        // A wallpaper-sized JPEG is not an acceptable blur, and a non-square PNG is not an avatar.
        let big = decode(&png_of(2000, 1200), DecodeLimits::default()).unwrap();
        let (big_wallpaper, _) = make_wallpaper(&big).unwrap();
        assert!(ok(&big_wallpaper.bytes, OutputRole::WallpaperBlur, OutputFormat::Jpeg, 2000, 1200).is_err());
        let mut rectangle = Vec::new();
        PngEncoder::new(&mut rectangle).write_image(&vec![0u8; 40 * 20 * 4], 40, 20, ExtendedColorType::Rgba8).unwrap();
        assert!(ok(&rectangle, OutputRole::Avatar, OutputFormat::Png, 40, 20).is_err());
    }

    #[test]
    fn transparency_is_flattened_onto_the_base_tone() {
        let transparent = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 0]));
        let flat = flatten(&DynamicImage::ImageRgba8(transparent));
        assert_eq!(flat.get_pixel(0, 0).0, WALLPAPER_BASE);
    }
}
