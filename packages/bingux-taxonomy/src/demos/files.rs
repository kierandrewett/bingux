//! Files: the file views, the address bar, the places sidebar and the chooser.
//!
//! The views read a real folder. The taxonomy makes one of its own, filled
//! with sample files, under the temporary directory (see [`fixture`]), so the
//! pages look the same on every machine and never show the user's files.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use bingux_ui::files::model::{Places, Selection, Sort};
use bingux_ui::files::{
    AddressBar, BarStatus, Browse, ChooserRequest, Find, SearchBar, ColourIcon, FileIcon, FileKind, FileView, Load, PlacesMenu, PlacesSidebar,
    TabMemory, Where, browse, choose, colour_icon_view, default_zoom_px, empty_state, error_state, file_icon_view,
    home_folder, loading_state, skeleton, use_directory,
};
use bingux_ui::preview::{
    ControlState, DocumentControls, DocumentState, MediaControls, Mode, PreviewDock, PreviewHost, PreviewNav, PreviewSource, Step,
};
use bingux_ui::shell::use_overlays;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

/// The sample files: `(path under the fixture, bytes)`. A trailing `/` is a
/// folder. The SVGs are real images, so thumbnails and previews have
/// something to draw.
const SAMPLES: &[(&str, &[u8])] = &[
    ("Documents/Taxes/", b""),
    ("Documents/Photos/", b""),
    ("Documents/Empty/", b""),
    ("Documents/Budget 2026.xlsx", &[0; 18_432]),
    (
        "Documents/Notes.txt",
        b"Bingux files\n\nA plain text file the preview can show.\n",
    ),
    ("Documents/readme.md", b"# Sample\n\nA markdown file.\n"),
    ("Documents/Scan 003.pdf", &[0; 2_097_152]),
    ("Documents/.hidden-config", b"hidden = true\n"),
    (
        "Documents/A file name that goes on and on for the overflow state of the name column.txt",
        b"long\n",
    ),
    (
        "Documents/Photos/network.svg",
        include_bytes!("../../../bingux-ui/icons/network-wireless-symbolic.svg"),
    ),
    (
        "Documents/Photos/sound.svg",
        include_bytes!("../../../bingux-ui/icons/audio-volume-high-symbolic.svg"),
    ),
    (
        "Documents/Photos/folder.svg",
        include_bytes!("../../../bingux-ui/icons/folder-symbolic.svg"),
    ),
    ("Documents/Taxes/2025.pdf", &[0; 8192]),
    ("Music/Track 01.mp3", &[0; 50_000]),
    ("Music/Track 02.mp3", &[0; 60_000]),
    // Made below: a film needs an encoder, a PDF and a song are written by hand.
    ("Downloads/installer.rpm", &[0; 1_000_000]),
];

/// The fixture home: made once, from the samples.
static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| {
    let root = std::env::temp_dir().join(format!("bingux-taxonomy-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, bytes) in SAMPLES {
        let target = root.join(path);
        if path.ends_with('/') {
            let _ = std::fs::create_dir_all(&target);
        } else {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&target, bytes);
        }
    }
    let _ = std::fs::write(root.join("Documents/Quarterly report.pdf"), bingux_preview_core::testing::three_page_pdf());
    let _ = std::fs::create_dir_all(root.join("Videos"));
    let _ = std::fs::write(root.join("Music/Tone.wav"), bingux_preview_core::testing::wav_tone(4));
    make_film(&root.join("Videos/Holiday clip.webm"));
    root
});

/// A four second clip with a ball and a tone, made with `gst-launch-1.0` when
/// the machine has it. Without it there is no film in the fixture, and the
/// film specimens show the card of a missing file: they never pretend.
fn make_film(path: &Path) {
    let _ = std::process::Command::new("gst-launch-1.0")
        .arg("-q")
        .args(["videotestsrc", "pattern=ball", "num-buffers=120", "!", "video/x-raw,width=480,height=270,framerate=30/1", "!", "videoconvert", "!"])
        .args(["vp8enc", "deadline=1", "keyframe-max-dist=15", "!", "webmmux", "name=mux", "!", "filesink"])
        .arg(format!("location={}", path.display()))
        .args(["audiotestsrc", "num-buffers=190", "samplesperbuffer=1024", "!", "audioconvert", "!", "vorbisenc", "!", "mux."])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The fixture home folder.
pub fn fixture() -> &'static Path {
    &FIXTURE
}

