//! What a source is: the first bytes decide, the name settles what they cannot.
//!
//! The name alone (`bingux_files::mime::for_name`) is what a folder listing
//! can afford. A preview opens the file anyway, so it reads the first 512
//! bytes too, and trusts them first: a PNG called `notes.txt` is a picture, a
//! file called `data.bin7` that is all text is text. The name still decides
//! inside a container family (a `.docx` is a zip that is a document; a
//! `.m4a` and an `.mp4` share a signature), and when nothing is recognised.
//!
//! [`sniff_head`] is the pure rule; [`Sniffed::of`] reads the head and applies
//! it.

use bingux_files::mime::{self, Category, Type};

use crate::source::{PreviewSource, read_at};

/// How many bytes of the head the rules look at.
pub const HEAD: usize = 512;

/// What a source holds, as far as showing it is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    Folder,
    /// A raster picture.
    Image,
    /// A vector picture, which is also text.
    Svg,
    Audio,
    Video,
    Pdf,
    /// An office document or a book: a container that is not an archive to a
    /// person.
    Document,
    Text,
    /// A zip, a tar, a compressed tar.
    Archive,
    Font,
    /// Bytes nothing recognises and that are not text.
    Binary,
    /// A socket, a device: nothing to read.
    Special,
}

/// A source after sniffing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sniffed {
    /// The type to name it by, corrected by what the bytes say.
    pub mime: Type,
    pub content: Content,
}

impl Sniffed {
    /// Read the head of `source` and decide. Blocks on the disk.
    pub fn of(source: &PreviewSource) -> std::io::Result<Sniffed> {
        let name = source.name();
        if source.is_dir() {
            return Ok(Sniffed {
                mime: mime::DIRECTORY,
                content: Content::Folder,
            });
        }
        if let PreviewSource::Path(path) = source {
            let meta = std::fs::metadata(path)?;
            if !meta.is_file() {
                return Ok(Sniffed {
                    mime: mime::for_name(&name),
                    content: Content::Special,
                });
            }
        }
        let mut reader = source.open()?;
        let head = read_at(&mut *reader, 0, HEAD)?;
        // Tar keeps its signature at byte 257, past the head of most formats.
        Ok(sniff_head(&name, &head))
    }
}

/// The rule: `name` and the first bytes of the file, `head`.
pub fn sniff_head(name: &str, head: &[u8]) -> Sniffed {
    let by_name = mime::for_name(name);
    let unknown = by_name.mime == mime::UNKNOWN.mime;
    let named = by_name.category;
    let magic = magic(head);

    let content = match magic {
        Some(Magic::Image) => Content::Image,
        Some(Magic::Pdf) => Content::Pdf,
        Some(Magic::Font) => Content::Font,
        Some(Magic::Archive) => match named {
            // A zip that a name calls a document or a book.
            Category::Document => Content::Document,
            _ => Content::Archive,
        },
        Some(Magic::Av(av)) => match named {
            Category::Audio => Content::Audio,
            Category::Video => Content::Video,
            _ => av,
        },
        Some(Magic::Executable) => Content::Binary,
        None => by_name_content(name, by_name, unknown, head),
    };
    // A name that disagrees with the bytes is not shown as the name's type.
    let mime = match (magic, content) {
        (Some(_), content) if category_of(content) != Some(named) && !unknown => canonical(content).unwrap_or(by_name),
        (Some(_), content) if unknown => canonical(content).unwrap_or(by_name),
        _ => by_name,
    };
    Sniffed { mime, content }
}

