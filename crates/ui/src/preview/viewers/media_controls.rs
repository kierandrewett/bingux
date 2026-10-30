//! The player's control bar: play and pause, the seek bar with what is
//! buffered, the time, the volume, and the subtitle and speed menus. Full
//! screen is the F key, not a button.
//!
//! [`MediaControls`] draws a [`ControlState`] and reports what the person did
//! as a [`ControlAction`]; it has no host and no player, so the taxonomy shows
//! every state of it and a test drives it. The pill floats over the picture
//! (`Paint::Floating`, ink `OnFloating`) like the picture viewer's zoom bar.
//!
//! The compact form is the docked pane's and the phone's: play, the position,
//! the seek bar and the mute button. Everything else is a menu away in the
//! window's popup.

use bingux_preview_core::host::player::{Player, RATES, clock, rate_label};
use freya::prelude::*;

use super::media_logic::{compact_bar, fraction_at, volume_at, volume_level};
use crate::component::ComponentName;
use crate::controls::capture::{CaptureEvent, Phase, use_capture};
use crate::controls::icon_button;
use crate::controls::path_label::measure_text;
use crate::icons::Icon;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::{MenuItem, Overlays, menu_item};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink};

/// The height of the seek bar's hit area, in logical pixels.
const BAR_HIT: f32 = 24.0;
/// The track, and the track while the pointer is on it.
const TRACK: f32 = 4.0;
const TRACK_ACTIVE: f32 = 6.0;
/// The knob of a bar that is hovered or dragged.
const KNOB: f32 = 12.0;
/// The volume bar's width.
const VOLUME_WIDTH: f32 = 72.0;
/// The widest time the clock labels have to hold, measured once.
const WIDEST_TIME: &str = "0:00:00";

/// What the bar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlState {
    pub playing: bool,
    /// The host is waiting for data.
    pub buffering: bool,
    pub ended: bool,
    pub position_ns: u64,
    pub duration_ns: u64,
    /// How much of the length is buffered, 0 to 1.
    pub buffered: f32,
    /// Where the thumb sits, 0 to 1.
    pub progress: f32,
    /// Volume in thousandths.
    pub volume: u32,
    pub muted: bool,
    /// Speed in thousandths.
    pub rate: u32,
    pub has_audio: bool,
    pub seekable: bool,
    pub subtitles: Vec<String>,
    /// The subtitle track shown, or -1.
    pub subtitle: i32,
}

impl Default for ControlState {
    fn default() -> Self {
        ControlState {
            playing: false,
            buffering: false,
            ended: false,
            position_ns: 0,
            duration_ns: 0,
            buffered: 0.0,
            progress: 0.0,
            volume: 1000,
            muted: false,
            rate: 1000,
            has_audio: true,
            seekable: true,
            subtitles: Vec::new(),
            subtitle: -1,
        }
    }
}

impl ControlState {
    /// What the bar shows for `player`.
    pub fn of(player: &Player) -> ControlState {
        ControlState {
            playing: player.wants_play && !player.ended,
            buffering: player.is_buffering(),
            ended: player.ended,
            position_ns: player.shown_position(),
            duration_ns: player.duration_ns,
            buffered: player.buffered(),
            progress: player.progress(),
            volume: player.volume_permille,
            muted: player.muted,
            rate: player.rate_permille,
            has_audio: player.has_audio,
            seekable: player.seekable,
            subtitles: player.subtitle_tracks.clone(),
            subtitle: player.subtitle,
        }
    }
}

/// What a person did on the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlAction {
    /// Play, or pause.
    Toggle,
    /// The seek bar moved to this fraction. `settled` is false while a drag is
    /// on (a fast keyframe seek is enough) and true when it ends (the exact one).
    Seek { fraction: f32, settled: bool },
    Volume(u32),
    Mute,
    /// A speed in thousandths.
    Rate(u32),
    /// A subtitle track, or -1 for none.
    Subtitle(i32),
}

/// The control bar. `docked` and a narrow `width` give the compact form.
#[derive(Clone, PartialEq)]
pub struct MediaControls {
    state: ControlState,
    docked: bool,
    width: f32,
    on_action: EventHandler<ControlAction>,
}

impl MediaControls {
    pub fn new(state: ControlState, on_action: impl Into<EventHandler<ControlAction>>) -> Self {
        MediaControls { state, docked: false, width: 1000.0, on_action: on_action.into() }
    }

    /// The bar of a docked pane: the compact form whatever the width.
    pub fn docked(mut self, docked: bool) -> Self {
        self.docked = docked;
        self
    }

