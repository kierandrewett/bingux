//! Listing a zip or a tar without extracting anything.
//!
//! A zip keeps its index at the end, so the listing reads the last kilobytes
//! and the index, never an entry's data. A tar is a chain of headers with the
//! data between them; a plain tar seeks over the data, a `.tar.gz` inflates
//! it and discards it, up to [`INFLATE_LIMIT`]. Other formats (7z, RAR, xz,
//! bzip2, zstd) are [`LoadError::Unsupported`]: the card says so and offers
//! Open with.

use std::io::{self, Read, Seek, SeekFrom};

use bingux_files::Cancel;
use flate2::read::GzDecoder;

use crate::registry::LoadError;
use crate::source::{PreviewSource, ReadSeek};
use crate::text::plain;

/// The most entries listed.
pub const ENTRY_LIMIT: usize = 5000;
/// The most bytes of a zip's index read.
pub const INDEX_LIMIT: u64 = 8 * 1024 * 1024;
/// The most a `.tar.gz` inflates while it is listed.
pub const INFLATE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
}

impl ArchiveFormat {
    pub fn name(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "Zip",
            ArchiveFormat::Tar => "Tar",
            ArchiveFormat::TarGz => "Tar, gzip-compressed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// The path inside the archive, with `/` separators.
    pub path: String,
    /// The unpacked size, when the archive says it.
    pub size: Option<u64>,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveListing {
    pub format: ArchiveFormat,
    pub entries: Vec<ArchiveEntry>,
    /// There are more entries than were listed.
    pub truncated: bool,
}

impl ArchiveListing {
    /// The files (not folders) and their unpacked size, as far as it is known.
    pub fn totals(&self) -> (usize, u64) {
        let files = self.entries.iter().filter(|entry| !entry.is_dir);
        (files.clone().count(), files.filter_map(|entry| entry.size).sum())
    }
}

/// List `source`. Blocks on the disk.
pub fn list(source: &PreviewSource, cancel: &Cancel) -> Result<ArchiveListing, LoadError> {
    let unreadable = |error: io::Error| LoadError::Unreadable(plain(&error));
    let mut reader = source.open().map_err(unreadable)?;
    let mut head = [0u8; 4];
    let got = read_up_to(&mut *reader, &mut head).map_err(unreadable)?;
    reader.seek(SeekFrom::Start(0)).map_err(unreadable)?;
    match &head[..got] {
        [b'P', b'K', 3, 4] | [b'P', b'K', 5, 6] => zip(&mut *reader, cancel),
        [0x1F, 0x8B, ..] => tar(Box::new(GzDecoder::new(reader).take(INFLATE_LIMIT)), ArchiveFormat::TarGz, cancel),
        _ => tar_plain(reader, cancel),
    }
}

fn read_up_to(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..])? {
            0 => break,
            more => filled += more,
        }
    }
    Ok(filled)
}

fn le16(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize
}

fn le32(bytes: &[u8], at: usize) -> u64 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as u64
}

fn corrupt() -> LoadError {
    LoadError::Unsupported("This archive is damaged or not a kind that can be listed.".to_owned())
}

fn zip(reader: &mut dyn ReadSeek, cancel: &Cancel) -> Result<ArchiveListing, LoadError> {
    let unreadable = |error: io::Error| LoadError::Unreadable(plain(&error));
    let length = reader.seek(SeekFrom::End(0)).map_err(unreadable)?;
    let tail_len = length.min(22 + 65535);
    reader.seek(SeekFrom::Start(length - tail_len)).map_err(unreadable)?;
    let mut tail = vec![0u8; tail_len as usize];
    reader.read_exact(&mut tail).map_err(unreadable)?;
    let end = tail
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .filter(|at| tail.len() >= at + 22)
        .ok_or_else(corrupt)?;
    let count = le16(&tail, end + 10);
    let size = le32(&tail, end + 12);
    let offset = le32(&tail, end + 16);
    if offset == 0xFFFF_FFFF || size == 0xFFFF_FFFF || count == 0xFFFF {
        return Err(LoadError::Unsupported("This zip is too large to list here (Zip64).".to_owned()));
    }
    if size > INDEX_LIMIT || offset + size > length {
        return Err(corrupt());
    }
    reader.seek(SeekFrom::Start(offset)).map_err(unreadable)?;
    let mut index = vec![0u8; size as usize];
    reader.read_exact(&mut index).map_err(unreadable)?;
    let mut entries = Vec::new();
    let mut at = 0;
    let mut truncated = false;
    while at + 46 <= index.len() && &index[at..at + 4] == b"PK\x01\x02" {
        if cancel.is_cancelled() {
            return Err(LoadError::Cancelled);
        }
        let flags = le16(&index, at + 8);
        let unpacked = le32(&index, at + 24);
        let (name_len, extra_len, comment_len) = (le16(&index, at + 28), le16(&index, at + 30), le16(&index, at + 32));
        let name_at = at + 46;
        let Some(name) = index.get(name_at..name_at + name_len) else {
            return Err(corrupt());
        };
        if entries.len() == ENTRY_LIMIT {
            truncated = true;
            break;
        }
        // Names are UTF-8 when bit 11 says so; otherwise the old code page,
        // which for the ASCII names that matter reads the same.
        let name = if flags & 0x800 != 0 {
            String::from_utf8_lossy(name).into_owned()
        } else {
            name.iter().map(|byte| *byte as char).collect()
        };
        let is_dir = name.ends_with('/');
        entries.push(ArchiveEntry {
            path: name.trim_end_matches('/').to_owned(),
            size: (!is_dir && unpacked != 0xFFFF_FFFF).then_some(unpacked),
            is_dir,
        });
        at = name_at + name_len + extra_len + comment_len;
    }
    Ok(ArchiveListing {
        format: ArchiveFormat::Zip,
        entries,
        truncated,
    })
}

