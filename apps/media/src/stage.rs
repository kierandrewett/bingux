//! The full player: a film filling the window with its controls floating over
//! it, or a song's artwork large with its controls under it. The mini player
//! is the same view in a small window.
//!
//! Over a film the controls and the pointer hide 2.5 s after the pointer last
//! moved while it plays, and come back with a move or a key. A press on the
//! picture plays or pauses, a double press goes full screen. Hovering the
//! timeline shows the frame at that moment.

use std::time::Duration;

use bingux_media::{Kind, PlayState, Precision, Repeat, Sleep, SleepLeft, Status};
use bingux_ui::controls::TipSide;
use bingux_ui::files::{ChooserRequest, Filter, choose};
use bingux_ui::media::{
    ArtworkKind, RepeatMode, Seek, TransportAction, VideoFrame, VideoFrames, VolumeChange, artwork,
    timeline, transport, video_surface, volume_control,
};
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::shell::drag_on_pointer_down;
use bingux_ui::typography::{BODY, CAPTION, EMPHASIS, TITLE};
use freya::prelude::*;

use crate::engine::{engine, use_media};
use crate::pages::Ui;
use crate::strings as text;

/// How long the controls stay over a playing film after the pointer stops.
const HIDE_AFTER: Duration = Duration::from_millis(2500);
/// A stage narrower than this is a phone: the footer drops the volume slider.
const NARROW_STAGE: f32 = 480.0;
/// The seek preview's longer side, in pixels.
const PREVIEW_SIDE: u32 = 192;

pub fn repeat_mode(repeat: Repeat) -> RepeatMode {
    match repeat {
        Repeat::Off => RepeatMode::Off,
        Repeat::All => RepeatMode::All,
        Repeat::One => RepeatMode::One,
    }
}

/// Runs a transport button.
pub fn run_transport(action: TransportAction, status: &Status) {
    let player = &engine().player;
    match action {
        TransportAction::PlayPause => player.toggle(),
        TransportAction::Previous => player.previous(),
        TransportAction::Next => player.next(),
        TransportAction::SkipBack => player.seek_by(-10.0),
        TransportAction::SkipForward => player.seek_by(10.0),
        TransportAction::Shuffle => player.set_shuffle(!status.shuffle),
        TransportAction::Repeat => player.set_repeat(status.repeat.next()),
    }
}

pub fn run_seek(seek: Seek) {
    engine().player.seek(
        seek.to,
        if seek.done {
            Precision::Exact
        } else {
            Precision::Fast
        },
    );
}

pub fn run_volume(change: VolumeChange) {
    match change {
        VolumeChange::Level(level) => engine().player.set_volume(level),
        VolumeChange::Mute(muted) => engine().player.set_muted(muted),
    }
    let status = engine().player.status();
    engine().change_settings(move |settings| {
        settings.volume = status.volume;
        settings.muted = status.muted;
    });
}

/// Flips full screen. The read of the current value ends before the write.
pub fn toggle_fullscreen(ui: Ui) {
    let on = !*ui.fullscreen.peek();
    set_fullscreen(ui, on);
}

/// Flips the mini player. The read of the current value ends before the write.
pub fn toggle_mini(ui: Ui) {
    let on = !*ui.mini.peek();
    set_mini(ui, on);
}

/// Turns full screen on or off.
pub fn set_fullscreen(ui: Ui, on: bool) {
    let mut fullscreen = ui.fullscreen;
    fullscreen.set(on);
    Platform::get().with_window(Platform::window_id(), move |window| {
        window.set_fullscreen(on.then_some(freya::winit::window::Fullscreen::Borderless(None)));
    });
}

/// Turns the mini player on or off: a small window that keeps playing.
pub fn set_mini(ui: Ui, on: bool) {
    let mut mini = ui.mini;
    let mut stage = ui.stage;
    mini.set(on);
    stage.set(on || *stage.peek());
    let (width, height) = if on { (360.0, 400.0) } else { (1100.0, 760.0) };
    Platform::get().with_window(Platform::window_id(), move |window| {
        let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(width, height));
        // Winit applies the new size at once but sends no resize event for it, so the
        // redraw is what lays the mini layout out at the new size.
        window.request_redraw();
    });
}

