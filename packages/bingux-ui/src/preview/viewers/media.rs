//! Films and songs: the player.
//!
//! A [`HostLink`] runs `bingux-preview-host`, which decodes with GStreamer and
//! sends the picture as shared-memory frames already scaled to this box; the
//! viewer draws the newest one letterboxed on black, with the subtitle line,
//! a spinner while it waits and the control bar floating over it. A song is
//! the file's card with the same bar. Nothing here waits on the helper: its
//! packets arrive in a task, a frame is one copy and one `ImageHandle`.
//!
//! | | Popup, full screen, sheet | Docked pane |
//! | - | ------------------------- | ----------- |
//! | Starts | playing, with sound (the person asked for this file) | paused on the first frame: a pane follows the selection, so it never makes sound by itself; the big play button or a click plays |
//! | Keys | the arrows choose the file; Shift and an arrow seeks 5 s, J and L 10 s, K plays and pauses (Space closes) | the same once it has the keyboard (a click); Space plays and pauses; Esc gives the keyboard back to the list |
//! | Bar | play, time, seek, time, mute, volume, speed (subtitles when the file has them) | compact: play, position, seek, mute |
//!
//! The control bar fades in when the pointer moves or a key is pressed and out
//! 2.5 s later while playing; while paused, hovered or failed it stays. A
//! failure (no decoder, a damaged file, the helper dying) is the file's card
//! with the reason, Open, and Try again for a failure a retry can fix.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bingux_preview_core::Content;
use bingux_preview_core::host::player::Player;
use bingux_preview_core::host::protocol::{ErrorCode, Message};
use freya::engine::prelude::AlphaType;
use freya_core::elements::image::ImageHandle;
use freya::prelude::*;

use super::super::chrome::PreviewNav;
use super::super::host_link::{HostLink, LinkEvent};
use super::super::keys::Keyboard;
use super::super::mode::Mode;
use super::super::subject::Resolved;
use super::card::Card;
use super::media_controls::{ControlAction, ControlState, MediaControls};
use super::media_logic::{BarInputs, HIDE_AFTER_MS, MediaKey, bar_visible, next_subtitle, route};
use crate::component::ComponentName;
use crate::controls::progress::{SpinnerSize, spinner};
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::motion::{MOVE_MS, use_tween};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The smallest and largest frame the player asks for, per side, in pixels.
const MIN_FRAME: u32 = 16;
const MAX_FRAME: u32 = 4096;
/// A change of size smaller than this is not worth a new set of frames.
const RESIZE_SLACK: u32 = 8;

/// What the person set the volume to, kept for the next file: a pane that
/// follows the selection must not reset it, and neither must the next film.
static PREFERENCE: Mutex<(u32, bool)> = Mutex::new((1000, false));

