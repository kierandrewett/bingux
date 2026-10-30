//! What a player knows about a film or a song: the state machine behind the
//! controls, pure so it is tested against a fake host.
//!
//! A viewer feeds it every packet the host sends ([`Player::apply`]) and asks
//! it what to send for a person's action ([`Player::toggle`],
//! [`Player::seek_by`], ...). Each action changes the state at once (the bar
//! does not wait for the host's echo) and returns the packets to send.
//!
//! Seeking: while a seek is pending the bar shows the target, not the stale
//! position the host reports before it has moved. The host ends a seek with a
//! `Position` that carries the seek's request id.

use super::protocol::{ErrorCode, Message, OpenedKind, Packet};

/// The playback speeds the speed menu offers, in thousandths.
pub const RATES: [u32; 7] = [500, 750, 1000, 1250, 1500, 1750, 2000];
/// The step of the volume keys, in thousandths.
pub const VOLUME_STEP: u32 = 50;
/// The most the volume reaches, in thousandths.
pub const VOLUME_MAX: u32 = 1000;

/// Why the player cannot play, in words for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

/// Where the player is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `OpenMedia` is sent; the host has not said what it found.
    Opening,
    Ready,
    /// The host reported an error, died, or stopped answering.
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingSeek {
    request: u32,
    target_ns: u64,
}

/// The state of one open media file.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub phase: Phase,
    pub kind: OpenedKind,
    pub title: String,
    pub duration_ns: u64,
    pub video_size: (u32, u32),
    pub has_audio: bool,
    pub seekable: bool,
    /// What the host reports: it is playing.
    pub playing: bool,
    /// What the person asked for; shown until the host agrees.
    pub wants_play: bool,
    pub ended: bool,
    /// 100 when the data is there; less while the host buffers.
    pub buffering: u8,
    pub position_ns: u64,
    pub buffered_ns: u64,
    pub volume_permille: u32,
    pub muted: bool,
    pub rate_permille: u32,
    pub audio_tracks: Vec<String>,
    pub subtitle_tracks: Vec<String>,
    /// The subtitle track shown, or -1.
    pub subtitle: i32,
    /// The line to draw under the picture.
    pub cue: Option<(String, u64)>,
    seek: Option<PendingSeek>,
    next_request: u32,
}

impl Default for Player {
    fn default() -> Self {
        Player::new()
    }
}

impl Player {
    pub fn new() -> Player {
        Player {
            phase: Phase::Opening,
            kind: OpenedKind::Video,
            title: String::new(),
            duration_ns: 0,
            video_size: (0, 0),
            has_audio: false,
            seekable: false,
            playing: false,
            wants_play: false,
            ended: false,
            buffering: 100,
            position_ns: 0,
            buffered_ns: 0,
            volume_permille: VOLUME_MAX,
            muted: false,
            rate_permille: 1000,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            subtitle: -1,
            cue: None,
            seek: None,
            next_request: 1,
        }
    }

