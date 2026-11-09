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

use crate::{app, content, launch, prefs};

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
    app::prepare(prefs);
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
    let read = std::fs::read(file)
        .map(|bytes| content::decode(bytes, Some(std::path::Path::new(file)), None, false));
    let read_time = started.elapsed();
    let (lines, bytes) = match &read {
        Ok(read) => (read.rope.len_lines(), read.rope.len_bytes()),
        Err(error) => {
            eprintln!("could not read {file}: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(shot: &mut Snapshot) -> Vec<String> {
        let mut all: Vec<String> = shot.runner().find_many(|_, element| {
            Label::try_downcast(element).map(|label| label.text.to_string())
        });
        all.extend(shot.runner().find_many(|_, element| {
            Paragraph::try_downcast(element).map(|paragraph| {
                paragraph
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect::<String>()
            })
        }));
        all
    }

    fn shows(shot: &mut Snapshot, text: &str) -> bool {
        texts(shot).iter().any(|shown| shown.contains(text))
    }

    fn open(shot: &mut Snapshot, path: &std::path::Path) {
        let address = launch::normalise(path.as_os_str());
        shot.navigate(Location::parse(&address).expect("an address"));
        shot.settle(Duration::from_millis(1500));
    }

    /// The window's tabs follow the shared document manager: a second file
    /// opens in its own tab, closing one with changes asks, Discard closes
    /// it, and Reopen closed tab brings the file back from disk.
    #[test]
    fn tabs_follow_the_documents_close_asks_and_a_closed_file_comes_back() {
        isolate();
        let dir =
            std::env::temp_dir().join(format!("bingux-text-editor-tabs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (first, second) = (dir.join("first.txt"), dir.join("second.txt"));
        std::fs::write(&first, "one\n").unwrap();
        std::fs::write(&second, "two\n").unwrap();
        std::thread::spawn(bingux_text::highlight::warm).join().ok();

        let mut shot = start(first.to_str());
        shot.settle(Duration::from_millis(2500));
        open(&mut shot, &second);
        assert!(shows(&mut shot, "first.txt") && shows(&mut shot, "second.txt"));
        // The same file again is the same tab.
        open(&mut shot, &first);
        let tabs = texts(&mut shot)
            .iter()
            .filter(|shown| shown.as_str() == "first.txt")
            .count();
        assert_eq!(tabs, 1, "a file is open once");

        // A tab with no changes closes without asking.
        shot.run_script("key ctrl+w; wait 600", launch::SCHEME)
            .unwrap();
        assert!(!shows(&mut shot, "Save changes to"));
        assert!(!shows(&mut shot, "first.txt"), "its tab is gone");
        // One with changes asks, and Discard closes it.
        open(&mut shot, &first);
        shot.run_script("type x; wait 300; key ctrl+w; wait 800", launch::SCHEME)
            .unwrap();
        assert!(shows(&mut shot, "Save changes to “first.txt”?"));
        assert!(shot.click_text("Discard"));
        shot.settle(Duration::from_millis(800));
        assert!(!shows(&mut shot, "Save changes to"));
        assert!(!shows(&mut shot, "first.txt"));
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "one\n",
            "discarding never touches the file"
        );

        // Reopen closed tab opens the file again, from disk.
        shot.run_script("key ctrl+shift+t; wait 1500", launch::SCHEME)
            .unwrap();
        assert!(shows(&mut shot, "first.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
