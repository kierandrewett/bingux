//! Captures of the file view's gestures: the list with columns, zoom, menus,
//! rubber band, inline rename and the status line.
//!
//! ```sh
//! scripts/file-chooser-fixture.py /tmp/home
//! HOME=/tmp/home BINGUX_FILES_STATE=/tmp/state.json \
//!   cargo run --example files_view_capture --features testing -- /tmp/out [scenario,...]
//! ```
//!
//! Each scenario opens the chooser on a folder, runs a script and saves
//! `NAME-WIDTHxHEIGHT.png`. A script is steps separated by `;`: everything
//! `Snapshot::run_script` knows, and these:
//!
//! - `ckey X`: press Ctrl with the key X (`ckey =` zooms in)
//! - `rclick-text TEXT`: right click the label with that text
//! - `drag-text TEXT,DX,DY`: press the label, move by DX and DY, release
//! - `at-text TEXT,DX,DY ACTION`: ACTION (`click`, `right_click`, `hover`,
//!   `down`, `move`, `up`) at the label's centre moved by DX and DY
//! - `save NAME`: save the frame as `NAME-WIDTHxHEIGHT.png`

use std::path::{Path, PathBuf};
use std::time::Duration;

use bingux_ui::files::{ChooserRequest, ViewMode, choose};
use bingux_ui::prelude::{OverlayHost, WindowHost, use_window};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg};
use freya::prelude::*;

const SIZES: [(f32, f32); 2] = [(1100.0, 820.0), (390.0, 800.0)];

