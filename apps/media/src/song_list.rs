//! A list of songs as a music library shows it, on bingux-ui's `data_table`:
//! columns for the title (with the artwork or the track number before it),
//! artist, album and time, which sort, resize, move and hide, and are
//! remembered per list. Rows select as a list's do (click, Shift, Ctrl, the
//! arrows, letters jump); a double click or Enter plays the list from that
//! song, a tap on a phone. The menu of a row (right click, long press, the
//! Menu key) has Play next and Play later for every song selected, and Go to
//! album and Go to artist. The song playing has its title in the accent, and
//! in an album a speaker in place of its number.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use bingux_media::Item;
use bingux_media::library::Track;
use bingux_ui::icons;
use bingux_ui::media::{ArtworkKind, artwork};
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::surface::tokens_now;
use bingux_ui::table::natural_cmp;
use bingux_ui::typography::{BODY, CAPTION};
use freya::prelude::*;

use crate::engine::engine;
use crate::pages::{Ui, location, play, use_ui};
use crate::strings as text;

/// What the title column shows before the title.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lead {
    Artwork,
    /// The track number: an album's own list.
    Number,
}

/// The songs, shared with the library's snapshot rather than copied.
struct Songs(Arc<Vec<Track>>);

impl RowSource<Track> for Songs {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, index: usize) -> Option<&Track> {
        self.0.get(index)
    }
}

pub struct SongList {
    pub songs: Arc<Vec<Track>>,
    pub current: Option<PathBuf>,
    pub lead: Lead,
    /// Show the album column (not in an album's own list).
    pub album: bool,
    /// Show the artist column (not in an album by one artist).
    pub artist: bool,
    /// Where the list keeps its columns and sort: "songs", "album", "search".
    pub id: &'static str,
}

impl PartialEq for SongList {
    fn eq(&self, other: &Self) -> bool {
        // The same snapshot: comparing every song on each tick of the
        // clock would cost more than drawing them.
        Arc::ptr_eq(&self.songs, &other.songs)
            && self.current == other.current
            && self.lead == other.lead
            && self.album == other.album
            && self.artist == other.artist
            && self.id == other.id
    }
}

fn title_cell(track: &Track, lead: Lead, current: State<Option<PathBuf>>) -> Element {
    let tokens = tokens_now();
    let playing = current.read().as_ref() == Some(&track.path);
    let ink = if playing { Ink::Accent } else { Ink::Text };
    let title = BODY
        .in_ink(ink)
        .label(&tokens, track.title.clone())
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis);
    // A phone's row is the title over a line of the rest.
    if is_compact() {
        return title.into_element();
    }
    let lead: Element = match lead {
        Lead::Artwork => artwork(track.artwork.clone(), track.album.clone())
            .kind(ArtworkKind::Song)
            .size(Dim::Box7)
            .into_element(),
        Lead::Number if playing => rect()
            .tw(&tokens, &[w(Dim::Box7), flex_row(), justify(Align::End)])
            .child(icons::icon(
                Icon::Sound,
                IconSize::Mark,
                Ink::Accent.color(&tokens),
            ))
            .into_element(),
        Lead::Number => rect()
            .tw(&tokens, &[w(Dim::Box7), flex_row(), justify(Align::End)])
            .child(
                CAPTION.in_ink(Ink::Muted).label(
                    &tokens,
                    track
                        .track
                        .map(|number| number.to_string())
                        .unwrap_or_default(),
                ),
            )
            .into_element(),
    };
    rect()
        .width(Size::fill())
        .tw(
            &tokens,
            &[flex_row(), grow_children(), items(Align::Center), gap(S3)],
        )
        .child(lead)
        .child(rect().width(Size::flex(1.0)).child(title))
        .into_element()
}