fn by_name_content(name: &str, by_name: Type, unknown: bool, head: &[u8]) -> Content {
    let text = looks_like_text(head);
    match by_name.category {
        Category::Folder => Content::Folder,
        Category::Image => {
            if by_name.mime == "image/svg+xml" {
                if head_contains(head, b"<svg") || text { Content::Svg } else { Content::Binary }
            } else {
                Content::Image
            }
        }
        Category::Audio => Content::Audio,
        Category::Video => Content::Video,
        Category::Archive => Content::Archive,
        Category::Document => {
            if by_name.mime == "application/pdf" {
                Content::Pdf
            } else {
                Content::Document
            }
        }
        Category::Text | Category::Code => {
            if text { Content::Text } else { Content::Binary }
        }
        Category::Other => {
            if is_font_name(name) {
                Content::Font
            } else if text && (unknown || by_name.mime.starts_with("text/")) {
                Content::Text
            } else {
                Content::Binary
            }
        }
    }
}

fn is_font_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["ttf", "otf", "ttc", "woff", "woff2"]
        .iter()
        .any(|extension| lower.rsplit_once('.').is_some_and(|(_, found)| found == *extension))
}

fn category_of(content: Content) -> Option<Category> {
    Some(match content {
        Content::Folder => Category::Folder,
        Content::Image | Content::Svg => Category::Image,
        Content::Audio => Category::Audio,
        Content::Video => Category::Video,
        Content::Pdf | Content::Document => Category::Document,
        Content::Text => Category::Text,
        Content::Archive => Category::Archive,
        Content::Font | Content::Binary | Content::Special => return None,
    })
}

/// A representative type for content the name did not name.
fn canonical(content: Content) -> Option<Type> {
    let name = match content {
        Content::Image => "x.png",
        Content::Pdf => "x.pdf",
        Content::Audio => "x.mp3",
        Content::Video => "x.mp4",
        Content::Archive => "x.zip",
        Content::Text => "x.txt",
        _ => return None,
    };
    Some(mime::for_name(name))
}

enum Magic {
    Image,
    Pdf,
    Font,
    Archive,
    Av(Content),
    Executable,
}

fn starts(head: &[u8], at: usize, bytes: &[u8]) -> bool {
    head.get(at..at + bytes.len()) == Some(bytes)
}

