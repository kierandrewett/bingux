//! Making pictures, films and songs smaller: what the options mean to
//! `ffmpeg` and ImageMagick, decided without touching a disk.
//!
//! Everything here is pure, so the rules are tested to the argument: which
//! CRF a level is, that a name with a dash or a colon is never read as a
//! switch or a protocol, how the progress lines read, how an estimate is
//! scaled. [`super::shrink`] runs the plans.
//!
//! # Which program does what
//!
//! | Kind | Program | Why |
//! | ---- | ------- | --- |
//! | JPEG, WebP, PNG out | ImageMagick (`magick`), else `ffmpeg` | quality, orientation and metadata handling; `ffmpeg` drops EXIF |
//! | AVIF out | `ffmpeg` (SVT-AV1 or libaom) | ImageMagick here has no AVIF |
//! | Video, audio | `ffmpeg`; `ffprobe` for length, size and frame rate | |
//!
//! A file's bytes reach ImageMagick on standard input, so no name, however
//! odd (a colon, brackets, a leading dash), is parsed as a format or a frame
//! selector. `ffmpeg` gets `file:` URLs after `-i`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::Category;

use super::pack_plan::Level;
use super::tool::{self, Input, Spec};

/// What kind of media a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
}

impl MediaKind {
    pub fn of(category: Category) -> Option<MediaKind> {
        match category {
            Category::Image => Some(MediaKind::Image),
            Category::Video => Some(MediaKind::Video),
            Category::Audio => Some(MediaKind::Audio),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => "Pictures",
            MediaKind::Video => "Videos",
            MediaKind::Audio => "Audio",
        }
    }
}

/// What this machine has to work with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub magick: Option<PathBuf>,
    /// The encoders `ffmpeg -encoders` lists.
    pub encoders: Vec<String>,
    /// The formats ImageMagick can read and write.
    pub magick_formats: Vec<String>,
}

static TOOLS: OnceLock<Tools> = OnceLock::new();

impl Tools {
    /// Look at the machine. Starts `ffmpeg` and ImageMagick once, which takes
    /// a moment: call it from a worker, early (see [`Tools::get`]).
    pub fn detect() -> Tools {
        let cancel = crate::Cancel::new();
        let ffmpeg = tool::find("ffmpeg");
        let ffprobe = tool::find("ffprobe");
        let magick = tool::find("magick").or_else(|| tool::find("convert"));
        let encoders = ffmpeg
            .as_ref()
            .and_then(|program| {
                let spec = Spec::new(program).args(["-hide_banner", "-encoders"]);
                tool::capture(&spec, &cancel, 256 * 1024).ok()
            })
            .map(|(_, out)| parse_encoders(&String::from_utf8_lossy(&out)))
            .unwrap_or_default();
        let magick_formats = magick
            .as_ref()
            .and_then(|program| {
                let spec = Spec::new(program).args(["-list", "format"]);
                tool::capture(&spec, &cancel, 256 * 1024).ok()
            })
            .map(|(_, out)| parse_magick_formats(&String::from_utf8_lossy(&out)))
            .unwrap_or_default();
        Tools {
            ffmpeg,
            ffprobe,
            magick,
            encoders,
            magick_formats,
        }
    }

    /// [`Tools::detect`], once per process.
    pub fn get() -> &'static Tools {
        TOOLS.get_or_init(Tools::detect)
    }

    /// Already detected? A caller on the UI thread must not call
    /// [`Tools::get`] before this is true.
    pub fn is_ready() -> bool {
        TOOLS.get().is_some()
    }

    fn has_encoder(&self, name: &str) -> bool {
        self.encoders.iter().any(|encoder| encoder == name)
    }

    fn magick_writes(&self, format: &str) -> bool {
        self.magick.is_some() && self.magick_formats.iter().any(|name| name == format)
    }

    /// Why pictures cannot be reduced here, or `None`.
    pub fn image_unavailable(&self) -> Option<String> {
        if self.magick.is_some() || self.ffmpeg.is_some() {
            None
        } else {
            Some("ImageMagick and ffmpeg are not installed.".into())
        }
    }

    /// Why films and songs cannot be reduced here, or `None`.
    pub fn av_unavailable(&self) -> Option<String> {
        if self.ffmpeg.is_none() {
            Some("ffmpeg is not installed.".into())
        } else if self.ffprobe.is_none() {
            Some("ffprobe is not installed.".into())
        } else {
            None
        }
    }

    /// Why `kind` cannot be reduced here, or `None`.
    pub fn unavailable(&self, kind: MediaKind) -> Option<String> {
        match kind {
            MediaKind::Image => self.image_unavailable(),
            MediaKind::Video | MediaKind::Audio => self.av_unavailable(),
        }
    }

    /// Why a picture cannot be written as `format`, or `None`.
    pub fn image_format_unavailable(&self, format: ImageFormat) -> Option<String> {
        if let Some(reason) = self.image_unavailable() {
            return Some(reason);
        }
        match format {
            ImageFormat::Keep => None,
            ImageFormat::Jpeg => (!(self.magick_writes("JPEG") || self.has_encoder("mjpeg")))
                .then(|| "No JPEG encoder is installed.".to_string()),
            ImageFormat::Png => (!(self.magick_writes("PNG") || self.has_encoder("png")))
                .then(|| "No PNG encoder is installed.".to_string()),
            ImageFormat::Webp => (!(self.magick_writes("WEBP") || self.has_encoder("libwebp")))
                .then(|| "No WebP encoder is installed.".to_string()),
            ImageFormat::Avif => {
                if self.ffmpeg.is_none() {
                    Some("AVIF needs ffmpeg, which is not installed.".into())
                } else if !(self.has_encoder("libsvtav1") || self.has_encoder("libaom-av1")) {
                    Some("ffmpeg here has no AV1 encoder.".into())
                } else {
                    None
                }
            }
        }
    }

    /// Why a video cannot be made with `codec`, or `None`.
    pub fn video_codec_unavailable(&self, codec: VideoCodec) -> Option<String> {
        if let Some(reason) = self.av_unavailable() {
            return Some(reason);
        }
        let any = codec.encoders().iter().any(|name| self.has_encoder(name));
        (!any).then(|| format!("ffmpeg here has no {} encoder.", codec.label()))
    }

    /// Why audio cannot be made with `codec`, or `None`.
    pub fn audio_codec_unavailable(&self, codec: AudioCodec) -> Option<String> {
        if let Some(reason) = self.av_unavailable() {
            return Some(reason);
        }
        (!self.has_encoder(codec.encoder()))
            .then(|| format!("ffmpeg here has no {} encoder.", codec.label()))
    }

    /// The video encoder to use for `codec`: the first the machine has.
    fn video_encoder(&self, codec: VideoCodec) -> Option<&'static str> {
        codec
            .encoders()
            .iter()
            .copied()
            .find(|name| self.has_encoder(name))
    }

    /// The audio encoder for a video's sound: AAC in MP4, Opus (else Vorbis)
    /// in WebM.
    fn sound_encoder(&self, codec: VideoCodec) -> Option<&'static str> {
        match codec {
            VideoCodec::Vp9 => ["libopus", "libvorbis"]
                .into_iter()
                .find(|name| self.has_encoder(name)),
            _ => self.has_encoder("aac").then_some("aac"),
        }
    }
}