    /// A request id no earlier packet of this player used.
    pub fn request(&mut self) -> u32 {
        let id = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        id
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    pub fn failure(&self) -> Option<&Failure> {
        match &self.phase {
            Phase::Failed(failure) => Some(failure),
            _ => None,
        }
    }

    /// The host died or stopped answering.
    pub fn fail(&mut self, code: ErrorCode, message: &str) {
        self.phase = Phase::Failed(Failure { code, message: message.to_owned() });
        self.playing = false;
        self.wants_play = false;
        self.seek = None;
    }

    /// Take in what the host said.
    pub fn apply(&mut self, packet: &Packet) {
        match &packet.message {
            Message::Opened { kind, duration_ns, video_w, video_h, has_audio, seekable, title, .. } => {
                if matches!(self.phase, Phase::Opening) {
                    self.phase = Phase::Ready;
                }
                self.kind = *kind;
                self.duration_ns = *duration_ns;
                self.video_size = (*video_w, *video_h);
                self.has_audio = *has_audio;
                self.seekable = *seekable;
                self.title = title.clone();
            }
            Message::State { playing, buffering, ended } => {
                self.playing = *playing;
                self.wants_play = *playing;
                self.buffering = *buffering;
                self.ended = *ended;
                if *ended {
                    self.position_ns = self.duration_ns.max(self.position_ns);
                    self.seek = None;
                }
            }
            Message::Position { position_ns, buffered_ns } => {
                match self.seek {
                    // A report from before the host moved says nothing about
                    // where it is now.
                    Some(seek) if packet.request != seek.request => {
                        self.buffered_ns = *buffered_ns;
                        return;
                    }
                    Some(_) => self.seek = None,
                    None => {}
                }
                self.position_ns = *position_ns;
                self.buffered_ns = *buffered_ns;
                if self.duration_ns > 0 && *position_ns < self.duration_ns {
                    self.ended = false;
                }
            }
            Message::Tracks { audio, subtitles } => {
                self.audio_tracks = audio.clone();
                self.subtitle_tracks = subtitles.clone();
                if self.subtitle >= subtitles.len() as i32 {
                    self.subtitle = -1;
                }
            }
            Message::Cue { text, until_ns } => {
                self.cue = (!text.is_empty()).then(|| (text.clone(), *until_ns));
            }
            Message::Error { code, message } => self.fail(*code, message),
            _ => {}
        }
    }

    /// The position the bar shows: the target of a pending seek, else where
    /// the host is.
    pub fn shown_position(&self) -> u64 {
        self.seek.map_or(self.position_ns, |seek| seek.target_ns)
    }

    pub fn is_seeking(&self) -> bool {
        self.seek.is_some()
    }

    /// Where the bar's thumb is, 0 to 1.
    pub fn progress(&self) -> f32 {
        if self.duration_ns == 0 {
            return 0.0;
        }
        (self.shown_position() as f64 / self.duration_ns as f64).clamp(0.0, 1.0) as f32
    }

    /// How far the data is buffered, 0 to 1.
    pub fn buffered(&self) -> f32 {
        if self.duration_ns == 0 {
            return 0.0;
        }
        (self.buffered_ns as f64 / self.duration_ns as f64).clamp(0.0, 1.0) as f32
    }

    /// Whether the player is waiting for data.
    pub fn is_buffering(&self) -> bool {
        self.buffering < 100
    }

    /// The subtitle line to show at the shown position, if any.
    pub fn cue_now(&self) -> Option<&str> {
        self.cue
            .as_ref()
            .filter(|(_, until)| *until == 0 || self.position_ns <= *until)
            .map(|(text, _)| text.as_str())
    }

    fn packet(&mut self, message: Message) -> Packet {
        let request = self.request();
        Packet { request, message }
    }

    /// Play, or pause when playing. At the end it starts again from the start.
    pub fn toggle(&mut self) -> Vec<Packet> {
        if self.wants_play && !self.ended { self.pause() } else { self.play() }
    }

    pub fn play(&mut self) -> Vec<Packet> {
        let mut out = Vec::new();
        if self.ended {
            out.extend(self.seek_to(0, false));
        }
        self.wants_play = true;
        out.push(self.packet(Message::Play));
        out
    }

    pub fn pause(&mut self) -> Vec<Packet> {
        self.wants_play = false;
        vec![self.packet(Message::Pause)]
    }

    /// Move to `ns`, clamped to the length. `accurate` lands on that frame.
    pub fn seek_to(&mut self, ns: u64, accurate: bool) -> Vec<Packet> {
        if !self.seekable && self.duration_ns == 0 {
            return Vec::new();
        }
        let target = if self.duration_ns > 0 { ns.min(self.duration_ns) } else { ns };
        let request = self.request();
        self.seek = Some(PendingSeek { request, target_ns: target });
        self.ended = false;
        vec![Packet { request, message: Message::Seek { ns: target, accurate } }]
    }

    /// Move by `delta_ms` from the shown position.
    pub fn seek_by(&mut self, delta_ms: i64, accurate: bool) -> Vec<Packet> {
        let now = self.shown_position() as i64;
        let target = (now + delta_ms * 1_000_000).max(0) as u64;
        self.seek_to(target, accurate)
    }

    /// Move to `fraction` (0 to 1) of the length.
    pub fn seek_fraction(&mut self, fraction: f32, accurate: bool) -> Vec<Packet> {
        let target = (self.duration_ns as f64 * f64::from(fraction.clamp(0.0, 1.0))) as u64;
        self.seek_to(target, accurate)
    }

    pub fn set_volume(&mut self, permille: u32) -> Vec<Packet> {
        self.volume_permille = permille.min(VOLUME_MAX);
        let mut out = vec![self.packet(Message::SetVolume { permille: self.volume_permille })];
        // Raising the volume of a muted player unmutes it, as every player does.
        if self.muted && self.volume_permille > 0 {
            out.extend(self.set_muted(false));
        }
        out
    }

    pub fn step_volume(&mut self, steps: i32) -> Vec<Packet> {
        let next = (self.volume_permille as i32 + steps * VOLUME_STEP as i32).clamp(0, VOLUME_MAX as i32);
        self.set_volume(next as u32)
    }

    pub fn set_muted(&mut self, muted: bool) -> Vec<Packet> {
        self.muted = muted;
        vec![self.packet(Message::SetMute { muted })]
    }

    pub fn toggle_mute(&mut self) -> Vec<Packet> {
        self.set_muted(!self.muted)
    }

    pub fn set_rate(&mut self, permille: u32) -> Vec<Packet> {
        self.rate_permille = permille.clamp(250, 4000);
        vec![self.packet(Message::SetRate { permille: self.rate_permille })]
    }

    /// The next slower (`-1`) or faster (`1`) speed of [`RATES`].
    pub fn step_rate(&mut self, direction: i32) -> Vec<Packet> {
        let at = RATES.iter().position(|rate| *rate >= self.rate_permille).unwrap_or(RATES.len() - 1);
        let next = (at as i32 + direction).clamp(0, RATES.len() as i32 - 1) as usize;
        self.set_rate(RATES[next])
    }

    pub fn set_subtitle(&mut self, index: i32) -> Vec<Packet> {
        self.subtitle = index.clamp(-1, self.subtitle_tracks.len() as i32 - 1);
        self.cue = None;
        vec![self.packet(Message::SetSubtitle { index: self.subtitle })]
    }
}

/// How a time reads on the bar: `1:05`, `12:03`, `1:02:03`.
pub fn clock(ns: u64) -> String {
    let seconds = ns / 1_000_000_000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

/// The label of a speed: `1×`, `1.25×`.
pub fn rate_label(permille: u32) -> String {
    let text = format!("{:.2}", f64::from(permille) / 1000.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}\u{d7}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(duration_s: u64) -> Packet {
        Packet {
            request: 1,
            message: Message::Opened {
                kind: OpenedKind::Video,
                duration_ns: duration_s * 1_000_000_000,
                video_w: 640,
                video_h: 360,
                has_audio: true,
                seekable: true,
                pages: 0,
                title: "Clip".into(),
            },
        }
    }

    fn ready(duration_s: u64) -> Player {
        let mut player = Player::new();
        player.apply(&opened(duration_s));
        player
    }

    fn at(request: u32, position_s: u64, buffered_s: u64) -> Packet {
        Packet {
            request,
            message: Message::Position { position_ns: position_s * 1_000_000_000, buffered_ns: buffered_s * 1_000_000_000 },
        }
    }

    #[test]
    fn opening_becomes_ready_with_what_the_host_found() {
        let mut player = Player::new();
        assert_eq!(player.phase, Phase::Opening);
        player.apply(&opened(120));
        assert!(player.is_ready());
        assert_eq!((player.duration_ns, player.video_size, player.has_audio), (120_000_000_000, (640, 360), true));
        assert_eq!(player.title, "Clip");
    }

    #[test]
    fn play_and_pause_flip_the_intent_at_once_and_wait_for_the_host_to_agree() {
        let mut player = ready(10);
        let sent = player.toggle();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].message, Message::Play);
        assert!(player.wants_play && !player.playing);
        player.apply(&Packet { request: 0, message: Message::State { playing: true, buffering: 100, ended: false } });
        assert!(player.playing);
        let sent = player.toggle();
        assert_eq!(sent[0].message, Message::Pause);
        assert!(!player.wants_play);
    }

