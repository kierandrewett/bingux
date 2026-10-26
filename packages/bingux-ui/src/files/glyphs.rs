//! Which icon stands for a place or a file.

use bingux_files::{Category, Entry, PlaceKind};

use crate::icons::Icon;

/// The sidebar icon of a place.
pub fn place_icon(kind: PlaceKind) -> Icon {
    match kind {
        PlaceKind::Recent => Icon::Clock,
        PlaceKind::Home => Icon::UserHome,
        PlaceKind::Desktop => Icon::Desktop,
        PlaceKind::Documents => Icon::FolderDocuments,
        PlaceKind::Downloads => Icon::FolderDownload,
        PlaceKind::Music => Icon::FolderMusic,
        PlaceKind::Pictures => Icon::FolderPictures,
        PlaceKind::Videos => Icon::FolderVideos,
        PlaceKind::Computer => Icon::DriveHarddisk,
        PlaceKind::Volume => Icon::MediaRemovable,
        PlaceKind::Bookmark => Icon::Bookmarks,
    }
}

/// The icon of a file or folder in a list.
pub fn entry_icon(entry: &Entry) -> Icon {
    match entry.category() {
        Category::Folder => Icon::Folder,
        Category::Image => Icon::ImageGeneric,
        Category::Video => Icon::VideoGeneric,
        Category::Audio => Icon::AudioXGeneric,
        Category::Archive => Icon::PackageXGeneric,
        Category::Text | Category::Document | Category::Code | Category::Other => {
            Icon::TextXGeneric
        }
    }
}
