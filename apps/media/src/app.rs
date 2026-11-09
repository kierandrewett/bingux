//! The window: the library in a navigation split view with the now-playing
//! bar along the bottom, or the full player over the whole window.
//!
//! What the window can do is declared as commands (`window_commands`): they
//! make the main menu, the command palette (Ctrl+Shift+P), the keyboard
//! shortcuts sheet (Ctrl+/) and the shortcuts: Ctrl+Space play or pause,
//! Ctrl+Left and Ctrl+Right previous and next, Ctrl+Up and Ctrl+Down the
//! volume, Ctrl+M mute, Ctrl+O open a file, Ctrl+Shift+F the player,
//! Ctrl+Shift+M the mini player, F11 full screen, Ctrl+U up next, Ctrl+,
//! settings. The keyboard's media keys work everywhere.
//!
//! The full player answers single keys too (plain letters stay with the
//! search field in the library): Space and K play or pause, Left and Right
//! seek 5 s (Shift: 30 s), J and L 10 s, Up and Down the volume, M mute,
//! F full screen, `[` and `]` the speed, C the subtitles, `,` and `.` a frame
//! back and forward while paused, Shift+L a loop between two points, S saves
//! the frame to Pictures, Escape leaves full screen and then the player.
//! They are listed in the shortcuts sheet as key hints.
//!
//! Files dragged onto the window from any app play; the window shows that it
//! takes them while the drag is over it.

use std::path::{Path, PathBuf};

use bingux_media::{Kind, PlayState, Repeat, Status};
use bingux_ui::a11y::use_announcer;
use bingux_ui::dnd::{DropTarget, Dropped, Format, use_drop_target};
use bingux_ui::files::{ChooserRequest, Filter, choose};
use bingux_ui::media::{ArtworkKind, VolumeChange, now_playing_bar, video_surface};
use bingux_ui::prelude::*;
use bingux_ui::task::spawn_blocking;
use bingux_ui::typography::EMPHASIS;
use freya::prelude::*;

use crate::engine::{engine, use_live};
use crate::pages::{SearchResults, Ui, add_folder, location};
use crate::stage::{
    Stage, close_stage, repeat_mode, run_seek, run_transport, run_volume, toggle_fullscreen,
    toggle_mini,
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

/// Plays files and folders given by path, on a worker: the folders are read.
pub fn play_paths(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("media-open".into())
        .spawn(move || engine().play_paths(&paths));
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
                play_paths(chosen.paths.clone());
            }
        },
    );
}

/// Shows the full player.
fn open_stage(ui: Ui) {
    let mut stage = ui.stage;
    stage.set(true);
}

fn change_volume(by: f64) {
    let status = engine().player.status();
    run_volume(VolumeChange::Level((status.volume + by).clamp(0.0, 100.0)));
}

