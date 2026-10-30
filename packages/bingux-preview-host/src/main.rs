//! `bingux-preview-host`: the helper process behind file previews.
//!
//! It plays audio and video (GStreamer) and renders documents (poppler) for a
//! viewer in another process, so a malformed file takes this process and not
//! the file manager. The viewer starts it with one end of a `SOCK_SEQPACKET`
//! socket pair as descriptor 3 (or `--fd N`), opens the file itself and hands
//! the descriptor over: the host never receives a path. It speaks
//! `bingux_preview_core::host::protocol`, answers one `Hello`, serves one file
//! and ends when the viewer closes the socket, sends `Close`, or dies.
//!
//! ```text
//! bingux-preview-host [--fd N] [--audio-sink ELEMENT]
//! bingux-preview-host --version | --check
//! ```
//!
//! `--audio-sink` replaces the default audio output with a GStreamer element
//! (`fakesink` in tests). `--check` initialises GStreamer and prints which
//! features work, one per line; it exits non-zero when the player is missing.
//! `BINGUX_PREVIEW_DEBUG=1` prints decoder errors to stderr.

mod document;
mod frames;
mod limits;
mod link;
mod media;

use std::os::fd::OwnedFd;
use std::sync::Arc;

use bingux_preview_core::host::channel::{Channel, Received, RecvError};
use bingux_preview_core::host::protocol::{ErrorCode, Message, VERSION, feature};

use document::{Job, Worker};
use link::{Link, Watchdog};
use media::{Media, OpenRequest, Options};

/// The descriptor the viewer's end of the socket arrives on.
const DEFAULT_FD: i32 = 3;

struct Args {
    fd: i32,
    options: Options,
}

enum Start {
    Run(Args),
    Version,
    Check,
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<Start, String> {
    let mut args = Args { fd: DEFAULT_FD, options: Options::default() };
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
            None => (argument, None),
        };
        let mut value = |what: &str| inline.clone().or_else(|| arguments.next()).ok_or(format!("{what} needs a value"));
        match name.as_str() {
            "--fd" => args.fd = value("--fd")?.parse().map_err(|_| "--fd needs a number".to_owned())?,
            "--audio-sink" => args.options.audio_sink = Some(value("--audio-sink")?),
            "--version" => return Ok(Start::Version),
            "--check" => return Ok(Start::Check),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Start::Run(args))
}

/// What the host can do on this machine, one `name: yes|no` line each.
fn check() -> bool {
    let player = gstreamer::init().is_ok() && gstreamer::ElementFactory::find("playbin3").is_some();
    println!("player: {}", if player { "yes" } else { "no" });
    for (name, element) in [("h264", "avdec_h264"), ("h264-openh264", "openh264dec"), ("vp8", "vp8dec"), ("vp9", "vp9dec"), ("aac", "avdec_aac"), ("opus", "opusdec"), ("vorbis", "vorbisdec"), ("flac", "flacdec"), ("mp3", "mpg123audiodec")] {
        let present = player && gstreamer::ElementFactory::find(element).is_some();
        println!("{name}: {}", if present { "yes" } else { "no" });
    }
    println!("document: yes");
    player
}

enum Session {
    None,
    Media(Media),
    Document(Worker),
}