/// The encoder names in `ffmpeg -encoders`: lines like
/// ` V....D libx264   libx264 H.264`.
pub fn parse_encoders(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let flags = words.next()?;
            let name = words.next()?;
            let kind = flags.chars().next()?;
            (flags.len() == 6 && matches!(kind, 'V' | 'A' | 'S') && name != "=").then(|| name.to_string())
        })
        .collect()
}

/// The names of the formats ImageMagick reads and writes (`rw`), from
/// `magick -list format`.
pub fn parse_magick_formats(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?.trim_end_matches('*');
            let _module = words.next()?;
            let mode = words.next()?;
            (mode.len() == 3 && mode.starts_with('r') && mode[1..].starts_with('w')).then(|| name.to_string())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// What a picture is written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// The picture's own format (JPEG stays JPEG; BMP and TIFF become PNG).
    Keep,
    Jpeg,
    Webp,
    Avif,
    Png,
}

impl ImageFormat {
    pub const ALL: [ImageFormat; 5] = [
        ImageFormat::Keep,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Png,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ImageFormat::Keep => "Keep format",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Webp => "WebP",
            ImageFormat::Avif => "AVIF",
            ImageFormat::Png => "PNG",
        }
    }

    /// PNG has no quality: it is lossless.
    pub fn has_quality(self) -> bool {
        self != ImageFormat::Png
    }

    pub fn key(self) -> &'static str {
        match self {
            ImageFormat::Keep => "keep",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Webp => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Png => "png",
        }
    }

    pub fn from_key(key: &str) -> Option<ImageFormat> {
        ImageFormat::ALL.into_iter().find(|format| format.key() == key)
    }
}

/// The pictures this can read and shrink, by extension, with the format they
/// keep. GIF and SVG are not here: a GIF may move and an SVG is not pixels.
pub fn image_format_of(name: &str) -> Option<ImageFormat> {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.')?.1;
    match ext {
        "jpg" | "jpeg" | "jpe" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "webp" => Some(ImageFormat::Webp),
        "avif" => Some(ImageFormat::Avif),
        "bmp" | "tif" | "tiff" => Some(ImageFormat::Png),
        _ => None,
    }
}

/// Picture options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageOptions {
    pub format: ImageFormat,
    /// 1 to 100.
    pub quality: u8,
    /// The longest side, or `None` for the original size.
    pub max_dimension: Option<u32>,
    /// Remove EXIF, GPS and other metadata (colour profiles stay).
    pub strip_metadata: bool,
}

impl Default for ImageOptions {
    fn default() -> Self {
        ImageOptions {
            format: ImageFormat::Keep,
            quality: 75,
            max_dimension: None,
            strip_metadata: false,
        }
    }
}

/// The dimension caps on offer, the longest side in pixels.
pub const DIMENSIONS: [u32; 4] = [3840, 2560, 1920, 1280];

/// A video codec preset: the codec and its container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    Vp9,
}

impl VideoCodec {
    pub const ALL: [VideoCodec; 4] = [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1, VideoCodec::Vp9];

    pub fn label(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264 (MP4)",
            VideoCodec::H265 => "H.265 (MP4)",
            VideoCodec::Av1 => "AV1 (MP4)",
            VideoCodec::Vp9 => "VP9 (WebM)",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "h265",
            VideoCodec::Av1 => "av1",
            VideoCodec::Vp9 => "vp9",
        }
    }

    pub fn from_key(key: &str) -> Option<VideoCodec> {
        VideoCodec::ALL.into_iter().find(|codec| codec.key() == key)
    }

    pub fn extension(self) -> &'static str {
        match self {
            VideoCodec::Vp9 => "webm",
            _ => "mp4",
        }
    }

    /// The `ffmpeg` encoders that make it, best first.
    fn encoders(self) -> &'static [&'static str] {
        match self {
            VideoCodec::H264 => &["libx264"],
            VideoCodec::H265 => &["libx265"],
            VideoCodec::Av1 => &["libsvtav1", "libaom-av1"],
            VideoCodec::Vp9 => &["libvpx-vp9"],
        }
    }

    /// The CRF that is "about right" for a level, and the speed that goes
    /// with it. Smallest is slower and a little stronger; Fastest is quick.
    pub fn crf_for(self, level: Level) -> u8 {
        match (self, level) {
            (VideoCodec::H264, Level::Fastest) => 25,
            (VideoCodec::H264, Level::Balanced) => 24,
            (VideoCodec::H264, Level::Smallest) => 27,
            (VideoCodec::H265, Level::Fastest) => 28,
            (VideoCodec::H265, Level::Balanced) => 27,
            (VideoCodec::H265, Level::Smallest) => 30,
            (VideoCodec::Av1, Level::Fastest) => 35,
            (VideoCodec::Av1, Level::Balanced) => 34,
            (VideoCodec::Av1, Level::Smallest) => 37,
            (VideoCodec::Vp9, Level::Fastest) => 34,
            (VideoCodec::Vp9, Level::Balanced) => 33,
            (VideoCodec::Vp9, Level::Smallest) => 36,
        }
    }

    /// The range a custom CRF may take: lower is better and bigger.
    pub fn crf_range(self) -> (u8, u8) {
        match self {
            VideoCodec::H264 | VideoCodec::H265 => (16, 40),
            VideoCodec::Av1 | VideoCodec::Vp9 => (20, 50),
        }
    }
}

/// How a video is squeezed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoQuality {
    Level(Level),
    /// A constant rate factor of the person's choosing.
    Crf(u8),
}

/// What happens to a video's sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Remove,
    /// Re-encode at this many kilobits a second.
    Kbps(u32),
}

/// The sound bitrates on offer.
pub const SOUND_KBPS: [u32; 4] = [64, 96, 128, 192];

/// Film options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoOptions {
    pub codec: VideoCodec,
    pub quality: VideoQuality,
    /// The tallest the picture gets, in pixels, or `None` for the original.
    pub max_height: Option<u32>,
    /// The most frames a second, or `None` for the original.
    pub max_fps: Option<u32>,
    pub sound: Sound,
    pub strip_metadata: bool,
}