/// The commands of the window, as things are now.
fn window_commands(
    window: Window,
    ui: Ui,
    overlays: Overlays,
    router: Router,
    status: &Status,
    can_next: bool,
) -> Vec<Command> {
    let controls = CommandGroup::Other(text::CONTROLS_GROUP);
    let has_item = status.item.is_some();
    let playing = status.state == PlayState::Playing;
    let stage_open = *ui.stage.peek();
    let mini = *ui.mini.peek();
    let fullscreen = *ui.fullscreen.peek();
    let repeat = status.repeat;
    let shuffle = status.shuffle;
    let muted = status.muted;
    let up_next = location(&["queue"]);
    vec![
        Command::new("file.open", text::MENU_OPEN, move |_| open_files(window))
            .shortcut("Ctrl+O")
            .icon(Icon::DocumentOpen)
            .group(CommandGroup::File)
            .keywords(["play", "file"]),
        Command::new("file.add-folder", text::MENU_ADD_FOLDER, move |_| {
            add_folder(window)
        })
        .icon(Icon::FolderMusic)
        .group(CommandGroup::File)
        .keywords(["library", "music"]),
        Command::new("file.update-library", text::MENU_SCAN, |_| engine().scan())
            .icon(Icon::Refresh)
            .group(CommandGroup::File)
            .keywords(["scan", "refresh", "rescan"]),
        Command::new(
            "controls.play-pause",
            if playing {
                text::COMMAND_PAUSE
            } else {
                text::COMMAND_PLAY
            },
            |_| engine().player.toggle(),
        )
        .shortcut("Ctrl+Space")
        .icon(if playing {
            Icon::Pause
        } else {
            Icon::MediaPlaybackStart
        })
        .enabled(has_item)
        .group(controls)
        .section("transport"),
        Command::new("controls.next", text::COMMAND_NEXT, |_| {
            engine().player.next()
        })
        .shortcut("Ctrl+Right")
        .icon(Icon::MediaSkipForward)
        .enabled(can_next)
        .group(controls)
        .section("transport"),
        Command::new("controls.previous", text::COMMAND_PREVIOUS, |_| {
            engine().player.previous()
        })
        .shortcut("Ctrl+Left")
        .icon(Icon::MediaSkipBackward)
        .enabled(has_item)
        .group(controls)
        .section("transport"),
        Command::new("controls.louder", text::COMMAND_LOUDER, |_| {
            change_volume(5.0)
        })
        .shortcut("Ctrl+Up")
        .icon(Icon::Sound)
        .group(controls)
        .section("volume")
        .keywords(["louder", "volume up"]),
        Command::new("controls.quieter", text::COMMAND_QUIETER, |_| {
            change_volume(-5.0)
        })
        .shortcut("Ctrl+Down")
        .group(controls)
        .section("volume")
        .keywords(["quieter", "volume down"]),
        Command::new("controls.mute", text::COMMAND_MUTE, move |_| {
            run_volume(VolumeChange::Mute(!muted))
        })
        .shortcut("Ctrl+M")
        .checked(muted)
        .group(controls)
        .section("volume")
        .keywords(["silence", "sound"]),
        Command::new("controls.shuffle", text::COMMAND_SHUFFLE, move |_| {
            engine().player.set_shuffle(!shuffle)
        })
        .checked(shuffle)
        .icon(Icon::MediaPlaylistShuffle)
        .group(controls)
        .section("order")
        .keywords(["random"]),
        Command::new("controls.repeat", text::COMMAND_REPEAT, move |_| {
            engine().player.set_repeat(if repeat == Repeat::All {
                Repeat::Off
            } else {
                Repeat::All
            })
        })
        .checked(repeat == Repeat::All)
        .icon(Icon::MediaPlaylistRepeat)
        .group(controls)
        .section("order")
        .keywords(["loop"]),
        Command::new("controls.repeat-one", text::COMMAND_REPEAT_ONE, move |_| {
            engine().player.set_repeat(if repeat == Repeat::One {
                Repeat::Off
            } else {
                Repeat::One
            })
        })
        .checked(repeat == Repeat::One)
        .group(controls)
        .section("order")
        .keywords(["loop"]),
        Command::new("view.player", text::MENU_PLAYER, move |_| {
            if *ui.stage.peek() {
                close_stage(ui)
            } else {
                open_stage(ui)
            }
        })
        .shortcut("Ctrl+Shift+F")
        .checked(stage_open)
        .enabled(has_item)
        .group(CommandGroup::View)
        .keywords(["now playing"]),
        Command::new("view.mini-player", text::MENU_MINI, move |_| {
            toggle_mini(ui)
        })
        .shortcut("Ctrl+Shift+M")
        .checked(mini)
        .icon(Icon::ViewPip)
        .group(CommandGroup::View)
        .keywords(["picture in picture", "small"]),
        Command::new("view.full-screen", text::MENU_FULL_SCREEN, move |_| {
            if !*ui.stage.peek() {
                open_stage(ui);
            }
            toggle_fullscreen(ui)
        })
        .shortcut("F11")
        .checked(fullscreen)
        .icon(Icon::ViewFullscreen)
        .group(CommandGroup::View),
        Command::new("go.up-next", text::MENU_UP_NEXT, move |_| {
            close_stage(ui);
            router.switch_root(&up_next);
        })
        .shortcut("Ctrl+U")
        .icon(Icon::ViewListOrdered)
        .group(CommandGroup::Go)
        .keywords(["queue"]),
        Command::new("app.settings", text::MENU_SETTINGS, move |_| {
            crate::settings_page::open(window, overlays)
        })
        .shortcut("Ctrl+,")
        .icon(Icon::PreferencesSystem)
        .keywords(["preferences", "last.fm", "folders"]),
        Command::key_hint("player.play", text::HINT_PLAY, "Space or K"),
        Command::key_hint("player.seek", text::HINT_SEEK, "Left and Right"),
        Command::key_hint(
            "player.seek-long",
            text::HINT_SEEK_LONG,
            "Shift+Left and Shift+Right",
        ),
        Command::key_hint("player.skip", text::HINT_SKIP, "J and L"),
        Command::key_hint("player.volume", text::HINT_VOLUME, "Up and Down"),
        Command::key_hint("player.mute", text::HINT_MUTE, "M"),
        Command::key_hint("player.full-screen", text::HINT_FULL_SCREEN, "F"),
        Command::key_hint("player.speed", text::HINT_SPEED, "[ and ]"),
        Command::key_hint("player.subtitles", text::HINT_SUBTITLES, "C"),
        Command::key_hint("player.frame", text::HINT_FRAME, ", and ."),
        Command::key_hint("player.loop", text::HINT_LOOP, "Shift+L"),
        Command::key_hint("player.snapshot", text::HINT_SNAPSHOT, "S"),
        Command::key_hint("player.leave", text::HINT_LEAVE, "Esc"),
    ]
}

