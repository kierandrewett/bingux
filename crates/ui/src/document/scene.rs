//! What an app comes back to: the scene.
//!
//! The window's size, the documents that were open (which file, which was
//! showing, where each was scrolled to), and whatever else the app wants back
//! (a value of its own per document, and a few of its own by name). It is
//! written while the app runs and read once when it starts, so that quitting
//! and opening it again, or a crash and opening it again, puts the person
//! back where they were.
//!
//! The scene is one small JSON file, `$XDG_STATE_HOME/<app-id>/scene.json`
//! (`~/.local/state/...` when that is unset; `BINGUX_SCENE_FILE` names another
//! file, for tests and documentation captures, which also keep a real
//! person's scene out of the picture). It is read by [`Application::run`]
//! before the window exists, and written by a worker half a second after the
//! last change, and once more when the window closes. A file that does not
//! parse, or that has another format number, is a scene that was never
//! written.
//!
//! Only the first window of the process keeps the scene: a window that was
//! torn off or opened later does not write over it.
//!
//! [`Application::run`]: crate::app::Application::run

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::time::Duration;

use freya::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::write::write_atomic;
use crate::task::spawn_blocking;

/// The format number of the file. A file with another number is ignored.
const FORMAT: u32 = 1;
/// How long a change waits for the next one before it is written.
const WRITE_DELAY: Duration = Duration::from_millis(500);
/// The smallest and the largest window the scene brings back, in logical px.
const SMALLEST: f32 = 240.0;
const LARGEST: f32 = 16_000.0;

/// One document of the scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneDocument {
    /// The file; `None` for a document that was never saved.
    pub path: Option<PathBuf>,
    /// The draft that holds its unsaved changes, if it had any.
    pub draft: Option<String>,
    /// The name it showed ("notes.txt", "Untitled 2").
    pub title: String,
    /// How far it was scrolled, as the app measures it.
    pub scroll: (f32, f32),
    /// What the app keeps for the document: a caret, a language, a zoom.
    pub app: Value,
    /// Whether it was the one showing.
    pub current: bool,
}

/// What the scene holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SceneData {
    /// The window's size in logical pixels.
    pub window: Option<(f32, f32)>,
    /// The documents, in the order the app shows them.
    pub documents: Vec<SceneDocument>,
    /// Values the app keeps by name.
    pub values: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct StoredDocument {
    path: Option<Vec<u8>>,
    draft: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    scroll: (f32, f32),
    #[serde(default)]
    app: Value,
    #[serde(default)]
    current: bool,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    format: u32,
    window: Option<(f32, f32)>,
    #[serde(default)]
    documents: Vec<StoredDocument>,
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

impl SceneData {
    /// The file's text.
    pub fn to_json(&self) -> Vec<u8> {
        let stored = Stored {
            format: FORMAT,
            window: self.window,
            documents: self
                .documents
                .iter()
                .map(|document| StoredDocument {
                    path: document
                        .path
                        .as_ref()
                        .map(|path| path.as_os_str().as_bytes().to_vec()),
                    draft: document.draft.clone(),
                    title: document.title.clone(),
                    scroll: document.scroll,
                    app: document.app.clone(),
                    current: document.current,
                })
                .collect(),
            values: self.values.clone(),
        };
        serde_json::to_vec_pretty(&stored).unwrap_or_default()
    }

    /// The scene a file holds, or `None` when it is not a scene this build can
    /// read (it does not parse, it has another format number, or it is
    /// empty). A size that is absurd is dropped, not the rest.
    pub fn from_json(bytes: &[u8]) -> Option<SceneData> {
        let stored: Stored = serde_json::from_slice(bytes).ok()?;
        if stored.format != FORMAT {
            return None;
        }
        let sane = |value: f32| value.is_finite() && (SMALLEST..=LARGEST).contains(&value);
        Some(SceneData {
            window: stored
                .window
                .filter(|(width, height)| sane(*width) && sane(*height)),
            documents: stored
                .documents
                .into_iter()
                .map(|document| SceneDocument {
                    path: document
                        .path
                        .map(|bytes| PathBuf::from(std::ffi::OsString::from_vec(bytes))),
                    draft: document.draft,
                    title: document.title,
                    scroll: (
                        if document.scroll.0.is_finite() {
                            document.scroll.0
                        } else {
                            0.0
                        },
                        if document.scroll.1.is_finite() {
                            document.scroll.1
                        } else {
                            0.0
                        },
                    ),
                    app: document.app,
                    current: document.current,
                })
                .collect(),
            values: stored.values,
        })
    }
}

/// The scene file of the app `app_id`.
fn file_of(app_id: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BINGUX_SCENE_FILE") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join(app_id).join("scene.json"))
}

struct Global {
    path: Option<PathBuf>,
    /// What the file held when the app started: what to come back to.
    loaded: SceneData,
    /// What the app has now: what the file will hold.
    current: SceneData,
}

// The scene belongs to the UI thread, where every window of the app runs. A
// test thread therefore has a scene of its own.
thread_local! {
    static SCENE: RefCell<Global> = const {
        RefCell::new(Global {
            path: None,
            loaded: SceneData { window: None, documents: Vec::new(), values: BTreeMap::new() },
            current: SceneData { window: None, documents: Vec::new(), values: BTreeMap::new() },
        })
    };
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static PRIMARY: Cell<bool> = const { Cell::new(false) };
}

fn with_global<R>(use_it: impl FnOnce(&mut Global) -> R) -> R {
    SCENE.with(|scene| use_it(&mut scene.borrow_mut()))
}

/// Reads the scene of `app_id`. Called by `Application::run` before the first
/// window exists, so that the window opens at its old size and a root finds
/// the old scene in its first render with no read of its own.
pub(crate) fn preload(app_id: &str) {
    let path = file_of(app_id);
    let loaded = path
        .as_ref()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| SceneData::from_json(&bytes))
        .unwrap_or_default();
    with_global(|global| {
        global.path = path;
        global.current = loaded.clone();
        global.loaded = loaded;
    });
}

