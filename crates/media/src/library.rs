//! The library: every song and film in the person's folders, indexed.
//!
//! The folders are the XDG Music and Videos folders (`~/Music`, `~/Videos`, or
//! wherever `user-dirs.dirs` puts them) and any the person adds. A scan walks
//! them and compares each file's size and modified time with the index
//! (SQLite, `$XDG_DATA_HOME/bingux-media/library.db`): only new and changed
//! files are opened and their tags read, and files that are gone are dropped,
//! so a scan of a library that has not changed reads directories and nothing
//! else.
//!
//! Tags are read with lofty (title, artist, album, album artist, track and
//! disc numbers, year, genre, length, ReplayGain is left to mpv). Cover art is
//! the picture inside the file (the front cover first), else a `cover.jpg`,
//! `folder.jpg`, `front.png` or `AlbumArt*.jpg` beside it; a picture from
//! inside a file is written once per album to
//! `$XDG_CACHE_HOME/bingux-media/artwork/`.
//!
//! The index also keeps where each film was left ([`Library::position`]),
//! how often each song was played, and the plays waiting to be scrobbled.
//!
//! Every method waits on the disk: call them from a worker, never from the
//! window's thread.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::queue::{Item, Kind, next_id};

/// The audio files the library takes, by extension.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "m4b", "aac", "alac", "wav", "aif", "aiff", "ape",
    "wv", "mpc", "wma", "dsf", "dff", "mka", "spx", "tta",
];

/// The video files the library takes, by extension.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "mts",
    "ogv", "3gp", "vob", "divx", "asf", "rmvb",
];

/// What a file is, by its extension.
pub fn kind_of(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        Some(Kind::Audio)
    } else if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        Some(Kind::Video)
    } else {
        None
    }
}

/// A song in the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub genre: String,
    pub duration: Option<f64>,
    pub artwork: Option<PathBuf>,
    /// Seconds since 1970 when it was first found.
    pub added: i64,
    pub plays: u32,
    pub last_played: Option<i64>,
}

impl Track {
    /// The artist an album is filed under: the album artist, else the artist.
    pub fn filed_artist(&self) -> &str {
        if self.album_artist.is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }

    /// The key albums are grouped by: the album and who it is filed under.
    pub fn album_key(&self) -> String {
        album_key(&self.album, self.filed_artist())
    }

    pub fn item(&self) -> Item {
        Item {
            id: next_id(),
            path: self.path.clone(),
            kind: Kind::Audio,
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            album_artist: self.album_artist.clone(),
            track: self.track,
            duration: self.duration,
            artwork: self.artwork.clone(),
        }
    }
}

/// The grouping key of an album: case does not split an album.
pub fn album_key(album: &str, artist: &str) -> String {
    format!(
        "{}\u{1f}{}",
        album.trim().to_lowercase(),
        artist.trim().to_lowercase()
    )
}

/// A film in the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    /// The folder it is in, relative to the library folder ("Holidays"), or empty.
    pub folder: String,
    pub duration: Option<f64>,
    pub size: u64,
    pub added: i64,
    /// Where it was left, when it was not finished.
    pub position: Option<f64>,
    /// A frame of it, once one is made.
    pub thumbnail: Option<PathBuf>,
    pub modified: i64,
}

impl Video {
    pub fn item(&self) -> Item {
        Item {
            id: next_id(),
            path: self.path.clone(),
            kind: Kind::Video,
            title: self.title.clone(),
            duration: self.duration,
            artwork: self.thumbnail.clone(),
            ..Item::default()
        }
    }

    /// How far through it was left, 0 to 1.
    pub fn progress(&self) -> Option<f64> {
        let position = self.position?;
        let duration = self.duration.filter(|duration| *duration > 0.0)?;
        Some((position / duration).clamp(0.0, 1.0))
    }
}

/// An album: its tracks share the album name and the filed artist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Album {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    pub artwork: Option<PathBuf>,
    pub tracks: Vec<Track>,
}

impl Album {
    pub fn duration(&self) -> f64 {
        self.tracks.iter().filter_map(|track| track.duration).sum()
    }
}

