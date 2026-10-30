//! The host against real files: it is spawned, given a descriptor, and judged
//! by the packets it sends. Fixtures are generated with GStreamer and by hand.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use bingux_preview_core::host::protocol::{ErrorCode, Message, OpenedKind};
use common::*;

const LONG: Duration = Duration::from_secs(20);

fn open(host: &mut Host, path: &Path, autoplay: bool) -> u32 {
    host.send_file(Message::OpenMedia { want_video: true, max_w: 160, max_h: 90, autoplay, subtitle_file: false }, &[path])
}

fn opened(host: &mut Host) -> Message {
    host.wait(LONG, |message| matches!(message, Message::Opened { .. } | Message::Error { .. })).expect("Opened or Error").packet.message
}

#[test]
fn a_video_opens_with_its_length_size_and_a_poster_frame() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    // The poster frame is the first picture the pipeline prerolls: it arrives
    // before `Opened`, without a Play.
    let mut packets = host.collect_until(LONG, |message| matches!(message, Message::Opened { .. } | Message::Error { .. }));
    let opened = packets.pop().expect("something").packet.message;
    let Message::Opened { kind, duration_ns, video_w, video_h, has_audio, seekable, .. } = opened else {
        panic!("the clip should open: {opened:?}");
    };
    assert_eq!(kind, OpenedKind::Video);
    assert!((1_900_000_000..=2_300_000_000).contains(&duration_ns), "duration {duration_ns}");
    assert_eq!((video_w, video_h), (320, 180));
    assert!(has_audio && seekable);

    let frame = match packets.into_iter().find(|received| matches!(received.packet.message, Message::FrameReady { .. })) {
        Some(frame) => frame,
        None => host.wait(LONG, |message| matches!(message, Message::FrameReady { .. })).expect("the poster frame"),
    };
    let Message::FrameReady { slot, width, height, stride, size, .. } = frame.packet.message else { unreachable!() };
    // 320x180 fitted inside 160x90: scaled down, aspect kept.
    assert_eq!((width, height), (160, 90));
    assert_eq!(stride, width * 4);
    assert_eq!(size, width * height * 4);
    let mut fds = frame.fds;
    assert_eq!(fds.len(), 1, "the first use of a slot carries its memory");
    let bytes = pixels(fds.remove(0), size);
    assert!(bytes.chunks_exact(4).any(|pixel| pixel[..3] != [0, 0, 0]), "the poster is not black");
    // videotestsrc draws colour bars: more than a few distinct colours.
    let mut colours: Vec<[u8; 3]> = bytes.chunks_exact(4).map(|pixel| [pixel[0], pixel[1], pixel[2]]).collect();
    colours.sort_unstable();
    colours.dedup();
    assert!(colours.len() > 4, "{} colours", colours.len());
    host.send(Message::FrameDone { slot });
}

#[test]
fn playing_sends_frames_and_positions_and_ends_with_end_of_stream() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    host.send(Message::Play);
    let started = Instant::now();
    let mut frames = 0;
    let mut last_position = 0;
    let mut saw_playing = false;
    let mut ended = false;
    let deadline = Instant::now() + LONG;
    while Instant::now() < deadline && !ended {
        let Some(received) = host.next(Duration::from_millis(500)) else { continue };
        match received.packet.message {
            Message::FrameReady { slot, .. } => {
                frames += 1;
                host.send(Message::FrameDone { slot });
            }
            Message::Position { position_ns, .. } => {
                assert!(position_ns + 1_000_000 >= last_position, "the position went backwards: {position_ns} after {last_position}");
                last_position = position_ns;
            }
            Message::State { playing, ended: end, .. } => {
                saw_playing |= playing;
                ended |= end;
            }
            Message::Error { message, .. } => panic!("playback failed: {message}"),
            _ => {}
        }
    }
    assert!(ended, "the end of the stream is reported");
    assert!(saw_playing, "the host reported that it plays");
    assert!(frames >= 20, "{frames} frames for 2 s of video");
    assert!(started.elapsed() > Duration::from_millis(1500), "it played in real time, not instantly");
    assert!(last_position >= 1_800_000_000, "position {last_position}");
}