fn tar_plain(mut reader: Box<dyn ReadSeek>, cancel: &Cancel) -> Result<ArchiveListing, LoadError> {
    let mut block = [0u8; 512];
    let got = read_up_to(&mut *reader, &mut block).map_err(|error| LoadError::Unreadable(plain(&error)))?;
    reader.seek(SeekFrom::Start(0)).map_err(|error| LoadError::Unreadable(plain(&error)))?;
    if got < 512 || &block[257..262] != b"ustar" {
        return Err(LoadError::Unsupported("This archive is not a kind that can be listed here.".to_owned()));
    }
    tar(Box::new(reader), ArchiveFormat::Tar, cancel)
}

/// A tar over `reader`. Data between headers is discarded by reading it: a
/// plain tar is no worse than a gzip one for the files a preview meets, and
/// the 8 MiB-per-entry of seeking is not worth a second code path.
fn tar(mut reader: Box<dyn Read>, format: ArchiveFormat, cancel: &Cancel) -> Result<ArchiveListing, LoadError> {
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut long_name: Option<String> = None;
    let mut pax_path: Option<String> = None;
    let mut zero_blocks = 0;
    let mut first = true;
    loop {
        if cancel.is_cancelled() {
            return Err(LoadError::Cancelled);
        }
        let mut block = [0u8; 512];
        let got = read_up_to(&mut reader, &mut block).map_err(|_| corrupt())?;
        if got < 512 {
            break;
        }
        if block.iter().all(|byte| *byte == 0) {
            zero_blocks += 1;
            if zero_blocks == 2 {
                break;
            }
            continue;
        }
        zero_blocks = 0;
        if first && &block[257..262] != b"ustar" {
            return Err(LoadError::Unsupported("This archive is not a kind that can be listed here.".to_owned()));
        }
        first = false;
        let size = octal(&block[124..136]).ok_or_else(corrupt)?;
        let kind = block[156];
        let padded = size.div_ceil(512) * 512;
        match kind {
            b'L' | b'x' => {
                // The data is a name, or `length key=value` records.
                let mut data = vec![0u8; size.min(64 * 1024) as usize];
                reader.read_exact(&mut data).map_err(|_| corrupt())?;
                discard(&mut reader, padded - data.len() as u64)?;
                if kind == b'L' {
                    long_name = Some(cstring(&data));
                } else {
                    pax_path = pax_value(&data, "path");
                }
                continue;
            }
            b'g' => {
                discard(&mut reader, padded)?;
                continue;
            }
            _ => {}
        }
        let mut name = cstring(&block[0..100]);
        if &block[257..262] == b"ustar" {
            let prefix = cstring(&block[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        if let Some(long) = long_name.take().or_else(|| pax_path.take()) {
            name = long;
        }
        let is_dir = kind == b'5' || name.ends_with('/');
        if entries.len() == ENTRY_LIMIT {
            truncated = true;
            break;
        }
        entries.push(ArchiveEntry {
            path: name.trim_end_matches('/').to_owned(),
            size: matches!(kind, b'0' | 0).then_some(size),
            is_dir,
        });
        discard(&mut reader, if matches!(kind, b'0' | 0 | b'7') { padded } else { 0 })?;
    }
    Ok(ArchiveListing { format, entries, truncated })
}

/// Skip `count` bytes by reading them.
fn discard(reader: &mut dyn Read, count: u64) -> Result<(), LoadError> {
    let copied = io::copy(&mut reader.take(count), &mut io::sink()).map_err(|_| corrupt())?;
    if copied < count {
        // The inflate limit or a cut-off file ends the listing here.
        return Err(LoadError::Unsupported("The archive is too large to list in full.".to_owned()));
    }
    Ok(())
}

fn octal(field: &[u8]) -> Option<u64> {
    // A size over 8 GiB is stored base 256 with the high bit set.
    if field[0] & 0x80 != 0 {
        return Some(field[1..].iter().fold(0u64, |value, byte| value.wrapping_shl(8) | *byte as u64));
    }
    let text = std::str::from_utf8(field).ok()?.trim_matches(|c: char| c == '\0' || c == ' ');
    if text.is_empty() { Some(0) } else { u64::from_str_radix(text, 8).ok() }
}

fn cstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// `value` of the `key=value` record in pax data.
fn pax_value(data: &[u8], key: &str) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    text.lines().find_map(|line| {
        let record = line.split_once(' ')?.1;
        record.strip_prefix(key)?.strip_prefix('=').map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn tar_header(name: &str, kind: u8, size: usize) -> [u8; 512] {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        header[156] = kind;
        header[257..263].copy_from_slice(b"ustar\0");
        header
    }

    fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (name, data) in files {
            let kind = if name.ends_with('/') { b'5' } else { b'0' };
            bytes.extend_from_slice(&tar_header(name, kind, data.len()));
            bytes.extend_from_slice(data);
            bytes.resize(bytes.len().div_ceil(512) * 512, 0);
        }
        bytes.extend_from_slice(&[0u8; 1024]);
        bytes
    }

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut index = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&[20, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            index.extend_from_slice(b"PK\x01\x02");
            index.extend_from_slice(&[20, 0, 20, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            index.extend_from_slice(&(data.len() as u32).to_le_bytes());
            index.extend_from_slice(&(data.len() as u32).to_le_bytes());
            index.extend_from_slice(&(name.len() as u16).to_le_bytes());
            index.extend_from_slice(&[0; 2 + 2 + 2 + 2 + 4]);
            index.extend_from_slice(&offset.to_le_bytes());
            index.extend_from_slice(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend_from_slice(&index);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(index.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn list_bytes(name: &str, bytes: Vec<u8>) -> Result<ArchiveListing, LoadError> {
        list(&PreviewSource::bytes(name, bytes), &Cancel::new())
    }

    #[test]
    fn a_zip_lists_its_index() {
        let listing = list_bytes("a.zip", zip_bytes(&[("a.txt", b"hello"), ("d/", b""), ("d/b.txt", b"xy")])).unwrap();
        assert_eq!(listing.format, ArchiveFormat::Zip);
        let paths: Vec<_> = listing.entries.iter().map(|entry| (entry.path.as_str(), entry.size, entry.is_dir)).collect();
        assert_eq!(paths, [("a.txt", Some(5), false), ("d", None, true), ("d/b.txt", Some(2), false)]);
        assert_eq!(listing.totals(), (2, 7));
        assert!(!listing.truncated);
    }

    #[test]
    fn a_tar_lists_its_headers_and_skips_the_data() {
        let big = vec![7u8; 1500];
        let listing = list_bytes("a.tar", tar_bytes(&[("a.txt", b"hello"), ("d/", b""), ("d/big.bin", &big)])).unwrap();
        assert_eq!(listing.format, ArchiveFormat::Tar);
        let sizes: Vec<_> = listing.entries.iter().map(|entry| (entry.path.as_str(), entry.size)).collect();
        assert_eq!(sizes, [("a.txt", Some(5)), ("d", None), ("d/big.bin", Some(1500))]);
    }

    #[test]
    fn a_gzip_tar_is_inflated_for_the_listing() {
        let tar = tar_bytes(&[("a.txt", b"hello"), ("b.txt", b"world")]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        let listing = list_bytes("a.tar.gz", gz.finish().unwrap()).unwrap();
        assert_eq!(listing.format, ArchiveFormat::TarGz);
        assert_eq!(listing.entries.len(), 2);
    }

    #[test]
    fn long_listings_are_cut() {
        let names: Vec<String> = (0..ENTRY_LIMIT + 10).map(|n| format!("f{n}")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|name| (name.as_str(), &b""[..])).collect();
        let listing = list_bytes("a.tar", tar_bytes(&files)).unwrap();
        assert_eq!(listing.entries.len(), ENTRY_LIMIT);
        assert!(listing.truncated);
    }

    #[test]
    fn what_is_not_an_archive_is_unsupported_not_a_panic() {
        assert!(matches!(list_bytes("a.zip", b"PK\x03\x04 broken".to_vec()), Err(LoadError::Unsupported(_))));
        assert!(matches!(list_bytes("a.tar", vec![1; 700]), Err(LoadError::Unsupported(_))));
        assert!(matches!(list_bytes("a.7z", b"7z\xbc\xaf\x27\x1c".to_vec()), Err(LoadError::Unsupported(_))));
        assert!(list_bytes("a.zip", Vec::new()).is_err());
    }

    #[test]
    fn a_cancelled_listing_stops() {
        let cancel = Cancel::new();
        cancel.cancel();
        let source = PreviewSource::bytes("a.zip", zip_bytes(&[("a", b"x")]));
        assert_eq!(list(&source, &cancel), Err(LoadError::Cancelled));
    }
}