/// An artist, with their albums.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Artist {
    pub name: String,
    pub albums: Vec<Album>,
    pub artwork: Option<PathBuf>,
}

impl Artist {
    pub fn track_count(&self) -> usize {
        self.albums.iter().map(|album| album.tracks.len()).sum()
    }
}

/// Everything in the library, read at once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub tracks: Vec<Track>,
    pub videos: Vec<Video>,
}

impl Snapshot {
    /// The albums, by title. Tracks in disc and track order.
    pub fn albums(&self) -> Vec<Album> {
        let mut albums: BTreeMap<String, Album> = BTreeMap::new();
        for track in &self.tracks {
            let key = track.album_key();
            let album = albums.entry(key.clone()).or_insert_with(|| Album {
                key,
                title: if track.album.is_empty() {
                    "Unknown Album".into()
                } else {
                    track.album.clone()
                },
                artist: if track.filed_artist().is_empty() {
                    "Unknown Artist".into()
                } else {
                    track.filed_artist().to_owned()
                },
                ..Album::default()
            });
            album.year = album.year.or(track.year);
            if album.artwork.is_none() {
                album.artwork = track.artwork.clone();
            }
            album.tracks.push(track.clone());
        }
        let mut albums: Vec<Album> = albums.into_values().collect();
        for album in &mut albums {
            album.tracks.sort_by_key(|track| {
                (
                    track.disc.unwrap_or(1),
                    track.track.unwrap_or(u32::MAX),
                    track.title.to_lowercase(),
                )
            });
        }
        albums.sort_by(|a, b| {
            sort_name(&a.title)
                .cmp(&sort_name(&b.title))
                .then_with(|| a.artist.cmp(&b.artist))
        });
        albums
    }

    /// The artists, by name, each with their albums newest first.
    pub fn artists(&self) -> Vec<Artist> {
        let mut artists: BTreeMap<String, Artist> = BTreeMap::new();
        for album in self.albums() {
            let artist = artists
                .entry(album.artist.to_lowercase())
                .or_insert_with(|| Artist {
                    name: album.artist.clone(),
                    ..Artist::default()
                });
            if artist.artwork.is_none() {
                artist.artwork = album.artwork.clone();
            }
            artist.albums.push(album);
        }
        let mut artists: Vec<Artist> = artists.into_values().collect();
        for artist in &mut artists {
            artist
                .albums
                .sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)));
        }
        artists.sort_by(|a, b| sort_name(&a.name).cmp(&sort_name(&b.name)));
        artists
    }

    /// Songs by title.
    pub fn songs(&self) -> Vec<Track> {
        let mut songs = self.tracks.clone();
        songs.sort_by(|a, b| sort_name(&a.title).cmp(&sort_name(&b.title)));
        songs
    }

    /// Songs and films whose title, artist or album has every word of `query`.
    pub fn search(&self, query: &str) -> (Vec<Track>, Vec<Video>) {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let hit = |fields: &[&str]| {
            let text = fields.join(" ").to_lowercase();
            words.iter().all(|word| text.contains(word.as_str()))
        };
        let tracks = self
            .songs()
            .into_iter()
            .filter(|track| {
                hit(&[
                    &track.title,
                    &track.artist,
                    &track.album,
                    &track.album_artist,
                    &track.genre,
                ])
            })
            .collect();
        let videos = self
            .videos
            .iter()
            .filter(|video| hit(&[&video.title, &video.folder]))
            .cloned()
            .collect();
        (tracks, videos)
    }

    /// Films most recently added first.
    pub fn videos_by_added(&self) -> Vec<Video> {
        let mut videos = self.videos.clone();
        videos.sort_by(|a, b| b.added.cmp(&a.added).then_with(|| a.title.cmp(&b.title)));
        videos
    }

    /// Films left part way through, most recently watched first.
    pub fn continue_watching(&self) -> Vec<Video> {
        self.videos
            .iter()
            .filter(|video| video.position.is_some())
            .cloned()
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.videos.is_empty()
    }
}

/// A title as it sorts: "The Tones" files under T, "A Four Forty" under F.
pub fn sort_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    lower
}