struct Scenario {
    name: &'static str,
    folder: &'static str,
    grid: bool,
    /// `desktop | phone`, or one script for both.
    script: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "list",
        folder: "Documents",
        grid: false,
        script: "wait 500",
    },
    Scenario {
        name: "list-columns-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; rclick-text Modified; wait 300",
    },
    Scenario {
        name: "list-columns-shown",
        folder: "Documents",
        grid: false,
        script: "wait 500; rclick-text Modified; wait 300; click-text Type; wait 300; rclick-text Modified; wait 300; click-text Created; wait 400",
    },
    Scenario {
        name: "list-resize",
        folder: "Documents",
        grid: false,
        script: "wait 500; at-text Size,18,0 move; at-text Size,18,0 down; at-text Size,60,0 move; at-text Size,100,0 up; wait 400",
    },
    Scenario {
        name: "list-reorder",
        folder: "Documents",
        grid: false,
        script: "wait 500; at-text Modified,0,0 down; at-text Modified,-60,0 move; at-text Modified,-110,0 move; wait 200; save list-reorder-mid; at-text Modified,-115,0 up; wait 400",
    },
    Scenario {
        name: "list-row-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; rclick-text Quarterly report.pdf; wait 400",
    },
    Scenario {
        name: "list-folder-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; rclick-text Taxes; wait 400",
    },
    Scenario {
        name: "list-multi-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; ckey a; wait 300; rclick-text Quarterly report.pdf; wait 400",
    },
    Scenario {
        name: "list-empty-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; right_click 700,640; wait 400",
    },
    Scenario {
        name: "list-rubber-band",
        folder: "Documents",
        grid: false,
        script: "wait 500; at-text Budget 2026.xlsx,300,0 down; at-text Scan 003.pdf,200,10 move; at-text Scan 004.pdf,200,10 move; wait 200",
    },
    Scenario {
        name: "list-rename",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400",
    },
    Scenario {
        name: "list-rename-menu",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; right_click 480,365; wait 500 | wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; right_click 200,466; wait 500",
    },
    Scenario {
        name: "list-rename-outside-rclick",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; right_click 440,525; wait 500 | wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; right_click 200,580; wait 500",
    },
    Scenario {
        name: "list-rename-click-inside",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; click 600,365; wait 200; click 420,365; click 420,365; wait 300 | wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; click 200,466; wait 200; click 120,466; click 120,466; wait 300",
    },
    Scenario {
        name: "list-rename-drag-inside",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; down 380,365; move 450,365; move 540,370; up 540,380; wait 300 | wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; down 80,466; move 150,466; move 250,470; up 250,490; wait 300",
    },
    Scenario {
        name: "list-rename-escape-refocus",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; type X; key escape; wait 300; save list-rename-escape-refocus-closed; key f2; wait 400",
    },
    Scenario {
        name: "list-rename-click-other",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; type Q; click-text Scan 003.pdf; wait 600 | wait 100",
    },
    Scenario {
        name: "list-rename-touch",
        folder: "Documents",
        grid: false,
        script: "wait 100 | wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; touch 200,466; wait 300; save list-rename-touch-tap; long_press 120,466; wait 500",
    },
    Scenario {
        name: "list-rename-enter-refocus",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Quarterly report.pdf; wait 200; key f2; wait 400; type Kept; key return; wait 700; save list-rename-enter-refocus-closed; key f2; wait 400 | wait 100",
    },
    Scenario {
        name: "list-zoom-in",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Budget 2026.xlsx; wait 100; ckey =; wait 400; ckey =; wait 600",
    },
    Scenario {
        name: "list-zoom-out",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Budget 2026.xlsx; wait 100; ckey -; wait 400; ckey -; wait 600",
    },
    Scenario {
        name: "grid",
        folder: "Pictures",
        grid: true,
        script: "wait 900",
    },
    Scenario {
        name: "grid-row-menu",
        folder: "Pictures",
        grid: true,
        script: "wait 900; rclick-text Aurora.png; wait 400",
    },
    Scenario {
        name: "grid-rename",
        folder: "Pictures",
        grid: true,
        script: "wait 900; click-text Aurora.png; wait 200; key f2; wait 400",
    },
    Scenario {
        name: "grid-zoom",
        folder: "Pictures",
        grid: true,
        script: "wait 900; ckey =; wait 700; ckey =; wait 700",
    },
    // The zoom ladder from the default: 16 px in a list, then each level
    // (20, 24, 32 list; 48, 64, 96, 128, 192 grid). Run it with a fresh
    // `BINGUX_FILES_STATE`: every step is remembered for the folder.
    Scenario {
        name: "zoom-ladder",
        folder: "Pictures",
        grid: false,
        script: "wait 700; save zoom-16; ckey =; wait 200; save zoom-20; ckey =; wait 200; save zoom-24; ckey =; wait 200; save zoom-32; ckey =; wait 500; save zoom-48; ckey =; wait 500; save zoom-64; ckey =; wait 600; save zoom-96; ckey =; wait 800; save zoom-128; ckey =; wait 900; save zoom-192; ckey 0; wait 500",
    },
    Scenario {
        name: "grid-empty-menu",
        folder: "Pictures",
        grid: true,
        script: "wait 900; right_click 640,700; wait 400",
    },
    Scenario {
        name: "toggle-view",
        folder: "Pictures",
        grid: true,
        script: "wait 900; click 916,159; wait 600; save toggle-view-list; click 916,159; wait 600",
    },
    Scenario {
        name: "empty-folder-menu",
        folder: "Empty",
        grid: false,
        script: "wait 500; right_click 700,400; wait 400",
    },
    Scenario {
        name: "list-resize-modified",
        folder: "Documents",
        grid: false,
        script: "wait 500; at-text Modified,-35,0 down; at-text Modified,-65,0 move; wait 100; save list-resize-modified-mid; at-text Modified,-65,0 up; wait 400",
    },
    Scenario {
        name: "list-resize-fit",
        folder: "Documents",
        grid: false,
        script: "wait 500; at-text Modified,-35,0 down; at-text Modified,-95,0 move; at-text Modified,-35,0 up; wait 300; save list-resize-fit-wide; at-text Modified,-35,0 click; at-text Modified,-35,0 click; wait 400",
    },
    Scenario {
        name: "delete-permanently-dialog",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Meeting notes; wait 300; key shift+delete; wait 700",
    },
    Scenario {
        name: "delete-permanently-many",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Meeting notes; key ctrl+a; wait 300; key shift+delete; wait 700",
    },
    Scenario {
        name: "delete-permanently-cancel-keeps",
        folder: "Documents",
        grid: false,
        script: "wait 500; click-text Meeting notes; wait 300; key shift+delete; wait 700; key return; wait 500",
    },
];

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/tmp/bingux-files-view".into()),
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
            let request = ChooserRequest::open()
                .title("Open file")
                .multiple(true)
                .view(if scenario.grid {
                    ViewMode::Grid
                } else {
                    ViewMode::List
                })
                .current_folder(home.join(scenario.folder));
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
            let tag = |name: &str| out.join(format!("{name}-{width}x{height}.png"));
            for step in steps.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                if let Err(error) = run_step(&mut shot, step, &tag) {
                    eprintln!("[files_view_capture] {}: {step}: {error}", scenario.name);
                }
            }
            let path = tag(scenario.name);
            shot.save(&path);
            println!("[files_view_capture] wrote {}", path.display());
        }
    }
}

