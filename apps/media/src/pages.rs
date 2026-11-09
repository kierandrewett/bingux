//! The library's pages: songs, albums, artists, videos, the queue, search
//! results and the empty library.

use std::path::PathBuf;
use std::sync::Arc;

use bingux_media::library::{Album, Snapshot, Track, Video};
use bingux_media::{Item, Kind};
use bingux_ui::files::{ChooserRequest, choose};
use bingux_ui::media::{ArtworkKind, artwork, format_time};
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::typography::{CAPTION, EMPHASIS, HEADING, TITLE};
use freya::prelude::*;

use crate::SCHEME;
use crate::engine::{engine, use_media};
use crate::strings as text;

/// Whether the full player is open, and how the window is shown.
#[derive(Clone, Copy, PartialEq)]
pub struct Ui {
    pub stage: State<bool>,
    pub mini: State<bool>,
    pub fullscreen: State<bool>,
}

pub fn use_ui() -> Ui {
    use_consume::<Ui>()
}

fn location(parts: &[&str]) -> Location {
    parts
        .iter()
        .fold(Location::new(SCHEME), |location, part| location.join(*part))
}

/// Plays `items` from `start` and, for a video, opens the player.
pub fn play(items: Vec<Item>, start: usize, ui: Ui) {
    let video = items
        .get(start)
        .is_some_and(|item| item.kind == Kind::Video);
    engine().player.play_items(items, start);
    if video {
        let mut stage = ui.stage;
        stage.set(true);
    }
}

/// The menu of a song: play next, play later, the album and the artist.
fn song_menu(track: &Track, router: Router) -> Vec<MenuItem> {
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

/// One song row; `list` is what plays from it.
fn song_row(
    track: &Track,
    list: &Arc<Vec<Track>>,
    index: usize,
    current: Option<&PathBuf>,
    ui: Ui,
    router: Router,
) -> Row {
    let list = list.clone();
    let subtitle = match (track.artist.is_empty(), track.album.is_empty()) {
        (false, false) => format!("{}, {}", track.artist, track.album),
        (false, true) => track.artist.clone(),
        (true, false) => track.album.clone(),
        (true, true) => String::new(),
    };
    row(track.title.clone())
        .maybe_subtitle((!subtitle.is_empty()).then_some(subtitle))
        .value(track.duration.map(format_time).unwrap_or_default())
        .selected(current == Some(&track.path))
        .context_menu(song_menu(track, router))
        .on_press(move |_| play(list.iter().map(Track::item).collect(), index, ui))
}

fn page(tokens: &Tokens) -> Rect {
    rect()
        .width(Size::fill())
        .tw(tokens, &[flex_col(), gap(S6), pb(S6)])
}

/// What a page shows when the library has nothing for it.
#[derive(PartialEq)]
struct Empty {
    icon: Icon,
    title: &'static str,
    body: &'static str,
}

impl Component for Empty {
    fn render(&self) -> impl IntoElement {
        let live = use_media();
        let window = use_window();
        if *live.scanning.read() {
            return status_page(Icon::Refresh, text::SCANNING).into_element();
        }
        let whole_library_empty = live.library.read().is_empty();
        if !whole_library_empty {
            return status_page(self.icon, self.title)
                .description(self.body)
                .into_element();
        }
        status_page(Icon::FolderMusic, text::EMPTY_TITLE)
            .description(text::EMPTY_BODY)
            .action(text::EMPTY_ACTION, move |_| add_folder(window))
            .into_element()
    }
}

/// Asks for a folder and adds it to the library.
pub fn add_folder(window: Window) {
    choose(
        window,
        ChooserRequest::select_folder()
            .title(text::EMPTY_ACTION)
            .accept_label("Add"),
        |chosen| {
            if let Some(chosen) = chosen {
                let folders = chosen.paths.clone();
                engine().change_settings(move |settings| {
                    for folder in folders {
                        if !settings.folders.contains(&folder) {
                            settings.folders.push(folder);
                        }
                    }
                });
                engine().scan();
            }
        },
    );
}

// ---- pages ----------------------------------------------------------------------

pub fn index() -> Element {
    songs()
}

pub fn songs() -> Element {
    SongsPage.into_element()
}

#[derive(PartialEq)]
struct SongsPage;

impl Component for SongsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let ui = use_ui();
        let router = use_router();
        let library = live.library.read().clone();
        let current = live
            .status
            .read()
            .item
            .as_ref()
            .map(|item| item.path.clone());
        let songs = Arc::new(library.songs());
        if songs.is_empty() {
            return Empty {
                icon: Icon::AudioXGeneric,
                title: text::EMPTY_SECTION_SONGS,
                body: text::EMPTY_SECTION_SONGS_BODY,
            }
            .into_element();
        }
        let total: f64 = songs.iter().filter_map(|song| song.duration).sum();
        let play_all = {
            let songs = songs.clone();
            move |_| play(songs.iter().map(Track::item).collect(), 0, ui)
        };
        let shuffle_all = {
            let songs = songs.clone();
            move |_| {
                engine().player.set_shuffle(true);
                play(songs.iter().map(Track::item).collect(), 0, ui);
            }
        };
        let mut group = Group::new(None::<String>);
        for (index, track) in songs.iter().enumerate() {
            group = group.row(song_row(track, &songs, index, current.as_ref(), ui, router));
        }
        page(&tokens)
            .child(
                PageHead::new()
                    .subtitle(format!(
                        "{}, {}",
                        text::songs(songs.len()),
                        text::length(total)
                    ))
                    .action(text::SHUFFLE, shuffle_all),
            )
            .child(
                hstack(S2).child(
                    action_button(text::PLAY, play_all)
                        .suggested()
                        .icon(Icon::MediaPlaybackStart),
                ),
            )
            .child(group)
            .into_element()
    }
}