#[test]
fn an_accurate_seek_lands_on_the_requested_time_and_shows_that_frame() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    // Everything the seek causes, up to its answer: the new frame comes first.
    let request = host.send(Message::Seek { ns: 1_000_000_000, accurate: true });
    let packets = host.collect_until(LONG, |message| matches!(message, Message::Position { .. }));
    let _ = request;
    let mut packets = packets;
    // Positions sent before the seek are not its answer.
    while !packets.iter().any(|received| received.packet.request == request && matches!(received.packet.message, Message::Position { .. })) {
        let more = host.collect_until(LONG, |message| matches!(message, Message::Position { .. }));
        assert!(!more.is_empty(), "the seek is answered");
        packets.extend(more);
    }
    let answer = packets.iter().find(|received| received.packet.request == request && matches!(received.packet.message, Message::Position { .. })).unwrap();
    let Message::Position { position_ns, .. } = answer.packet.message else { unreachable!() };
    assert!(position_ns.abs_diff(1_000_000_000) < 80_000_000, "landed at {position_ns}");
    let mut frame_seen = packets.iter().any(|received| matches!(received.packet.message, Message::FrameReady { .. }));
    if !frame_seen {
        frame_seen = host.wait(LONG, |message| matches!(message, Message::FrameReady { .. })).is_some();
    }
    assert!(frame_seen, "a frame of the new place is shown while paused");
}

#[test]
fn a_keyframe_seek_snaps_to_a_nearby_keyframe() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    let request = host.send(Message::Seek { ns: 1_200_000_000, accurate: false });
    let answer = host
        .wait(LONG, |message| matches!(message, Message::Position { .. }))
        .into_iter()
        .chain(std::iter::from_fn(|| host.wait(Duration::from_millis(200), |m| matches!(m, Message::Position { .. }))))
        .find(|received| received.packet.request == request);
    let Some(Message::Position { position_ns, .. }) = answer.map(|received| received.packet.message) else { panic!("answered") };
    // Keyframes are every 0.5 s: 1.0 s or 1.5 s, never far from the request.
    assert!(position_ns.abs_diff(1_200_000_000) <= 400_000_000, "landed at {position_ns}");
}

#[test]
fn faster_playback_finishes_sooner() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    host.send(Message::SetRate { permille: 2000 });
    host.send(Message::Play);
    let started = Instant::now();
    let end = host.wait(LONG, |message| matches!(message, Message::State { ended: true, .. }));
    assert!(end.is_some(), "the end of the stream");
    assert!(started.elapsed() < Duration::from_millis(1900), "2 s of video at 2x took {:?}", started.elapsed());
}