/// The window size the scene brings back.
pub(crate) fn loaded_window_size() -> Option<(f32, f32)> {
    with_global(|global| global.loaded.window)
}

/// What a write of the scene is, taken on the UI thread so that the worker
/// that does the write needs nothing from it.
fn pending_write() -> Option<(PathBuf, Vec<u8>)> {
    with_global(|global| Some((global.path.clone()?, global.current.to_json())))
}

/// Writes the scene. Reads and writes the disk: call it on a worker.
fn write_out(path: PathBuf, bytes: Vec<u8>) {
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    // A scene that cannot be written is a scene that is not kept: nothing
    // depends on it, and nobody is asked.
    let _ = write_atomic(&path, &bytes);
}

/// The scene of the running app, for a root to read what the app comes back
/// to and to say what it is now. Copy it into closures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SceneState {
    primary: bool,
}

/// The scene of this app. Call it in the window's root, once.
///
/// It keeps the window's size in the scene while the window is the first of
/// the process (see the module docs). Documents add theirs through
/// [`use_documents`](super::use_documents), which calls this itself.
pub fn use_scene_state() -> SceneState {
    let primary = use_hook(|| !PRIMARY.with(|primary| primary.replace(true)));
    use_drop(move || {
        if primary {
            PRIMARY.with(|primary| primary.set(false));
        }
    });
    let scene = SceneState { primary };
    use_side_effect(move || {
        // Read inside the effect: a resize runs the effect, not the root.
        let size = crate::surface::window_size();
        if scene.primary && size.width > 0.0 && size.height > 0.0 {
            scene.set_window_size(size.width, size.height);
        }
    });
    scene
}

impl SceneState {
    /// Whether this window keeps the scene: the first window of the process.
    pub fn is_primary(self) -> bool {
        self.primary
    }

    /// The window size the app was last closed with, in logical pixels.
    pub fn window_size(self) -> Option<(f32, f32)> {
        loaded_window_size()
    }

    /// The documents that were open, in order. What to open again.
    pub fn documents(self) -> Vec<SceneDocument> {
        with_global(|global| global.loaded.documents.clone())
    }

    /// The value saved under `key`, if there is one this build can read.
    pub fn value<T: DeserializeOwned>(self, key: &str) -> Option<T> {
        with_global(|global| serde_json::from_value(global.loaded.values.get(key)?.clone()).ok())
    }

    /// Keeps `value` under `key`. Written shortly after the last call.
    pub fn set_value<T: Serialize>(self, key: &str, value: &T) {
        let Ok(value) = serde_json::to_value(value) else {
            return;
        };
        if !self.primary {
            return;
        }
        let changed = with_global(|global| {
            if global.current.values.get(key) == Some(&value) {
                return false;
            }
            global.current.values.insert(key.to_string(), value);
            true
        });
        if changed {
            touch();
        }
    }

