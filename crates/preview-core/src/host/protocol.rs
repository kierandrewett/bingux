//! The wire format between a viewer and `bingux-preview-host`, version 1.
//!
//! One message is one datagram on a `SOCK_SEQPACKET` socket, so there is no
//! framing to get wrong. A message is an 8-byte header and the fields of its
//! tag, all little-endian:
//!
//! ```text
//! u8 tag | u8 version (1) | u16 reserved (0) | u32 request | fields...
//! ```
//!
//! Strings are a `u16` length and UTF-8 bytes; lists are a `u16` count and
//! their items. A message is at most [`MAX_MESSAGE`] bytes. Decoding is a total
//! function: bytes that are too long, cut short, carry an unknown tag, a wrong
//! version or invalid text are a [`ProtocolError`], never a panic, and the
//! session ends. File descriptors (the file to open, a shared frame) travel
//! beside the message as `SCM_RIGHTS`, never inside it; see
//! [`channel`](super::channel).
//!
//! `request` ties a reply to what asked for it (a page render, a search). The
//! host answers an unknown request id for `Cancel` with nothing.

use std::fmt;

/// The version this build speaks.
pub const VERSION: u8 = 1;
/// The largest message, in bytes.
pub const MAX_MESSAGE: usize = 4096;
/// The words of one `PageText` message at most.
pub const MAX_WORDS: usize = 24;
/// The most tracks one `Tracks` message lists.
pub const MAX_TRACKS: usize = 16;
/// The most pages one `PageSizes` message lists.
pub const MAX_PAGE_SIZES: usize = 256;
/// The most pages the host opens; a longer document is shown up to here.
pub const MAX_PAGES: u32 = 5000;

/// What the host can do, from `Ready`.
pub mod feature {
    pub const MEDIA: u32 = 1;
    pub const DOCUMENT: u32 = 2;
}

/// Why the host gives up, from `Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// The file would not open.
    NotFound,
    /// The type is not one the host plays or renders.
    Unsupported,
    /// The data is damaged.
    Corrupt,
    /// A decoder (a codec plugin) is missing.
    NoDecoder,
    /// The host's own failure.
    Internal,
}

/// What the host opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenedKind {
    Audio,
    Video,
    Document,
}

/// A rectangle on a page, in points from its top left corner, in thousandths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// A word of a page and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub rect: Rect,
    pub text: String,
}

/// One message.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    // ---- viewer to host ----
    Hello { version: u8 },
    /// Open the media on the first descriptor, and a subtitle file on the
    /// second when `subtitle_file` is set. Frames come at most `max_w` by
    /// `max_h`.
    OpenMedia { want_video: bool, max_w: u32, max_h: u32, autoplay: bool, subtitle_file: bool },
    /// Open the document on the first descriptor.
    OpenDocument,
    Play,
    Pause,
    /// Move to `ns`. `accurate` lands on that exact frame (slower); otherwise
    /// the host snaps to the nearest keyframe (fast, for dragging a bar). The
    /// host answers with a `Position` carrying this request's id once the
    /// move is done.
    Seek { ns: u64, accurate: bool },
    /// Volume in thousandths: 1000 is full.
    SetVolume { permille: u32 },
    SetMute { muted: bool },
    /// Speed in thousandths: 1000 is normal.
    SetRate { permille: u32 },
    SetSize { max_w: u32, max_h: u32 },
    /// Show subtitle track `index`, or none (`-1`).
    SetSubtitle { index: i32 },
    /// The viewer has copied the frame in `slot`; the host may write it again.
    FrameDone { slot: u8 },
    RenderPage { page: u32, scale_milli: u32, max_w: u32, max_h: u32 },
    PageText { page: u32 },
    Search { query: String },
    /// Stop work for the request `target`.
    Cancel { target: u32 },
    Close,

    // ---- host to viewer ----
    Ready { version: u8, features: u32 },
    Opened {
        kind: OpenedKind,
        duration_ns: u64,
        video_w: u32,
        video_h: u32,
        has_audio: bool,
        seekable: bool,
        pages: u32,
        title: String,
    },
    /// A frame is in `slot`; the first use of a slot carries its memory
    /// (`size` bytes, RGBA, opaque) as a descriptor.
    FrameReady { slot: u8, width: u32, height: u32, stride: u32, size: u32, pts_ns: u64 },
    /// A page is rendered into the descriptor that came with this message.
    PageReady { page: u32, width: u32, height: u32, stride: u32, size: u32 },
    State { playing: bool, buffering: u8, ended: bool },
    Position { position_ns: u64, buffered_ns: u64 },
    Tracks { audio: Vec<String>, subtitles: Vec<String> },
    /// A subtitle to show until `until_ns`; empty text clears it.
    Cue { text: String, until_ns: u64 },
    /// Words of a page; `more` says another message follows.
    PageTextIs { page: u32, words: Vec<Word>, more: bool },
    SearchHit { page: u32, rects: Vec<Rect> },
    /// The size of pages `first..` in thousandths of a point, sent after
    /// `Opened` until every page is listed.
    PageSizes { first: u32, sizes: Vec<(u32, u32)> },
    /// A search finished: `hits` words matched over all pages.
    SearchDone { hits: u32 },
    Error { code: ErrorCode, message: String },
}