fn documents() -> Where {
    Where::Folder(fixture().join("Documents"))
}

/// One folder in a [`FileView`], zoomed to `px`, with nothing picked or the
/// first file picked. Grid or list follows the zoom level, as it does in the
/// chooser.
#[derive(PartialEq)]
struct FolderView {
    at: Where,
    px: u16,
    picked: bool,
    sort: Sort,
    show_hidden: bool,
}

impl Component for FolderView {
    fn render(&self) -> impl IntoElement {
        let (directory, _reload) = use_directory(&self.at);
        let level = use_state(|| self.px);
        let mut selection = use_state(Selection::new);
        let loaded = directory.read(&self.at);
        match loaded {
            Load::Loading => skeleton(self.px, specimen_viewport(LIST_HEIGHT)),
            Load::Failed(error) => error_state(&error),
            Load::Ready(listing) => {
                let mut order: Vec<usize> = listing
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| self.show_hidden || !entry.hidden)
                    .map(|(index, _)| index)
                    .collect();
                self.sort.order(&listing.entries, &mut order);
                if self.picked
                    && selection.peek().is_empty()
                    && let Some(first) = order.first()
                {
                    selection.write().select_only(&listing.entries[*first].path);
                }
                let placeholder = order.is_empty().then(|| empty_state(false, false, false));
                let mut view = FileView::new(listing.clone(), Arc::new(order), selection)
                    .icon_px(level)
                    .sort(self.sort)
                    .multiple(true)
                    .show_hidden(self.show_hidden)
                    .thumbnails(true);
                if let Some(placeholder) = placeholder {
                    view = view.placeholder(placeholder);
                }
                view.into_element()
            }
        }
    }
}

fn folder(at: Where, px: u16) -> Element {
    FolderView {
        at,
        px,
        picked: false,
        sort: Sort::default(),
        show_hidden: false,
    }
    .into_element()
}

/// The viewport a placeholder specimen draws into: the width of the stage and
/// `height` px.
fn specimen_viewport(height: f32) -> Area {
    Area::new((0.0, 0.0).into(), (720.0, height).into())
}

const LIST_HEIGHT: f32 = 340.0;

/// The nine zoom levels of the ladder: the first four are list rows, the rest
/// are grids of larger and larger tiles.
pub const FILE_VIEW: &[Spec] = &[
    Spec::live("List, the default zoom (16 px icons; 32 px on a phone)", || {
        folder(documents(), default_zoom_px())
    })
    .page(LIST_HEIGHT),
    Spec::live("List, 20 px", || folder(documents(), 20)).page(LIST_HEIGHT),
    Spec::live("List, 24 px", || folder(documents(), 24)).page(LIST_HEIGHT),
    Spec::live("List, 32 px", || folder(documents(), 32)).page(LIST_HEIGHT),
    Spec::live("Grid, 48 px", || folder(documents(), 48)).page(LIST_HEIGHT),
    Spec::live("Grid, 64 px", || folder(documents(), 64)).page(LIST_HEIGHT),
    Spec::live("Grid, 96 px", || folder(documents(), 96)).page(LIST_HEIGHT),
    Spec::live("Grid, 128 px", || folder(documents(), 128)).page(LIST_HEIGHT),
    Spec::live("Grid, 192 px", || folder(documents(), 192)).page(LIST_HEIGHT),
    Spec::forced("A file picked", || {
        FolderView {
            at: documents(),
            px: 16,
            picked: true,
            sort: Sort::default(),
            show_hidden: false,
        }
        .into_element()
    })
    .page(LIST_HEIGHT),
    Spec::forced("Hidden files shown", || {
        FolderView {
            at: documents(),
            px: 16,
            picked: false,
            sort: Sort::default(),
            show_hidden: true,
        }
        .into_element()
    })
    .page(LIST_HEIGHT),
    Spec::forced("Empty folder", || {
        folder(Where::Folder(fixture().join("Documents/Empty")), 16)
    })
    .page(200.0),
    Spec::forced("Folder that cannot be read", || {
        folder(
            Where::Folder(PathBuf::from("/nonexistent-bingux-taxonomy")),
            16,
        )
    })
    .page(200.0),
    Spec::forced("Loading", || {
        vstack(S2)
            .child(loading_state())
            .child(skeleton(16, specimen_viewport(180.0)))
            .into_element()
    })
    .page(260.0),
    Spec::live("Right-click a file for its menu", || {
        folder(documents(), 16)
    })
    .page(LIST_HEIGHT)
    .script("wait 600; rclick-text Notes.txt; wait 400; save {out}"),
];

