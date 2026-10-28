//! Files: the file views, the address bar, the places sidebar and the chooser.
//!
//! The views read a real folder. The taxonomy makes one of its own, filled
//! with sample files, under the temporary directory (see [`fixture`]), so the
//! pages look the same on every machine and never show the user's files.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use bingux_ui::files::model::{Places, Selection, Sort};
use bingux_ui::files::{
    AddressBar, Browse, ChooserRequest, FileIcon, FileKind, FileView, Load, PlacesMenu, PlacesSidebar,
    TabMemory, Where, browse, choose, empty_state, error_state, file_icon_view, home_folder,
    loading_state, skeleton, use_directory,
};
use bingux_ui::preview::{Mode, PreviewDock, PreviewHost, PreviewNav, PreviewSource, Step};
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
    ("Documents/Quarterly report.pdf", &[0; 4096]),
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
    root
});

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
        let (load, _reload) = use_directory(&self.at);
        let level = use_state(|| self.px);
        let mut selection = use_state(Selection::new);
        let loaded = load.read().clone();
        match loaded {
            Load::Loading => skeleton(self.px > 32),
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

const LIST_HEIGHT: f32 = 340.0;

/// The nine zoom levels of the ladder: the first four are list rows, the rest
/// are grids of larger and larger tiles.
pub const FILE_VIEW: &[Spec] = &[
    Spec::live("List, 16 px icons (the default zoom)", || {
        folder(documents(), 16)
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
            .child(skeleton(false))
            .into_element()
    })
    .page(260.0),
    Spec::live("Right-click a file for its menu", || {
        folder(documents(), 16)
    })
    .page(LIST_HEIGHT)
    .script("wait 600; rclick-text Notes.txt; wait 400; save {out}"),
];

/// The icon sizes the file view zooms through.
const ICON_SIZES: [u16; 4] = [16, 32, 64, 128];

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
    Spec::forced("Every kind at 16, 32, 64 and 128 px, resting and selected", || {
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
    .page(1900.0),
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
        for px in ICON_SIZES {
            let mut line = flow(&tokens);
            for (name, icon, selected) in [
                ("folder link", FileIcon::for_kind(FileKind::Folder).linked(true), false),
                ("folder link, selected", FileIcon::for_kind(FileKind::Folder).linked(true), true),
                ("file link", FileIcon::for_kind(FileKind::Pdf).linked(true), false),
                ("hidden folder", FileIcon::for_kind(FileKind::Folder).hidden(true), false),
                ("hidden file", FileIcon::for_kind(FileKind::Text).hidden(true), false),
                ("hidden, selected", FileIcon::for_kind(FileKind::Text).hidden(true), true),
            ] {
                line = line.child(labelled(&tokens, &icon, px, selected, name.to_owned()));
            }
            page = page.child(caption(format!("{px} px"))).child(line);
        }
        page.into_element()
    })
    .page(560.0),
];

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

pub const ADDRESS_BAR: &[Spec] = &[
    Spec::live(
        "Crumbs: press one to go there, press the bar to type",
        || address(Where::Folder(fixture().join("Documents/Photos")), false, ""),
    ),
    Spec::forced("Editing a path", || {
        let text = AddressBar::text_of(&Where::Folder(fixture().join("Documents")));
        address(Where::Folder(fixture().join("Documents")), true, &text)
    }),
    Spec::forced("Completing a path", || {
        address(
            Where::Folder(fixture().to_path_buf()),
            true,
            &format!("{}/Doc", fixture().display()),
        )
    })
    .page(260.0),
    Spec::forced("Searching (not a path)", || {
        address(Where::Folder(fixture().join("Documents")), true, "report")
    }),
    Spec::forced("Recent files", || address(Where::Recent, false, "")),
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
];

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
        .on_open(|_| {})
        .on_properties(|_| {})
        .on_step(|_: Step| {})
        .on_mode(|_: Mode| {})
}

/// The sample file `path` previewed in `mode`: the content of every frame is
/// the same component.
fn preview(path: &str, mode: Mode) -> Element {
    let source = PreviewSource::path(fixture().join(path));
    PreviewHost::new(Some(source), mode, preview_nav()).into_element()
}

/// The height of the row a docked preview sits in, and of the pane shown alone.
const DOCK_HEIGHT: f32 = 360.0;
const PANE_HEIGHT: f32 = 420.0;

/// A file list with the preview docked at its right edge, as the file manager
/// has it: the list stays usable and the pane takes the rest of the row.
fn docked() -> Element {
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
        preview("Documents/Quarterly report.pdf", Mode::Popup)
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
