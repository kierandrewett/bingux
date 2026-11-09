//! The local store: calendars as folders of `.ics` files.
//!
//! The layout is the one `vdirsyncer`, `khal` and Thunderbird's "calendar
//! folder" read: one folder per calendar, one `UID.ics` per event, and the
//! plain files `displayname` and `color` beside them. Another program can
//! add, change or remove a file and the app follows (see [`Store::fingerprint`]).
//!
//! # Failure modes
//!
//! What can go wrong with a folder of files that other programs also write,
//! what the store does, and the test that holds it.
//!
//! | Failure | What happens | Test |
//! | ------- | ------------ | ---- |
//! | First run: no folder at all | "Personal" is made | `a_first_run_makes_personal` |
//! | A file that is not iCalendar | It is skipped with a warning; the other files load | `a_file_that_is_not_a_calendar_does_not_hide_the_others` |
//! | The same UID in two files (a sync conflict, a copied file) | The first file read wins, the other is named in a warning, so a view never shows it twice | `a_uid_in_two_files_is_one_event` |
//! | An event moved to another calendar | One copy is left | `moving_between_calendars_leaves_one_copy` |
//! | No `displayname`, or a colour that is not a colour | The folder's name; no colour | `a_calendar_without_a_name_or_colour_still_loads` |
//! | Files that are not events: a text file, a hidden folder, an `.etag` | Ignored | `other_files_and_hidden_folders_are_ignored` |
//! | A `readonly` marker | The calendar is read-only for the app | `a_readonly_marker_makes_a_read_only_calendar` |
//! | A crash while writing | A reader sees the old file or the new one, never half; no temporary file is left | `a_write_leaves_no_temporary_file` |
//! | Another program changes a file | The fingerprint changes and the window reads again | `the_fingerprint_moves_when_a_file_does` |
//! | A UID that is a path (`../x`) | The file name is made safe, so nothing is written outside the folder | `file_names_are_safe` |
//! | Deleting an event whose file is already gone | Nothing happens, and no error | `deleting_what_is_not_there_is_fine` |
//! | Importing a file twice | Events are replaced by UID, and counted as updated | `import_replaces_by_uid_and_counts` |
//!
//! ```text
//! ~/.local/share/bingux-calendar/calendars/
//!     personal/
//!         displayname          Personal
//!         color                #3584e4
//!         20250301T100000-0001-a1b2@bingux.ics
//! ```

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Duration, NaiveDateTime};

use crate::event::{Item, Occurrence};
use crate::ics;
use crate::recur::{self, Change};
use crate::zone::Zone;

/// Where a calendar comes from, when it is not just a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    /// The account's id in `accounts.json`.
    pub account: String,
    /// The calendar's collection URL on the server.
    pub href: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Calendar {
    /// The folder's name.
    pub id: String,
    pub name: String,
    /// `#rrggbb` as `color` holds it; `None` lets the app choose.
    pub colour: Option<String>,
    pub read_only: bool,
    pub remote: Option<Remote>,
}

/// Everything in the store at one moment. Cheap to clone: the items are
/// shared, so a change copies the list and one item.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub calendars: Vec<Calendar>,
    pub items: Vec<Arc<Item>>,
}

impl Snapshot {
    pub fn calendar(&self, id: &str) -> Option<&Calendar> {
        self.calendars.iter().find(|calendar| calendar.id == id)
    }

    pub fn item(&self, uid: &str) -> Option<&Arc<Item>> {
        self.items.iter().find(|item| item.uid == uid)
    }

    /// The occurrences touching `from..to` (wall-clock on `view`), in the
    /// calendars not in `hidden`, in order of start.
    pub fn occurrences(
        &self,
        from: NaiveDateTime,
        to: NaiveDateTime,
        view: Zone,
        hidden: &HashSet<String>,
    ) -> Vec<Occurrence> {
        let mut all: Vec<Occurrence> = self
            .items
            .iter()
            .filter(|item| !hidden.contains(&item.calendar))
            .flat_map(|item| recur::occurrences(item, from, to, view))
            .collect();
        all.sort_by(|a, b| {
            b.all_day
                .cmp(&a.all_day)
                .then(a.start.cmp(&b.start))
                .then(b.end.cmp(&a.end))
                .then(a.summary.cmp(&b.summary))
        });
        all
    }