/// The icon sizes the file view zooms through, and the ones between that a row
/// or a tile also draws.
const ICON_SIZES: [u16; 6] = [16, 20, 24, 32, 64, 128];

/// One icon over its name.
fn labelled(tokens: &Tokens, icon: &FileIcon, size: u16, selected: bool, name: String) -> Element {
    vstack(S1)
        .tw_layout(&[items(Align::Center)])
        .child(file_icon_view(tokens, icon, size, selected))
        .child(caption(name))
        .into_element()
}

/// A wrapping line of icons.
fn flow(tokens: &Tokens) -> Rect {
    rect().tw(tokens, &[w_full(), flex_row(), wrap(S3), gap(S3), items(Align::End)])
}

/// The icon every surface draws: each kind of file, at each size the view
/// zooms through, resting and selected, then the folders a path decides, the
/// places and drives, and the link emblem and hidden look.
pub const FILE_ICON: &[Spec] = &[
    Spec::forced("Beside the Files app icon: folders, the bin and pages, drawn from the vector at 128 and 64 px", || {
        let tokens = use_tokens();
        let mut page = vstack(S4).tw_layout(&[w_full()]);
        for px in [128.0, 64.0] {
            let tile = |picture: Element, name: &str| -> Element {
                vstack(S1).tw_layout(&[items(Align::Center)]).child(picture).child(caption(name.to_owned())).into_element()
            };
            let app = SvgViewer::new(include_bytes!("../../../bingux-files-app/bingux-files.svg"))
                .width(Size::px(px))
                .height(Size::px(px));
            let mut line = flow(&tokens).child(tile(app.into(), "Files app icon"));
            for icon in [
                ColourIcon::Folder,
                ColourIcon::FolderDocuments,
                ColourIcon::FolderPictures,
                ColourIcon::TrashFull,
                ColourIcon::FilePdf,
                ColourIcon::FileSpreadsheet,
                ColourIcon::FileCode,
                ColourIcon::FileImage,
                ColourIcon::FileAudio,
                ColourIcon::FileArchive,
            ] {
                line = line.child(tile(colour_icon_view(&tokens, icon, px), icon.name()));
            }
            page = page.child(caption(format!("{px} px"))).child(line);
        }
        page.into_element()
    })
    .page(900.0),
    Spec::forced("Every kind in colour at 16, 20, 24, 32, 64 and 128 px, resting and selected", || {
        let tokens = use_tokens();
        let mut page = vstack(S4).tw_layout(&[w_full()]);
        for px in ICON_SIZES {
            for (selected, state) in [(false, "resting"), (true, "selected")] {
                let mut line = flow(&tokens);
                for kind in FileKind::ALL {
                    line = line.child(labelled(
                        &tokens,
                        &FileIcon::for_kind(kind),
                        px,
                        selected,
                        kind.label().to_owned(),
                    ));
                }
                page = page
                    .child(caption(format!("{px} px, {state}")))
                    .child(line);
            }
        }
        page.into_element()
    })
    .page(4200.0),
    Spec::forced("Folders: the accent, resting, selected and hidden", || {
        use bingux_ui::files::model::places::SpecialFolder;
        let tokens = use_tokens();
        let mut page = vstack(S4).tw_layout(&[w_full()]);
        for px in [16, 32, 64, 128] {
            for (state, selected, hidden) in [("resting", false, false), ("selected", true, false), ("hidden", false, true)] {
                let mut line = flow(&tokens);
                let mut icons: Vec<(String, FileIcon)> = vec![("Folder".to_owned(), FileIcon::for_kind(FileKind::Folder))];
                for special in [
                    SpecialFolder::Home,
                    SpecialFolder::Desktop,
                    SpecialFolder::Documents,
                    SpecialFolder::Downloads,
                    SpecialFolder::Music,
                    SpecialFolder::Pictures,
                    SpecialFolder::Videos,
                    SpecialFolder::Templates,
                    SpecialFolder::PublicShare,
                ] {
                    icons.push((format!("{special:?}"), FileIcon::for_special(special)));
                }
                icons.push(("Trash, empty".to_owned(), FileIcon::for_trash(false)));
                icons.push(("Trash, items".to_owned(), FileIcon::for_trash(true)));
                for (name, icon) in icons {
                    line = line.child(labelled(&tokens, &icon.hidden(hidden), px, selected, name));
                }
                page = page.child(caption(format!("{px} px, {state}"))).child(line);
            }
        }
        page.into_element()
    })
    .page(2600.0),
    Spec::forced("The colour set: every icon, by name, at 16, 24, 32 and 64 px", || {
        let tokens = use_tokens();
        let mut page = vstack(S3).tw_layout(&[w_full()]);
        for icon in ColourIcon::ALL {
            let mut line = rect().tw(&tokens, &[flex_row(), gap(S4), items(Align::End)]);
            for px in [16.0, 24.0, 32.0, 64.0] {
                line = line.child(colour_icon_view(&tokens, *icon, px));
            }
            page = page.child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), gap(S6), items(Align::Center)])
                    .child(line)
                    .child(caption(format!("{}  ({})", icon.name(), icon.family()))),
            );
        }
        page.into_element()
    })
    .page(2900.0),
    Spec::forced("Folders: decided by the text of the path", || {
        use bingux_ui::files::model::places::SpecialFolder;
        let tokens = use_tokens();
        let mut line = flow(&tokens);
        for kind in [
            SpecialFolder::Home,
            SpecialFolder::Desktop,
            SpecialFolder::Documents,
            SpecialFolder::Downloads,
            SpecialFolder::Music,
            SpecialFolder::Pictures,
            SpecialFolder::Videos,
            SpecialFolder::Templates,
            SpecialFolder::PublicShare,
            SpecialFolder::Trash,
            SpecialFolder::Root,
        ] {
            line = line.child(labelled(
                &tokens,
                &FileIcon::for_special(kind),
                32,
                false,
                format!("{kind:?}"),
            ));
        }
        line = line.child(labelled(
            &tokens,
            &FileIcon::for_kind(FileKind::Folder),
            32,
            false,
            "any other".to_owned(),
        ));
        line.into_element()
    })
    .page(140.0),
    Spec::forced("Places and drives", || {
        use bingux_ui::files::model::mounts::MountKind;
        let tokens = use_tokens();
        let mut line = flow(&tokens);
        for kind in [
            bingux_ui::files::model::PlaceKind::Recent,
            bingux_ui::files::model::PlaceKind::Home,
            bingux_ui::files::model::PlaceKind::Trash,
            bingux_ui::files::model::PlaceKind::Computer,
            bingux_ui::files::model::PlaceKind::Volume,
        ] {
            line = line.child(labelled(
                &tokens,
                &FileIcon::for_place_kind(kind),
                32,
                false,
                format!("{kind:?}"),
            ));
        }
        for kind in [MountKind::Local, MountKind::Removable, MountKind::Network, MountKind::Optical] {
            line = line.child(labelled(
                &tokens,
                &FileIcon::for_mount(kind),
                32,
                false,
                format!("{kind:?}"),
            ));
        }
        line.into_element()
    })
    .page(140.0),
    Spec::forced("A link (emblem) and a hidden item (faint)", || {
        let tokens = use_tokens();
        let mut page = vstack(S4).tw_layout(&[w_full()]);
        // The emblem is drawn on glyphs of 64 px and more only; the smaller
        // rows show the linked folder without it, as the hidden look shows.
        for px in ICON_SIZES {
            let mut line = flow(&tokens);
            let mut specs = vec![
                ("hidden folder", FileIcon::for_kind(FileKind::Folder).hidden(true), false),
                ("hidden file", FileIcon::for_kind(FileKind::Text).hidden(true), false),
                ("hidden, selected", FileIcon::for_kind(FileKind::Text).hidden(true), true),
            ];
            if px >= 64 {
                specs.splice(
                    0..0,
                    [
                        ("folder link", FileIcon::for_kind(FileKind::Folder).linked(true), false),
                        ("folder link, selected", FileIcon::for_kind(FileKind::Folder).linked(true), true),
                        ("file link", FileIcon::for_kind(FileKind::Pdf).linked(true), false),
                    ],
                );
            }
            for (name, icon, selected) in specs {
                line = line.child(labelled(&tokens, &icon, px, selected, name.to_owned()));
            }
            page = page.child(caption(format!("{px} px"))).child(line);
        }
        page.into_element()
    })
    .page(560.0),
];

