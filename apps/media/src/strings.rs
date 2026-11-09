//! Every string Media shows, in one place, so they move into a Fluent
//! catalogue (`locale/en-GB/`) when bingux-ui's i18n lands. Whole messages
//! only; counts go through the plural functions below.

use bingux_ui::format::{Timespan, Width};

pub const EMPTY_TITLE: &str = "No music or videos";
pub const EMPTY_BODY: &str =
    "Add files to your Music or Videos folder, or drag them into this window.";
pub const EMPTY_ACTION: &str = "Add folder…";
pub const EMPTY_SECTION_SONGS: &str = "No songs";
pub const EMPTY_SECTION_SONGS_BODY: &str = "Songs in your Music folder appear here.";
pub const EMPTY_SECTION_ALBUMS: &str = "No albums";
pub const EMPTY_SECTION_ALBUMS_BODY: &str = "Albums in your Music folder appear here.";
pub const EMPTY_SECTION_ARTISTS: &str = "No artists";
pub const EMPTY_SECTION_ARTISTS_BODY: &str = "Artists in your Music folder appear here.";
pub const EMPTY_SECTION_VIDEOS: &str = "No videos";
pub const EMPTY_SECTION_VIDEOS_BODY: &str = "Videos in your Videos folder appear here.";
pub const EMPTY_QUEUE: &str = "Nothing is queued";
pub const EMPTY_QUEUE_BODY: &str = "Songs and videos you play appear here.";
pub const NO_RESULTS: &str = "No results";
pub const NO_RESULTS_BODY: &str = "Check the spelling or try another word.";
pub const SCANNING: &str = "Updating library…";
pub const NOT_PLAYING: &str = "Not playing";
pub const CANNOT_PLAY: &str = "Could not play this file. Try another copy of it.";
pub const FRAME_SAVED: &str = "Frame saved to Pictures";
pub const FRAME_NOT_SAVED: &str = "Could not save the frame. Check that Pictures has room.";
pub const DROP_HERE: &str = "Drop to play";

pub const PLAY: &str = "Play";
pub const SHUFFLE: &str = "Play shuffled";
pub const PLAY_NEXT: &str = "Play next";
pub const PLAY_LATER: &str = "Play later";
pub const REMOVE_FROM_QUEUE: &str = "Remove from queue";
pub const CLEAR_UP_NEXT: &str = "Clear up next";
pub const SHOW_ALBUM: &str = "Go to album";
pub const SHOW_ARTIST: &str = "Go to artist";
pub const CONTINUE_WATCHING: &str = "Continue watching";
pub const ALL_VIDEOS: &str = "All videos";
pub const NOW_PLAYING: &str = "Now playing";
pub const UP_NEXT: &str = "Up next";
pub const SONGS: &str = "Songs";
pub const VIDEOS: &str = "Videos";
pub const ALBUMS: &str = "Albums";
pub const COLUMN_TITLE: &str = "Title";
pub const COLUMN_ARTIST: &str = "Artist";
pub const COLUMN_ALBUM: &str = "Album";
pub const COLUMN_TIME: &str = "Time";

pub const MENU_OPEN: &str = "Open file…";
pub const MENU_ADD_FOLDER: &str = "Add folder…";
pub const MENU_SCAN: &str = "Update library";
pub const MENU_SETTINGS: &str = "Settings";
pub const MENU_MINI: &str = "Mini player";
pub const MENU_PLAYER: &str = "Show player";
pub const MENU_FULL_SCREEN: &str = "Full screen";
pub const MENU_UP_NEXT: &str = "Go to up next";
pub const CONTROLS_GROUP: &str = "Controls";
pub const COMMAND_PLAY: &str = "Play";
pub const COMMAND_PAUSE: &str = "Pause";
pub const COMMAND_NEXT: &str = "Next";
pub const COMMAND_PREVIOUS: &str = "Previous";
pub const COMMAND_LOUDER: &str = "Increase volume";
pub const COMMAND_QUIETER: &str = "Decrease volume";
pub const COMMAND_MUTE: &str = "Mute";
pub const COMMAND_SHUFFLE: &str = "Shuffle";
pub const COMMAND_REPEAT: &str = "Repeat";
pub const COMMAND_REPEAT_ONE: &str = "Repeat one";
pub const HINT_PLAY: &str = "Play or pause, in the player";
pub const HINT_SEEK: &str = "Back or forward 5 seconds";
pub const HINT_SEEK_LONG: &str = "Back or forward 30 seconds";
pub const HINT_SKIP: &str = "Back or forward 10 seconds";
pub const HINT_VOLUME: &str = "Volume, in the player";
pub const HINT_MUTE: &str = "Mute, in the player";
pub const HINT_FULL_SCREEN: &str = "Full screen, in the player";
pub const HINT_SPEED: &str = "Slower or faster";
pub const HINT_SUBTITLES: &str = "Next subtitles";
pub const HINT_FRAME: &str = "A frame back or forward, paused";
pub const HINT_LOOP: &str = "Loop a part: start, end, off";
pub const HINT_SNAPSHOT: &str = "Save the frame to Pictures";
pub const HINT_LEAVE: &str = "Leave full screen, then the player";

