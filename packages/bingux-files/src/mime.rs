//! The MIME database: what a file is, from its name.
//!
//! The table is a compact copy of the common part of shared-mime-info's glob
//! list, embedded in the binary. It is used instead of reading
//! `/usr/share/mime` because Bingux must not need files only some
//! distributions ship (see `AGENTS.md`), and instead of a crate such as
//! `mime_guess` or `infer` to keep the dependency list empty. Like GIO's
//! "fast content type", it looks at the name only and never opens the file,
//! so listing a folder of ten thousand files stays one `readdir` and one
//! `stat` each. Unknown names are `application/octet-stream`, or
//! `text/plain` for the few extension-less names people read as text.
//!
//! [`for_name`] is the only entry point. A later bingux-files can load the
//! system database behind it without touching any caller.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The coarse kind of a file, for icons and for "show only images".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Folder,
    Image,
    Audio,
    Video,
    /// Plain text and markup.
    Text,
    /// Office documents, PDF and books.
    Document,
    Archive,
    /// Source code and scripts.
    Code,
    Other,
}

/// A MIME type with its plain-words name and category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type {
    pub mime: &'static str,
    /// "PNG image", "Folder": what a person calls it.
    pub description: &'static str,
    pub category: Category,
}

/// The type of a folder.
pub static DIRECTORY: Type = Type {
    mime: "inode/directory",
    description: "Folder",
    category: Category::Folder,
};

/// The type of a file nothing is known about.
pub static UNKNOWN: Type = Type {
    mime: "application/octet-stream",
    description: "Unknown type",
    category: Category::Other,
};

use Category::{Archive, Audio, Code, Document, Image, Other, Text, Video};

/// Extensions (lower case, space separated), MIME type, description, category.
const TABLE: &[(&str, &str, &str, Category)] = &[
    ("png", "image/png", "PNG image", Image),
    ("jpg jpeg jpe jfif", "image/jpeg", "JPEG image", Image),
    ("webp", "image/webp", "WebP image", Image),
    ("gif", "image/gif", "GIF image", Image),
    ("bmp", "image/bmp", "BMP image", Image),
    ("svg svgz", "image/svg+xml", "SVG image", Image),
    ("avif", "image/avif", "AVIF image", Image),
    ("jxl", "image/jxl", "JPEG XL image", Image),
    ("heic heif", "image/heif", "HEIF image", Image),
    ("tif tiff", "image/tiff", "TIFF image", Image),
    ("ico", "image/vnd.microsoft.icon", "Icon", Image),
    ("xcf", "image/x-xcf", "GIMP image", Image),
    ("mp3", "audio/mpeg", "MP3 audio", Audio),
    ("ogg oga opus spx", "audio/ogg", "Ogg audio", Audio),
    ("flac", "audio/flac", "FLAC audio", Audio),
    ("wav", "audio/x-wav", "WAV audio", Audio),
    ("m4a", "audio/mp4", "MPEG-4 audio", Audio),
    ("aac", "audio/aac", "AAC audio", Audio),
    ("mid midi", "audio/midi", "MIDI audio", Audio),
    ("mp4 m4v", "video/mp4", "MPEG-4 video", Video),
    ("mkv", "video/x-matroska", "Matroska video", Video),
    ("webm", "video/webm", "WebM video", Video),
    ("avi", "video/x-msvideo", "AVI video", Video),
    ("mov qt", "video/quicktime", "QuickTime video", Video),
    ("ogv", "video/ogg", "Ogg video", Video),
    ("txt text", "text/plain", "Text document", Text),
    ("md markdown", "text/markdown", "Markdown document", Text),
    ("csv", "text/csv", "CSV spreadsheet", Text),
    ("log", "text/x-log", "Log file", Text),
    ("html htm xhtml", "text/html", "HTML document", Text),
    ("css", "text/css", "Stylesheet", Text),
    ("json", "application/json", "JSON document", Text),
    ("xml", "application/xml", "XML document", Text),
    ("yaml yml", "application/yaml", "YAML document", Text),
    ("toml", "application/toml", "TOML document", Text),
    ("conf cfg ini", "text/plain", "Configuration file", Text),
    (
        "ovpn",
        "application/x-openvpn-profile",
        "OpenVPN profile",
        Text,
    ),
    ("desktop", "application/x-desktop", "Desktop entry", Text),
    ("pdf", "application/pdf", "PDF document", Document),
    (
        "odt",
        "application/vnd.oasis.opendocument.text",
        "OpenDocument text",
        Document,
    ),
    (
        "ods",
        "application/vnd.oasis.opendocument.spreadsheet",
        "OpenDocument spreadsheet",
        Document,
    ),
    (
        "odp",
        "application/vnd.oasis.opendocument.presentation",
        "OpenDocument presentation",
        Document,
    ),
    ("doc", "application/msword", "Word document", Document),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "Word document",
        Document,
    ),
    (
        "xls",
        "application/vnd.ms-excel",
        "Excel spreadsheet",
        Document,
    ),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "Excel spreadsheet",
        Document,
    ),
    (
        "ppt",
        "application/vnd.ms-powerpoint",
        "PowerPoint presentation",
        Document,
    ),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "PowerPoint presentation",
        Document,
    ),
    ("epub", "application/epub+zip", "EPUB book", Document),
    ("rtf", "application/rtf", "Rich text document", Document),
    ("zip", "application/zip", "ZIP archive", Archive),
    ("tar", "application/x-tar", "Tar archive", Archive),
    ("gz tgz", "application/gzip", "Gzip archive", Archive),
    ("xz txz", "application/x-xz", "XZ archive", Archive),
    ("bz2 tbz2", "application/x-bzip2", "Bzip2 archive", Archive),
    ("zst", "application/zstd", "Zstandard archive", Archive),
    (
        "7z",
        "application/x-7z-compressed",
        "7-Zip archive",
        Archive,
    ),
    ("rar", "application/vnd.rar", "RAR archive", Archive),
    (
        "deb",
        "application/vnd.debian.binary-package",
        "Debian package",
        Archive,
    ),
    ("rpm", "application/x-rpm", "RPM package", Archive),
    ("iso", "application/x-iso9660-image", "Disc image", Archive),
    ("rs", "text/rust", "Rust source", Code),
    ("py", "text/x-python", "Python source", Code),
    ("js mjs", "text/javascript", "JavaScript source", Code),
    ("ts", "text/x-typescript", "TypeScript source", Code),
    ("c", "text/x-csrc", "C source", Code),
    ("h", "text/x-chdr", "C header", Code),
    ("cpp cc cxx", "text/x-c++src", "C++ source", Code),
    ("java", "text/x-java", "Java source", Code),
    ("go", "text/x-go", "Go source", Code),
    ("qml", "text/x-qml", "QML source", Code),
    ("nix", "text/x-nix", "Nix source", Code),
    ("rb", "application/x-ruby", "Ruby source", Code),
    ("php", "application/x-php", "PHP source", Code),
    ("lua", "text/x-lua", "Lua source", Code),
    ("sh bash", "application/x-shellscript", "Shell script", Code),
    ("sql", "application/sql", "SQL script", Code),
    (
        "sqlite sqlite3 db",
        "application/vnd.sqlite3",
        "SQLite database",
        Other,
    ),
    ("ttf", "font/ttf", "TrueType font", Other),
    ("otf", "font/otf", "OpenType font", Other),
    ("woff woff2", "font/woff", "Web font", Other),
    (
        "torrent",
        "application/x-bittorrent",
        "BitTorrent file",
        Other,
    ),
    ("so", "application/x-sharedlib", "Shared library", Other),
    ("appimage", "application/vnd.appimage", "AppImage", Other),
    (
        "flatpakref",
        "application/vnd.flatpak.ref",
        "Flatpak reference",
        Other,
    ),
];

