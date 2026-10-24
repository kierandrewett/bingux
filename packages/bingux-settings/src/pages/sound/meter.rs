//! Live level meters for the Sound page. A meter reads the peak of one node
//! (an output's monitor, an input or one application's stream) from
//! `pw-record`. Only levels are kept: the samples are read, measured and
//! thrown away. A meter runs only while its row is on screen; the page keys
//! each meter by its node, so a new device or stream starts a new capture.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use freya::prelude::*;

use bingux_ui::style::*;

use crate::ui::use_tokens;

/// How often the UI reads the peak and redraws the bar.
const TICK: Duration = Duration::from_millis(50);
/// The level falls by this factor per tick once the sound stops.
const DECAY: f32 = 0.8;
/// The quietest level the bar shows, in dBFS. Silence is an empty bar.
const FLOOR_DB: f32 = -60.0;
/// Segments in a segmented bar.
const SEGMENTS: usize = 20;
/// Bytes per read from `pw-record`.
const CHUNK: usize = 4096;

/// What a meter listens to.
#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    /// The monitor of an output device, so what it plays.
    Output,
    /// An input device, so what it records.
    Input,
    /// One application's stream, so what it plays or records.
    Stream,
}

/// A capture of one node. `pw-record` is started on a background thread, so
/// the UI never waits for a process; the reader thread keeps the loudest
/// sample since the UI last took it.
#[derive(Clone)]
struct Feed {
    /// The running capture, once the background thread has started it.
    child: Arc<Mutex<Option<Child>>>,
    /// Set by `stop`, so a capture that starts late is killed at once.
    stopped: Arc<AtomicBool>,
    /// The largest absolute sample since the last read, as the bits of an
    /// f32. Non-negative floats order the same as their bits, so `fetch_max`
    /// keeps the loudest.
    peak: Arc<AtomicU32>,
}

impl Feed {
    fn start(node: u32, source: Source) -> Feed {
        let feed = Feed {
            child: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(false)),
            peak: Arc::new(AtomicU32::new(0)),
        };
        let worker = feed.clone();
        std::thread::spawn(move || worker.run(node, source));
        feed
    }

    fn run(&self, node: u32, source: Source) {
        let mut command = Command::new("pw-record");
        command.arg("--target").arg(node.to_string());
        // An output's monitor records what it plays; a stream's monitor
        // records what that application plays or records.
        match source {
            Source::Output => {
                command.args(["-P", "stream.capture.sink=true"]);
            }
            Source::Stream => {
                command.args(["-P", "stream.monitor=true"]);
            }
            Source::Input => {}
        }
        command
            .args([
                "--raw",
                "--format",
                "f32",
                "--rate",
                "48000",
                "--channels",
                "2",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Ok(mut child) = command.spawn() else {
            return;
        };
        let stdout = child.stdout.take();
        *self.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        if self.stopped.load(Ordering::SeqCst) {
            self.kill();
            return;
        }
        let Some(mut stdout) = stdout else {
            return;
        };
        let mut chunk = [0u8; CHUNK];
        // A read can end inside a sample; the rest waits for the next read.
        let mut pending: Vec<u8> = Vec::with_capacity(CHUNK + 4);
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    pending.extend_from_slice(&chunk[..read]);
                    let whole = pending.len() / 4 * 4;
                    let mut loudest = 0.0_f32;
                    for sample in pending[..whole].chunks_exact(4) {
                        let value =
                            f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]).abs();
                        if value.is_finite() && value > loudest {
                            loudest = value;
                        }
                    }
                    pending.drain(..whole);
                    if loudest > 0.0 {
                        self.peak.fetch_max(loudest.to_bits(), Ordering::Relaxed);
                    }
                }
            }
        }
        self.kill();
    }

    /// Whether the capture has been stopped.
    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Stop the capture. Killing closes the pipe, which ends the reader
    /// thread; the process is reaped on its own thread.
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.kill();
    }

    fn kill(&self) {
        let child = self.child.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut child) = child {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }

    /// The peak since the last call, as a linear amplitude, and zeroes it.
    fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }
}

/// The bar's fill as a 0 to 1 fraction of its travel, from a linear amplitude
/// on a dBFS scale. Silence and anything under the floor are empty.
fn fraction_of(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * amplitude.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// A level meter for one node. Place it under a key of the node so another
/// device or stream starts a new capture.
#[derive(Clone, PartialEq)]
pub struct LevelMeter {
    pub node: u32,
    pub source: Source,
    /// Segments, as GNOME draws a device's level. Otherwise one thin bar, as
    /// under an application's slider.
    pub segmented: bool,
}

impl Component for LevelMeter {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let node = self.node;
        let source = self.source;
        let segmented = self.segmented;
        // The displayed level as a 0 to 1 fraction of the travel.
        let mut level = use_state(|| 0.0_f32);
        let feed = use_hook(move || {
            let feed = Feed::start(node, source);
            let reader = feed.clone();
            spawn(async move {
                loop {
                    timer(TICK).await;
                    // The capture is stopped when the row goes away.
                    if reader.stopped() {
                        break;
                    }
                    // A new peak jumps the bar up; silence lets it fall.
                    let current = *level.read();
                    let next = fraction_of(reader.take_peak()).max(current * DECAY);
                    if (next - current).abs() > 0.002 {
                        level.set(next);
                    }
                }
            });
            feed
        });
        use_drop(move || feed.stop());

        let fill = *level.read();
        if segmented {
            let lit = (fill * SEGMENTS as f32).round() as usize;
            let segments = (0..SEGMENTS).map(|index| -> Element {
                let paint = if index < lit {
                    Paint::Accent
                } else {
                    Paint::Control
                };
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_flex(),
                            h(Dim::SliderTrack),
                            rounded(Radius::Mark),
                            bg(paint),
                        ],
                    )
                    .into()
            });
            rect()
                .tw(&tokens, &[flex_row(), gap(S1), w_full()])
                .children(segments)
        } else {
            rect()
                .tw(
                    &tokens,
                    &[
                        w_full(),
                        h(Dim::SliderTrack),
                        rounded(Radius::Pill),
                        bg(Paint::Control),
                    ],
                )
                .child(rect().width(Size::percent(fill * 100.0)).tw(
                    &tokens,
                    &[h_full(), rounded(Radius::Pill), bg(Paint::Accent)],
                ))
        }
    }
}