    /// The room the bar has, so it picks its form.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

/// One of the two bars: a track with a fill, what is buffered, and a knob.
#[derive(PartialEq)]
struct TrackBar {
    fraction: f32,
    buffered: Option<f32>,
    enabled: bool,
    /// What a screen reader calls it.
    name: &'static str,
    /// The text of its value for a screen reader.
    value_text: String,
    /// The fraction the pointer chose, and whether the drag has ended.
    on_change: EventHandler<(f32, bool)>,
}

impl Component for TrackBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut width = use_state(|| 0.0f32);
        let on_change = self.on_change.clone();
        let capture = use_capture(move |event: CaptureEvent| {
            let at = fraction_at(event.local.0, *width.peek());
            match event.phase {
                Phase::Start => {
                    dragging.set(true);
                    on_change.call((at, false));
                }
                Phase::Move => on_change.call((at, false)),
                Phase::End => {
                    dragging.set_if_modified(false);
                    on_change.call((at, true));
                }
                Phase::Cancel => dragging.set_if_modified(false),
            }
        });
        let tracker = capture.clone();
        let raised = hovering() || dragging();
        let track = if raised { TRACK_ACTIVE } else { TRACK };
        let measured = width();
        let fraction = self.fraction.clamp(0.0, 1.0);
        let played = measured * fraction;
        let buffered = measured * self.buffered.unwrap_or(0.0).clamp(0.0, 1.0);
        let centre = BAR_HIT / 2.0;
        let value = f64::from(fraction);
        let text = self.value_text.clone();
        let element = rect()
            .component("TrackBar")
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt(self.name.to_owned())
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(1.0);
                builder.set_value(text.clone());
            })
            .width(Size::flex(1.0))
            .min_width(Size::px(0.0))
            .height(Size::px(BAR_HIT))
            .maybe(!self.enabled, |element| element.opacity(0.5))
            .on_sized(move |event: Event<SizedEventData>| {
                let size = event.area.width();
                if (*width.peek() - size).abs() > 0.01 {
                    width.set(size);
                }
                tracker.sized(&event);
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .child(
                rect()
                    .position(Position::new_absolute().top(centre - track / 2.0).left(0.0))
                    .width(Size::px(measured))
                    .height(Size::px(track))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::FloatingOutline)]),
            )
            .maybe_child((self.buffered.is_some() && buffered > 0.0).then(|| {
                rect()
                    .position(Position::new_absolute().top(centre - track / 2.0).left(0.0))
                    .width(Size::px(buffered))
                    .height(Size::px(track))
                    .opacity(0.4)
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::OnFloating)])
            }))
            .child(
                rect()
                    .position(Position::new_absolute().top(centre - track / 2.0).left(0.0))
                    .width(Size::px(played))
                    .height(Size::px(track))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::OnFloating)]),
            )
            .maybe_child(raised.then(|| {
                rect()
                    .position(Position::new_absolute().top(centre - KNOB / 2.0).left((played - KNOB / 2.0).clamp(0.0, (measured - KNOB).max(0.0))))
                    .width(Size::px(KNOB))
                    .height(Size::px(KNOB))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Knob), shadow(Elevation::Floating)])
            }));
        if self.enabled { capture.handlers(element) } else { element }
    }
}

/// An icon button that opens `items` below (above, when there is no room) on a press.
#[derive(PartialEq)]
struct MenuIcon {
    icon: Icon,
    tooltip: &'static str,
    items: Vec<MenuItem>,
    enabled: bool,
}

impl Component for MenuIcon {
    fn render(&self) -> impl IntoElement {
        let overlays = use_try_consume::<Overlays>();
        let id = use_a11y();
        let mut area = use_state(Area::default);
        let items = self.items.clone();
        rect()
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(
                icon_button(self.icon, move |_| {
                    if let Some(overlays) = overlays {
                        overlays.show_menu_from(Some(id), PanelTrigger::Control(PanelRect::from_area(&area.read())), items.clone(), true);
                    }
                })
                .ink(Ink::OnFloating)
                .a11y_id(id)
                .tooltip(self.tooltip)
                .enabled(self.enabled),
            )
    }
}

/// The speed menu's rows.
fn rate_items(current: u32, on_action: &EventHandler<ControlAction>) -> Vec<MenuItem> {
    RATES
        .iter()
        .map(|rate| {
            let on_action = on_action.clone();
            let rate = *rate;
            menu_item(rate_label(rate), move |_| on_action.call(ControlAction::Rate(rate))).checked(rate == current)
        })
        .collect()
}

