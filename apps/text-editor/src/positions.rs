//! Where each file was left: the caret and the first line on screen of the
//! last 500 files, so a file opens where it was last read. Kept in
//! `$XDG_STATE_HOME/bingux/text-editor/positions.json`, held in memory and
//! written behind (to a temporary file renamed into place, on a worker).
//!
//! The documents themselves (which are open, their unsaved text, their
//! caret) are `bingux_ui::document`'s: this is only the memory of files that
//! are not open.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many files' positions are remembered.
const KEPT: usize = 500;

fn folder() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::prefs::home().join(".local/state"));
    base.join("bingux").join("text-editor")
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

fn read(path: &Path) -> Positions {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("part");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

thread_local! {
    static POSITIONS: std::cell::RefCell<Positions> =
        std::cell::RefCell::new(read(&folder().join("positions.json")));
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

pub fn remember(path: &Path, position: Position) {
    let bytes = POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        if !positions.remember(path, position) {
            return None;
        }
        serde_json::to_vec(&*positions).ok()
    });
    if let Some(bytes) = bytes {
        bingux_ui::task::spawn_detached("text-editor-positions", move || {
            let _ = write(&folder().join("positions.json"), &bytes);
        });
    }
}

impl Positions {
    /// Puts `path` first with `position`. False when it already was, so
    /// nothing needs writing.
    fn remember(&mut self, path: &Path, position: Position) -> bool {
        if self
            .files
            .iter()
            .any(|(known, old)| known == path && *old == position)
        {
            return false;
        }
        self.files.retain(|(known, _)| known != path);
        self.files.push_front((path.to_path_buf(), position));
        self.files.truncate(KEPT);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_file_is_first_and_the_oldest_are_forgotten() {
        let mut positions = Positions::default();
        let here = Position { caret: 4, line: 1 };
        assert!(positions.remember(Path::new("/a"), here));
        assert!(
            !positions.remember(Path::new("/a"), here),
            "the same place is nothing to write"
        );
        for index in 0..KEPT {
            positions.remember(Path::new(&format!("/f{index}")), here);
        }
        assert_eq!(positions.files.len(), KEPT);
        assert!(
            positions
                .files
                .iter()
                .all(|(path, _)| path != Path::new("/a")),
            "the oldest was dropped"
        );
        // Going back to a file moves it to the front, with the new place.
        let moved = Position { caret: 9, line: 3 };
        assert!(positions.remember(Path::new("/f10"), moved));
        assert_eq!(positions.files.front().unwrap().1, moved);
        assert_eq!(positions.files.len(), KEPT);
    }

    #[test]
    fn a_file_from_an_older_version_with_fields_missing_still_reads() {
        let positions: Positions = serde_json::from_str("{}").unwrap();
        assert!(positions.files.is_empty());
    }
}