/// What a scan did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Files that could not be read (damaged, or not what their name says).
    pub failed: usize,
}

impl ScanReport {
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}

/// The library index.
pub struct Library {
    db: Mutex<Connection>,
    artwork_dir: PathBuf,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0)
}

fn path_text(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

fn path_from(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

/// `$XDG_DATA_HOME/bingux-media`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("bingux-media")
}

/// `$XDG_CACHE_HOME`.
pub fn cache_home() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(".cache"))
}

pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The folders a library has with no setup: the XDG Music and Videos folders.
pub fn default_folders() -> Vec<PathBuf> {
    use bingux_files::places::{PlaceKind, user_dir};
    let mut folders = Vec::new();
    for (kind, fallback) in [(PlaceKind::Music, "Music"), (PlaceKind::Videos, "Videos")] {
        let folder = user_dir(kind).unwrap_or_else(|| home().join(fallback));
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
    path BLOB PRIMARY KEY,
    kind INTEGER NOT NULL,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    added INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    artist TEXT NOT NULL DEFAULT '',
    album TEXT NOT NULL DEFAULT '',
    album_artist TEXT NOT NULL DEFAULT '',
    track INTEGER,
    disc INTEGER,
    year INTEGER,
    genre TEXT NOT NULL DEFAULT '',
    duration REAL,
    artwork BLOB,
    root BLOB NOT NULL DEFAULT x''
);
CREATE TABLE IF NOT EXISTS positions (
    path BLOB PRIMARY KEY,
    position REAL NOT NULL,
    duration REAL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS plays (
    path BLOB NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS plays_path ON plays(path);
CREATE TABLE IF NOT EXISTS scrobbles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    artist TEXT NOT NULL,
    title TEXT NOT NULL,
    album TEXT NOT NULL,
    album_artist TEXT NOT NULL,
    duration REAL,
    at INTEGER NOT NULL
);
";

impl Library {
    /// Opens (or makes) the index in the data folder.
    pub fn open_default() -> rusqlite::Result<Library> {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        Library::open(
            &dir.join("library.db"),
            &cache_home().join("bingux-media/artwork"),
        )
    }

    pub fn open(path: &Path, artwork_dir: &Path) -> rusqlite::Result<Library> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        db.execute_batch(SCHEMA)?;
        Ok(Library {
            db: Mutex::new(db),
            artwork_dir: artwork_dir.to_owned(),
        })
    }

    /// An index in memory, for tests.
    pub fn in_memory(artwork_dir: &Path) -> rusqlite::Result<Library> {
        let db = Connection::open_in_memory()?;
        db.execute_batch(SCHEMA)?;
        Ok(Library {
            db: Mutex::new(db),
            artwork_dir: artwork_dir.to_owned(),
        })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Brings the index up to date with `folders`. `progress` is called with
    /// the number of files looked at so far, now and then.
    pub fn scan(&self, folders: &[PathBuf], progress: &dyn Fn(usize)) -> ScanReport {
        let mut report = ScanReport::default();
        // What the index has, by path: (size, modified).
        let known: HashMap<Vec<u8>, (i64, i64)> = {
            let db = self.db();
            let mut statement = match db.prepare("SELECT path, size, modified FROM files") {
                Ok(statement) => statement,
                Err(_) => return report,
            };
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, (row.get(1)?, row.get(2)?)))
                })
                .map(|rows| rows.flatten().collect())
                .unwrap_or_default()
        };
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        let mut found: Vec<(PathBuf, PathBuf, Kind, i64, i64)> = Vec::new();
        for root in folders {
            walk(root, &mut |path, kind, size, modified| {
                found.push((root.clone(), path.to_owned(), kind, size, modified));
                if found.len() % 500 == 0 {
                    progress(found.len());
                }
            });
        }
        progress(found.len());
        // Read tags outside the lock: it is the slow part.
        let mut fresh: Vec<(PathBuf, PathBuf, Kind, i64, i64, Tags)> = Vec::new();
        let mut album_art: HashMap<String, Option<PathBuf>> = HashMap::new();
        for (root, path, kind, size, modified) in found {
            let key = path_text(&path);
            seen.insert(key.clone());
            match known.get(&key) {
                Some(&(known_size, known_modified))
                    if known_size == size && known_modified == modified =>
                {
                    report.unchanged += 1;
                    continue;
                }
                Some(_) => report.updated += 1,
                None => report.added += 1,
            }
            let tags = match kind {
                Kind::Audio => match read_tags(&path, &self.artwork_dir, &mut album_art) {
                    Some(tags) => tags,
                    None => {
                        report.failed += 1;
                        Tags::from_name(&path)
                    }
                },
                Kind::Video => Tags::from_name(&path),
            };
            fresh.push((root, path, kind, size, modified, tags));
        }
        let gone: Vec<Vec<u8>> = known
            .keys()
            .filter(|path| !seen.contains(*path))
            .cloned()
            .collect();
        report.removed = gone.len();
        let mut db = self.db();
        let Ok(transaction) = db.transaction() else {
            return report;
        };
        for path in &gone {
            let _ = transaction.execute("DELETE FROM files WHERE path = ?1", params![path]);
        }
        let added = now();
        for (root, path, kind, size, modified, tags) in fresh {
            let _ = transaction.execute(
                "INSERT INTO files (path, kind, size, modified, added, title, artist, album, album_artist, track, disc, year, genre, duration, artwork, root)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(path) DO UPDATE SET kind = ?2, size = ?3, modified = ?4, title = ?6, artist = ?7, album = ?8,
                   album_artist = ?9, track = ?10, disc = ?11, year = ?12, genre = ?13,
                   duration = COALESCE(?14, duration), artwork = ?15, root = ?16",
                params![
                    path_text(&path),
                    if kind == Kind::Audio { 0 } else { 1 },
                    size,
                    modified,
                    added,
                    tags.title,
                    tags.artist,
                    tags.album,
                    tags.album_artist,
                    tags.track,
                    tags.disc,
                    tags.year,
                    tags.genre,
                    tags.duration,
                    tags.artwork.as_deref().map(path_text),
                    path_text(&root),
                ],
            );
        }
        let _ = transaction.commit();
        report
    }

    /// Everything in the index.
    pub fn snapshot(&self) -> Snapshot {
        let db = self.db();
        let positions: HashMap<Vec<u8>, f64> = db
            .prepare("SELECT path, position FROM positions")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, f64>(1)?))
                    })
                    .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default();
        let plays: HashMap<Vec<u8>, (u32, i64)> = db
            .prepare("SELECT path, COUNT(*), MAX(at) FROM plays GROUP BY path")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            (row.get::<_, u32>(1)?, row.get::<_, i64>(2)?),
                        ))
                    })
                    .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default();
        let mut snapshot = Snapshot::default();
        let Ok(mut statement) = db.prepare(
            "SELECT path, kind, size, modified, added, title, artist, album, album_artist, track, disc, year, genre, duration, artwork, root FROM files",
        ) else {
            return snapshot;
        };
        let rows = statement.query_map([], |row| {
            let path: Vec<u8> = row.get(0)?;
            let kind: i64 = row.get(1)?;
            let root: Vec<u8> = row.get(15)?;
            let artwork: Option<Vec<u8>> = row.get(14)?;
            Ok((
                path,
                kind,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                [
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(12)?,
                ],
                (
                    row.get::<_, Option<u32>>(9)?,
                    row.get::<_, Option<u32>>(10)?,
                    row.get::<_, Option<u32>>(11)?,
                ),
                row.get::<_, Option<f64>>(13)?,
                artwork,
                root,
            ))
        });
        let Ok(rows) = rows else {
            return snapshot;
        };
        for (
            path_bytes,
            kind,
            size,
            modified,
            added,
            [title, artist, album, album_artist, genre],
            (track, disc, year),
            duration,
            artwork,
            root,
        ) in rows.flatten()
        {
            let path = path_from(path_bytes.clone());
            if kind == 0 {
                let (plays, last_played) = plays
                    .get(&path_bytes)
                    .map_or((0, None), |(count, at)| (*count, Some(*at)));
                snapshot.tracks.push(Track {
                    id: 0,
                    path,
                    title,
                    artist,
                    album,
                    album_artist,
                    track,
                    disc,
                    year,
                    genre,
                    duration,
                    artwork: artwork.map(path_from),
                    added,
                    plays,
                    last_played,
                });
            } else {
                let root = path_from(root);
                let folder = path
                    .parent()
                    .and_then(|parent| parent.strip_prefix(&root).ok())
                    .map(|relative| relative.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let thumbnail = crate::thumbnail::cached(&path, modified);
                snapshot.videos.push(Video {
                    id: 0,
                    position: positions.get(&path_bytes).copied(),
                    path,
                    title,
                    folder,
                    duration,
                    size: size.max(0) as u64,
                    added,
                    thumbnail,
                    modified,
                });
            }
        }
        for (index, track) in snapshot.tracks.iter_mut().enumerate() {
            track.id = index as i64 + 1;
        }
        for (index, video) in snapshot.videos.iter_mut().enumerate() {
            video.id = index as i64 + 1;
        }
        snapshot
    }

    /// A film's length, learnt when its thumbnail was made.
    pub fn set_duration(&self, path: &Path, duration: f64) {
        let _ = self.db().execute(
            "UPDATE files SET duration = ?2 WHERE path = ?1",
            params![path_text(path), duration],
        );
    }

    /// Where `path` was left, if it was left part way.
    pub fn position(&self, path: &Path) -> Option<f64> {
        self.db()
            .query_row(
                "SELECT position FROM positions WHERE path = ?1",
                params![path_text(path)],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Remembers where `path` is; `None` (it was finished), or a place in its
    /// first or last 15 seconds or 5 %, forgets it, so a film started again
    /// starts at the start.
    pub fn set_position(&self, path: &Path, position: Option<f64>, duration: Option<f64>) {
        let keep = position.filter(|position| match duration {
            Some(duration) if duration > 0.0 => {
                *position > 15.0 && *position < duration - 15.0 && *position / duration < 0.95
            }
            _ => *position > 15.0,
        });
        let db = self.db();
        let _ = match keep {
            Some(position) => db.execute(
                "INSERT INTO positions (path, position, duration, updated) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(path) DO UPDATE SET position = ?2, duration = ?3, updated = ?4",
                params![path_text(path), position, duration, now()],
            ),
            None => db.execute(
                "DELETE FROM positions WHERE path = ?1",
                params![path_text(path)],
            ),
        };
    }

    /// Counts a play of `path` at `at` (seconds since 1970).
    pub fn record_play(&self, path: &Path, at: i64) {
        let _ = self.db().execute(
            "INSERT INTO plays (path, at) VALUES (?1, ?2)",
            params![path_text(path), at],
        );
    }

    /// Keeps a play for Last.fm until it is sent.
    pub fn queue_scrobble(&self, scrobble: &Scrobble) {
        let _ = self.db().execute(
            "INSERT INTO scrobbles (artist, title, album, album_artist, duration, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![scrobble.artist, scrobble.title, scrobble.album, scrobble.album_artist, scrobble.duration, scrobble.at],
        );
    }

    /// The plays waiting for Last.fm, oldest first, at most `limit`.
    pub fn pending_scrobbles(&self, limit: usize) -> Vec<(i64, Scrobble)> {
        let db = self.db();
        let Ok(mut statement) = db.prepare(
            "SELECT id, artist, title, album, album_artist, duration, at FROM scrobbles ORDER BY at LIMIT ?1",
        ) else {
            return Vec::new();
        };
        statement
            .query_map(params![limit as i64], |row| {
                Ok((
                    row.get(0)?,
                    Scrobble {
                        artist: row.get(1)?,
                        title: row.get(2)?,
                        album: row.get(3)?,
                        album_artist: row.get(4)?,
                        duration: row.get(5)?,
                        at: row.get(6)?,
                    },
                ))
            })
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// Forgets plays Last.fm has taken.
    pub fn forget_scrobbles(&self, ids: &[i64]) {
        let db = self.db();
        for id in ids {
            let _ = db.execute("DELETE FROM scrobbles WHERE id = ?1", params![id]);
        }
    }
}

/// One play for Last.fm.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scrobble {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub album_artist: String,
    pub duration: Option<f64>,
    /// When the play started, seconds since 1970.
    pub at: i64,
}

/// What a file's tags say.
#[derive(Clone, Debug, Default, PartialEq)]
struct Tags {
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
    genre: String,
    duration: Option<f64>,
    artwork: Option<PathBuf>,
}

impl Tags {
    /// A file with no tags is called by its name: "01 First Light.flac" is
    /// "First Light", track 1.
    fn from_name(path: &Path) -> Tags {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (track, title) = split_track_number(&stem);
        Tags {
            title,
            track,
            ..Tags::default()
        }
    }
}

/// "01 First Light", "01 - First Light", "1. First Light" into (1, "First Light").
fn split_track_number(stem: &str) -> (Option<u32>, String) {
    let digits: String = stem.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 3 {
        return (None, stem.replace('_', " "));
    }
    let rest = stem[digits.len()..]
        .trim_start_matches([' ', '-', '.', '_'])
        .trim();
    if rest.is_empty() {
        return (None, stem.to_owned());
    }
    (digits.parse().ok(), rest.replace('_', " "))
}

/// Calls `found` for each media file under `root`, skipping hidden folders.
fn walk(root: &Path, found: &mut dyn FnMut(&Path, Kind, i64, i64)) {
    let mut stack = vec![root.to_owned()];
    let mut visited: HashSet<(u64, u64)> = HashSet::new();
    while let Some(dir) = stack.pop() {
        let Ok(meta) = std::fs::metadata(&dir) else {
            continue;
        };
        {
            use std::os::unix::fs::MetadataExt;
            // A link back up the tree is followed once.
            if !visited.insert((meta.dev(), meta.ino())) {
                continue;
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file()
                && let Some(kind) = kind_of(&path)
            {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |time| time.as_secs() as i64);
                found(&path, kind, meta.len() as i64, modified);
            }
        }
    }
}

/// The names of cover pictures kept beside the songs, best first.
const COVER_NAMES: &[&str] = &[
    "cover",
    "folder",
    "front",
    "album",
    "albumart",
    "albumartsmall",
];

/// A cover picture in `dir`, if it has one.
fn cover_beside(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(extension.as_deref(), Some("jpg" | "jpeg" | "png" | "webp")) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let rank = COVER_NAMES.iter().position(|name| {
            stem == *name || (name.starts_with("albumart") && stem.starts_with(name))
        });
        if let Some(rank) = rank
            && best.as_ref().is_none_or(|(best, _)| rank < *best)
        {
            best = Some((rank, path));
        }
    }
    best.map(|(_, path)| path)
}

