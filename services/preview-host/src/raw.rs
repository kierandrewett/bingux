//! The picture a camera RAW file carries for its own display.
//!
//! A RAW file is the sensor's data in a maker's format, which needs a
//! demosaicing library to show. Every camera also stores a full-size or
//! screen-size JPEG in the file (Canon CR2 and CR3, Nikon NEF, Sony ARW, Adobe
//! DNG, Olympus ORF, Panasonic RW2, Fujifilm RAF, Pentax PEF). That is the
//! picture the camera's own screen shows, and the one this finds: the largest
//! JPEG inside the file, by the size its frame header says. Thumbnails of a
//! few hundred pixels are passed over for the big one.
//!
//! This is a search for JPEG streams, not a parser of each maker's container,
//! so it covers formats it was never taught. The honest limit: a RAW file
//! with no embedded preview (some compressed formats of very new cameras) has
//! nothing to show here and the viewer says so.

/// A JPEG stream inside a larger file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Embedded {
    pub start: usize,
    pub end: usize,
    pub width: u32,
    pub height: u32,
}

impl Embedded {
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// The frame size of the JPEG that starts at `data[start..]` (which begins
/// `FF D8 FF`), and where it ends, or `None` when it is not a whole JPEG.
fn measure(data: &[u8], start: usize) -> Option<Embedded> {
    let mut at = start + 2;
    let mut size = None;
    loop {
        // A marker: FF, then the code. Fill bytes (FF FF) are skipped.
        if *data.get(at)? != 0xFF {
            return None;
        }
        while *data.get(at + 1)? == 0xFF {
            at += 1;
        }
        let code = *data.get(at + 1)?;
        at += 2;
        match code {
            // Standalone markers: no length.
            0x01 | 0xD0..=0xD7 => continue,
            0xD8 => return None,
            0xD9 => return None,
            _ => {}
        }
        let length = usize::from(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]));
        if length < 2 {
            return None;
        }
        // The start-of-frame markers carry the size: baseline, extended,
        // progressive and lossless (C0 to CF except C4, C8 and CC).
        if matches!(code, 0xC0..=0xCF) && !matches!(code, 0xC4 | 0xC8 | 0xCC) {
            let height = u32::from(u16::from_be_bytes([*data.get(at + 3)?, *data.get(at + 4)?]));
            let width = u32::from(u16::from_be_bytes([*data.get(at + 5)?, *data.get(at + 6)?]));
            size = Some((width, height));
        }
        at += length;
        if code == 0xDA {
            // The entropy-coded data: FF 00 is a stuffed byte and FF D0..D7 a
            // restart; the next other marker ends the scan. Another scan may
            // follow (progressive), so keep going until the end of the image.
            loop {
                let next = *data.get(at)?;
                at += 1;
                if next != 0xFF {
                    continue;
                }
                match *data.get(at)? {
                    0x00 | 0xD0..=0xD7 => at += 1,
                    0xFF => continue,
                    0xD9 => {
                        let (width, height) = size?;
                        return Some(Embedded {
                            start,
                            end: at + 1,
                            width,
                            height,
                        });
                    }
                    _ => {
                        // A marker that continues the headers (another scan).
                        at -= 1;
                        break;
                    }
                }
            }
        }
    }
}

/// Every whole JPEG stream in `data`, in file order.
pub fn streams(data: &[u8]) -> Vec<Embedded> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + 3 < data.len() {
        // `FF D8 FF` starts every JPEG; search for it with the standard library.
        let Some(offset) = data[at..]
            .windows(3)
            .position(|window| window == [0xFF, 0xD8, 0xFF])
        else {
            break;
        };
        let start = at + offset;
        match measure(data, start) {
            Some(stream) => {
                at = stream.end;
                found.push(stream);
            }
            None => at = start + 3,
        }
    }
    found
}

/// The largest JPEG in `data` that is at least `smallest` pixels on its long
/// side, or `None`.
pub fn largest_preview(data: &[u8], smallest: u32) -> Option<Embedded> {
    streams(data)
        .into_iter()
        .filter(|stream| stream.width.max(stream.height) >= smallest)
        .max_by_key(Embedded::pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny but complete baseline JPEG of `width` by `height`, written by hand:
    /// the headers are real, the scan is one byte of stuffed data.
    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        // A comment.
        out.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x05, b'h', b'i', b'!']);
        // SOF0: precision 8, size, one component.
        out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        // SOS: one component.
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
        // Entropy-coded data with a stuffed FF and a restart marker.
        out.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    #[test]
    fn a_jpeg_is_measured_from_its_frame_header() {
        let data = jpeg(640, 480);
        let found = streams(&data);
        assert_eq!(
            found,
            [Embedded {
                start: 0,
                end: data.len(),
                width: 640,
                height: 480
            }]
        );
    }

    #[test]
    fn the_largest_stream_of_a_file_is_the_preview() {
        let mut file = b"II*\0\x08\0\0\0 some maker's header ".to_vec();
        let thumb = jpeg(160, 120);
        let big = jpeg(5472, 3648);
        let small = jpeg(1620, 1080);
        file.extend_from_slice(&thumb);
        file.extend_from_slice(&[0, 0, 0, 0, 0]);
        file.extend_from_slice(&big);
        file.extend_from_slice(&small);
        let preview = largest_preview(&file, 1).unwrap();
        assert_eq!((preview.width, preview.height), (5472, 3648));
        assert_eq!(&file[preview.start..preview.end], &big[..]);
        // Asking for at least a long side of 6000 finds nothing.
        assert_eq!(largest_preview(&file, 6000), None);
    }

    #[test]
    fn data_that_only_looks_like_a_jpeg_is_passed_over() {
        let mut file = vec![0u8; 16];
        file.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0x00, 0x00]);
        file.extend_from_slice(&[0x01; 8]);
        assert!(streams(&file).is_empty());
        // Cut short: a JPEG with no end is not a stream.
        let mut cut = jpeg(100, 100);
        cut.truncate(cut.len() - 4);
        assert!(streams(&cut).is_empty());
        assert!(streams(&[]).is_empty());
    }
}
