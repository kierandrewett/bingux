//! The session earlier builds kept, brought into the shared document manager
//! once. They wrote `session.json` (the open tabs) and `drafts/NAME.txt` (the
//! unsaved text of a tab) in `$XDG_STATE_HOME/bingux/text-editor/`; the
//! manager keeps its drafts and recent files in its own places and formats.
//!
//! On the first start with the manager, each old draft becomes a draft the
//! recovery question offers ("Recover unsaved work?"), and each file of the
//! old session joins the recent files. A marker in the manager's state folder
//! says it was done, so it is done once. The old files are never deleted.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bingux_ui::document::recents::{self, Registration};
use bingux_ui::document::{DraftMeta, Drafts, Stamp};
use bingux_ui::format::Moment;
use serde::Deserialize;

/// The name of the file that says the old session was imported.
const MARKER: &str = "imported-earlier-session";

#[derive(Default, Deserialize)]
#[serde(default)]
struct OldTab {
    path: Option<PathBuf>,
    /// The draft holding unsaved text, if there is one.
    draft: Option<String>,
    /// "Untitled", "Untitled 2".
    untitled: Option<String>,
    caret: usize,
    line: usize,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct OldSession {
    tabs: Vec<OldTab>,
}

/// What an import brought in.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Imported {
    pub drafts: usize,
    pub recents: usize,
}

/// Imports what `old` (the earlier builds' folder) holds into `drafts` and the
/// recent files at `xbel`, unless it was done before. Nothing in `old` is
/// changed or deleted.
pub fn import(old: &Path, drafts: &Drafts, xbel: &Path) -> Imported {
    let mut imported = Imported::default();
    let Some(marker) = drafts.dir().parent().map(|folder| folder.join(MARKER)) else {
        return imported;
    };
    if marker.exists() {
        return imported;
    }
    let session: OldSession = std::fs::read(old.join("session.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let names: Vec<String> = std::fs::read_dir(old.join("drafts"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "txt").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    if session.tabs.is_empty() && names.is_empty() {
        return imported;
    }
    for name in names {
        let file = old.join("drafts").join(format!("{name}.txt"));
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let tab = session
            .tabs
            .iter()
            .find(|tab| tab.draft.as_deref() == Some(name.as_str()));
        let path = tab.and_then(|tab| tab.path.clone());
        let untitled = tab
            .and_then(|tab| tab.untitled.as_deref())
            .map_or(1, untitled_number);
        let title = match (&path, tab.and_then(|tab| tab.untitled.clone())) {
            (Some(path), _) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            ),
            (None, Some(name)) => name,
            (None, None) => "Untitled".to_owned(),
        };
        let meta = DraftMeta {
            title,
            base: path.as_deref().and_then(Stamp::now),
            extra: serde_json::json!({
                "app": { "caret": tab.map_or(0, |tab| tab.caret) },
                "untitled": if path.is_some() { 0 } else { untitled },
                "scroll": [tab.map_or(0, |tab| tab.line) as f32, 0.0],
            }),
            path,
        };
        let saved = std::fs::metadata(&file)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs() as i64);
        if drafts
            .import(&meta, &bytes, Moment::from_unix(saved))
            .is_ok()
        {
            imported.drafts += 1;
        }
    }
    for tab in &session.tabs {
        let Some(path) = tab.path.as_deref().filter(|path| path.is_file()) else {
            continue;
        };
        let registration = Registration {
            path,
            mime: Some("text/plain"),
            app_name: "Text Editor",
            exec: "bingux-text-editor",
        };
        if recents::register_at(xbel, &registration, Moment::now()).is_ok() {
            imported.recents += 1;
        }
    }
    let _ = std::fs::write(&marker, b"");
    imported
}

/// 1 for "Untitled", 3 for "Untitled 3".
fn untitled_number(name: &str) -> u32 {
    name.rsplit(' ')
        .next()
        .and_then(|last| last.parse().ok())
        .unwrap_or(1)
}

/// Imports the earlier builds' session into this user's own folders.
pub fn import_earlier_session() {
    import(
        &crate::positions::folder(),
        &Drafts::open("bingux-text-editor"),
        &recents::default_path(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-legacy-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The layout the earlier builds left: a session and two drafts.
    fn old_layout(dir: &Path) -> (PathBuf, PathBuf) {
        let old = dir.join("old");
        std::fs::create_dir_all(old.join("drafts")).unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, "saved text").unwrap();
        let session = serde_json::json!({
            "tabs": [
                {"path": notes, "draft": "aaa-1", "caret": 6, "line": 2},
                {"draft": "bbb-2", "untitled": "Untitled 3", "caret": 4},
                {"path": notes}
            ],
            "active": 1
        });
        std::fs::write(old.join("session.json"), session.to_string()).unwrap();
        std::fs::write(old.join("drafts/aaa-1.txt"), "typed in notes").unwrap();
        std::fs::write(old.join("drafts/bbb-2.txt"), "an untitled thought").unwrap();
        (old, notes)
    }

    #[test]
    fn old_drafts_are_offered_for_recovery_and_old_files_become_recent_once() {
        let dir = scratch("import");
        let (old, notes) = old_layout(&dir);
        let drafts = Drafts::in_dir(dir.join("new/bingux-text-editor/drafts"));
        let xbel = dir.join("recently-used.xbel");

        let imported = import(&old, &drafts, &xbel);
        assert_eq!(
            imported,
            Imported {
                drafts: 2,
                recents: 2
            }
        );
        let mut found = drafts.recoverable();
        found.sort_by(|a, b| a.meta.title.cmp(&b.meta.title));
        assert_eq!(found.len(), 2, "both are offered: no process owns them");
        assert_eq!(found[0].meta.title, "Untitled 3");
        assert_eq!(found[0].meta.path, None);
        assert_eq!(found[0].meta.extra["untitled"], 3);
        assert_eq!(drafts.bytes(&found[0].id).unwrap(), b"an untitled thought");
        assert_eq!(found[1].meta.title, "notes.txt");
        assert_eq!(found[1].meta.path.as_deref(), Some(notes.as_path()));
        assert_eq!(found[1].meta.extra["app"]["caret"], 6);
        assert_eq!(found[1].meta.extra["scroll"][0], 2.0);
        assert_eq!(drafts.bytes(&found[1].id).unwrap(), b"typed in notes");
        assert_eq!(
            recents::list_of_app_at(&xbel, "Text Editor", 10),
            [notes.clone()]
        );

        // Once: a second start brings nothing, and the old files are all still there.
        assert_eq!(import(&old, &drafts, &xbel), Imported::default());
        assert_eq!(drafts.all().len(), 2);
        assert!(old.join("session.json").exists());
        assert!(old.join("drafts/aaa-1.txt").exists());
        assert!(old.join("drafts/bbb-2.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_old_means_nothing_imported_and_no_marker() {
        let dir = scratch("none");
        let drafts = Drafts::in_dir(dir.join("new/bingux-text-editor/drafts"));
        assert_eq!(
            import(&dir.join("absent"), &drafts, &dir.join("x.xbel")),
            Imported::default()
        );
        assert!(!dir.join("new/bingux-text-editor").join(MARKER).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