pub fn albums() -> Element {
    AlbumsPage.into_element()
}

#[derive(PartialEq)]
struct AlbumsPage;

/// The width of a tile in a grid of `width`: as many as fit at least
/// `least` wide, sharing the row.
fn tile_width(width: f32, least: f32, gap: f32) -> f32 {
    let columns = ((width + gap) / (least + gap)).floor().max(1.0);
    ((width - gap * (columns - 1.0)) / columns).floor()
}

#[derive(PartialEq)]
struct AlbumTile {
    album: Album,
    side: f32,
}

impl Component for AlbumTile {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let router = use_router();
        let target = location(&["albums", &self.album.key]);
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}, {}", self.album.title, self.album.artist))
            .width(Size::px(self.side))
            .tw(&tokens, &[flex_col(), gap(S1)])
            .on_press(move |_| router.push(&target))
            .child(artwork(self.album.artwork.clone(), self.album.title.clone()).side(self.side))
            .child(
                EMPHASIS
                    .label(&tokens, self.album.title.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, self.album.artist.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

/// A grid of album tiles that fills its width.
#[derive(PartialEq)]
struct AlbumGrid {
    albums: Vec<Album>,
}

impl Component for AlbumGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| 0.0f32);
        let gap_px = S4.px();
        let side = tile_width(
            *width.read(),
            if is_compact() { 140.0 } else { 160.0 },
            gap_px,
        );
        let mut grid = rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_row(), wrap(S4), gap(S4)])
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width())
            });
        if side > 0.0 {
            for album in &self.albums {
                grid = grid.child(AlbumTile {
                    album: album.clone(),
                    side,
                });
            }
        }
        grid
    }
}

impl Component for AlbumsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let albums = live.library.read().albums();
        if albums.is_empty() {
            return Empty {
                icon: Icon::MediaOptical,
                title: text::EMPTY_SECTION_SONGS,
                body: text::EMPTY_SECTION_SONGS_BODY,
            }
            .into_element();
        }
        page(&tokens)
            .child(PageHead::new().subtitle(text::albums(albums.len())))
            .child(AlbumGrid { albums })
            .into_element()
    }
}

pub fn album_title(key: &str) -> String {
    engine()
        .snapshot()
        .albums()
        .into_iter()
        .find(|album| album.key == key)
        .map(|album| album.title)
        .unwrap_or_default()
}

pub fn album(key: String) -> Element {
    AlbumPage { key }.into_element()
}

#[derive(PartialEq)]
struct AlbumPage {
    key: String,
}