impl Default for VideoOptions {
    fn default() -> Self {
        VideoOptions {
            codec: VideoCodec::H264,
            quality: VideoQuality::Level(Level::Balanced),
            max_height: None,
            max_fps: None,
            sound: Sound::Kbps(128),
            strip_metadata: false,
        }
    }
}

/// The heights on offer.
pub const HEIGHTS: [u32; 5] = [2160, 1440, 1080, 720, 480];
/// The frame rates on offer.
pub const FRAME_RATES: [u32; 3] = [60, 30, 24];

/// A song codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioCodec {
    Opus,
    Aac,
    Mp3,
    Flac,
}

impl AudioCodec {
    pub const ALL: [AudioCodec; 4] = [AudioCodec::Opus, AudioCodec::Aac, AudioCodec::Mp3, AudioCodec::Flac];

    pub fn label(self) -> &'static str {
        match self {
            AudioCodec::Opus => "Opus",
            AudioCodec::Aac => "AAC",
            AudioCodec::Mp3 => "MP3",
            AudioCodec::Flac => "FLAC (lossless)",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            AudioCodec::Opus => "opus",
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Flac => "flac",
        }
    }

    pub fn from_key(key: &str) -> Option<AudioCodec> {
        AudioCodec::ALL.into_iter().find(|codec| codec.key() == key)
    }

    pub fn extension(self) -> &'static str {
        match self {
            AudioCodec::Opus => "opus",
            AudioCodec::Aac => "m4a",
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Flac => "flac",
        }
    }

    fn encoder(self) -> &'static str {
        match self {
            AudioCodec::Opus => "libopus",
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "libmp3lame",
            AudioCodec::Flac => "flac",
        }
    }

    pub fn has_bitrate(self) -> bool {
        self != AudioCodec::Flac
    }
}

/// The bitrates on offer, kilobits a second.
pub const AUDIO_KBPS: [u32; 6] = [64, 96, 128, 192, 256, 320];

/// Song options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOptions {
    pub codec: AudioCodec,
    pub kbps: u32,
    pub strip_metadata: bool,
}

impl Default for AudioOptions {
    fn default() -> Self {
        AudioOptions {
            codec: AudioCodec::Opus,
            kbps: 128,
            strip_metadata: false,
        }
    }
}

/// Everything the Reduce size choice holds: one set per kind, since a
/// selection may hold pictures and films together.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReduceOptions {
    pub image: ImageOptions,
    pub video: VideoOptions,
    pub audio: AudioOptions,
    /// The reduced file takes the original's place; the original goes to the
    /// Trash once the new one checks out.
    pub replace_original: bool,
}

// ---------------------------------------------------------------------------
// Probing
// ---------------------------------------------------------------------------

/// What `ffprobe` says about a file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub has_video: bool,
    pub has_audio: bool,
}

/// The command that probes `input`.
pub fn probe_spec(ffprobe: &Path, input: &Path) -> Spec {
    Spec::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,avg_frame_rate:format=duration",
            "-of",
            "compact=p=0",
            "-i",
        ])
        .arg(file_url(input))
        .nice(true)
}

/// Read `ffprobe`'s compact output: one line a stream, `key=value|key=value`,
/// then the format's own line.
pub fn parse_probe(text: &str) -> Probe {
    let mut probe = Probe::default();
    for line in text.lines() {
        let mut kind = "";
        let (mut width, mut height, mut rate, mut duration) = (None, None, None, None);
        for pair in line.split('|') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            match key {
                "codec_type" => kind = value,
                "width" => width = value.parse::<u32>().ok(),
                "height" => height = value.parse::<u32>().ok(),
                "avg_frame_rate" => rate = parse_rate(value),
                "duration" => duration = value.parse::<f64>().ok().filter(|d| d.is_finite() && *d >= 0.0),
                _ => {}
            }
        }
        match kind {
            // A cover picture is a "video" stream of one frame: it is not a film.
            "video" => {
                if probe.width.is_none() {
                    probe.width = width;
                    probe.height = height;
                    probe.fps = rate;
                }
                probe.has_video = true;
            }
            "audio" => probe.has_audio = true,
            _ => {
                if duration.is_some() {
                    probe.duration = duration;
                }
            }
        }
    }
    probe
}

fn parse_rate(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let (numerator, denominator): (f64, f64) = (numerator.parse().ok()?, denominator.parse().ok()?);
    (denominator > 0.0 && numerator > 0.0).then(|| numerator / denominator)
}

/// A path as the `file:` URL `ffmpeg` reads, so that a name with a colon or
/// a leading dash is the file and nothing else.
pub fn file_url(path: &Path) -> OsString {
    let mut url = OsString::from("file:");
    url.push(path.as_os_str());
    url
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

/// The file name of a reduced `stem` with `extension`, the way it sits beside
/// the original: `holiday (compressed).jpg`, then `holiday (compressed 2).jpg`.
pub fn reduced_name(stem: &str, extension: &str, number: u32) -> String {
    let tag = if number <= 1 {
        "compressed".to_string()
    } else {
        format!("compressed {number}")
    };
    format!("{stem} ({tag}).{extension}")
}

/// The first reduced name whose file is free.
pub fn free_reduced_name(stem: &str, extension: &str, taken: impl Fn(&str) -> bool) -> String {
    (1u32..)
        .map(|number| reduced_name(stem, extension, number))
        .find(|name| !taken(name))
        .unwrap_or_else(|| reduced_name(stem, extension, 1))
}

/// A name's stem and extension: `holiday.tar.gz` is not special here, only
/// the last dot counts. A dot-file has no extension.
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

// ---------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------

/// What to run for one file.
#[derive(Debug)]
pub struct Plan {
    pub spec: Spec,
    /// The extension of the file it makes, without the dot.
    pub extension: String,
    /// The length of what is read, when known, for the progress.
    pub duration: Option<f64>,
}

fn quiet_ffmpeg(ffmpeg: &Path) -> Spec {
    Spec::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"])
        .nice(true)
}

/// The size a picture of `width` x `height` gets when its longest side may
/// be `cap`: the same shape, never bigger than it is.
pub fn fitted(width: u32, height: u32, cap: u32) -> (u32, u32) {
    let longest = width.max(height);
    if cap == 0 || longest <= cap {
        return (width, height);
    }
    let scale = f64::from(cap) / f64::from(longest);
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (scaled(width), scaled(height))
}

/// The quality of AVIF as a CRF: 100 is nearly lossless, 1 is rough.
pub fn avif_crf(quality: u8) -> u8 {
    let quality = f64::from(quality.clamp(1, 100));
    (63.0 - quality * 0.57).round().clamp(6.0, 63.0) as u8
}

/// The extension a picture is written with.
pub fn image_extension(options: &ImageOptions, source_name: &str) -> Option<&'static str> {
    let format = match options.format {
        ImageFormat::Keep => image_format_of(source_name)?,
        other => other,
    };
    Some(match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Avif => "avif",
        ImageFormat::Png | ImageFormat::Keep => "png",
    })
}