fn read_tags(
    path: &Path,
    artwork_dir: &Path,
    album_art: &mut HashMap<String, Option<PathBuf>>,
) -> Option<Tags> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::picture::{MimeType, PictureType};
    use lofty::tag::{Accessor, ItemKey};

    let file = lofty::read_from_path(path).ok()?;
    let mut tags = Tags::from_name(path);
    tags.duration =
        Some(file.properties().duration().as_secs_f64()).filter(|duration| *duration > 0.0);
    let tag = file.primary_tag().or_else(|| file.first_tag());
    if let Some(tag) = tag {
        let text = |value: Option<std::borrow::Cow<'_, str>>| {
            value
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        if let Some(title) = text(tag.title()) {
            tags.title = title;
        }
        tags.artist = text(tag.artist()).unwrap_or_default();
        tags.album = text(tag.album()).unwrap_or_default();
        tags.album_artist = tag
            .get_string(&ItemKey::AlbumArtist)
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        tags.track = tag.track().or(tags.track);
        tags.disc = tag.disk();
        tags.year = tag.year();
        tags.genre = text(tag.genre()).unwrap_or_default();
    }
    // Cover art: once per album, from the first file of it that has one.
    let key = album_key(
        &tags.album,
        if tags.album_artist.is_empty() {
            &tags.artist
        } else {
            &tags.album_artist
        },
    );
    let dir_key = format!(
        "{key}\u{1f}{}",
        path.parent()
            .map(|dir| dir.to_string_lossy())
            .unwrap_or_default()
    );
    if let Some(known) = album_art.get(&dir_key) {
        tags.artwork = known.clone();
        if tags.artwork.is_some() {
            return Some(tags);
        }
    }
    let embedded = tag.and_then(|tag| {
        let pictures = tag.pictures();
        pictures
            .iter()
            .find(|picture| picture.pic_type() == PictureType::CoverFront)
            .or_else(|| pictures.first())
            .cloned()
    });
    let artwork = match embedded {
        Some(picture) => {
            let extension = match picture.mime_type() {
                Some(MimeType::Png) => "png",
                Some(MimeType::Gif) => "gif",
                Some(MimeType::Bmp) => "bmp",
                _ => "jpg",
            };
            let name = format!("{:016x}.{extension}", fnv(dir_key.as_bytes()));
            let file = artwork_dir.join(name);
            if !file.exists() {
                let _ = std::fs::create_dir_all(artwork_dir);
                let partial = file.with_extension("part");
                if std::fs::write(&partial, picture.data()).is_ok() {
                    let _ = std::fs::rename(&partial, &file);
                }
            }
            file.exists().then_some(file)
        }
        None => path.parent().and_then(cover_beside),
    };
    album_art.insert(dir_key, artwork.clone());
    tags.artwork = artwork;
    Some(tags)
}