#[test]
fn volume_mute_and_a_new_size_do_not_disturb_playback() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    host.send(Message::SetVolume { permille: 300 });
    host.send(Message::SetMute { muted: true });
    host.send(Message::SetSize { max_w: 80, max_h: 80 });
    host.send(Message::Play);
    let mut small = None;
    let deadline = Instant::now() + LONG;
    while Instant::now() < deadline && small.is_none() {
        let Some(received) = host.next(Duration::from_millis(500)) else { continue };
        match received.packet.message {
            Message::FrameReady { slot, width, height, .. } => {
                host.send(Message::FrameDone { slot });
                if width <= 80 && height <= 80 {
                    small = Some((width, height, received.fds.len()));
                }
            }
            Message::Error { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    let (width, height, fds) = small.expect("frames fit the new size");
    assert_eq!((width, height), (80, 45), "aspect kept inside 80x80");
    assert_eq!(fds, 1, "a new size brings new memory");
}

#[test]
fn an_mp4_with_h264_and_aac_plays() {
    let mut host = Host::spawn();
    open(&mut host, clip_mp4(), true);
    let Message::Opened { kind, duration_ns, .. } = opened(&mut host) else { panic!("an H.264 clip opens") };
    assert_eq!(kind, OpenedKind::Video);
    assert!(duration_ns > 1_500_000_000);
    let mut frames = 0;
    let deadline = Instant::now() + LONG;
    while Instant::now() < deadline {
        let Some(received) = host.next(Duration::from_millis(500)) else { continue };
        match received.packet.message {
            Message::FrameReady { slot, .. } => {
                frames += 1;
                host.send(Message::FrameDone { slot });
            }
            Message::State { ended: true, .. } => break,
            Message::Error { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    assert!(frames >= 20, "{frames} frames");
}

#[test]
fn a_song_is_audio_without_frames_and_plays_to_the_end() {
    let mut host = Host::spawn();
    host.send_file(Message::OpenMedia { want_video: false, max_w: 160, max_h: 90, autoplay: true, subtitle_file: false }, &[tone_wav()]);
    let Message::Opened { kind, duration_ns, video_w, has_audio, .. } = opened(&mut host) else { panic!("the tone opens") };
    assert_eq!(kind, OpenedKind::Audio);
    assert!((1_900_000_000..=2_100_000_000).contains(&duration_ns), "{duration_ns}");
    assert_eq!(video_w, 0);
    assert!(has_audio);
    let packets = host.collect_until(LONG, |message| matches!(message, Message::State { ended: true, .. }));
    assert!(packets.iter().any(|received| matches!(received.packet.message, Message::State { ended: true, .. })), "end of stream");
    assert!(!packets.iter().any(|received| matches!(received.packet.message, Message::FrameReady { .. })), "no picture for a song");
}

#[test]
fn a_sidecar_subtitle_file_becomes_cues_and_a_track() {
    let mut host = Host::spawn();
    host.send_file(Message::OpenMedia { want_video: true, max_w: 160, max_h: 90, autoplay: true, subtitle_file: true }, &[clip_webm(), clip_srt()]);
    let mut tracks = Vec::new();
    let mut cues = Vec::new();
    let deadline = Instant::now() + LONG;
    while Instant::now() < deadline {
        let Some(received) = host.next(Duration::from_millis(500)) else { continue };
        match received.packet.message {
            Message::FrameReady { slot, .. } => {
                host.send(Message::FrameDone { slot });
            }
            Message::Tracks { subtitles, .. } if !subtitles.is_empty() => tracks = subtitles,
            Message::Cue { text, .. } if !text.is_empty() => cues.push(text),
            Message::State { ended: true, .. } => break,
            Message::Error { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    assert!(!tracks.is_empty(), "the sidecar is listed as a subtitle track");
    assert!(cues.iter().any(|cue| cue == "Hello there"), "markup removed from cues: {cues:?}");
}

#[test]
fn a_file_that_is_not_media_is_an_error_the_host_survives() {
    let mut host = Host::spawn();
    open(&mut host, &junk("junk.webm"), false);
    let Message::Error { code, message } = opened(&mut host) else { panic!("junk is refused") };
    assert!(matches!(code, ErrorCode::Unsupported | ErrorCode::Corrupt | ErrorCode::NoDecoder), "{code:?}: {message}");
    assert!(!message.is_empty());
    // The host still answers: it is told to close and does.
    host.send(Message::Close);
    let status = host.exited(Duration::from_secs(5)).expect("the host ends on Close");
    assert!(status.success(), "{status:?}");
}

#[test]
fn closing_the_socket_ends_the_host() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), true);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    host.owned = false;
    assert!(host.child.try_wait().unwrap().is_none(), "it plays on until its socket closes");
    host.channel.shutdown();
    let status = host.exited(Duration::from_secs(5)).expect("the host outlived its viewer");
    assert!(status.success(), "{status:?}");
}

#[test]
fn a_second_open_and_calls_for_the_wrong_kind_of_file_are_refused_not_fatal() {
    let mut host = Host::spawn();
    open(&mut host, clip_webm(), false);
    assert!(matches!(opened(&mut host), Message::Opened { .. }));
    open(&mut host, clip_webm(), false);
    let error = host.wait(LONG, |message| matches!(message, Message::Error { .. })).expect("refused");
    assert!(matches!(error.packet.message, Message::Error { code: ErrorCode::Internal, .. }));
    host.send(Message::RenderPage { page: 0, scale_milli: 1000, max_w: 100, max_h: 100 });
    let error = host.wait(LONG, |message| matches!(message, Message::Error { .. })).expect("refused");
    assert!(matches!(error.packet.message, Message::Error { code: ErrorCode::Unsupported, .. }));
    // Still alive: a seek is answered.
    let request = host.send(Message::Seek { ns: 500_000_000, accurate: true });
    let mut answered = false;
    while let Some(received) = host.wait(LONG, |message| matches!(message, Message::Position { .. })) {
        if received.packet.request == request {
            answered = true;
            break;
        }
    }
    assert!(answered);
}
