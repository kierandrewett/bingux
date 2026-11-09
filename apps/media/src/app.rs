//! The window: the library in a navigation split view with the now-playing
//! bar along the bottom, or the full player over the whole window.
//!
//! Keys. In the full player: Space and K play or pause, Left and Right seek
//! 5 s (Shift: 30 s), J and L 10 s, Up and Down the volume, M mute, F full
//! screen, `[` and `]` the speed, C the subtitles, `,` and `.` a frame back
//! and forward while paused, Escape leaves full screen and then the player.
//! Everywhere: the keyboard's media keys, Ctrl+Space play or pause, Ctrl+Left
//! and Ctrl+Right previous and next, Ctrl+Up and Ctrl+Down the volume,
//! Ctrl+O open a file. (Plain letters stay with the search field in the
//! library.)

use std::path::PathBuf;

use bingux_media::{Kind, PlayState};
use bingux_ui::files::{ChooserRequest, Filter, choose};
use bingux_ui::media::{ArtworkKind, now_playing_bar, video_surface};
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::engine::{engine, use_live};
use crate::pages::{SearchResults, Ui, add_folder};
use crate::stage::{
    Stage, repeat_mode, run_seek, run_transport, run_volume, set_fullscreen, set_mini,
};
use crate::strings as text;

/// The window root.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        use_live();
        use_provide_context(|| Ui {
            stage: State::create(std::env::var_os("BINGUX_MEDIA_STAGE").is_some()),
            mini: State::create(false),
            fullscreen: State::create(false),
        });
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

/// Asks for files to play.
pub fn open_files(window: Window) {
    choose(
        window,
        ChooserRequest::open()
            .title(text::MENU_OPEN)
            .multiple(true)
            .filter(
                Filter::new("Music and videos")
                    .mime("audio/*")
                    .mime("video/*"),
            )
            .filter(Filter::new("All files")),
        |chosen| {
            if let Some(chosen) = chosen {
                let paths = chosen.paths.clone();
                let _ = std::thread::Builder::new()
                    .name("media-open".into())
                    .spawn(move || engine().play_paths(&paths));
            }
        },
    );
}

fn main_menu(window: Window, ui: Ui, overlays: Overlays) -> Vec<MenuItem> {
    vec![
        menu_item(text::MENU_OPEN, move |_| open_files(window)).shortcut("Ctrl+O"),
        menu_item(text::MENU_ADD_FOLDER, move |_| add_folder(window)),
        menu_item(text::MENU_SCAN, |_| engine().scan()),
        menu_separator(),
        menu_item(text::MENU_MINI, move |_| set_mini(ui, true)),
        menu_item(text::MENU_SETTINGS, move |_| {
            crate::settings_page::open(window, overlays)
        })
        .shortcut("Ctrl+,"),
    ]
}