impl Component for AlbumPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let ui = use_ui();
        let router = use_router();
        let current = live
            .status
            .read()
            .item
            .as_ref()
            .map(|item| item.path.clone());
        let Some(album) = live
            .library
            .read()
            .albums()
            .into_iter()
            .find(|album| album.key == self.key)
        else {
            return status_page(Icon::MediaOptical, "This album is not in the library")
                .into_element();
        };
        let tracks = Arc::new(album.tracks.clone());
        let duration = album.duration();
        let details = match album.year {
            Some(year) => format!(
                "{year}, {}, {}",
                text::songs(tracks.len()),
                text::length(duration)
            ),
            None => format!("{}, {}", text::songs(tracks.len()), text::length(duration)),
        };
        let artist_target = location(&["artists", &album.artist]);
        let play_all = {
            let tracks = tracks.clone();
            move |_| play(tracks.iter().map(Track::item).collect(), 0, ui)
        };
        let shuffle = {
            let tracks = tracks.clone();
            move |_| {
                engine().player.set_shuffle(true);
                play(tracks.iter().map(Track::item).collect(), 0, ui);
            }
        };
        let side = if is_compact() { 140.0 } else { 200.0 };
        let head = rect()
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    if is_compact() { flex_col() } else { flex_row() },
                    gap(S6),
                    items(Align::End),
                ],
            )
            .child(
                artwork(album.artwork.clone(), album.title.clone())
                    .side(side)
                    .label(format!("{}, cover", album.title)),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[flex_col(), gap(S2)])
                    .child(TITLE.label(&tokens, album.title.clone()))
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Link)
                            .on_press(move |_| router.push(&artist_target))
                            .child(
                                HEADING
                                    .in_ink(Ink::Accent)
                                    .label(&tokens, album.artist.clone()),
                            ),
                    )
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, details))
                    .child(
                        hstack(S2)
                            .child(
                                action_button(text::PLAY, play_all)
                                    .suggested()
                                    .icon(Icon::MediaPlaybackStart),
                            )
                            .child(
                                action_button(text::SHUFFLE, shuffle)
                                    .icon(Icon::MediaPlaylistShuffle),
                            ),
                    ),
            );
        let mut group = Group::new(None::<String>);
        for (index, track) in tracks.iter().enumerate() {
            let list = tracks.clone();
            let title = match track.track {
                Some(number) => format!("{number}. {}", track.title),
                None => track.title.clone(),
            };
            let artist = (track.artist != album.artist && !track.artist.is_empty())
                .then(|| track.artist.clone());
            group = group.row(
                row(title)
                    .maybe_subtitle(artist)
                    .value(track.duration.map(format_time).unwrap_or_default())
                    .selected(current.as_ref() == Some(&track.path))
                    .context_menu(song_menu(track, router))
                    .on_press(move |_| play(list.iter().map(Track::item).collect(), index, ui)),
            );
        }
        page(&tokens).child(head).child(group).into_element()
    }
}

pub fn artists() -> Element {
    ArtistsPage.into_element()
}

#[derive(PartialEq)]
struct ArtistsPage;

impl Component for ArtistsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let router = use_router();
        let artists = live.library.read().artists();
        if artists.is_empty() {
            return Empty {
                icon: Icon::AvatarDefault,
                title: text::EMPTY_SECTION_SONGS,
                body: text::EMPTY_SECTION_SONGS_BODY,
            }
            .into_element();
        }
        let mut group = Group::new(None::<String>);
        for artist in &artists {
            let target = location(&["artists", &artist.name]);
            group = group.row(
                row(artist.name.clone())
                    .subtitle(format!(
                        "{}, {}",
                        text::albums(artist.albums.len()),
                        text::songs(artist.track_count())
                    ))
                    .leading(Icon::AvatarDefault)
                    .navigates()
                    .on_press(move |_| router.push(&target)),
            );
        }
        page(&tokens).child(group).into_element()
    }
}

pub fn artist(name: String) -> Element {
    ArtistPage { name }.into_element()
}

#[derive(PartialEq)]
struct ArtistPage {
    name: String,
}

impl Component for ArtistPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let ui = use_ui();
        let Some(artist) = live
            .library
            .read()
            .artists()
            .into_iter()
            .find(|artist| artist.name == self.name)
        else {
            return status_page(Icon::AvatarDefault, "This artist is not in the library")
                .into_element();
        };
        let songs: Vec<Item> = artist
            .albums
            .iter()
            .flat_map(|album| album.tracks.iter().map(Track::item))
            .collect();
        let play_all = move |_| play(songs.clone(), 0, ui);
        page(&tokens)
            .child(
                hstack(S4)
                    .child(
                        artwork(artist.artwork.clone(), artist.name.clone())
                            .kind(ArtworkKind::Artist)
                            .size(Dim::Box24),
                    )
                    .child(
                        rect()
                            .tw(&tokens, &[flex_col(), gap(S2)])
                            .child(TITLE.label(&tokens, artist.name.clone()))
                            .child(CAPTION.in_ink(Ink::Muted).label(
                                &tokens,
                                format!(
                                    "{}, {}",
                                    text::albums(artist.albums.len()),
                                    text::songs(artist.track_count())
                                ),
                            ))
                            .child(
                                action_button(text::PLAY, play_all)
                                    .suggested()
                                    .icon(Icon::MediaPlaybackStart),
                            ),
                    ),
            )
            .child(
                Group::new(Some(text::ALBUMS.to_owned())).content(AlbumGrid {
                    albums: artist.albums.clone(),
                }),
            )
            .into_element()
    }
}

