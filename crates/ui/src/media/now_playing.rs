//! What is playing, along the bottom of a window: [`now_playing_bar`].
//!
//! On a desktop window the bar has three parts: the artwork, title and
//! artist on the left (a press opens the full player), the transport over
//! the timeline in the middle, and the volume with the caller's own buttons
//! (the queue, a mini player) on the right. On a phone it is one row, the
//! artwork, the title and artist, play and next, under a thin line that
//! shows how far through the item is, as the mini player of a phone's music
//! app; a press on it opens the full player.

use std::path::PathBuf;

use freya::prelude::*;

use super::artwork::{ArtworkKind, artwork};
use super::timeline::{Seek, timeline};
use super::transport::{RepeatMode, TransportAction, transport};
use super::volume::{VolumeChange, volume_control};
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, EMPHASIS, Ink};

/// The bar. Build with [`now_playing_bar`].
#[derive(PartialEq)]
pub struct NowPlayingBar {
    title: String,
    subtitle: String,
    artwork: Option<PathBuf>,
    picture: Option<Element>,
    seed: String,
    kind: ArtworkKind,
    playing: bool,
    position: f64,
    duration: Option<f64>,
    buffered: Vec<(f64, f64)>,
    chapters: Vec<(f64, String)>,
    shuffle: Option<bool>,
    repeat: Option<RepeatMode>,
    can_next: bool,
    can_previous: bool,
    volume: Option<(f64, bool, EventHandler<VolumeChange>)>,
    on_transport: EventHandler<TransportAction>,
    on_seek: EventHandler<Seek>,
    on_open: Option<EventHandler<()>>,
    trailing: Vec<Element>,
}

/// A bar for the item titled `title`, by `subtitle` (the artist, or the
/// album and artist).
pub fn now_playing_bar(title: impl Into<String>, subtitle: impl Into<String>) -> NowPlayingBar {
    NowPlayingBar {
        title: title.into(),
        subtitle: subtitle.into(),
        artwork: None,
        picture: None,
        seed: String::new(),
        kind: ArtworkKind::Song,
        playing: false,
        position: 0.0,
        duration: None,
        buffered: Vec::new(),
        chapters: Vec::new(),
        shuffle: None,
        repeat: None,
        can_next: true,
        can_previous: true,
        volume: None,
        on_transport: EventHandler::new(|_: TransportAction| {}),
        on_seek: EventHandler::new(|_: Seek| {}),
        on_open: None,
        trailing: Vec::new(),
    }
}

impl NowPlayingBar {
    /// The picture: a file, the name its fallback is made from, and what it is.
    pub fn artwork(
        mut self,
        source: Option<PathBuf>,
        seed: impl Into<String>,
        kind: ArtworkKind,
    ) -> Self {
        self.artwork = source;
        self.seed = seed.into();
        self.kind = kind;
        self
    }