/// The Trash's own folder, where the Trash specimens stand.
fn trash_folder() -> PathBuf {
    bingux_ui::files::Roots::current()
        .trash_folder()
        .map_or_else(|| fixture().join(".local/share/Trash/files"), std::path::Path::to_path_buf)
}

fn address(at: Where, editing: bool, draft: &str) -> Element {
    let editing = use_state(|| editing);
    let draft = use_state(|| draft.to_string());
    AddressBar::new(
        at,
        fixture().to_path_buf(),
        editing,
        draft,
        |_: Where| {},
        |_: PathBuf| {},
    )
    .into_element()
}

fn search_bar(filters_open: bool, words: &str, filters: &str, status: BarStatus) -> Element {
    use bingux_ui::files::model::find::{Query, Recents, Scope};
    let find = use_state(|| {
        let mut find = Find::default();
        find.open = true;
        find.filters_open = filters_open;
        find.filters = Query::parse(filters).filters;
        find
    });
    let draft = use_state(|| words.to_string());
    let include_noise = use_state(|| true);
    let recents = use_state(Recents::default);
    let choices = bingux_ui::files::scope_choices(&Places::default(), fixture(), &Scope::Subfolders);
    SearchBar {
        find,
        draft,
        choices,
        status,
        include_noise,
        recents,
        on_open: EventHandler::new(|_: ()| {}),
        on_step: EventHandler::new(|_: i32| {}),
        on_list: EventHandler::new(|_: ()| {}),
        on_close: EventHandler::new(|_: ()| {}),
        on_stop: EventHandler::new(|_: ()| {}),
    }
    .into_element()
}

