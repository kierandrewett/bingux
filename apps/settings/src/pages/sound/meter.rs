//! Live level meters for the Sound page. A meter reads the peak of one node
//! (an output's monitor, an input or one application's stream) from
//! `pw-record`. Only levels are kept: the samples are read, measured and
//! thrown away.
//!
//! A meter costs nothing unless it is on screen in a focused window: the
//! capture starts when the meter appears or the window gains focus, and
//! `pw-record` is killed when the meter goes or focus is lost. While there is
//! sound the bar updates on the window's frame clock, at most 30 times a
//! second; when everything is silent it requests no frames.

use std::cell::RefCell;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use freya::prelude::*;

use bingux_audio_effects as effects;
use bingux_audio_effects::Direction;
use bingux_ui::style::*;
use bingux_ui::typography::CAPTION;

use crate::system;
use crate::ui::use_tokens;

/// The least time between two readings while there is sound: 30 frames a second.
const FRAME: Duration = Duration::from_millis(33);
/// How often a silent meter looks for sound.
const IDLE: Duration = Duration::from_millis(250);
/// How fast the bar falls once the sound stops, as a share of its travel per second.
const FALL_PER_SECOND: f32 = 1.4;
/// The smallest change of the bar, as a share of its travel, that is redrawn.
const MIN_STEP: f32 = 0.004;
/// The quietest level the bar shows, in dBFS. Silence is an empty bar.
const FLOOR_DB: f32 = -60.0;
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
        bingux_ui::runtime::die_with_parent(&mut command);
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

    /// Whether any sound has arrived since the last `take_peak`.
    fn heard(&self) -> bool {
        self.peak.load(Ordering::Relaxed) != 0
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

/// Stops its capture when dropped, so cancelling the task that holds it (the
/// row left the screen, or the window lost focus) ends `pw-record`.
struct Capture(Feed);

impl Drop for Capture {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Publish a smoothed level from `sample`, which gives the bar's new 0 to 1
/// position each time it is called.
///
/// While there is something to show the loop is paced by the window's frame
/// clock (`RenderingTicker`, which the compositor's frame callbacks drive),
/// and takes a reading at most every `FRAME`. Once the bar has fallen to
/// nothing and the source is silent it asks for no more frames and wakes
/// every `IDLE` to look for a signal, so a quiet meter costs no redraws.
async fn pace(mut sample: impl FnMut() -> f32, mut level: State<f32>, window: PlatformWindow) {
    let ticker = RenderingTicker::get();
    let mut shown = 0.0_f32;
    let mut target = 0.0_f32;
    let mut last = Instant::now();
    loop {
        let quiet = shown <= 0.0 && target <= 0.0;
        if quiet {
            timer(IDLE).await;
        } else {
            window.request_redraw();
            ticker.tick().await;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(last);
        if !quiet && elapsed < FRAME {
            continue;
        }
        last = now;
        // A new peak jumps the bar up; silence lets it fall at a steady rate.
        let fall = elapsed.as_secs_f32().min(0.25) * FALL_PER_SECOND;
        target = sample();
        shown = target.max(shown - fall).max(0.0);
        if (shown - *level.peek()).abs() > MIN_STEP || (shown == 0.0 && *level.peek() != 0.0) {
            level.set(shown);
        }
    }
}

/// Record a node and follow its peak. Dropping the future (the meter left
/// the screen, or the window lost focus) drops the capture, which ends
/// `pw-record`.
async fn follow(node: u32, source: Source, level: State<f32>, window: PlatformWindow) {
    let capture = Capture(Feed::start(node, source));
    pace(move || fraction_of(capture.0.take_peak()), level, window).await;
}

/// Follow a reading in the effects engine's meter file.
async fn follow_reading(direction: Direction, slot: u32, index: usize, range_db: f32, level: State<f32>, window: PlatformWindow) {
    pace(
        move || {
            let db = effects::service::read_meters(direction, slot).map_or(0.0, |values| values[index]);
            (-db / range_db).clamp(0.0, 1.0)
        },
        level,
        window,
    )
    .await;
}

/// The level a meter shows, as a 0 to 1 fraction. `make` starts the task
/// that feeds it. The task runs only while the window has focus and `active`
/// is true; when focus goes it is cancelled, and the bar empties.
fn use_following<F: Future<Output = ()> + 'static>(
    active: bool,
    make: impl Fn(State<f32>, PlatformWindow) -> F + 'static,
) -> State<f32> {
    let mut level = use_state(|| 0.0_f32);
    let focus = system::window_focus();
    let scope = current_scope_id();
    let window = Platform::get().current_window();
    let task = use_hook(|| Rc::new(RefCell::new(None::<TaskHandle>)));
    use_side_effect({
        let task = Rc::clone(&task);
        move || {
            let focused = *focus.read();
            if let Some(running) = task.borrow_mut().take() {
                running.cancel();
            }
            if focused && active {
                *task.borrow_mut() = Some(spawn_in_scope(make(level, window.clone()), scope));
            } else if *level.peek() != 0.0 {
                level.set(0.0);
            }
        }
    });
    use_drop({
        let task = Rc::clone(&task);
        move || {
            if let Some(running) = task.borrow_mut().take() {
                running.try_cancel();
            }
        }
    });
    level
}

/// How often a page with no spectrum yet looks for one.
const SPECTRUM_IDLE: Duration = Duration::from_millis(600);

/// Follow the engine's analyser: the spectrum is read from the meter file on
/// each rendered frame, at most 30 times a second. While the analyser has
/// published nothing the task wakes twice a second to look.
async fn follow_spectrum(direction: Direction, mut spectrum: State<Option<Vec<f32>>>, window: PlatformWindow) {
    let ticker = RenderingTicker::get();
    let mut last = Instant::now();
    let mut previous: Vec<f32> = Vec::new();
    let mut still = 0u32;
    loop {
        let read = effects::service::read_spectrum(direction).map(|bands| bands.to_vec());
        let has_reading = read.is_some();
        match read {
            Some(bands) => {
                still = if bands == previous { still + 1 } else { 0 };
                if still == 0 {
                    previous = bands.clone();
                    spectrum.set(Some(bands));
                }
            }
            None => {
                if spectrum.peek().is_some() {
                    spectrum.set(None);
                }
            }
        }
        // A spectrum that has stopped changing (nothing is playing) is read
        // twice a second until it moves again.
        if !has_reading || still > 5 {
            timer(SPECTRUM_IDLE).await;
        } else {
            window.request_redraw();
            ticker.tick().await;
            let wait = FRAME.saturating_sub(last.elapsed());
            if !wait.is_zero() {
                timer(wait).await;
            }
        }
        last = Instant::now();
    }
}

/// The analyser's spectrum of `direction`, in dBFS, while the window has
/// focus. `None` while the analyser has published nothing.
pub fn use_spectrum(direction: Direction) -> State<Option<Vec<f32>>> {
    let mut spectrum = use_state(|| None::<Vec<f32>>);
    let focus = system::window_focus();
    let scope = current_scope_id();
    let window = Platform::get().current_window();
    let task = use_hook(|| Rc::new(RefCell::new(None::<TaskHandle>)));
    use_side_effect({
        let task = Rc::clone(&task);
        move || {
            let focused = *focus.read();
            if let Some(running) = task.borrow_mut().take() {
                running.cancel();
            }
            if focused {
                *task.borrow_mut() = Some(spawn_in_scope(follow_spectrum(direction, spectrum, window.clone()), scope));
            } else if spectrum.peek().is_some() {
                spectrum.set(None);
            }
        }
    });
    use_drop({
        let task = Rc::clone(&task);
        move || {
            if let Some(running) = task.borrow_mut().take() {
                running.try_cancel();
            }
        }
    });
    spectrum
}

/// A level meter for one node. Place it under a key of the node so another
/// device or stream starts a new capture. It records only while it is on
/// screen and its window has focus.
#[derive(Clone, PartialEq)]
pub struct LevelMeter {
    pub node: u32,
    pub source: Source,
    /// A short bar, 40 px long, for a heading. Otherwise one thin bar that
    /// spans its row, as under an application's slider.
    pub compact: bool,
    /// False for a row that has stopped: the bar stays empty and nothing is
    /// recorded. Key the meter by this too, so a change starts afresh.
    pub live: bool,
}

impl Component for LevelMeter {
    /// A meter follows one node, so another node, or a row that has stopped,
    /// is a new meter with a new capture.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(u64::from(self.node) << 1 | u64::from(self.live))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (node, source) = (self.node, self.source);
        let level = use_following(self.live, move |level, window| follow(node, source, level, window));
        let fill = *level.read();
        // A compact bar is a short pill for a heading. Otherwise the bar spans
        // its row, as under an application's slider.
        let width = if self.compact { w(Dim::Button) } else { w_full() };
        rect()
            .tw(
                &tokens,
                &[
                    width,
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

/// How much gain a plugin is taking off, from its meter slot in the effects
/// engine's meter file. The bar grows from the left as the reduction deepens
/// and the figure beside it names the reduction in dB. `range_db` is the
/// reduction that fills the bar.
#[derive(Clone, PartialEq)]
pub struct ReductionMeter {
    pub direction: Direction,
    pub slot: u32,
    /// Which of the plugin's meters to show.
    pub index: usize,
    pub range_db: f32,
}

impl Component for ReductionMeter {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(u64::from(self.slot) << 8 | self.index as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (direction, slot, index, range_db) = (self.direction, self.slot, self.index, self.range_db);
        let level = use_following(true, move |level, window| {
            follow_reading(direction, slot, index, range_db, level, window)
        });
        let fill = *level.read();
        // A fixed-width figure, so the bar does not change length as it counts.
        let text = format!("-{:.1} dB", fill * range_db);
        rect()
            .tw(&tokens, &[w_full(), flex_row(), items(Align::Center), gap(S3)])
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[h(Dim::SliderTrack), rounded(Radius::Pill), bg(Paint::Control), clip()])
                    .child(rect().width(Size::percent(fill * 100.0)).tw(
                        &tokens,
                        &[h_full(), rounded(Radius::Pill), bg(Paint::Warning)],
                    )),
            )
            .child(
                rect()
                    .tw(&tokens, &[w(Dim::Field), flex_row(), justify(Align::End)])
                    .child(CAPTION.label(&tokens, text).max_lines(1)),
            )
    }
}
