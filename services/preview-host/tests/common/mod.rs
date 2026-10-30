//! Shared helpers of the host's integration tests: a host process on a socket
//! pair, generated fixtures, and a small client that waits for packets.

#![allow(dead_code)]

use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bingux_preview_core::host::channel::{Channel, Received, RecvError};
use bingux_preview_core::host::protocol::{Message, Packet, VERSION};
use bingux_preview_core::host::shm::Shared;

/// A running host and our end of its socket.
pub struct Host {
    pub child: Child,
    pub channel: Channel,
    next_request: u32,
    /// Packets received while waiting for something else.
    pub backlog: Vec<Received>,
    /// Kill the process on drop; a test that watches it end on its own clears this.
    pub owned: bool,
}

impl Host {
    pub fn spawn() -> Host {
        let (ours, theirs) = Channel::pair().expect("a socket pair");
        let child_fd = theirs.as_raw_fd();
        let mut command = Command::new(env!("CARGO_BIN_EXE_bingux-preview-host"));
        command.args(["--audio-sink", "fakesink"]).stdin(Stdio::null()).env("BINGUX_PREVIEW_DEBUG", "1");
        // SAFETY: dup2 is async-signal-safe; it puts the child's end on fd 3
        // without close-on-exec.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(child_fd, 3) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().expect("the host starts");
        drop(theirs);
        let mut host = Host { child, channel: ours, next_request: 1, backlog: Vec::new(), owned: true };
        host.send(Message::Hello { version: VERSION });
        let ready = host.wait(Duration::from_secs(10), |message| matches!(message, Message::Ready { .. }));
        assert!(ready.is_some(), "the host says Ready");
        host
    }

    pub fn request(&mut self) -> u32 {
        self.next_request += 1;
        self.next_request
    }

    pub fn send(&mut self, message: Message) -> u32 {
        let request = self.request();
        self.channel.send(&Packet { request, message }, &[]).expect("send");
        request
    }

    pub fn send_file(&mut self, message: Message, files: &[&Path]) -> u32 {
        let request = self.request();
        let opened: Vec<File> = files.iter().map(|path| File::open(path).expect("fixture")).collect();
        let raw: Vec<i32> = opened.iter().map(AsRawFd::as_raw_fd).collect();
        self.channel.send(&Packet { request, message }, &raw).expect("send");
        request
    }

    /// The next packet within `timeout`, from the backlog first.
    pub fn next(&mut self, timeout: Duration) -> Option<Received> {
        if !self.backlog.is_empty() {
            return Some(self.backlog.remove(0));
        }
        if !self.channel.readable(timeout.as_millis() as i32) {
            return None;
        }
        match self.channel.recv() {
            Ok(received) => Some(received),
            Err(RecvError::Closed) => None,
            Err(error) => panic!("the host sent something unreadable: {error}"),
        }
    }

    /// Wait for a packet `accept`ed, up to `timeout`. Everything before it is
    /// dropped, so a test names what it needs.
    pub fn wait(&mut self, timeout: Duration, accept: impl Fn(&Message) -> bool) -> Option<Received> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let received = self.next(left)?;
            if accept(&received.packet.message) {
                return Some(received);
            }
        }
    }

    /// Collect every packet until one `accept`s (it is included), up to `timeout`.
    pub fn collect_until(&mut self, timeout: Duration, accept: impl Fn(&Message) -> bool) -> Vec<Received> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Some(received) = self.next(left) else { break };
            let done = accept(&received.packet.message);
            out.push(received);
            if done {
                break;
            }
        }
        out
    }

    /// Whether the process has ended within `timeout`.
    pub fn exited(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status);
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if self.owned {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// The pixels of a frame or page that came with `received`.
pub fn pixels(fd: OwnedFd, size: u32) -> Vec<u8> {
    Shared::map(fd, size as usize).expect("sealed memory of the promised size").copy_to(size as usize)
}

/// Where fixtures are written: a directory of this test process.
pub fn fixture_dir() -> &'static Path {
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let dir = std::env::temp_dir().join(format!("bingux-preview-host-fixtures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a fixture directory");
        dir
    });
    &DIR
}

