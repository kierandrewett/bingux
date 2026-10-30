//! Windowed reading of text.
//!
//! A preview never reads a whole file: [`read_window`] takes `limit` bytes
//! from an offset, cuts at the last line break so the next window starts on
//! a line, and decodes them. A log of a gigabyte opens at once and the next
//! window is one more call.
//!
//! Encodings: UTF-8 (with or without a byte order mark), UTF-16 with a mark,
//! and, for a file that is not valid UTF-8 but is plainly text, Windows-1252,
//! the encoding such files nearly always are. The window says which it used,
//! so the viewer can tell the person.

use std::io::{Read, SeekFrom};

use bingux_files::Cancel;

use crate::source::{PreviewSource, ReadSeek};

/// The first window of a preview: enough to see what a file is.
pub const HEAD_LIMIT: usize = 64 * 1024;
/// The most text one preview loads over all its windows.
pub const TOTAL_LIMIT: u64 = 4 * 1024 * 1024;

/// How the bytes were read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Not UTF-8: read as Windows-1252.
    Windows1252,
}

impl Encoding {
    /// The name to show, `None` for the unremarkable UTF-8.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Encoding::Utf8 => None,
            Encoding::Utf16Le => Some("UTF-16 (little-endian)"),
            Encoding::Utf16Be => Some("UTF-16 (big-endian)"),
            Encoding::Windows1252 => Some("Windows-1252: the file is not valid UTF-8"),
        }
    }
}

/// A piece of a text file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextWindow {
    pub text: String,
    pub encoding: Encoding,
    /// Where in the file this window starts and ends, in bytes.
    pub start: u64,
    pub end: u64,
    /// The file's length, when known.
    pub total: Option<u64>,
}

impl TextWindow {
    /// The file goes on past this window.
    pub fn has_more(&self) -> bool {
        self.total.is_none_or(|total| self.end < total) && self.end < TOTAL_LIMIT
    }

    /// The file goes on but the preview has read all it will.
    pub fn is_capped(&self) -> bool {
        self.total.is_none_or(|total| self.end < total) && self.end >= TOTAL_LIMIT
    }
}

/// Why text could not be shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// It holds binary data.
    Binary,
    Cancelled,
    /// The disk said no, in words for a person.
    Unreadable(String),
}

/// A file error in words for a person.
pub fn plain(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "You do not have permission to read this file.".to_owned(),
        std::io::ErrorKind::NotFound => "This file no longer exists.".to_owned(),
        _ => "This file could not be read.".to_owned(),
    }
}

/// Read up to `limit` bytes of `source` from `offset`, cut at a line break.
/// Blocks on the disk.
pub fn read_window(source: &PreviewSource, offset: u64, limit: usize, cancel: &Cancel) -> Result<TextWindow, TextError> {
    if cancel.is_cancelled() {
        return Err(TextError::Cancelled);
    }
    let unreadable = |error: std::io::Error| TextError::Unreadable(plain(&error));
    let mut reader = source.open().map_err(unreadable)?;
    let (bytes, mark, skipped) = read_raw(&mut *reader, offset, limit).map_err(unreadable)?;
    if cancel.is_cancelled() {
        return Err(TextError::Cancelled);
    }
    let (text, used, encoding) = decode_window(&bytes, mark, limit)?;
    Ok(TextWindow {
        text,
        encoding,
        start: offset,
        end: offset.max(skipped as u64) + used as u64,
        total: source.len(),
    })
}

/// The bytes of one window, one past the limit when the file goes on, with
/// the encoding the byte order mark names. The mark is skipped at offset 0;
/// `skipped` counts it.
fn read_raw(reader: &mut dyn ReadSeek, offset: u64, limit: usize) -> std::io::Result<(Vec<u8>, Option<Encoding>, usize)> {
    let mut mark = [0u8; 3];
    reader.seek(SeekFrom::Start(0))?;
    let marked = reader.read(&mut mark)?;
    let (mark_encoding, mark_len) = match &mark[..marked] {
        [0xEF, 0xBB, 0xBF] => (Some(Encoding::Utf8), 3),
        [0xFF, 0xFE, ..] => (Some(Encoding::Utf16Le), 2),
        [0xFE, 0xFF, ..] => (Some(Encoding::Utf16Be), 2),
        _ => (None, 0),
    };
    reader.seek(SeekFrom::Start(offset.max(mark_len as u64)))?;
    let mut bytes = Vec::with_capacity(limit.min(1 << 20) + 1);
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    Ok((bytes, mark_encoding, if offset == 0 { mark_len } else { 0 }))
}