/// FNV-1a, for stable artwork file names.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_without_tags_lose_their_track_numbers() {
        assert_eq!(
            split_track_number("01 First Light"),
            (Some(1), "First Light".to_owned())
        );
        assert_eq!(
            split_track_number("3 - Solar_Wind"),
            (Some(3), "Solar Wind".to_owned())
        );
        assert_eq!(
            split_track_number("2001 A Space Odyssey"),
            (None, "2001 A Space Odyssey".to_owned())
        );
        assert_eq!(
            split_track_number("voice memo"),
            (None, "voice memo".to_owned())
        );
    }

    #[test]
    fn articles_do_not_sort() {
        assert_eq!(sort_name("The Tones"), "tones");
        assert_eq!(sort_name("A"), "a");
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_of(Path::new("/a/b.FLAC")), Some(Kind::Audio));
        assert_eq!(kind_of(Path::new("/a/b.mkv")), Some(Kind::Video));
        assert_eq!(kind_of(Path::new("/a/b.txt")), None);
    }

    #[test]
    fn positions_near_the_ends_are_forgotten() {
        let dir = std::env::temp_dir();
        let library = Library::in_memory(&dir).unwrap();
        let film = Path::new("/films/a.mkv");
        library.set_position(film, Some(600.0), Some(3600.0));
        assert_eq!(library.position(film), Some(600.0));
        library.set_position(film, Some(3590.0), Some(3600.0));
        assert_eq!(library.position(film), None);
        library.set_position(film, Some(5.0), Some(3600.0));
        assert_eq!(library.position(film), None);
    }

    #[test]
    fn a_scan_indexes_and_notices_removals() {
        let root = std::env::temp_dir().join(format!("bingux-media-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Album")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("Album/01 Song.mp3"), b"not really an mp3").unwrap();
        std::fs::write(root.join("Album/clip.mkv"), b"x").unwrap();
        std::fs::write(root.join(".hidden/secret.mp3"), b"x").unwrap();
        std::fs::write(root.join("Album/notes.txt"), b"x").unwrap();
        let library = Library::in_memory(&root.join("art")).unwrap();
        let first = library.scan(std::slice::from_ref(&root), &|_| {});
        assert_eq!(first.added, 2);
        let snapshot = library.snapshot();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].title, "Song");
        assert_eq!(snapshot.tracks[0].track, Some(1));
        assert_eq!(snapshot.videos[0].folder, "Album");
        let again = library.scan(std::slice::from_ref(&root), &|_| {});
        assert_eq!((again.added, again.unchanged), (0, 2));
        std::fs::remove_file(root.join("Album/clip.mkv")).unwrap();
        let after = library.scan(std::slice::from_ref(&root), &|_| {});
        assert_eq!(after.removed, 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
