//! Reading and writing a file's bytes: its character encoding, byte order
//! mark and line endings.
//!
//! Inside the editor text is UTF-8 with `\n` line breaks. What the file had
//! is kept in a [`FileFormat`] and written back on save, so opening and
//! saving a file changes only what was edited.
//!
//! Detection, in order:
//!
//! 1. A byte order mark names UTF-8, UTF-16 LE or UTF-16 BE.
//! 2. Valid UTF-8 is UTF-8.
//! 3. UTF-8 with stray bytes of a legacy encoding (a file that two programs
//!    wrote, a log with a Latin-1 line in it) is read as UTF-8 with each
//!    stray byte read in the legacy encoding `chardetng` guesses from those
//!    bytes. The result is reported as mixed: saving writes UTF-8.
//! 4. Anything else is decoded in the encoding `chardetng` guesses.
//!
//! A file with NUL bytes in its first 64 KiB that is not UTF-16 is binary.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

/// How a file ends its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    /// The short name the status bar shows.
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }

    /// The longer name a menu shows.
    pub fn describe(self) -> &'static str {
        match self {
            LineEnding::Lf => "Unix and macOS (LF)",
            LineEnding::CrLf => "Windows (CRLF)",
            LineEnding::Cr => "Classic Mac OS (CR)",
        }
    }

    pub const ALL: [LineEnding; 3] = [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr];
}

/// What a file's bytes look like apart from its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileFormat {
    pub encoding: &'static Encoding,
    pub bom: bool,
    pub line_ending: LineEnding,
}

impl Default for FileFormat {
    fn default() -> Self {
        Self {
            encoding: UTF_8,
            bom: false,
            line_ending: LineEnding::Lf,
        }
    }
}

impl FileFormat {
    /// The status bar's name for the encoding: `UTF-8`, `UTF-16 LE`,
    /// `Windows-1252`, with `with BOM` for a UTF-8 file that has one.
    pub fn encoding_label(&self) -> String {
        let name = encoding_label(self.encoding);
        if self.bom && self.encoding == UTF_8 {
            format!("{name} with BOM")
        } else {
            name
        }
    }
}

/// A readable name for an encoding.
pub fn encoding_label(encoding: &'static Encoding) -> String {
    match encoding.name() {
        "UTF-16LE" => "UTF-16 LE".into(),
        "UTF-16BE" => "UTF-16 BE".into(),
        "windows-874" => "Windows-874 (Thai)".into(),
        name if name.starts_with("windows-") => format!("Windows-{}", &name[8..]),
        "macintosh" => "Mac Roman".into(),
        "x-mac-cyrillic" => "Mac Cyrillic".into(),
        "gbk" => "GBK".into(),
        "x-user-defined" => "User defined".into(),
        name => name.to_owned(),
    }
}

/// The encodings a person can reopen or save a file in, grouped by script,
/// in the order a menu lists them.
pub fn encodings() -> Vec<(&'static str, Vec<&'static Encoding>)> {
    use encoding_rs::*;
    vec![
        ("Unicode", vec![UTF_8, UTF_16LE, UTF_16BE]),
        (
            "Western",
            vec![WINDOWS_1252, ISO_8859_15, MACINTOSH, IBM866],
        ),
        ("Central European", vec![WINDOWS_1250, ISO_8859_2]),
        ("Baltic", vec![WINDOWS_1257, ISO_8859_4, ISO_8859_13]),
        (
            "Cyrillic",
            vec![WINDOWS_1251, KOI8_R, KOI8_U, ISO_8859_5, X_MAC_CYRILLIC],
        ),
        ("Greek", vec![WINDOWS_1253, ISO_8859_7]),
        ("Turkish", vec![WINDOWS_1254]),
        ("Hebrew", vec![WINDOWS_1255, ISO_8859_8]),
        ("Arabic", vec![WINDOWS_1256, ISO_8859_6]),
        ("Vietnamese", vec![WINDOWS_1258]),
        ("Thai", vec![WINDOWS_874]),
        ("Chinese", vec![GB18030, GBK, BIG5]),
        ("Japanese", vec![SHIFT_JIS, EUC_JP, ISO_2022_JP]),
        ("Korean", vec![EUC_KR]),
        (
            "Other",
            vec![ISO_8859_3, ISO_8859_10, ISO_8859_14, ISO_8859_16],
        ),
    ]
}