fn found(running: bool) -> BarStatus {
    BarStatus { place: "Documents".into(), asked: true, walking: true, running, found: 1204, ..BarStatus::default() }
}

pub const SEARCH_BAR: &[Spec] = &[
    Spec::live("Resting: the field, where to look, Filters", || {
        search_bar(false, "", "", BarStatus { place: "Documents".into(), ..BarStatus::default() })
    })
    .wide(),
    Spec::forced("Searching: spinner, count and Stop", || search_bar(false, "report", "", found(true))).wide(),
    Spec::forced("Done: how many were found", || search_bar(false, "report", "", found(false))).wide(),
    Spec::forced("Filters set: tokens and Clear all", || {
        search_bar(false, "report", "type:image size:>10MB modified:week", found(false))
    })
    .wide(),
    Spec::forced("The Filters panel", || search_bar(false, "report", "type:image", found(false)))
        .wide()
        .script("click-text Filters; wait 600; save {out}"),
];

pub const ADDRESS_BAR: &[Spec] = &[
    Spec::live(
        "Crumbs: press one to go there, press the bar to type",
        || address(Where::Folder(fixture().join("Documents/Photos")), false, ""),
    ),
    // The folder of the crumbs specimen above, so the two sit in the same box
    // at the same width and the swap between them can be read by eye.
    Spec::forced("Editing a path", || {
        let here = Where::Folder(fixture().join("Documents/Photos"));
        let text = AddressBar::text_of(&here);
        address(here, true, &text)
    }),
    Spec::forced("Completing a path", || {
        address(
            Where::Folder(fixture().to_path_buf()),
            true,
            &format!("{}/Doc", fixture().display()),
        )
    })
    .page(260.0),
    Spec::forced("Recent files", || address(Where::Recent, false, "")),
    Spec::forced("The Trash: its icon and name, not a path", || {
        address(Where::Folder(trash_folder()), false, "")
    }),
    // A folder that was trashed: the name it had, below the Trash.
    Spec::forced("Inside the Trash: Trash, then the folder", || {
        address(Where::Folder(trash_folder().join("Old photos")), false, "")
    }),
    // The same box, the same icon and the same start of the text as the
    // trail above it: the address of the Trash is what is typed.
    Spec::forced("The Trash being typed: trash:///", || {
        let here = Where::Folder(trash_folder().join("Old photos"));
        let text = AddressBar::text_of(&here);
        address(here, true, &text)
    }),
    Spec::forced("Deep path, too long for the bar", || {
        address(
            Where::Folder(fixture().join("Documents/Photos/2026/Holidays/Summer/Mountains/Day 3")),
            false,
            "",
        )
    }),
];

