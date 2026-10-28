//! What the places list shows, as plain data: the groups and their order,
//! which row is selected, the drive usage line and the text that goes with
//! it. The views draw it; nothing here touches a window or a disk.

use std::path::Path;

use bingux_files::display;
use bingux_files::mounts::{Mount, MountKind, Usage, mount_for};
use bingux_files::volumes::Volume;
use bingux_files::{Place, PlaceKind, Places};

use super::directory::Where;
use super::glyphs::{drive_icon, sidebar_icon};
use crate::controls::USAGE_WARN;
use crate::icons::Icon;

/// What a row is, which decides its menu and its end button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ItemKind {
    /// Recent, Home and the user folders.
    Place,
    Bookmark,
    /// Computer and the mounted drives.
    Drive,
}

/// One row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub icon: Icon,
    pub label: String,
    pub target: Where,
    pub kind: ItemKind,
    /// The mount behind a drive row. Computer carries the mount of `/`.
    pub mount: Option<Mount>,
    /// The drive behind a row that is plugged in and not mounted yet.
    /// Opening the row mounts it.
    pub volume: Option<Volume>,
    /// Whether the folder is in the bookmarks already.
    pub bookmarked: bool,
}

impl Item {
    /// The folder, for a row that has one.
    pub fn path(&self) -> Option<&Path> {
        if self.volume.is_some() {
            return None;
        }
        match &self.target {
            Where::Folder(path) => Some(path),
            Where::Recent => None,
        }
    }
}

/// A group of rows under an optional heading.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Section {
    pub title: Option<&'static str>,
    pub items: Vec<Item>,
}

/// The groups, in the order a person looks for them: the places they use
/// every day, then their bookmarks, then the drives.
///
/// `places` supplies the standard places (and the name of Computer). The
/// drives come from `mounts`, which knows their sizes, so any volumes in
/// `places.devices` are ignored; `volumes` are the drives plugged in and not
/// mounted, listed after them. A mount of `/` is the Computer row. Recent
/// is left out when `recent` is false.
pub(crate) fn sections(
    places: &Places,
    bookmarks: &[Place],
    mounts: &[Mount],
    volumes: &[Volume],
    recent: bool,
) -> Vec<Section> {
    let bookmarked = |path: &Path| {
        bookmarks
            .iter()
            .any(|bookmark| bookmark.path.as_deref() == Some(path))
    };
    let place_item = |place: &Place, kind: ItemKind| Item {
        icon: sidebar_icon(place),
        label: place.name.clone(),
        target: match &place.path {
            Some(path) => Where::Folder(path.clone()),
            None => Where::Recent,
        },
        kind,
        mount: None,
        volume: None,
        bookmarked: place.path.as_deref().is_some_and(bookmarked),
    };

    let mut out = Vec::new();
    let standard: Vec<Item> = places
        .standard
        .iter()
        .filter(|place| recent || place.kind != PlaceKind::Recent)
        .map(|place| place_item(place, ItemKind::Place))
        .collect();
    if !standard.is_empty() {
        out.push(Section {
            title: None,
            items: standard,
        });
    }
    if !bookmarks.is_empty() {
        out.push(Section {
            title: Some("Bookmarks"),
            items: bookmarks
                .iter()
                .map(|place| place_item(place, ItemKind::Bookmark))
                .collect(),
        });
    }

    let root = Path::new("/");
    let computer = places
        .devices
        .iter()
        .find(|place| place.kind == PlaceKind::Computer);
    let mut drives = vec![Item {
        icon: drive_icon(MountKind::Local),
        label: computer.map_or_else(bingux_files::location::host_name, |place| place.name.clone()),
        target: Where::Folder(root.to_path_buf()),
        kind: ItemKind::Drive,
        mount: mounts
            .iter()
            .find(|mount| mount.mount_point == root)
            .cloned(),
        volume: None,
        bookmarked: bookmarked(root),
    }];
    drives.extend(
        mounts
            .iter()
            .filter(|mount| mount.mount_point != root)
            .map(|mount| Item {
                icon: drive_icon(mount.kind),
                label: mount.label.clone(),
                target: Where::Folder(mount.mount_point.clone()),
                kind: ItemKind::Drive,
                mount: Some(mount.clone()),
                volume: None,
                bookmarked: bookmarked(&mount.mount_point),
            }),
    );
    drives.extend(volumes.iter().map(|volume| Item {
        icon: drive_icon(volume.kind),
        label: volume.label.clone(),
        // Not a folder: the device, so two volumes never look alike.
        target: Where::Folder(volume.device.clone().into()),
        kind: ItemKind::Drive,
        mount: None,
        volume: Some(volume.clone()),
        bookmarked: false,
    }));
    out.push(Section {
        title: Some("Drives"),
        items: drives,
    });
    out
}

