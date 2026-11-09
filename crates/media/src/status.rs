//! What the player is doing, as one value the window and the shell's media
//! controls read.

use crate::mpv::Node;
use crate::queue::{Item, Repeat};

/// Playing, paused or stopped: MPRIS's `PlaybackStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// A kind of stream in a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}

/// One audio, video or subtitle stream of the file playing.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// mpv's id for it within its kind (`aid`, `sid`).
    pub id: i64,
    pub kind: TrackKind,
    pub title: Option<String>,
    /// A language code (`en`, `deu`).
    pub language: Option<String>,
    pub codec: Option<String>,
    /// From a file next to the film (`.srt`, `.ass`) rather than inside it.
    pub external: bool,
    pub selected: bool,
    /// A stream of cover art (an audio file's picture), not a film.
    pub album_art: bool,
    pub channels: Option<i64>,
}

impl Track {
    /// A name for a menu: the title, else the language, else "Track N".
    pub fn label(&self) -> String {
        let language = self.language.as_deref().map(language_name);
        match (&self.title, language) {
            (Some(title), Some(language))
                if !title.to_lowercase().contains(&language.to_lowercase()) =>
            {
                format!("{title} ({language})")
            }
            (Some(title), _) => title.clone(),
            (None, Some(language)) => language,
            (None, None) => format!("Track {}", self.id),
        }
    }
}

/// A part of the file with a name.
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub title: String,
    /// Seconds from the start.
    pub start: f64,
}

/// A stretch of the file that is read and can be played at once, in seconds.
pub type Range = (f64, f64);

/// Everything the window shows about playback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub state: PlayState,
    /// Waiting for data or for a seek to land.
    pub buffering: bool,
    /// Seconds into the item.
    pub position: f64,
    pub duration: Option<f64>,
    /// 0 to 100.
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    /// The index in the queue of the item playing.
    pub current: Option<usize>,
    /// The item playing, as queued.
    pub item: Option<Item>,
    /// The file has a moving picture (not only cover art).
    pub has_video: bool,
    /// The picture's size in pixels, once known.
    pub video_size: Option<(u32, u32)>,
    pub tracks: Vec<Track>,
    pub chapters: Vec<Chapter>,
    pub chapter: Option<usize>,
    /// What is read ahead, for the timeline's buffered stretches.
    pub buffered: Vec<Range>,
    /// The A and B points of a loop.
    pub loop_a: Option<f64>,
    pub loop_b: Option<f64>,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The queue played to its end.
    pub ended: bool,
    /// Why the last file could not be played.
    pub error: Option<String>,
    /// Tags mpv read from the file (title, artist, album), which fill in an
    /// item queued from a bare path.
    pub tags: Tags,
    /// When a sleep timer stops playback, in seconds from now, or at the end of the item.
    pub sleep: Option<SleepLeft>,
}

/// What is left of a sleep timer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SleepLeft {
    Seconds(u32),
    EndOfItem,
}

