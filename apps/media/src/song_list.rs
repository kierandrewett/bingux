//! A list of songs as a music library shows it: on a desktop, aligned
//! columns (artwork or track number, title, artist, album, time) under a
//! header; on a phone, two-line rows with the artwork. The song playing is
//! lit and its title is in the accent. A press plays the list from that song;
//! Enter does the same with the keyboard; each row's More button has Play
//! next, Play later, Go to album and Go to artist.
//!
//! Kept in the app until bingux-ui's `data_table` lands; then the columns
//! move to it.

use std::path::PathBuf;
use std::sync::Arc;

use bingux_media::library::Track;
use bingux_ui::media::{ArtworkKind, artwork, format_time};
use bingux_ui::motion::Focusable;
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, CAPTION};
use freya::prelude::*;

use crate::engine::engine;
use crate::pages::{Ui, location, play, use_ui};
use crate::strings as text;

/// What the first column shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lead {
    Artwork,
    /// The track number: an album's own list.
    Number,
}

#[derive(PartialEq)]
pub struct SongList {
    pub songs: Arc<Vec<Track>>,
    pub current: Option<PathBuf>,
    pub lead: Lead,
    /// Leave the album column out (an album's own list).
    pub album: bool,
}

fn more_menu(track: &Track, router: Router) -> Vec<MenuItem> {
    let next = track.item();
    let later = track.item();
    let album = location(&["albums", &track.album_key()]);
    let artist = location(&["artists", track.filed_artist()]);
    vec![
        menu_item(text::PLAY_NEXT, move |_| {
            engine().player.enqueue(vec![next.clone()], true)
        }),
        menu_item(text::PLAY_LATER, move |_| {
            engine().player.enqueue(vec![later.clone()], false)
        }),
        menu_separator(),
        menu_item(text::SHOW_ALBUM, move |_| router.push(&album)),
        menu_item(text::SHOW_ARTIST, move |_| router.push(&artist)),
    ]
}

impl Component for SongList {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let header = (!compact).then(|| {
            let heading = |label: &str| {
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, label.to_owned())
                    .max_lines(1)
            };
            rect()
                .width(Size::fill())
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S3),
                        px(S2),
                        pb(S1),
                        border(Line::HairlineBottom),
                    ],
                )
                .child(rect().tw(&tokens, &[w(Dim::Box9)]))
                .child(
                    rect()
                        .width(Size::flex(2.0))
                        .child(heading(text::COLUMN_TITLE)),
                )
                .child(
                    rect()
                        .width(Size::flex(1.2))
                        .child(heading(text::COLUMN_ARTIST)),
                )
                .maybe_child(self.album.then(|| {
                    rect()
                        .width(Size::flex(1.2))
                        .child(heading(text::COLUMN_ALBUM))
                }))
                .child(
                    rect()
                        .tw(&tokens, &[w(Dim::Box11), flex_row(), justify(Align::End)])
                        .child(heading(text::COLUMN_TIME)),
                )
                .child(rect().tw(&tokens, &[w(Dim::Button)]))
        });
        let mut list = rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col()])
            .maybe_child(header);
        for (index, track) in self.songs.iter().enumerate() {
            list = list.child(
                SongRow {
                    songs: self.songs.clone(),
                    index,
                    playing: self.current.as_ref() == Some(&track.path),
                    lead: self.lead,
                    album: self.album,
                }
                .into_element(),
            );
        }
        list
    }
}

#[derive(PartialEq)]
struct SongRow {
    songs: Arc<Vec<Track>>,
    index: usize,
    playing: bool,
    lead: Lead,
    album: bool,
}

impl Component for SongRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ui: Ui = use_ui();
        let router = use_router();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let compact = is_compact();
        let track = &self.songs[self.index];
        let songs = self.songs.clone();
        let index = self.index;
        let start = move || play(songs.iter().map(Track::item).collect(), index, ui);
        let title_ink = if self.playing { Ink::Accent } else { Ink::Text };
        let lead: Element = match self.lead {
            Lead::Artwork => artwork(track.artwork.clone(), track.album.clone())
                .kind(ArtworkKind::Song)
                .size(Dim::Box9)
                .into_element(),
            Lead::Number => rect()
                .tw(&tokens, &[w(Dim::Box9), flex_row(), justify(Align::End)])
                .child(
                    CAPTION
                        .in_ink(if self.playing {
                            Ink::Accent
                        } else {
                            Ink::Muted
                        })
                        .label(
                            &tokens,
                            track
                                .track
                                .map(|number| number.to_string())
                                .unwrap_or_default(),
                        ),
                )
                .into_element(),
        };
        let line = |style: bingux_ui::typography::TextStyle, ink: Ink, value: String| {
            style
                .in_ink(ink)
                .label(&tokens, value)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
        };
        let duration = track.duration.map(format_time).unwrap_or_default();
        let more = menu_button(text::MORE, more_menu(track, router)).icon(Icon::ViewMore);
        let spoken = format!("{}, {}, {}", track.title, track.artist, duration);
        let row = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Row)
            .a11y_alt(spoken)
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S3),
                    px(S2),
                    py(S1),
                    rounded(Radius::Control),
                ],
            )
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering(),
                    pressing(),
                    self.playing,
                )],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press({
                let start = start.clone();
                move |_| start()
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if matches!(event.key, Key::Named(NamedKey::Enter)) {
                    event.stop_propagation();
                    start();
                }
            })
            .child(lead);
        let row = if compact {
            let second = match (
                track.artist.is_empty(),
                track.album.is_empty() || !self.album,
            ) {
                (false, false) => format!("{}, {}", track.artist, track.album),
                (false, true) => track.artist.clone(),
                _ => track.album.clone(),
            };
            row.child(
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[flex_col()])
                    .child(line(BODY, title_ink, track.title.clone()))
                    .maybe_child((!second.is_empty()).then(|| line(CAPTION, Ink::Muted, second))),
            )
            .child(line(CAPTION, Ink::Muted, duration))
            .child(more)
        } else {
            row.child(rect().width(Size::flex(2.0)).child(line(
                BODY,
                title_ink,
                track.title.clone(),
            )))
            .child(rect().width(Size::flex(1.2)).child(line(
                BODY,
                Ink::Muted,
                track.artist.clone(),
            )))
            .maybe_child(self.album.then(|| {
                rect()
                    .width(Size::flex(1.2))
                    .child(line(BODY, Ink::Muted, track.album.clone()))
            }))
            .child(
                rect()
                    .tw(&tokens, &[w(Dim::Box11), flex_row(), justify(Align::End)])
                    .child(line(BODY, Ink::Muted, duration)),
            )
            .child(more)
        };
        row.focusable(focus())
    }
}
