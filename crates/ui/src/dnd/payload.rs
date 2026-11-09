//! What a drag or the clipboard carries: [`Payload`] going out, [`Received`]
//! coming in, and [`Format`], the kinds of data both are made of.
//!
//! Wayland names data by MIME type and a program offers the same thing in
//! several (a file is `text/uri-list` for a file manager and its path as
//! `text/plain` for a text editor). [`Format`] is the typed view: a target
//! says it takes [`Format::Uris`] and the best MIME type the source offers
//! for it is chosen ([`Format::best_mime`]).

use std::any::Any;
use std::path::{Path, PathBuf};

use bingux_files::location::{file_uri, from_file_uri};
use std::rc::Rc;
use std::sync::Arc;

/// `text/uri-list`: one URI a line (RFC 2483), how every Linux program drags
/// and copies files.
pub const URI_LIST: &str = "text/uri-list";
/// GNOME's (and most file managers') clipboard form of copied or cut files:
/// `copy` or `cut`, then one URI a line.
pub const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
/// KDE's mark of a cut: `1` when the files were cut.
pub const KDE_CUT_SELECTION: &str = "application/x-kde-cutselection";
/// The plain text types, best first. `UTF8_STRING`, `TEXT` and `STRING` are
/// X11 names that Xwayland programs still offer and ask for.
pub const TEXT_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];
/// The marker a drag from this program carries, so a drop on one of its own
/// windows is handed over in memory. `;pid=<pid>;id=<drag>` follows.
pub(crate) const LOCAL_MARKER: &str = "application/x-bingux-local-drag";

/// A kind of data.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// Files, folders or links: `text/uri-list` (and the file managers'
    /// copied-files type on the clipboard).
    Uris,
    /// Plain text.
    Text,
    /// HTML markup, `text/html`.
    Html,
    /// A picture: `image/png` first, then any `image/*` the source offers.
    Image,
    /// Any other MIME type, exactly.
    Mime(String),
}

impl Format {
    /// Whether `mime` carries this format.
    pub fn matches(&self, mime: &str) -> bool {
        let base = mime.split(';').next().unwrap_or(mime).trim();
        match self {
            Format::Uris => base == URI_LIST || base == GNOME_COPIED_FILES,
            Format::Text => TEXT_TYPES.contains(&mime) || base == "text/plain",
            Format::Html => base == "text/html",
            Format::Image => base.starts_with("image/"),
            Format::Mime(wanted) => mime == wanted,
        }
    }

    /// The best of `offered` for this format, or `None`. Order matters only
    /// where types differ in quality: UTF-8 text before other text, PNG
    /// before other pictures, a URI list before the copied-files type.
    pub fn best_mime<'a>(&self, offered: &'a [String]) -> Option<&'a str> {
        let rank = |mime: &str| -> usize {
            match self {
                Format::Text => TEXT_TYPES
                    .iter()
                    .position(|candidate| *candidate == mime)
                    .unwrap_or(TEXT_TYPES.len()),
                Format::Image => usize::from(mime != "image/png"),
                Format::Uris => usize::from(mime != URI_LIST),
                Format::Html | Format::Mime(_) => 0,
            }
        };
        offered
            .iter()
            .map(String::as_str)
            .filter(|mime| self.matches(mime))
            .min_by_key(|mime| rank(mime))
    }

    /// The format a MIME type carries, as [`Format::matches`] reads it.
    pub fn of(mime: &str) -> Format {
        [Format::Uris, Format::Text, Format::Html, Format::Image]
            .into_iter()
            .find(|format| format.matches(mime))
            .unwrap_or_else(|| Format::Mime(mime.to_string()))
    }

    /// The formats in a list of MIME types, each once, in offered order.
    /// The local-drag marker is not a format.
    pub fn list(offered: &[String]) -> Vec<Format> {
        let mut formats: Vec<Format> = Vec::new();
        for mime in offered {
            if mime.starts_with(LOCAL_MARKER) {
                continue;
            }
            let format = Format::of(mime);
            if !formats.contains(&format) {
                formats.push(format);
            }
        }
        formats
    }
}

/// How a representation's bytes are produced.
#[derive(Clone)]
enum Bytes {
    Ready(Arc<[u8]>),
    /// Made when a target asks for this type, on the UI thread.
    Lazy(Rc<dyn Fn() -> Vec<u8>>),
}

/// What a drag or a copy offers: the same thing in one or more MIME types,
/// and optionally a value for windows of this program.
///
/// ```ignore
/// // Files: a file manager takes the URIs, a text editor the paths.
/// Payload::files(&paths);
/// // A picture, encoded only if someone drops it.
/// Payload::new().lazy("image/png", move || encode_png(&image)).text(caption);
/// ```
#[derive(Clone, Default)]
pub struct Payload {
    items: Vec<(String, Bytes)>,
    local: Option<Rc<dyn Any>>,
}

impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload")
            .field("mime_types", &self.mime_types())
            .field("local", &self.local.is_some())
            .finish()
    }
}

impl Payload {
    pub fn new() -> Self {
        Self::default()
    }