/// What decoding found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// The text with `\n` line breaks and no byte order mark.
    pub text: String,
    pub format: FileFormat,
    /// Bytes that are not valid in the encoding were replaced with U+FFFD.
    /// Saving would write the replacement, not the bytes.
    pub lossy: bool,
    /// UTF-8 with stray bytes of this legacy encoding, read as that.
    pub mixed: Option<&'static Encoding>,
    /// The file had more than one kind of line break; all are `\n` now and
    /// saving writes `format.line_ending` everywhere.
    pub mixed_line_endings: bool,
}

/// True when `bytes` looks like a binary file rather than text.
pub fn looks_binary(bytes: &[u8]) -> bool {
    if bom_of(bytes).is_some() {
        return false;
    }
    let head = &bytes[..bytes.len().min(64 * 1024)];
    if !head.contains(&0) {
        return false;
    }
    utf16_guess(head).is_none()
}

/// [`decode`] for bytes read from a file: valid UTF-8, the common case, is
/// taken over without a copy.
pub fn decode_vec(bytes: Vec<u8>) -> Decoded {
    if bom_of(&bytes).is_some() {
        return decode(&bytes);
    }
    match String::from_utf8(bytes) {
        Ok(text) => finish(text, FileFormat::default(), false, None),
        Err(error) => decode(error.as_bytes()),
    }
}

/// Decodes a file, finding its encoding.
pub fn decode(bytes: &[u8]) -> Decoded {
    if let Some((encoding, bom_len)) = bom_of(bytes) {
        return decode_as(&bytes[bom_len..], encoding, true);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return finish(text.to_owned(), FileFormat::default(), false, None);
    }
    if let Some(encoding) = utf16_guess(&bytes[..bytes.len().min(64 * 1024)]) {
        return decode_as(bytes, encoding, false);
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    // A legacy multi-byte encoding (Shift_JIS, GBK, Big5, EUC) makes stray
    // pairs that can look like UTF-8 by chance: trust the guess.
    if (encoding.is_single_byte() || encoding == UTF_8)
        && let Some(decoded) = decode_mixed(bytes)
    {
        return decoded;
    }
    decode_as(bytes, encoding, false)
}

/// Decodes `bytes` (without a byte order mark) in `encoding`: what Reopen
/// with Encoding does. `bom` is whether the file had one.
pub fn decode_as(bytes: &[u8], encoding: &'static Encoding, bom: bool) -> Decoded {
    let bytes = match bom_of(bytes) {
        Some((found, len)) if found == encoding => &bytes[len..],
        _ => bytes,
    };
    let (text, lossy) = encoding.decode_without_bom_handling(bytes);
    let format = FileFormat {
        encoding: output_encoding(encoding),
        bom,
        line_ending: LineEnding::Lf,
    };
    finish(text.into_owned(), format, lossy, None)
}

/// The encoding that is written for `encoding`. encoding_rs decodes a few
/// labels it cannot encode (ISO-2022-JP and the UTF-16 family encode as
/// UTF-8 there); this module writes UTF-16 itself, so only the rest map.
fn output_encoding(encoding: &'static Encoding) -> &'static Encoding {
    if encoding == UTF_16LE || encoding == UTF_16BE {
        encoding
    } else {
        encoding.output_encoding()
    }
}

fn bom_of(bytes: &[u8]) -> Option<(&'static Encoding, usize)> {
    Encoding::for_bom(bytes)
}

/// UTF-16 without a byte order mark: many NULs on one side of each pair.
fn utf16_guess(bytes: &[u8]) -> Option<&'static Encoding> {
    if bytes.len() < 4 {
        return None;
    }
    let pairs = bytes.len() / 2;
    let even = bytes.iter().step_by(2).filter(|byte| **byte == 0).count();
    let odd = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    if odd * 10 > pairs * 4 && even * 20 < pairs {
        Some(UTF_16LE)
    } else if even * 10 > pairs * 4 && odd * 20 < pairs {
        Some(UTF_16BE)
    } else {
        None
    }
}

/// UTF-8 with stray legacy bytes. Needs at least one valid multi-byte UTF-8
/// sequence, so a plain Latin-1 file is not taken for it.
fn decode_mixed(bytes: &[u8]) -> Option<Decoded> {
    let mut stray = Vec::new();
    let mut multibyte = 0usize;
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                multibyte += valid.bytes().filter(|byte| *byte >= 0xc0).count();
                break;
            }
            Err(error) => {
                let (valid, after) = rest.split_at(error.valid_up_to());
                multibyte += valid.iter().filter(|byte| **byte >= 0xc0).count();
                let bad = error.error_len().unwrap_or(after.len()).max(1);
                stray.extend_from_slice(&after[..bad.min(after.len())]);
                rest = &after[bad.min(after.len())..];
            }
        }
    }
    // Mixed means mostly UTF-8: at least as many valid multi-byte sequences
    // as stray bytes.
    if multibyte == 0 || stray.is_empty() || multibyte < stray.len() {
        return None;
    }
    const MIXED_GUESS_MIN: usize = 64;
    // A handful of stray bytes says too little to tell scripts apart (two
    // accented letters read as Hebrew as readily as French), and Latin-1 is
    // by far the commonest stray; a longer run is guessed from its bytes.
    let legacy = if stray.len() < MIXED_GUESS_MIN {
        WINDOWS_1252
    } else {
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(&stray, true);
        detector.guess(None, true)
    };
    // Single-byte encodings only: a stray byte of a multi-byte encoding has
    // no meaning on its own.
    let legacy = if legacy.is_single_byte() && legacy != UTF_8 {
        legacy
    } else {
        WINDOWS_1252
    };
    let mut text = String::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                text.push_str(valid);
                break;
            }
            Err(error) => {
                let (valid, after) = rest.split_at(error.valid_up_to());
                // SAFETY-free: `valid` was checked by from_utf8 above.
                text.push_str(std::str::from_utf8(valid).unwrap_or_default());
                let bad = error
                    .error_len()
                    .unwrap_or(after.len())
                    .max(1)
                    .min(after.len());
                let (decoded, _) = legacy.decode_without_bom_handling(&after[..bad]);
                text.push_str(&decoded);
                rest = &after[bad..];
            }
        }
    }
    let format = FileFormat::default();
    Some(finish(text, format, false, Some(legacy)))
}