/// How to shrink one picture into `out` (a file name of ours in a work
/// folder). `probe` is read for the AVIF path, which needs the size.
pub fn image_plan(
    tools: &Tools,
    options: &ImageOptions,
    input: &Path,
    out: &Path,
    probe: Option<&Probe>,
) -> Result<Plan, String> {
    let name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = match options.format {
        ImageFormat::Keep => image_format_of(&name).ok_or_else(|| "This kind of picture cannot be reduced here.".to_string())?,
        other => other,
    };
    let source = image_format_of(&name).ok_or_else(|| "This kind of picture cannot be reduced here.".to_string())?;
    if let Some(reason) = tools.image_format_unavailable(target) {
        return Err(reason);
    }
    let extension = image_extension(options, &name).unwrap_or("png").to_string();
    let quality = options.quality.clamp(1, 100);
    // ImageMagick reads AVIF badly or not at all here; `ffmpeg` writes AVIF.
    let magick_reads = source != ImageFormat::Avif;
    let by_magick = target != ImageFormat::Avif
        && magick_reads
        && tools.magick.is_some()
        && match target {
            ImageFormat::Jpeg => tools.magick_writes("JPEG"),
            ImageFormat::Webp => tools.magick_writes("WEBP"),
            _ => tools.magick_writes("PNG"),
        };
    if by_magick {
        let program = tools.magick.as_ref().ok_or_else(|| "ImageMagick is not installed.".to_string())?;
        let mut spec = Spec::new(program).nice(true).arg("-").arg("-auto-orient");
        if let Some(cap) = options.max_dimension {
            spec = spec.arg("-resize").arg(format!("{cap}x{cap}>"));
        }
        if options.strip_metadata {
            // Everything but the colour profile: EXIF (camera, GPS), XMP, IPTC.
            spec = spec.args(["+profile", "!icc,*", "-set", "comment", ""]);
        }
        spec = match target {
            ImageFormat::Jpeg => spec.args(["-sampling-factor", "4:2:0", "-interlace", "Plane", "-quality"]).arg(quality.to_string()),
            ImageFormat::Webp => spec.args(["-define", "webp:method=4", "-quality"]).arg(quality.to_string()),
            _ => spec.args(["-define", "png:compression-level=9", "-define", "png:compression-filter=5"]),
        };
        // The output is named by us and begins with a known letter, so the
        // format prefix pins the format and nothing else can be read from it.
        let prefix = match target {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            _ => "png",
        };
        let mut target_arg = OsString::from(format!("{prefix}:"));
        target_arg.push(out.as_os_str());
        spec = spec.arg(target_arg).with_input(Input::File(input.to_path_buf()));
        return Ok(Plan {
            spec,
            extension,
            duration: None,
        });
    }
    let ffmpeg = tools.ffmpeg.as_ref().ok_or_else(|| "ffmpeg is not installed.".to_string())?;
    let mut spec = quiet_ffmpeg(ffmpeg).arg("-i").arg(file_url(input));
    if let (Some(cap), Some(probe)) = (options.max_dimension, probe)
        && let (Some(width), Some(height)) = (probe.width, probe.height)
    {
        let (new_width, new_height) = fitted(width, height, cap);
        if (new_width, new_height) != (width, height) {
            spec = spec.arg("-vf").arg(format!("scale={new_width}:{new_height}"));
        }
    }
    spec = spec.args(["-frames:v", "1"]);
    spec = if options.strip_metadata {
        spec.args(["-map_metadata", "-1"])
    } else {
        spec.args(["-map_metadata", "0"])
    };
    spec = match target {
        ImageFormat::Avif => {
            let encoder = tools.video_encoder(VideoCodec::Av1).unwrap_or("libaom-av1");
            let crf = avif_crf(quality).to_string();
            let spec = spec.args(["-pix_fmt", "yuv420p", "-c:v", encoder, "-crf"]).arg(crf);
            if encoder == "libaom-av1" {
                spec.args(["-b:v", "0", "-cpu-used", "6", "-still-picture", "1"])
            } else {
                spec.args(["-preset", "6"])
            }
        }
        ImageFormat::Webp => spec.args(["-c:v", "libwebp", "-quality"]).arg(quality.to_string()),
        ImageFormat::Jpeg => {
            // mjpeg's scale is 2 (best) to 31 (worst).
            let q = (31.0 - f64::from(quality) * 0.29).round().clamp(2.0, 31.0) as u32;
            spec.args(["-pix_fmt", "yuvj420p", "-c:v", "mjpeg", "-q:v"]).arg(q.to_string())
        }
        _ => spec.args(["-c:v", "png", "-compression_level", "9"]),
    };
    spec = spec.arg("-f").arg(match target {
        ImageFormat::Avif => "avif",
        ImageFormat::Webp => "webp",
        ImageFormat::Jpeg => "image2",
        _ => "image2",
    });
    spec = spec.arg(out);
    Ok(Plan {
        spec,
        extension,
        duration: None,
    })
}

/// The video filters, from what the source is and what the caps say.
pub fn video_filters(options: &VideoOptions, probe: &Probe) -> Option<String> {
    let mut filters = Vec::new();
    if let (Some(cap), Some(height)) = (options.max_height, probe.height)
        && height > cap
    {
        // -2 keeps the width even and the shape.
        filters.push(format!("scale=-2:{cap}"));
    }
    if let (Some(cap), Some(fps)) = (options.max_fps, probe.fps)
        && fps > f64::from(cap) + 0.01
    {
        filters.push(format!("fps={cap}"));
    }
    (!filters.is_empty()).then(|| filters.join(","))
}

/// The effective CRF of a video's quality.
pub fn crf_of(codec: VideoCodec, quality: VideoQuality) -> u8 {
    match quality {
        VideoQuality::Level(level) => codec.crf_for(level),
        VideoQuality::Crf(crf) => {
            let (low, high) = codec.crf_range();
            crf.clamp(low, high)
        }
    }
}