fn magic(head: &[u8]) -> Option<Magic> {
    use Magic::*;
    if starts(head, 0, b"\x89PNG\r\n\x1a\n")
        || starts(head, 0, &[0xFF, 0xD8, 0xFF])
        || starts(head, 0, b"GIF87a")
        || starts(head, 0, b"GIF89a")
        || (starts(head, 0, b"RIFF") && starts(head, 8, b"WEBP"))
        || starts(head, 0, b"II*\0")
        || starts(head, 0, b"MM\0*")
    {
        return Some(Image);
    }
    if starts(head, 0, b"%PDF-") {
        return Some(Pdf);
    }
    if starts(head, 0, b"PK\x03\x04") || starts(head, 0, b"PK\x05\x06") {
        return Some(Archive);
    }
    if starts(head, 0, &[0x1F, 0x8B])
        || starts(head, 0, b"BZh")
        || starts(head, 0, &[0xFD, b'7', b'z', b'X', b'Z', 0])
        || starts(head, 0, &[0x28, 0xB5, 0x2F, 0xFD])
        || starts(head, 0, &[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C])
        || starts(head, 0, b"Rar!")
        || starts(head, 257, b"ustar")
    {
        return Some(Archive);
    }
    if starts(head, 0, b"\0\x01\0\0") || starts(head, 0, b"true") || starts(head, 0, b"OTTO") || starts(head, 0, b"ttcf") || starts(head, 0, b"wOFF") || starts(head, 0, b"wOF2") {
        return Some(Font);
    }
    if starts(head, 0, b"\x7fELF") {
        return Some(Executable);
    }
    if starts(head, 0, b"OggS") || starts(head, 0, b"fLaC") || (starts(head, 0, b"RIFF") && starts(head, 8, b"WAVE")) || starts(head, 0, b"ID3") || (head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 && !matches!(head[1], 0xFE | 0xFF)) {
        return Some(Av(Content::Audio));
    }
    if starts(head, 4, b"ftyp") {
        return Some(match head.get(8..12) {
            Some(b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" | b"avif" | b"avis") => Image,
            Some(b"M4A " | b"M4B " | b"M4P ") => Av(Content::Audio),
            _ => Av(Content::Video),
        });
    }
    if starts(head, 0, &[0x1A, 0x45, 0xDF, 0xA3]) || (starts(head, 0, b"RIFF") && starts(head, 8, b"AVI ")) {
        return Some(Av(Content::Video));
    }
    None
}

fn head_contains(head: &[u8], needle: &[u8]) -> bool {
    head.windows(needle.len()).any(|window| window == needle)
}

/// Whether `head` could be the start of a text file: a byte order mark of
/// UTF-8 or UTF-16, or no NUL and few control bytes.
pub fn looks_like_text(head: &[u8]) -> bool {
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) || head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    if head.contains(&0) {
        return false;
    }
    let controls = head
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B) || **byte == 0x7F)
        .count();
    controls * 20 <= head.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn the_bytes_beat_a_wrong_name() {
        let sniffed = sniff_head("notes.txt", PNG);
        assert_eq!(sniffed.content, Content::Image);
        assert_eq!(sniffed.mime.mime, "image/png", "named by what it is");
    }

    #[test]
    fn a_name_decides_inside_a_family() {
        let zip = b"PK\x03\x04\x14\0";
        assert_eq!(sniff_head("report.docx", zip).content, Content::Document);
        assert_eq!(sniff_head("backup.zip", zip).content, Content::Archive);
        assert_eq!(sniff_head("app.jar", zip).content, Content::Archive);
        let mp4 = b"\0\0\0\x18ftypisom\0\0\0\0";
        assert_eq!(sniff_head("film.mp4", mp4).content, Content::Video);
        let m4a = b"\0\0\0\x18ftypM4A \0\0\0\0";
        assert_eq!(sniff_head("song.m4a", m4a).content, Content::Audio);
        // A film whose name says audio only is still played; a name that says
        // nothing falls to what the container usually is.
        assert_eq!(sniff_head("track", m4a).content, Content::Audio);
        assert_eq!(sniff_head("clip", mp4).content, Content::Video);
        let heic = b"\0\0\0\x18ftypheic\0\0\0\0";
        assert_eq!(sniff_head("IMG_1.heic", heic).content, Content::Image);
    }

    #[test]
    fn unknown_names_fall_to_text_or_binary() {
        assert_eq!(sniff_head("README", b"Read me first.\n").content, Content::Text);
        assert_eq!(sniff_head("mystery.bin7", b"plain words").content, Content::Text);
        assert_eq!(sniff_head("firmware.dat", b"\x01\x02\0\0\xff\xfe").content, Content::Binary);
        assert_eq!(sniff_head("notes.txt", b"\0\0\0\0binary").content, Content::Binary);
        assert_eq!(sniff_head("a.sh", b"#!/bin/sh\necho hi\n").content, Content::Text);
    }

    #[test]
    fn executables_are_binary_whatever_they_are_called() {
        assert_eq!(sniff_head("run.txt", b"\x7fELF\x02\x01\x01").content, Content::Binary);
    }

    #[test]
    fn pdf_svg_fonts_and_archives() {
        assert_eq!(sniff_head("a.pdf", b"%PDF-1.7\n").content, Content::Pdf);
        assert_eq!(sniff_head("logo.svg", b"<svg xmlns=\"x\"/>").content, Content::Svg);
        assert_eq!(sniff_head("logo.svg", b"\0\0\0").content, Content::Binary);
        assert_eq!(sniff_head("f.ttf", b"\0\x01\0\0\0\x0c").content, Content::Font);
        assert_eq!(sniff_head("x.ttf", b"\0\x01\0\0").mime.category, Category::Other);
        assert_eq!(sniff_head("a.tar.gz", &[0x1F, 0x8B, 8]).content, Content::Archive);
        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert_eq!(sniff_head("a.tar", &tar).content, Content::Archive);
    }

    #[test]
    fn utf16_with_a_mark_is_text() {
        assert!(looks_like_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]));
        assert!(!looks_like_text(&[b'h', 0, b'i', 0]));
    }
}
