//! A headless compositor on its own thread with a stepped clock, for deterministic
//! tests of the scene, the transitions and the connection security.
//!
//! Time and frames move only when the test says so: [`Harness::step`] advances the
//! clock and composes exactly one frame on every output, returning the pixels. Manager
//! commands and device input go through the same channel as `step`, so their order
//! relative to frames is exactly the order of the calls.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bsm_proto::scene::{CoverColor, OutputInfo, SceneCommand, SceneEvent};
use bsm_proto::state::InputOwner;
use smithay::reexports::calloop;

use crate::engine::Plan;
use crate::{Backend, Config, HeadlessConfig, HeadlessDrive};

/// Commands the stepped headless driver understands.
pub enum HarnessCmd {
    Scene(SceneCommand),
    /// Advance the clock by `dt_ms` and compose one frame on every output.
    Step { dt_ms: u64, reply: mpsc::Sender<Vec<CapturedFrame>> },
    /// Answered once every earlier command was handled.
    Barrier(mpsc::Sender<()>),
    SetOutputs(Vec<OutputInfo>),
    Key { code: u32, pressed: bool },
    PointerTo { x: f64, y: f64 },
    PointerButton { code: u32, pressed: bool },
    Focus(mpsc::Sender<FocusSnapshot>),
    /// Behave like a page-flipping backend: a frame is acknowledged only after `CompleteFlip`.
    NeedsRedraw(mpsc::Sender<bool>),
    FakeFlips(bool),
    CompleteFlip,
    /// Act as if the 500 ms flip watchdog fired.
    ExpireFlip,
    /// The next N renders fail.
    FailRenders(u32),
    /// With fake flips, the next frame fails on one output while the others are queued.
    PartialFailure,
    Stop,
}

/// Who holds keyboard and pointer focus right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusTarget {
    Greeter,
    Session(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusSnapshot {
    pub owner: InputOwner,
    pub keyboard: Option<FocusTarget>,
    pub pointer: Option<FocusTarget>,
}

/// One composed frame of one output.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub output: String,
    /// Frame counter of the compositor (all outputs of one step share it).
    pub seq: u64,
    pub time_ms: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA, row-major, no padding.
    pub rgba: Vec<u8>,
    /// What the engine planned for this frame.
    pub plan: Plan,
}

impl CapturedFrame {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let o = ((y * self.width + x) * 4) as usize;
        [self.rgba[o], self.rgba[o + 1], self.rgba[o + 2], self.rgba[o + 3]]
    }

    pub fn save_png(&self, path: &Path) -> Result<()> {
        let f = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(&self.rgba)?;
        Ok(())
    }
}

pub struct HarnessConfig {
    pub outputs: Vec<OutputInfo>,
    pub cover: CoverColor,
    /// Write every composed frame as `frame-<seq>-<output>.png` here.
    pub frame_dump_dir: Option<PathBuf>,
    /// Compose the pointer. Off by default so whole-frame comparisons are exact.
    pub draw_cursor: bool,
}

impl HarnessConfig {
    /// One or two outputs of the given size, side by side.
    pub fn with_outputs(count: usize, width: u32, height: u32) -> Self {
        let outputs = (0..count)
            .map(|i| OutputInfo {
                name: format!("Virtual-{}", i + 1),
                x: (i as u32 * width) as i32,
                y: 0,
                width,
                height,
                scale_120: 120,
                refresh_mhz: 60_000,
                transform: 0,
            })
            .collect();
        Self { outputs, cover: CoverColor { r: 0.0, g: 0.0, b: 0.0 }, frame_dump_dir: None, draw_cursor: false }
    }
}

pub struct Harness {
    ctl: calloop::channel::Sender<HarnessCmd>,
    // Held open so the compositor's command channel never reports "closed" by accident.
    _cmd_tx: calloop::channel::Sender<SceneCommand>,
    events: mpsc::Receiver<SceneEvent>,
    backlog: Vec<SceneEvent>,
    thread: Option<JoinHandle<Result<()>>>,
    tmp: tempfile::TempDir,
    pub greeter_socket: PathBuf,
    pub session_dir: PathBuf,
    pub outputs: Vec<OutputInfo>,
    now_ms: u64,
}

impl Harness {
    pub fn start(cfg: HarnessConfig) -> Result<Harness> {
        let tmp = tempfile::Builder::new().prefix("bsm-h").tempdir()?;
        let greeter_socket = tmp.path().join("g.sock");
        let session_dir = tmp.path().join("s");
        let (ctl_tx, ctl_rx) = calloop::channel::channel::<HarnessCmd>();
        let (cmd_tx, cmd_rx) = calloop::channel::channel::<SceneCommand>();
        let (evt_tx, events) = mpsc::channel();
        let config = Config {
            backend: Backend::Headless,
            runtime_dir: tmp.path().to_path_buf(),
            greeter_socket: greeter_socket.clone(),
            greeter_socket_mode: 0o660,
            greeter_socket_gid: None,
            session_socket_dir: session_dir.clone(),
            cover: cfg.cover,
            headless: HeadlessConfig { outputs: cfg.outputs.clone(), drive: HeadlessDrive::Stepped(ctl_rx), draw_cursor: cfg.draw_cursor },
            frame_dump_dir: cfg.frame_dump_dir,
            seat_name: "seat-test".into(),
            drm_device: None,
        };
        let thread = std::thread::Builder::new()
            .name("bsm-compositor".into())
            .spawn(move || crate::run(config, cmd_rx, evt_tx))?;
        let mut h = Harness {
            ctl: ctl_tx,
            _cmd_tx: cmd_tx,
            events,
            backlog: Vec::new(),
            thread: Some(thread),
            tmp,
            greeter_socket,
            session_dir,
            outputs: cfg.outputs,
            now_ms: 0,
        };
        match h.wait_event(Duration::from_secs(10), |e| matches!(e, SceneEvent::Ready { .. })) {
            Some(_) => Ok(h),
            None => bail!("compositor did not become ready"),
        }
    }

