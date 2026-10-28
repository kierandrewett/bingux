//! A picture's size, from its header.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::exif::{Exif, parse_exif};

/// What a picture's header says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    /// Pixels, as the picture is shown: turned a quarter when its EXIF
    /// orientation says so.
    pub width: u32,
    pub height: u32,
    pub exif: Exif,
}

impl ImageInfo {
    /// "4032 × 3024".
    pub fn dimensions(&self) -> String {
        format!("{} × {}", self.width, self.height)
    }

    /// "12.2 megapixels", or `None` under a tenth of one.
    pub fn megapixels(&self) -> Option<String> {
        let mega = f64::from(self.width) * f64::from(self.height) / 1_000_000.0;
        (mega >= 0.1).then(|| format!("{mega:.1} megapixels"))
    }
}

/// The size of the picture at `path`. `None` for a format this does not
/// read, or a file that is not what its name says.
pub fn image_info(path: &Path) -> Option<ImageInfo> {
    image_info_from(File::open(path).ok()?)
}

/// [`image_info`] over any reader.
pub fn image_info_from(mut source: impl Read + Seek) -> Option<ImageInfo> {
    let mut head = [0u8; 32];
    let have = read_up_to(&mut source, &mut head)?;
    let head = &head[..have];
    let (width, height, exif) = if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        let (width, height) = (be32(head, 16)?, be32(head, 20)?);
        (width, height, Exif::default())
    } else if head.starts_with(b"GIF8") {
        (le16(head, 6)?.into(), le16(head, 8)?.into(), Exif::default())
    } else if head.starts_with(b"BM") {
        let width = le32(head, 18)? as i32;
        let height = le32(head, 22)? as i32;
        (width.unsigned_abs(), height.unsigned_abs(), Exif::default())
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        let (width, height) = webp(head)?;
        (width, height, Exif::default())
    } else if head.starts_with(&[0xFF, 0xD8]) {
        jpeg(&mut source)?
    } else {
        return None;
    };
    if width == 0 || height == 0 {
        return None;
    }
    let (width, height) = if exif.is_sideways() {
        (height, width)
    } else {
        (width, height)
    };
    Some(ImageInfo {
        width,
        height,
        exif,
    })
}

fn read_up_to(source: &mut impl Read, buffer: &mut [u8]) -> Option<usize> {
    let mut have = 0;
    while have < buffer.len() {
        match source.read(&mut buffer[have..]) {
            Ok(0) => break,
            Ok(count) => have += count,
            Err(_) => return None,
        }
    }
    Some(have)
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// WebP: the first chunk after `RIFF....WEBP` says lossy (`VP8 `), lossless
/// (`VP8L`) or extended (`VP8X`), and each stores the size its own way.
fn webp(head: &[u8]) -> Option<(u32, u32)> {
    match head.get(12..16)? {
        b"VP8 " => {
            // After the 3-byte frame tag: a start code, then 14-bit sizes.
            if head.get(23..26)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            Some((
                u32::from(le16(head, 26)? & 0x3FFF),
                u32::from(le16(head, 28)? & 0x3FFF),
            ))
        }
        b"VP8L" => {
            if *head.get(20)? != 0x2F {
                return None;
            }
            let bits = le32(head, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => {
            let canvas = |at: usize| -> Option<u32> {
                let bytes = head.get(at..at + 3)?;
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]) + 1)
            };
            Some((canvas(24)?, canvas(27)?))
        }
        _ => None,
    }
}