/// Why bytes are not a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    TooLong,
    Truncated,
    UnknownTag(u8),
    UnknownVersion(u8),
    BadValue,
    BadText,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::TooLong => f.write_str("message too long"),
            ProtocolError::Truncated => f.write_str("message cut short"),
            ProtocolError::UnknownTag(tag) => write!(f, "unknown message {tag}"),
            ProtocolError::UnknownVersion(version) => write!(f, "unknown protocol version {version}"),
            ProtocolError::BadValue => f.write_str("a value out of range"),
            ProtocolError::BadText => f.write_str("text that is not UTF-8"),
        }
    }
}

impl std::error::Error for ProtocolError {}

mod tag {
    pub const HELLO: u8 = 1;
    pub const OPEN_MEDIA: u8 = 2;
    pub const OPEN_DOCUMENT: u8 = 3;
    pub const PLAY: u8 = 4;
    pub const PAUSE: u8 = 5;
    pub const SEEK: u8 = 6;
    pub const SET_VOLUME: u8 = 7;
    pub const SET_MUTE: u8 = 8;
    pub const SET_RATE: u8 = 9;
    pub const SET_SIZE: u8 = 10;
    pub const SET_SUBTITLE: u8 = 11;
    pub const FRAME_DONE: u8 = 12;
    pub const RENDER_PAGE: u8 = 13;
    pub const PAGE_TEXT: u8 = 14;
    pub const SEARCH: u8 = 15;
    pub const CANCEL: u8 = 16;
    pub const CLOSE: u8 = 17;
    pub const READY: u8 = 64;
    pub const OPENED: u8 = 65;
    pub const FRAME_READY: u8 = 66;
    pub const PAGE_READY: u8 = 67;
    pub const STATE: u8 = 68;
    pub const POSITION: u8 = 69;
    pub const TRACKS: u8 = 70;
    pub const CUE: u8 = 71;
    pub const PAGE_TEXT_IS: u8 = 72;
    pub const SEARCH_HIT: u8 = 73;
    pub const ERROR: u8 = 74;
    pub const PAGE_SIZES: u8 = 75;
    pub const SEARCH_DONE: u8 = 76;
}