    /// A live picture in place of the artwork: a film that plays on while
    /// its full player is closed shows in the bar.
    pub fn picture(mut self, picture: Element) -> Self {
        self.picture = Some(picture);
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    /// Seconds played of `duration`.
    pub fn progress(mut self, position: f64, duration: Option<f64>) -> Self {
        self.position = position;
        self.duration = duration;
        self
    }

    pub fn buffered(mut self, ranges: Vec<(f64, f64)>) -> Self {
        self.buffered = ranges;
        self
    }

    pub fn chapters(mut self, chapters: Vec<(f64, String)>) -> Self {
        self.chapters = chapters;
        self
    }

    pub fn shuffle(mut self, on: bool) -> Self {
        self.shuffle = Some(on);
        self
    }

    pub fn repeat(mut self, mode: RepeatMode) -> Self {
        self.repeat = Some(mode);
        self
    }

    pub fn can_next(mut self, can: bool) -> Self {
        self.can_next = can;
        self
    }

    pub fn can_previous(mut self, can: bool) -> Self {
        self.can_previous = can;
        self
    }

    /// The volume control, at `level` (0 to 100).
    pub fn volume(
        mut self,
        level: f64,
        muted: bool,
        on_change: impl Into<EventHandler<VolumeChange>>,
    ) -> Self {
        self.volume = Some((level, muted, on_change.into()));
        self
    }

    pub fn on_transport(mut self, on_transport: impl Into<EventHandler<TransportAction>>) -> Self {
        self.on_transport = on_transport.into();
        self
    }

    pub fn on_seek(mut self, on_seek: impl Into<EventHandler<Seek>>) -> Self {
        self.on_seek = on_seek.into();
        self
    }

    /// A press on the artwork and title (the whole bar on a phone): open the
    /// full player.
    pub fn on_open(mut self, on_open: impl Into<EventHandler<()>>) -> Self {
        self.on_open = Some(on_open.into());
        self
    }

    /// A button of the app's own at the right end (a queue, a mini player).
    pub fn trailing(mut self, element: impl Into<Element>) -> Self {
        self.trailing.push(element.into());
        self
    }
}

impl Component for NowPlayingBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let open = self.on_open.clone();
        let identity = |art: Dim| {
            let open = open.clone();
            rect()
                .component("NowPlayingItem")
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!(
                    "{}, {}. Open the player",
                    self.title, self.subtitle
                ))
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S3), min_w(Dim::Box5)],
                )
                .width(Size::flex(1.0))
                .maybe(open.is_some(), |element| {
                    element.on_press(move |_| {
                        if let Some(open) = &open {
                            open.call(());
                        }
                    })
                })
                .child(match &self.picture {
                    Some(picture) => rect()
                        .tw(&tokens, &[w(art), h(art), rounded(Radius::Control), clip()])
                        .child(picture.clone())
                        .into_element(),
                    None => artwork(self.artwork.clone(), self.seed.clone())
                        .kind(self.kind)
                        .size(art)
                        .into_element(),
                })
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .tw(&tokens, &[flex_col(), gap(S0_5)])
                        .child(
                            EMPHASIS
                                .label(&tokens, self.title.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        )
                        .maybe_child((!self.subtitle.is_empty()).then(|| {
                            CAPTION
                                .in_ink(Ink::Muted)
                                .label(&tokens, self.subtitle.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis)
                        })),
                )
        };
        let mut controls = transport(self.playing, self.on_transport.clone())
            .can_next(self.can_next)
            .can_previous(self.can_previous);

        if compact {
            // A thin line along the top shows how far through the item is.
            let fraction = match self.duration {
                Some(duration) if duration > 0.0 => {
                    (self.position / duration).clamp(0.0, 1.0) as f32
                }
                _ => 0.0,
            };
            return rect()
                .component("NowPlayingBar")
                .width(Size::fill())
                .tw(
                    &tokens,
                    &[flex_col(), bg(Paint::Surface), border(Line::HairlineTop)],
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .tw(&tokens, &[h(Dim::SliderTrack), bg(Paint::Outline)])
                        .child(
                            rect()
                                .width(Size::percent(fraction * 100.0))
                                .height(Size::fill())
                                .tw(&tokens, &[bg(Paint::Accent)]),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .tw(
                            &tokens,
                            &[flex_row(), items(Align::Center), gap(S2), px(S3), py(S2)],
                        )
                        .child(identity(Dim::Box11))
                        .child(
                            transport(self.playing, self.on_transport.clone())
                                .compact()
                                .can_next(self.can_next),
                        ),
                )
                .into_element();
        }

        if let Some(on) = self.shuffle {
            controls = controls.shuffle(on);
        }
        if let Some(mode) = self.repeat {
            controls = controls.repeat(mode);
        }
        let middle = rect()
            .tw(
                &tokens,
                &[flex_col(), items(Align::Center), max_w(Dim::Page)],
            )
            .width(Size::flex(1.4))
            .child(controls)
            .child(
                timeline(self.position, self.duration, self.on_seek.clone())
                    .buffered(self.buffered.clone())
                    .chapters(self.chapters.clone())
                    .on_surface(Paint::Surface)
                    .times(),
            );
        let mut right = rect().width(Size::flex(1.0)).tw(
            &tokens,
            &[
                flex_row(),
                items(Align::Center),
                justify(Align::End),
                gap(S1),
            ],
        );
        if let Some((level, muted, on_change)) = &self.volume {
            right = right.child(volume_control(*level, *muted, on_change.clone()));
        }
        for element in &self.trailing {
            right = right.child(element.clone());
        }
        rect()
            .component("NowPlayingBar")
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S4),
                    px(S4),
                    py(S2),
                    bg(Paint::Surface),
                    border(Line::HairlineTop),
                ],
            )
            .child(identity(Dim::Box11))
            .child(middle)
            .child(right)
            .into_element()
    }
}
