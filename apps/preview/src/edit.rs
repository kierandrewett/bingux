//! Turning and flipping a picture and keeping the result: Save a copy, and Copy
//! picture.
//!
//! The viewer shows a picture turned and mirrored; the file on disk is never
//! changed. A copy is made from the stored pixels with the same turn and mirror
//! applied, written beside the original under a name that says it is a copy.
//! The pictures it can write are the ones the `image` crate reads (PNG, JPEG,
//! GIF's first frame, WebP, BMP, ICO, TIFF); for the rest (SVG, HEIF, camera
//! files) the commands are not offered.
//!
//! A copy can also be cropped. The crop is a box in fractions of the picture as
//! it looks (turned and mirrored), so the person's box and the decode here
//! agree whatever a decoder's size says; it is cut after the turn.
//!
//! The orientation is a member of the eight-element group of turns and
//! mirrors: `(turns, mirrored)` means mirror left to right, then turn that
//! many quarters clockwise, the way `bingux_ui::zoom_view::View` says it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::{DynamicImage, ImageFormat, ImageReader};

/// How a picture is turned and mirrored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation {
    pub turns: u8,
    pub mirrored: bool,
}

impl Orientation {
    pub const UPRIGHT: Orientation = Orientation {
        turns: 0,
        mirrored: false,
    };

    pub fn new(turns: u8, mirrored: bool) -> Self {
        Orientation {
            turns: turns % 4,
            mirrored,
        }
    }

    /// `self` after `other`: the turn and mirror of doing `other`, then `self`.
    pub fn after(self, other: Orientation) -> Orientation {
        // M R^k = R^-k M, so a mirror in `self` reverses the turns of `other`.
        let theirs = if self.mirrored {
            (4 - other.turns) % 4
        } else {
            other.turns
        };
        Orientation::new(self.turns + theirs, self.mirrored != other.mirrored)
    }

    /// The orientation that undoes this one.
    pub fn inverse(self) -> Orientation {
        if self.mirrored {
            self
        } else {
            Orientation::new((4 - self.turns) % 4, false)
        }
    }

    /// What has to be done to a picture that is already `home` to make it look
    /// `total`.
    pub fn relative(total: Orientation, home: Orientation) -> Orientation {
        total.after(home.inverse())
    }

    pub fn is_upright(self) -> bool {
        self == Orientation::UPRIGHT
    }

    fn apply(self, picture: DynamicImage) -> DynamicImage {
        let picture = if self.mirrored {
            picture.fliph()
        } else {
            picture
        };
        match self.turns % 4 {
            1 => picture.rotate90(),
            2 => picture.rotate180(),
            3 => picture.rotate270(),
            _ => picture,
        }
    }
}

/// Whether the commands that make a copy can work on `path`.
pub fn can_copy(path: &Path) -> bool {
    path.extension()
        .and_then(ImageFormat::from_extension)
        .is_some_and(|format| format.reading_enabled())
}

/// A box in a picture as it looks: left, top, width and height as fractions
/// of its width and height.
pub type Crop = (f32, f32, f32, f32);

/// The size of the picture as it looks when the file's own orientation tag is
/// applied: what a viewer that applies it draws. Reads the header only.
pub fn upright_size(path: &Path) -> Option<(u32, u32)> {
    use image::metadata::Orientation as Tag;
    let mut decoder = ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let tag = image::ImageDecoder::orientation(&mut decoder).unwrap_or(Tag::NoTransforms);
    let (width, height) = image::ImageDecoder::dimensions(&decoder);
    let sideways = matches!(
        tag,
        Tag::Rotate90 | Tag::Rotate270 | Tag::Rotate90FlipH | Tag::Rotate270FlipH
    );
    Some(if sideways {
        (height, width)
    } else {
        (width, height)
    })
}

/// The part of `picture` that `crop` names, at least one pixel each way.
fn cut(picture: DynamicImage, crop: Crop) -> DynamicImage {
    let size = (picture.width() as f32, picture.height() as f32);
    let (left, top, width, height) = (
        crop.0 * size.0,
        crop.1 * size.1,
        crop.2 * size.0,
        crop.3 * size.1,
    );
    let (x, y, w, h) = bingux_ui::crop::CropRect {
        x: left,
        y: top,
        width,
        height,
    }
    .pixels(size);
    picture.crop_imm(x, y, w, h)
}

