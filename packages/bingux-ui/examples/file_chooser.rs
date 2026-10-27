//! Renders the file chooser headlessly, one PNG per scenario and window size.
//!
//! Run it against a fixture home so the captures do not show a real one:
//!
//! ```sh
//! scripts/file-chooser-fixture.py /tmp/bingux-files-home
//! HOME=/tmp/bingux-files-home cargo run --example file_chooser --features testing \
//!     -- /tmp/bingux-files-captures [scenario,...]
//! ```
//!
//! Each scenario opens the chooser over an empty window, runs a few scripted
//! steps (see `Snapshot::run_script`) and saves the frame as
//! `NAME-WIDTHxHEIGHT.png`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bingux_ui::files::{ChooserRequest, Filter, ViewMode, choose};
use bingux_ui::prelude::{OverlayHost, WindowHost, use_window};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg};
use freya::prelude::*;

/// Window sizes of the captures: a desktop window and a phone.
const SIZES: [(f32, f32); 2] = [(1100.0, 820.0), (390.0, 800.0)];

struct Scenario {
    name: &'static str,
    request: fn(&Path) -> ChooserRequest,
    /// Steps after the chooser has opened, before the capture.
    script: &'static str,
}

fn images(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Choose a wallpaper")
        .current_folder(home.join("Pictures"))
        .filter(
            Filter::new("Pictures")
                .glob("*.png")
                .glob("*.jpg")
                .glob("*.jpeg")
                .glob("*.webp"),
        )
}

fn vpn(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Import VPN configuration")
        .current_folder(home.join("Downloads"))
        .view(ViewMode::List)
        .filter(
            Filter::new("VPN configuration")
                .glob("*.conf")
                .glob("*.ovpn"),
        )
        .filter(Filter::new("All files"))
}

fn folder(home: &Path) -> ChooserRequest {
    ChooserRequest::select_folder()
        .title("Choose a folder")
        .current_folder(home.to_path_buf())
}

fn save(home: &Path) -> ChooserRequest {
    ChooserRequest::save("Report.pdf")
        .title("Save as")
        .current_folder(home.join("Documents"))
        .filter(Filter::new("PDF document").glob("*.pdf"))
        .filter(Filter::new("All files"))
}

fn documents(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open documents")
        .multiple(true)
        .current_folder(home.join("Documents"))
}

/// A small folder of text, logs and data, for Quick Look.
fn notes(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open file")
        .current_folder(home.join("Notes"))
}

fn empty(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open file")
        .current_folder(home.join("Empty"))
}

fn denied(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open file")
        .current_folder(home.join("Private"))
}

fn home_open(_: &Path) -> ChooserRequest {
    ChooserRequest::open().title("Open file")
}

/// No filters given: the dropdown offers All files and the presets.
fn presets(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open file")
        .current_folder(home.join("Downloads"))
}

/// The VPN request, with the presets added after its own filters.
fn vpn_and_presets(home: &Path) -> ChooserRequest {
    vpn(home).presets(true)
}

/// A folder two links deep in the home, to show a long trail.
fn deep(home: &Path) -> ChooserRequest {
    let mut path = home.join("Deep");
    for level in [
        "north-america",
        "united-states",
        "california",
        "san-francisco",
        "mission-district",
        "valencia-street",
        "apartment-files",
        "tenancy",
        "2026",
        "correspondence",
        "landlord",
        "repairs",
        "kitchen",
        "photos",
    ] {
        path.push(level);
    }
    ChooserRequest::open()
        .title("Open file")
        .current_folder(path)
}

/// A folder with sixty subfolders, for the crumb menu.
fn projects(home: &Path) -> ChooserRequest {
    ChooserRequest::select_folder()
        .title("Choose a folder")
        .current_folder(home.join("Projects").join("alpha-06"))
}