fn columns(
    lead: Lead,
    current: State<Option<PathBuf>>,
    album: bool,
    artist: bool,
) -> Vec<Column<Track>> {
    let mut columns = vec![
        Column::new("title", text::column_title(), move |track: &Track| {
            title_cell(track, lead, current)
        })
        .summary(|track: &Track| track.title.clone())
        .flex()
        .min_width(160.0)
        .priority(0)
        .sort_by(|a: &Track, b: &Track| natural_cmp(&a.title, &b.title)),
    ];
    if artist {
        columns.push(
            Column::text("artist", text::column_artist(), |track: &Track| {
                track.artist.clone()
            })
            .flex()
            .min_width(120.0)
            .muted()
            .priority(2)
            .sort_by_text(),
        );
    }
    if album {
        columns.push(
            Column::text("album", text::column_album(), |track: &Track| {
                track.album.clone()
            })
            .flex()
            .min_width(120.0)
            .muted()
            .priority(3)
            .sort_by_text(),
        );
    }
    columns.push(
        Column::text("time", text::column_time(), |track: &Track| {
            track.duration.map(text::clock).unwrap_or_default()
        })
        .width(72.0)
        .numeric()
        .muted()
        .priority(1)
        .sort_by(|a: &Track, b: &Track| {
            a.duration
                .unwrap_or_default()
                .total_cmp(&b.duration.unwrap_or_default())
        }),
    );
    columns
}

/// The menu of the rows `keys`: queue them all; go to the album and artist
/// of one.
fn row_menu(keys: &[PathBuf], songs: &[Track], router: Router) -> Vec<MenuItem> {
    let chosen: Vec<&Track> = keys
        .iter()
        .filter_map(|key| songs.iter().find(|track| &track.path == key))
        .collect();
    let items: Vec<Item> = chosen.iter().map(|track| track.item()).collect();
    if items.is_empty() {
        return Vec::new();
    }
    let later = items.clone();
    let mut menu = vec![
        menu_item(text::play_next(), move |_| {
            engine().player.enqueue(items.clone(), true)
        }),
        menu_item(text::play_later(), move |_| {
            engine().player.enqueue(later.clone(), false)
        }),
    ];
    if let [track] = chosen.as_slice() {
        let album = location(&["albums", &track.album_key()]);
        let artist = location(&["artists", track.filed_artist()]);
        menu.push(menu_separator());
        menu.push(menu_item(text::show_album(), move |_| router.push(&album)));
        menu.push(menu_item(text::show_artist(), move |_| {
            router.push(&artist)
        }));
    }
    menu
}

impl Component for SongList {
    fn render(&self) -> impl IntoElement {
        let ui: Ui = use_ui();
        let router = use_router();
        let compact = is_compact();
        // The song playing, as a state the title cells read: a new song
        // redraws two rows, not the list.
        let mut current = use_state(|| self.current.clone());
        use_side_effect_with_deps(&self.current, move |now| {
            current.set_if_modified(now.clone())
        });
        let (lead, album, artist) = (self.lead, self.album, self.artist);
        let memo = use_hook(|| {
            Rc::new(RefCell::new(
                None::<((Lead, bool, bool), Rc<Vec<Column<Track>>>)>,
            ))
        });
        let columns = {
            let mut memo = memo.borrow_mut();
            match memo.as_ref() {
                Some((made, columns)) if *made == (lead, album, artist) => columns.clone(),
                _ => {
                    let made = Rc::new(columns(lead, current, album, artist));
                    *memo = Some(((lead, album, artist), made.clone()));
                    made
                }
            }
        };
        let rows_memo = use_hook(|| Rc::new(RefCell::new(None::<(Arc<Vec<Track>>, Rows<Track>)>)));
        let rows = {
            let mut memo = rows_memo.borrow_mut();
            match memo.as_ref() {
                Some((songs, rows)) if Arc::ptr_eq(songs, &self.songs) => rows.clone(),
                _ => {
                    let rows = Rows::from_source(Rc::new(Songs(self.songs.clone())));
                    *memo = Some((self.songs.clone(), rows.clone()));
                    rows
                }
            }
        };
        let songs = self.songs.clone();
        let play_from = move |key: PathBuf| {
            if let Some(index) = songs.iter().position(|track| track.path == key) {
                play(songs.iter().map(Track::item).collect(), index, ui);
            }
        };
        let menu_songs = self.songs.clone();
        // The page scrolls; the table is as tall as its rows.
        let row = if compact {
            Dim::Row.px()
        } else {
            Dim::NavItem.px()
        };
        let height = Dim::Control.px() + 8.0 + self.songs.len() as f32 * row;
        rect().width(Size::fill()).height(Size::px(height)).child(
            data_table(rows, columns, |track: &Track| track.path.clone())
                .id(self.id)
                .label(text::songs_heading())
                .chevrons(false)
                .row_menu(move |keys: &[PathBuf]| row_menu(keys, &menu_songs, router))
                .on_activate(play_from),
        )
    }
}