/// Whole names that mean a type regardless of extension.
const NAMES: &[(&str, &str, &str, Category)] = &[
    ("makefile", "text/x-makefile", "Makefile", Code),
    ("readme", "text/plain", "Text document", Text),
    ("license", "text/plain", "Text document", Text),
    ("copying", "text/plain", "Text document", Text),
    ("dockerfile", "text/x-dockerfile", "Dockerfile", Code),
];

struct Index {
    by_extension: HashMap<&'static str, Type>,
    by_name: HashMap<&'static str, Type>,
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut by_extension = HashMap::new();
        for (extensions, mime, description, category) in TABLE {
            for extension in extensions.split(' ') {
                by_extension.insert(
                    extension,
                    Type {
                        mime,
                        description,
                        category: *category,
                    },
                );
            }
        }
        let by_name = NAMES
            .iter()
            .map(|(name, mime, description, category)| {
                (
                    *name,
                    Type {
                        mime,
                        description,
                        category: *category,
                    },
                )
            })
            .collect();
        Index {
            by_extension,
            by_name,
        }
    })
}

/// The type of a file called `name`. Extensions match without regard to case;
/// `archive.tar.gz` is a Gzip archive, as in shared-mime-info.
pub fn for_name(name: &str) -> Type {
    let index = index();
    let lower = name.to_lowercase();
    if let Some(found) = index.by_name.get(lower.as_str()) {
        return *found;
    }
    match lower.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => index
            .by_extension
            .get(extension)
            .copied()
            .unwrap_or(UNKNOWN),
        _ => UNKNOWN,
    }
}

/// Whether `mime` is `wanted`, or a kind of it. `image/*` matches every
/// image; `text/plain` matches every `text/*` type and the structured-text
/// types that end in `+xml` or `+json`; `application/octet-stream` matches
/// everything, as in shared-mime-info's subclass tree.
pub fn is_a(mime: &str, wanted: &str) -> bool {
    if mime.eq_ignore_ascii_case(wanted) || wanted == "application/octet-stream" {
        return true;
    }
    if let Some(group) = wanted.strip_suffix("/*") {
        return mime
            .split_once('/')
            .is_some_and(|(kind, _)| kind.eq_ignore_ascii_case(group));
    }
    match wanted {
        "text/plain" => {
            mime.starts_with("text/") || mime.ends_with("+xml") || mime.ends_with("+json")
        }
        "application/xml" => mime.ends_with("+xml") || mime == "text/html",
        "application/zip" => {
            mime.starts_with("application/vnd.openxmlformats-officedocument.")
                || mime.starts_with("application/vnd.oasis.opendocument.")
                || mime == "application/epub+zip"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_by_extension_ignoring_case() {
        assert_eq!(for_name("Photo.JPG").mime, "image/jpeg");
        assert_eq!(for_name("archive.tar.gz").mime, "application/gzip");
        assert_eq!(for_name("vpn.ovpn").category, Category::Text);
        assert_eq!(for_name("Makefile").mime, "text/x-makefile");
        assert_eq!(for_name(".bashrc"), UNKNOWN);
        assert_eq!(for_name("noextension"), UNKNOWN);
    }

    #[test]
    fn subclasses_match() {
        assert!(is_a("image/png", "image/*"));
        assert!(is_a("text/markdown", "text/plain"));
        assert!(is_a(
            "application/vnd.oasis.opendocument.text",
            "application/zip"
        ));
        assert!(!is_a("image/png", "audio/*"));
        assert!(is_a("anything/else", "application/octet-stream"));
    }
}