#[derive(PartialEq)]
struct Content;

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = crate::engine::use_media();
        let ui = use_consume::<Ui>();
        let window = use_window();
        let overlays = use_overlays();
        let query = use_state(String::new);
        let status = live.status.read().clone();
        let queue = live.queue.read().clone();
        let stage_open = *ui.stage.read();
        // A launch or a second launch with a file: play it, show the player,
        // and leave the address for the queue.
        let location = use_location();
        let router = use_router();
        use_side_effect_with_deps(&location, move |location| {
            if location.segments().first().map(String::as_str) != Some("open") {
                return;
            }
            if let Some(path) = location.query("path").filter(|path| !path.is_empty()) {
                let path = PathBuf::from(path);
                let _ = std::thread::Builder::new()
                    .name("media-open".into())
                    .spawn(move || engine().play_paths(&[path]));
                let mut stage = ui.stage;
                stage.set(true);
            }
            router.replace(&crate::pages::location(&["queue"]));
        });

        let on_key = move |event: Event<KeyboardEventData>| {
            let status = engine().player.status();
            let player = &engine().player;
            let ctrl = event.modifiers.contains(Modifiers::CONTROL);
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            let in_stage = *ui.stage.peek();
            let mut handled = true;
            match &event.key {
                Key::Named(NamedKey::MediaPlayPause) => player.toggle(),
                Key::Named(NamedKey::MediaPlay) => player.play(),
                Key::Named(NamedKey::MediaPause) | Key::Named(NamedKey::MediaStop) => {
                    player.pause()
                }
                Key::Named(NamedKey::MediaTrackNext) => player.next(),
                Key::Named(NamedKey::MediaTrackPrevious) => player.previous(),
                Key::Character(text) if ctrl && text == " " => player.toggle(),
                Key::Character(text) if ctrl && text == "o" => open_files(window),
                Key::Character(text) if ctrl && text == "," => {
                    crate::settings_page::open(window, overlays)
                }
                Key::Named(NamedKey::ArrowRight) if ctrl => player.next(),
                Key::Named(NamedKey::ArrowLeft) if ctrl => player.previous(),
                Key::Named(NamedKey::ArrowUp) if ctrl => {
                    run_volume(bingux_ui::media::VolumeChange::Level(status.volume + 5.0))
                }
                Key::Named(NamedKey::ArrowDown) if ctrl => {
                    run_volume(bingux_ui::media::VolumeChange::Level(status.volume - 5.0))
                }
                _ if !in_stage || ctrl => handled = false,
                Key::Character(text) => match text.as_str() {
                    " " | "k" | "K" => player.toggle(),
                    "j" | "J" => player.seek_by(-10.0),
                    "l" | "L" => player.seek_by(10.0),
                    "m" | "M" => run_volume(bingux_ui::media::VolumeChange::Mute(!status.muted)),
                    "f" | "F" => set_fullscreen(ui, !*ui.fullscreen.peek()),
                    "[" => player.set_speed(status.speed - 0.25),
                    "]" => player.set_speed(status.speed + 0.25),
                    "c" | "C" => {
                        let tracks: Vec<i64> =
                            status.subtitle_tracks().map(|track| track.id).collect();
                        let current = status.subtitle_tracks().position(|track| track.selected);
                        let next = match current {
                            None => tracks.first().copied(),
                            Some(index) => tracks.get(index + 1).copied(),
                        };
                        player.select_subtitle(next);
                    }
                    "," if status.state == PlayState::Paused => player.frame_step(false),
                    "." if status.state == PlayState::Paused => player.frame_step(true),
                    _ => handled = false,
                },
                Key::Named(NamedKey::ArrowLeft) => player.seek_by(if shift { -30.0 } else { -5.0 }),
                Key::Named(NamedKey::ArrowRight) => player.seek_by(if shift { 30.0 } else { 5.0 }),
                Key::Named(NamedKey::ArrowUp) => {
                    run_volume(bingux_ui::media::VolumeChange::Level(status.volume + 5.0))
                }
                Key::Named(NamedKey::ArrowDown) => {
                    run_volume(bingux_ui::media::VolumeChange::Level(status.volume - 5.0))
                }
                Key::Named(NamedKey::Escape) => {
                    if *ui.fullscreen.peek() {
                        set_fullscreen(ui, false);
                    } else if *ui.mini.peek() {
                        set_mini(ui, false);
                    } else {
                        let mut stage = ui.stage;
                        stage.set(false);
                    }
                }
                _ => handled = false,
            }
            if handled {
                event.stop_propagation();
            }
        };
        let on_drop = move |event: Event<FileEventData>| {
            let paths: Vec<PathBuf> = event.file_paths.clone();
            let _ = std::thread::Builder::new()
                .name("media-open".into())
                .spawn(move || engine().play_paths(&paths));
        };

        let body: Element = if stage_open {
            Stage { ui }.into_element()
        } else {
            let results: Element = SearchResults { query }.into();
            let split = NavigationSplitView::new("Media")
                .search(query, results, |_| {})
                .header_trailing(app_menu(main_menu(window, ui, overlays)));
            let item = status.item.clone();
            let bar = item.map(|item| {
                let video = item.kind == Kind::Video || status.has_video;
                let subtitle = match (item.artist.is_empty(), item.album.is_empty()) {
                    (false, false) => format!("{}, {}", item.artist, item.album),
                    (false, true) => item.artist.clone(),
                    _ => item.album.clone(),
                };
                let action = {
                    let status = status.clone();
                    move |action| run_transport(action, &status)
                };
                let mut bar = now_playing_bar(status.title().unwrap_or_default(), subtitle)
                    .artwork(
                        item.artwork.clone(),
                        item.album.clone(),
                        if video {
                            ArtworkKind::Video
                        } else {
                            ArtworkKind::Album
                        },
                    )
                    .playing(status.state == PlayState::Playing)
                    .progress(status.position, status.duration)
                    .buffered(status.buffered.clone())
                    .chapters(
                        status
                            .chapters
                            .iter()
                            .map(|chapter| (chapter.start, chapter.title.clone()))
                            .collect(),
                    )
                    .shuffle(status.shuffle)
                    .repeat(repeat_mode(status.repeat))
                    .can_next(
                        !queue.upcoming().is_empty() || status.repeat != bingux_media::Repeat::Off,
                    )
                    .volume(status.volume, status.muted, run_volume)
                    .on_transport(action)
                    .on_seek(run_seek)
                    .on_open(move |_| {
                        let mut stage = ui.stage;
                        stage.set(true);
                    })
                    .trailing(
                        icon_button(Icon::ViewPip, move |_| set_mini(ui, true))
                            .tooltip(text::MINI_PLAYER),
                    );
                if video {
                    // The film plays on, small, in the bar.
                    bar = bar.picture(
                        video_surface(engine().frames.clone())
                            .label(status.title().unwrap_or_default())
                            .into_element(),
                    );
                }
                bar
            });
            rect()
                .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .child(split),
                )
                .maybe_child(bar)
                .into_element()
        };
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .on_global_key_down(on_key)
            .on_file_drop(on_drop)
            .child(body)
    }
}