    pub fn dir(&self) -> &Path {
        self.tmp.path()
    }

    pub fn now(&self) -> Duration {
        Duration::from_millis(self.now_ms)
    }

    pub fn send(&self, cmd: SceneCommand) {
        self.ctl.send(HarnessCmd::Scene(cmd)).expect("compositor thread is gone");
    }

    /// Wait until every command sent so far was handled.
    pub fn sync(&self) {
        let (tx, rx) = mpsc::channel();
        self.ctl.send(HarnessCmd::Barrier(tx)).expect("compositor thread is gone");
        rx.recv_timeout(Duration::from_secs(10)).expect("compositor did not answer");
    }

    /// Advance the clock by `dt` and compose one frame per output.
    pub fn step(&mut self, dt: Duration) -> Vec<CapturedFrame> {
        let (tx, rx) = mpsc::channel();
        let dt_ms = dt.as_millis() as u64;
        self.now_ms += dt_ms;
        self.ctl
            .send(HarnessCmd::Step { dt_ms, reply: tx })
            .expect("compositor thread is gone");
        rx.recv_timeout(Duration::from_secs(10)).expect("compositor did not render")
    }

    pub fn set_outputs(&mut self, outputs: Vec<OutputInfo>) {
        self.outputs = outputs.clone();
        self.ctl.send(HarnessCmd::SetOutputs(outputs)).expect("compositor thread is gone");
        self.sync();
    }

    pub fn key(&self, code: u32, pressed: bool) {
        self.ctl.send(HarnessCmd::Key { code, pressed }).expect("compositor thread is gone");
    }

    pub fn pointer_to(&self, x: f64, y: f64) {
        self.ctl.send(HarnessCmd::PointerTo { x, y }).expect("compositor thread is gone");
    }

    pub fn pointer_button(&self, code: u32, pressed: bool) {
        self.ctl.send(HarnessCmd::PointerButton { code, pressed }).expect("compositor thread is gone");
    }

    /// Test hooks for the page-flip path (see `HarnessCmd`).
    pub fn fake_flips(&self, on: bool) {
        self.ctl.send(HarnessCmd::FakeFlips(on)).expect("compositor thread is gone");
    }

    /// Does the compositor want another frame?
    pub fn needs_redraw(&self) -> bool {
        let (tx, rx) = mpsc::channel();
        self.ctl.send(HarnessCmd::NeedsRedraw(tx)).expect("compositor thread is gone");
        rx.recv_timeout(Duration::from_secs(10)).expect("compositor did not answer")
    }

    pub fn complete_flip(&self) {
        self.ctl.send(HarnessCmd::CompleteFlip).expect("compositor thread is gone");
        self.sync();
    }

    pub fn expire_flip(&self) {
        self.ctl.send(HarnessCmd::ExpireFlip).expect("compositor thread is gone");
        self.sync();
    }

    pub fn fail_renders(&self, n: u32) {
        self.ctl.send(HarnessCmd::FailRenders(n)).expect("compositor thread is gone");
    }

    pub fn partial_failure(&self) {
        self.ctl.send(HarnessCmd::PartialFailure).expect("compositor thread is gone");
    }

    pub fn focus(&self) -> FocusSnapshot {
        let (tx, rx) = mpsc::channel();
        self.ctl.send(HarnessCmd::Focus(tx)).expect("compositor thread is gone");
        rx.recv_timeout(Duration::from_secs(10)).expect("compositor did not answer")
    }

    fn pump(&mut self) {
        while let Ok(e) = self.events.try_recv() {
            self.backlog.push(e);
        }
    }

    /// Every event seen so far, oldest first.
    pub fn events(&mut self) -> &[SceneEvent] {
        self.pump();
        &self.backlog
    }

    /// Take and clear the events seen so far.
    pub fn take_events(&mut self) -> Vec<SceneEvent> {
        self.pump();
        std::mem::take(&mut self.backlog)
    }

    /// Wait for an event matching `pred`, looking at the backlog first. The matching
    /// event stays in the backlog.
    pub fn wait_event(&mut self, timeout: Duration, pred: impl Fn(&SceneEvent) -> bool) -> Option<SceneEvent> {
        let deadline = Instant::now() + timeout;
        let mut seen = 0;
        loop {
            self.pump();
            if let Some(e) = self.backlog[seen..].iter().find(|e| pred(e)) {
                return Some(e.clone());
            }
            seen = self.backlog.len();
            let left = deadline.checked_duration_since(Instant::now())?;
            match self.events.recv_timeout(left.min(Duration::from_millis(50))) {
                Ok(e) => self.backlog.push(e),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    pub fn stop(mut self) -> Result<()> {
        self.stop_inner()
    }

    fn stop_inner(&mut self) -> Result<()> {
        let _ = self.ctl.send(HarnessCmd::Stop);
        match self.thread.take() {
            Some(t) => t.join().map_err(|_| anyhow::anyhow!("compositor thread panicked"))?,
            None => Ok(()),
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}
