//! The one place that decides which icon a file, a folder, a place or a drive
//! has, and the one element that draws it.
//!
//! Every surface that shows such an icon asks here: the sidebar's places,
//! bookmarks and drives, list rows, grid tiles, the address bar's crumbs and
//! their menus, Properties and Quick Look, the drag chip, the chooser, tab
//! titles and the file-type rows of Settings. None of them chooses a glyph
//! itself, so the same folder looks the same wherever it appears.
//!
//! ```ignore
//! let spec = FileIcon::for_entry(&entry);          // or for_path, for_mime, for_place
//! file_icon_view(&tokens, &spec, IconSize::Row, selected)
//! ```
//!
//! # What decides the glyph
//!
//! - A folder is decided by the **text of its path** and never by where a link
//!   leads, so `~/Downloads` keeps the Downloads glyph when it is a link to
//!   `/mnt/x`, and `/mnt/x` is a plain folder. The special folders are the
//!   home folder, the XDG user directories (the paths `user-dirs.dirs`
//!   configures and the standard names under home: Desktop, Documents,
//!   Downloads, Music, Pictures, Videos, Templates, Public), the trash, the
//!   filesystem root and the drive kinds. Any other folder, a bookmark such
//!   as Projects included, has the plain folder glyph. Paths are compared as
//!   written: case counts and a trailing slash does not.
//! - A file is decided by its MIME type through [`FileKind`]: image, audio,
//!   video, text, code, PDF, archive, spreadsheet, presentation, document,
//!   font, executable, and unknown. `bingux_files::mime` classifies a name;
//!   [`FileKind::of_mime`] classifies a type the system reports.
//! - A symbolic link keeps the glyph of what it is and gains the small link
//!   emblem ([`Emblem::Link`]). Places never read the disk, so only an entry
//!   (a listing row) carries it.
//! - A hidden file or folder is drawn faint ([`Tint::Dim`]).
//!
//! # What decides the colour
//!
//! One rule for every surface: the glyph is the accent only when its row or
//! tile is selected, muted otherwise, and fainter still when the thing is
//! hidden or not responding. A folder is not told from a file by colour but
//! by shape.
//!
//! # Size
//!
//! A [`FileIconSize`] is an [`IconSize`] token (rows, the sidebar, the address
//! bar) or a size in pixels (the file view's own zoom, 16 to 128).

use std::path::Path;

use bingux_files::mime;
use bingux_files::mounts::MountKind;
use bingux_files::places::{SpecialFolder, special_folder};
use bingux_files::{Category, Entry, Kind, Place, PlaceKind};
use freya::prelude::*;

use crate::component::{ComponentName, Prop, Published};
use crate::icons::{self, Icon};
use crate::style::*;
use crate::theme::Tokens;

/// The kind of a file, as far as its icon cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    Folder,
    Image,
    Audio,
    Video,
    /// Plain text and markup.
    Text,
    /// Source code and scripts.
    Code,
    Pdf,
    Archive,
    Spreadsheet,
    Presentation,
    /// A word-processor document or an e-book.
    Document,
    Font,
    /// A program or a shared library.
    Executable,
    Unknown,
}

impl FileKind {
    /// Every kind, in the order a catalogue lists them.
    pub const ALL: [FileKind; 14] = [
        FileKind::Folder,
        FileKind::Image,
        FileKind::Audio,
        FileKind::Video,
        FileKind::Text,
        FileKind::Code,
        FileKind::Pdf,
        FileKind::Archive,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Document,
        FileKind::Font,
        FileKind::Executable,
        FileKind::Unknown,
    ];

