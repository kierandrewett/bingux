//! Unsaved work, kept on disk so that a crash or a power cut loses none of it.
//!
//! A document with unsaved changes writes a *draft* a few seconds after the
//! last edit: its content and a little about it (name, where it came from,
//! whatever else the app wants back). Saving or closing without saving
//! removes the draft. A draft that is still there when the app starts, and
//! whose process is gone, is what a crash left behind: the app offers it back
//! ([`Drafts::recoverable`], and the dialog in [`super::dialogs`]).
//!
//! Drafts live in `$XDG_STATE_HOME/<app-id>/drafts/` (`~/.local/state/...`
//! when that is unset; `BINGUX_DRAFTS_DIR` names another folder, for tests and
//! documentation captures): `<id>.draft` holds the content, written whole
//! with [`write_atomic`], and `<id>.json` the record, written after it. A
//! draft with no record, or a record with no content, was torn by a crash
//! between the two writes and is ignored (and removed by [`Drafts::prune`]).
//!
//! A record names the process that owns it (its id and start time, so that a
//! new process with a reused id is not taken for it): a second window or a
//! second copy of the app never offers a draft its sibling is still using.
//!
//! Every function reads or writes the disk: call it on a worker.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::stamp::Stamp;
use super::write::write_atomic;
use crate::format::Moment;

/// The format number of a record. A record with another number is ignored.
const FORMAT: u32 = 1;

/// What a draft says about its document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftMeta {
    /// The name the document shows ("Untitled", "notes.txt").
    pub title: String,
    /// The file the document was opened from or saved to; `None` for one
    /// that was never saved.
    pub path: Option<PathBuf>,
    /// The file as it was when the document was read or last saved. A draft
    /// whose file has changed since is offered with that said.
    pub base: Option<Stamp>,
    /// Whatever else the app wants back: a caret, a scroll position, an
    /// encoding.
    pub extra: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct Record {
    format: u32,
    owner_pid: u32,
    owner_start: Option<u64>,
    saved_at: i64,
    len: u64,
    title: String,
    path: Option<Vec<u8>>,
    base: Option<Stamp>,
    #[serde(default)]
    extra: serde_json::Value,
}

/// A draft as it is on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub id: String,
    pub meta: DraftMeta,
    /// When the draft was written.
    pub saved_at: Moment,
    /// The size of the content in bytes.
    pub len: u64,
    owner: Option<(u32, Option<u64>)>,
}

impl Draft {
    /// Whether the process that wrote the draft is still running.
    pub fn is_owned_by_a_running_process(&self) -> bool {
        self.owner
            .is_some_and(|(pid, start)| process_start(pid).is_some() && process_start(pid) == start)
    }
}

/// The start time of process `pid` in clock ticks since boot, `None` when
/// there is no such process.
fn process_start(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and may hold spaces: count the
    // fields from the last `)`. Field 3 is the state; field 22 the start.
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(19)?.parse().ok()
}