fn main() {
    let start = match parse(std::env::args().skip(1)) {
        Ok(start) => start,
        Err(message) => {
            eprintln!("bingux-preview-host: {message}");
            std::process::exit(2);
        }
    };
    let args = match start {
        Start::Version => {
            println!("bingux-preview-host {} protocol {VERSION}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Start::Check => std::process::exit(if check() { 0 } else { 1 }),
        Start::Run(args) => args,
    };

    limits::harden();
    // SAFETY: the viewer started this process with the socket on `args.fd`
    // and nothing else here owns it.
    let channel = unsafe { Channel::from_raw_fd(args.fd) };
    let link = Arc::new(Link::new(channel));
    let watchdog = Watchdog::start(Arc::clone(&link), limits::JOB_TIME);

    match link.channel().recv() {
        Ok(Received { packet, .. }) if matches!(packet.message, Message::Hello { version } if version == VERSION) => {}
        Ok(_) | Err(_) => std::process::exit(2),
    }
    link.send(0, Message::Ready { version: VERSION, features: feature::MEDIA | feature::DOCUMENT });

    let mut session = Session::None;
    loop {
        match link.channel().recv() {
            Ok(received) => handle(&link, &watchdog, &args.options, &mut session, received),
            Err(RecvError::Closed) => end(&session),
            Err(error) => {
                link.error(0, ErrorCode::Internal, &format!("The preview host got a message it could not read: {error}."));
                end(&session)
            }
        }
    }
}

/// Stop and leave.
fn end(session: &Session) -> ! {
    if let Session::Media(media) = session {
        media.close();
    }
    std::process::exit(0);
}

fn take_fd(link: &Link, request: u32, fds: &mut Vec<OwnedFd>) -> Option<OwnedFd> {
    if fds.is_empty() {
        link.error(request, ErrorCode::Internal, "The viewer sent no file.");
        return None;
    }
    Some(fds.remove(0))
}

fn handle(link: &Arc<Link>, watchdog: &Arc<Watchdog>, options: &Options, session: &mut Session, received: Received) {
    let Received { packet, mut fds } = received;
    let request = packet.request;
    match (packet.message, &*session) {
        (Message::Close, _) => end(session),
        (Message::OpenMedia { want_video, max_w, max_h, autoplay, subtitle_file }, Session::None) => {
            let Some(file) = take_fd(link, request, &mut fds) else { return };
            let subtitle = if subtitle_file { take_fd(link, request, &mut fds) } else { None };
            limits::limit_media();
            let open = OpenRequest { request, want_video, max_w, max_h, autoplay, file, subtitle_file: subtitle };
            match Media::open(Arc::clone(link), options, open) {
                Ok(media) => {
                    media.watch_open(Arc::clone(link), limits::OPEN_TIME);
                    *session = Session::Media(media);
                }
                Err((code, message)) => link.error(request, code, &message),
            }
        }
        (Message::OpenDocument, Session::None) => {
            let Some(file) = take_fd(link, request, &mut fds) else { return };
            limits::limit_document();
            match Worker::start(Arc::clone(link), Arc::clone(watchdog)) {
                Ok(worker) => {
                    worker.submit(Job::Open { request, file });
                    *session = Session::Document(worker);
                }
                Err(error) => link.error(request, ErrorCode::Internal, &format!("The document worker did not start: {error}.")),
            }
        }
        (Message::OpenMedia { .. } | Message::OpenDocument, _) => {
            link.error(request, ErrorCode::Internal, "A host serves one file.");
        }
        (Message::Play, Session::Media(media)) => media.play(),
        (Message::Pause, Session::Media(media)) => media.pause(),
        (Message::Seek { ns, accurate }, Session::Media(media)) => media.seek(request, ns, accurate),
        (Message::SetVolume { permille }, Session::Media(media)) => media.set_volume(permille),
        (Message::SetMute { muted }, Session::Media(media)) => media.set_mute(muted),
        (Message::SetRate { permille }, Session::Media(media)) => media.set_rate(request, permille),
        (Message::SetSize { max_w, max_h }, Session::Media(media)) => media.set_size(max_w, max_h),
        (Message::SetSubtitle { index }, Session::Media(media)) => media.set_subtitle(index),
        (Message::FrameDone { slot }, Session::Media(media)) => media.frame_done(slot),
        (Message::RenderPage { page, scale_milli, max_w, max_h }, Session::Document(worker)) => {
            worker.submit(Job::Render { request, page, scale_milli, max_w, max_h });
        }
        (Message::PageText { page }, Session::Document(worker)) => worker.submit(Job::Text { request, page }),
        (Message::Search { query }, Session::Document(worker)) => worker.submit(Job::Search { request, query }),
        (Message::Cancel { target }, Session::Document(worker)) => worker.cancel(target),
        (Message::Cancel { .. }, _) => {}
        (Message::Hello { .. }, _) => {
            link.error(request, ErrorCode::Internal, "Hello was already said.");
            end(session);
        }
        (_, _) => link.error(request, ErrorCode::Unsupported, "This host does not do that for this file."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(arguments: &[&str]) -> Result<Start, String> {
        parse(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn arguments_are_the_socket_the_sink_and_the_two_queries() {
        let Ok(Start::Run(args)) = parse_all(&[]) else { panic!("defaults") };
        assert_eq!(args.fd, DEFAULT_FD);
        assert!(args.options.audio_sink.is_none());
        let Ok(Start::Run(args)) = parse_all(&["--fd", "7", "--audio-sink=fakesink"]) else { panic!("run") };
        assert_eq!(args.fd, 7);
        assert_eq!(args.options.audio_sink.as_deref(), Some("fakesink"));
        assert!(matches!(parse_all(&["--version"]), Ok(Start::Version)));
        assert!(matches!(parse_all(&["--check"]), Ok(Start::Check)));
        assert!(parse_all(&["--fd"]).is_err());
        assert!(parse_all(&["--fd", "x"]).is_err());
        assert!(parse_all(&["--frobnicate"]).is_err());
    }
}