    /// What a person calls it.
    pub fn label(self) -> &'static str {
        match self {
            FileKind::Folder => "Folder",
            FileKind::Image => "Image",
            FileKind::Audio => "Audio",
            FileKind::Video => "Video",
            FileKind::Text => "Text",
            FileKind::Code => "Code",
            FileKind::Pdf => "PDF",
            FileKind::Archive => "Archive",
            FileKind::Spreadsheet => "Spreadsheet",
            FileKind::Presentation => "Presentation",
            FileKind::Document => "Document",
            FileKind::Font => "Font",
            FileKind::Executable => "Program",
            FileKind::Unknown => "Unknown",
        }
    }

    /// The glyph of this kind.
    pub fn glyph(self) -> Icon {
        match self {
            FileKind::Folder => Icon::Folder,
            FileKind::Image => Icon::ImageGeneric,
            FileKind::Audio => Icon::AudioXGeneric,
            FileKind::Video => Icon::VideoGeneric,
            FileKind::Text => Icon::TextXGeneric,
            FileKind::Code => Icon::TextXScript,
            FileKind::Pdf => Icon::ApplicationPdf,
            FileKind::Archive => Icon::PackageXGeneric,
            FileKind::Spreadsheet => Icon::XOfficeSpreadsheet,
            FileKind::Presentation => Icon::XOfficePresentation,
            FileKind::Document => Icon::XOfficeDocument,
            FileKind::Font => Icon::FontXGeneric,
            FileKind::Executable => Icon::Application,
            FileKind::Unknown => Icon::ApplicationXGeneric,
        }
    }

    /// Where a picture of a file of this kind may come from, if it can have
    /// one: the thumbnail cache, or for an image the file itself.
    pub fn thumbnail(self) -> Option<Source> {
        match self {
            FileKind::Image => Some(Source::Decode),
            FileKind::Video
            | FileKind::Pdf
            | FileKind::Document
            | FileKind::Spreadsheet
            | FileKind::Presentation => Some(Source::Cache),
            _ => None,
        }
    }

    /// The kind of the MIME type `mime` (`image/png`, `application/pdf`),
    /// judged from its name alone. A type nothing is known about is
    /// [`FileKind::Unknown`].
    pub fn of_mime(mime: &str) -> FileKind {
        kind_of_mime(mime).unwrap_or(FileKind::Unknown)
    }

    /// The kind of a file whose type is `mime` and whose coarse category (from
    /// the name table) is `category`: the type's own name first, then the
    /// category when the type says nothing.
    fn of(mime: &str, category: Category) -> FileKind {
        kind_of_mime(mime).unwrap_or(match category {
            Category::Folder => FileKind::Folder,
            Category::Image => FileKind::Image,
            Category::Audio => FileKind::Audio,
            Category::Video => FileKind::Video,
            Category::Text => FileKind::Text,
            Category::Document => FileKind::Document,
            Category::Archive => FileKind::Archive,
            Category::Code => FileKind::Code,
            Category::Other => FileKind::Unknown,
        })
    }
}

/// The kind a MIME type's own name says, or `None` when it says nothing.
fn kind_of_mime(mime: &str) -> Option<FileKind> {
    let (kind, subtype) = mime.split_once('/').unwrap_or((mime, ""));
    match kind {
        "image" => return Some(FileKind::Image),
        "audio" => return Some(FileKind::Audio),
        "video" => return Some(FileKind::Video),
        "font" => return Some(FileKind::Font),
        "inode" if subtype == "directory" => return Some(FileKind::Folder),
        _ => {}
    }
    let has = |parts: &[&str]| parts.iter().any(|part| subtype.contains(part));
    if has(&["font", "woff", "truetype", "opentype", "x-ttf", "x-otf"]) {
        return Some(FileKind::Font);
    }
    if has(&["pdf"]) {
        return Some(FileKind::Pdf);
    }
    if has(&["spreadsheet", "ms-excel", "csv"]) {
        return Some(FileKind::Spreadsheet);
    }
    if has(&["presentation", "ms-powerpoint"]) {
        return Some(FileKind::Presentation);
    }
    if has(&[
        "msword",
        "wordprocessingml",
        "rtf",
        "epub",
        "djvu",
        "abiword",
        "fictionbook",
        "postscript",
        "openxmlformats",
        "oasis.opendocument",
        "vnd.ms-",
        "x-cbz",
        "x-cbr",
        "calendar",
    ]) {
        return Some(FileKind::Document);
    }
    if has(&[
        "x-executable",
        "x-pie-executable",
        "x-sharedlib",
        "vnd.appimage",
        "x-msdos-program",
    ]) {
        return Some(FileKind::Executable);
    }
    if has(&[
        "zip", "tar", "gzip", "bzip", "xz", "7z", "rar", "zstd", "compress", "cpio", "archive",
        "lzma", "lz4", "iso9660", "cd-image", "x-rpm", "debian",
    ]) {
        return Some(FileKind::Archive);
    }
    if has(&[
        "json",
        "xml",
        "javascript",
        "shellscript",
        "x-python",
        "x-perl",
        "x-ruby",
        "x-php",
        "x-java",
        "sql",
        "yaml",
        "toml",
        "x-csrc",
        "wasm",
    ]) || (kind == "text" && subtype.starts_with("x-"))
    {
        return Some(FileKind::Code);
    }
    (kind == "text").then_some(FileKind::Text)
}