/// Decode a window of at most `limit` of `bytes` (a byte more says the file
/// goes on). Returns the text, how many bytes it used and the encoding.
fn decode_window(bytes: &[u8], mark: Option<Encoding>, limit: usize) -> Result<(String, usize, Encoding), TextError> {
    let more = bytes.len() > limit;
    let window = &bytes[..bytes.len().min(limit)];
    match mark {
        Some(encoding @ (Encoding::Utf16Le | Encoding::Utf16Be)) => {
            let used = &window[..window.len() & !1];
            let units = used.chunks_exact(2).map(|pair| {
                if encoding == Encoding::Utf16Le {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            });
            Ok((char::decode_utf16(units).map(|unit| unit.unwrap_or('\u{FFFD}')).collect(), used.len(), encoding))
        }
        _ => {
            // A window that stops short of the file ends at its last line.
            let cut = if more {
                window.iter().rposition(|byte| *byte == b'\n').map_or(window.len(), |at| at + 1)
            } else {
                window.len()
            };
            let used = &window[..cut];
            if used.contains(&0) {
                return Err(TextError::Binary);
            }
            match std::str::from_utf8(used) {
                Ok(text) => Ok((text.to_owned(), used.len(), Encoding::Utf8)),
                // A character cut by the window's edge is not damage.
                Err(error) if more && error.error_len().is_none() && error.valid_up_to() > 0 => {
                    let valid = error.valid_up_to();
                    Ok((String::from_utf8_lossy(&used[..valid]).into_owned(), valid, Encoding::Utf8))
                }
                Err(_) if plausibly_latin1(used) => Ok((used.iter().map(|byte| windows_1252(*byte)).collect(), used.len(), Encoding::Windows1252)),
                Err(_) => Err(TextError::Binary),
            }
        }
    }
}

/// Text in a single-byte encoding: few high bytes, no control bytes.
fn plausibly_latin1(bytes: &[u8]) -> bool {
    let high = bytes.iter().filter(|byte| **byte >= 0x80).count();
    let controls = bytes
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B) || **byte == 0x7F)
        .count();
    high * 10 <= bytes.len() && controls * 20 <= bytes.len()
}

/// A byte of Windows-1252 as a character.
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›',
        'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        other => other as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(bytes: &[u8], offset: u64, limit: usize) -> Result<TextWindow, TextError> {
        read_window(&PreviewSource::bytes("t.txt", bytes.to_vec()), offset, limit, &Cancel::new())
    }

    #[test]
    fn a_short_file_is_read_whole() {
        let got = window("héllo\nworld\n".as_bytes(), 0, HEAD_LIMIT).unwrap();
        assert_eq!(got.text, "héllo\nworld\n");
        assert_eq!(got.encoding, Encoding::Utf8);
        assert!(!got.has_more());
        assert_eq!(got.end, got.total.unwrap());
    }

    #[test]
    fn a_long_file_is_cut_at_a_line_and_the_next_window_continues() {
        let text: String = (0..1000).map(|n| format!("line {n}\n")).collect();
        let first = window(text.as_bytes(), 0, 1000).unwrap();
        assert!(first.has_more());
        assert!(first.text.ends_with('\n'));
        assert!(first.end <= 1000);
        let second = window(text.as_bytes(), first.end, 1000).unwrap();
        assert!(second.text.starts_with("line "), "starts on a line: {:?}", &second.text[..10]);
        assert_eq!(format!("{}{}", first.text, second.text), text[..second.end as usize]);
    }

    #[test]
    fn a_multibyte_character_on_the_edge_is_not_damage() {
        let text = "é".repeat(5000);
        let got = window(text.as_bytes(), 0, 1001).unwrap();
        assert!(got.text.chars().all(|character| character == 'é'));
        assert_eq!(got.end as usize, got.text.len());
    }

    #[test]
    fn a_file_of_exactly_the_limit_has_no_more() {
        let got = window(&vec![b'a'; 100], 0, 100).unwrap();
        assert!(!got.has_more());
        assert_eq!(got.text.len(), 100);
    }

    #[test]
    fn binary_files_are_not_text() {
        assert_eq!(window(&[0x7f, b'E', b'L', b'F', 0, 1, 2, 3], 0, 100), Err(TextError::Binary));
        let noise: Vec<u8> = (0..400u32).map(|n| 0x80 + (n % 0x40) as u8).collect();
        assert_eq!(window(&noise, 0, 1000), Err(TextError::Binary));
    }

    #[test]
    fn latin1_text_is_shown_and_named() {
        let got = window(b"caf\xe9 cr\xe8me\nlatin1 line\n", 0, 1000).unwrap();
        assert_eq!(got.text, "café crème\nlatin1 line\n");
        assert_eq!(got.encoding, Encoding::Windows1252);
        assert!(got.encoding.note().is_some());
    }

    #[test]
    fn utf16_with_a_mark_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "héllo\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let got = window(&bytes, 0, 100).unwrap();
        assert_eq!(got.text, "héllo\n");
        assert_eq!(got.encoding, Encoding::Utf16Le);
    }

    #[test]
    fn a_byte_order_mark_is_not_text() {
        let got = window(b"\xEF\xBB\xBFhello", 0, 100).unwrap();
        assert_eq!(got.text, "hello");
    }

    #[test]
    fn cancelled_and_missing() {
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(read_window(&PreviewSource::bytes("a", vec![b'x']), 0, 10, &cancel), Err(TextError::Cancelled));
        let missing = PreviewSource::path("/var/tmp/PreviewKit/not-here.txt");
        assert!(matches!(read_window(&missing, 0, 10, &Cancel::new()), Err(TextError::Unreadable(_))));
    }

    #[test]
    fn mostly_text_with_a_stray_byte_is_still_text() {
        let mut bytes = b"mostly fine text here, with one \xff stray byte".to_vec();
        bytes.extend_from_slice(b" and more text after it to keep the share low");
        let got = window(&bytes, 0, 1000).unwrap();
        assert_eq!(got.encoding, Encoding::Windows1252);
    }
}
