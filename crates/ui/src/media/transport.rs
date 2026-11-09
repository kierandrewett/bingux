//! Play, pause, previous and next, with shuffle and repeat: [`transport`].
//!
//! One row of buttons in the order every player has them: shuffle, previous,
//! play or pause (larger, filled), next, repeat. Shuffle and repeat appear
//! only when the caller gives their state, and are lit (`active`) while on;
//! repeat's glyph shows which repeat. `skips()` puts back 10 s and forward
//! 10 s either side of play, for a film or a long recording.
//!
//! The row is a tape: it keeps its left-to-right order in a right-to-left
//! interface (design/rtl.md), as the preview player's bar does.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::icon_button;
use crate::controls::tooltip::TipSide;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::Ink;

/// What repeats when the queue runs out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// A button of the row was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportAction {
    PlayPause,
    Previous,
    Next,
    /// Back 10 seconds.
    SkipBack,
    /// Forward 10 seconds.
    SkipForward,
    Shuffle,
    /// The next repeat mode: off, all, one.
    Repeat,
}

/// The transport row. Build with [`transport`].
#[derive(PartialEq)]
pub struct Transport {
    playing: bool,
    on_action: EventHandler<TransportAction>,
    shuffle: Option<bool>,
    repeat: Option<RepeatMode>,
    skips: bool,
    can_previous: bool,
    can_next: bool,
    enabled: bool,
    floating: bool,
    compact: bool,
}

/// The row for a player that is `playing` (or paused).
pub fn transport(playing: bool, on_action: impl Into<EventHandler<TransportAction>>) -> Transport {
    Transport {
        playing,
        on_action: on_action.into(),
        shuffle: None,
        repeat: None,
        skips: false,
        can_previous: true,
        can_next: true,
        enabled: true,
        floating: false,
        compact: false,
    }
}

impl Transport {
    /// Show the shuffle button, lit while `on`.
    pub fn shuffle(mut self, on: bool) -> Self {
        self.shuffle = Some(on);
        self
    }

    /// Show the repeat button in `mode`.
    pub fn repeat(mut self, mode: RepeatMode) -> Self {
        self.repeat = Some(mode);
        self
    }

    /// Back and forward 10 seconds beside play.
    pub fn skips(mut self) -> Self {
        self.skips = true;
        self
    }

    /// Whether there is an item after the current one.
    pub fn can_next(mut self, can: bool) -> Self {
        self.can_next = can;
        self
    }

    /// Whether Previous does anything (an item before, or a start to go back to).
    pub fn can_previous(mut self, can: bool) -> Self {
        self.can_previous = can;
        self
    }

    /// Nothing to play: every button dimmed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// On a floating bar over a picture: light glyphs.
    pub fn floating(mut self) -> Self {
        self.floating = true;
        self
    }

    /// Play and next only, for a narrow bar.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

impl Component for Transport {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ink = if self.floating {
            Ink::OnFloating
        } else {
            Ink::Text
        };
        let action = |which: TransportAction| {
            let on_action = self.on_action.clone();
            move |_: ()| on_action.call(which)
        };
        let button = |glyph: Icon, which: TransportAction, tip: &str, enabled: bool| -> Element {
            icon_button(glyph, action(which))
                .ink(ink)
                .tooltip_side(TipSide::Above)
                .tooltip(tip)
                .enabled(self.enabled && enabled)
                .into()
        };
        let play_tip = if self.playing {
            crate::tr!("ui-media-pause-tooltip")
        } else {
            crate::tr!("ui-media-play-tooltip")
        };
        let play = icon_button(Icon::MediaPlaybackStart, action(TransportAction::PlayPause))
            .morph(Icon::Pause, self.playing)
            .size(Dim::Box11)
            .radius(Radius::Pill)
            .filled()
            .ink(ink)
            .tooltip_side(TipSide::Above)
            .tooltip(&play_tip)
            .enabled(self.enabled);

        let mut row = rect()
            .component("Transport")
            .text_direction(TextDirection::Ltr)
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(if self.compact { S1 } else { S2 }),
                ],
            );
        if self.compact {
            return row
                .child(play)
                .child(button(
                    Icon::MediaSkipForward,
                    TransportAction::Next,
                    &crate::tr!("ui-media-next"),
                    self.can_next,
                ))
                .into_element();
        }
        if let Some(on) = self.shuffle {
            row = row.child(
                icon_button(Icon::MediaPlaylistShuffle, action(TransportAction::Shuffle))
                    .ink(ink)
                    .active(on)
                    .tooltip_side(TipSide::Above)
                    .tooltip(&if on {
                        crate::tr!("ui-media-shuffle-on")
                    } else {
                        crate::tr!("ui-media-shuffle")
                    })
                    .enabled(self.enabled),
            );
        }
        row = row.child(button(
            Icon::MediaSkipBackward,
            TransportAction::Previous,
            &crate::tr!("ui-media-previous"),
            self.can_previous,
        ));
        if self.skips {
            row = row.child(button(
                Icon::MediaSeekBackward,
                TransportAction::SkipBack,
                &crate::tr!("ui-media-skip-back"),
                true,
            ));
        }
        row = row.child(play);
        if self.skips {
            row = row.child(button(
                Icon::MediaSeekForward,
                TransportAction::SkipForward,
                &crate::tr!("ui-media-skip-forward"),
                true,
            ));
        }
        row = row.child(button(
            Icon::MediaSkipForward,
            TransportAction::Next,
            &crate::tr!("ui-media-next"),
            self.can_next,
        ));
        if let Some(mode) = self.repeat {
            let (glyph, tip) = match mode {
                RepeatMode::Off => (Icon::MediaPlaylistRepeat, crate::tr!("ui-media-repeat")),
                RepeatMode::All => (Icon::MediaPlaylistRepeat, crate::tr!("ui-media-repeat-all")),
                RepeatMode::One => (
                    Icon::MediaPlaylistRepeatSong,
                    crate::tr!("ui-media-repeat-one"),
                ),
            };
            row = row.child(
                icon_button(glyph, action(TransportAction::Repeat))
                    .ink(ink)
                    .active(mode != RepeatMode::Off)
                    .tooltip_side(TipSide::Above)
                    .tooltip(&tip)
                    .enabled(self.enabled),
            );
        }
        row.into_element()
    }
}
