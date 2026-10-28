//! Files: the file views, the address bar, the places sidebar and the chooser.
//!
//! The views read a real folder. The taxonomy makes one of its own, filled
//! with sample files, under the temporary directory (see [`fixture`]), so the
//! pages look the same on every machine and never show the user's files.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use bingux_ui::files::model::{Places, Selection, Sort};
use bingux_ui::files::{
    AddressBar, Browse, ChooserRequest, FileView, Load, PlacesMenu, PlacesSidebar, TabMemory,
    Where, browse, choose, empty_state, entry_icon, error_state, folder_icon, home_folder,
    loading_state, place_icon, skeleton, use_directory,
};
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

/// The icons the views choose for a place, a folder and a file.
pub const GLYPHS: &[Spec] = &[Spec::forced("Place, folder and entry icons", || {
    let tokens = use_tokens();
    let (load, _reload) = use_directory(&documents());
    let mut line = rect().tw(&tokens, &[w_full(), flex_row(), wrap(S3), gap(S3)]);
    for kind in [
        bingux_ui::files::model::PlaceKind::Home,
        bingux_ui::files::model::PlaceKind::Desktop,
        bingux_ui::files::model::PlaceKind::Documents,
        bingux_ui::files::model::PlaceKind::Downloads,
        bingux_ui::files::model::PlaceKind::Music,
        bingux_ui::files::model::PlaceKind::Computer,
    ] {
        line = line.child(
            vstack(S1)
                .child(icon(
                    place_icon(kind),
                    IconSize::Row,
                    Paint::Text.color(&tokens),
                ))
                .child(caption(format!("{kind:?}"))),
        );
    }
    line = line.child(
        vstack(S1)
            .child(icon(
                folder_icon(None),
                IconSize::Row,
                Paint::Text.color(&tokens),
            ))
            .child(caption("folder")),
    );
    if let Load::Ready(listing) = &*load.read() {
        for entry in listing.entries.iter().take(8) {
            line = line.child(
                vstack(S1)
                    .child(icon(
                        entry_icon(entry),
                        IconSize::Row,
                        Paint::Text.color(&tokens),
                    ))
                    .child(caption(entry.name.chars().take(10).collect::<String>())),
            );
        }
    }
    line.into_element()
})];

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