    pub(crate) fn set_window_size(self, width: f32, height: f32) {
        let changed = with_global(|global| {
            let size = Some((width.round(), height.round()));
            if global.current.window == size {
                return false;
            }
            global.current.window = size;
            true
        });
        if changed {
            touch();
        }
    }

    pub(crate) fn set_documents(self, documents: Vec<SceneDocument>) {
        if !self.primary {
            return;
        }
        let changed = with_global(|global| {
            if global.current.documents == documents {
                return false;
            }
            global.current.documents = documents;
            true
        });
        if changed {
            touch();
        }
    }

    /// Writes the scene on a worker and waits for it. Called when the window
    /// closes, so that the scene it leaves is the last one.
    pub(crate) async fn flush_now(self) {
        if let Some((path, bytes)) = pending_write().filter(|_| self.primary) {
            spawn_blocking(move || write_out(path, bytes)).await;
        }
    }
}

/// Writes the scene half a second from now, once for any number of changes.
fn touch() {
    if ARMED.with(|armed| armed.replace(true)) {
        return;
    }
    spawn_in_window(async {
        timer(WRITE_DELAY).await;
        ARMED.with(|armed| armed.set(false));
        if let Some((path, bytes)) = pending_write() {
            spawn_blocking(move || write_out(path, bytes)).await;
        }
    });
}

/// What a test of the manager needs of the scene.
#[cfg(test)]
pub(crate) mod for_tests {
    use super::*;

    /// Makes `scene` what the app finds at start, for a test of the restore.
    pub(crate) fn preload_for_test(scene: SceneData) {
        with_global(|global| {
            global.path = None;
            global.current = scene.clone();
            global.loaded = scene;
        });
    }

    /// What the app has said the scene is now, for a test of the writing side.
    pub(crate) fn current_documents_for_test() -> Vec<SceneDocument> {
        with_global(|global| global.current.documents.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SceneData {
        SceneData {
            window: Some((1100.0, 720.0)),
            documents: vec![
                SceneDocument {
                    path: Some(PathBuf::from("/home/a/notes.txt")),
                    draft: None,
                    title: "notes.txt".into(),
                    scroll: (0.0, 412.5),
                    app: serde_json::json!({"caret": 31, "language": "plain"}),
                    current: false,
                },
                SceneDocument {
                    path: None,
                    draft: Some("69f-1a2-0".into()),
                    title: "Untitled 2".into(),
                    scroll: (0.0, 0.0),
                    app: Value::Null,
                    current: true,
                },
            ],
            values: BTreeMap::from([("zoom".to_string(), serde_json::json!(1.25))]),
        }
    }

    #[test]
    fn a_scene_round_trips() {
        let scene = sample();
        assert_eq!(SceneData::from_json(&scene.to_json()), Some(scene));
    }

    #[test]
    fn a_path_that_is_not_text_survives() {
        let mut scene = sample();
        let bytes = std::ffi::OsString::from_vec(b"/tmp/caf\xe9.txt".to_vec());
        scene.documents[0].path = Some(PathBuf::from(bytes));
        assert_eq!(SceneData::from_json(&scene.to_json()), Some(scene));
    }

    #[test]
    fn a_scene_that_cannot_be_read_is_no_scene() {
        for bad in [
            &b""[..],
            b"not json",
            b"{\"format\": 1",
            b"{\"format\": 99, \"window\": [800, 600], \"documents\": []}",
            b"[1, 2, 3]",
            b"{\"window\": [800, 600]}",
        ] {
            assert_eq!(
                SceneData::from_json(bad),
                None,
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn an_absurd_size_is_dropped_and_the_rest_is_kept() {
        let text = br#"{"format":1,"window":[1,99999999],"documents":[{"path":null,"draft":"x"}]}"#;
        let scene = SceneData::from_json(text).unwrap();
        assert_eq!(scene.window, None);
        assert_eq!(scene.documents.len(), 1);
        let text = br#"{"format":1,"window":[1000,700],"documents":[]}"#;
        assert_eq!(
            SceneData::from_json(text).unwrap().window,
            Some((1000.0, 700.0))
        );
    }

    #[test]
    fn a_scene_written_by_a_newer_build_with_more_fields_is_still_read() {
        let text = br#"{"format":1,"window":[1000,700],"documents":[{"path":null,"draft":null,"future":true}],"extra":1}"#;
        assert_eq!(SceneData::from_json(text).unwrap().documents.len(), 1);
    }
}
