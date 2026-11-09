//! What survives a restart, in `$XDG_STATE_HOME/bingux/text-editor/`:
//!
//! | File | Holds |
//! | ---- | ----- |
//! | `session.json` | The open tabs in order (a file, or an untitled draft), the one on show, each caret and scroll line |
//! | `drafts/NAME.txt` | The unsaved text of a document, written two seconds after a change, deleted when it is saved or closed without saving |
//! | `positions.json` | The caret and scroll line of the last 500 files, so a file opens where it was left |
//! | `recent.json` | The last 20 files opened, for Open recent |
//!
//! Nothing typed is lost: a document with changes, saved or not, has a draft,
//! and the next start opens it again with its changes. Every write is to a
//! temporary file renamed into place, on a worker.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many files Open recent keeps.
pub const RECENT_KEPT: usize = 20;
/// How many files' caret positions are remembered.
const POSITIONS_KEPT: usize = 500;
/// Drafts larger than this are not written: the file itself is the copy.
pub const DRAFT_LIMIT: usize = 64 << 20;

pub fn folder() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::prefs::home().join(".local/state"));
    base.join("bingux").join("text-editor")
}

fn drafts_folder() -> PathBuf {
    folder().join("drafts")
}

/// One tab of a saved session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedTab {
    pub path: Option<PathBuf>,
    /// The draft holding unsaved text, if there is one.
    pub draft: Option<String>,
    pub untitled: Option<String>,
    pub caret: usize,
    pub line: usize,
    /// A language the person chose, by id.
    pub language: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    pub active: usize,
}

/// Where a file was left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub caret: usize,
    pub line: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Positions {
    files: VecDeque<(PathBuf, Position)>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes `bytes` to `path` through a temporary file. Call it on a worker.
pub fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("part");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

/// The session the last run left. One small file, read before the window.
pub fn load_session() -> Session {
    read_json(&folder().join("session.json"))
}

pub fn save_session(session: &Session) {
    let Ok(bytes) = serde_json::to_vec_pretty(session) else {
        return;
    };
    bingux_ui::task::spawn_detached("text-editor-session", move || {
        let _ = write_file(&folder().join("session.json"), &bytes);
    });
}

/// The text of draft `name`, if it is there. Read before the window opens.
pub fn read_draft(name: &str) -> Option<String> {
    let bytes = std::fs::read(drafts_folder().join(format!("{name}.txt"))).ok()?;
    String::from_utf8(bytes).ok()
}

/// Writes draft `name` on a worker.
pub fn write_draft(name: String, text: String) {
    if text.len() > DRAFT_LIMIT {
        return;
    }
    bingux_ui::task::spawn_detached("text-editor-draft", move || {
        let _ = write_file(
            &drafts_folder().join(format!("{name}.txt")),
            text.as_bytes(),
        );
    });
}

pub fn remove_draft(name: String) {
    bingux_ui::task::spawn_detached("text-editor-draft", move || {
        let _ = std::fs::remove_file(drafts_folder().join(format!("{name}.txt")));
    });
}

/// Drafts no tab of the session names: from a crash before the session was
/// written. Read before the window opens.
pub fn orphan_drafts(session: &Session) -> Vec<String> {
    let named: Vec<&str> = session
        .tabs
        .iter()
        .filter_map(|tab| tab.draft.as_deref())
        .collect();
    let Ok(entries) = std::fs::read_dir(drafts_folder()) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_suffix(".txt")
                .map(str::to_owned)
        })
        .filter(|name| !named.contains(&name.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------
// Recent files and positions, held in memory and written behind.
// ---------------------------------------------------------------------------

thread_local! {
    static RECENT: std::cell::RefCell<Vec<PathBuf>> = std::cell::RefCell::new(read_json::<Vec<PathBuf>>(&folder().join("recent.json")));
    static POSITIONS: std::cell::RefCell<Positions> = std::cell::RefCell::new(read_json(&folder().join("positions.json")));
}

pub fn recent() -> Vec<PathBuf> {
    RECENT.with(|recent| recent.borrow().clone())
}

pub fn add_recent(path: &Path) {
    let list = RECENT.with(|recent| {
        let mut recent = recent.borrow_mut();
        recent.retain(|known| known != path);
        recent.insert(0, path.to_path_buf());
        recent.truncate(RECENT_KEPT);
        recent.clone()
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&list) {
        bingux_ui::task::spawn_detached("text-editor-recent", move || {
            let _ = write_file(&folder().join("recent.json"), &bytes);
        });
    }
}

pub fn clear_recent() {
    RECENT.with(|recent| recent.borrow_mut().clear());
    bingux_ui::task::spawn_detached("text-editor-recent", move || {
        let _ = write_file(&folder().join("recent.json"), b"[]");
    });
}

pub fn position(path: &Path) -> Option<Position> {
    POSITIONS.with(|positions| {
        positions
            .borrow()
            .files
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, position)| *position)
    })
}

pub fn remember_position(path: &Path, position: Position) {
    let bytes = POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        if positions
            .files
            .iter()
            .any(|(known, old)| known == path && *old == position)
        {
            return None;
        }
        positions.files.retain(|(known, _)| known != path);
        positions.files.push_front((path.to_path_buf(), position));
        positions.files.truncate(POSITIONS_KEPT);
        serde_json::to_vec(&*positions).ok()
    });
    if let Some(bytes) = bytes {
        bingux_ui::task::spawn_detached("text-editor-positions", move || {
            let _ = write_file(&folder().join("positions.json"), &bytes);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_round_trip() {
        let session = Session {
            tabs: vec![
                SavedTab {
                    path: Some("/tmp/a.rs".into()),
                    caret: 4,
                    line: 2,
                    ..SavedTab::default()
                },
                SavedTab {
                    draft: Some("x".into()),
                    untitled: Some("Untitled".into()),
                    ..SavedTab::default()
                },
            ],
            active: 1,
        };
        let text = serde_json::to_string(&session).unwrap();
        assert_eq!(serde_json::from_str::<Session>(&text).unwrap(), session);
        // A file from an older version with fields missing still reads.
        assert_eq!(
            serde_json::from_str::<Session>("{\"tabs\":[{}]}")
                .unwrap()
                .tabs
                .len(),
            1
        );
    }
}