/// A message and the request it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub request: u32,
    pub message: Message,
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }
    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn str(&mut self, value: &str) {
        // Cut on a character: a name longer than a message allows is shortened.
        let mut end = value.len().min(512);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.u16(end as u16);
        self.0.extend_from_slice(&value.as_bytes()[..end]);
    }
    fn rect(&mut self, rect: Rect) {
        self.i32(rect.x0);
        self.i32(rect.y0);
        self.i32(rect.x1);
        self.i32(rect.y1);
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        if self.0.len() < count {
            return Err(ProtocolError::Truncated);
        }
        let (head, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, ProtocolError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::BadValue),
        }
    }
    fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| ProtocolError::Truncated)?))
    }
    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| ProtocolError::Truncated)?))
    }
    fn i32(&mut self) -> Result<i32, ProtocolError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().map_err(|_| ProtocolError::Truncated)?))
    }
    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| ProtocolError::Truncated)?))
    }
    fn str(&mut self) -> Result<String, ProtocolError> {
        let length = self.u16()? as usize;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| ProtocolError::BadText)
    }
    fn rect(&mut self) -> Result<Rect, ProtocolError> {
        Ok(Rect {
            x0: self.i32()?,
            y0: self.i32()?,
            x1: self.i32()?,
            y1: self.i32()?,
        })
    }
    fn count(&mut self, limit: usize) -> Result<usize, ProtocolError> {
        let count = self.u16()? as usize;
        if count > limit { Err(ProtocolError::BadValue) } else { Ok(count) }
    }
}

impl ErrorCode {
    fn to_u8(self) -> u8 {
        match self {
            ErrorCode::NotFound => 0,
            ErrorCode::Unsupported => 1,
            ErrorCode::Corrupt => 2,
            ErrorCode::NoDecoder => 3,
            ErrorCode::Internal => 4,
        }
    }
    fn from_u8(value: u8) -> Result<Self, ProtocolError> {
        Ok(match value {
            0 => ErrorCode::NotFound,
            1 => ErrorCode::Unsupported,
            2 => ErrorCode::Corrupt,
            3 => ErrorCode::NoDecoder,
            4 => ErrorCode::Internal,
            _ => return Err(ProtocolError::BadValue),
        })
    }
}

impl OpenedKind {
    fn to_u8(self) -> u8 {
        match self {
            OpenedKind::Audio => 0,
            OpenedKind::Video => 1,
            OpenedKind::Document => 2,
        }
    }
    fn from_u8(value: u8) -> Result<Self, ProtocolError> {
        Ok(match value {
            0 => OpenedKind::Audio,
            1 => OpenedKind::Video,
            2 => OpenedKind::Document,
            _ => return Err(ProtocolError::BadValue),
        })
    }
}

