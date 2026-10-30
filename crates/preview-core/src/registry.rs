//! Previewers and the registry that picks them.
//!
//! A [`Previewer`] knows one family of content. It says how well it handles a
//! sniffed source ([`Previewer::claim`]) and reads what its viewer needs
//! ([`Previewer::load`]), cancellable and off the UI thread. The result is
//! data ([`Loaded`]); the Freya viewer in `bingux_ui::preview` draws it.
//!
//! [`Registry::chain`] answers with every viewer that claims the source, best
//! first. The host shows the first; when it fails (a corrupt file, a missing
//! decoder, a helper that died) it moves to the next, and the last is always
//! [`ViewerId::Card`], so nothing is unpreviewable.
//!
//! Pixels of video and PDF pages never travel inside [`Loaded`]: the `Media`
//! and `Document` previewers only read what the header says, and the player
//! and page renderer arrive through the host protocol (see
//! `design/preview.md`).

use bingux_files::Cancel;

use crate::archive::{self, ArchiveListing};
use crate::font::{self, FontFacts};
use crate::media::{self, AvInfo, ImageInfo};
use crate::sniff::{Content, Sniffed};
use crate::source::PreviewSource;
use crate::text::{self, TextError, TextWindow};

/// Which viewer. Each is a Freya component in `bingux_ui::preview`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewerId {
    Image,
    Media,
    Document,
    Text,
    Archive,
    Font,
    Folder,
    /// The type's icon and the file's facts: the viewer of last resort.
    Card,
}

/// How well a previewer handles a source: higher wins.
pub type Priority = u8;

/// What a viewer needs, read from the source.
#[derive(Clone, Debug, PartialEq)]
pub enum Loaded {
    Image(Option<ImageInfo>),
    Media(Option<AvInfo>),
    Document,
    Text(TextWindow),
    Archive(ArchiveListing),
    Font(FontFacts),
    Folder,
    Card,
}

/// Why a previewer gave up, in words for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Cancelled,
    /// The data is not what the previewer shows.
    Unsupported(String),
    Unreadable(String),
}

impl LoadError {
    pub fn message(&self) -> &str {
        match self {
            LoadError::Cancelled => "The preview was cancelled.",
            LoadError::Unsupported(message) | LoadError::Unreadable(message) => message,
        }
    }
}

impl From<TextError> for LoadError {
    fn from(error: TextError) -> Self {
        match error {
            TextError::Binary => LoadError::Unsupported("This file holds binary data, not text.".to_owned()),
            TextError::Cancelled => LoadError::Cancelled,
            TextError::Unreadable(message) => LoadError::Unreadable(message),
        }
    }
}

/// One family of content.
pub trait Previewer: Send + Sync {
    fn id(&self) -> ViewerId;
    /// How well this previewer handles `sniffed`, or `None` for not at all.
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority>;
    /// What the viewer needs from `source`. Blocks on the disk, stops at its
    /// next read once `cancel` is set.
    fn load(&self, source: &PreviewSource, sniffed: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError>;
}

/// The previewers, by priority.
pub struct Registry {
    previewers: Vec<Box<dyn Previewer>>,
}

impl Registry {
    /// The previewers Bingux ships.
    pub fn standard() -> Registry {
        Registry {
            previewers: vec![
                Box::new(Folder),
                Box::new(Image),
                Box::new(Media),
                Box::new(Document),
                Box::new(Text),
                Box::new(Archive),
                Box::new(Font),
                Box::new(Card),
            ],
        }
    }

    /// An empty registry, for tests and for apps that show one kind.
    pub fn with(previewers: Vec<Box<dyn Previewer>>) -> Registry {
        Registry { previewers }
    }

    /// Every previewer that claims `sniffed`, best first. Ties keep
    /// registration order.
    pub fn chain(&self, sniffed: &Sniffed) -> Vec<&dyn Previewer> {
        let mut claimed: Vec<(Priority, &dyn Previewer)> = self
            .previewers
            .iter()
            .filter_map(|previewer| previewer.claim(sniffed).map(|priority| (priority, previewer.as_ref())))
            .collect();
        claimed.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
        claimed.into_iter().map(|(_, previewer)| previewer).collect()
    }