pub const PLACES: &[Spec] = &[
    Spec::live("Places sidebar: bookmarks and every mounted drive", || {
        let places = use_hook(Places::load);
        PlacesSidebar::new(places, documents(), |_: Where| {})
            .recent(true)
            .into_element()
    })
    .page(420.0),
    Spec::live("Places menu (the phone's form)", || {
        let places = use_hook(Places::load);
        PlacesMenu::new(places, documents(), |_: Where| {})
            .recent(true)
            .into_element()
    })
    .page(420.0),
    Spec::live("Right-click a folder of yours: Open, Open in new tab, Copy path, Properties", app_sidebar)
        .page(420.0)
        .script("wait 600; rclick-text Documents; wait 400; save {out}"),
    Spec::live("Right-click a bookmark: Rename…, Copy path, Remove from bookmarks", app_sidebar)
        .page(420.0)
        .script("wait 600; rclick-text Photos; wait 400; save {out}"),
    Spec::live("Right-click Recent: Open only, there is no folder", app_sidebar)
        .page(420.0)
        .script("wait 600; rclick-text Recent; wait 400; save {out}"),
    Spec::live("Menu key (Shift+F10) on a focused row opens its menu", app_sidebar)
        .page(420.0)
        .script("wait 600; click-text Music; wait 300; key shift+f10; wait 400; save {out}"),
    Spec::live("Properties on a place: the dialog of its folder", app_sidebar)
        .page(520.0)
        .script("wait 600; rclick-text Documents; wait 400; click-text Properties; wait 900; save {out}"),
];

/// The places of the fixture home (Documents, Music, Downloads, Videos and a
/// bookmark of Photos), so a row's menu is the same on every machine.
fn fixture_places() -> Places {
    use bingux_ui::files::model::places::Sources;
    let home = fixture().to_path_buf();
    Places::build(
        &Sources {
            home: home.clone(),
            user_dirs: Some(
                "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\nXDG_MUSIC_DIR=\"$HOME/Music\"\n\
                 XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_VIDEOS_DIR=\"$HOME/Videos\"\n"
                    .to_owned(),
            ),
            mounts: None,
            bookmarks: Some(format!("file://{} Photos\n", home.join("Documents/Photos").display())),
        },
        |path| path.is_dir(),
    )
}

/// The sidebar of the Files app (it has tabs) over the fixture places.
fn app_sidebar() -> Element {
    let places = use_hook(fixture_places);
    PlacesSidebar::new(places, documents(), |_: Where| {})
        .on_open_new_tab(|_: Where| {})
        .on_empty_trash(|_: ()| {})
        .into_element()
}

pub const BROWSE: &[Spec] = &[Spec::live(
    "The whole file manager: places, address bar, list or grid, status pill",
    || {
        let memory = use_hook(|| TabMemory::new(documents()));
        browse(Browse::new(memory))
    },
)
.page(560.0)
.script("wait 800; click-text Notes.txt; wait 300; save {out}")];

pub const CHOOSER: &[Spec] = &[
    Spec::live("Open a file", || {
        let window = use_window();
        action_button("Open the chooser", move |_| {
            choose(
                window,
                ChooserRequest::open()
                    .title("Choose a document")
                    .current_folder(fixture().join("Documents"))
                    .filter_spec("Documents", "pdf txt md"),
                |_| {},
            );
        })
        .into_element()
    })
    .script("click-text Open the chooser; wait 1200; save {out}"),
    Spec::live("Save a file", || {
        let window = use_window();
        action_button("Save as", move |_| {
            choose(
                window,
                ChooserRequest::save("Untitled.txt")
                    .title("Save the note")
                    .current_folder(fixture().join("Documents")),
                |_| {},
            );
        })
        .into_element()
    })
    .script("click-text Save as; wait 1200; save {out}"),
    Spec::live("Choose a folder", || {
        let window = use_window();
        action_button("Choose a folder", move |_| {
            choose(
                window,
                ChooserRequest::select_folder()
                    .title("Choose a folder")
                    .current_folder(home_folder()),
                |_| {},
            );
        })
        .into_element()
    })
    .script("click-text Choose a folder; wait 1200; save {out}"),
    Spec::forced("The version of the dialog", || {
        rows(vec![
            row("files::VERSION")
                .value(bingux_ui::files::VERSION.to_string())
                .into_element(),
        ])
    }),
];