    /// Plain text, in every text type programs ask for.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        let bytes: Arc<[u8]> = Arc::from(text.into().into_bytes());
        self.items.retain(|(mime, _)| !Format::Text.matches(mime));
        for mime in TEXT_TYPES {
            self.items
                .push((mime.to_string(), Bytes::Ready(bytes.clone())));
        }
        self
    }

    /// HTML markup. Add [`text`](Self::text) as well for programs that take
    /// only plain text.
    pub fn html(self, html: impl Into<String>) -> Self {
        self.bytes("text/html", html.into().into_bytes())
    }

    /// URIs (`file:`, `https:`, …) as a `text/uri-list`.
    pub fn uris<S: AsRef<str>>(self, uris: impl IntoIterator<Item = S>) -> Self {
        let list: Vec<String> = uris
            .into_iter()
            .map(|uri| uri.as_ref().to_string())
            .collect();
        self.bytes(URI_LIST, uri_list(&list).into_bytes())
    }

    /// Files and folders: their URIs, and their paths one a line as text.
    pub fn files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Self {
        let paths: Vec<PathBuf> = paths.into_iter().map(|path| path.as_ref().into()).collect();
        let text = paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        Payload::new()
            .uris(paths.iter().map(|path| file_uri(path)))
            .text(text)
    }

    /// Files copied or cut for a paste in a file manager: the URIs, the
    /// copied-files types GNOME and KDE file managers read, and the paths as
    /// text. `cut` asks the pasting side to move them.
    pub fn copied_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>, cut: bool) -> Self {
        let paths: Vec<PathBuf> = paths.into_iter().map(|path| path.as_ref().into()).collect();
        let uris: Vec<String> = paths.iter().map(|path| file_uri(path)).collect();
        let verb = if cut { "cut" } else { "copy" };
        let gnome = std::iter::once(verb.to_string())
            .chain(uris.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        let mut payload = Payload::files(&paths).bytes(GNOME_COPIED_FILES, gnome.into_bytes());
        if cut {
            payload = payload.bytes(KDE_CUT_SELECTION, b"1".to_vec());
        }
        payload
    }

    /// A picture already encoded as `mime` (`image/png`, `image/jpeg`, …).
    pub fn image(self, mime: impl Into<String>, encoded: Vec<u8>) -> Self {
        self.bytes(mime, encoded)
    }

    /// Bytes as `mime`, replacing any earlier bytes of that type.
    pub fn bytes(mut self, mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        let mime = mime.into();
        self.items.retain(|(existing, _)| *existing != mime);
        self.items.push((mime, Bytes::Ready(Arc::from(bytes))));
        self
    }

    /// Bytes as `mime`, made only when a target asks for them. A drag runs
    /// `make` on the UI thread at the drop; a copy runs it at once (the
    /// clipboard outlives the window).
    pub fn lazy(mut self, mime: impl Into<String>, make: impl Fn() -> Vec<u8> + 'static) -> Self {
        let mime = mime.into();
        self.items.retain(|(existing, _)| *existing != mime);
        self.items.push((mime, Bytes::Lazy(Rc::new(make))));
        self
    }

    /// A value a drop on a window of this program receives as it is
    /// ([`Received::local`]), with no copy: a tab, a row of a list.
    /// Other programs see only the MIME types.
    pub fn local<T: Any>(mut self, value: T) -> Self {
        self.local = Some(Rc::new(value));
        self
    }

    /// The MIME types offered, in order.
    pub fn mime_types(&self) -> Vec<String> {
        self.items.iter().map(|(mime, _)| mime.clone()).collect()
    }

    /// The formats offered.
    pub fn formats(&self) -> Vec<Format> {
        Format::list(&self.mime_types())
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.local.is_none()
    }

    /// The bytes for `mime`, made now if they are lazy.
    pub fn bytes_for(&self, mime: &str) -> Option<Arc<[u8]>> {
        self.items
            .iter()
            .find(|(offered, _)| offered == mime)
            .map(|(_, bytes)| match bytes {
                Bytes::Ready(bytes) => bytes.clone(),
                Bytes::Lazy(make) => Arc::from(make()),
            })
    }

    /// Every representation made now: what the clipboard holds.
    pub(crate) fn resolve(&self) -> Vec<(String, Arc<[u8]>)> {
        self.items
            .iter()
            .filter_map(|(mime, _)| Some((mime.clone(), self.bytes_for(mime)?)))
            .collect()
    }

    /// What a target of this program receives for `format`, without a pipe.
    pub(crate) fn received(&self, format: &Format) -> Option<Received> {
        let mime_types = self.mime_types();
        let mime = format.best_mime(&mime_types)?.to_string();
        let bytes = self.bytes_for(&mime)?;
        Some(Received {
            mime,
            bytes: bytes.to_vec(),
            local: self.local.clone(),
        })
    }
}

/// What a drop or a paste received: bytes of one MIME type, read as the
/// format they carry.
#[derive(Clone)]
pub struct Received {
    mime: String,
    bytes: Vec<u8>,
    local: Option<Rc<dyn Any>>,
}