pub fn videos() -> Element {
    VideosPage.into_element()
}

#[derive(PartialEq)]
struct VideoTile {
    video: Video,
    side: f32,
}

impl Component for VideoTile {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ui = use_ui();
        let item = self.video.item();
        let progress = self.video.progress();
        let details = match (self.video.duration, progress) {
            (Some(duration), Some(progress)) => {
                format!("{} left", format_time(duration * (1.0 - progress)))
            }
            (Some(duration), None) => format_time(duration),
            _ => self.video.folder.clone(),
        };
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Play {}", self.video.title))
            .width(Size::px(self.side))
            .tw(&tokens, &[flex_col(), gap(S1)])
            .on_press(move |_| play(vec![item.clone()], 0, ui))
            .child(
                rect()
                    .child(
                        artwork(self.video.thumbnail.clone(), self.video.title.clone())
                            .kind(ArtworkKind::Video)
                            .side(self.side),
                    )
                    .maybe_child(progress.map(|progress| {
                        // How far it was watched, along the bottom of the frame.
                        rect()
                            .position(Position::new_absolute().bottom(0.0).left(0.0))
                            .width(Size::px(self.side))
                            .tw(&tokens, &[h(Dim::SliderTrack), bg(Paint::Backdrop)])
                            .child(
                                rect()
                                    .width(Size::percent(progress as f32 * 100.0))
                                    .height(Size::fill())
                                    .tw(&tokens, &[bg(Paint::Accent)]),
                            )
                    })),
            )
            .child(
                EMPHASIS
                    .label(&tokens, self.video.title.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, details)
                    .max_lines(1),
            )
    }
}

#[derive(PartialEq)]
struct VideoGrid {
    videos: Vec<Video>,
}

impl Component for VideoGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| 0.0f32);
        let side = tile_width(
            *width.read(),
            if is_compact() { 150.0 } else { 220.0 },
            S4.px(),
        );
        let mut grid = rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_row(), wrap(S4), gap(S4)])
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width())
            });
        if side > 0.0 {
            for video in &self.videos {
                grid = grid.child(VideoTile {
                    video: video.clone(),
                    side,
                });
            }
        }
        grid
    }
}

#[derive(PartialEq)]
struct VideosPage;

impl Component for VideosPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let library = live.library.read().clone();
        let videos = library.videos_by_added();
        if videos.is_empty() {
            return Empty {
                icon: Icon::VideoGeneric,
                title: text::EMPTY_SECTION_VIDEOS,
                body: text::EMPTY_SECTION_VIDEOS_BODY,
            }
            .into_element();
        }
        let resume = library.continue_watching();
        page(&tokens)
            .maybe_child((!resume.is_empty()).then(|| {
                Group::new(Some(text::CONTINUE_WATCHING.to_owned()))
                    .content(VideoGrid { videos: resume })
            }))
            .child(Group::new(Some(text::ALL_VIDEOS.to_owned())).content(VideoGrid { videos }))
            .into_element()
    }
}

pub fn queue() -> Element {
    QueuePage.into_element()
}

#[derive(PartialEq)]
struct QueuePage;