/// The subtitle menu's rows: Off, then a row per track.
fn subtitle_items(tracks: &[String], current: i32, on_action: &EventHandler<ControlAction>) -> Vec<MenuItem> {
    let off = {
        let on_action = on_action.clone();
        menu_item("Off", move |_| on_action.call(ControlAction::Subtitle(-1))).checked(current < 0)
    };
    std::iter::once(off)
        .chain(tracks.iter().enumerate().map(|(index, name)| {
            let on_action = on_action.clone();
            menu_item(name.clone(), move |_| on_action.call(ControlAction::Subtitle(index as i32))).checked(current == index as i32)
        }))
        .collect()
}

/// The icon of the volume button for a level.
fn volume_icon(permille: u32, muted: bool) -> Icon {
    match volume_level(permille, muted) {
        0 => Icon::AudioVolumeMuted,
        1 => Icon::AudioVolumeLow,
        2 => Icon::AudioVolumeMedium,
        _ => Icon::Sound,
    }
}

/// A clock label of a fixed width, so the bar does not move as digits change.
fn time_label(tokens: &Tokens, text: String, width: f32, align: Align) -> Element {
    rect()
        .width(Size::px(width))
        .tw(tokens, &[flex_row(), justify(align)])
        .child(CAPTION.in_ink(Ink::OnFloating).label(tokens, text).max_lines(1))
        .into_element()
}

impl Component for MediaControls {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let state = &self.state;
        // The form follows the room the bar really has once it has been laid out,
        // so a caller that gave no width (a phone's sheet) still gets the compact
        // form and a seek bar that has room to be one.
        let mut room = use_state(|| self.width);
        let compact = compact_bar(self.width.min(*room.read()), self.docked);
        let on_action = self.on_action.clone();
        let time_width = measure_text(WIDEST_TIME, CAPTION) + 4.0;

        let toggle = {
            let on_action = on_action.clone();
            let (icon, tip) = if state.ended {
                (Icon::MediaPlaybackStart, "Play again (K)")
            } else if state.playing {
                (Icon::Pause, "Pause (K)")
            } else {
                (Icon::MediaPlaybackStart, "Play (K)")
            };
            icon_button(icon, move |_| on_action.call(ControlAction::Toggle)).ink(Ink::OnFloating).tooltip(tip)
        };
        let position = clock(state.position_ns);
        let duration = clock(state.duration_ns);
        let seek = TrackBar {
            fraction: state.progress,
            buffered: Some(state.buffered),
            enabled: state.seekable && state.duration_ns > 0,
            name: "Seek",
            value_text: format!("{position} of {duration}"),
            on_change: {
                let on_action = on_action.clone();
                EventHandler::new(move |(fraction, settled): (f32, bool)| on_action.call(ControlAction::Seek { fraction, settled }))
            },
        };
        let mute = {
            let on_action = on_action.clone();
            icon_button(volume_icon(state.volume, state.muted), move |_| on_action.call(ControlAction::Mute))
                .ink(Ink::OnFloating)
                .tooltip(if state.muted { "Unmute (M)" } else { "Mute (M)" })
                .enabled(state.has_audio)
        };

        // A player is a physical object and its bar is a tape: play, position and
        // volume keep their places in every language (design/rtl.md).
        let mut bar = rect()
            .text_direction(TextDirection::Ltr)
            .component("MediaControls")
            .tw(&tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S1), p(S1), rounded(Radius::Pill), bg(Paint::Floating), shadow(Elevation::Floating)])
            .width(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| room.set_if_modified(event.area.width()))
            .child(toggle);
        if compact {
            // Play, position, the bar, mute: all a pane has room for.
            bar = bar.child(time_label(&tokens, position, time_width, Align::End)).child(seek).child(mute);
            return bar.into_element();
        }
        let volume = TrackBar {
            fraction: if state.muted { 0.0 } else { state.volume as f32 / 1000.0 },
            buffered: None,
            enabled: state.has_audio,
            name: "Volume",
            value_text: format!("{}%", state.volume / 10),
            on_change: {
                let on_action = on_action.clone();
                EventHandler::new(move |(fraction, _): (f32, bool)| on_action.call(ControlAction::Volume(volume_at(fraction, 1.0))))
            },
        };
        bar = bar
            .child(time_label(&tokens, position, time_width, Align::End))
            .child(seek)
            .child(time_label(&tokens, duration, time_width, Align::Start))
            .child(mute)
            .child(rect().width(Size::px(VOLUME_WIDTH)).child(volume))
            .maybe_child((!state.subtitles.is_empty()).then(|| {
                Element::from(MenuIcon {
                    icon: Icon::MediaViewSubtitles,
                    tooltip: "Subtitles (C)",
                    items: subtitle_items(&state.subtitles, state.subtitle, &on_action),
                    enabled: true,
                })
            }))
            .child(MenuIcon { icon: Icon::ViewMore, tooltip: "Playback speed ([ and ])", items: rate_items(state.rate, &on_action), enabled: true });
        bar.into_element()
    }
}