impl std::fmt::Debug for Received {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Received")
            .field("mime", &self.mime)
            .field("len", &self.bytes.len())
            .finish()
    }
}

impl Received {
    pub fn new(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            mime: mime.into(),
            bytes,
            local: None,
        }
    }

    /// The MIME type the data came as.
    pub fn mime(&self) -> &str {
        &self.mime
    }

    pub fn format(&self) -> Format {
        Format::of(&self.mime)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The data as text: text, HTML or a URI list as written (line ends
    /// made `\n`). `None` for a picture or bytes that are not UTF-8.
    pub fn text(&self) -> Option<String> {
        if matches!(self.format(), Format::Image) {
            return None;
        }
        let text = String::from_utf8(self.bytes.clone()).ok()?;
        Some(text.replace("\r\n", "\n"))
    }

    /// The URIs in a URI list or a copied-files list. Comments and blank
    /// lines are skipped.
    pub fn uris(&self) -> Vec<String> {
        if !matches!(self.format(), Format::Uris) {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&self.bytes);
        let mut lines = text.lines().map(str::trim);
        if self.mime.starts_with(GNOME_COPIED_FILES) {
            // The first line is `copy` or `cut`.
            lines.next();
        }
        lines
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    /// The local files among [`uris`](Self::uris). A URI of another host,
    /// or not a `file:` URI, is left out.
    pub fn files(&self) -> Vec<PathBuf> {
        self.uris()
            .iter()
            .filter_map(|uri| from_file_uri(uri))
            .collect()
    }

    /// For copied files: whether they were cut (the paste should move them).
    /// `None` when the data does not say.
    pub fn is_cut(&self) -> Option<bool> {
        if !self.mime.starts_with(GNOME_COPIED_FILES) {
            return None;
        }
        let text = String::from_utf8_lossy(&self.bytes);
        Some(text.lines().next().map(str::trim) == Some("cut"))
    }

    /// The encoded picture and its type, for an `image/*` drop or paste.
    pub fn image(&self) -> Option<(&str, &[u8])> {
        matches!(self.format(), Format::Image).then_some((self.mime.as_str(), &self.bytes))
    }

    /// The value a drag of this program carried with [`Payload::local`], if
    /// it came from this program and the value is a `T`.
    pub fn local<T: Any>(&self) -> Option<&T> {
        self.local.as_deref()?.downcast_ref::<T>()
    }

    /// Whether the drag came from a window of this program.
    pub fn is_local(&self) -> bool {
        self.local.is_some()
    }
}

/// A `text/uri-list` of `uris`: CRLF after each, as RFC 2483 asks.
pub fn uri_list(uris: &[String]) -> String {
    uris.iter().map(|uri| format!("{uri}\r\n")).collect()
}

#[cfg(test)]
mod tests {
    //! Failure modes guarded: a URI list from another program read wrongly
    //! (CRLF, comments, the copied-files verb line), a remote URI taken for
    //! a local file, and the wrong type picked from an offer.
    use super::*;

    #[test]
    fn a_uri_list_from_another_program_reads_its_files() {
        let list = "# from Nautilus\r\nfile:///tmp/a%20b\r\nfile://localhost/tmp/c\r\n\r\nhttps://example.org/d\r\nfile://otherhost/e\r\n";
        let received = Received::new(URI_LIST, list.as_bytes().to_vec());
        assert_eq!(received.uris().len(), 4);
        assert_eq!(
            received.files(),
            vec![PathBuf::from("/tmp/a b"), PathBuf::from("/tmp/c")]
        );
    }

    #[test]
    fn copied_files_skip_the_verb_and_say_whether_they_were_cut() {
        let payload = Payload::copied_files(["/tmp/x"], true);
        let gnome = payload.bytes_for(GNOME_COPIED_FILES).unwrap();
        let received = Received::new(GNOME_COPIED_FILES, gnome.to_vec());
        assert_eq!(received.files(), vec![PathBuf::from("/tmp/x")]);
        assert_eq!(received.is_cut(), Some(true));
        assert!(payload.bytes_for(KDE_CUT_SELECTION).is_some());
    }

    #[test]
    fn the_best_type_of_an_offer_is_picked() {
        let offered: Vec<String> = ["STRING", "text/plain", "image/jpeg", "image/png", URI_LIST]
            .iter()
            .map(|mime| mime.to_string())
            .collect();
        assert_eq!(Format::Text.best_mime(&offered), Some("text/plain"));
        assert_eq!(Format::Image.best_mime(&offered), Some("image/png"));
        assert_eq!(Format::Uris.best_mime(&offered), Some(URI_LIST));
        assert_eq!(Format::Html.best_mime(&offered), None);
    }

    #[test]
    fn files_are_offered_to_file_managers_and_text_editors() {
        let payload = Payload::files(["/tmp/a b"]);
        let text = payload.received(&Format::Text).unwrap().text();
        assert_eq!(text.as_deref(), Some("/tmp/a b"));
        let uris = payload.received(&Format::Uris).unwrap();
        assert_eq!(uris.files(), vec![PathBuf::from("/tmp/a b")]);
    }
}