/// The tags of the file playing, as mpv reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl Status {
    pub fn audio_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
    }

    pub fn subtitle_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Subtitle)
    }

    /// The title to show: the queued item's (the player fills a bare file's
    /// item in from its tags), else the file's own tag.
    pub fn title(&self) -> Option<String> {
        self.item
            .as_ref()
            .map(|item| item.title.clone())
            .filter(|title| !title.is_empty())
            .or_else(|| self.tags.title.clone())
    }

    pub fn artist(&self) -> Option<String> {
        self.item
            .as_ref()
            .map(|item| item.artist.clone())
            .filter(|artist| !artist.is_empty())
            .or_else(|| self.tags.artist.clone())
    }

    /// How far through, 0 to 1.
    pub fn fraction(&self) -> f64 {
        match self.duration {
            Some(duration) if duration > 0.0 => (self.position / duration).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
}

/// Reads mpv's `track-list`.
pub(crate) fn tracks_of(node: &Node) -> Vec<Track> {
    node.array()
        .iter()
        .filter_map(|entry| {
            let kind = match entry.str("type")? {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                "sub" => TrackKind::Subtitle,
                _ => return None,
            };
            Some(Track {
                id: entry.int("id")?,
                kind,
                title: entry
                    .str("title")
                    .map(str::to_owned)
                    .filter(|title| !title.is_empty()),
                language: entry
                    .str("lang")
                    .map(str::to_owned)
                    .filter(|lang| !lang.is_empty() && lang != "und"),
                codec: entry.str("codec").map(str::to_owned),
                external: entry.flag("external").unwrap_or(false),
                selected: entry.flag("selected").unwrap_or(false),
                album_art: entry.flag("albumart").unwrap_or(false)
                    || entry.flag("image").unwrap_or(false),
                channels: entry.int("demux-channel-count"),
            })
        })
        .collect()
}

/// Reads mpv's `chapter-list`.
pub(crate) fn chapters_of(node: &Node) -> Vec<Chapter> {
    node.array()
        .iter()
        .enumerate()
        .map(|(index, entry)| Chapter {
            title: entry
                .str("title")
                .map(str::to_owned)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| format!("Chapter {}", index + 1)),
            start: entry.double("time").unwrap_or(0.0),
        })
        .collect()
}

/// Reads the `seekable-ranges` of mpv's `demuxer-cache-state`.
pub(crate) fn buffered_of(node: &Node) -> Vec<Range> {
    node.get("seekable-ranges")
        .map(|ranges| {
            ranges
                .array()
                .iter()
                .filter_map(|range| Some((range.double("start")?, range.double("end")?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads title, artist and album from mpv's `metadata` (keys in any case).
pub(crate) fn tags_of(node: &Node) -> Tags {
    let Node::Map(map) = node else {
        return Tags::default();
    };
    let find = |wanted: &str| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .and_then(|(_, value)| match value {
                Node::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
                _ => None,
            })
    };
    Tags {
        title: find("title"),
        artist: find("artist"),
        album: find("album"),
    }
}

/// An English name for the common language codes; the code itself otherwise.
pub fn language_name(code: &str) -> String {
    let name = match code.to_ascii_lowercase().as_str() {
        "en" | "eng" => "English",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "es" | "spa" => "Spanish",
        "it" | "ita" => "Italian",
        "pt" | "por" => "Portuguese",
        "nl" | "dut" | "nld" => "Dutch",
        "sv" | "swe" => "Swedish",
        "no" | "nor" | "nob" => "Norwegian",
        "da" | "dan" => "Danish",
        "fi" | "fin" => "Finnish",
        "pl" | "pol" => "Polish",
        "cs" | "cze" | "ces" => "Czech",
        "ru" | "rus" => "Russian",
        "uk" | "ukr" => "Ukrainian",
        "el" | "gre" | "ell" => "Greek",
        "tr" | "tur" => "Turkish",
        "ar" | "ara" => "Arabic",
        "he" | "heb" => "Hebrew",
        "hi" | "hin" => "Hindi",
        "ja" | "jpn" => "Japanese",
        "ko" | "kor" => "Korean",
        "zh" | "chi" | "zho" => "Chinese",
        _ => return code.to_owned(),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_is_named_by_title_and_language() {
        let track = Track {
            id: 2,
            kind: TrackKind::Subtitle,
            title: Some("Forced".into()),
            language: Some("eng".into()),
            codec: None,
            external: false,
            selected: false,
            album_art: false,
            channels: None,
        };
        assert_eq!(track.label(), "Forced (English)");
        let bare = Track {
            title: None,
            language: None,
            ..track.clone()
        };
        assert_eq!(bare.label(), "Track 2");
        let english = Track {
            title: Some("English SDH".into()),
            ..track
        };
        assert_eq!(english.label(), "English SDH");
    }
}