/// What the owner of a list wires into a preview's header: the file's place in
/// the list, Open, Properties and the switch between frames. A button whose
/// handler is absent is not drawn.
fn preview_nav() -> PreviewNav {
    PreviewNav::new(|_| {})
        .position(2, 5)
        .dockable(true)
        // A specimen in the middle of the page never takes the keyboard: the
        // last one to would scroll the page to itself.
        .takes_focus(false)
        .on_open(|_| {})
        .on_properties(|_| {})
        .on_step(|_: Step| {})
        .on_mode(|_: Mode| {})
}

/// How many helper processes this app lets previews start at once. A file
/// manager needs two (a pane and a popup); this page shows every preview side
/// by side, at desktop and at phone width, and the default would end all but
/// the last two with "stopped to make room".
const HELPERS: usize = 32;

/// The sample file `path` previewed in `mode`: the content of every frame is
/// the same component.
fn preview(path: &str, mode: Mode) -> Element {
    bingux_ui::preview::allow_helpers(HELPERS);
    let source = PreviewSource::path(fixture().join(path));
    PreviewHost::new(Some(source), mode, preview_nav()).into_element()
}

/// The height of the row a docked preview sits in, and of the pane shown alone.
const DOCK_HEIGHT: f32 = 360.0;
const PANE_HEIGHT: f32 = 420.0;

/// A file list with the preview docked at its right edge, as the file manager
/// has it: the list stays usable and the pane takes the rest of the row.
fn docked() -> Element {
    bingux_ui::preview::allow_helpers(HELPERS);
    let mut row_area = use_state(Area::default);
    let source = PreviewSource::path(fixture().join("Documents/Notes.txt"));
    rect()
        .horizontal()
        .width(Size::fill())
        .height(Size::px(DOCK_HEIGHT))
        .on_sized(move |event: Event<SizedEventData>| row_area.set_if_modified(event.area))
        .child(
            rect()
                .width(Size::flex(1.0))
                .height(Size::fill())
                .child(folder(documents(), 32)),
        )
        .child(PreviewDock::new(true, row_area(), Some(source), preview_nav()))
        .into_element()
}

pub const PREVIEW: &[Spec] = &[
    Spec::live("Popup: a picture (Open, Info, dock, full screen, Close)", || {
        preview("Documents/Photos/network.svg", Mode::Popup)
    })
    .page(460.0),
    Spec::live("Popup: a text file (Info on the right, W wraps lines)", || {
        preview("Documents/Notes.txt", Mode::Popup)
    })
    .page(360.0),
    Spec::live("Popup: a film (Space plays and pauses, arrows seek, the bar fades while it plays)", || {
        preview("Videos/Holiday clip.webm", Mode::Popup)
    })
    .page(520.0),
    Spec::live("Pane: a film waits on its first frame until you play it", || {
        rect()
            .width(Size::px(bingux_ui::preview::DEFAULT_WIDTH))
            .height(Size::px(PANE_HEIGHT))
            .child(preview("Videos/Holiday clip.webm", Mode::Pane))
            .into_element()
    })
    .page(PANE_HEIGHT),
    Spec::live("Popup: a song (its card, the same controls)", || preview("Music/Tone.wav", Mode::Popup))
        .page(420.0),
    Spec::forced("A film the player cannot open: the reason and Open", || preview("Music/Track 01.mp3", Mode::Popup))
        .page(420.0),
    Spec::live("Popup: a PDF (scroll, Ctrl+wheel zooms, Ctrl+F finds, F9 shows pages)", || {
        preview("Documents/Quarterly report.pdf", Mode::Popup)
    })
    .page(620.0),
    Spec::live("Pane: a PDF with the compact toolbar", || {
        rect()
            .width(Size::px(bingux_ui::preview::DEFAULT_WIDTH))
            .height(Size::px(PANE_HEIGHT))
            .child(preview("Documents/Quarterly report.pdf", Mode::Pane))
            .into_element()
    })
    .page(PANE_HEIGHT),
    Spec::forced("Pane: a PDF that is damaged", || preview("Documents/Scan 003.pdf", Mode::Pane)).page(PANE_HEIGHT),
    Spec::forced("Pane: the same content in a column", || {
        rect()
            .width(Size::px(bingux_ui::preview::DEFAULT_WIDTH))
            .height(Size::px(PANE_HEIGHT))
            .child(preview("Documents/Notes.txt", Mode::Pane))
            .into_element()
    })
    .page(PANE_HEIGHT),
    Spec::forced("A folder: its contents, counted live", || {
        preview("Documents", Mode::Pane)
    })
    .page(360.0),
    Spec::forced("A file with no viewer: its icon and facts", || {
        preview("Documents/Budget 2026.xlsx", Mode::Popup)
    })
    .page(360.0),
    Spec::forced("Nothing selected", || {
        PreviewHost::new(None, Mode::Pane, PreviewNav::new(|_| {})).into_element()
    })
    .page(220.0),
    Spec::live("Docked beside a list (drag the edge, Alt+P)", docked).page(380.0),
    Spec::live("Over the window: Space or Escape closes it", || {
        let overlays = use_overlays();
        action_button("Preview Notes.txt", move |_| {
            bingux_ui::preview::open(overlays, PreviewSource::path(fixture().join("Documents/Notes.txt")));
        })
        .into_element()
    })
    .script("click-text Preview Notes.txt; wait 1200; save {out}"),
];