    /// The previewer of `id`, if registered.
    pub fn get(&self, id: ViewerId) -> Option<&dyn Previewer> {
        self.previewers.iter().find(|previewer| previewer.id() == id).map(|previewer| previewer.as_ref())
    }
}

struct Folder;
struct Image;
struct Media;
struct Document;
struct Text;
struct Archive;
struct Font;
struct Card;

impl Previewer for Folder {
    fn id(&self) -> ViewerId {
        ViewerId::Folder
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        (sniffed.content == Content::Folder).then_some(100)
    }
    fn load(&self, _: &PreviewSource, _: &Sniffed, _: &Cancel) -> Result<Loaded, LoadError> {
        Ok(Loaded::Folder)
    }
}

impl Previewer for Image {
    fn id(&self) -> ViewerId {
        ViewerId::Image
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        matches!(sniffed.content, Content::Image | Content::Svg).then_some(90)
    }
    fn load(&self, source: &PreviewSource, _: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError> {
        if cancel.is_cancelled() {
            return Err(LoadError::Cancelled);
        }
        let info = source.open().ok().and_then(media::image_info_from);
        Ok(Loaded::Image(info))
    }
}

impl Previewer for Media {
    fn id(&self) -> ViewerId {
        ViewerId::Media
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        matches!(sniffed.content, Content::Audio | Content::Video).then_some(90)
    }
    fn load(&self, source: &PreviewSource, _: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError> {
        if cancel.is_cancelled() {
            return Err(LoadError::Cancelled);
        }
        Ok(Loaded::Media(source.open().ok().and_then(media::av_info_from)))
    }
}

impl Previewer for Document {
    fn id(&self) -> ViewerId {
        ViewerId::Document
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        (sniffed.content == Content::Pdf).then_some(90)
    }
    fn load(&self, _: &PreviewSource, _: &Sniffed, _: &Cancel) -> Result<Loaded, LoadError> {
        Ok(Loaded::Document)
    }
}

impl Previewer for Text {
    fn id(&self) -> ViewerId {
        ViewerId::Text
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        match sniffed.content {
            Content::Text => Some(80),
            // A vector picture is text when the picture viewer cannot show it.
            Content::Svg => Some(40),
            _ => None,
        }
    }
    fn load(&self, source: &PreviewSource, _: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError> {
        Ok(Loaded::Text(text::read_window(source, 0, text::HEAD_LIMIT, cancel)?))
    }
}

impl Previewer for Archive {
    fn id(&self) -> ViewerId {
        ViewerId::Archive
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        (sniffed.content == Content::Archive).then_some(80)
    }
    fn load(&self, source: &PreviewSource, _: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError> {
        archive::list(source, cancel).map(Loaded::Archive)
    }
}

impl Previewer for Font {
    fn id(&self) -> ViewerId {
        ViewerId::Font
    }
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority> {
        (sniffed.content == Content::Font).then_some(80)
    }
    fn load(&self, source: &PreviewSource, _: &Sniffed, cancel: &Cancel) -> Result<Loaded, LoadError> {
        font::read(source, cancel).map(Loaded::Font)
    }
}

impl Previewer for Card {
    fn id(&self) -> ViewerId {
        ViewerId::Card
    }
    fn claim(&self, _: &Sniffed) -> Option<Priority> {
        Some(1)
    }
    fn load(&self, _: &PreviewSource, _: &Sniffed, _: &Cancel) -> Result<Loaded, LoadError> {
        Ok(Loaded::Card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sniff::sniff_head;

    fn chain(name: &str, head: &[u8]) -> Vec<ViewerId> {
        let sniffed = sniff_head(name, head);
        Registry::standard().chain(&sniffed).iter().map(|previewer| previewer.id()).collect()
    }

    #[test]
    fn every_chain_ends_in_the_card() {
        for (name, head) in [
            ("a.png", &b"\x89PNG\r\n\x1a\n"[..]),
            ("a.pdf", b"%PDF-1.4"),
            ("a.mp4", b"\0\0\0\x18ftypisom"),
            ("a.txt", b"text"),
            ("a.zip", b"PK\x03\x04"),
            ("a.bin7", b"\0\0\0"),
        ] {
            assert_eq!(chain(name, head).last(), Some(&ViewerId::Card), "{name}");
        }
    }

    #[test]
    fn the_best_viewer_comes_first() {
        assert_eq!(chain("a.png", b"\x89PNG\r\n\x1a\n"), [ViewerId::Image, ViewerId::Card]);
        assert_eq!(chain("a.pdf", b"%PDF-1.4"), [ViewerId::Document, ViewerId::Card]);
        assert_eq!(chain("a.mp4", b"\0\0\0\x18ftypisom"), [ViewerId::Media, ViewerId::Card]);
        assert_eq!(chain("a.mp3", b"ID3\x04"), [ViewerId::Media, ViewerId::Card]);
        assert_eq!(chain("a.zip", b"PK\x03\x04"), [ViewerId::Archive, ViewerId::Card]);
        assert_eq!(chain("a.docx", b"PK\x03\x04"), [ViewerId::Card]);
        assert_eq!(chain("notes.txt", b"words"), [ViewerId::Text, ViewerId::Card]);
        assert_eq!(chain("a.ttf", b"\0\x01\0\0"), [ViewerId::Font, ViewerId::Card]);
    }

    #[test]
    fn a_vector_picture_falls_back_to_its_source() {
        assert_eq!(chain("logo.svg", b"<svg/>"), [ViewerId::Image, ViewerId::Text, ViewerId::Card]);
    }

    #[test]
    fn unknown_bytes_are_text_or_a_card() {
        assert_eq!(chain("mystery", b"hello"), [ViewerId::Text, ViewerId::Card]);
        assert_eq!(chain("mystery", b"\0\x01\x02"), [ViewerId::Card]);
    }

    #[test]
    fn folders_have_their_own_viewer() {
        let sniffed = Sniffed {
            mime: bingux_files::mime::DIRECTORY,
            content: Content::Folder,
        };
        let ids: Vec<ViewerId> = Registry::standard().chain(&sniffed).iter().map(|p| p.id()).collect();
        assert_eq!(ids, [ViewerId::Folder, ViewerId::Card]);
    }

    #[test]
    fn a_custom_registry_orders_by_priority() {
        struct Loud(ViewerId, Priority);
        impl Previewer for Loud {
            fn id(&self) -> ViewerId {
                self.0
            }
            fn claim(&self, _: &Sniffed) -> Option<Priority> {
                Some(self.1)
            }
            fn load(&self, _: &PreviewSource, _: &Sniffed, _: &Cancel) -> Result<Loaded, LoadError> {
                Ok(Loaded::Card)
            }
        }
        let registry = Registry::with(vec![Box::new(Loud(ViewerId::Text, 10)), Box::new(Loud(ViewerId::Image, 50))]);
        let sniffed = sniff_head("a", b"x");
        let ids: Vec<ViewerId> = registry.chain(&sniffed).iter().map(|p| p.id()).collect();
        assert_eq!(ids, [ViewerId::Image, ViewerId::Text]);
    }
}