    /// The snapshot after `changes`, with no disk involved.
    pub fn apply(&self, changes: &[Change]) -> Snapshot {
        let mut next = self.clone();
        for change in changes {
            match change {
                Change::Save(item) => {
                    let item = Arc::new(item.clone());
                    match next
                        .items
                        .iter()
                        .position(|existing| existing.uid == item.uid)
                    {
                        Some(at) => next.items[at] = item,
                        None => next.items.push(item),
                    }
                }
                Change::Delete(uid) => next.items.retain(|item| item.uid != *uid),
            }
        }
        next
    }

    /// Occurrences whose title, place or notes contain `query` (every word
    /// of it, in any case), from `from` to `to`, nearest to `near` first.
    pub fn search(
        &self,
        query: &str,
        from: NaiveDateTime,
        to: NaiveDateTime,
        near: NaiveDateTime,
        view: Zone,
        hidden: &HashSet<String>,
        limit: usize,
    ) -> Vec<Occurrence> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<Occurrence> = Vec::new();
        for item in self
            .items
            .iter()
            .filter(|item| !hidden.contains(&item.calendar))
        {
            let hay = format!(
                "{}\n{}\n{}",
                item.event.summary.to_lowercase(),
                item.event.location.to_lowercase(),
                item.event.description.to_lowercase()
            );
            let hay_exceptions: String = item
                .exceptions
                .iter()
                .map(|exception| {
                    format!(
                        "{} {}",
                        exception.event.summary.to_lowercase(),
                        exception.event.location.to_lowercase()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !words
                .iter()
                .all(|word| hay.contains(word) || hay_exceptions.contains(word))
            {
                continue;
            }
            for occurrence in recur::occurrences(item, from, to, view) {
                let text = format!(
                    "{}\n{}",
                    occurrence.summary.to_lowercase(),
                    occurrence.location.to_lowercase()
                );
                if words
                    .iter()
                    .all(|word| text.contains(word) || hay.contains(word))
                {
                    found.push(occurrence);
                }
            }
        }
        found.sort_by_key(|occurrence| (occurrence.start - near).num_seconds().abs());
        found.truncate(limit);
        found.sort_by_key(|occurrence| occurrence.start);
        found
    }
}

/// What reading the store produced.
#[derive(Debug, Default)]
pub struct Loaded {
    pub snapshot: Snapshot,
    /// Files that could not be read, and things that were read forgivingly.
    pub warnings: Vec<String>,
}

/// What importing a file did.
#[derive(Debug, Default)]
pub struct Imported {
    pub added: usize,
    pub updated: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

/// The id of the calendar a first launch makes.
pub const DEFAULT_CALENDAR: &str = "personal";

impl Store {
    /// The store in the user's data folder: `$XDG_DATA_HOME/bingux-calendar`.
    pub fn open() -> Store {
        Store::at(data_dir())
    }

    /// A store in `root`, for tests and for `--data`.
    pub fn at(root: impl Into<PathBuf>) -> Store {
        Store { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn calendars_dir(&self) -> PathBuf {
        self.root.join("calendars")
    }

    fn calendar_dir(&self, id: &str) -> PathBuf {
        self.calendars_dir().join(sanitise(id))
    }

    /// Reads every calendar. A first run finds none and makes "Personal".
    pub fn load(&self) -> Loaded {
        let mut loaded = Loaded::default();
        let dir = self.calendars_dir();
        let mut ids: Vec<String> = match fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| !name.starts_with('.'))
                .collect(),
            Err(_) => Vec::new(),
        };
        if ids.is_empty() {
            if self
                .create_calendar_with_id(DEFAULT_CALENDAR, "Personal", Some("#3584e4"))
                .is_ok()
            {
                ids.push(DEFAULT_CALENDAR.to_string());
            }
        }
        ids.sort();
        // The file each UID was first read from.
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for id in ids {
            let folder = dir.join(&id);
            let name = read_trimmed(&folder.join("displayname")).unwrap_or_else(|| id.clone());
            let colour =
                read_trimmed(&folder.join("color")).and_then(|text| normalise_colour(&text));
            let remote = read_remote(&folder);
            let read_only = folder.join("readonly").exists();
            loaded.snapshot.calendars.push(Calendar {
                id: id.clone(),
                name,
                colour,
                read_only,
                remote,
            });
            let mut files: Vec<PathBuf> = match fs::read_dir(&folder) {
                Ok(entries) => entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ics"))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            files.sort();
            for file in files {
                let file_label = file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text = match fs::read(&file) {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                    Err(_) => {
                        loaded.warnings.push(format!("{name} could not be read."));
                        continue;
                    }
                };
                match ics::parse(&text, &id) {
                    Ok(parsed) => {
                        loaded
                            .warnings
                            .extend(parsed.warnings.into_iter().map(|w| format!("{name}: {w}")));
                        for mut item in parsed.items {
                            if let Some(first) = seen.get(&item.uid) {
                                // A second file for an event already read: a
                                // view must not draw it twice, and an edit
                                // finds events by UID.
                                if *first != file_label {
                                    loaded.warnings.push(format!(
                                        "{file_label} repeats an event that {first} already has, so it is left out."
                                    ));
                                }
                                continue;
                            }
                            seen.insert(item.uid.clone(), file_label.clone());
                            // Keep the server's tag with the item.
                            item.etag = read_trimmed(&file.with_extension("etag"));
                            loaded.snapshot.items.push(Arc::new(item));
                        }
                    }
                    Err(error) => loaded
                        .warnings
                        .push(format!("{name} is not a calendar file ({error}).")),
                }
            }
        }
        loaded
    }

    /// Writes the changes to disk. A save goes to the folder of the item's
    /// calendar and removes the same event from any other folder, so moving
    /// an event between calendars leaves one copy.
    pub fn write(&self, changes: &[Change]) -> io::Result<()> {
        for change in changes {
            match change {
                Change::Save(item) => {
                    let folder = self.calendar_dir(&item.calendar);
                    fs::create_dir_all(&folder)?;
                    let name = file_name(&item.uid);
                    write_atomic(&folder.join(&name), ics::write(item).as_bytes())?;
                    if let Some(tag) = &item.etag {
                        write_atomic(&folder.join(&name).with_extension("etag"), tag.as_bytes())?;
                    }
                    for other in self.calendar_ids() {
                        if other != sanitise(&item.calendar) {
                            let stale = self.calendars_dir().join(&other).join(&name);
                            let _ = fs::remove_file(&stale);
                            let _ = fs::remove_file(stale.with_extension("etag"));
                        }
                    }
                }
                Change::Delete(uid) => {
                    let name = file_name(uid);
                    for id in self.calendar_ids() {
                        let file = self.calendars_dir().join(&id).join(&name);
                        let _ = fs::remove_file(&file);
                        let _ = fs::remove_file(file.with_extension("etag"));
                    }
                }
            }
        }
        Ok(())
    }

    fn calendar_ids(&self) -> Vec<String> {
        fs::read_dir(self.calendars_dir())
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().is_dir())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Adds the events of `text` to calendar `calendar`. An event whose UID
    /// is already there replaces it.
    pub fn import(
        &self,
        text: &str,
        calendar: &str,
        snapshot: &Snapshot,
    ) -> Result<(Imported, Vec<Change>), String> {
        let parsed = ics::parse(text, calendar)?;
        let mut report = Imported {
            warnings: parsed.warnings,
            ..Imported::default()
        };
        let mut changes = Vec::new();
        for item in parsed.items {
            let existing = snapshot
                .item(&item.uid)
                .is_some_and(|existing| existing.calendar == calendar);
            if existing {
                report.updated += 1;
            } else {
                report.added += 1;
            }
            changes.push(Change::Save(item));
        }
        Ok((report, changes))
    }

    /// Makes a calendar named `name`, with an id from the name.
    pub fn create_calendar(
        &self,
        name: &str,
        colour: Option<&str>,
        existing: &[Calendar],
    ) -> io::Result<Calendar> {
        let base = {
            let id = sanitise(&name.to_lowercase().replace(' ', "-"));
            if id.is_empty() {
                "calendar".to_string()
            } else {
                id
            }
        };
        let mut id = base.clone();
        let mut number = 2;
        while existing.iter().any(|calendar| calendar.id == id) || self.calendar_dir(&id).exists() {
            id = format!("{base}-{number}");
            number += 1;
        }
        self.create_calendar_with_id(&id, name, colour)
    }

    fn create_calendar_with_id(
        &self,
        id: &str,
        name: &str,
        colour: Option<&str>,
    ) -> io::Result<Calendar> {
        let folder = self.calendar_dir(id);
        fs::create_dir_all(&folder)?;
        write_atomic(&folder.join("displayname"), name.as_bytes())?;
        if let Some(colour) = colour {
            write_atomic(&folder.join("color"), colour.as_bytes())?;
        }
        Ok(Calendar {
            id: id.to_string(),
            name: name.to_string(),
            colour: colour.and_then(normalise_colour),
            read_only: false,
            remote: None,
        })
    }

    pub fn rename_calendar(&self, id: &str, name: &str) -> io::Result<()> {
        write_atomic(&self.calendar_dir(id).join("displayname"), name.as_bytes())
    }

    pub fn set_calendar_colour(&self, id: &str, colour: &str) -> io::Result<()> {
        write_atomic(&self.calendar_dir(id).join("color"), colour.as_bytes())
    }

    /// Removes a calendar and every event in it.
    pub fn delete_calendar(&self, id: &str) -> io::Result<()> {
        fs::remove_dir_all(self.calendar_dir(id))
    }

    /// A number that changes when anything under the store changes: file
    /// names, sizes and modification times, combined. Cheap enough to ask
    /// every few seconds; a change means "read the store again".
    pub fn fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x100000001b3);
        };
        let mut visit = |path: &Path, mix: &mut dyn FnMut(u64)| {
            if let Ok(meta) = fs::metadata(path) {
                mix(meta.len());
                if let Ok(modified) = meta.modified() {
                    if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
                        mix(since.as_nanos() as u64);
                    }
                }
            }
            for byte in path.to_string_lossy().bytes() {
                mix(u64::from(byte));
            }
        };
        let dir = self.calendars_dir();
        visit(&dir, &mut mix);
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut folders: Vec<PathBuf> =
                entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            folders.sort();
            for folder in folders {
                visit(&folder, &mut mix);
                if let Ok(files) = fs::read_dir(&folder) {
                    let mut files: Vec<PathBuf> =
                        files.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                    files.sort();
                    for file in files {
                        visit(&file, &mut mix);
                    }
                }
            }
        }
        hash
    }

    /// The events as one file, for an export.
    pub fn export(&self, snapshot: &Snapshot, calendar: &str) -> String {
        let items: Vec<Item> = snapshot
            .items
            .iter()
            .filter(|item| item.calendar == calendar)
            .map(|item| (**item).clone())
            .collect();
        ics::write_many(&items)
    }
}