/// The bytes of `packet`.
pub fn encode(packet: &Packet) -> Vec<u8> {
    use Message::*;
    let mut out = Writer(Vec::with_capacity(64));
    let tag = match &packet.message {
        Hello { .. } => tag::HELLO,
        OpenMedia { .. } => tag::OPEN_MEDIA,
        OpenDocument => tag::OPEN_DOCUMENT,
        Play => tag::PLAY,
        Pause => tag::PAUSE,
        Seek { .. } => tag::SEEK,
        SetVolume { .. } => tag::SET_VOLUME,
        SetMute { .. } => tag::SET_MUTE,
        SetRate { .. } => tag::SET_RATE,
        SetSize { .. } => tag::SET_SIZE,
        SetSubtitle { .. } => tag::SET_SUBTITLE,
        FrameDone { .. } => tag::FRAME_DONE,
        RenderPage { .. } => tag::RENDER_PAGE,
        PageText { .. } => tag::PAGE_TEXT,
        Search { .. } => tag::SEARCH,
        Cancel { .. } => tag::CANCEL,
        Close => tag::CLOSE,
        Ready { .. } => tag::READY,
        Opened { .. } => tag::OPENED,
        FrameReady { .. } => tag::FRAME_READY,
        PageReady { .. } => tag::PAGE_READY,
        State { .. } => tag::STATE,
        Position { .. } => tag::POSITION,
        Tracks { .. } => tag::TRACKS,
        Cue { .. } => tag::CUE,
        PageTextIs { .. } => tag::PAGE_TEXT_IS,
        SearchHit { .. } => tag::SEARCH_HIT,
        PageSizes { .. } => tag::PAGE_SIZES,
        SearchDone { .. } => tag::SEARCH_DONE,
        Error { .. } => tag::ERROR,
    };
    out.u8(tag);
    out.u8(VERSION);
    out.u16(0);
    out.u32(packet.request);
    match &packet.message {
        Hello { version } => out.u8(*version),
        OpenMedia { want_video, max_w, max_h, autoplay, subtitle_file } => {
            out.bool(*want_video);
            out.u32(*max_w);
            out.u32(*max_h);
            out.bool(*autoplay);
            out.bool(*subtitle_file);
        }
        OpenDocument | Play | Pause | Close => {}
        Seek { ns, accurate } => {
            out.u64(*ns);
            out.bool(*accurate);
        }
        SetVolume { permille } | SetRate { permille } => out.u32(*permille),
        SetMute { muted } => out.bool(*muted),
        SetSize { max_w, max_h } => {
            out.u32(*max_w);
            out.u32(*max_h);
        }
        SetSubtitle { index } => out.i32(*index),
        FrameDone { slot } => out.u8(*slot),
        RenderPage { page, scale_milli, max_w, max_h } => {
            out.u32(*page);
            out.u32(*scale_milli);
            out.u32(*max_w);
            out.u32(*max_h);
        }
        PageText { page } => out.u32(*page),
        Search { query } => out.str(query),
        Cancel { target } => out.u32(*target),
        Ready { version, features } => {
            out.u8(*version);
            out.u32(*features);
        }
        Opened { kind, duration_ns, video_w, video_h, has_audio, seekable, pages, title } => {
            out.u8(kind.to_u8());
            out.u64(*duration_ns);
            out.u32(*video_w);
            out.u32(*video_h);
            out.bool(*has_audio);
            out.bool(*seekable);
            out.u32(*pages);
            out.str(title);
        }
        FrameReady { slot, width, height, stride, size, pts_ns } => {
            out.u8(*slot);
            out.u32(*width);
            out.u32(*height);
            out.u32(*stride);
            out.u32(*size);
            out.u64(*pts_ns);
        }
        PageReady { page, width, height, stride, size } => {
            out.u32(*page);
            out.u32(*width);
            out.u32(*height);
            out.u32(*stride);
            out.u32(*size);
        }
        State { playing, buffering, ended } => {
            out.bool(*playing);
            out.u8(*buffering);
            out.bool(*ended);
        }
        Position { position_ns, buffered_ns } => {
            out.u64(*position_ns);
            out.u64(*buffered_ns);
        }
        Tracks { audio, subtitles } => {
            for list in [audio, subtitles] {
                let list = &list[..list.len().min(MAX_TRACKS)];
                out.u16(list.len() as u16);
                for name in list {
                    out.str(name);
                }
            }
        }
        Cue { text, until_ns } => {
            out.str(text);
            out.u64(*until_ns);
        }
        PageTextIs { page, words, more } => {
            out.u32(*page);
            out.bool(*more);
            // As many words as fit: a message is at most `MAX_MESSAGE`.
            let mut budget = MAX_MESSAGE - 64;
            let fit = words
                .iter()
                .take(MAX_WORDS)
                .take_while(|word| {
                    let size = 16 + 2 + word.text.len().min(512);
                    let fits = size <= budget;
                    budget = budget.saturating_sub(size);
                    fits
                })
                .count();
            out.u16(fit as u16);
            for word in &words[..fit] {
                out.rect(word.rect);
                out.str(&word.text);
            }
        }
        SearchHit { page, rects } => {
            out.u32(*page);
            let rects = &rects[..rects.len().min(64)];
            out.u16(rects.len() as u16);
            for rect in rects {
                out.rect(*rect);
            }
        }
        PageSizes { first, sizes } => {
            out.u32(*first);
            let sizes = &sizes[..sizes.len().min(MAX_PAGE_SIZES)];
            out.u16(sizes.len() as u16);
            for (width, height) in sizes {
                out.u32(*width);
                out.u32(*height);
            }
        }
        SearchDone { hits } => out.u32(*hits),
        Error { code, message } => {
            out.u8(code.to_u8());
            out.str(message);
        }
    }
    out.0
}