/// The drafts of one app.
#[derive(Clone, Debug)]
pub struct Drafts {
    dir: PathBuf,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Drafts {
    /// The drafts folder of the app `app_id`.
    pub fn open(app_id: &str) -> Drafts {
        if let Some(dir) = std::env::var_os("BINGUX_DRAFTS_DIR") {
            return Drafts::in_dir(PathBuf::from(dir));
        }
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .unwrap_or_else(std::env::temp_dir);
        Drafts::in_dir(base.join(app_id).join("drafts"))
    }

    /// The drafts kept in `dir`.
    pub fn in_dir(dir: impl Into<PathBuf>) -> Drafts {
        Drafts { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// An id no other draft has: made of the time, the process and a counter.
    pub fn new_id() -> String {
        let now = Moment::now().unix_seconds();
        format!(
            "{now:x}-{:x}-{:x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// An id is letters, digits, `-` and `_`: never a path.
    fn valid(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }

    fn content_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.draft"))
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Writes the draft `id`: the content, then its record. Replaces what was
    /// there.
    pub fn put(&self, id: &str, meta: &DraftMeta, content: &[u8]) -> io::Result<()> {
        self.write(id, meta, content, Moment::now(), std::process::id())
    }

    pub(crate) fn write(
        &self,
        id: &str,
        meta: &DraftMeta,
        content: &[u8],
        at: Moment,
        pid: u32,
    ) -> io::Result<()> {
        if !Drafts::valid(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a draft id",
            ));
        }
        std::fs::create_dir_all(&self.dir)?;
        let other = |error: super::SaveError| io::Error::other(format!("{error:?}"));
        write_atomic(&self.content_path(id), content).map_err(other)?;
        let record = Record {
            format: FORMAT,
            owner_pid: pid,
            owner_start: process_start(pid),
            saved_at: at.unix_seconds(),
            len: content.len() as u64,
            title: meta.title.clone(),
            path: meta
                .path
                .as_ref()
                .map(|path| path.as_os_str().as_bytes().to_vec()),
            base: meta.base,
            extra: meta.extra.clone(),
        };
        let text = serde_json::to_vec(&record).map_err(io::Error::other)?;
        write_atomic(&self.record_path(id), &text).map_err(other)?;
        Ok(())
    }

    /// The content of draft `id`.
    pub fn bytes(&self, id: &str) -> io::Result<Vec<u8>> {
        if !Drafts::valid(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a draft id",
            ));
        }
        std::fs::read(self.content_path(id))
    }

    /// Removes draft `id`. Nothing happens when there is none.
    pub fn remove(&self, id: &str) {
        if Drafts::valid(id) {
            let _ = std::fs::remove_file(self.record_path(id));
            let _ = std::fs::remove_file(self.content_path(id));
        }
    }

    /// Makes this process the owner of draft `id`, as a document that was
    /// recovered from it is: no other copy of the app offers it again.
    pub fn adopt(&self, id: &str) -> io::Result<()> {
        let draft = self
            .read(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such draft"))?;
        let content = self.bytes(id)?;
        self.put(id, &draft.meta, &content)
    }

    fn read(&self, id: &str) -> Option<Draft> {
        if !Drafts::valid(id) {
            return None;
        }
        let record: Record =
            serde_json::from_slice(&std::fs::read(self.record_path(id)).ok()?).ok()?;
        if record.format != FORMAT || !self.content_path(id).is_file() {
            return None;
        }
        Some(Draft {
            id: id.to_string(),
            meta: DraftMeta {
                title: record.title,
                path: record
                    .path
                    .map(|bytes| PathBuf::from(OsString::from_vec(bytes))),
                base: record.base,
                extra: record.extra,
            },
            saved_at: Moment::from_unix(record.saved_at),
            len: record.len,
            owner: Some((record.owner_pid, record.owner_start)),
        })
    }

    /// Every draft with a readable record and content, newest first,
    /// whoever owns it.
    pub fn all(&self) -> Vec<Draft> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut drafts: Vec<Draft> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let id = name.to_str()?.strip_suffix(".json")?.to_string();
                self.read(&id)
            })
            .collect();
        drafts.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.id.cmp(&b.id)));
        drafts
    }

    /// The drafts a crash left: those whose process is gone, newest first.
    pub fn recoverable(&self) -> Vec<Draft> {
        self.all()
            .into_iter()
            .filter(|draft| !draft.is_owned_by_a_running_process())
            .collect()
    }

    /// Removes what can no longer be recovered: content with no record or a
    /// record with no content (a crash between the two writes), records this
    /// build cannot read, and drafts older than `max_age` whose process is
    /// gone.
    pub fn prune(&self, max_age: Duration) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let now = Moment::now().unix_seconds();
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = name
                .strip_suffix(".json")
                .or_else(|| name.strip_suffix(".draft"))
            else {
                continue;
            };
            match self.read(id) {
                None => {
                    // Leave a hidden temporary of a write in progress alone:
                    // only files that are whole names of ours are removed.
                    if Drafts::valid(id) {
                        self.remove(id);
                    }
                }
                Some(draft) => {
                    let age = now.saturating_sub(draft.saved_at.unix_seconds());
                    if !draft.is_owned_by_a_running_process() && age > max_age.as_secs() as i64 {
                        self.remove(id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> Drafts {
        let dir = std::env::temp_dir().join(format!("bingux-drafts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Drafts::in_dir(dir)
    }

    fn meta(title: &str) -> DraftMeta {
        DraftMeta {
            title: title.into(),
            path: Some(PathBuf::from("/home/a/notes.txt")),
            base: None,
            extra: serde_json::json!({"caret": 12}),
        }
    }

    /// A process id that is certainly not running: a child that has ended.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn a_draft_round_trips_with_its_record() {
        let drafts = scratch("round");
        let id = Drafts::new_id();
        drafts
            .put(&id, &meta("notes.txt"), b"hello\nworld")
            .unwrap();
        assert_eq!(drafts.bytes(&id).unwrap(), b"hello\nworld");
        let all = drafts.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].meta, meta("notes.txt"));
        assert_eq!(all[0].len, 11);
        drafts.remove(&id);
        assert!(drafts.all().is_empty());
        assert!(drafts.bytes(&id).is_err());
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn a_draft_of_this_process_is_not_offered_but_one_of_a_dead_process_is() {
        let drafts = scratch("owner");
        drafts.put("mine", &meta("mine"), b"a").unwrap();
        drafts
            .write(
                "theirs",
                &meta("theirs"),
                b"b",
                Moment::from_unix(100),
                dead_pid(),
            )
            .unwrap();
        let offered: Vec<_> = drafts.recoverable().into_iter().map(|d| d.id).collect();
        assert_eq!(offered, ["theirs"]);
        // Recovering takes it over: it is not offered again.
        drafts.adopt("theirs").unwrap();
        assert!(drafts.recoverable().is_empty());
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn a_reused_process_id_is_not_taken_for_the_owner() {
        let drafts = scratch("reuse");
        drafts.put("d", &meta("d"), b"a").unwrap();
        // The same process id with another start time is another process.
        let mut record: Record =
            serde_json::from_slice(&std::fs::read(drafts.record_path("d")).unwrap()).unwrap();
        record.owner_start = record.owner_start.map(|start| start + 1);
        std::fs::write(
            drafts.record_path("d"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert_eq!(drafts.recoverable().len(), 1);
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn the_newest_draft_comes_first() {
        let drafts = scratch("order");
        for (id, at) in [("a", 300), ("b", 100), ("c", 200)] {
            drafts
                .write(id, &meta(id), b"x", Moment::from_unix(at), dead_pid())
                .unwrap();
        }
        let ids: Vec<_> = drafts.recoverable().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, ["a", "c", "b"]);
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn torn_corrupt_and_foreign_drafts_are_ignored_and_pruned() {
        let drafts = scratch("torn");
        let dir = drafts.dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        // Content with no record, a record with no content, a record that is
        // not JSON, and one of another format.
        std::fs::write(dir.join("nojson.draft"), "x").unwrap();
        drafts.put("nocontent", &meta("x"), b"x").unwrap();
        std::fs::remove_file(dir.join("nocontent.draft")).unwrap();
        std::fs::write(dir.join("junk.json"), "{{{").unwrap();
        std::fs::write(dir.join("junk.draft"), "x").unwrap();
        drafts.put("future", &meta("x"), b"x").unwrap();
        let mut record: Record =
            serde_json::from_slice(&std::fs::read(dir.join("future.json")).unwrap()).unwrap();
        record.format = 99;
        std::fs::write(
            dir.join("future.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        drafts
            .write("good", &meta("good"), b"x", Moment::now(), dead_pid())
            .unwrap();
        assert_eq!(drafts.all().len(), 1);
        drafts.prune(Duration::from_secs(30 * 86_400));
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["good.draft", "good.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_drafts_of_dead_processes_are_pruned_but_not_young_or_running_ones() {
        let drafts = scratch("age");
        drafts
            .write(
                "old",
                &meta("old"),
                b"x",
                Moment::from_unix(1_000),
                dead_pid(),
            )
            .unwrap();
        drafts
            .write("young", &meta("young"), b"x", Moment::now(), dead_pid())
            .unwrap();
        drafts
            .write(
                "running-old",
                &meta("r"),
                b"x",
                Moment::from_unix(1_000),
                std::process::id(),
            )
            .unwrap();
        drafts.prune(Duration::from_secs(30 * 86_400));
        let mut ids: Vec<_> = drafts.all().into_iter().map(|d| d.id).collect();
        ids.sort();
        assert_eq!(ids, ["running-old", "young"]);
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn an_id_that_is_a_path_is_refused() {
        let drafts = scratch("ids");
        for id in ["", "../escape", "a/b", "a b", "ü", &"x".repeat(200)] {
            assert!(drafts.put(id, &meta("x"), b"x").is_err(), "{id:?}");
            assert!(drafts.bytes(id).is_err());
            drafts.remove(id);
        }
        assert!(
            !drafts.dir().exists() || std::fs::read_dir(drafts.dir()).unwrap().next().is_none()
        );
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn a_path_that_is_not_text_survives() {
        let drafts = scratch("bytes");
        let odd = PathBuf::from(OsString::from_vec(b"/tmp/caf\xe9.txt".to_vec()));
        let mut meta = meta("x");
        meta.path = Some(odd.clone());
        drafts.put("d", &meta, b"x").unwrap();
        assert_eq!(drafts.all()[0].meta.path, Some(odd));
        let _ = std::fs::remove_dir_all(drafts.dir());
    }

    #[test]
    fn ids_are_unique() {
        let ids: std::collections::HashSet<_> = (0..1000).map(|_| Drafts::new_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| Drafts::valid(id)));
    }
}