fn sleep_items(status: &Status) -> Vec<MenuItem> {
    let on = status.sleep;
    let mut items =
        vec![menu_item(text::OFF, |_| engine().player.set_sleep(Sleep::Off)).checked(on.is_none())];
    for minutes in [15u32, 30, 45, 60] {
        items.push(menu_item(text::minutes(minutes), move |_| {
            engine()
                .player
                .set_sleep(Sleep::After(Duration::from_secs(u64::from(minutes) * 60)))
        }));
    }
    items.push(
        menu_item(text::END_OF_ITEM, |_| {
            engine().player.set_sleep(Sleep::EndOfItem)
        })
        .checked(on == Some(SleepLeft::EndOfItem)),
    );
    items
}

fn speed_items(status: &Status) -> Vec<MenuItem> {
    [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]
        .into_iter()
        .map(|rate| {
            menu_item(text::speed(rate), move |_| engine().player.set_speed(rate))
                .checked((status.speed - rate).abs() < 0.01)
        })
        .collect()
}

fn subtitle_items(status: &Status, window: Window) -> Vec<MenuItem> {
    let any = status.subtitle_tracks().any(|track| track.selected);
    let mut items =
        vec![menu_item(text::OFF, |_| engine().player.select_subtitle(None)).checked(!any)];
    for track in status.subtitle_tracks() {
        let id = track.id;
        items.push(
            menu_item(track.label(), move |_| {
                engine().player.select_subtitle(Some(id))
            })
            .checked(track.selected),
        );
    }
    items.push(menu_separator());
    items.push(menu_item(text::ADD_SUBTITLE_FILE, move |_| {
        choose(
            window,
            ChooserRequest::open().filter(
                Filter::new("Subtitles")
                    .glob("*.srt")
                    .glob("*.ass")
                    .glob("*.ssa")
                    .glob("*.vtt")
                    .glob("*.sub"),
            ),
            |chosen| {
                if let Some(chosen) = chosen
                    && let Some(path) = chosen.paths.first()
                {
                    engine().player.add_subtitle(path);
                }
            },
        );
    }));
    items
}

fn audio_items(status: &Status) -> Vec<MenuItem> {
    status
        .audio_tracks()
        .map(|track| {
            let id = track.id;
            menu_item(track.label(), move |_| engine().player.select_audio(id))
                .checked(track.selected)
        })
        .collect()
}

fn chapter_items(status: &Status) -> Vec<MenuItem> {
    status
        .chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            menu_item(
                format!("{}  {}", text::clock(chapter.start), chapter.title),
                move |_| engine().player.go_to_chapter(index),
            )
            .checked(status.chapter == Some(index))
        })
        .collect()
}

/// The full player.
#[derive(PartialEq)]
pub struct Stage {
    pub ui: Ui,
}

impl Component for Stage {
    fn render(&self) -> impl IntoElement {
        let live = use_media();
        let status = live.status.read().clone();
        let film = status.has_video
            || status
                .item
                .as_ref()
                .is_some_and(|item| item.kind == Kind::Video);
        if film {
            // A film's controls are dark in a light window too.
            scheme_scope(Scheme::PreferDark, FilmStage { ui: self.ui })
                .fill()
                .into_element()
        } else {
            SongStage { ui: self.ui }.into_element()
        }
    }
}

#[derive(PartialEq)]
struct FilmStage {
    ui: Ui,
}