    #[test]
    fn the_end_of_the_stream_stops_and_play_starts_over() {
        let mut player = ready(10);
        player.apply(&at(0, 9, 10));
        player.apply(&Packet { request: 0, message: Message::State { playing: false, buffering: 100, ended: true } });
        assert!(player.ended && !player.playing);
        assert_eq!(player.position_ns, 10_000_000_000);
        let sent = player.toggle();
        let messages: Vec<_> = sent.iter().map(|packet| packet.message.clone()).collect();
        assert_eq!(messages, vec![Message::Seek { ns: 0, accurate: false }, Message::Play]);
        assert!(!player.ended);
    }

    #[test]
    fn a_seek_shows_its_target_until_the_host_has_moved() {
        let mut player = ready(100);
        player.apply(&at(0, 10, 20));
        let sent = player.seek_by(5_000, true);
        assert_eq!(sent[0].message, Message::Seek { ns: 15_000_000_000, accurate: true });
        let id = sent[0].request;
        assert!(player.is_seeking());
        assert_eq!(player.shown_position(), 15_000_000_000);
        // A report that was already on its way does not move the bar back.
        player.apply(&at(0, 10, 25));
        assert_eq!(player.shown_position(), 15_000_000_000);
        assert_eq!(player.buffered_ns, 25_000_000_000, "the buffered range still updates");
        player.apply(&at(id, 15, 25));
        assert!(!player.is_seeking());
        assert_eq!(player.shown_position(), 15_000_000_000);
    }