/// Where a picture of a file may come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The cache, or else the file itself.
    Decode,
    /// The cache only.
    Cache,
}

/// A small mark drawn over the corner of a glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emblem {
    /// The icon is a symbolic link.
    Link,
}

impl Emblem {
    fn glyph(self) -> Icon {
        match self {
            Emblem::Link => Icon::EmblemSymbolicLink,
        }
    }
}

/// How strongly the glyph is drawn when its row is not selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    /// Muted, the ink of a quiet icon.
    Normal,
    /// Fainter: a hidden file, or a drive that does not answer.
    Dim,
}

/// What an icon is, before it is drawn: the glyph, an optional emblem, how
/// faint it is, and whether a picture of the file may replace it. Build one
/// with a `for_*` constructor and draw it with [`file_icon_view`]; the same
/// description gives the same icon on every surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIcon {
    pub glyph: Icon,
    /// What kind of file it stands for.
    pub kind: FileKind,
    pub emblem: Option<Emblem>,
    pub tint: Tint,
    /// Where a thumbnail may come from; `None` when the icon is final.
    pub thumbnail: Option<Source>,
}

impl FileIcon {
    fn plain(glyph: Icon, kind: FileKind) -> FileIcon {
        FileIcon {
            glyph,
            kind,
            emblem: None,
            tint: Tint::Normal,
            thumbnail: None,
        }
    }

    /// The icon of a kind of file.
    pub fn for_kind(kind: FileKind) -> FileIcon {
        FileIcon {
            thumbnail: kind.thumbnail(),
            ..FileIcon::plain(kind.glyph(), kind)
        }
    }

    /// The icon of a file of MIME type `mime`.
    pub fn for_mime(mime: &str) -> FileIcon {
        FileIcon::for_kind(FileKind::of_mime(mime))
    }

    /// The icon of a file called `name`, for a name that has no entry yet,
    /// such as a suggestion in the address bar.
    pub fn for_name(name: &str) -> FileIcon {
        let found = mime::for_name(name);
        FileIcon::for_kind(FileKind::of(found.mime, found.category)).hidden(name.starts_with('.'))
    }

    /// The icon of the folder `special` names.
    pub fn for_special(special: SpecialFolder) -> FileIcon {
        let glyph = match special {
            SpecialFolder::Home => Icon::UserHome,
            SpecialFolder::Desktop => Icon::FolderDesktop,
            SpecialFolder::Documents => Icon::FolderDocuments,
            SpecialFolder::Downloads => Icon::FolderDownload,
            SpecialFolder::Music => Icon::FolderMusic,
            SpecialFolder::Pictures => Icon::FolderPictures,
            SpecialFolder::Videos => Icon::FolderVideos,
            SpecialFolder::Templates => Icon::FolderTemplates,
            SpecialFolder::PublicShare => Icon::FolderPublicshare,
            SpecialFolder::Trash => Icon::UserTrash,
            SpecialFolder::Root => Icon::DriveHarddisk,
        };
        FileIcon::plain(glyph, FileKind::Folder)
    }

    /// The icon of the item at `path`, which is a `kind`. A folder is decided
    /// by the text of `path` alone: nothing is read from the disk and a link
    /// is not followed.
    pub fn for_path(path: &Path, kind: Kind) -> FileIcon {
        let hidden = path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."));
        let icon = match kind {
            Kind::Directory => FileIcon::folder_at(path),
            Kind::File => match path.file_name() {
                Some(name) => FileIcon::for_name(&name.to_string_lossy()),
                None => FileIcon::for_kind(FileKind::Unknown),
            },
            Kind::Other => FileIcon::for_kind(FileKind::Unknown),
        };
        icon.hidden(hidden)
    }

    /// The icon of the folder at `path`: the home or user folder it is, or
    /// the plain folder.
    pub fn folder_at(path: &Path) -> FileIcon {
        match special_folder(path) {
            Some(special) => FileIcon::for_special(special),
            None => FileIcon::for_kind(FileKind::Folder),
        }
    }