impl Component for FilmStage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let window = use_window();
        let ui = self.ui;
        let status = live.status.read().clone();
        let shown = use_state(|| true);
        let moves = use_state(|| 0u64);
        let mut hovering_controls = use_state(|| false);
        let preview_frames = use_hook(VideoFrames::new);
        let mut previewing = use_state(|| false);
        let playing = status.state == PlayState::Playing;
        let mini = *ui.mini.read();
        let fullscreen = *ui.fullscreen.read();

        // Any move shows the controls; they hide again once it is still.
        let reveal = move || {
            let (mut moves, mut shown) = (moves, shown);
            let count = *moves.peek() + 1;
            moves.set(count);
            shown.set_if_modified(true);
            spawn(async move {
                timer(HIDE_AFTER).await;
                if *moves.peek() == count && !*hovering_controls.peek() {
                    shown.set_if_modified(false);
                }
            });
        };
        // The controls show when the film starts and fade like any others.
        use_hook(reveal);
        let visible = *shown.read() || !playing;
        // The pointer hides with the controls.
        use_side_effect_with_deps(&visible, move |visible| {
            let visible = *visible;
            Platform::get().with_window(Platform::window_id(), move |window| {
                window.set_cursor_visible(visible)
            });
        });
        use_drop(|| {
            Platform::get().with_window(Platform::window_id(), |window| {
                window.set_cursor_visible(true)
            });
        });

        let title = status.title().unwrap_or_default();
        let picture = video_surface(engine().frames.clone())
            .label(title.clone())
            .on_press(|_| engine().player.toggle())
            .on_double_press(move |_| toggle_fullscreen(ui));

        let path = status.item.as_ref().map(|item| item.path.clone());
        let preview = (*previewing.read()).then(|| {
            rect()
                .tw(&tokens, &[rounded(Radius::Control), clip()])
                .width(Size::px(PREVIEW_SIDE as f32 * 0.8))
                .height(Size::px(PREVIEW_SIDE as f32 * 0.45))
                .child(video_surface(preview_frames.clone()).label(text::NOW_PLAYING))
                .into_element()
        });
        let on_hover = {
            let preview_frames = preview_frames.clone();
            move |at: Option<f64>| match (&path, at) {
                (Some(path), Some(at)) => {
                    previewing.set_if_modified(true);
                    let frames = preview_frames.clone();
                    engine()
                        .thumbs
                        .preview(path, at, PREVIEW_SIDE, move |frame| {
                            if let Some(frame) = frame {
                                frames.push(VideoFrame {
                                    width: frame.width,
                                    height: frame.height,
                                    pixels: frame.pixels,
                                });
                            }
                        });
                }
                _ => previewing.set_if_modified(false),
            }
        };
        let seek_bar = timeline(status.position, status.duration, run_seek)
            .buffered(status.buffered.clone())
            .chapters(
                status
                    .chapters
                    .iter()
                    .map(|chapter| (chapter.start, chapter.title.clone()))
                    .collect(),
            )
            .loop_points(status.loop_a, status.loop_b)
            .on_hover(on_hover)
            .preview(preview)
            .on_surface(Paint::Popover)
            .times();
        let action = {
            let status = status.clone();
            move |action: TransportAction| run_transport(action, &status)
        };
        let mut tools = hstack(S1);
        if status.subtitle_tracks().next().is_some() || !mini {
            tools = tools.child(
                menu_button(text::SUBTITLES, subtitle_items(&status, window))
                    .icon(Icon::MediaViewSubtitles),
            );
        }
        if status.audio_tracks().count() > 1 {
            tools =
                tools.child(menu_button(text::AUDIO_TRACK, audio_items(&status)).icon(Icon::Sound));
        }
        if !status.chapters.is_empty() && !mini {
            tools = tools.child(
                menu_button(text::CHAPTERS, chapter_items(&status)).icon(Icon::ViewListOrdered),
            );
        }
        if !mini {
            tools = tools
                .child(menu_button(text::SPEED, speed_items(&status)).icon(Icon::MediaSeekForward));
        }
        tools = tools
            .child(
                icon_button(Icon::ViewPip, move |_| toggle_mini(ui))
                    .active(mini)
                    .tooltip_side(TipSide::Above)
                    .tooltip(if mini {
                        text::LEAVE_MINI_PLAYER
                    } else {
                        text::MINI_PLAYER
                    }),
            )
            .child(
                icon_button(Icon::ViewFullscreen, move |_| toggle_fullscreen(ui))
                    .morph(Icon::ViewRestore, fullscreen)
                    .tooltip_side(TipSide::Above)
                    .tooltip(if fullscreen {
                        text::LEAVE_FULL_SCREEN
                    } else {
                        text::FULL_SCREEN
                    }),
            );
        let card = |content: Rect| {
            content.tw(
                &tokens,
                &[
                    rounded(Radius::Overlay),
                    bg(Paint::Popover),
                    shadow(Elevation::Floating),
                    p(S2),
                ],
            )
        };
        // Each bar is pinned to its own edge. An absolute box is not given
        // its parent's height, so the bottom bar is placed from the measured
        // heights of the stage and of the bar.
        let mut stage_size = use_state(|| (0.0f32, 0.0f32));
        let (stage_width, stage_height) = *stage_size.read();
        // A phone-width stage keeps the mute button and drops the slider, so
        // the slider cannot collide with the transport. Volume keys still work.
        let narrow = stage_width > 0.0 && stage_width < NARROW_STAGE;
        // The footer's measured area: the transport's tips open above the whole
        // bar, clear of the timeline, not just above their own button.
        let mut controls_area = use_state(|| None::<Area>);
        let controls = card(
            rect()
                .width(Size::fill())
                .tw(&tokens, &[flex_col(), gap(S1)]),
        )
        .on_sized(move |event: Event<SizedEventData>| {
            controls_area.set_if_modified(Some(event.area))
        })
        .child(seek_bar)
        .child(
            rect()
                .width(Size::fill())
                .tw(
                    &tokens,
                    &[flex_row(), grow_children(), items(Align::Center), gap(S2)],
                )
                .child(
                    transport(playing, action)
                        .tooltips_over(controls_area)
                        .skips()
                        .can_next(live.queue.read().upcoming().len() > 0)
                        .compact_if(mini),
                )
                .child(rect().width(Size::flex(1.0)))
                .maybe_child((!mini).then(|| {
                    let volume = volume_control(status.volume, status.muted, run_volume);
                    if narrow { volume.compact() } else { volume }
                }))
                .child(tools),
        );
        let top = card(rect().tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)]))
            .child(icon_button(Icon::PanDown, move |_| close_stage(ui)).tooltip(text::CLOSE_PLAYER))
            .child(
                EMPHASIS
                    .label(&tokens, title)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        let fade = use_tween(if visible { 1.0 } else { 0.0 }, tokens.motion(200));
        let margin = if mini { S2 } else { S4 };
        let edge = |position: Position| {
            rect()
                .position(position)
                .width(Size::px(stage_width))
                .tw(&tokens, &[p(margin)])
        };
        let header = edge(Position::new_absolute().top(0.0).left(0.0))
            // The empty part of the top moves the window, as a title bar does.
            .on_pointer_down(|event: Event<PointerEventData>| drag_on_pointer_down(event))
            .child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), justify(Align::Between)],
                    )
                    .child(top)
                    .maybe_child(
                        (!fullscreen).then(|| card(rect()).child(WindowControls).into_element()),
                    ),
            );
        let footer = rect()
            .width(Size::fill())
            .tw(&tokens, &[p(margin)])
            .on_pointer_enter(move |_| hovering_controls.set(true))
            .on_pointer_leave(move |_| hovering_controls.set(false))
            .child(controls);
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Backdrop), clip()])
            .on_sized(move |event: Event<SizedEventData>| {
                stage_size.set_if_modified((event.area.width(), event.area.height()))
            })
            .on_pointer_move(move |_| reveal())
            .on_global_key_down(move |_: Event<KeyboardEventData>| reveal())
            .child(picture)
            .maybe_child(status.error.is_some().then(|| {
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .expanded()
                    .tw(&tokens, &[center()])
                    .child(card(rect()).child(BODY.label(&tokens, text::CANNOT_PLAY)))
            }))
            .maybe_child((fade > 0.01).then(|| {
                rect()
                    .layer(Layer::Overlay)
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::px(stage_width))
                    .height(Size::px(stage_height))
                    .tw(&tokens, &[flex_col(), justify(Align::End)])
                    .opacity(fade)
                    .child(header)
                    .child(footer)
            }))
    }
}