/// Finds the line endings and makes them all `\n`.
fn finish(
    mut text: String,
    mut format: FileFormat,
    lossy: bool,
    mixed: Option<&'static Encoding>,
) -> Decoded {
    let (crlf, lf, cr) = count_line_endings(&text);
    format.line_ending = if crlf >= lf && crlf >= cr && crlf > 0 {
        LineEnding::CrLf
    } else if cr > lf && cr > crlf {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    };
    let kinds = [crlf, lf, cr].iter().filter(|count| **count > 0).count();
    if crlf > 0 || cr > 0 {
        text = normalise_line_endings(&text);
    }
    Decoded {
        text,
        format,
        lossy,
        mixed,
        mixed_line_endings: kinds > 1,
    }
}

/// Counts `\r\n`, lone `\n` and lone `\r`.
pub fn count_line_endings(text: &str) -> (usize, usize, usize) {
    let bytes = text.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let mut index = 0;
    while let Some(offset) = memchr::memchr2(b'\r', b'\n', &bytes[index..]) {
        let at = index + offset;
        if bytes[at] == b'\r' {
            if bytes.get(at + 1) == Some(&b'\n') {
                crlf += 1;
                index = at + 2;
                continue;
            }
            cr += 1;
        } else {
            lf += 1;
        }
        index = at + 1;
    }
    (crlf, lf, cr)
}

/// `\r\n` and lone `\r` become `\n`.
pub fn normalise_line_endings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Encoding failed: the text has characters the encoding cannot hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmappable {
    /// How many characters could not be written.
    pub count: usize,
    /// The first of them.
    pub first: char,
}

