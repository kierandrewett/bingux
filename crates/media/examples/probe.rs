//! Plays a file for a few seconds with a sink of a given size and prints the
//! frames per second the renderer delivered. `cargo run --example probe -- FILE [WxH] [SECONDS]`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingux_media::{Frame, FrameSink, Item, Kind, NoHooks, Options, Player};

struct Sink {
    size: (u32, u32),
    frames: AtomicU64,
    first: Mutex<Option<Frame>>,
}

impl FrameSink for Sink {
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn push(&self, frame: Frame) {
        let count = self.frames.fetch_add(1, Ordering::Relaxed);
        if count == 30 {
            *self.first.lock().unwrap() = Some(frame);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("a file");
    let size = args
        .next()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1280, 720));
    let seconds: u64 = args.next().and_then(|text| text.parse().ok()).unwrap_or(5);
    let sink = Arc::new(Sink {
        size,
        frames: AtomicU64::new(0),
        first: Mutex::new(None),
    });
    let options = Options {
        audio_output: std::env::var("PROBE_AO").unwrap_or_default(),
        ..Options::default()
    };
    let player = Player::new(sink.clone(), Arc::new(NoHooks), &options).expect("player");
    player.subscribe(|change| {
        if std::env::var("PROBE_VERBOSE").is_ok() {
            eprintln!("change {change:?}");
        }
    });
    let kind = if file.ends_with(".mp4") || file.ends_with(".mkv") {
        Kind::Video
    } else {
        Kind::Audio
    };
    player.play_items(vec![Item::for_path(file.into(), kind)], 0);
    std::thread::sleep(Duration::from_secs(1));
    let start_frames = sink.frames.load(Ordering::Relaxed);
    let start = Instant::now();
    std::thread::sleep(Duration::from_secs(seconds));
    let frames = sink.frames.load(Ordering::Relaxed) - start_frames;
    let status = player.status();
    println!(
        "backend {:?} size {}x{} frames {} in {:.1}s = {:.1} fps; position {:.2} / {:?}; video {:?}; tracks {}; chapters {}; state {:?}; title {:?}",
        player.backend(),
        size.0,
        size.1,
        frames,
        start.elapsed().as_secs_f64(),
        frames as f64 / start.elapsed().as_secs_f64(),
        status.position,
        status.duration,
        status.video_size,
        status.tracks.len(),
        status.chapters.len(),
        status.state,
        status.title(),
    );
    if let Some(frame) = sink.first.lock().unwrap().take() {
        // A PPM of frame 30, to look at.
        let mut out = format!("P6\n{} {}\n255\n", frame.width, frame.height).into_bytes();
        for pixel in frame.pixels.chunks_exact(4) {
            out.extend_from_slice(&pixel[..3]);
        }
        std::fs::write("/tmp/probe-frame.ppm", out).unwrap();
        println!("wrote /tmp/probe-frame.ppm");
    }
}