/// The pixels of `path` with the person's turn and mirror applied, then cut to
/// `crop`. The decoder applies the file's own orientation tag; `home` is the
/// orientation the viewer started in and `total` how the picture looks now.
fn picture(
    path: &Path,
    total: Orientation,
    home: Orientation,
    crop: Option<Crop>,
) -> Result<DynamicImage, String> {
    let mut decoder = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())?
        .into_decoder()
        .map_err(|error| error.to_string())?;
    let tag = image::ImageDecoder::orientation(&mut decoder)
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?;
    picture.apply_orientation(tag);
    let picture = Orientation::relative(total, home).apply(picture);
    Ok(match crop {
        Some(crop) => cut(picture, crop),
        None => picture,
    })
}

/// The name a copy of `path` gets: "photo edited.jpg", then "photo edited 2.jpg"
/// when that is taken.
pub fn copy_name(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "picture".to_owned());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let first = folder.join(format!("{stem} edited{extension}"));
    if !first.exists() {
        return first;
    }
    (2..1000)
        .map(|n| folder.join(format!("{stem} edited {n}{extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

/// Write a copy of `path` to `to`, turned and mirrored. The format is the
/// target's extension; a name with none is written as PNG.
pub fn save_copy(
    path: &Path,
    to: &Path,
    total: Orientation,
    home: Orientation,
    crop: Option<Crop>,
) -> Result<(), String> {
    let picture = picture(path, total, home, crop)?;
    let format = to
        .extension()
        .and_then(ImageFormat::from_extension)
        .unwrap_or(ImageFormat::Png);
    match format {
        ImageFormat::Jpeg => {
            let file = std::fs::File::create(to).map_err(|error| error.to_string())?;
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                std::io::BufWriter::new(file),
                92,
            );
            picture
                .to_rgb8()
                .write_with_encoder(encoder)
                .map_err(|error| error.to_string())
        }
        other => picture
            .save_with_format(to, other)
            .map_err(|error| error.to_string()),
    }
}

/// The bytes of a PNG of the picture as it looks.
pub fn png_bytes(
    path: &Path,
    total: Orientation,
    home: Orientation,
    crop: Option<Crop>,
) -> Result<Vec<u8>, String> {
    // A PNG nobody turned goes as it is: no re-encode, no loss.
    let untouched =
        Orientation::relative(total, home).is_upright() && home.is_upright() && crop.is_none();
    if untouched
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return std::fs::read(path).map_err(|error| error.to_string());
    }
    let picture = picture(path, total, home, crop)?;
    let mut out = Vec::new();
    picture
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(out)
}

/// Put PNG data on the clipboard as an image, with whichever clipboard tool
/// the session has (`wl-copy` on Wayland, `xclip` on X11).
pub fn copy_to_clipboard(png: &[u8]) -> Result<(), String> {
    let tools: [(&str, &[&str]); 2] = [
        ("wl-copy", &["--type", "image/png"]),
        (
            "xclip",
            &["-selection", "clipboard", "-t", "image/png", "-i"],
        ),
    ];
    let mut last = String::from("no clipboard tool");
    for (program, arguments) in tools {
        let mut child = match Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                last = format!("{program}: {error}");
                continue;
            }
        };
        let written = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(png))
            .transpose();
        // wl-copy stays running to serve the paste; do not wait for it.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        match written {
            Ok(_) => return Ok(()),
            Err(error) => last = format!("{program}: {error}"),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Orientation> {
        (0..4)
            .flat_map(|turns| [false, true].map(|mirrored| Orientation::new(turns, mirrored)))
            .collect()
    }

    #[test]
    fn the_orientations_form_a_group() {
        for a in all() {
            assert_eq!(a.after(a.inverse()), Orientation::UPRIGHT, "{a:?}");
            assert_eq!(a.inverse().after(a), Orientation::UPRIGHT, "{a:?}");
            assert_eq!(a.after(Orientation::UPRIGHT), a);
            for b in all() {
                for c in all() {
                    assert_eq!(a.after(b).after(c), a.after(b.after(c)));
                }
            }
        }
    }

    #[test]
    fn what_is_left_to_do_is_the_total_after_the_home_is_undone() {
        for home in all() {
            for total in all() {
                assert_eq!(
                    Orientation::relative(total, home).after(home),
                    total,
                    "{home:?} {total:?}"
                );
            }
        }
        assert!(
            Orientation::relative(Orientation::new(1, false), Orientation::new(1, false))
                .is_upright()
        );
    }

    /// A 3 by 2 picture whose six pixels are numbered, so a turn can be read.
    fn numbered() -> DynamicImage {
        let mut image = image::RgbaImage::new(3, 2);
        for (index, pixel) in image.pixels_mut().enumerate() {
            *pixel = image::Rgba([index as u8, 0, 0, 255]);
        }
        DynamicImage::ImageRgba8(image)
    }

    fn reds(picture: &DynamicImage) -> Vec<u8> {
        picture.to_rgba8().pixels().map(|pixel| pixel[0]).collect()
    }

    #[test]
    fn a_turn_to_the_right_is_clockwise_and_a_mirror_is_left_to_right() {
        // 0 1 2 / 3 4 5 turned a quarter clockwise is 3 0 / 4 1 / 5 2.
        assert_eq!(
            reds(&Orientation::new(1, false).apply(numbered())),
            [3, 0, 4, 1, 5, 2]
        );
        assert_eq!(
            reds(&Orientation::new(0, true).apply(numbered())),
            [2, 1, 0, 5, 4, 3]
        );
        assert_eq!(
            reds(&Orientation::new(2, false).apply(numbered())),
            [5, 4, 3, 2, 1, 0]
        );
        // Mirrored, then turned a quarter: the transverse of the picture.
        assert_eq!(
            reds(&Orientation::new(1, true).apply(numbered())),
            [5, 2, 4, 1, 3, 0]
        );
    }

    #[test]
    fn a_composed_orientation_is_the_two_applied_in_turn() {
        for a in all() {
            for b in all() {
                let in_turn = a.apply(b.apply(numbered()));
                let at_once = a.after(b).apply(numbered());
                assert_eq!(reds(&in_turn), reds(&at_once), "{a:?} after {b:?}");
            }
        }
    }

    #[test]
    fn a_copy_is_named_and_numbered_beside_the_original() {
        let dir = std::env::temp_dir().join(format!("bingux-preview-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("holiday.jpg");
        assert_eq!(copy_name(&original), dir.join("holiday edited.jpg"));
        std::fs::write(dir.join("holiday edited.jpg"), b"x").unwrap();
        assert_eq!(copy_name(&original), dir.join("holiday edited 2.jpg"));
    }

    #[test]
    fn a_copy_of_a_png_is_written_turned() {
        let dir = std::env::temp_dir().join(format!("bingux-preview-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("numbers.png");
        numbered().save(&original).unwrap();
        let copy = dir.join("copy.png");
        save_copy(
            &original,
            &copy,
            Orientation::new(1, false),
            Orientation::UPRIGHT,
            None,
        )
        .unwrap();
        let written = image::open(&copy).unwrap();
        assert_eq!((written.width(), written.height()), (2, 3));
        assert_eq!(reds(&written), [3, 0, 4, 1, 5, 2]);
        // Nothing turned and a PNG: the bytes are the file's own.
        assert_eq!(
            png_bytes(&original, Orientation::UPRIGHT, Orientation::UPRIGHT, None).unwrap(),
            std::fs::read(&original).unwrap()
        );
        assert!(
            can_copy(&original)
                && !can_copy(&dir.join("logo.svg"))
                && !can_copy(&dir.join("notes.txt"))
        );
    }

    #[test]
    fn a_crop_is_cut_from_the_picture_as_it_looks() {
        // 0 1 2 / 3 4 5, cropped to its right two columns: 1 2 / 4 5.
        let right = cut(numbered(), (1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0));
        assert_eq!((right.width(), right.height()), (2, 2));
        assert_eq!(reds(&right), [1, 2, 4, 5]);
        // The crop box is in the turned picture: turned right it is 3 0 / 4 1 / 5 2,
        // and its top row is 3 0.
        let dir = std::env::temp_dir().join(format!("bingux-preview-crop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("numbers.png");
        numbered().save(&original).unwrap();
        let turned_top = picture(
            &original,
            Orientation::new(1, false),
            Orientation::UPRIGHT,
            Some((0.0, 0.0, 1.0, 1.0 / 3.0)),
        )
        .unwrap();
        assert_eq!(reds(&turned_top), [3, 0]);
        // A box that is only a sliver is still a pixel; one off the edge is pulled in.
        let sliver = cut(numbered(), (0.999, 0.999, 0.0, 0.0));
        assert_eq!((sliver.width(), sliver.height()), (1, 1));
        let beyond = cut(numbered(), (-1.0, -1.0, 9.0, 9.0));
        assert_eq!((beyond.width(), beyond.height()), (3, 2));
        // A cropped PNG is re-encoded: it is not the file's own bytes.
        let cropped = png_bytes(
            &original,
            Orientation::UPRIGHT,
            Orientation::UPRIGHT,
            Some((0.0, 0.0, 0.5, 1.0)),
        )
        .unwrap();
        assert_ne!(cropped, std::fs::read(&original).unwrap());
    }

    #[test]
    fn the_size_of_a_sideways_photo_is_the_size_it_looks() {
        let dir = std::env::temp_dir().join(format!("bingux-preview-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plain = dir.join("plain.png");
        numbered().save(&plain).unwrap();
        assert_eq!(upright_size(&plain), Some((3, 2)));
        assert_eq!(upright_size(&dir.join("missing.png")), None);
    }
}
