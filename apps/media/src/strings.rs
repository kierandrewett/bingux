//! Every string Media shows, each one a `tr!` of its catalogue message in
//! `locale/en-US/media.ftl`. Counts and durations go through the variables of
//! those messages; a value that is only a formatted number or time keeps its
//! `bingux-ui` formatter.

use bingux_ui::format::{Timespan, Width};
use bingux_ui::prelude::tr;

pub fn empty_title() -> String {
    tr!("media-empty-title")
}
pub fn empty_body() -> String {
    tr!("media-empty-body")
}
pub fn empty_action() -> String {
    tr!("media-empty-action")
}
pub fn empty_section_songs() -> String {
    tr!("media-empty-section-songs")
}
pub fn empty_section_songs_body() -> String {
    tr!("media-empty-section-songs-body")
}
pub fn empty_section_albums() -> String {
    tr!("media-empty-section-albums")
}
pub fn empty_section_albums_body() -> String {
    tr!("media-empty-section-albums-body")
}
pub fn empty_section_artists() -> String {
    tr!("media-empty-section-artists")
}
pub fn empty_section_artists_body() -> String {
    tr!("media-empty-section-artists-body")
}
pub fn empty_section_videos() -> String {
    tr!("media-empty-section-videos")
}
pub fn empty_section_videos_body() -> String {
    tr!("media-empty-section-videos-body")
}
pub fn empty_queue() -> String {
    tr!("media-empty-queue")
}
pub fn empty_queue_body() -> String {
    tr!("media-empty-queue-body")
}
pub fn no_results() -> String {
    tr!("media-no-results")
}
pub fn no_results_body() -> String {
    tr!("media-no-results-body")
}
pub fn scanning() -> String {
    tr!("media-scanning")
}
pub fn not_playing() -> String {
    tr!("media-not-playing")
}
pub fn cannot_play() -> String {
    tr!("media-cannot-play")
}
pub fn frame_saved() -> String {
    tr!("media-frame-saved")
}
pub fn frame_not_saved() -> String {
    tr!("media-frame-not-saved")
}
pub fn drop_here() -> String {
    tr!("media-drop-here")
}

pub fn play() -> String {
    tr!("media-play")
}
pub fn shuffle() -> String {
    tr!("media-shuffle")
}
pub fn play_next() -> String {
    tr!("media-play-next")
}
pub fn play_later() -> String {
    tr!("media-play-later")
}
pub fn remove_from_queue() -> String {
    tr!("media-remove-from-queue")
}
pub fn clear_up_next() -> String {
    tr!("media-clear-up-next")
}
pub fn show_album() -> String {
    tr!("media-show-album")
}
pub fn show_artist() -> String {
    tr!("media-show-artist")
}
pub fn continue_watching() -> String {
    tr!("media-continue-watching")
}
pub fn all_videos() -> String {
    tr!("media-all-videos")
}
pub fn now_playing() -> String {
    tr!("media-now-playing")
}
pub fn up_next() -> String {
    tr!("media-up-next")
}
pub fn songs_heading() -> String {
    tr!("media-songs-heading")
}
pub fn videos_heading() -> String {
    tr!("media-videos-heading")
}
pub fn albums_heading() -> String {
    tr!("media-albums-heading")
}

/// "42 minutes", "1 hour, 5 minutes": a list's length, to the minute.
pub fn length(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round().max(1.0);
    Timespan::from_secs_f64(minutes * 60.0)
        .width(Width::Long)
        .to_string()
}
pub fn column_title() -> String {
    tr!("media-column-title")
}
pub fn column_artist() -> String {
    tr!("media-column-artist")
}
pub fn column_album() -> String {
    tr!("media-column-album")
}
pub fn column_time() -> String {
    tr!("media-column-time")
}

pub fn menu_open() -> String {
    tr!("media-menu-open")
}
pub fn menu_add_folder() -> String {
    tr!("media-menu-add-folder")
}
pub fn menu_scan() -> String {
    tr!("media-menu-scan")
}
pub fn menu_settings() -> String {
    tr!("media-menu-settings")
}
pub fn menu_mini() -> String {
    tr!("media-menu-mini")
}
pub fn menu_player() -> String {
    tr!("media-menu-player")
}
pub fn menu_full_screen() -> String {
    tr!("media-menu-full-screen")
}
pub fn menu_up_next() -> String {
    tr!("media-menu-up-next")
}

pub fn command_play() -> String {
    tr!("media-command-play")
}
pub fn command_pause() -> String {
    tr!("media-command-pause")
}
pub fn command_next() -> String {
    tr!("media-command-next")
}
pub fn command_previous() -> String {
    tr!("media-command-previous")
}
pub fn command_louder() -> String {
    tr!("media-command-louder")
}
pub fn command_quieter() -> String {
    tr!("media-command-quieter")
}
pub fn command_mute() -> String {
    tr!("media-command-mute")
}
pub fn command_shuffle() -> String {
    tr!("media-command-shuffle")
}
pub fn command_repeat() -> String {
    tr!("media-command-repeat")
}
pub fn command_repeat_one() -> String {
    tr!("media-command-repeat-one")
}
pub fn hint_play() -> String {
    tr!("media-hint-play")
}
pub fn hint_seek() -> String {
    tr!("media-hint-seek")
}
pub fn hint_seek_long() -> String {
    tr!("media-hint-seek-long")
}
pub fn hint_skip() -> String {
    tr!("media-hint-skip")
}
pub fn hint_volume() -> String {
    tr!("media-hint-volume")
}
pub fn hint_mute() -> String {
    tr!("media-hint-mute")
}
pub fn hint_full_screen() -> String {
    tr!("media-hint-full-screen")
}
pub fn hint_speed() -> String {
    tr!("media-hint-speed")
}
pub fn hint_subtitles() -> String {
    tr!("media-hint-subtitles")
}
pub fn hint_frame() -> String {
    tr!("media-hint-frame")
}
pub fn hint_loop() -> String {
    tr!("media-hint-loop")
}
pub fn hint_snapshot() -> String {
    tr!("media-hint-snapshot")
}
pub fn hint_leave() -> String {
    tr!("media-hint-leave")
}