/// The Downloads folder of a home where it is a link to a share: the
/// fixture of `file-chooser-fixture.py --link-downloads`.
fn links(home: &Path) -> ChooserRequest {
    ChooserRequest::open()
        .title("Open file")
        .current_folder(home.to_path_buf())
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "open-image",
        request: images,
        script: "wait 500",
    },
    Scenario {
        name: "open-image-selected",
        request: images,
        script: "wait 500; click-text Aurora.png; wait 300",
    },
    Scenario {
        name: "select-folder",
        request: folder,
        script: "wait 400; click-text Projects; wait 300",
    },
    Scenario {
        name: "save",
        request: save,
        script: "wait 500",
    },
    Scenario {
        name: "save-replace",
        request: save,
        script: "wait 400; click-text Quarterly report.pdf; wait 200; click-text Save; wait 500 | wait 400; click-text Quarterly report.pdf; wait 200; click 362,132; wait 500",
    },
    Scenario {
        name: "vpn-import",
        request: vpn,
        script: "wait 500; click-text work.ovpn; wait 300",
    },
    Scenario {
        name: "keyboard-path",
        request: home_open,
        script: "wait 400; type ~; type /Doc; wait 400",
    },
    Scenario {
        name: "empty-folder",
        request: empty,
        script: "wait 500",
    },
    Scenario {
        name: "no-permission",
        request: denied,
        script: "wait 500",
    },
    Scenario {
        name: "recent",
        request: home_open,
        script: "wait 400; click-text Recent; wait 500 | wait 400; click-text Home; wait 300; click-text Recent; wait 500",
    },
    Scenario {
        name: "filter-presets",
        request: presets,
        script: "wait 500; click 858,708; wait 500 | wait 500; click 305,772; wait 500",
    },
    Scenario {
        name: "filter-images",
        request: presets,
        script: "wait 500; click 858,708; wait 400; click-text Images; wait 700 | wait 500; click 305,772; wait 400; click-text Images; wait 700",
    },
    Scenario {
        name: "filter-vpn-presets",
        request: vpn_and_presets,
        script: "wait 500; click 858,708; wait 500 | wait 500; click 305,772; wait 500",
    },
    Scenario {
        name: "deep-path",
        request: deep,
        script: "wait 700",
    },
    Scenario {
        name: "many-folders",
        request: projects,
        script: "wait 600",
    },
    Scenario {
        name: "symlinked-downloads",
        request: links,
        script: "wait 600",
    },
    Scenario {
        name: "new-folder",
        request: documents,
        script: "wait 500; click 872,159; wait 600; type Reports; wait 300 | wait 500; click 251,193; wait 600; type Reports; wait 300",
    },
    Scenario {
        name: "multiple",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 200",
    },
    // Quick Look: Space on the selected file, and the keys inside it.
    Scenario {
        name: "preview-image",
        request: images,
        script: "wait 500; click-text Aurora.png; wait 300; key space; wait 1200",
    },
    Scenario {
        name: "preview-image-next",
        request: images,
        script: "wait 500; click-text Aurora.png; wait 300; key space; wait 800; key right; wait 1200",
    },
    Scenario {
        name: "preview-photo",
        request: images,
        script: "wait 500; click-text Camera.jpg; wait 300; key space; wait 1200",
    },
    Scenario {
        name: "preview-text",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-long-log",
        request: notes,
        script: "wait 500; click-text server.log; wait 300; key space; wait 1200",
    },
    Scenario {
        name: "preview-json",
        request: notes,
        script: "wait 500; click-text settings.json; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-unknown-text",
        request: notes,
        script: "wait 500; click-text README; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-binary",
        request: notes,
        script: "wait 500; click-text firmware.dat; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-folder",
        request: documents,
        script: "wait 500; click-text Taxes; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-document",
        request: documents,
        script: "wait 500; click-text Quarterly report.pdf; wait 300; key space; wait 1000",
    },
    Scenario {
        name: "preview-closed",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key space; wait 600; key space; wait 800",
    },
    Scenario {
        name: "preview-in-address-bar",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key ctrl+l; wait 200; key space; wait 600",
    },
    // Search: the address bar filters the folder, and the last row searches
    // the subfolders.
    Scenario {
        name: "search-folder",
        request: documents,
        script: "wait 500; key ctrl+f; wait 300; type sc; wait 700",
    },
    Scenario {
        name: "search-by-typing",
        request: documents,
        script: "wait 500; type q; wait 300; type u; wait 700",
    },
    Scenario {
        name: "search-fuzzy",
        request: documents,
        script: "wait 500; key ctrl+f; wait 300; type qrp; wait 700",
    },
    Scenario {
        name: "search-no-match",
        request: documents,
        script: "wait 500; key ctrl+f; wait 300; type zzq; wait 700",
    },
    Scenario {
        name: "search-down-arrow",
        request: documents,
        script: "wait 500; key ctrl+f; wait 300; type scan; wait 400; key down; wait 200; key down; wait 500",
    },
    Scenario {
        name: "search-escape",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 200; key ctrl+f; wait 300; type sc; wait 400; key escape; wait 600",
    },
    Scenario {
        name: "search-subfolders",
        request: home_open,
        script: "wait 500; key ctrl+f; wait 300; type 2026; wait 400; key ctrl+return; wait 1500",
    },
    Scenario {
        name: "search-subfolders-row",
        request: home_open,
        script: "wait 500; key ctrl+f; wait 300; type report; wait 700",
    },
    Scenario {
        name: "search-images-only",
        request: images,
        script: "wait 500; key ctrl+f; wait 300; type a; wait 500; key ctrl+return; wait 1500",
    },
    Scenario {
        name: "search-images-mid",
        request: images,
        script: "wait 500; key ctrl+f; wait 300; type a; wait 600",
    },
    Scenario {
        name: "search-path-is-not-search",
        request: home_open,
        script: "wait 500; key ctrl+f; wait 300; type Documents/; wait 600",
    },
    // Properties: Alt+Enter on the selection.
    Scenario {
        name: "properties-image",
        request: images,
        script: "wait 500; click-text Camera.jpg; wait 300; key alt+return; wait 1200",
    },
    Scenario {
        name: "properties-file",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key ctrl+i; wait 1200",
    },
    Scenario {
        name: "properties-image-scrolled",
        request: images,
        script: "wait 500; click-text Camera.jpg; wait 300; key alt+return; wait 1200; scroll 550,450,-700; wait 600",
    },
    Scenario {
        name: "properties-file-scrolled",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key ctrl+i; wait 1200; scroll 550,450,-1400; wait 600",
    },
    Scenario {
        name: "properties-folder",
        request: documents,
        script: "wait 500; click-text Taxes; wait 300; key alt+return; wait 1500",
    },
    Scenario {
        name: "properties-several",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 300; key ctrl+a; wait 200; key alt+return; wait 1500",
    },
];

