//! Times the file views on a big folder: frames while scrolling and while
//! zooming, headless, in the software renderer.
//!
//! ```sh
//! BINGUX_FILES_STATE=/var/tmp/files-bench-state.json \
//!   cargo run --release --example files_bench --features testing -- /path/to/20k-folder [list|grid]
//! ```
//!
//! Every step is one input followed by `sync_and_update` (events, component
//! render, layout) and `render_to_surface` (paint). The tables give the
//! median, 95th percentile and worst of each, in milliseconds. The folder is
//! only read, never written.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bingux_ui::files::{ChooserRequest, ViewMode, choose};
use bingux_ui::prelude::{OverlayHost, WindowHost, use_window};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg};
use freya::prelude::*;

const SIZE: (f32, f32) = (1100.0, 820.0);

fn main() {
    let folder = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/var/tmp/Bench/local20k".into()),
    );
    let grid = std::env::args().nth(2).is_some_and(|mode| mode == "grid");
    let request = ChooserRequest::open()
        .title("Open file")
        .view(if grid { ViewMode::Grid } else { ViewMode::List })
        .current_folder(folder);
    let started = Instant::now();
    let mut shot = Snapshot::new(
        move || {
            Element::from(Themed {
                child: WindowHost::new(OverlayHost::new(Opener {
                    request: request.clone(),
                }))
                .into(),
            })
        },
        SIZE,
    );
    shot.settle(Duration::from_millis(300));
    let mut waited = 0;
    // The first full frame with a listing in it; a cold network folder may
    // take a while.
    while waited < 30_000 {
        shot.settle(Duration::from_millis(100));
        waited += 100;
        let rows = shot.runner().find_many(|_, element| {
            let label = Label::try_downcast(element)?;
            (label.text.starts_with("folder-") || label.text.starts_with("report-")).then_some(())
        });
        if !rows.is_empty() {
            break;
        }
    }
    println!(
        "first frames with rows: {:.0} ms after start",
        started.elapsed().as_secs_f64() * 1000.0
    );
    shot.settle(Duration::from_millis(500));

    let centre = (SIZE.0 * 0.6, SIZE.1 * 0.55);
    let mut scroll = Vec::new();
    for step in 0..240 {
        // Down for a hundred steps, then back up: a person flicking.
        let dy = if step < 120 { -160.0 } else { 160.0 };
        scroll.push(frame(&mut shot, |shot| {
            shot.runner().scroll((centre.0 as f64, centre.1 as f64), (0.0, dy));
        }));
    }
    report("scroll", &scroll);

    if std::env::var_os("BENCH_ZOOM").is_some() {
        let mut zoom = Vec::new();
        for step in 0..120 {
            let more = step < 60;
            zoom.push(frame(&mut shot, |shot| {
                shot.key_with(
                    Key::Character(if more { "+" } else { "-" }.into()),
                    Modifiers::CONTROL,
                );
            }));
        }
        report("zoom", &zoom);
    }
}

/// One step: the input, the update, the paint. Returns both times.
fn frame(shot: &mut Snapshot, input: impl FnOnce(&mut Snapshot)) -> (f64, f64) {
    let start = Instant::now();
    input(shot);
    shot.runner().sync_and_update();
    let updated = Instant::now();
    shot.runner().render_to_surface();
    let painted = Instant::now();
    (
        updated.duration_since(start).as_secs_f64() * 1000.0,
        painted.duration_since(updated).as_secs_f64() * 1000.0,
    )
}

fn report(name: &str, frames: &[(f64, f64)]) {
    let stats = |mut values: Vec<f64>| {
        values.sort_by(f64::total_cmp);
        let at = |share: f64| values[((values.len() - 1) as f64 * share) as usize];
        (at(0.5), at(0.95), at(1.0))
    };
    let update = stats(frames.iter().map(|frame| frame.0).collect());
    let paint = stats(frames.iter().map(|frame| frame.1).collect());
    let total = stats(frames.iter().map(|frame| frame.0 + frame.1).collect());
    println!("{name}: {} steps, ms (median / p95 / worst)", frames.len());
    println!(
        "  update {:.2} / {:.2} / {:.2}",
        update.0, update.1, update.2
    );
    println!("  paint  {:.2} / {:.2} / {:.2}", paint.0, paint.1, paint.2);
    println!("  total  {:.2} / {:.2} / {:.2}", total.0, total.1, total.2);
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