impl Component for QueuePage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let ui = use_ui();
        let queue = live.queue.read().clone();
        let Some(current) = queue.current() else {
            return status_page(Icon::ViewListOrdered, text::EMPTY_QUEUE)
                .description(text::EMPTY_QUEUE_BODY)
                .into_element();
        };
        let item_row = |index: usize, item: &Item| -> Row {
            let subtitle = if item.artist.is_empty() {
                None
            } else {
                Some(item.artist.clone())
            };
            let count = queue.items().len();
            let mut menu = vec![menu_item(text::REMOVE_FROM_QUEUE, move |_| {
                engine().player.remove(index)
            })];
            if index > current + 1 {
                menu.insert(
                    0,
                    menu_item(text::PLAY_NEXT, move |_| {
                        engine().player.reorder(index, current + 1)
                    }),
                );
            }
            if index + 1 < count && index > current {
                menu.push(menu_item("Move down", move |_| {
                    engine().player.reorder(index, index + 1)
                }));
            }
            if index > current + 1 {
                menu.push(menu_item("Move up", move |_| {
                    engine().player.reorder(index, index - 1)
                }));
            }
            row(item.title.clone())
                .maybe_subtitle(subtitle)
                .value(item.duration.map(format_time).unwrap_or_default())
                .leading(if item.kind == Kind::Video {
                    Icon::VideoGeneric
                } else {
                    Icon::AudioXGeneric
                })
                .context_menu(menu)
                .on_press(move |_| {
                    engine().player.play_index(index);
                    if engine()
                        .player
                        .queue()
                        .items()
                        .get(index)
                        .is_some_and(|item| item.kind == Kind::Video)
                    {
                        let mut stage = ui.stage;
                        stage.set(true);
                    }
                })
        };
        let playing = queue.items()[current].clone();
        let mut now = Group::new(Some(text::NOW_PLAYING.to_owned()));
        now = now.row(item_row(current, &playing).selected(true));
        let mut next = Group::new(Some(text::UP_NEXT.to_owned()));
        for (offset, item) in queue.upcoming().iter().enumerate() {
            next = next.row(item_row(current + 1 + offset, item));
        }
        page(&tokens)
            .child(
                PageHead::new()
                    .subtitle(text::songs(queue.items().len()))
                    .action(text::CLEAR_UP_NEXT, |_| engine().player.clear_upcoming()),
            )
            .child(now)
            .maybe_child((!queue.upcoming().is_empty()).then_some(next))
            .into_element()
    }
}

/// The search results while the sidebar search has text.
#[derive(PartialEq)]
pub struct SearchResults {
    pub query: State<String>,
}

impl Component for SearchResults {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let live = use_media();
        let ui = use_ui();
        let router = use_router();
        let query = self.query.read().clone();
        let library: Arc<Snapshot> = live.library.read().clone();
        let (songs, videos) = library.search(&query);
        if songs.is_empty() && videos.is_empty() {
            return status_page(Icon::Search, text::NO_RESULTS)
                .description(text::NO_RESULTS_BODY)
                .into_element();
        }
        let current = live
            .status
            .read()
            .item
            .as_ref()
            .map(|item| item.path.clone());
        let songs = Arc::new(songs);
        let mut column = page(&tokens);
        if !songs.is_empty() {
            let mut group = Group::new(Some(text::SONGS.to_owned()));
            for (index, track) in songs.iter().enumerate().take(50) {
                group = group.row(song_row(track, &songs, index, current.as_ref(), ui, router));
            }
            column = column.child(group);
        }
        if !videos.is_empty() {
            let mut group = Group::new(Some(text::VIDEOS.to_owned()));
            for video in videos.iter().take(50) {
                let item = video.item();
                group = group.row(
                    row(video.title.clone())
                        .leading(Icon::VideoGeneric)
                        .value(video.duration.map(format_time).unwrap_or_default())
                        .on_press(move |_| play(vec![item.clone()], 0, ui)),
                );
            }
            column = column.child(group);
        }
        column.into_element()
    }
}

/// `bingux-media://open?path=…`: plays the file or folder, then shows it.
pub fn open(path: String) -> Element {
    OpenPage { path }.into_element()
}

#[derive(PartialEq)]
struct OpenPage {
    path: String,
}

impl Component for OpenPage {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let ui = use_ui();
        let path = PathBuf::from(self.path.clone());
        use_hook(move || {
            if path.as_os_str().is_empty() {
                return;
            }
            // Reading a folder waits on the disk: a worker does it.
            let _ = std::thread::Builder::new()
                .name("media-open".into())
                .spawn(move || {
                    engine().play_paths(&[path]);
                });
            let mut stage = ui.stage;
            stage.set(true);
            router.replace(&location(&["queue"]));
        });
        rect().into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_share_the_row() {
        assert_eq!(tile_width(700.0, 160.0, 16.0), 163.0);
        assert_eq!(tile_width(100.0, 160.0, 16.0), 100.0);
    }
}