/// A free name for the frame of `title` at `seconds` in `folder`:
/// "Test Pattern at 0.21.png", then "Test Pattern at 0.21 (2).png".
fn frame_path(folder: &Path, title: &str, seconds: f64) -> PathBuf {
    let clean: String = title
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    let clock = text::clock(seconds).replace(':', ".");
    let base = format!("{clean} at {clock}");
    let mut path = folder.join(format!("{base}.png"));
    let mut count = 2;
    while path.exists() {
        path = folder.join(format!("{base} ({count}).png"));
        count += 1;
    }
    path
}

/// Saves the frame on screen to Pictures and says so.
fn save_frame(overlays: Overlays) {
    let status = engine().player.status();
    let title = status.title().unwrap_or_default();
    let position = status.position;
    spawn(async move {
        let saved = spawn_blocking(move || {
            let folder = bingux_ui::files::user_dir(bingux_files::places::PlaceKind::Pictures)
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_default()
                        .join("Pictures")
                });
            let _ = std::fs::create_dir_all(&folder);
            let path = frame_path(&folder, &title, position);
            engine().player.save_frame(&path)
        })
        .await;
        overlays.toast(if saved {
            text::FRAME_SAVED
        } else {
            text::FRAME_NOT_SAVED
        });
    });
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
        let announcer = use_announcer();
        let query = use_state(String::new);
        let status = live.status.read().clone();
        let queue = live.queue.read().clone();
        let stage_open = *ui.stage.read();
        // Read so the commands' ticks follow them.
        let _ = (*ui.mini.read(), *ui.fullscreen.read());
        let can_next = !queue.upcoming().is_empty() || status.repeat != Repeat::Off;
        let location_now = use_location();
        let router = use_router();
        use_window_commands(window_commands(
            window, ui, overlays, router, &status, can_next,
        ));

        // A launch or a second launch with a file: play it, show the player,
        // and leave the address for the queue.
        use_side_effect_with_deps(&location_now, move |location| {
            if location.segments().first().map(String::as_str) != Some("open") {
                return;
            }
            if let Some(path) = location.query("path").filter(|path| !path.is_empty()) {
                play_paths(vec![PathBuf::from(path)]);
                open_stage(ui);
            }
            router.replace(&crate::pages::location(&["queue"]));
        });

        // A screen reader hears each song or film as it starts.
        let now = status
            .item
            .as_ref()
            .map(|item| (item.path.clone(), item.title.clone(), item.artist.clone()));
        use_side_effect_with_deps(&now, move |now| {
            if let Some((_, title, artist)) = now {
                announcer.announce(text::now_playing_spoken(title, artist));
            }
        });

        let on_key = move |event: Event<KeyboardEventData>| {
            if overlays.dialog_open() {
                return;
            }
            let status = engine().player.status();
            let player = &engine().player;
            let held = event.modifiers;
            let ctrl = held.contains(Modifiers::CONTROL) || held.contains(Modifiers::ALT);
            let shift = held.contains(Modifiers::SHIFT);
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
                Key::Named(NamedKey::AudioVolumeUp) => change_volume(5.0),
                Key::Named(NamedKey::AudioVolumeDown) => change_volume(-5.0),
                Key::Named(NamedKey::AudioVolumeMute) => {
                    run_volume(VolumeChange::Mute(!status.muted))
                }
                _ if !in_stage || ctrl => handled = false,
                Key::Character(text) => match text.as_str() {
                    " " | "k" | "K" => player.toggle(),
                    "j" | "J" => player.seek_by(-10.0),
                    "l" | "L" if !shift => player.seek_by(10.0),
                    "L" | "l" => player.cycle_loop(),
                    "m" | "M" => run_volume(VolumeChange::Mute(!status.muted)),
                    "f" | "F" => toggle_fullscreen(ui),
                    "[" => player.set_speed(status.speed - 0.25),
                    "]" => player.set_speed(status.speed + 0.25),
                    "s" | "S" if status.has_video => save_frame(overlays),
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
                Key::Named(NamedKey::ArrowUp) => change_volume(5.0),
                Key::Named(NamedKey::ArrowDown) => change_volume(-5.0),
                Key::Named(NamedKey::Escape) => {
                    if *ui.fullscreen.peek() {
                        set_fullscreen(ui, false);
                    } else if *ui.mini.peek() {
                        set_mini(ui, false);
                    } else {
                        close_stage(ui);
                    }
                }
                _ => handled = false,
            }
            if handled {
                event.stop_propagation();
                event.prevent_default();
            }
        };

        // Files dragged in from Files or any other app play.
        let drop = use_drop_target(
            DropTarget::new()
                .accepts([Format::Uris])
                .on_drop(|dropped: Dropped| play_paths(dropped.data.files())),
        );

        let body: Element = if stage_open {
            Stage { ui }.into_element()
        } else {
            let results: Element = SearchResults { query }.into();
            let split = NavigationSplitView::new("Media")
                .search(query, results, |_| {})
                .header_trailing(app_menu(Vec::new()));
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
                    .can_next(can_next)
                    .volume(status.volume, status.muted, run_volume)
                    .on_transport(action)
                    .on_seek(run_seek)
                    .on_open(move |_| open_stage(ui))
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
        let dragging = drop.over().is_some();
        drop.track(
            rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                .on_global_key_down(on_key)
                .child(body)
                .maybe_child(dragging.then(|| {
                    // The whole window takes the drop: it says so.
                    rect()
                        .layer(Layer::Overlay)
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .expanded()
                        .tw(
                            &tokens,
                            &[
                                bg(Paint::AccentSoft),
                                border(Line::Ring(Paint::Accent)),
                                rounded(Radius::Window),
                                center(),
                            ],
                        )
                        .child(
                            rect()
                                .tw(
                                    &tokens,
                                    &[
                                        rounded(Radius::Overlay),
                                        bg(Paint::Popover),
                                        shadow(Elevation::Floating),
                                        px(S4),
                                        py(S3),
                                    ],
                                )
                                .child(EMPHASIS.label(&tokens, text::DROP_HERE)),
                        )
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_names_are_free_and_clean() {
        let dir = std::env::temp_dir().join(format!("media-frame-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = frame_path(&dir, "A/B", 21.4);
        assert_eq!(first, dir.join("A_B at 0.21.png"));
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(
            frame_path(&dir, "A/B", 21.4),
            dir.join("A_B at 0.21 (2).png")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