    /// The icon of a listed entry: [`FileIcon::for_path`] with what the
    /// listing knows besides, the link emblem and the hidden flag.
    pub fn for_entry(entry: &Entry) -> FileIcon {
        // A link not looked at yet that bears a user folder's name is that
        // folder far more often than not; do not show a page and then a folder.
        let folder = entry.is_dir() || (entry.pending_link && entry.special.is_some());
        let icon = if folder {
            match entry.special.or_else(|| special_folder(&entry.path)) {
                Some(special) => FileIcon::for_special(special),
                None => FileIcon::for_kind(FileKind::Folder),
            }
        } else {
            let mut kind = FileKind::of(entry.mime.mime, entry.mime.category);
            if kind == FileKind::Unknown && entry.kind == Kind::File && entry.mode & 0o111 != 0 {
                kind = FileKind::Executable;
            }
            if entry.kind == Kind::Other {
                kind = FileKind::Unknown;
            }
            FileIcon::for_kind(kind)
        };
        icon.hidden(entry.hidden).linked(entry.symlink)
    }

    /// The sidebar icon of a place. A bookmark is a folder, so it has a
    /// folder's icon, decided by its path as everywhere else.
    pub fn for_place(place: &Place) -> FileIcon {
        match place.kind {
            PlaceKind::Bookmark => match &place.path {
                Some(path) => FileIcon::folder_at(path),
                None => FileIcon::for_kind(FileKind::Folder),
            },
            kind => FileIcon::for_place_kind(kind),
        }
    }

    /// The icon of a standard place.
    pub fn for_place_kind(kind: PlaceKind) -> FileIcon {
        match kind {
            PlaceKind::Recent => FileIcon::plain(Icon::Clock, FileKind::Folder),
            PlaceKind::Home => FileIcon::for_special(SpecialFolder::Home),
            PlaceKind::Desktop => FileIcon::for_special(SpecialFolder::Desktop),
            PlaceKind::Documents => FileIcon::for_special(SpecialFolder::Documents),
            PlaceKind::Downloads => FileIcon::for_special(SpecialFolder::Downloads),
            PlaceKind::Music => FileIcon::for_special(SpecialFolder::Music),
            PlaceKind::Pictures => FileIcon::for_special(SpecialFolder::Pictures),
            PlaceKind::Videos => FileIcon::for_special(SpecialFolder::Videos),
            PlaceKind::Trash => FileIcon::for_special(SpecialFolder::Trash),
            PlaceKind::Computer => FileIcon::for_mount(MountKind::Local),
            PlaceKind::Volume => FileIcon::for_mount(MountKind::Removable),
            PlaceKind::Bookmark => FileIcon::for_kind(FileKind::Folder),
        }
    }

    /// The sidebar icon of the Trash: the trash can, drawn faint while there is
    /// nothing in it and as every folder is when there is. This is the one
    /// place that says so; a row never picks the tint itself.
    pub fn for_trash(full: bool) -> FileIcon {
        let icon = FileIcon::for_special(SpecialFolder::Trash);
        if full { icon } else { icon.dimmed() }
    }

    /// The icon of a drive of `kind`.
    pub fn for_mount(kind: MountKind) -> FileIcon {
        let glyph = match kind {
            MountKind::Local | MountKind::Other => Icon::DriveHarddisk,
            MountKind::Removable => Icon::MediaRemovable,
            MountKind::Network => Icon::NetworkServer,
            MountKind::Optical => Icon::MediaOptical,
        };
        FileIcon::plain(glyph, FileKind::Folder)
    }

    /// The same icon with the link emblem when `linked`.
    pub fn linked(mut self, linked: bool) -> FileIcon {
        self.emblem = linked.then_some(Emblem::Link);
        self
    }

    /// The same icon drawn faint when `hidden`.
    pub fn hidden(mut self, hidden: bool) -> FileIcon {
        if hidden {
            self.tint = Tint::Dim;
        }
        self
    }

    /// The same icon drawn faint: a drive that does not answer.
    pub fn dimmed(mut self) -> FileIcon {
        self.tint = Tint::Dim;
        self
    }

    /// The colour of the glyph: the accent when `selected`, otherwise muted,
    /// or faint for a dim icon.
    pub fn paint(&self, selected: bool) -> Paint {
        if selected {
            Paint::Accent
        } else if self.tint == Tint::Dim {
            Paint::MutedFaint
        } else {
            Paint::Muted
        }
    }
}

/// How big an icon is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileIconSize {
    /// A size token: rows, the sidebar, the address bar.
    Token(IconSize),
    /// Logical pixels: the file view's zoom.
    Px(u16),
}