/// The row to select for `current`, as (section, row). A row that is exactly
/// the folder wins; otherwise the drive the folder is on, by the mount table
/// and the path's text, so no folder is looked at.
pub(crate) fn selected(
    sections: &[Section],
    current: &Where,
    mounts: &[Mount],
) -> Option<(usize, usize)> {
    let find = |matches: &dyn Fn(&Item) -> bool| {
        sections.iter().enumerate().find_map(|(section, group)| {
            group
                .items
                .iter()
                .position(matches)
                .map(|item| (section, item))
        })
    };
    if let Some(found) = find(&|item| item.target == *current) {
        return Some(found);
    }
    let Where::Folder(path) = current else {
        return None;
    };
    let root = Path::new("/");
    let mount_point = mount_for(path, mounts).map_or(root, |mount| mount.mount_point.as_path());
    find(&|item| item.kind == ItemKind::Drive && item.path() == Some(mount_point))
}

/// What a drive row says under its name.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UsageLine {
    /// Nothing: not a drive, or one that reports no size.
    Nothing,
    /// The size is being read. The row keeps the room the line will take, so
    /// it does not grow when the figure arrives.
    Reading,
    Known {
        fraction: f32,
        text: String,
        warn: bool,
    },
    NotResponding,
    /// One line of words and no bar: a drive that is not mounted yet.
    Text(String),
}

/// `Not mounted, 16 GB`.
pub(crate) fn volume_text(volume: &Volume) -> String {
    format!("Not mounted \u{b7} {}", display::size(volume.size))
}

/// `230 GB free of 465 GB`.
pub(crate) fn usage_text(free: u64, total: u64) -> String {
    format!("{} free of {}", display::size(free), display::size(total))
}

/// The usage line of `mount`. `settled` is whether the sizes have been asked
/// for: an unknown size is still "being read" before that and nothing after.
pub(crate) fn usage_line(mount: Option<&Mount>, settled: bool) -> UsageLine {
    let Some(mount) = mount else {
        return UsageLine::Nothing;
    };
    match mount.usage {
        Usage::Known { total, free } if total > 0 => {
            let fraction = (total.saturating_sub(free)) as f32 / total as f32;
            UsageLine::Known {
                fraction,
                text: usage_text(free, total),
                warn: fraction >= USAGE_WARN,
            }
        }
        Usage::Known { .. } => UsageLine::Nothing,
        Usage::Pending => UsageLine::Reading,
        Usage::Unknown if !settled => UsageLine::Reading,
        Usage::Unknown => UsageLine::Nothing,
        Usage::NotResponding => UsageLine::NotResponding,
    }
}

/// How a drive row's end button lets go of a drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Release {
    /// Unmount and, where the drive allows it, power it off.
    Eject,
    Unmount,
}

impl Release {
    pub fn label(self) -> &'static str {
        match self {
            Release::Eject => "Eject",
            Release::Unmount => "Unmount",
        }
    }
}

/// The way to let go of `mount`, or `None` for a drive the system lives on.
pub(crate) fn release_for(mount: &Mount) -> Option<Release> {
    if !mount.can_unmount() {
        return None;
    }
    Some(match mount.kind {
        MountKind::Removable | MountKind::Optical => Release::Eject,
        MountKind::Local | MountKind::Network | MountKind::Other => Release::Unmount,
    })
}

/// A fresh mount table with the sizes of the table it replaces carried over,
/// so a drive appearing does not blank the bars of the others.
pub(crate) fn carry_usage(mut fresh: Vec<Mount>, old: &[Mount]) -> Vec<Mount> {
    for mount in &mut fresh {
        if !matches!(mount.usage, Usage::Unknown | Usage::Pending) {
            continue;
        }
        if let Some(before) = old
            .iter()
            .find(|before| before.mount_point == mount.mount_point && before.device == mount.device)
            && matches!(before.usage, Usage::Known { .. } | Usage::NotResponding)
        {
            mount.usage = before.usage;
        }
    }
    fresh
}