fn speed_args(codec: VideoCodec, encoder: &str, quality: VideoQuality) -> Vec<&'static str> {
    let level = match quality {
        VideoQuality::Level(level) => level,
        VideoQuality::Crf(_) => Level::Balanced,
    };
    match (codec, encoder) {
        (VideoCodec::H264 | VideoCodec::H265, _) => vec![
            "-preset",
            match level {
                Level::Fastest => "veryfast",
                Level::Balanced => "medium",
                Level::Smallest => "slower",
            },
        ],
        (VideoCodec::Av1, "libaom-av1") => vec![
            "-b:v",
            "0",
            "-cpu-used",
            match level {
                Level::Fastest => "8",
                Level::Balanced => "6",
                Level::Smallest => "4",
            },
        ],
        (VideoCodec::Av1, _) => vec![
            "-preset",
            match level {
                Level::Fastest => "10",
                Level::Balanced => "8",
                Level::Smallest => "5",
            },
        ],
        (VideoCodec::Vp9, _) => vec![
            "-b:v",
            "0",
            "-row-mt",
            "1",
            "-cpu-used",
            match level {
                Level::Fastest => "5",
                Level::Balanced => "2",
                Level::Smallest => "1",
            },
        ],
    }
}

/// How to shrink one film into `out`.
pub fn video_plan(
    tools: &Tools,
    options: &VideoOptions,
    input: &Path,
    out: &Path,
    probe: &Probe,
    window: Option<(f64, f64)>,
) -> Result<Plan, String> {
    if let Some(reason) = tools.video_codec_unavailable(options.codec) {
        return Err(reason);
    }
    let ffmpeg = tools.ffmpeg.as_ref().ok_or_else(|| "ffmpeg is not installed.".to_string())?;
    let encoder = tools
        .video_encoder(options.codec)
        .ok_or_else(|| format!("ffmpeg here has no {} encoder.", options.codec.label()))?;
    let mut spec = quiet_ffmpeg(ffmpeg);
    if let Some((start, length)) = window {
        spec = spec.arg("-ss").arg(format!("{start:.3}")).arg("-t").arg(format!("{length:.3}"));
    }
    spec = spec.arg("-i").arg(file_url(input)).args(["-map", "0:v:0"]);
    let keep_sound = probe.has_audio && matches!(options.sound, Sound::Kbps(_));
    if keep_sound {
        spec = spec.args(["-map", "0:a:0"]);
    }
    spec = if options.strip_metadata {
        spec.args(["-map_metadata", "-1", "-map_chapters", "-1"])
    } else {
        spec.args(["-map_metadata", "0"])
    };
    spec = spec
        .args(["-c:v", encoder, "-crf"])
        .arg(crf_of(options.codec, options.quality).to_string())
        .args(speed_args(options.codec, encoder, options.quality))
        .args(["-pix_fmt", "yuv420p"]);
    if options.codec == VideoCodec::H265 {
        // Apple's players only open H.265 in MP4 under this tag.
        spec = spec.args(["-tag:v", "hvc1"]);
    }
    if let Some(filters) = video_filters(options, probe) {
        spec = spec.arg("-vf").arg(filters);
    }
    spec = match options.sound {
        Sound::Kbps(kbps) if keep_sound => {
            let sound = tools
                .sound_encoder(options.codec)
                .ok_or_else(|| "ffmpeg here has no encoder for the sound.".to_string())?;
            spec.args(["-c:a", sound, "-b:a"]).arg(format!("{kbps}k"))
        }
        _ => spec.arg("-an"),
    };
    if options.codec != VideoCodec::Vp9 {
        spec = spec.args(["-movflags", "+faststart"]);
    }
    spec = spec.arg(out);
    let duration = window.map(|(_, length)| length).or(probe.duration);
    Ok(Plan {
        spec,
        extension: options.codec.extension().to_string(),
        duration,
    })
}

/// How to shrink one song into `out`.
pub fn audio_plan(
    tools: &Tools,
    options: &AudioOptions,
    input: &Path,
    out: &Path,
    probe: &Probe,
    window: Option<(f64, f64)>,
) -> Result<Plan, String> {
    if let Some(reason) = tools.audio_codec_unavailable(options.codec) {
        return Err(reason);
    }
    let ffmpeg = tools.ffmpeg.as_ref().ok_or_else(|| "ffmpeg is not installed.".to_string())?;
    let mut spec = quiet_ffmpeg(ffmpeg);
    if let Some((start, length)) = window {
        spec = spec.arg("-ss").arg(format!("{start:.3}")).arg("-t").arg(format!("{length:.3}"));
    }
    spec = spec.arg("-i").arg(file_url(input)).args(["-map", "0:a:0"]);
    // A cover picture rides along where the container has room for one.
    let cover = matches!(options.codec, AudioCodec::Mp3 | AudioCodec::Aac | AudioCodec::Flac)
        && probe.has_video
        && !options.strip_metadata;
    if cover {
        spec = spec.args(["-map", "0:v:0", "-c:v", "copy", "-disposition:v", "attached_pic"]);
    }
    spec = if options.strip_metadata {
        spec.args(["-map_metadata", "-1"])
    } else {
        spec.args(["-map_metadata", "0"])
    };
    spec = spec.args(["-c:a", options.codec.encoder()]);
    spec = match options.codec {
        AudioCodec::Flac => spec.args(["-compression_level", "8"]),
        _ => spec.arg("-b:a").arg(format!("{}k", options.kbps)),
    };
    if options.codec == AudioCodec::Aac {
        spec = spec.args(["-movflags", "+faststart"]);
    }
    spec = spec.arg(out);
    Ok(Plan {
        spec,
        extension: options.codec.extension().to_string(),
        duration: window.map(|(_, length)| length).or(probe.duration),
    })
}

// ---------------------------------------------------------------------------
// Progress and estimates
// ---------------------------------------------------------------------------

/// What one line of `ffmpeg -progress pipe:1` says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Beat {
    /// Seconds of the output written so far.
    Time(f64),
    End,
}

/// Read a line of `-progress` output. `out_time_ms` is, despite its name,
/// microseconds, as is `out_time_us`.
pub fn parse_progress(line: &[u8]) -> Option<Beat> {
    let text = std::str::from_utf8(line).ok()?.trim();
    let (key, value) = text.split_once('=')?;
    match key {
        "out_time_us" | "out_time_ms" => {
            let micros: i64 = value.trim().parse().ok()?;
            (micros >= 0).then(|| Beat::Time(micros as f64 / 1_000_000.0))
        }
        "progress" => (value.trim() == "end").then_some(Beat::End),
        _ => None,
    }
}

/// How far a film or song is, 0 to 1, from the seconds written and its length.
pub fn fraction_done(seconds: f64, duration: Option<f64>) -> Option<f32> {
    let duration = duration.filter(|d| *d > 0.0)?;
    Some((seconds / duration).clamp(0.0, 1.0) as f32)
}