/// The floating bar over a black stage, as it sits over a picture.
fn over_picture(bar: MediaControls) -> Element {
    rect()
        .width(Size::fill())
        .tw(&tokens_for_demo(), &[p(S4), bg(Paint::Backdrop)])
        .child(bar)
        .into_element()
}

fn tokens_for_demo() -> bingux_ui::theme::Tokens {
    bingux_ui::surface::use_tokens()
}

fn playing() -> ControlState {
    ControlState {
        playing: true,
        position_ns: 83_000_000_000,
        duration_ns: 212_000_000_000,
        progress: 83.0 / 212.0,
        buffered: 0.62,
        volume: 700,
        subtitles: vec!["English".to_owned(), "Deutsch".to_owned()],
        subtitle: 0,
        ..ControlState::default()
    }
}

/// The media control bar in each of its states.
pub const PLAYER_CONTROLS: &[Spec] = &[
    Spec::live("Playing: the bar, with the buffered range ahead of the thumb", || over_picture(MediaControls::new(playing(), |_| {}))).page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("Paused", || over_picture(MediaControls::new(ControlState { playing: false, ..playing() }, |_| {}))).page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("Muted, and a speed other than 1x", || {
        over_picture(MediaControls::new(ControlState { muted: true, rate: 1500, ..playing() }, |_| {}))
    })
    .page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("At the end: play again", || {
        over_picture(MediaControls::new(ControlState { playing: false, ended: true, progress: 1.0, position_ns: 212_000_000_000, buffered: 1.0, ..playing() }, |_| {}))
    })
    .page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("A stream that cannot seek: the bar is inert", || {
        over_picture(MediaControls::new(ControlState { seekable: false, duration_ns: 0, progress: 0.0, position_ns: 0, ..playing() }, |_| {}))
    })
    .page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("Compact: the docked pane and a phone (play, position, seek, mute)", || {
        over_picture(MediaControls::new(playing(), |_| {}).docked(true).width(bingux_ui::preview::MIN_WIDTH))
    })
    .page(120.0)
    .script("wait 800; save {out}"),
];

/// The document toolbar in each of its states. The icons are rasterised on
/// their first frame, so each specimen waits one beat before it is captured.
pub const DOCUMENT_CONTROLS: &[Spec] = &[
    Spec::live("Page 2 of 12 at fit width", || {
        let text = use_state(|| "2".to_owned());
        DocumentControls::new(DocumentState { page: 1, pages: 12, zoom: "Fit width".to_owned(), find: false }, text, |_| {}).into_element()
    })
    .page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("Fit page, the default of a document, with find on", || {
        let text = use_state(|| "7".to_owned());
        DocumentControls::new(DocumentState { page: 6, pages: 12, zoom: "Fit page".to_owned(), find: true }, text, |_| {}).into_element()
    })
    .page(120.0)
    .script("wait 800; save {out}"),
    Spec::live("At 125%, in the docked pane's width", || {
        let text = use_state(|| "1".to_owned());
        rect()
            .width(Size::px(bingux_ui::preview::DEFAULT_WIDTH))
            .child(DocumentControls::new(DocumentState { page: 0, pages: 3, zoom: "125%".to_owned(), find: false }, text, |_| {}))
            .into_element()
    })
    .page(120.0)
    .script("wait 800; save {out}"),
];
