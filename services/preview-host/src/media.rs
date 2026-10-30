//! Playing a film or a song with GStreamer's `playbin3`.
//!
//! The pipeline is `playbin3` with a video sink of
//! `videoconvert ! videoscale ! capsfilter ! appsink` whose caps fit the
//! picture inside the size the viewer asked for (the frames are scaled here,
//! so a 4K film costs the viewer a pane-sized copy), an audio sink that is the
//! default output (or `--audio-sink` for tests) and a text sink that turns
//! subtitles into `Cue` packets instead of burning them into the picture.
//!
//! Threads: the caller's thread reads the viewer's packets and runs commands;
//! GStreamer's streaming thread runs the appsink callbacks (copy a frame into
//! a slot, announce it); a bus thread turns bus messages into packets; a
//! poller reports the position while playing. They share [`Core`].

use std::os::fd::{IntoRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bingux_preview_core::host::protocol::{ErrorCode, Message, OpenedKind};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::VideoFrameExt;

use crate::frames::Frames;
use crate::link::Link;

/// What the viewer asked for when it opened the media.
pub struct OpenRequest {
    pub request: u32,
    pub want_video: bool,
    pub max_w: u32,
    pub max_h: u32,
    pub autoplay: bool,
    pub file: OwnedFd,
    pub subtitle_file: Option<OwnedFd>,
}

/// Options of the host process, not of one file.
#[derive(Clone, Default)]
pub struct Options {
    /// A GStreamer element for the audio output instead of the default sink
    /// (tests use `fakesink`).
    pub audio_sink: Option<String>,
}

#[derive(Default)]
struct Inner {
    want_playing: bool,
    /// Paused by the host while the pipeline buffers.
    waiting_for_data: bool,
    ended: bool,
    buffering: u8,
    opened: bool,
    autoplay: bool,
    /// The request of the last seek, answered with a `Position` when it is done.
    seek_request: Option<u32>,
    rate: f64,
    title: String,
    collection: Option<gst::StreamCollection>,
    subtitle: i32,
    missing: Vec<String>,
    video_size: (u32, u32),
    /// The pipeline reported an error.
    failed: bool,
}

struct Core {
    link: Arc<Link>,
    pipeline: gst::Element,
    fit: gst::Element,
    scale: gst::Element,
    frames: Arc<Frames>,
    inner: Mutex<Inner>,
    request: u32,
}

pub struct Media {
    core: Arc<Core>,
}

fn nanos(time: Option<gst::ClockTime>) -> u64 {
    time.map_or(0, gst::ClockTime::nseconds)
}

/// The caps of frames that fit inside `max_w` by `max_h`: `videoscale` keeps
/// the aspect ratio when the size is a range, and never enlarges.
fn fit_caps(max_w: u32, max_h: u32) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "RGBx")
        .field("width", gst::IntRange::new(1, max_w.clamp(1, 8192) as i32))
        .field("height", gst::IntRange::new(1, max_h.clamp(1, 8192) as i32))
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// A volume of thousandths as the pipeline's linear amplitude. Loudness is
/// not linear, so the slider is cubic: half way is an eighth of the amplitude.
fn amplitude(permille: u32) -> f64 {
    let fraction = f64::from(permille.min(1000)) / 1000.0;
    fraction * fraction * fraction
}

/// Remove the markup of a subtitle (`<i>`, `<font ...>`).
pub fn plain_cue(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            // A `<` starts a tag only before a name or a slash: "2 < 3" is text.
            '<' if !inside && chars.peek().is_some_and(|next| next.is_alphabetic() || *next == '/') => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(character),
            _ => {}
        }
    }
    out.trim().replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