/// `$XDG_DATA_HOME/bingux-calendar`, with the default `~/.local/share`.
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("bingux-calendar")
}

/// `$XDG_CONFIG_HOME/bingux-calendar`.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("bingux-calendar")
}

/// `$XDG_STATE_HOME/bingux-calendar`.
pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("bingux-calendar")
}

fn xdg(variable: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(variable) {
        Some(value) if !value.is_empty() && Path::new(&value).is_absolute() => PathBuf::from(value),
        _ => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(fallback),
    }
}

/// A name safe as a file or folder name: letters, digits and `._@-`.
pub fn sanitise(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '@' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_start_matches('.')
        .to_string()
}

fn file_name(uid: &str) -> String {
    format!("{}.ics", sanitise(uid))
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn read_remote(folder: &Path) -> Option<Remote> {
    let text = fs::read_to_string(folder.join("remote.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(Remote {
        account: value.get("account")?.as_str()?.to_string(),
        href: value.get("href")?.as_str()?.to_string(),
    })
}

/// `#rrggbb` from `#rrggbb`, `#rrggbbaa` (the alpha dropped) or `rrggbb`.
pub fn normalise_colour(text: &str) -> Option<String> {
    let hex = text.trim().trim_start_matches('#');
    let hex = if hex.len() == 8 { &hex[..6] } else { hex };
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_lowercase()))
    } else {
        None
    }
}