/// The estimated size of every file when the first of them, `sample_in`
/// bytes, came out as `sample_out`: each file shrinks by the same ratio.
pub fn estimate_sizes(sizes: &[u64], sample_in: u64, sample_out: u64) -> Vec<u64> {
    if sample_in == 0 {
        return sizes.to_vec();
    }
    sizes
        .iter()
        .map(|size| (*size as f64 * (sample_out as f64 / sample_in as f64)).round() as u64)
        .collect()
}

/// A film's size from a sample of `seconds` that came out as `bytes`.
pub fn scale_sample(bytes: u64, seconds: f64, duration: f64) -> u64 {
    if seconds <= 0.0 {
        return bytes;
    }
    (bytes as f64 * (duration / seconds)).round() as u64
}

/// The size of a lossy song at `kbps` for `seconds`.
pub fn lossy_size(kbps: u32, seconds: f64) -> u64 {
    (f64::from(kbps) * 1000.0 / 8.0 * seconds).round() as u64
}

/// The window of a long file to encode for an estimate: up to `length`
/// seconds from the middle. `None` when the file is short enough to encode
/// whole.
pub fn sample_window(duration: f64, length: f64) -> Option<(f64, f64)> {
    if duration <= length * 1.5 {
        return None;
    }
    Some(((duration - length) / 2.0, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Tools {
        Tools {
            ffmpeg: Some(PathBuf::from("/usr/bin/ffmpeg")),
            ffprobe: Some(PathBuf::from("/usr/bin/ffprobe")),
            magick: Some(PathBuf::from("/usr/bin/magick")),
            encoders: ["libx264", "libx265", "libsvtav1", "libvpx-vp9", "aac", "libopus", "libmp3lame", "flac", "libwebp", "mjpeg", "png"]
                .map(String::from)
                .to_vec(),
            magick_formats: ["JPEG", "PNG", "WEBP"].map(String::from).to_vec(),
        }
    }

    fn args(spec: &Spec) -> Vec<String> {
        spec.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn encoders_are_read_from_the_listing() {
        let listing = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n A....D aac   AAC\n";
        assert_eq!(parse_encoders(listing), ["libx264", "aac"]);
        let formats = "   Format  Mode  Description\n  PNG* PNG       rw-   Portable\n    WEBP* WEBP      rw+   WebP\n  JNG  JNG r-- x\n";
        assert_eq!(parse_magick_formats(formats), ["PNG", "WEBP"]);
    }

    #[test]
    fn tools_say_why_a_kind_is_missing() {
        let none = Tools::default();
        assert_eq!(none.unavailable(MediaKind::Video).as_deref(), Some("ffmpeg is not installed."));
        assert!(none.unavailable(MediaKind::Image).is_some());
        assert!(tools().unavailable(MediaKind::Audio).is_none());
        let no_probe = Tools { ffprobe: None, ..tools() };
        assert_eq!(no_probe.unavailable(MediaKind::Video).as_deref(), Some("ffprobe is not installed."));
        let only_ffmpeg = Tools { magick: None, magick_formats: vec![], ..tools() };
        assert!(only_ffmpeg.unavailable(MediaKind::Image).is_none(), "ffmpeg alone can do pictures");
    }

    #[test]
    fn only_the_encoders_the_machine_has_are_offered() {
        let tools = tools();
        assert!(tools.video_codec_unavailable(VideoCodec::H264).is_none());
        let no_x265 = Tools { encoders: vec!["libx264".into(), "aac".into()], ..tools.clone() };
        assert_eq!(
            no_x265.video_codec_unavailable(VideoCodec::H265).as_deref(),
            Some("ffmpeg here has no H.265 (MP4) encoder.")
        );
        assert!(no_x265.audio_codec_unavailable(AudioCodec::Opus).is_some());
        assert!(tools.image_format_unavailable(ImageFormat::Avif).is_none());
        let no_av1 = Tools { encoders: vec!["libx264".into()], ..tools };
        assert_eq!(no_av1.image_format_unavailable(ImageFormat::Avif).as_deref(), Some("ffmpeg here has no AV1 encoder."));
    }

    #[test]
    fn picture_formats_by_name() {
        assert_eq!(image_format_of("a.JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(image_format_of("a.tiff"), Some(ImageFormat::Png));
        assert_eq!(image_format_of("a.gif"), None);
        assert_eq!(image_format_of("jpg"), None);
        let keep = ImageOptions::default();
        assert_eq!(image_extension(&keep, "x.jpeg"), Some("jpg"));
        assert_eq!(image_extension(&keep, "x.bmp"), Some("png"));
        assert_eq!(image_extension(&ImageOptions { format: ImageFormat::Avif, ..keep }, "x.png"), Some("avif"));
    }

    #[test]
    fn a_picture_by_imagemagick_reads_stdin_and_names_its_format() {
        let options = ImageOptions {
            format: ImageFormat::Webp,
            quality: 60,
            max_dimension: Some(1920),
            strip_metadata: true,
        };
        let plan = image_plan(&tools(), &options, Path::new("/p/-odd:name[0].png"), Path::new("/w/1.webp"), None).unwrap();
        let all = args(&plan.spec);
        assert_eq!(all[0], "-", "the picture is read from standard input");
        assert!(!all.iter().any(|arg| arg.contains("odd")), "its name is never an argument");
        assert!(matches!(plan.spec.input, Input::File(_)));
        assert!(all.windows(2).any(|pair| pair == ["-resize", "1920x1920>"]));
        assert!(all.windows(2).any(|pair| pair == ["+profile", "!icc,*"]), "{all:?}");
        assert!(all.windows(2).any(|pair| pair == ["-quality", "60"]));
        assert_eq!(all.last().unwrap(), "webp:/w/1.webp");
        assert_eq!(plan.extension, "webp");
        // Without stripping, the metadata is left alone; orientation is applied either way.
        let kept = image_plan(&tools(), &ImageOptions::default(), Path::new("/p/a.jpg"), Path::new("/w/1.jpg"), None).unwrap();
        let all = args(&kept.spec);
        assert!(!all.contains(&"+profile".to_string()) && all.contains(&"-auto-orient".to_string()));
        assert!(all.windows(2).any(|pair| pair == ["-quality", "75"]));
        assert!(all.contains(&"4:2:0".to_string()));
    }

    #[test]
    fn png_is_lossless_and_ignores_quality() {
        let options = ImageOptions { format: ImageFormat::Png, quality: 10, ..ImageOptions::default() };
        let plan = image_plan(&tools(), &options, Path::new("/p/a.png"), Path::new("/w/1.png"), None).unwrap();
        assert!(!args(&plan.spec).contains(&"-quality".to_string()));
        assert!(!ImageFormat::Png.has_quality() && ImageFormat::Jpeg.has_quality());
    }

    #[test]
    fn avif_goes_through_ffmpeg_with_a_file_url_and_a_crf() {
        let options = ImageOptions { format: ImageFormat::Avif, quality: 80, max_dimension: Some(1000), ..ImageOptions::default() };
        let probe = Probe { width: Some(4000), height: Some(3000), ..Probe::default() };
        let plan = image_plan(&tools(), &options, Path::new("/p/-a:b.jpg"), Path::new("/w/1.avif"), Some(&probe)).unwrap();
        let all = args(&plan.spec);
        let at = all.iter().position(|arg| arg == "-i").unwrap();
        assert_eq!(all[at + 1], "file:/p/-a:b.jpg");
        assert!(all.windows(2).any(|pair| pair == ["-vf", "scale=1000:750"]));
        assert!(all.windows(2).any(|pair| pair == ["-c:v", "libsvtav1"]));
        assert!(all.windows(2).any(|pair| pair == ["-crf", &avif_crf(80).to_string()]));
        assert!(all.windows(2).any(|pair| pair == ["-map_metadata", "0"]));
        let stripped = ImageOptions { strip_metadata: true, ..options };
        let plan = image_plan(&tools(), &stripped, Path::new("/p/a.jpg"), Path::new("/w/1.avif"), Some(&probe)).unwrap();
        assert!(args(&plan.spec).windows(2).any(|pair| pair == ["-map_metadata", "-1"]));
    }

    #[test]
    fn without_imagemagick_ffmpeg_makes_every_picture() {
        let tools = Tools { magick: None, magick_formats: vec![], ..tools() };
        let options = ImageOptions { format: ImageFormat::Jpeg, quality: 50, ..ImageOptions::default() };
        let plan = image_plan(&tools, &options, Path::new("/p/a.png"), Path::new("/w/1.jpg"), None).unwrap();
        assert!(args(&plan.spec).contains(&"mjpeg".to_string()));
        let none = Tools::default();
        assert!(image_plan(&none, &options, Path::new("/p/a.png"), Path::new("/w/1.jpg"), None).is_err());
        assert!(image_plan(&tools, &ImageOptions::default(), Path::new("/p/a.gif"), Path::new("/w/1.gif"), None).is_err());
    }

    #[test]
    fn pictures_are_fitted_never_enlarged() {
        assert_eq!(fitted(4000, 3000, 2000), (2000, 1500));
        assert_eq!(fitted(3000, 4000, 2000), (1500, 2000));
        assert_eq!(fitted(800, 600, 2000), (800, 600));
        assert_eq!(fitted(1, 5000, 100), (1, 100));
        assert_eq!(fitted(100, 100, 0), (100, 100));
    }

    #[test]
    fn avif_quality_is_a_crf_that_falls_as_quality_rises() {
        assert!(avif_crf(100) < avif_crf(75) && avif_crf(75) < avif_crf(20));
        assert!(avif_crf(1) <= 63 && avif_crf(100) >= 6);
    }

    #[test]
    fn levels_are_crfs_and_custom_crf_stays_in_range() {
        for codec in VideoCodec::ALL {
            let (low, high) = codec.crf_range();
            for level in Level::ALL {
                let crf = codec.crf_for(level);
                assert!((low..=high).contains(&crf), "{codec:?} {level:?}");
            }
            assert!(codec.crf_for(Level::Smallest) > codec.crf_for(Level::Balanced), "smaller means a higher CRF");
            assert_eq!(crf_of(codec, VideoQuality::Crf(1)), low);
            assert_eq!(crf_of(codec, VideoQuality::Crf(99)), high);
        }
    }

    #[test]
    fn a_video_is_capped_by_height_and_rate_only_when_it_is_over() {
        let probe = Probe { height: Some(2160), width: Some(3840), fps: Some(60.0), has_video: true, has_audio: true, duration: Some(10.0) };
        let options = VideoOptions { max_height: Some(1080), max_fps: Some(30), ..VideoOptions::default() };
        assert_eq!(video_filters(&options, &probe).as_deref(), Some("scale=-2:1080,fps=30"));
        let small = Probe { height: Some(720), fps: Some(25.0), ..probe.clone() };
        assert_eq!(video_filters(&options, &small), None, "never upscaled, never sped up");
        assert_eq!(video_filters(&VideoOptions::default(), &probe), None);
    }

    #[test]
    fn a_video_plan_names_its_input_as_a_url_and_keeps_or_drops_sound() {
        let probe = Probe { height: Some(1080), width: Some(1920), fps: Some(30.0), has_video: true, has_audio: true, duration: Some(10.0) };
        let plan = video_plan(&tools(), &VideoOptions::default(), Path::new("/v/-clip: 1.mp4"), Path::new("/w/1.mp4"), &probe, None).unwrap();
        let all = args(&plan.spec);
        let at = all.iter().position(|arg| arg == "-i").unwrap();
        assert_eq!(all[at + 1], "file:/v/-clip: 1.mp4");
        assert!(all.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
        assert!(all.windows(2).any(|pair| pair == ["-crf", "24"]));
        assert!(all.windows(2).any(|pair| pair == ["-preset", "medium"]));
        assert!(all.windows(2).any(|pair| pair == ["-b:a", "128k"]));
        assert!(all.windows(2).any(|pair| pair == ["-map_metadata", "0"]));
        assert_eq!(plan.extension, "mp4");
        assert_eq!(plan.duration, Some(10.0));
        let quiet = VideoOptions { sound: Sound::Remove, strip_metadata: true, ..VideoOptions::default() };
        let plan = video_plan(&tools(), &quiet, Path::new("/v/a.mp4"), Path::new("/w/1.mp4"), &probe, None).unwrap();
        let all = args(&plan.spec);
        assert!(all.contains(&"-an".to_string()) && !all.contains(&"0:a:0".to_string()));
        assert!(all.windows(2).any(|pair| pair == ["-map_metadata", "-1"]));
    }

    #[test]
    fn each_codec_has_its_container_and_tags() {
        let probe = Probe { has_video: true, duration: Some(4.0), ..Probe::default() };
        let make = |codec| video_plan(&tools(), &VideoOptions { codec, ..VideoOptions::default() }, Path::new("/v/a.mov"), Path::new("/w/1"), &probe, None).unwrap();
        assert!(args(&make(VideoCodec::H265).spec).windows(2).any(|pair| pair == ["-tag:v", "hvc1"]));
        let vp9 = make(VideoCodec::Vp9);
        assert_eq!(vp9.extension, "webm");
        assert!(!args(&vp9.spec).contains(&"-movflags".to_string()));
        let av1 = make(VideoCodec::Av1);
        assert!(args(&av1.spec).windows(2).any(|pair| pair == ["-c:v", "libsvtav1"]));
        assert_eq!(av1.extension, "mp4");
        let missing = Tools { encoders: vec![], ..tools() };
        assert!(video_plan(&missing, &VideoOptions::default(), Path::new("/v/a.mov"), Path::new("/w/1"), &probe, None).is_err());
    }

    #[test]
    fn a_sample_window_is_cut_from_the_middle() {
        let probe = Probe { has_video: true, has_audio: true, duration: Some(100.0), ..Probe::default() };
        assert_eq!(sample_window(100.0, 4.0), Some((48.0, 4.0)));
        assert_eq!(sample_window(5.0, 4.0), None);
        let plan = video_plan(&tools(), &VideoOptions::default(), Path::new("/v/a.mp4"), Path::new("/w/1.mp4"), &probe, Some((48.0, 4.0))).unwrap();
        let all = args(&plan.spec);
        assert!(all.windows(2).any(|pair| pair == ["-ss", "48.000"]) && all.windows(2).any(|pair| pair == ["-t", "4.000"]));
        assert_eq!(plan.duration, Some(4.0));
    }

    #[test]
    fn audio_plans_pick_encoder_extension_and_bitrate() {
        let probe = Probe { has_audio: true, duration: Some(30.0), ..Probe::default() };
        for (codec, encoder, extension) in [
            (AudioCodec::Opus, "libopus", "opus"),
            (AudioCodec::Aac, "aac", "m4a"),
            (AudioCodec::Mp3, "libmp3lame", "mp3"),
            (AudioCodec::Flac, "flac", "flac"),
        ] {
            let options = AudioOptions { codec, kbps: 96, strip_metadata: false };
            let plan = audio_plan(&tools(), &options, Path::new("/a/-x.wav"), Path::new("/w/1"), &probe, None).unwrap();
            let all = args(&plan.spec);
            assert!(all.windows(2).any(|pair| pair == ["-c:a", encoder]), "{codec:?}");
            assert_eq!(plan.extension, extension);
            assert_eq!(all.contains(&"-b:a".to_string()), codec.has_bitrate());
            assert!(all.contains(&"file:/a/-x.wav".to_string()));
        }
        let covered = Probe { has_video: true, ..probe };
        let plan = audio_plan(&tools(), &AudioOptions { codec: AudioCodec::Mp3, ..AudioOptions::default() }, Path::new("/a/x.mp3"), Path::new("/w/1"), &covered, None).unwrap();
        assert!(args(&plan.spec).contains(&"attached_pic".to_string()), "the cover rides along");
        let plan = audio_plan(&tools(), &AudioOptions { codec: AudioCodec::Opus, ..AudioOptions::default() }, Path::new("/a/x.mp3"), Path::new("/w/1"), &covered, None).unwrap();
        assert!(!args(&plan.spec).contains(&"attached_pic".to_string()), "ogg has no place for one");
    }

    #[test]
    fn probe_output_is_read() {
        let text = "codec_type=video|width=640|height=360|avg_frame_rate=30/1\ncodec_type=audio|avg_frame_rate=0/0\nduration=3.000000\n";
        let probe = parse_probe(text);
        assert_eq!((probe.width, probe.height, probe.fps), (Some(640), Some(360), Some(30.0)));
        assert!(probe.has_video && probe.has_audio);
        assert_eq!(probe.duration, Some(3.0));
        let bad = parse_probe("duration=N/A\ncodec_type=video|avg_frame_rate=0/0\n");
        assert_eq!((bad.duration, bad.fps), (None, None));
    }

    #[test]
    fn progress_lines_read_as_seconds_and_the_end() {
        assert_eq!(parse_progress(b"out_time_us=2933333"), Some(Beat::Time(2.933333)));
        assert_eq!(parse_progress(b"out_time_ms=1500000"), Some(Beat::Time(1.5)));
        assert_eq!(parse_progress(b"progress=end"), Some(Beat::End));
        assert_eq!(parse_progress(b"progress=continue"), None);
        assert_eq!(parse_progress(b"out_time_us=N/A"), None);
        assert_eq!(parse_progress(b"out_time_us=-9223372036854775807"), None);
        assert_eq!(parse_progress(b"fps=0.00"), None);
        assert_eq!(fraction_done(1.5, Some(3.0)), Some(0.5));
        assert_eq!(fraction_done(9.0, Some(3.0)), Some(1.0));
        assert_eq!(fraction_done(1.0, None), None);
        assert_eq!(fraction_done(1.0, Some(0.0)), None);
    }

    #[test]
    fn estimates_scale_by_the_first_files_ratio() {
        assert_eq!(estimate_sizes(&[1000, 500], 1000, 250), [250, 125]);
        assert_eq!(estimate_sizes(&[10], 0, 0), [10]);
        assert_eq!(scale_sample(100, 4.0, 40.0), 1000);
        assert_eq!(lossy_size(128, 10.0), 160_000);
    }

    #[test]
    fn reduced_names_sit_beside_the_original_and_never_collide() {
        assert_eq!(reduced_name("holiday", "jpg", 1), "holiday (compressed).jpg");
        assert_eq!(reduced_name("holiday", "jpg", 3), "holiday (compressed 3).jpg");
        let taken = |name: &str| name == "a (compressed).mp4" || name == "a (compressed 2).mp4";
        assert_eq!(free_reduced_name("a", "mp4", taken), "a (compressed 3).mp4");
        assert_eq!(split_name("a.b.jpg"), ("a.b", "jpg"));
        assert_eq!(split_name(".hidden"), (".hidden", ""));
        assert_eq!(split_name("noext"), ("noext", ""));
        assert_eq!(split_name("trailing."), ("trailing.", ""));
    }

    #[test]
    fn kinds_come_from_categories() {
        assert_eq!(MediaKind::of(Category::Image), Some(MediaKind::Image));
        assert_eq!(MediaKind::of(Category::Video), Some(MediaKind::Video));
        assert_eq!(MediaKind::of(Category::Audio), Some(MediaKind::Audio));
        assert_eq!(MediaKind::of(Category::Archive), None);
        assert_eq!(MediaKind::of(Category::Folder), None);
    }

    #[test]
    fn keys_round_trip() {
        for format in ImageFormat::ALL {
            assert_eq!(ImageFormat::from_key(format.key()), Some(format));
        }
        for codec in VideoCodec::ALL {
            assert_eq!(VideoCodec::from_key(codec.key()), Some(codec));
        }
        for codec in AudioCodec::ALL {
            assert_eq!(AudioCodec::from_key(codec.key()), Some(codec));
        }
    }
}
