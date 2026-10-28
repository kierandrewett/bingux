//! Which icon stands for a place or a file.

use bingux_files::mounts::MountKind;
use bingux_files::places::SpecialFolder;
use bingux_files::{Category, Entry, Place, PlaceKind};

use crate::icons::Icon;

/// The sidebar icon of a place.
pub fn place_icon(kind: PlaceKind) -> Icon {
    match kind {
        PlaceKind::Recent => Icon::Clock,
        PlaceKind::Home => Icon::UserHome,
        PlaceKind::Desktop => Icon::FolderDesktop,
        PlaceKind::Documents => Icon::FolderDocuments,
        PlaceKind::Downloads => Icon::FolderDownload,
        PlaceKind::Music => Icon::FolderMusic,
        PlaceKind::Pictures => Icon::FolderPictures,
        PlaceKind::Videos => Icon::FolderVideos,
        PlaceKind::Trash => Icon::UserTrash,
        PlaceKind::Computer => Icon::DriveHarddisk,
        PlaceKind::Volume => Icon::MediaRemovable,
        PlaceKind::Bookmark => Icon::Bookmarks,
    }
}

/// The icon of a folder. A user folder (Documents, Downloads...) has its own;
/// any other folder has the plain one. `special` comes from the path as a
/// person sees it, never from where a link leads.
pub fn folder_icon(special: Option<SpecialFolder>) -> Icon {
    match special {
        None => Icon::Folder,
        Some(SpecialFolder::Home) => Icon::UserHome,
        Some(SpecialFolder::Desktop) => Icon::FolderDesktop,
        Some(SpecialFolder::Documents) => Icon::FolderDocuments,
        Some(SpecialFolder::Downloads) => Icon::FolderDownload,
        Some(SpecialFolder::Music) => Icon::FolderMusic,
        Some(SpecialFolder::Pictures) => Icon::FolderPictures,
        Some(SpecialFolder::Videos) => Icon::FolderVideos,
        Some(SpecialFolder::Templates) => Icon::FolderTemplates,
        Some(SpecialFolder::PublicShare) => Icon::FolderPublicshare,
    }
}

/// The sidebar icon of a place. A bookmark is a folder, so it has a folder's
/// icon: the user folder's own when it is one (by its path, as a person sees
/// it), the plain folder otherwise.
pub fn sidebar_icon(place: &Place) -> Icon {
    match place.kind {
        PlaceKind::Bookmark => folder_icon(place.special()),
        kind => place_icon(kind),
    }
}

/// The icon of a drive of `kind`.
pub fn drive_icon(kind: MountKind) -> Icon {
    match kind {
        MountKind::Local | MountKind::Other => Icon::DriveHarddisk,
        MountKind::Removable => Icon::MediaRemovable,
        MountKind::Network => Icon::NetworkServer,
        MountKind::Optical => Icon::MediaOptical,
    }
}

/// The icon of a file or folder in a list.
pub fn entry_icon(entry: &Entry) -> Icon {
    category_icon(entry.category())
}

/// The icon of a kind of file, for a name that has no `Entry` yet, such as a
/// suggestion in the address bar.
pub fn category_icon(category: Category) -> Icon {
    match category {
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