fn run_step(shot: &mut Snapshot, step: &str, tag: &dyn Fn(&str) -> PathBuf) -> Result<(), String> {
    let (verb, rest) = step
        .split_once(char::is_whitespace)
        .map_or((step, ""), |(verb, rest)| (verb, rest.trim()));
    match verb {
        "ckey" => {
            shot.key_with(Key::Character(rest.to_string()), Modifiers::CONTROL);
            shot.settle(Duration::from_millis(50));
            Ok(())
        }
        "key" if matches!(rest, "f2" | "f5" | "delete" | "menu") => {
            let key = match rest {
                "f2" => NamedKey::F2,
                "f5" => NamedKey::F5,
                "delete" => NamedKey::Delete,
                _ => NamedKey::ContextMenu,
            };
            shot.key(Key::Named(key));
            Ok(())
        }
        "rclick-text" => {
            let (x, y) = find(shot, rest).ok_or("no label")?;
            shot.right_click(x, y);
            Ok(())
        }
        "at-text" => {
            let (spec, action) = rest.rsplit_once(' ').ok_or("at-text needs an action")?;
            let mut parts = spec.rsplitn(3, ',');
            let dy: f32 = parts.next().and_then(|v| v.parse().ok()).ok_or("dy")?;
            let dx: f32 = parts.next().and_then(|v| v.parse().ok()).ok_or("dx")?;
            let text = parts.next().ok_or("text")?;
            let (x, y) = find(shot, text).ok_or("no label")?;
            let (x, y) = (x + dx, y + dy);
            shot.run_script(&format!("{action} {x},{y}"), "files")
        }
        "save" => {
            shot.save(tag(rest));
            Ok(())
        }
        _ => shot.run_script(step, "files"),
    }
}

/// The centre of the label whose text is `text`, an exact match first.
fn find(shot: &mut Snapshot, text: &str) -> Option<(f32, f32)> {
    let hits: Vec<(bool, f32, f32)> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        if !label.text.contains(text) {
            return None;
        }
        let area = node.layout().area;
        Some((
            label.text == text,
            area.min_x() + area.width() / 2.0,
            area.min_y() + area.height() / 2.0,
        ))
    });
    hits.iter()
        .find(|hit| hit.0)
        .or_else(|| hits.first())
        .map(|&(_, x, y)| (x, y))
}

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

#[derive(PartialEq)]
struct Opener {
    request: ChooserRequest,
}

impl Component for Opener {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let request = self.request.clone();
        use_side_effect(move || {
            choose(window, request.clone(), |_| {});
        });
        rect().expanded()
    }
}

#[allow(dead_code)]
fn unused(_: &Path) {}