fn remembered_volume() -> (u32, bool) {
    *PREFERENCE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn remember_volume(permille: u32, muted: bool) {
    *PREFERENCE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = (permille, muted);
}

/// The frame size for a box of `logical` pixels at `scale`.
pub(super) fn frame_size(logical: (f32, f32), scale: f64) -> (u32, u32) {
    let side = |length: f32| ((f64::from(length) * scale).round() as u32).clamp(MIN_FRAME, MAX_FRAME);
    (side(logical.0), side(logical.1))
}

/// Whether the player should be asked for a different size, after the last
/// one it was given.
pub(super) fn resize_wanted(last: Option<(u32, u32)>, now: (u32, u32)) -> bool {
    last.is_none_or(|last| last.0.abs_diff(now.0) >= RESIZE_SLACK || last.1.abs_diff(now.1) >= RESIZE_SLACK)
}

/// Whether a failure is one a second try can fix.
pub(super) fn retry_helps(code: ErrorCode) -> bool {
    matches!(code, ErrorCode::Internal | ErrorCode::NotFound)
}

#[derive(PartialEq)]
pub(super) struct MediaView {
    pub resolved: Arc<Resolved>,
    pub nav: PreviewNav,
    pub mode: Mode,
}

impl Component for MediaView {
    fn render(&self) -> impl IntoElement {
        let mut attempt = use_state(|| 0u32);
        Playback {
            resolved: self.resolved.clone(),
            nav: self.nav.clone(),
            mode: self.mode,
            attempt: *attempt.read(),
            on_retry: EventHandler::new(move |_: ()| *attempt.write() += 1),
        }
    }
}

/// One attempt at playing the file: a new attempt is a new helper.
#[derive(PartialEq)]
struct Playback {
    resolved: Arc<Resolved>,
    nav: PreviewNav,
    mode: Mode,
    attempt: u32,
    on_retry: EventHandler<()>,
}

/// A pending hide of the control bar.
type HideTask = Rc<RefCell<Option<TaskHandle>>>;

impl Component for Playback {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(u64::from(self.attempt))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let docked = self.mode == Mode::Pane;
        let path: Option<PathBuf> = self.resolved.source.as_path().map(ToOwned::to_owned);
        let want_video = self.resolved.sniffed.content == Content::Video;

        let link = use_hook(|| Rc::new(HostLink::start()));
        let mut player = use_state(|| {
            let mut player = Player::new();
            let (volume, muted) = remembered_volume();
            player.volume_permille = volume;
            player.muted = muted;
            player
        });
        let mut frame = use_state(|| None::<ImageHandle>);
        let viewport = use_state(|| (0.0f32, 0.0f32));
        let activity = use_state(Instant::now);
        let tick = use_state(|| 0u32);
        let hovering = use_state(|| false);
        let sent_size = use_hook(|| Rc::new(Cell::new(None::<(u32, u32)>)));
        let hide_task: HideTask = use_hook(|| Rc::new(RefCell::new(None)));
        let keyboard = use_try_consume::<Keyboard>().unwrap_or_else(Keyboard::always);

        // What the helper says becomes state, on this thread, one task.
        use_hook({
            let link = link.clone();
            move || {
                spawn(async move {
                    while let Some(event) = link.next().await {
                        match event {
                            LinkEvent::Packet(packet) => {
                                if matches!(packet.message, Message::Position { .. }) {
                                    // A position that changes nothing visible is no redraw.
                                    let mut next = player.peek().clone();
                                    next.apply(&packet);
                                    player.set_if_modified(next);
                                } else {
                                    player.write().apply(&packet);
                                }
                            }
                            LinkEvent::Frame => {
                                if let Some(picture) = link.take_frame() {
                                    if let Some(handle) = ImageHandle::from_rgba(picture.width, picture.height, picture.pixels, AlphaType::Opaque) {
                                        frame.set(Some(handle));
                                    }
                                }
                            }
                            LinkEvent::Page(_) => {}
                            LinkEvent::Died(failure) => player.write().fail(failure.code, &failure.message),
                        }
                    }
                })
            }
        });
        use_drop({
            let link = link.clone();
            move || link.close()
        });

        let act = {
            let link = link.clone();
            move |change: &mut dyn FnMut(&mut Player) -> Vec<bingux_preview_core::host::protocol::Packet>| {
                let mut player = player;
                let packets = change(&mut player.write());
                link.send_all(packets);
            }
        };

        // Wake the render when the bar is due to hide.
        let reveal = {
            let hide_task = hide_task.clone();
            move || {
                let mut activity = activity;
                let mut tick = tick;
                activity.set(Instant::now());
                if let Some(task) = hide_task.borrow_mut().take() {
                    task.try_cancel();
                }
                let task = spawn(async move {
                    timer(Duration::from_millis(HIDE_AFTER_MS + 50)).await;
                    *tick.write() += 1;
                });
                *hide_task.borrow_mut() = Some(task);
            }
        };

        let run = {
            let act = act.clone();
            let reveal = reveal.clone();
            move |key: MediaKey| {
                reveal();
                let act = act.clone();
                match key {
                    MediaKey::TogglePlay => act(&mut |player| player.toggle()),
                    MediaKey::SeekBy(ms) => act(&mut |player| player.seek_by(ms, true)),
                    MediaKey::SeekTo(fraction) => act(&mut |player| player.seek_fraction(fraction, true)),
                    MediaKey::Volume(steps) => {
                        act(&mut |player| player.step_volume(steps));
                        let now = player.peek();
                        remember_volume(now.volume_permille, now.muted);
                    }
                    MediaKey::Mute => {
                        act(&mut |player| player.toggle_mute());
                        let now = player.peek();
                        remember_volume(now.volume_permille, now.muted);
                    }
                    MediaKey::Rate(direction) => act(&mut |player| player.step_rate(direction)),
                    MediaKey::Subtitles => {
                        let (current, count) = {
                            let now = player.peek();
                            (now.subtitle, now.subtitle_tracks.len())
                        };
                        let next = next_subtitle(current, count);
                        act(&mut |player| player.set_subtitle(next));
                    }
                }
            }
        };

        let on_action = {
            let act = act.clone();
            let run = run.clone();
            EventHandler::new(move |action: ControlAction| {
                let act = act.clone();
                match action {
                    ControlAction::Toggle => run(MediaKey::TogglePlay),
                    ControlAction::Seek { fraction, settled } => act(&mut |player| player.seek_fraction(fraction, settled)),
                    ControlAction::Volume(permille) => {
                        act(&mut |player| player.set_volume(permille));
                        let now = player.peek();
                        remember_volume(now.volume_permille, now.muted);
                    }
                    ControlAction::Mute => run(MediaKey::Mute),
                    ControlAction::Rate(rate) => act(&mut |player| player.set_rate(rate)),
                    ControlAction::Subtitle(index) => act(&mut |player| player.set_subtitle(index)),
                }
            })
        };

        let now = player.read().clone();
        let failed = now.failure().cloned();
        let ready = now.is_ready();
        let inputs = BarInputs {
            // A song has no picture to clear for: its bar is its whole interface and stays.
            playing: now.wants_play && !now.ended && now.kind != bingux_preview_core::host::protocol::OpenedKind::Audio,
            hovered: *hovering.read(),
            engaged: false,
            failed: failed.is_some(),
            idle_ms: activity.read().elapsed().as_millis() as u64,
        };
        let _ = *tick.read();
        let wanted = bar_visible(inputs);
        let fade = use_tween(if wanted { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));

        let (vw, vh) = *viewport.read();
        let controls_state = ControlState::of(&now);
        let bar_margin = if docked { S2 } else { S4 };
        let bar = (fade > 0.01 && failed.is_none() && ready).then(|| {
            Element::from(
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().bottom(bar_margin.px()).left(0.0))
                    .width(Size::fill())
                    .opacity(fade)
                    .tw(&tokens, &[flex_row(), justify(Align::Center), px(bar_margin)])
                    // ui-lint: allow(the bar is as wide as the measured viewport less its two margins)
                    .child(MediaControls::new(controls_state, on_action.clone()).docked(docked).width(vw - 2.0 * bar_margin.px()).into_element()),
            )
        });

        // The stage: the picture, or the file's card for a song and while it loads.
        let picture = frame.read().clone();
        let card = Card::of(&self.resolved, &PreviewNav::new(|_| {})).on_backdrop();
        let stage: Element = match (&failed, &picture) {
            (Some(failure), _) => {
                let retry = retry_helps(failure.code).then(|| self.on_retry.clone());
                // The retry is a button of the card: the card is as tall as the stage, so a button under it was cut off.
                rect()
                    .tw(&tokens, &[w_full(), h_full()])
                    .child(Card::of(&self.resolved, &self.nav).on_backdrop().noted(failure.message.clone()).retrying(retry))
                    .into()
            }
            (None, Some(handle)) if now.kind != bingux_preview_core::host::protocol::OpenedKind::Audio => image(handle.clone())
                .width(Size::fill())
                .height(Size::fill())
                .aspect_ratio(AspectRatio::Min)
                .image_cover(ImageCover::Center)
                .sampling_mode(SamplingMode::Bilinear)
                .into(),
            (None, _) => card.into(),
        };
        let waiting = failed.is_none() && (!ready || now.is_buffering() || (picture.is_none() && now.kind == bingux_preview_core::host::protocol::OpenedKind::Video));
        let paused_overlay = failed.is_none() && ready && !now.wants_play && !now.is_buffering();
        let cue = now.cue_now().map(str::to_owned);

        let on_key = {
            let run = run.clone();
            move |event: Event<KeyboardEventData>| {
                // A pane leaves its keys with the list until it is clicked.
                if !keyboard.held() {
                    return;
                }
                if let Some(key) = route(&event.key, event.modifiers, docked) {
                    event.stop_propagation();
                    run(key);
                }
            }
        };

        let first_size = use_hook(|| Rc::new(Cell::new(true)));
        let on_sized = {
            let link = link.clone();
            let sent_size = sent_size.clone();
            move |event: Event<SizedEventData>| {
                let mut viewport = viewport;
                let logical = (event.area.width(), event.area.height());
                if *viewport.peek() != logical {
                    viewport.set(logical);
                }
                if logical.0 < 1.0 || logical.1 < 1.0 {
                    return;
                }
                let scale = *Platform::get().current_window().scale_factor.peek();
                let size = frame_size(logical, scale);
                if first_size.replace(false) {
                    if let Some(path) = &path {
                        link.open_media(path, want_video, size, !docked);
                        let (volume, muted) = remembered_volume();
                        let mut player = player;
                        let mut packets = Vec::new();
                        if volume != 1000 {
                            packets.extend(player.write().set_volume(volume));
                        }
                        if muted {
                            packets.extend(player.write().set_muted(true));
                        }
                        link.send_all(packets);
                    }
                    sent_size.set(Some(size));
                } else if resize_wanted(sent_size.get(), size) {
                    sent_size.set(Some(size));
                    let request = player.write().request();
                    link.send(bingux_preview_core::host::protocol::Packet { request, message: Message::SetSize { max_w: size.0, max_h: size.1 } });
                }
            }
        };

        // Overlays are sized from the measured box, not "fill": an absolute box
        // that asked to fill sat at the top of its parent instead of its middle.
        let spin = waiting.then(|| {
            Element::from(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::px(vw))
                    .height(Size::px(vh))
                    .tw(&tokens, &[flex_col(), center()])
                    .child(spinner(SpinnerSize::Medium).ink(Paint::OnFloating)),
            )
        });
        let center_play = paused_overlay.then(|| {
            let toggle = on_action.clone();
            Element::from(
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::px(vw))
                    .height(Size::px(vh))
                    .tw(&tokens, &[flex_col(), center()])
                    .child(
                        rect()
                            .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Floating), shadow(Elevation::Floating), p(S2)])
                            .child(icon_button(Icon::MediaPlaybackStart, move |_| toggle.call(ControlAction::Toggle)).ink(Ink::OnFloating).tooltip("Play")),
                    ),
            )
        });
        let subtitle = cue.map(|text| {
            Element::from(
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().bottom(if wanted { 64.0 } else { 24.0 }).left(0.0))
                    .width(Size::fill())
                    .tw(&tokens, &[flex_row(), justify(Align::Center), px(S6)])
                    .child(
                        rect()
                            .tw(&tokens, &[rounded(Radius::Control), bg(Paint::Floating), px(S3), py(S1)])
                            .child(CAPTION.in_ink(Ink::OnFloating).paragraph(&tokens, text).text_align(TextAlign::Center)),
                    ),
            )
        });

        let stage_press = {
            let on_action = on_action.clone();
            move |_: Event<PressEventData>| {
                keyboard.take();
                on_action.call(ControlAction::Toggle);
            }
        };

        rect()
            .component("PreviewPlayer")
            .tw(&tokens, &[w_full(), h_full(), clip(), bg(Paint::Backdrop)])
            .on_sized(on_sized)
            .on_global_key_down(on_key)
            .on_pointer_move({
                let reveal = reveal.clone();
                move |_| reveal()
            })
            .on_pointer_enter({
                let reveal = reveal.clone();
                move |_| reveal()
            })
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_full()])
                    .on_press(stage_press)
                    .child(stage),
            )
            .maybe_child(spin)
            .maybe_child(subtitle)
            .maybe_child(center_play)
            .maybe_child(bar.map(|bar| {
                let mut hovering = hovering;
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().bottom(0.0).left(0.0))
                    .width(Size::fill())
                    .on_pointer_enter(move |_| hovering.set_if_modified(true))
                    .on_pointer_leave(move |_| hovering.set_if_modified(false))
                    .child(bar)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_asked_for_in_device_pixels_within_limits() {
        assert_eq!(frame_size((800.0, 600.0), 1.0), (800, 600));
        assert_eq!(frame_size((800.0, 600.0), 2.0), (1600, 1200));
        assert_eq!(frame_size((800.0, 600.0), 1.25), (1000, 750));
        assert_eq!(frame_size((0.0, 3.0), 1.0), (MIN_FRAME, MIN_FRAME));
        assert_eq!(frame_size((20_000.0, 20_000.0), 2.0), (MAX_FRAME, MAX_FRAME));
    }

    #[test]
    fn a_resize_is_sent_only_when_it_matters() {
        assert!(resize_wanted(None, (400, 300)));
        assert!(!resize_wanted(Some((400, 300)), (404, 302)));
        assert!(resize_wanted(Some((400, 300)), (420, 300)));
        assert!(resize_wanted(Some((400, 300)), (400, 280)));
    }

    #[test]
    fn only_failures_a_second_try_can_fix_offer_it() {
        assert!(retry_helps(ErrorCode::Internal));
        assert!(retry_helps(ErrorCode::NotFound));
        assert!(!retry_helps(ErrorCode::NoDecoder));
        assert!(!retry_helps(ErrorCode::Corrupt));
        assert!(!retry_helps(ErrorCode::Unsupported));
    }

    #[test]
    fn the_volume_is_remembered_for_the_next_file() {
        remember_volume(420, true);
        assert_eq!(remembered_volume(), (420, true));
        remember_volume(1000, false);
    }
}