/// Leaves the player: out of full screen and the mini player first.
pub fn close_stage(ui: Ui) {
    if *ui.fullscreen.peek() {
        set_fullscreen(ui, false);
    }
    if *ui.mini.peek() {
        set_mini(ui, false);
    }
    let mut stage = ui.stage;
    stage.set(false);
}

#[derive(PartialEq)]
struct SongStage {
    ui: Ui,
}

impl Component for SongStage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let router = use_router();
        let ui = self.ui;
        let status = live.status.read().clone();
        let mini = *ui.mini.read();
        let mut size = use_state(|| (0.0f32, 0.0f32));
        let (width, height) = *size.read();
        let side = (width * 0.8)
            .min(height * if mini { 0.42 } else { 0.48 })
            .clamp(96.0, 480.0);
        let item = status.item.clone();
        let title = status
            .title()
            .unwrap_or_else(|| text::NOT_PLAYING.to_owned());
        let subtitle = item
            .as_ref()
            .map(
                |item| match (item.artist.is_empty(), item.album.is_empty()) {
                    (false, false) => format!("{}, {}", item.artist, item.album),
                    (false, true) => item.artist.clone(),
                    _ => item.album.clone(),
                },
            )
            .unwrap_or_default();
        let action = {
            let status = status.clone();
            move |action: TransportAction| run_transport(action, &status)
        };
        let close = move |_| close_stage(ui);
        let sleep_label = match status.sleep {
            Some(SleepLeft::Seconds(seconds)) => text::stops_in(f64::from(seconds)),
            Some(SleepLeft::EndOfItem) => text::STOPS_AT_END.to_owned(),
            None => String::new(),
        };
        let queue_target = Location::new(crate::SCHEME).join("queue");
        // The player covers the library's header, so it has its own: it
        // moves the window and holds the window's buttons.
        let header = HeaderBar::new("")
            .fill(Paint::Bg)
            .leading(icon_button(Icon::PanDown, close).tooltip(text::CLOSE_PLAYER))
            .trailing(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
                    .maybe_child(
                        (!sleep_label.is_empty())
                            .then(|| CAPTION.in_ink(Ink::Muted).label(&tokens, sleep_label)),
                    )
                    .child(
                        menu_button(text::SLEEP_TIMER, sleep_items(&status))
                            .icon(Icon::WeatherClearNight),
                    )
                    .maybe_child((!mini).then(|| {
                        icon_button(Icon::ViewListOrdered, move |_| {
                            let mut stage = ui.stage;
                            stage.set(false);
                            router.switch_root(&queue_target);
                        })
                        .tooltip(text::QUEUE)
                    }))
                    .child(
                        icon_button(Icon::ViewPip, move |_| toggle_mini(ui))
                            .active(mini)
                            .tooltip(if mini {
                                text::LEAVE_MINI_PLAYER
                            } else {
                                text::MINI_PLAYER
                            }),
                    ),
            )
            .trailing(WindowControls);
        let body = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .tw(
                &tokens,
                &[
                    flex_col(),
                    grow_children(),
                    items(Align::Center),
                    px(if mini { S3 } else { S6 }),
                    pb(if mini { S3 } else { S6 }),
                    gap(S3),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified((event.area.width(), event.area.height()))
            })
            .child(rect().height(Size::flex(1.0)))
            .child(
                artwork(
                    item.as_ref().and_then(|item| item.artwork.clone()),
                    item.as_ref()
                        .map(|item| item.album.clone())
                        .unwrap_or_default(),
                )
                .kind(ArtworkKind::Album)
                .side(side),
            )
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[flex_col(), items(Align::Center), gap(S1), max_w(Dim::Page)],
                    )
                    .child(
                        TITLE
                            .label(&tokens, title)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .maybe_child((!subtitle.is_empty()).then(|| {
                        BODY.in_ink(Ink::Muted)
                            .label(&tokens, subtitle)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                    })),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[max_w(Dim::Page)])
                    .child(
                        timeline(status.position, status.duration, run_seek)
                            .buffered(status.buffered.clone())
                            .chapters(
                                status
                                    .chapters
                                    .iter()
                                    .map(|chapter| (chapter.start, chapter.title.clone()))
                                    .collect(),
                            )
                            .times(),
                    ),
            )
            .child(
                transport(status.state == PlayState::Playing, action)
                    .shuffle(status.shuffle)
                    .repeat(repeat_mode(status.repeat))
                    .enabled(status.item.is_some()),
            )
            .maybe_child((!mini).then(|| volume_control(status.volume, status.muted, run_volume)))
            .child(rect().height(Size::flex(1.0)));
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg), flex_col(), grow_children()])
            .child(header)
            .child(body)
    }
}

/// `compact()` when a condition holds, for a row whose room changes.
trait CompactIf {
    fn compact_if(self, compact: bool) -> Self;
}

impl CompactIf for bingux_ui::media::Transport {
    fn compact_if(self, compact: bool) -> Self {
        if compact { self.compact() } else { self }
    }
}