pub fn close_player() -> String {
    tr!("media-close-player")
}
pub fn full_screen() -> String {
    tr!("media-full-screen")
}
pub fn leave_full_screen() -> String {
    tr!("media-leave-full-screen")
}
pub fn mini_player() -> String {
    tr!("media-mini-player")
}
pub fn leave_mini_player() -> String {
    tr!("media-leave-mini-player")
}
pub fn subtitles() -> String {
    tr!("media-subtitles")
}
pub fn audio_track() -> String {
    tr!("media-audio-track")
}
pub fn speed_menu() -> String {
    tr!("media-speed-menu")
}
pub fn sleep_timer() -> String {
    tr!("media-sleep-timer")
}
pub fn queue() -> String {
    tr!("media-queue")
}
pub fn off() -> String {
    tr!("media-off")
}
pub fn end_of_item() -> String {
    tr!("media-end-of-item")
}
pub fn stops_at_end() -> String {
    tr!("media-stops-at-end")
}
pub fn add_subtitle_file() -> String {
    tr!("media-add-subtitle-file")
}
pub fn chapters() -> String {
    tr!("media-chapters")
}

pub fn settings_title() -> String {
    tr!("media-settings-title")
}
pub fn library_group() -> String {
    tr!("media-library-group")
}
pub fn playback_group() -> String {
    tr!("media-playback-group")
}
pub fn lastfm_group() -> String {
    tr!("media-lastfm-group")
}
pub fn remove_folder() -> String {
    tr!("media-remove-folder")
}
pub fn levelling() -> String {
    tr!("media-levelling")
}
pub fn levelling_off() -> String {
    tr!("media-levelling-off")
}
pub fn levelling_track() -> String {
    tr!("media-levelling-track")
}
pub fn levelling_album() -> String {
    tr!("media-levelling-album")
}
pub fn levelling_auto() -> String {
    tr!("media-levelling-auto")
}
pub fn resume() -> String {
    tr!("media-resume")
}
pub fn resume_body() -> String {
    tr!("media-resume-body")
}
pub fn subtitles_setting() -> String {
    tr!("media-subtitles-setting")
}
pub fn scrobble() -> String {
    tr!("media-scrobble")
}
pub fn api_key() -> String {
    tr!("media-api-key")
}
pub fn api_secret() -> String {
    tr!("media-api-secret")
}
pub fn api_help() -> String {
    tr!("media-api-help")
}
pub fn sign_in() -> String {
    tr!("media-sign-in")
}
pub fn finish_sign_in() -> String {
    tr!("media-finish-sign-in")
}
pub fn sign_out() -> String {
    tr!("media-sign-out")
}
pub fn signed_in_as() -> String {
    tr!("media-signed-in-as")
}
pub fn lastfm_unreachable() -> String {
    tr!("media-lastfm-unreachable")
}
pub fn lastfm_bad_keys() -> String {
    tr!("media-lastfm-bad-keys")
}
pub fn lastfm_not_allowed() -> String {
    tr!("media-lastfm-not-allowed")
}
pub fn lastfm_failed() -> String {
    tr!("media-lastfm-failed")
}

/// "1 song", "12 songs".
pub fn count_songs(count: usize) -> String {
    tr!("media-songs", count = count)
}

/// "1 album", "3 albums".
pub fn count_albums(count: usize) -> String {
    tr!("media-albums", count = count)
}

/// "1 artist", "4 artists".
pub fn count_artists(count: usize) -> String {
    tr!("media-artists", count = count)
}

/// "1 video", "9 videos".
pub fn count_videos(count: usize) -> String {
    tr!("media-videos", count = count)
}

/// "3:45", "1:02:03": a song's or a film's length.
pub fn clock(seconds: f64) -> String {
    Timespan::from_secs_f64(seconds).digital().to_string()
}

/// "1 hr, 3 min left": what is left of a film watched part way.
pub fn left(seconds: f64) -> String {
    let rounded = (seconds / 60.0).ceil().max(1.0) * 60.0;
    tr!(
        "media-left",
        time = Timespan::from_secs_f64(rounded).to_string()
    )
}

/// "15 minutes", for the sleep timer menu. The length is formatted by
/// `bingux-ui`, so it already reads in the person's language.
pub fn minutes(count: u32) -> String {
    Timespan::new(std::time::Duration::from_secs(u64::from(count) * 60))
        .width(Width::Long)
        .to_string()
}

/// "Stops in 12:30", the sleep timer's state.
pub fn stops_in(seconds: f64) -> String {
    tr!("media-stops-in", time = clock(seconds))
}

/// What a screen reader hears when the next song starts: "Now playing First
/// Light by Aurora Lane".
pub fn now_playing_spoken(title: &str, artist: &str) -> String {
    if artist.is_empty() {
        tr!("media-now-playing-spoken-alone", title = title)
    } else {
        tr!("media-now-playing-spoken", title = title, artist = artist)
    }
}

/// Playback speed in a menu: "1×", "1.5×". A number and a sign, the same in
/// every language.
pub fn speed(rate: f64) -> String {
    let text = format!("{rate:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}