/// The packet in `bytes`.
pub fn decode(bytes: &[u8]) -> Result<Packet, ProtocolError> {
    use Message::*;
    if bytes.len() > MAX_MESSAGE {
        return Err(ProtocolError::TooLong);
    }
    let mut r = Reader(bytes);
    let tag = r.u8()?;
    let version = r.u8()?;
    if version != VERSION {
        return Err(ProtocolError::UnknownVersion(version));
    }
    if r.u16()? != 0 {
        return Err(ProtocolError::BadValue);
    }
    let request = r.u32()?;
    let message = match tag {
        tag::HELLO => Hello { version: r.u8()? },
        tag::OPEN_MEDIA => OpenMedia {
            want_video: r.bool()?,
            max_w: r.u32()?,
            max_h: r.u32()?,
            autoplay: r.bool()?,
            subtitle_file: r.bool()?,
        },
        tag::OPEN_DOCUMENT => OpenDocument,
        tag::PLAY => Play,
        tag::PAUSE => Pause,
        tag::SEEK => Seek { ns: r.u64()?, accurate: r.bool()? },
        tag::SET_VOLUME => SetVolume { permille: r.u32()? },
        tag::SET_MUTE => SetMute { muted: r.bool()? },
        tag::SET_RATE => SetRate { permille: r.u32()? },
        tag::SET_SIZE => SetSize { max_w: r.u32()?, max_h: r.u32()? },
        tag::SET_SUBTITLE => SetSubtitle { index: r.i32()? },
        tag::FRAME_DONE => FrameDone { slot: r.u8()? },
        tag::RENDER_PAGE => RenderPage {
            page: r.u32()?,
            scale_milli: r.u32()?,
            max_w: r.u32()?,
            max_h: r.u32()?,
        },
        tag::PAGE_TEXT => PageText { page: r.u32()? },
        tag::SEARCH => Search { query: r.str()? },
        tag::CANCEL => Cancel { target: r.u32()? },
        tag::CLOSE => Close,
        tag::READY => Ready { version: r.u8()?, features: r.u32()? },
        tag::OPENED => Opened {
            kind: OpenedKind::from_u8(r.u8()?)?,
            duration_ns: r.u64()?,
            video_w: r.u32()?,
            video_h: r.u32()?,
            has_audio: r.bool()?,
            seekable: r.bool()?,
            pages: r.u32()?,
            title: r.str()?,
        },
        tag::FRAME_READY => FrameReady {
            slot: r.u8()?,
            width: r.u32()?,
            height: r.u32()?,
            stride: r.u32()?,
            size: r.u32()?,
            pts_ns: r.u64()?,
        },
        tag::PAGE_READY => PageReady {
            page: r.u32()?,
            width: r.u32()?,
            height: r.u32()?,
            stride: r.u32()?,
            size: r.u32()?,
        },
        tag::STATE => State {
            playing: r.bool()?,
            buffering: r.u8()?.min(100),
            ended: r.bool()?,
        },
        tag::POSITION => Position {
            position_ns: r.u64()?,
            buffered_ns: r.u64()?,
        },
        tag::TRACKS => {
            let mut lists = [Vec::new(), Vec::new()];
            for list in &mut lists {
                for _ in 0..r.count(MAX_TRACKS)? {
                    list.push(r.str()?);
                }
            }
            let [audio, subtitles] = lists;
            Tracks { audio, subtitles }
        }
        tag::CUE => Cue {
            text: r.str()?,
            until_ns: r.u64()?,
        },
        tag::PAGE_TEXT_IS => {
            let page = r.u32()?;
            let more = r.bool()?;
            let mut words = Vec::new();
            for _ in 0..r.count(MAX_WORDS)? {
                words.push(Word {
                    rect: r.rect()?,
                    text: r.str()?,
                });
            }
            PageTextIs { page, words, more }
        }
        tag::SEARCH_HIT => {
            let page = r.u32()?;
            let mut rects = Vec::new();
            for _ in 0..r.count(64)? {
                rects.push(r.rect()?);
            }
            SearchHit { page, rects }
        }
        tag::PAGE_SIZES => {
            let first = r.u32()?;
            let mut sizes = Vec::new();
            for _ in 0..r.count(MAX_PAGE_SIZES)? {
                sizes.push((r.u32()?, r.u32()?));
            }
            PageSizes { first, sizes }
        }
        tag::SEARCH_DONE => SearchDone { hits: r.u32()? },
        tag::ERROR => Error {
            code: ErrorCode::from_u8(r.u8()?)?,
            message: r.str()?,
        },
        other => return Err(ProtocolError::UnknownTag(other)),
    };
    if !r.0.is_empty() {
        return Err(ProtocolError::BadValue);
    }
    Ok(Packet { request, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_message() -> Vec<Message> {
        let rect = Rect { x0: 1, y0: -2, x1: 3000, y1: 4000 };
        vec![
            Message::Hello { version: 1 },
            Message::OpenMedia { want_video: true, max_w: 1280, max_h: 720, autoplay: false, subtitle_file: true },
            Message::OpenDocument,
            Message::Play,
            Message::Pause,
            Message::Seek { ns: 5_000_000_000, accurate: true },
            Message::SetVolume { permille: 800 },
            Message::SetMute { muted: true },
            Message::SetRate { permille: 1500 },
            Message::SetSize { max_w: 640, max_h: 360 },
            Message::SetSubtitle { index: -1 },
            Message::FrameDone { slot: 2 },
            Message::RenderPage { page: 3, scale_milli: 1250, max_w: 900, max_h: 1200 },
            Message::PageText { page: 9 },
            Message::Search { query: "inv\u{f6}ice".into() },
            Message::Cancel { target: 7 },
            Message::Close,
            Message::Ready { version: 1, features: feature::MEDIA | feature::DOCUMENT },
            Message::Opened {
                kind: OpenedKind::Video,
                duration_ns: 6_000_000_000,
                video_w: 640,
                video_h: 360,
                has_audio: true,
                seekable: true,
                pages: 0,
                title: "Film".into(),
            },
            Message::FrameReady { slot: 1, width: 640, height: 360, stride: 2560, size: 921_600, pts_ns: 33_000_000 },
            Message::PageReady { page: 0, width: 893, height: 1263, stride: 3572, size: 4_511_836 },
            Message::State { playing: true, buffering: 40, ended: false },
            Message::Position { position_ns: 1, buffered_ns: 2 },
            Message::Tracks { audio: vec!["English".into(), "Deutsch".into()], subtitles: vec!["English".into()] },
            Message::Cue { text: "Hello\nthere".into(), until_ns: 9 },
            Message::PageTextIs {
                page: 1,
                words: vec![Word { rect, text: "word".into() }, Word { rect, text: String::new() }],
                more: true,
            },
            Message::SearchHit { page: 2, rects: vec![rect, rect] },
            Message::PageSizes { first: 3, sizes: vec![(612_000, 792_000), (595_000, 842_000)] },
            Message::SearchDone { hits: 4 },
            Message::Error { code: ErrorCode::NoDecoder, message: "No decoder for H.265.".into() },
        ]
    }

    #[test]
    fn every_message_round_trips() {
        for (request, message) in every_message().into_iter().enumerate() {
            let packet = Packet { request: request as u32 + 100, message };
            let bytes = encode(&packet);
            assert!(bytes.len() <= MAX_MESSAGE);
            assert_eq!(decode(&bytes), Ok(packet.clone()), "{packet:?}");
        }
    }

    #[test]
    fn the_header_is_what_the_document_says() {
        let bytes = encode(&Packet { request: 0x01020304, message: Message::Play });
        assert_eq!(bytes, [tag::PLAY, 1, 0, 0, 4, 3, 2, 1]);
    }

    #[test]
    fn every_cut_of_every_message_is_an_error_not_a_panic() {
        for message in every_message() {
            let bytes = encode(&Packet { request: 1, message });
            for cut in 0..bytes.len() {
                assert!(decode(&bytes[..cut]).is_err(), "cut at {cut} of {bytes:?}");
            }
            // Trailing bytes are not a message either.
            let mut more = bytes.clone();
            more.push(0);
            assert!(decode(&more).is_err());
        }
    }

    #[test]
    fn unknown_tags_versions_and_values_are_refused() {
        assert_eq!(decode(&[200, 1, 0, 0, 0, 0, 0, 0]), Err(ProtocolError::UnknownTag(200)));
        assert_eq!(decode(&[tag::PLAY, 2, 0, 0, 0, 0, 0, 0]), Err(ProtocolError::UnknownVersion(2)));
        assert_eq!(decode(&[tag::PLAY, 1, 1, 0, 0, 0, 0, 0]), Err(ProtocolError::BadValue));
        assert_eq!(decode(&[tag::SET_MUTE, 1, 0, 0, 0, 0, 0, 0, 2]), Err(ProtocolError::BadValue));
        let mut error = encode(&Packet { request: 0, message: Message::Error { code: ErrorCode::Internal, message: "x".into() } });
        error[8] = 99;
        assert_eq!(decode(&error), Err(ProtocolError::BadValue));
        assert_eq!(decode(&vec![0; MAX_MESSAGE + 1]), Err(ProtocolError::TooLong));
    }

    #[test]
    fn text_that_is_not_utf8_is_refused() {
        let mut bytes = encode(&Packet { request: 0, message: Message::Search { query: "ab".into() } });
        let last = bytes.len() - 1;
        bytes[last] = 0xFF;
        assert_eq!(decode(&bytes), Err(ProtocolError::BadText));
    }

    #[test]
    fn long_strings_and_lists_are_cut_on_a_character_to_fit() {
        let query = "é".repeat(1000);
        let bytes = encode(&Packet { request: 0, message: Message::Search { query } });
        let Packet { message: Message::Search { query }, .. } = decode(&bytes).unwrap() else {
            panic!("a search");
        };
        assert!(query.len() <= 512 && query.chars().all(|c| c == 'é'));
        let tracks = Message::Tracks { audio: (0..40).map(|n| format!("track {n}")).collect(), subtitles: vec![] };
        let bytes = encode(&Packet { request: 0, message: tracks });
        assert!(bytes.len() <= MAX_MESSAGE);
        let Packet { message: Message::Tracks { audio, .. }, .. } = decode(&bytes).unwrap() else {
            panic!("tracks");
        };
        assert_eq!(audio.len(), MAX_TRACKS);
    }

    /// A deterministic spray of bytes: whatever it is, decoding never panics.
    #[test]
    fn random_bytes_never_panic() {
        let mut state = 0x2545F491u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..20_000 {
            let length = (next() % 80) as usize;
            let mut bytes: Vec<u8> = (0..length).map(|_| next() as u8).collect();
            if length > 1 && next() % 2 == 0 {
                bytes[1] = VERSION;
            }
            if length > 3 && next() % 2 == 0 {
                bytes[2] = 0;
                bytes[3] = 0;
            }
            let _ = decode(&bytes);
        }
    }
}