/// JPEG: walk the segments. The start-of-frame segment holds the size; an
/// APP1 segment that begins `Exif` holds the camera's notes.
fn jpeg(source: &mut (impl Read + Seek)) -> Option<(u32, u32, Exif)> {
    source.seek(SeekFrom::Start(2)).ok()?;
    let mut exif = Exif::default();
    // A real file has a handful of segments before the frame.
    for _ in 0..64 {
        let mut marker = [0u8; 2];
        source.read_exact(&mut marker).ok()?;
        if marker[0] != 0xFF {
            return None;
        }
        // Fill bytes between segments.
        let mut kind = marker[1];
        while kind == 0xFF {
            let mut next = [0u8; 1];
            source.read_exact(&mut next).ok()?;
            kind = next[0];
        }
        // Stand-alone markers carry no length; start of scan ends the walk.
        if kind == 0x01 || (0xD0..=0xD8).contains(&kind) {
            continue;
        }
        if kind == 0xD9 || kind == 0xDA {
            return None;
        }
        let mut length = [0u8; 2];
        source.read_exact(&mut length).ok()?;
        let length = usize::from(u16::from_be_bytes(length)).checked_sub(2)?;
        let is_frame = (0xC0..=0xCF).contains(&kind) && !matches!(kind, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            let mut frame = [0u8; 5];
            source.read_exact(&mut frame).ok()?;
            let height = u32::from(u16::from_be_bytes([frame[1], frame[2]]));
            let width = u32::from(u16::from_be_bytes([frame[3], frame[4]]));
            return Some((width, height, exif));
        }
        if kind == 0xE1 && length > 6 {
            let mut segment = vec![0u8; length];
            source.read_exact(&mut segment).ok()?;
            if let Some(tiff) = segment.strip_prefix(b"Exif\0\0")
                && let Some(found) = parse_exif(tiff)
            {
                exif = found;
            }
        } else {
            source.seek(SeekFrom::Current(length as i64)).ok()?;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn read(bytes: Vec<u8>) -> Option<ImageInfo> {
        image_info_from(Cursor::new(bytes))
    }

    #[test]
    fn png_gif_and_bmp_say_their_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(read(png).unwrap().dimensions(), "640 × 480");

        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&90u16.to_le_bytes());
        gif.extend_from_slice(&60u16.to_le_bytes());
        assert_eq!(read(gif).unwrap().dimensions(), "90 × 60");

        let mut bmp = vec![0u8; 18];
        bmp[0] = b'B';
        bmp[1] = b'M';
        bmp.extend_from_slice(&100u32.to_le_bytes());
        bmp.extend_from_slice(&(-50i32).to_le_bytes());
        assert_eq!(read(bmp).unwrap().dimensions(), "100 × 50");
    }

    #[test]
    fn webp_stores_its_size_three_ways() {
        let mut extended = b"RIFF\0\0\0\0WEBPVP8X\n\0\0\0\0\0\0\0".to_vec();
        extended.extend_from_slice(&[0x1F, 0x03, 0x00, 0xE7, 0x01, 0x00]);
        assert_eq!(read(extended).unwrap().dimensions(), "800 × 488");

        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2F".to_vec();
        let bits = 199u32 | (99u32 << 14);
        lossless.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(read(lossless).unwrap().dimensions(), "200 × 100");

        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0".to_vec();
        lossy.extend_from_slice(&[0x9D, 0x01, 0x2A]);
        lossy.extend_from_slice(&320u16.to_le_bytes());
        lossy.extend_from_slice(&240u16.to_le_bytes());
        assert_eq!(read(lossy).unwrap().dimensions(), "320 × 240");
    }

    fn jpeg_with(exif: Option<Vec<u8>>, width: u16, height: u16) -> Vec<u8> {
        let mut file = vec![0xFF, 0xD8];
        // A comment segment to skip.
        file.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x06, b'h', b'i', b'!', b'!']);
        if let Some(tiff) = exif {
            let mut segment = b"Exif\0\0".to_vec();
            segment.extend(tiff);
            file.extend_from_slice(&[0xFF, 0xE1]);
            file.extend_from_slice(&((segment.len() + 2) as u16).to_be_bytes());
            file.extend(segment);
        }
        file.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8]);
        file.extend_from_slice(&height.to_be_bytes());
        file.extend_from_slice(&width.to_be_bytes());
        file.extend_from_slice(&[1, 1, 0x11, 0]);
        file
    }

    #[test]
    fn a_jpeg_is_walked_past_other_segments() {
        let info = read(jpeg_with(None, 4000, 3000)).unwrap();
        assert_eq!(info.dimensions(), "4000 × 3000");
        assert_eq!(info.megapixels().as_deref(), Some("12.0 megapixels"));
        assert!(info.exif.is_empty());
    }

    #[test]
    fn a_turned_jpeg_swaps_its_size_and_names_the_camera() {
        let exif = super::super::exif::tests::sample();
        let info = read(jpeg_with(Some(exif), 4000, 3000)).unwrap();
        assert_eq!(info.dimensions(), "3000 × 4000", "orientation 6 is on its side");
        assert_eq!(info.exif.camera.as_deref(), Some("Canon EOS R5"));
    }

    #[test]
    fn other_files_and_cut_files_give_nothing() {
        assert_eq!(read(b"just some text, not a picture".to_vec()), None);
        assert_eq!(read(vec![0xFF, 0xD8, 0xFF]), None);
        assert_eq!(read(Vec::new()), None);
    }
}
