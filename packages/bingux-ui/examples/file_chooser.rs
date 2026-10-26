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
        name: "multiple",
        request: documents,
        script: "wait 500; click-text Meeting notes.txt; wait 200",
    },
];

fn main() {
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
            println!("[file_chooser] wrote {}", path.display());
        }
    }
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

/// Opens the chooser on mount, over an empty window.
#[derive(PartialEq)]
struct Opener {
    request: ChooserRequest,
}

impl Component for Opener {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let request = self.request.clone();
        use_side_effect(move || {
            choose(window, request.clone(), |chosen| {
                println!("[file_chooser] answer: {chosen:?}");
            });
        });
        rect().expanded()
    }
}