impl Media {
    /// Build the pipeline, start prerolling and return. `Opened` follows from
    /// the bus thread when the first frame is ready, or `Error` when it cannot be.
    pub fn open(link: Arc<Link>, options: &Options, open: OpenRequest) -> Result<Media, (ErrorCode, String)> {
        gst::init().map_err(|error| (ErrorCode::NoDecoder, format!("GStreamer did not start: {error}")))?;
        let internal = |what: &str, error: &dyn std::fmt::Display| (ErrorCode::Internal, format!("The player could not start ({what}: {error})."));

        let fd = open.file.into_raw_fd();
        let playbin = gst::ElementFactory::make("playbin3")
            .property("uri", format!("file:///proc/self/fd/{fd}"))
            .build()
            .map_err(|_| (ErrorCode::NoDecoder, "GStreamer's playbin3 is not installed.".to_owned()))?;
        if let Some(subtitle) = open.subtitle_file {
            playbin.set_property("suburi", format!("file:///proc/self/fd/{}", subtitle.into_raw_fd()));
        }

        let flags = if open.want_video { "audio+video+text+soft-volume" } else { "audio+soft-volume" };
        playbin.set_property_from_str("flags", flags);

        let bin = gst::parse::bin_from_description("videoconvert ! videoscale name=scale ! capsfilter name=fit ! appsink name=out", true)
            .map_err(|error| internal("video bin", &error))?;
        let fit = bin.by_name("fit").ok_or_else(|| internal("video bin", &"no capsfilter"))?;
        let scale = bin.by_name("scale").ok_or_else(|| internal("video bin", &"no videoscale"))?;
        fit.set_property("caps", fit_caps(open.max_w, open.max_h));
        let video_out = bin
            .by_name("out")
            .and_then(|element| element.downcast::<gst_app::AppSink>().ok())
            .ok_or_else(|| internal("video bin", &"no appsink"))?;
        video_out.set_property("sync", true);
        video_out.set_max_buffers(1);
        playbin.set_property("video-sink", &bin);

        let frames = Frames::new(open.max_w, open.max_h);
        let core = Arc::new(Core {
            link: Arc::clone(&link),
            pipeline: playbin.clone(),
            fit,
            scale,
            frames,
            inner: Mutex::new(Inner { rate: 1.0, buffering: 100, autoplay: open.autoplay, subtitle: -1, ..Inner::default() }),
            request: open.request,
        });

        let for_sample = Arc::clone(&core);
        let for_preroll = Arc::clone(&core);
        video_out.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    if let Ok(sample) = sink.pull_sample() {
                        deliver(&for_sample, &sample);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .new_preroll(move |sink| {
                    if let Ok(sample) = sink.pull_preroll() {
                        deliver(&for_preroll, &sample);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // Subtitles become packets.
        let cues = gst_app::AppSink::builder()
            .caps(&gst::Caps::builder("text/x-raw").build())
            .sync(true)
            .build();
        let for_cue = Arc::clone(&core);
        cues.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    if let Ok(sample) = sink.pull_sample() {
                        if let Some(buffer) = sample.buffer() {
                            let until = buffer.pts().zip(buffer.duration()).map_or(0, |(pts, length)| (pts + length).nseconds());
                            let text = buffer
                                .map_readable()
                                .map(|map| plain_cue(&String::from_utf8_lossy(map.as_slice())))
                                .unwrap_or_default();
                            for_cue.link.send(0, Message::Cue { text, until_ns: until });
                        }
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        playbin.set_property("text-sink", &cues);

        // Audio goes to the default output; a speed change keeps the pitch.
        if let Some(name) = &options.audio_sink {
            let sink = gst::ElementFactory::make(name)
                .build()
                .map_err(|_| (ErrorCode::Internal, format!("No audio output named {name}.")))?;
            if sink.has_property("sync") {
                sink.set_property("sync", true);
            }
            playbin.set_property("audio-sink", &sink);
        }
        if let Ok(tempo) = gst::ElementFactory::make("scaletempo").build() {
            playbin.set_property("audio-filter", &tempo);
        }

        let bus = playbin.bus().ok_or_else(|| internal("bus", &"none"))?;
        let for_bus = Arc::clone(&core);
        std::thread::Builder::new()
            .name("bus".into())
            .spawn(move || watch_bus(&for_bus, &bus))
            .map_err(|error| internal("thread", &error))?;
        let for_poll = Arc::clone(&core);
        std::thread::Builder::new()
            .name("position".into())
            .spawn(move || poll_position(&for_poll))
            .map_err(|error| internal("thread", &error))?;

        if let Err(error) = playbin.set_state(gst::State::Paused) {
            return Err((ErrorCode::Corrupt, format!("The file could not be opened ({error}).")));
        }
        Ok(Media { core })
    }

    /// Stop the host when the file has neither opened nor failed within
    /// `limit`: a decoder stuck on a hostile file cannot be interrupted.
    pub fn watch_open(&self, link: Arc<Link>, limit: Duration) {
        let core = Arc::clone(&self.core);
        let spawned = std::thread::Builder::new().name("open-timeout".into()).spawn(move || {
            std::thread::sleep(limit);
            let settled = {
                let inner = core.lock();
                inner.opened || inner.failed
            };
            if !settled {
                link.error(core.request, ErrorCode::Internal, "The file took too long to open and was stopped.");
                std::process::exit(124);
            }
        });
        if spawned.is_err() {
            eprintln!("bingux-preview-host: no open timeout thread");
        }
    }

    pub fn frame_done(&self, slot: u8) {
        self.core.frames.done(slot);
    }

    pub fn play(&self) {
        let mut inner = self.core.lock();
        inner.want_playing = true;
        if !inner.waiting_for_data {
            let _ = self.core.pipeline.set_state(gst::State::Playing);
        }
    }

    pub fn pause(&self) {
        let mut inner = self.core.lock();
        inner.want_playing = false;
        inner.waiting_for_data = false;
        let _ = self.core.pipeline.set_state(gst::State::Paused);
    }

    pub fn seek(&self, request: u32, ns: u64, accurate: bool) {
        let rate = {
            let mut inner = self.core.lock();
            inner.seek_request = Some(request);
            inner.ended = false;
            inner.rate
        };
        let flags = gst::SeekFlags::FLUSH | if accurate { gst::SeekFlags::ACCURATE } else { gst::SeekFlags::KEY_UNIT | gst::SeekFlags::SNAP_NEAREST };
        let result = self.core.pipeline.seek(
            rate,
            flags,
            gst::SeekType::Set,
            gst::ClockTime::from_nseconds(ns),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        );
        if result.is_err() {
            self.core.lock().seek_request = None;
            self.core.link.error(request, ErrorCode::Unsupported, "This file cannot be seeked.");
        }
    }

    pub fn set_volume(&self, permille: u32) {
        self.core.pipeline.set_property("volume", amplitude(permille));
    }

    pub fn set_mute(&self, muted: bool) {
        self.core.pipeline.set_property("mute", muted);
    }

    pub fn set_rate(&self, request: u32, permille: u32) {
        let rate = f64::from(permille.clamp(250, 4000)) / 1000.0;
        let position = self.core.pipeline.query_position::<gst::ClockTime>();
        {
            let mut inner = self.core.lock();
            inner.rate = rate;
            inner.seek_request = Some(request);
        }
        let _ = self.core.pipeline.seek(
            rate,
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::SeekType::Set,
            position.unwrap_or(gst::ClockTime::ZERO),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        );
    }

    pub fn set_size(&self, max_w: u32, max_h: u32) {
        self.core.frames.resize(max_w, max_h);
        self.core.fit.set_property("caps", fit_caps(max_w, max_h));
    }

    pub fn set_subtitle(&self, index: i32) {
        let ids = {
            let mut inner = self.core.lock();
            inner.subtitle = index;
            let Some(collection) = inner.collection.clone() else { return };
            selection(&collection, index)
        };
        if !ids.is_empty() {
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            self.core.pipeline.send_event(gst::event::SelectStreams::new(refs));
        }
    }

    /// Stop the pipeline. The process ends right after.
    pub fn close(&self) {
        if std::env::var_os("BINGUX_PREVIEW_DEBUG").is_some() {
            let (sent, dropped) = self.core.frames.counts();
            eprintln!("bingux-preview-host: {sent} frames sent, {dropped} dropped");
        }
        let _ = self.core.pipeline.set_state(gst::State::Null);
    }
}

impl Core {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state_packet(&self, inner: &Inner) -> Message {
        Message::State {
            playing: inner.want_playing && !inner.waiting_for_data && !inner.ended,
            buffering: inner.buffering,
            ended: inner.ended,
        }
    }

    /// The size of the picture before it is scaled to fit.
    fn native_size(&self) -> Option<(u32, u32)> {
        let caps = self.scale.static_pad("sink")?.current_caps()?;
        let structure = caps.structure(0)?;
        Some((structure.get::<i32>("width").ok()? as u32, structure.get::<i32>("height").ok()? as u32))
    }

    fn position(&self) -> u64 {
        nanos(self.pipeline.query_position::<gst::ClockTime>())
    }

    fn duration(&self) -> u64 {
        nanos(self.pipeline.query_duration::<gst::ClockTime>())
    }

    /// How far the data is there, in nanoseconds. A local file is all there.
    fn buffered(&self, position: u64) -> u64 {
        let mut query = gst::query::Buffering::new(gst::Format::Time);
        if self.pipeline.query(&mut query) {
            let (_, stop, _) = query.range();
            if let gst::GenericFormattedValue::Time(Some(stop)) = stop {
                if stop.nseconds() >= position {
                    return stop.nseconds();
                }
            }
        }
        self.duration().max(position)
    }

    fn seekable(&self) -> bool {
        let mut query = gst::query::Seeking::new(gst::Format::Time);
        self.pipeline.query(&mut query) && query.result().0
    }

    fn send_position(&self, request: u32) {
        let position = self.position();
        let buffered = self.buffered(position);
        self.link.send(request, Message::Position { position_ns: position, buffered_ns: buffered });
    }
}

/// Copy a decoded frame into a slot and announce it.
fn deliver(core: &Core, sample: &gst::Sample) {
    let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) else { return };
    let Ok(info) = gst_video::VideoInfo::from_caps(caps) else { return };
    let Ok(frame) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info) else { return };
    let (width, height) = (info.width(), info.height());
    let stride = frame.plane_stride()[0] as usize;
    let Ok(data) = frame.plane_data(0) else { return };
    let pts = nanos(buffer.pts());
    core.frames.publish(&core.link, width, height, pts, data.chunks(stride.max(1)).take(height as usize));
}

/// The stream ids to select for subtitle track `index` (`-1` for none): the
/// first video stream, the first audio stream and the chosen text stream.
fn selection(collection: &gst::StreamCollection, index: i32) -> Vec<String> {
    let mut ids = Vec::new();
    let mut video = false;
    let mut audio = false;
    let mut text = 0;
    for stream in collection.iter() {
        let Some(id) = stream.stream_id() else { continue };
        let kind = stream.stream_type();
        if kind.contains(gst::StreamType::VIDEO) && !video {
            video = true;
            ids.push(id.to_string());
        } else if kind.contains(gst::StreamType::AUDIO) && !audio {
            audio = true;
            ids.push(id.to_string());
        } else if kind.contains(gst::StreamType::TEXT) {
            if text == index {
                ids.push(id.to_string());
            }
            text += 1;
        }
    }
    ids
}

fn stream_name(stream: &gst::Stream, ordinal: usize) -> String {
    let tags = stream.tags();
    let title = tags.as_ref().and_then(|tags| tags.get::<gst::tags::Title>().map(|value| value.get().to_owned()));
    let language = tags.as_ref().and_then(|tags| {
        tags.get::<gst::tags::LanguageName>()
            .map(|value| value.get().to_owned())
            .or_else(|| tags.get::<gst::tags::LanguageCode>().map(|value| value.get().to_owned()))
    });
    match (title, language) {
        (Some(title), Some(language)) if title != language => format!("{title} ({language})"),
        (Some(name), _) | (None, Some(name)) => name,
        (None, None) => format!("Track {}", ordinal + 1),
    }
}

/// The names of the audio and subtitle tracks of a collection.
fn tracks_of(collection: &gst::StreamCollection) -> (Vec<String>, Vec<String>) {
    let (mut audio, mut text) = (Vec::new(), Vec::new());
    for stream in collection.iter() {
        let kind = stream.stream_type();
        if kind.contains(gst::StreamType::AUDIO) {
            audio.push(stream_name(&stream, audio.len()));
        } else if kind.contains(gst::StreamType::TEXT) {
            text.push(stream_name(&stream, text.len()));
        }
    }
    (audio, text)
}

fn video_stream_size(collection: &gst::StreamCollection) -> Option<(u32, u32)> {
    collection.iter().find(|stream| stream.stream_type().contains(gst::StreamType::VIDEO)).and_then(|stream| {
        let caps = stream.caps()?;
        let structure = caps.structure(0)?;
        Some((structure.get::<i32>("width").ok()? as u32, structure.get::<i32>("height").ok()? as u32))
    })
}

fn classify(error: &glib::Error, missing: &[String]) -> (ErrorCode, String) {
    if error.matches(gst::StreamError::CodecNotFound) || error.matches(gst::CoreError::MissingPlugin) {
        let what = missing.first().cloned().unwrap_or_else(|| "this format".to_owned());
        return (ErrorCode::NoDecoder, format!("This computer has no decoder for {what}."));
    }
    if error.matches(gst::ResourceError::NotFound) || error.matches(gst::ResourceError::OpenRead) {
        return (ErrorCode::NotFound, "The file could not be opened.".to_owned());
    }
    if error.matches(gst::StreamError::WrongType) || error.matches(gst::StreamError::TypeNotFound) {
        return (ErrorCode::Unsupported, "This kind of file cannot be played.".to_owned());
    }
    if error.matches(gst::StreamError::Decode) || error.matches(gst::StreamError::Demux) || error.matches(gst::StreamError::Format) {
        return (ErrorCode::Corrupt, "This file is damaged and cannot be played.".to_owned());
    }
    (ErrorCode::Internal, format!("Playback failed: {}.", error.message().trim_end_matches('.')))
}

fn watch_bus(core: &Core, bus: &gst::Bus) {
    use gst::MessageView as View;
    loop {
        let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(500)) else { continue };
        let from_pipeline = message.src().is_some_and(|source| source == core.pipeline.upcast_ref::<gst::Object>());
        match message.view() {
            View::Error(error) => {
                let (code, text) = classify(&error.error(), &core.lock().missing);
                if std::env::var_os("BINGUX_PREVIEW_DEBUG").is_some() {
                    eprintln!("bingux-preview-host: {} ({:?})", error.error(), error.debug());
                }
                core.lock().failed = true;
                core.link.error(core.request, code, &text);
            }
            View::Warning(warning) => {
                if std::env::var_os("BINGUX_PREVIEW_DEBUG").is_some() {
                    eprintln!("bingux-preview-host: warning: {} ({:?})", warning.error(), warning.debug());
                }
            }
            View::Eos(_) => {
                let (position, duration) = (core.position(), core.duration());
                let mut inner = core.lock();
                inner.ended = true;
                let state = core.state_packet(&inner);
                drop(inner);
                core.link.send(0, Message::Position { position_ns: duration.max(position), buffered_ns: duration });
                core.link.send(0, state);
            }
            View::Buffering(buffering) => {
                let percent = buffering.percent().clamp(0, 100) as u8;
                let mut inner = core.lock();
                inner.buffering = percent;
                if percent < 100 && inner.want_playing && !inner.waiting_for_data {
                    inner.waiting_for_data = true;
                    let _ = core.pipeline.set_state(gst::State::Paused);
                } else if percent == 100 && inner.waiting_for_data {
                    inner.waiting_for_data = false;
                    if inner.want_playing {
                        let _ = core.pipeline.set_state(gst::State::Playing);
                    }
                }
                let state = core.state_packet(&inner);
                drop(inner);
                core.link.send(0, state);
            }
            View::StreamCollection(collection) => {
                let collection = collection.stream_collection();
                let (audio, subtitles) = tracks_of(&collection);
                let opened = {
                    let mut inner = core.lock();
                    inner.collection = Some(collection.clone());
                    if let Some(size) = video_stream_size(&collection) {
                        inner.video_size = size;
                    }
                    inner.opened
                };
                if opened {
                    core.link.send(0, Message::Tracks { audio, subtitles });
                }
            }
            View::Tag(tags) => {
                if from_pipeline || message.src().is_some() {
                    if let Some(title) = tags.tags().get::<gst::tags::Title>() {
                        core.lock().title = title.get().to_owned();
                    }
                }
            }
            View::Element(element) => {
                if let Some(structure) = element.structure() {
                    if structure.name() == "missing-plugin" {
                        let description = structure.get::<String>("name").or_else(|_| structure.get::<String>("detail")).unwrap_or_default();
                        if !description.is_empty() {
                            core.lock().missing.push(description);
                        }
                    }
                }
            }
            View::AsyncDone(_) if from_pipeline => async_done(core),
            View::StateChanged(change) if from_pipeline => {
                if change.pending() == gst::State::VoidPending && matches!(change.current(), gst::State::Playing | gst::State::Paused) {
                    let inner = core.lock();
                    if inner.opened {
                        let state = core.state_packet(&inner);
                        drop(inner);
                        core.link.send(0, state);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A state change or a seek finished: the first one is the file opening.
fn async_done(core: &Core) {
    let first = {
        let mut inner = core.lock();
        let first = !inner.opened;
        inner.opened = true;
        first
    };
    if first {
        let (collection, title, mut size) = {
            let inner = core.lock();
            let known = (inner.video_size.0 > 0).then_some(inner.video_size);
            (inner.collection.clone(), inner.title.clone(), known.or_else(|| core.native_size()).unwrap_or((0, 0)))
        };
        if std::env::var_os("BINGUX_PREVIEW_DEBUG").is_some() {
            let scale_in = core.scale.static_pad("sink").and_then(|pad| pad.current_caps());
            eprintln!("bingux-preview-host: opened; native {:?}, collection {:?}, scaler input {scale_in:?}", core.native_size(), size);
        }
        let (audio, subtitles) = collection.as_ref().map(tracks_of).unwrap_or_default();
        let has_video = collection
            .as_ref()
            .map(|collection| collection.iter().any(|stream| stream.stream_type().contains(gst::StreamType::VIDEO)))
            .unwrap_or(size.0 > 0);
        if !has_video {
            size = (0, 0);
        }
        core.link.send(
            core.request,
            Message::Opened {
                kind: if has_video { OpenedKind::Video } else { OpenedKind::Audio },
                duration_ns: core.duration(),
                video_w: size.0,
                video_h: size.1,
                has_audio: !audio.is_empty(),
                seekable: core.seekable(),
                pages: 0,
                title,
            },
        );
        core.link.send(0, Message::Tracks { audio, subtitles });
        core.send_position(0);
        let autoplay = core.lock().autoplay;
        if autoplay {
            let mut inner = core.lock();
            inner.want_playing = true;
            let _ = core.pipeline.set_state(gst::State::Playing);
        }
        let inner = core.lock();
        let state = core.state_packet(&inner);
        drop(inner);
        core.link.send(0, state);
        return;
    }
    let request = core.lock().seek_request.take();
    if let Some(request) = request {
        core.send_position(request);
    }
}

fn poll_position(core: &Core) {
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let report = {
            let inner = core.lock();
            inner.opened && inner.want_playing && !inner.ended && inner.seek_request.is_none()
        };
        if report {
            core.send_position(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subtitle_loses_its_markup() {
        assert_eq!(plain_cue("<i>Hello</i> <font color=\"red\">there</font>\n"), "Hello there");
        assert_eq!(plain_cue("2 < 3"), "2 < 3");
        assert_eq!(plain_cue(""), "");
    }

    #[test]
    fn the_volume_slider_is_cubic() {
        assert_eq!(amplitude(0), 0.0);
        assert!((amplitude(500) - 0.125).abs() < 1e-9);
        assert_eq!(amplitude(1000), 1.0);
        assert_eq!(amplitude(5000), 1.0);
    }
}