fn main() {
    if std::env::var_os("FILE_CHOOSER_LIVE").is_some() {
        live();
        return;
    }
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/tmp/bingux-files-captures".into()),
    );
    let only: Vec<String> = std::env::args()
        .nth(2)
        .map(|names| names.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let home = PathBuf::from(std::env::var("HOME").expect("HOME names the fixture home"));
    for scenario in SCENARIOS {
        if !only.is_empty() && !only.iter().any(|name| name == scenario.name) {
            continue;
        }
        for (width, height) in SIZES {
            let request = (scenario.request)(&home);
            let mut shot = Snapshot::new(
                move || {
                    Element::from(Themed {
                        child: WindowHost::new(OverlayHost::new(Opener {
                            request: request.clone(),
                            again: false,
                        }))
                        .into(),
                    })
                },
                (width, height),
            );
            shot.settle(Duration::from_millis(700));
            // `desktop | phone`: a script may differ for the phone.
            let steps = match scenario.script.split_once('|') {
                Some((desktop, phone)) => {
                    if width < 700.0 {
                        phone
                    } else {
                        desktop
                    }
                }
                None => scenario.script,
            };
            if let Err(error) = shot.run_script(steps, "files") {
                eprintln!("[file_chooser] {}: {error}", scenario.name);
            }
            let path = out.join(format!("{}-{}x{}.png", scenario.name, width, height));
            shot.save(&path);
            // New folder really creates one: take it away so the next size
            // starts from the same fixture.
            if scenario.name == "new-folder"
                && let Ok(entries) = std::fs::read_dir(home.join("Documents"))
            {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("Untitled folder")
                    {
                        let _ = std::fs::remove_dir(entry.path());
                    }
                }
            }
            println!("[file_chooser] wrote {}", path.display());
        }
    }
}

/// A real window that opens the scenario named by `$FILE_CHOOSER_LIVE` (the
/// first when the name matches none) and opens it again after every answer,
/// for driving with real pointer and keyboard input.
fn live() {
    let name = std::env::var("FILE_CHOOSER_LIVE").ok();
    let home = PathBuf::from(std::env::var("HOME").expect("HOME names the fixture home"));
    let scenario = SCENARIOS
        .iter()
        .find(|scenario| Some(scenario.name) == name.as_deref())
        .unwrap_or(&SCENARIOS[0]);
    let request = (scenario.request)(&home);
    bingux_ui::app::Application::new("bingux-file-chooser-example")
        .title("File chooser")
        .run(move || Themed {
            child: WindowHost::new(OverlayHost::new(Opener {
                request: request.clone(),
                again: true,
            }))
            .into(),
        });
}

/// Provides the tokens, as an application root does.
#[derive(PartialEq)]
struct Themed {
    child: Element,
}

impl Component for Themed {
    fn render(&self) -> impl IntoElement {
        let tokens = bingux_ui::theme::provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(self.child.clone())
    }
}

/// Opens the chooser on mount, over an empty window. With `again` it opens
/// the chooser again each time it is answered.
#[derive(PartialEq)]
struct Opener {
    request: ChooserRequest,
    again: bool,
}

impl Component for Opener {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let request = self.request.clone();
        let again = self.again;
        let mut round = use_state(|| 0_u32);
        // The effect also reruns when the window's size changes, which must
        // not open a second chooser: one is opened per round.
        let mut opened = use_state(|| None::<u32>);
        use_side_effect(move || {
            let this = round();
            if *opened.peek() == Some(this) {
                return;
            }
            opened.set(Some(this));
            choose(window, request.clone(), move |chosen| {
                println!("[file_chooser] answer: {chosen:?}");
                if again {
                    let next = *round.peek() + 1;
                    round.set(next);
                }
            });
        });
        rect().expanded()
    }
}