/// Writes through a temporary file and renames it, so a reader never sees a
/// half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all().ok();
    }
    fs::rename(&temp, path)
}

/// Default span searched when a view asks for "everything near now".
pub fn search_span() -> (Duration, Duration) {
    (Duration::days(365), Duration::days(730))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::event::Event;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bingux-calendar-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn day(d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 3, d)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    #[test]
    fn a_first_run_makes_personal() {
        let store = Store::at(temp("first"));
        let loaded = store.load();
        assert_eq!(loaded.snapshot.calendars.len(), 1);
        assert_eq!(loaded.snapshot.calendars[0].name, "Personal");
        assert!(loaded.snapshot.items.is_empty());
    }

    #[test]
    fn saved_events_are_read_back_and_deleted() {
        let store = Store::at(temp("save"));
        let snapshot = store.load().snapshot;
        let item = Item::new(
            "a@test",
            DEFAULT_CALENDAR,
            Event::new("Coffee", day(3), Zone::Utc),
        );
        store.write(&[Change::Save(item)]).unwrap();
        let again = store.load().snapshot;
        assert_eq!(again.items.len(), 1);
        assert_eq!(again.items[0].event.summary, "Coffee");
        store.write(&[Change::Delete("a@test".into())]).unwrap();
        assert!(store.load().snapshot.items.is_empty());
        drop(snapshot);
    }

    #[test]
    fn moving_between_calendars_leaves_one_copy() {
        let store = Store::at(temp("move"));
        let mut snapshot = store.load().snapshot;
        let work = store
            .create_calendar("Work", Some("#33d17a"), &snapshot.calendars)
            .unwrap();
        snapshot.calendars.push(work.clone());
        let mut item = Item::new(
            "m@test",
            DEFAULT_CALENDAR,
            Event::new("x", day(3), Zone::Utc),
        );
        store.write(&[Change::Save(item.clone())]).unwrap();
        item.calendar = work.id.clone();
        store.write(&[Change::Save(item)]).unwrap();
        let again = store.load().snapshot;
        assert_eq!(again.items.len(), 1);
        assert_eq!(again.items[0].calendar, "work");
        assert_eq!(
            again.calendar("work").unwrap().colour.as_deref(),
            Some("#33d17a")
        );
    }

    #[test]
    fn the_fingerprint_moves_when_a_file_does() {
        let store = Store::at(temp("print"));
        let _ = store.load();
        let before = store.fingerprint();
        assert_eq!(before, store.fingerprint());
        store
            .write(&[Change::Save(Item::new(
                "p@test",
                DEFAULT_CALENDAR,
                Event::new("x", day(3), Zone::Utc),
            ))])
            .unwrap();
        assert_ne!(before, store.fingerprint());
    }

    #[test]
    fn import_replaces_by_uid_and_counts() {
        let store = Store::at(temp("import"));
        let snapshot = store.load().snapshot;
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:i1\nSUMMARY:One\nDTSTART:20250303T090000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        let (report, changes) = store.import(text, DEFAULT_CALENDAR, &snapshot).unwrap();
        assert_eq!((report.added, report.updated), (1, 0));
        let applied = snapshot.apply(&changes);
        let (report, _) = store.import(text, DEFAULT_CALENDAR, &applied).unwrap();
        assert_eq!((report.added, report.updated), (0, 1));
    }

    #[test]
    fn search_finds_words_in_any_order() {
        let mut snapshot = Snapshot::default();
        snapshot.items.push(Arc::new(Item::new(
            "s1",
            "c",
            Event::new("Lunch with Sam", day(5), Zone::Utc),
        )));
        snapshot.items.push(Arc::new(Item::new(
            "s2",
            "c",
            Event::new("Dentist", day(6), Zone::Utc),
        )));
        let from = day(1);
        let to = day(30);
        let found = snapshot.search(
            "sam LUNCH",
            from,
            to,
            day(4),
            Zone::Utc,
            &HashSet::new(),
            10,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].summary, "Lunch with Sam");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("../evil/uid"), "_evil_uid.ics");
    }

    fn event_text(uid: &str, summary: &str) -> String {
        format!(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:{uid}\nSUMMARY:{summary}\nDTSTART:20250303T090000Z\nEND:VEVENT\nEND:VCALENDAR\n"
        )
    }

    #[test]
    fn a_file_that_is_not_a_calendar_does_not_hide_the_others() {
        let root = temp("garbage");
        let store = Store::at(root.clone());
        let _ = store.load();
        let folder = store.calendar_dir(DEFAULT_CALENDAR);
        fs::write(folder.join("good.ics"), event_text("good@test", "Good")).unwrap();
        fs::write(folder.join("bad.ics"), "this is not a calendar").unwrap();
        let loaded = store.load();
        assert_eq!(loaded.snapshot.items.len(), 1);
        assert_eq!(loaded.snapshot.items[0].event.summary, "Good");
        assert!(loaded.warnings.iter().any(|w| w.starts_with("bad.ics")));
    }

    #[test]
    fn a_uid_in_two_files_is_one_event() {
        let store = Store::at(temp("twice"));
        let _ = store.load();
        let folder = store.calendar_dir(DEFAULT_CALENDAR);
        fs::write(folder.join("a.ics"), event_text("same@test", "First")).unwrap();
        fs::write(folder.join("b.ics"), event_text("same@test", "Second")).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.snapshot.items.len(), 1);
        assert_eq!(loaded.snapshot.items[0].event.summary, "First");
        assert!(loaded.warnings.iter().any(|w| w.starts_with("b.ics")));
    }

    #[test]
    fn a_calendar_without_a_name_or_colour_still_loads() {
        let store = Store::at(temp("bare"));
        let folder = store.calendar_dir("holidays");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("color"), "not a colour").unwrap();
        let calendars = store.load().snapshot.calendars;
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].name, "holidays");
        assert_eq!(calendars[0].colour, None);
    }

    #[test]
    fn other_files_and_hidden_folders_are_ignored() {
        let store = Store::at(temp("other"));
        let _ = store.load();
        let folder = store.calendar_dir(DEFAULT_CALENDAR);
        fs::write(folder.join("notes.txt"), "not an event").unwrap();
        fs::write(folder.join("x.etag"), "\"tag\"").unwrap();
        fs::create_dir_all(store.calendars_dir().join(".trash")).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.snapshot.calendars.len(), 1);
        assert!(loaded.snapshot.items.is_empty());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn a_readonly_marker_makes_a_read_only_calendar() {
        let store = Store::at(temp("readonly"));
        let _ = store.load();
        fs::write(store.calendar_dir(DEFAULT_CALENDAR).join("readonly"), "").unwrap();
        assert!(store.load().snapshot.calendars[0].read_only);
    }

    #[test]
    fn a_write_leaves_no_temporary_file() {
        let store = Store::at(temp("atomic"));
        let _ = store.load();
        store
            .write(&[Change::Save(Item::new(
                "w@test",
                DEFAULT_CALENDAR,
                Event::new("x", day(3), Zone::Utc),
            ))])
            .unwrap();
        let names: Vec<String> = fs::read_dir(store.calendar_dir(DEFAULT_CALENDAR))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.contains(".tmp")), "{names:?}");
    }

    #[test]
    fn deleting_what_is_not_there_is_fine() {
        let store = Store::at(temp("gone"));
        let _ = store.load();
        assert!(
            store
                .write(&[Change::Delete("nothing@test".into())])
                .is_ok()
        );
    }
}