impl From<IconSize> for FileIconSize {
    fn from(size: IconSize) -> Self {
        FileIconSize::Token(size)
    }
}

impl From<u16> for FileIconSize {
    fn from(px: u16) -> Self {
        FileIconSize::Px(px)
    }
}

impl FileIconSize {
    /// The edge in logical pixels.
    pub fn px(self) -> f32 {
        match self {
            FileIconSize::Token(size) => size.px(),
            FileIconSize::Px(px) => f32::from(px),
        }
    }
}

/// The edge of the link emblem on a glyph of `edge` pixels: half of it, and
/// never so small that its arrow is lost.
fn emblem_edge(edge: f32) -> f32 {
    (edge * 0.5).round().max(8.0)
}

/// Draws `spec` in a square of `size`: its glyph in the colour of
/// [`FileIcon::paint`], and its emblem over the lower right corner.
pub fn file_icon_view(
    tokens: &Tokens,
    spec: &FileIcon,
    size: impl Into<FileIconSize>,
    selected: bool,
) -> Element {
    drawn(
        spec,
        size.into(),
        spec.paint(selected).color(tokens),
        Paint::Bg.color(tokens),
    )
}

/// [`file_icon_view`] for a floating surface (the drag chip, a tooltip): the
/// ink that reads on it, whatever the tint.
pub fn file_icon_view_floating(
    tokens: &Tokens,
    spec: &FileIcon,
    size: impl Into<FileIconSize>,
) -> Element {
    drawn(
        spec,
        size.into(),
        Paint::OnFloating.color(tokens),
        Paint::Floating.color(tokens),
    )
}