pub const CLOSE_PLAYER: &str = "Close player (Esc)";
pub const FULL_SCREEN: &str = "Full screen (F)";
pub const LEAVE_FULL_SCREEN: &str = "Leave full screen (F)";
pub const MINI_PLAYER: &str = "Mini player";
pub const LEAVE_MINI_PLAYER: &str = "Leave mini player";
pub const SUBTITLES: &str = "Subtitles (C)";
pub const AUDIO_TRACK: &str = "Audio";
pub const SPEED: &str = "Speed ([ and ])";
pub const SLEEP_TIMER: &str = "Sleep timer";
pub const QUEUE: &str = "Up next";
pub const OFF: &str = "Off";
pub const END_OF_ITEM: &str = "When this ends";
pub const STOPS_AT_END: &str = "Stops when this ends";
pub const ADD_SUBTITLE_FILE: &str = "Add subtitle file…";
pub const CHAPTERS: &str = "Chapters";

pub const SETTINGS_TITLE: &str = "Settings";
pub const LIBRARY_GROUP: &str = "Library";
pub const PLAYBACK_GROUP: &str = "Playback";
pub const LASTFM_GROUP: &str = "Last.fm";
pub const REMOVE_FOLDER: &str = "Remove from library";
pub const LEVELLING: &str = "Even out loudness";
pub const LEVELLING_OFF: &str = "Off";
pub const LEVELLING_TRACK: &str = "Song by song";
pub const LEVELLING_ALBUM: &str = "Album by album";
pub const LEVELLING_AUTO: &str = "Automatic";
pub const RESUME: &str = "Resume videos";
pub const RESUME_BODY: &str = "Videos start where you left them.";
pub const SUBTITLES_SETTING: &str = "Show subtitles";
pub const SCROBBLE: &str = "Scrobble to Last.fm";
pub const API_KEY: &str = "API key";
pub const API_SECRET: &str = "Shared secret";
pub const API_HELP: &str = "Create an API account on Last.fm, then enter its key and secret.";
pub const SIGN_IN: &str = "Sign in…";
pub const FINISH_SIGN_IN: &str = "Finish signing in";
pub const SIGN_OUT: &str = "Sign out";
pub const SIGNED_IN_AS: &str = "Signed in as";
pub const LASTFM_UNREACHABLE: &str =
    "Could not reach Last.fm. Check your connection, then try again.";
pub const LASTFM_BAD_KEYS: &str =
    "Last.fm did not accept the key or secret. Check them, then try again.";
pub const LASTFM_NOT_ALLOWED: &str = "Allow Media on the Last.fm page, then try again.";
pub const LASTFM_FAILED: &str = "Could not sign in to Last.fm. Try again later.";

/// "1 song", "12 songs".
pub fn songs(count: usize) -> String {
    match count {
        1 => "1 song".into(),
        count => format!("{count} songs"),
    }
}

/// "1 album", "3 albums".
pub fn albums(count: usize) -> String {
    match count {
        1 => "1 album".into(),
        count => format!("{count} albums"),
    }
}

/// "1 artist", "4 artists".
pub fn artists(count: usize) -> String {
    match count {
        1 => "1 artist".into(),
        count => format!("{count} artists"),
    }
}

/// "1 video", "9 videos".
pub fn videos(count: usize) -> String {
    match count {
        1 => "1 video".into(),
        count => format!("{count} videos"),
    }
}

/// "42 minutes", "1 hour, 5 minutes": a list's length, to the minute.
pub fn length(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round().max(1.0);
    Timespan::from_secs_f64(minutes * 60.0)
        .width(Width::Long)
        .to_string()
}

/// "3:45", "1:02:03": a song's or a film's length.
pub fn clock(seconds: f64) -> String {
    Timespan::from_secs_f64(seconds).digital().to_string()
}

/// "1 hr, 3 min left": what is left of a film watched part way.
pub fn left(seconds: f64) -> String {
    let rounded = (seconds / 60.0).ceil().max(1.0) * 60.0;
    format!("{} left", Timespan::from_secs_f64(rounded))
}

/// "15 minutes", for the sleep timer menu.
pub fn minutes(count: u32) -> String {
    Timespan::new(std::time::Duration::from_secs(u64::from(count) * 60))
        .width(Width::Long)
        .to_string()
}

/// "Stops in 12:30", the sleep timer's state.
pub fn stops_in(seconds: f64) -> String {
    format!("Stops in {}", clock(seconds))
}

/// What a screen reader hears when the next song starts: "Now playing First
/// Light by Aurora Lane".
pub fn now_playing_spoken(title: &str, artist: &str) -> String {
    if artist.is_empty() {
        format!("Now playing {title}")
    } else {
        format!("Now playing {title} by {artist}")
    }
}

/// Playback speed in a menu: "1×", "1.5×".
pub fn speed(rate: f64) -> String {
    let text = format!("{rate:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}