    #[test]
    fn seeking_clamps_to_the_length_and_to_zero() {
        let mut player = ready(60);
        player.apply(&at(0, 58, 60));
        let sent = player.seek_by(30_000, false);
        assert_eq!(sent[0].message, Message::Seek { ns: 60_000_000_000, accurate: false });
        let sent = player.seek_by(-120_000, false);
        assert_eq!(sent[0].message, Message::Seek { ns: 0, accurate: false });
        // A second seek while one is pending is relative to the target.
        let mut player = ready(100);
        player.seek_by(5_000, false);
        let sent = player.seek_by(5_000, false);
        assert_eq!(sent[0].message, Message::Seek { ns: 10_000_000_000, accurate: false });
    }

    #[test]
    fn dragging_to_a_fraction_seeks_there() {
        let mut player = ready(200);
        let sent = player.seek_fraction(0.25, false);
        assert_eq!(sent[0].message, Message::Seek { ns: 50_000_000_000, accurate: false });
        assert!((player.progress() - 0.25).abs() < 1e-6);
        assert_eq!(player.seek_fraction(7.0, false)[0].message, Message::Seek { ns: 200_000_000_000, accurate: false });
    }

    #[test]
    fn volume_steps_clamp_and_raising_it_unmutes() {
        let mut player = ready(10);
        player.set_volume(500);
        player.toggle_mute();
        assert!(player.muted);
        let sent = player.step_volume(1);
        assert_eq!(player.volume_permille, 550);
        assert!(!player.muted);
        assert_eq!(sent.len(), 2);
        for _ in 0..40 {
            player.step_volume(1);
        }
        assert_eq!(player.volume_permille, VOLUME_MAX);
        for _ in 0..40 {
            player.step_volume(-1);
        }
        assert_eq!(player.volume_permille, 0);
    }

    #[test]
    fn speed_steps_through_the_menu_values_and_stops_at_the_ends() {
        let mut player = ready(10);
        assert_eq!(player.rate_permille, 1000);
        player.step_rate(1);
        assert_eq!(player.rate_permille, 1250);
        player.step_rate(-1);
        player.step_rate(-1);
        assert_eq!(player.rate_permille, 750);
        for _ in 0..10 {
            player.step_rate(-1);
        }
        assert_eq!(player.rate_permille, 500);
        for _ in 0..20 {
            player.step_rate(1);
        }
        assert_eq!(player.rate_permille, 2000);
    }

    #[test]
    fn an_error_from_the_host_fails_the_player_and_stops_everything() {
        let mut player = ready(10);
        player.toggle();
        player.apply(&Packet {
            request: 0,
            message: Message::Error { code: ErrorCode::NoDecoder, message: "No decoder for H.265.".into() },
        });
        let failure = player.failure().expect("failed");
        assert_eq!(failure.code, ErrorCode::NoDecoder);
        assert!(!player.wants_play && !player.playing);
    }

    #[test]
    fn subtitles_are_chosen_from_the_tracks_the_host_lists() {
        let mut player = ready(10);
        player.apply(&Packet { request: 0, message: Message::Tracks { audio: vec![], subtitles: vec!["English".into(), "Deutsch".into()] } });
        let sent = player.set_subtitle(1);
        assert_eq!(sent[0].message, Message::SetSubtitle { index: 1 });
        assert_eq!(player.set_subtitle(9)[0].message, Message::SetSubtitle { index: 1 });
        assert_eq!(player.set_subtitle(-5)[0].message, Message::SetSubtitle { index: -1 });
        player.apply(&Packet { request: 0, message: Message::Cue { text: "Hello".into(), until_ns: 5_000_000_000 } });
        assert_eq!(player.cue_now(), Some("Hello"));
        player.apply(&Packet { request: 0, message: Message::Cue { text: String::new(), until_ns: 0 } });
        assert_eq!(player.cue_now(), None);
    }

    #[test]
    fn a_stream_that_cannot_seek_ignores_seeks() {
        let mut player = Player::new();
        assert!(player.seek_to(5, false).is_empty());
    }

    #[test]
    fn times_read_as_a_clock() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(65_000_000_000), "1:05");
        assert_eq!(clock(3_723_000_000_000), "1:02:03");
        assert_eq!(rate_label(1000), "1\u{d7}");
        assert_eq!(rate_label(1250), "1.25\u{d7}");
        assert_eq!(rate_label(1500), "1.5\u{d7}");
    }
}