/// The badge is the disc of the page behind the icon with its arrow cut out,
/// so the arrow shows in the glyph's own colour, over the folder's corner.
fn drawn(spec: &FileIcon, size: FileIconSize, color: Color, badge: Color) -> Element {
    let edge = size.px();
    let glyph = spec.glyph;
    let mut slot = rect()
        .component("FileIcon")
        .published(move || {
            Published::call("file_icon_view")
                .arg(Prop::text("glyph", format!("{glyph:?}")))
                .arg(Prop::text("size", format!("{edge}")))
        })
        .width(Size::px(edge))
        .height(Size::px(edge))
        .child(icons::glyph(glyph, edge, color));
    if let Some(emblem) = spec.emblem {
        let mark = emblem_edge(edge);
        slot = slot.child(
            rect()
                .position(
                    Position::new_absolute()
                        .right((edge * 0.04).round())
                        .bottom((edge * 0.1).round()),
                )
                .width(Size::px(mark))
                .height(Size::px(mark))
                .child(icons::glyph(emblem.glyph(), mark, badge)),
        );
    }
    slot.into()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use bingux_files::places::UserDirs;

    use super::*;

    fn entry(path: &str, kind: Kind) -> Entry {
        let path = PathBuf::from(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        Entry::named(path, name, kind, false, false)
    }

    #[test]
    fn mime_types_map_to_kinds() {
        for (mime, kind) in [
            ("image/png", FileKind::Image),
            ("audio/flac", FileKind::Audio),
            ("video/webm", FileKind::Video),
            ("text/plain", FileKind::Text),
            ("text/markdown", FileKind::Text),
            ("text/x-python", FileKind::Code),
            ("application/json", FileKind::Code),
            ("application/x-shellscript", FileKind::Code),
            ("application/pdf", FileKind::Pdf),
            ("application/zip", FileKind::Archive),
            ("application/x-tar", FileKind::Archive),
            ("application/vnd.oasis.opendocument.spreadsheet", FileKind::Spreadsheet),
            ("application/vnd.ms-excel", FileKind::Spreadsheet),
            ("text/csv", FileKind::Spreadsheet),
            ("application/vnd.oasis.opendocument.presentation", FileKind::Presentation),
            ("application/vnd.ms-powerpoint", FileKind::Presentation),
            ("application/msword", FileKind::Document),
            ("application/vnd.oasis.opendocument.text", FileKind::Document),
            ("application/epub+zip", FileKind::Document),
            ("font/ttf", FileKind::Font),
            ("application/x-font-ttf", FileKind::Font),
            ("application/x-sharedlib", FileKind::Executable),
            ("application/vnd.appimage", FileKind::Executable),
            ("inode/directory", FileKind::Folder),
            ("application/x-thing", FileKind::Unknown),
            ("application/octet-stream", FileKind::Unknown),
        ] {
            assert_eq!(FileKind::of_mime(mime), kind, "{mime}");
            assert_eq!(FileIcon::for_mime(mime).glyph, kind.glyph(), "{mime}");
        }
    }

    #[test]
    fn every_kind_has_a_glyph_of_its_own() {
        let glyphs: Vec<Icon> = FileKind::ALL.iter().map(|kind| kind.glyph()).collect();
        for (index, glyph) in glyphs.iter().enumerate() {
            assert!(!glyphs[..index].contains(glyph), "{:?} repeats", FileKind::ALL[index]);
        }
    }

    #[test]
    fn a_name_and_a_mime_type_give_the_same_icon() {
        for (name, mime) in [
            ("a.png", "image/png"),
            ("a.pdf", "application/pdf"),
            ("a.ods", "application/vnd.oasis.opendocument.spreadsheet"),
            ("a.json", "application/json"),
            ("a.ttf", "font/ttf"),
            ("a.zip", "application/zip"),
            ("a.txt", "text/plain"),
        ] {
            assert_eq!(FileIcon::for_name(name).glyph, FileIcon::for_mime(mime).glyph, "{name}");
        }
    }

    #[test]
    fn images_decode_documents_and_videos_only_use_the_cache() {
        let thumb = |name: &str| FileIcon::for_entry(&entry(&format!("/x/{name}"), Kind::File)).thumbnail;
        assert_eq!(thumb("a.png"), Some(Source::Decode));
        assert_eq!(thumb("a.jpg"), Some(Source::Decode));
        assert_eq!(thumb("a.pdf"), Some(Source::Cache));
        assert_eq!(thumb("a.mp4"), Some(Source::Cache));
        assert_eq!(thumb("a.odt"), Some(Source::Cache));
        assert_eq!(thumb("a.txt"), None);
        assert_eq!(thumb("a.zip"), None);
    }

    #[test]
    fn a_folder_has_no_picture_and_an_entry_names_its_glyph() {
        let folder = FileIcon::for_entry(&entry("/x/d", Kind::Directory));
        assert_eq!(folder.thumbnail, None);
        assert_eq!(folder.glyph, Icon::Folder);
        assert_eq!(FileIcon::for_entry(&entry("/x/a.pdf", Kind::File)).glyph, Icon::ApplicationPdf);
        assert_eq!(FileIcon::for_entry(&entry("/x/a.weird", Kind::File)).glyph, Icon::ApplicationXGeneric);
        assert_eq!(FileIcon::for_entry(&entry("/x/socket", Kind::Other)).glyph, Icon::ApplicationXGeneric);
    }

    #[test]
    fn an_unknown_file_with_the_execute_bit_is_a_program() {
        let mut program = entry("/x/run", Kind::File);
        program.mode = 0o755;
        assert_eq!(FileIcon::for_entry(&program).kind, FileKind::Executable);
        program.mode = 0o644;
        assert_eq!(FileIcon::for_entry(&program).kind, FileKind::Unknown);
        let mut script = entry("/x/run.py", Kind::File);
        script.mode = 0o755;
        assert_eq!(FileIcon::for_entry(&script).kind, FileKind::Code);
    }

    #[test]
    fn a_link_keeps_its_glyph_and_gains_the_emblem() {
        let mut link = entry("/x/sym", Kind::Directory);
        link.symlink = true;
        let icon = FileIcon::for_entry(&link);
        assert_eq!(icon.glyph, Icon::Folder);
        assert_eq!(icon.emblem, Some(Emblem::Link));
        assert_eq!(FileIcon::for_entry(&entry("/x/d", Kind::Directory)).emblem, None);
        assert_eq!(FileIcon::folder_at(Path::new("/x/d")).linked(true).glyph, Icon::Folder);
    }

    #[test]
    fn a_hidden_entry_is_dim_until_it_is_selected() {
        let mut hidden = entry("/x/.cache", Kind::Directory);
        hidden.hidden = true;
        let icon = FileIcon::for_entry(&hidden);
        assert_eq!(icon.tint, Tint::Dim);
        assert_eq!(icon.paint(false), Paint::MutedFaint);
        assert_eq!(icon.paint(true), Paint::Accent);
        assert_eq!(FileIcon::for_path(Path::new("/x/.cache"), Kind::Directory).tint, Tint::Dim);
        assert_eq!(FileIcon::for_name(".bashrc").tint, Tint::Dim);
        let plain = FileIcon::for_entry(&entry("/x/d", Kind::Directory));
        assert_eq!(plain.paint(false), Paint::Muted);
    }

    #[test]
    fn the_accent_is_for_the_selected_row_only() {
        for kind in FileKind::ALL {
            let icon = FileIcon::for_kind(kind);
            assert_eq!(icon.paint(true), Paint::Accent);
            assert_ne!(icon.paint(false), Paint::Accent);
        }
    }

    #[test]
    fn places_have_their_own_icons() {
        let place = |kind, path: Option<&str>| Place { kind, name: String::new(), path: path.map(PathBuf::from) };
        assert_eq!(FileIcon::for_place(&place(PlaceKind::Trash, None)).glyph, Icon::UserTrash);
        assert_eq!(FileIcon::for_place(&place(PlaceKind::Recent, None)).glyph, Icon::Clock);
        assert_eq!(FileIcon::for_place(&place(PlaceKind::Home, None)).glyph, Icon::UserHome);
        assert_eq!(FileIcon::for_place(&place(PlaceKind::Computer, None)).glyph, Icon::DriveHarddisk);
        assert_eq!(FileIcon::for_mount(MountKind::Network).glyph, Icon::NetworkServer);
        assert_eq!(FileIcon::for_mount(MountKind::Removable).glyph, Icon::MediaRemovable);
        assert_eq!(FileIcon::for_mount(MountKind::Optical).glyph, Icon::MediaOptical);
    }

    /// The path decisions, against a fake `user-dirs.dirs`, without the
    /// process-wide one: [`FileIcon::folder_at`] goes through the same
    /// [`UserDirs::special`].
    #[test]
    fn folders_are_decided_by_path_text_not_by_where_a_link_leads() {
        let root = std::env::temp_dir().join(format!("bingux-file-icon-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let share = root.join("mnt/x");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&share).unwrap();
        std::os::unix::fs::symlink(&share, home.join("Downloads")).unwrap();
        // The file the user has: every key but Desktop disabled with `$HOME/`.
        let dirs = UserDirs::parse(
            Some("XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/\"\nXDG_DOCUMENTS_DIR=\"$HOME/\"\nXDG_VIDEOS_DIR=\"$HOME/Clips\"\n"),
            &home,
        );
        let glyph = |path: &Path| match dirs.special(path) {
            Some(special) => FileIcon::for_special(special).glyph,
            None => Icon::Folder,
        };
        assert_eq!(glyph(&home.join("Downloads")), Icon::FolderDownload);
        assert_eq!(glyph(&home.join("Downloads/")), Icon::FolderDownload);
        assert_eq!(glyph(&share), Icon::Folder);
        assert_eq!(glyph(&home.join("Documents")), Icon::FolderDocuments);
        assert_eq!(glyph(&home.join("Clips")), Icon::FolderVideos);
        assert_eq!(glyph(&home.join("Videos")), Icon::FolderVideos);
        assert_eq!(glyph(&home), Icon::UserHome);
        // Bookmarks that are not special folders are plain.
        assert_eq!(glyph(&home.join("Notes")), Icon::Folder);
        assert_eq!(glyph(&home.join("dev")), Icon::Folder);
        // Case counts.
        assert_eq!(glyph(&home.join("downloads")), Icon::Folder);
        // A file with a folder's name is a file: only a directory is asked.
        let file = FileIcon::for_path(&home.join("Notes.txt"), Kind::File);
        assert_eq!(file.glyph, Icon::TextXGeneric);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_named_like_a_user_folder_is_still_a_file() {
        let mut file = entry("/home/a/Downloads", Kind::File);
        file.special = Some(SpecialFolder::Downloads);
        let icon = FileIcon::for_entry(&file);
        assert_ne!(icon.glyph, Icon::FolderDownload);
        assert_eq!(icon.kind, FileKind::Unknown);
        // A link not looked at yet with a folder's name already shows the folder.
        file.pending_link = true;
        assert_eq!(FileIcon::for_entry(&file).glyph, Icon::FolderDownload);
    }

    #[test]
    fn the_emblem_is_half_the_glyph_but_never_below_eight_pixels() {
        assert_eq!(emblem_edge(16.0), 8.0);
        assert_eq!(emblem_edge(12.0), 8.0);
        assert_eq!(emblem_edge(64.0), 32.0);
        assert_eq!(emblem_edge(128.0), 64.0);
    }
}