/// The row a typed prefix jumps to, among `labels`. One letter typed again
/// moves on to the next row with that letter; a longer prefix stays on a row
/// that still matches. Wraps around.
pub(crate) fn type_ahead(labels: &[String], from: Option<usize>, typed: &str) -> Option<usize> {
    let wanted = typed.to_lowercase();
    if wanted.is_empty() || labels.is_empty() {
        return None;
    }
    let single = wanted.chars().count() == 1;
    let start = match from {
        Some(index) if single => index + 1,
        Some(index) => index,
        None => 0,
    };
    (0..labels.len())
        .map(|step| (start + step) % labels.len())
        .find(|&index| labels[index].to_lowercase().starts_with(&wanted))
}

/// The name of the place `current` is, for the phone's places button: the
/// selected row, else the folder's own name.
pub(crate) fn current_name(current: &Where, row: Option<&Item>) -> String {
    match (row, current) {
        (Some(item), _) => item.label.clone(),
        (None, Where::Recent) => "Recent".to_string(),
        (None, Where::Folder(path)) => path.file_name().map_or_else(
            bingux_files::location::host_name,
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn mount(point: &str, label: &str, kind: MountKind, usage: Usage) -> Mount {
        Mount {
            id: point.len() as u32,
            label: label.into(),
            device: format!("/dev/{label}"),
            mount_point: point.into(),
            fs_type: "ext4".into(),
            kind,
            usage,
            read_only: false,
            mounted_by_user: kind == MountKind::Removable,
            device_number: (0, 0),
            visible: true,
        }
    }

    fn place(kind: PlaceKind, name: &str, path: Option<&str>) -> Place {
        Place {
            kind,
            name: name.into(),
            path: path.map(PathBuf::from),
        }
    }

    fn sample() -> Places {
        Places {
            standard: vec![
                place(PlaceKind::Recent, "Recent", None),
                place(PlaceKind::Home, "Home", Some("/home/a")),
                place(PlaceKind::Documents, "Documents", Some("/home/a/Documents")),
            ],
            devices: vec![
                place(PlaceKind::Computer, "Computer", Some("/")),
                place(PlaceKind::Volume, "stale", Some("/mnt/stale")),
            ],
            bookmarks: Vec::new(),
        }
    }

    fn known(total: u64, free: u64) -> Usage {
        Usage::Known { total, free }
    }

    #[test]
    fn groups_come_in_order_and_ignore_model_volumes() {
        let bookmarks = vec![place(PlaceKind::Bookmark, "Work", Some("/srv/work"))];
        let mounts = vec![
            mount("/", "root", MountKind::Local, known(100, 50)),
            mount("/mnt/nas", "nas", MountKind::Network, Usage::Unknown),
        ];
        let out = sections(&sample(), &bookmarks, &mounts, &[], true);
        let titles: Vec<_> = out.iter().map(|section| section.title).collect();
        assert_eq!(titles, [None, Some("Bookmarks"), Some("Drives")]);
        let drives: Vec<_> = out[2]
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(drives, ["Computer", "nas"]);
        assert_eq!(
            out[2].items[0]
                .mount
                .as_ref()
                .map(|m| m.mount_point.clone()),
            Some(PathBuf::from("/"))
        );
    }

    #[test]
    fn recent_is_left_out_on_request_and_empty_groups_vanish() {
        let out = sections(&sample(), &[], &[], &[], false);
        assert_eq!(out[0].items.len(), 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].title, Some("Drives"));
    }

    #[test]
    fn trash_is_a_standard_row_with_the_trash_icon() {
        let mut places = sample();
        places.standard.push(place(
            PlaceKind::Trash,
            "Trash",
            Some("/home/a/.local/share/Trash/files"),
        ));
        let out = sections(&places, &[], &[], &[], true);
        let trash = out[0].items.last().unwrap();
        assert_eq!(trash.label, "Trash");
        assert_eq!(trash.icon, Icon::UserTrash);
        assert_eq!(
            trash.path(),
            Some(Path::new("/home/a/.local/share/Trash/files"))
        );
    }

    #[test]
    fn a_folder_inside_a_drive_selects_the_drive() {
        let mounts = vec![
            mount("/", "root", MountKind::Local, Usage::Unknown),
            mount("/mnt/nas", "nas", MountKind::Network, Usage::Unknown),
        ];
        let out = sections(&sample(), &[], &mounts, &[], true);
        let at = |path: &str| selected(&out, &Where::Folder(path.into()), &mounts);
        assert_eq!(at("/home/a/Documents"), Some((0, 2)));
        assert_eq!(at("/mnt/nas/deep/er"), Some((1, 1)));
        assert_eq!(at("/var/log"), Some((1, 0)));
        assert_eq!(selected(&out, &Where::Recent, &mounts), Some((0, 0)));
    }

    #[test]
    fn usage_line_shows_figures_warns_and_reserves_room_until_settled() {
        let line =
            |usage, settled| usage_line(Some(&mount("/x", "x", MountKind::Local, usage)), settled);
        assert_eq!(line(Usage::Unknown, false), UsageLine::Reading);
        assert_eq!(line(Usage::Pending, true), UsageLine::Reading);
        assert_eq!(line(Usage::Unknown, true), UsageLine::Nothing);
        assert_eq!(line(Usage::NotResponding, true), UsageLine::NotResponding);
        assert_eq!(line(known(0, 0), true), UsageLine::Nothing);
        match line(known(500_000_000_000, 120_000_000_000), true) {
            UsageLine::Known {
                text,
                warn,
                fraction,
            } => {
                assert_eq!(text, "120 GB free of 500 GB");
                assert!(!warn);
                assert!((fraction - 0.76).abs() < 1e-6);
            }
            other => panic!("{other:?}"),
        }
        match line(known(1000, 100), true) {
            UsageLine::Known { warn, .. } => assert!(warn, "90% full warns"),
            other => panic!("{other:?}"),
        }
        match line(known(1000, 101), true) {
            UsageLine::Known { warn, .. } => assert!(!warn, "just under 90% does not"),
            other => panic!("{other:?}"),
        }
        assert_eq!(usage_line(None, true), UsageLine::Nothing);
    }

    #[test]
    fn usage_text_is_short() {
        assert_eq!(usage_text(0, 1_000_000_000_000), "0 bytes free of 1.0 TB");
        assert_eq!(
            usage_text(12_300_000_000, 64_000_000_000),
            "12 GB free of 64 GB"
        );
    }

    #[test]
    fn only_drives_a_person_may_release_get_a_button() {
        let root = mount("/", "root", MountKind::Local, Usage::Unknown);
        let usb = mount(
            "/run/media/a/usb",
            "usb",
            MountKind::Removable,
            Usage::Unknown,
        );
        let disc = mount(
            "/run/media/a/disc",
            "disc",
            MountKind::Optical,
            Usage::Unknown,
        );
        let nas = mount("/mnt/nas", "nas", MountKind::Network, Usage::Unknown);
        let home = mount("/home", "home", MountKind::Local, Usage::Unknown);
        assert_eq!(release_for(&root), None);
        assert_eq!(release_for(&home), None);
        assert_eq!(release_for(&usb), Some(Release::Eject));
        assert_eq!(release_for(&disc), Some(Release::Eject));
        assert_eq!(release_for(&nas), Some(Release::Unmount));
    }

    #[test]
    fn a_new_table_keeps_the_sizes_already_known() {
        let old = vec![mount("/a", "a", MountKind::Local, known(10, 5))];
        let fresh = vec![
            mount("/a", "a", MountKind::Local, Usage::Unknown),
            mount("/b", "b", MountKind::Local, Usage::Unknown),
        ];
        let out = carry_usage(fresh, &old);
        assert_eq!(out[0].usage, known(10, 5));
        assert_eq!(out[1].usage, Usage::Unknown);
    }

    #[test]
    fn type_ahead_cycles_one_letter_and_extends_a_prefix() {
        let labels: Vec<String> = ["Recent", "Documents", "Downloads", "Music"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(type_ahead(&labels, None, "d"), Some(1));
        assert_eq!(type_ahead(&labels, Some(1), "d"), Some(2));
        assert_eq!(type_ahead(&labels, Some(2), "d"), Some(1));
        assert_eq!(type_ahead(&labels, Some(1), "dow"), Some(2));
        assert_eq!(type_ahead(&labels, Some(1), "doc"), Some(1));
        assert_eq!(type_ahead(&labels, Some(1), "x"), None);
    }
}