/// The bytes of `text` (with `\n` line breaks) in `format`.
pub fn encode(text: &str, format: &FileFormat) -> Result<Vec<u8>, Unmappable> {
    let text = if format.line_ending == LineEnding::Lf {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(text.replace('\n', format.line_ending.as_str()))
    };
    let encoding = format.encoding;
    if encoding == UTF_16LE || encoding == UTF_16BE {
        let little = encoding == UTF_16LE;
        let mut out = Vec::with_capacity(text.len() * 2 + 2);
        if format.bom {
            out.extend_from_slice(if little { &[0xff, 0xfe] } else { &[0xfe, 0xff] });
        }
        for unit in text.encode_utf16() {
            out.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        return Ok(out);
    }
    if encoding == UTF_8 {
        let mut out = Vec::with_capacity(text.len() + 3);
        if format.bom {
            out.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        out.extend_from_slice(text.as_bytes());
        return Ok(out);
    }
    let (bytes, _, had_errors) = encoding.encode(&text);
    if had_errors {
        let mut count = 0;
        let mut first = None;
        let mut buffer = [0u8; 4];
        for ch in text.chars() {
            let (_, _, bad) = encoding.encode(ch.encode_utf8(&mut buffer));
            if bad {
                count += 1;
                first.get_or_insert(ch);
            }
        }
        return Err(Unmappable {
            count,
            first: first.unwrap_or('\u{fffd}'),
        });
    }
    Ok(bytes.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_with_crlf_round_trips() {
        let bytes = b"one\r\ntwo\r\n";
        let decoded = decode(bytes);
        assert_eq!(decoded.text, "one\ntwo\n");
        assert_eq!(decoded.format.line_ending, LineEnding::CrLf);
        assert_eq!(encode(&decoded.text, &decoded.format).unwrap(), bytes);
    }

    #[test]
    fn a_bom_is_kept() {
        let bytes = b"\xef\xbb\xbfhi\n";
        let decoded = decode(bytes);
        assert_eq!(decoded.text, "hi\n");
        assert!(decoded.format.bom);
        assert_eq!(encode(&decoded.text, &decoded.format).unwrap(), bytes);
    }

    #[test]
    fn utf16_with_bom_round_trips() {
        let mut bytes = vec![0xff, 0xfe];
        for unit in "héllo\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let decoded = decode(&bytes);
        assert_eq!(decoded.text, "héllo\n");
        assert_eq!(decoded.format.encoding, UTF_16LE);
        assert_eq!(encode(&decoded.text, &decoded.format).unwrap(), bytes);
        assert!(!looks_binary(&bytes));
    }

    #[test]
    fn latin1_is_found() {
        let bytes = b"Caf\xe9 cr\xe8me br\xfbl\xe9e, na\xefve fa\xe7ade\n";
        let decoded = decode(bytes);
        assert_eq!(decoded.text, "Café crème brûlée, naïve façade\n");
        assert_eq!(decoded.format.encoding, WINDOWS_1252);
        assert_eq!(encode(&decoded.text, &decoded.format).unwrap(), bytes);
    }

    #[test]
    fn a_few_stray_bytes_read_as_latin1() {
        let bytes = b"caf\xc3\xa9 written as UTF-8\nand caf\xe9 written as Latin-1\nna\xc3\xafve na\xefve\r\nend\n";
        let decoded = decode(bytes);
        assert_eq!(
            decoded.text,
            "café written as UTF-8\nand café written as Latin-1\nnaïve naïve\nend\n"
        );
        assert_eq!(decoded.mixed, Some(WINDOWS_1252));
        assert!(decoded.mixed_line_endings);
    }

    #[test]
    fn mixed_utf8_and_latin1_reads_both() {
        let bytes = b"caf\xc3\xa9 in UTF-8\nand caf\xe9 in Latin-1\n";
        let decoded = decode(bytes);
        assert_eq!(decoded.text, "café in UTF-8\nand café in Latin-1\n");
        assert_eq!(decoded.mixed, Some(WINDOWS_1252));
        assert_eq!(decoded.format.encoding, UTF_8);
        assert!(!decoded.lossy);
    }

    #[test]
    fn mixed_line_endings_are_reported() {
        let decoded = decode(b"a\r\nb\nc\r\n");
        assert!(decoded.mixed_line_endings);
        assert_eq!(decoded.format.line_ending, LineEnding::CrLf);
        assert_eq!(decoded.text, "a\nb\nc\n");
    }

    #[test]
    fn binary_is_found() {
        assert!(looks_binary(
            b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0\x03\0>\0"
        ));
        assert!(!looks_binary(b"plain text"));
    }

    #[test]
    fn unmappable_characters_are_counted() {
        let format = FileFormat {
            encoding: WINDOWS_1252,
            ..FileFormat::default()
        };
        let error = encode("a 日本 b", &format).unwrap_err();
        assert_eq!(error.count, 2);
        assert_eq!(error.first, '日');
    }

    #[test]
    fn shift_jis_round_trips() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS
            .encode("これは日本語のテキストです。かなり長い文章を書いています。\n");
        let decoded = decode(&bytes);
        assert_eq!(decoded.format.encoding, encoding_rs::SHIFT_JIS);
        assert_eq!(
            encode(&decoded.text, &decoded.format).unwrap(),
            bytes.into_owned()
        );
    }
}
