//! `bingux-text-editor --snapshot DIR [FILE]...`: the window to PNG without a
//! compositor, and `--bench-typing FILE`: how long a key takes in a large file.
//!
//! The snapshot runs the app in `freya-testing`'s software renderer through
//! `bingux_ui::snapshot`. It writes `text-editor.png` (the first FILE, or a
//! new document), then `text-editor-find.png` with the find bar open on
//! "fn". `BINGUX_TEXT_EDITOR_SNAPSHOT_SIZE="WxH"` picks the size and
//! `BINGUX_TEXT_EDITOR_SNAPSHOT_SCRIPT` replaces the captures by one taken
//! after a script (`Snapshot::run_script`), named by
//! `BINGUX_TEXT_EDITOR_SNAPSHOT_NAME`. The state folder is a temporary one,
//! so the person's session and drafts are never read or written.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bingux_ui::nav::{Location, provide_router};
use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use freya::prelude::*;

use crate::{app, launch, prefs, session};

const SIZE: (f32, f32) = (1100.0, 720.0);

fn size() -> (f32, f32) {
    std::env::var("BINGUX_TEXT_EDITOR_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE)
}

fn isolate() {
    let state = std::env::temp_dir().join(format!(
        "bingux-text-editor-snapshot-{}",
        std::process::id()
    ));
    // SAFETY: set before any thread that reads the environment starts.
    unsafe {
        std::env::set_var("XDG_STATE_HOME", &state);
        std::env::set_var("XDG_CONFIG_HOME", &state);
    }
}

fn start(file: Option<&str>) -> Snapshot {
    let mut prefs = prefs::Prefs::default();
    if std::env::var("BINGUX_TEXT_EDITOR_SNAPSHOT_NOWRAP").is_ok() {
        prefs.wrap = false;
    }
    app::prepare(prefs, session::Session::default());
    let address = file.map(|file| launch::normalise(std::ffi::OsStr::new(file)));
    let location = address
        .and_then(|address| Location::parse(&address))
        .unwrap_or_else(|| Location::rooted(launch::SCHEME));
    Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            provide_router(Arc::new(launch::table()), &location);
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size(),
    )
}

pub fn run(dir: &str, files: &[String]) {
    isolate();
    std::thread::spawn(bingux_text::highlight::warm).join().ok();
    let mut shot = start(files.first().map(String::as_str));
    shot.settle(Duration::from_millis(2500));
    if let Ok(script) = std::env::var("BINGUX_TEXT_EDITOR_SNAPSHOT_SCRIPT") {
        let script = match script.strip_prefix('@') {
            Some(file) => std::fs::read_to_string(file).unwrap_or_default(),
            None => script,
        };
        if let Err(error) = shot.run_script(&script, launch::SCHEME) {
            eprintln!("[snapshot] script stopped: {error}");
        }
        shot.settle(Duration::from_millis(600));
        let name = std::env::var("BINGUX_TEXT_EDITOR_SNAPSHOT_NAME")
            .unwrap_or_else(|_| "text-editor".into());
        let path = format!("{dir}/{name}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
        return;
    }
    for (name, script) in [
        ("text-editor", ""),
        (
            "text-editor-find",
            "key ctrl+f; wait 300; type fn; wait 500",
        ),
    ] {
        if let Err(error) = shot.run_script(script, launch::SCHEME) {
            eprintln!("[snapshot] {name}: script stopped: {error}");
        }
        let path = format!("{dir}/{name}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}

/// Opens `file` headlessly, then times: reading and decoding it, the first
/// frame, and typing in the middle of it, key by key, through the whole
/// window (key event, edit, highlighting, layout and paint).
pub fn bench_typing(file: &str) {
    isolate();
    let started = Instant::now();
    let read = crate::document::read(std::path::Path::new(file), None, false);
    let read_time = started.elapsed();
    let (lines, bytes) = match &read {
        Ok(loaded) => (loaded.rope.len_lines(), loaded.rope.len_bytes()),
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };
    println!(
        "read and decode: {:.0} ms for {} bytes, {} lines",
        read_time.as_secs_f64() * 1000.0,
        bytes,
        lines
    );

    let started = Instant::now();
    let mut shot = start(Some(file));
    shot.settle(Duration::from_millis(200));
    println!(
        "window with the file: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    // Go to the middle of the file, then type.
    let middle = (lines / 2).max(1);
    let _ = shot.run_script(
        &format!("key ctrl+i; wait 100; type {middle}; key return; wait 500"),
        launch::SCHEME,
    );
    let mut times = Vec::new();
    for ch in "the quick brown fox jumps over the lazy dog "
        .chars()
        .cycle()
        .take(200)
    {
        let key = if ch == ' ' {
            "type _".to_string()
        } else {
            format!("type {ch}")
        };
        let started = Instant::now();
        let _ = shot.run_script(&key, launch::SCHEME);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    // The frame after typing, to see that the keys landed in the middle.
    shot.settle(Duration::from_millis(300));
    let frame = std::env::var("BINGUX_TEXT_EDITOR_BENCH_PNG")
        .unwrap_or_else(|_| "/tmp/bingux-text-editor-bench.png".into());
    shot.save(&frame);
    println!("frame after typing: {frame}");
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "typing: {} keys, mean {:.2} ms, median {:.2} ms, p95 {:.2} ms, max {:.2} ms (key event to painted frame)",
        times.len(),
        mean,
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() - 1]
    );
}
