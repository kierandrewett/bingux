//! Every string Media shows, in one place, so they move into a Fluent
//! catalogue (`locale/en-GB/`) when bingux-ui's i18n lands. Whole messages
//! only; counts go through the plural functions below.

pub const EMPTY_TITLE: &str = "No music or videos";
pub const EMPTY_BODY: &str =
    "Add files to your Music or Videos folder, or drag them into this window.";
pub const EMPTY_ACTION: &str = "Add Folder…";
pub const EMPTY_SECTION_SONGS: &str = "No songs";
pub const EMPTY_SECTION_SONGS_BODY: &str = "Songs in your Music folder appear here.";
pub const EMPTY_SECTION_VIDEOS: &str = "No videos";
pub const EMPTY_SECTION_VIDEOS_BODY: &str = "Videos in your Videos folder appear here.";
pub const EMPTY_QUEUE: &str = "Nothing is queued";
pub const EMPTY_QUEUE_BODY: &str = "Songs and videos you play appear here.";
pub const NO_RESULTS: &str = "No results";
pub const NO_RESULTS_BODY: &str = "Check the spelling or try another word.";
pub const SCANNING: &str = "Updating library…";
pub const NOT_PLAYING: &str = "Not playing";
pub const CANNOT_PLAY: &str =
    "Could not play this file. It may be damaged or in a format Media cannot read.";

pub const PLAY: &str = "Play";
pub const SHUFFLE: &str = "Shuffle";
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

pub const MENU_OPEN: &str = "Open File…";
pub const MENU_ADD_FOLDER: &str = "Add Folder…";
pub const MENU_SCAN: &str = "Update Library";
pub const MENU_SETTINGS: &str = "Settings";
pub const MENU_MINI: &str = "Mini Player";

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
pub const END_OF_ITEM: &str = "End of song or video";
pub const ADD_SUBTITLE_FILE: &str = "Add Subtitle File…";
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
pub const SIGN_IN: &str = "Sign In…";
pub const FINISH_SIGN_IN: &str = "Finish Signing In";
pub const SIGN_OUT: &str = "Sign Out";
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

/// "42 minutes", "1 hour 5 minutes".
pub fn length(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    let minutes_text = match minutes {
        1 => "1 minute".to_owned(),
        count => format!("{count} minutes"),
    };
    match hours {
        0 => minutes_text,
        1 => format!("1 hour {minutes_text}"),
        count => format!("{count} hours {minutes_text}"),
    }
}

/// "15 minutes", for the sleep timer menu.
pub fn minutes(count: u32) -> String {
    match count {
        1 => "1 minute".into(),
        count => format!("{count} minutes"),
    }
}

/// "Stops in 12:30", the sleep timer's state.
pub fn stops_in(clock: &str) -> String {
    format!("Stops in {clock}")
}

/// Playback speed in a menu: "1×", "1.5×".
pub fn speed(rate: f64) -> String {
    let text = format!("{rate:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}