/// Run a GStreamer pipeline description to its end.
fn run_pipeline(description: &str) {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    gst::init().expect("gstreamer");
    let pipeline = gst::parse::launch(description).unwrap_or_else(|error| panic!("{description}: {error}"));
    pipeline.set_state(gst::State::Playing).expect("playing");
    let bus = pipeline.bus().expect("bus");
    for message in bus.iter_timed(gst::ClockTime::from_seconds(60)) {
        match message.view() {
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(error) => panic!("{description}: {} ({:?})", error.error(), error.debug()),
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null).expect("null");
}

/// A two-second 320x180 clip at 30 fps with a keyframe every half second and
/// a sine tone: WebM with VP8 and Vorbis.
pub fn clip_webm() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = fixture_dir().join("clip.webm");
        run_pipeline(&format!(
            "videotestsrc num-buffers=60 ! video/x-raw,width=320,height=180,framerate=30/1 ! videoconvert ! vp8enc keyframe-max-dist=15 deadline=1 ! webmmux name=mux ! filesink location={} \
             audiotestsrc num-buffers=94 samplesperbuffer=1024 wave=sine ! audioconvert ! audio/x-raw,rate=48000 ! vorbisenc ! mux.",
            path.display()
        ));
        path
    });
    &PATH
}

/// The same clip as H.264 and AAC in MP4, the common case.
pub fn clip_mp4() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = fixture_dir().join("clip.mp4");
        run_pipeline(&format!(
            "videotestsrc num-buffers=60 ! video/x-raw,width=320,height=180,framerate=30/1 ! videoconvert ! video/x-raw,format=I420 ! x264enc key-int-max=15 tune=zerolatency ! h264parse ! mp4mux name=mux ! filesink location={} \
             audiotestsrc num-buffers=94 samplesperbuffer=1024 wave=sine ! audioconvert ! audio/x-raw,rate=48000 ! fdkaacenc ! mux.",
            path.display()
        ));
        path
    });
    &PATH
}

/// A two-second 22.05 kHz mono sine in a WAV file, written by hand.
pub fn tone_wav() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = fixture_dir().join("tone.wav");
        std::fs::write(&path, bingux_preview_core::testing::wav_tone(2)).expect("write wav");
        path
    });
    &PATH
}

/// A clip with a subtitle track: the WebM again, plus an SRT beside it.
pub fn clip_srt() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = fixture_dir().join("clip.srt");
        std::fs::write(&path, "1\n00:00:00,200 --> 00:00:01,000\n<i>Hello</i> there\n\n2\n00:00:01,200 --> 00:00:01,900\nSecond line\n\n").expect("write srt");
        path
    });
    &PATH
}

/// Bytes that are not any media.
pub fn junk(name: &str) -> PathBuf {
    let path = fixture_dir().join(name);
    let mut state = 0x9E3779B9u32;
    let bytes: Vec<u8> = (0..20_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    std::fs::write(&path, bytes).expect("write junk");
    path
}

/// A PDF of three Letter pages written by hand, with text we can search for.
///
/// Page 1 has "Invoice 105" near the top and "Total due" near the bottom,
/// page 2 "Second page", page 3 "Last page invoice".
pub fn three_page_pdf() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = fixture_dir().join("three.pdf");
        std::fs::write(&path, bingux_preview_core::testing::three_page_pdf()).expect("write pdf");
        path
    });
    &PATH
}

/// A sparse file bigger than the host's document cap, named like a PDF.
pub fn huge_pdf() -> PathBuf {
    let path = fixture_dir().join("huge.pdf");
    let file = File::create(&path).expect("create");
    file.set_len(600 << 20).expect("sparse");
    path
}
