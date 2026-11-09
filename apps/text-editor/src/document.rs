//! The open documents: what each one is (a file or an untitled draft), how
//! its file was written (encoding, byte order mark, line endings), whether
//! the file changed on disk, and the loading and saving that happen off the
//! UI thread.
//!
//! Documents live in a registry of the window's thread, keyed by a number,
//! so a tab holds only that number and a tab moved to another window keeps
//! its document, undo history and all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use bingux_text::editorconfig::{self, EditorConfig};
use bingux_text::encoding::{self, Decoded, FileFormat};
use bingux_text::encoding_rs;
use bingux_text::{Editor, Indent, Language, Rope};
use bingux_ui::text_editor::EditorHandle;
use freya::prelude::*;

/// What the file on disk was when it was read or written last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

impl Stamp {
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        }
    }
}

/// A notice shown over a document until it is dealt with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The file changed on disk and the document has no unsaved changes:
    /// it was reloaded, and this says so (with Undo).
    Reloaded,
    /// The file changed on disk while the document has unsaved changes.
    ChangedOnDisk,
    /// The file was deleted or moved away.
    Deleted,
    /// The file cannot be written.
    ReadOnly,
    /// UTF-8 with stray bytes of another encoding: read as both, saved as UTF-8.
    Mixed(&'static encoding_rs::Encoding),
    /// Some bytes could not be read in the encoding and were replaced.
    Lossy,
    /// More than one kind of line ending: saving writes one.
    MixedLineEndings,
    /// The file looks binary; showing it as text could damage it on save.
    Binary,
}

/// Where a document is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    /// Being read; `len` is the file's size.
    Loading {
        len: u64,
    },
    /// The file could not be read; the text says why and what to do.
    Failed(String),
}

/// One open document.
pub struct Doc {
    pub id: u64,
    pub handle: EditorHandle,
    pub path: Option<PathBuf>,
    pub format: FileFormat,
    pub stamp: Option<Stamp>,
    pub notices: Vec<Notice>,
    pub phase: Phase,
    /// The name of an untitled document: "Untitled", "Untitled 2".
    pub untitled: String,
    /// The draft file this document's unsaved text is kept in.
    pub draft: String,
    /// The project's `.editorconfig`, read when the file opened.
    pub config: EditorConfig,
    /// The file could not be written last time we tried, or is not writable.
    pub read_only: bool,
    /// Whether a save is running.
    pub saving: bool,
    /// The revision of the text the last autosaved draft holds.
    pub draft_revision: u64,
    /// The bytes of the file as read, kept for Reopen with another encoding
    /// when the file is small.
    pub raw: Option<Rc<Vec<u8>>>,
    /// The same file in the shared document manager, once it is in it.
    pub shared: Option<bingux_ui::document::DocumentId>,
}

impl Doc {
    /// The name the tab and the title bar show.
    pub fn title(&self) -> String {
        match &self.path {
            Some(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            None => self.untitled.clone(),
        }
    }

    pub fn is_modified(&self) -> bool {
        self.handle.peek().is_modified()
    }
}

thread_local! {
    static DOCS: RefCell<HashMap<u64, Doc>> = RefCell::new(HashMap::new());
    static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    /// One signal for every document: only the document on show is drawn.
    static SIGNAL: State<u64> = State::create_global(0);
    /// Bumped when a document's facts (path, notices, phase) change, so the
    /// header, the tabs and the status bar redraw.
    static FACTS: State<u64> = State::create_global(0);
}

pub fn signal() -> State<u64> {
    SIGNAL.with(|signal| *signal)
}

/// Subscribes the caller to changes of the documents' facts.
pub fn watch() -> u64 {
    FACTS.with(|facts| *facts.read())
}

fn changed() {
    FACTS.with(|facts| {
        let mut facts = *facts;
        *facts.write() += 1;
    });
}

/// Reads document `id`.
pub fn with<R>(id: u64, read: impl FnOnce(&Doc) -> R) -> Option<R> {
    DOCS.with(|docs| docs.borrow().get(&id).map(read))
}

/// Changes document `id` and redraws what shows its facts.
pub fn update<R>(id: u64, edit: impl FnOnce(&mut Doc) -> R) -> Option<R> {
    let result = DOCS.with(|docs| docs.borrow_mut().get_mut(&id).map(edit));
    changed();
    result
}

pub fn handle(id: u64) -> Option<EditorHandle> {
    with(id, |doc| doc.handle.clone())
}

pub fn ids() -> Vec<u64> {
    DOCS.with(|docs| docs.borrow().keys().copied().collect())
}

pub fn remove(id: u64) {
    DOCS.with(|docs| docs.borrow_mut().remove(&id));
    changed();
}

/// The next free "Untitled" name.
fn untitled_name() -> String {
    let taken: Vec<String> = DOCS.with(|docs| {
        docs.borrow()
            .values()
            .filter(|doc| doc.path.is_none())
            .map(|doc| doc.untitled.clone())
            .collect()
    });
    if !taken.iter().any(|name| name == "Untitled") {
        return "Untitled".into();
    }
    (2..)
        .map(|number| format!("Untitled {number}"))
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| "Untitled".into())
}

/// A new draft name, unique in this session.
pub fn new_draft_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos:x}-{}", std::process::id())
}

/// A new empty document, ready to type in.
pub fn new_untitled(prefs: &crate::prefs::Prefs) -> u64 {
    let mut editor = Editor::new("");
    editor.settings.indent = prefs.indent();
    editor.settings.tab_width = prefs.tab_width;
    editor.settings.auto_close = prefs.auto_close;
    insert(
        editor,
        None,
        FileFormat::default(),
        Phase::Ready,
        new_draft_name(),
    )
}

/// Registers a document and returns its id.
pub fn insert(
    editor: Editor,
    path: Option<PathBuf>,
    format: FileFormat,
    phase: Phase,
    draft: String,
) -> u64 {
    let id = NEXT.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    let untitled = if path.is_none() {
        untitled_name()
    } else {
        String::new()
    };
    let handle = EditorHandle::new(editor, signal());
    DOCS.with(|docs| {
        docs.borrow_mut().insert(
            id,
            Doc {
                id,
                handle,
                path,
                format,
                stamp: None,
                notices: Vec::new(),
                phase,
                untitled,
                draft,
                config: EditorConfig::default(),
                read_only: false,
                saving: false,
                draft_revision: 0,
                raw: None,
                shared: None,
            },
        )
    });
    changed();
    id
}

/// A new untitled document with the text, language and indentation of
/// document `id`: Duplicate tab.
pub fn duplicate(id: u64, prefs: &crate::prefs::Prefs) -> u64 {
    let copy = with(id, |doc| {
        let editor = doc.handle.peek();
        let mut copy = Editor::from_rope(editor.rope().clone());
        copy.settings = editor.settings;
        copy.set_language(editor.language().clone());
        copy.set_selection(editor.selection().clone());
        copy.history_mut().mark_unsaved();
        copy
    });
    match copy {
        Some(editor) => insert(
            editor,
            None,
            FileFormat::default(),
            Phase::Ready,
            new_draft_name(),
        ),
        None => new_untitled(prefs),
    }
}

/// The open document for `path`, if any.
pub fn find_path(path: &Path) -> Option<u64> {
    DOCS.with(|docs| {
        docs.borrow()
            .values()
            .find(|doc| doc.path.as_deref() == Some(path))
            .map(|doc| doc.id)
    })
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// What reading a file came to: the plain-language reason when it failed.
pub type ReadResult = Result<LoadedSend, String>;

/// [`Loaded`] without the `Rc`, to cross from the worker.
pub struct LoadedSend {
    pub rope: Rope,
    pub format: FileFormat,
    pub notices: Vec<Notice>,
    pub stamp: Stamp,
    pub writable: bool,
    pub language: Language,
    pub indent: Option<Indent>,
    pub config: EditorConfig,
    pub raw: Option<Vec<u8>>,
}

/// Keep the file's bytes for Reopen with Encoding when it is smaller than this.
const KEEP_RAW: usize = 8 << 20;

/// Reads `path` on the calling (worker) thread. `encoding` forces one.
pub fn read(
    path: &Path,
    encoding: Option<&'static encoding_rs::Encoding>,
    allow_binary: bool,
) -> ReadResult {
    let metadata = std::fs::metadata(path).map_err(|error| read_error(path, &error))?;
    if metadata.is_dir() {
        return Err(format!(
            "“{}” is a folder. Open a file inside it.",
            display_name(path)
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| read_error(path, &error))?;
    if !allow_binary && encoding.is_none() && encoding::looks_binary(&bytes) {
        return Ok(LoadedSend {
            rope: Rope::new(),
            format: FileFormat::default(),
            notices: vec![Notice::Binary],
            stamp: Stamp::of(&metadata),
            writable: false,
            language: Language::plain(),
            indent: None,
            config: EditorConfig::default(),
            raw: None,
        });
    }
    let raw = (bytes.len() <= KEEP_RAW).then(|| bytes.clone());
    let decoded: Decoded = match encoding {
        Some(encoding) => encoding::decode_as(
            &bytes,
            encoding,
            encoding_rs::Encoding::for_bom(&bytes).is_some(),
        ),
        None => encoding::decode_vec(bytes),
    };
    let mut notices = Vec::new();
    if let Some(legacy) = decoded.mixed {
        notices.push(Notice::Mixed(legacy));
    }
    if decoded.lossy {
        notices.push(Notice::Lossy);
    }
    if decoded.mixed_line_endings {
        notices.push(Notice::MixedLineEndings);
    }
    let writable = is_writable(path, &metadata);
    if !writable {
        notices.push(Notice::ReadOnly);
    }
    let head: String = decoded.text.chars().take(4096).collect();
    let tail_start = decoded.text.len().saturating_sub(2048);
    let tail_start = (tail_start..decoded.text.len())
        .find(|index| decoded.text.is_char_boundary(*index))
        .unwrap_or(decoded.text.len());
    let modeline_text = format!("{head}\n{}", &decoded.text[tail_start..]);
    let language = bingux_text::language::detect(Some(path), &modeline_text);
    let rope = Rope::from_str(&decoded.text);
    drop(decoded.text);
    let indent = bingux_text::indent::detect(&rope);
    let config = editorconfig::for_file(path);
    Ok(LoadedSend {
        rope,
        format: decoded.format,
        notices,
        stamp: Stamp::of(&metadata),
        writable,
        language,
        indent,
        config,
        raw,
    })
}

fn is_writable(path: &Path, metadata: &std::fs::Metadata) -> bool {
    if metadata.permissions().readonly() {
        return false;
    }
    use std::os::unix::ffi::OsStrExt;
    let Ok(text) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return true;
    };
    // SAFETY: `text` is a valid NUL-terminated path for the call's duration.
    unsafe { libc_access(text.as_ptr()) }
}

unsafe extern "C" {
    fn access(path: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
}

/// `access(path, W_OK)`: whether this user may write the file.
unsafe fn libc_access(path: *const std::ffi::c_char) -> bool {
    const W_OK: std::ffi::c_int = 2;
    // SAFETY: the caller passes a valid C string.
    unsafe { access(path, W_OK) == 0 }
}

pub fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn read_error(path: &Path, error: &std::io::Error) -> String {
    let name = display_name(path);
    match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!("Could not find “{name}”. Open it again from where it is now.")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!(
                "Could not open “{name}” because you cannot read it. Ask its owner to share it."
            )
        }
        _ => format!("Could not open “{name}”. Check that the drive is connected, then try again."),
    }
}

/// Puts what a worker read into document `id`.
pub fn finish_loading(id: u64, loaded: LoadedSend, prefs: &crate::prefs::Prefs) {
    let handle = match handle(id) {
        Some(handle) => handle,
        None => return,
    };
    let indent = loaded.config.indent.or(loaded.indent).unwrap_or_else(|| {
        if loaded.language.tabs {
            Indent::Tabs
        } else {
            prefs.indent()
        }
    });
    let tab_width = loaded.config.tab_width.unwrap_or(prefs.tab_width);
    let mut format = loaded.format;
    if let Some(ending) = loaded.config.line_ending
        && loaded.rope.len_chars() == 0
    {
        format.line_ending = ending;
    }
    handle.update(|editor| {
        editor.load(loaded.rope);
        editor.settings.indent = indent;
        editor.settings.tab_width = tab_width;
        editor.settings.auto_close = prefs.auto_close;
        editor.set_language(loaded.language);
    });
    update(id, |doc| {
        doc.format = format;
        doc.stamp = Some(loaded.stamp);
        doc.notices = loaded.notices;
        doc.read_only = !loaded.writable;
        doc.phase = Phase::Ready;
        doc.config = loaded.config;
        doc.raw = loaded.raw.map(Rc::new);
        doc.draft_revision = doc.handle.peek().revision();
    });
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Why a save did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The text has characters the encoding cannot hold.
    Unmappable { count: usize, first: char },
    /// The folder or file cannot be written.
    Permission,
    /// The disk is full.
    Full,
    /// Anything else.
    Other,
}

impl SaveError {
    pub fn message(&self, name: &str, encoding: &str) -> String {
        match self {
            SaveError::Unmappable { first, .. } => format!(
                "Could not save “{name}” in {encoding}, which has no “{first}” and other characters in it. Save in UTF-8 instead."
            ),
            SaveError::Permission => format!(
                "Could not save “{name}” because you cannot change it. Save a copy somewhere else."
            ),
            SaveError::Full => {
                format!(
                    "Could not save “{name}” because the disk is full. Free up space, then try again."
                )
            }
            SaveError::Other => format!(
                "Could not save “{name}”. Check that the drive is connected, then try again."
            ),
        }
    }
}

/// Writes `text` to `path` in `format`, atomically: to a hidden file in the
/// same folder, flushed, then renamed over the old one, keeping its mode.
/// Runs on a worker thread.
pub fn write(path: &Path, text: &str, format: &FileFormat) -> Result<Stamp, SaveError> {
    let bytes = encoding::encode(text, format).map_err(|error| SaveError::Unmappable {
        count: error.count,
        first: error.first,
    })?;
    let folder = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = folder.join(format!(".{name}.bingux-save-{}", std::process::id()));
    let existing = std::fs::metadata(path).ok();
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if let Some(metadata) = &existing {
            std::fs::set_permissions(&temp, metadata.permissions())?;
        }
        // A link is written through, so the link stays a link.
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        std::fs::rename(&temp, &target)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        // A folder we cannot create files in, but a file we may write:
        // write it in place.
        if error.kind() == std::io::ErrorKind::PermissionDenied
            && existing
                .as_ref()
                .is_some_and(|metadata| !metadata.permissions().readonly())
        {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)
            {
                if file.write_all(&bytes).and_then(|_| file.sync_all()).is_ok() {
                    return std::fs::metadata(path)
                        .map(|metadata| Stamp::of(&metadata))
                        .map_err(|_| SaveError::Other);
                }
            }
        }
        return Err(match error.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                SaveError::Permission
            }
            std::io::ErrorKind::StorageFull => SaveError::Full,
            _ => SaveError::Other,
        });
    }
    std::fs::metadata(path)
        .map(|metadata| Stamp::of(&metadata))
        .map_err(|_| SaveError::Other)
}

/// The file's stamp now, or `None` when it is gone. Runs on a worker.
pub fn stamp_now(path: &Path) -> Option<Stamp> {
    std::fs::metadata(path)
        .ok()
        .map(|metadata| Stamp::of(&metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_atomically_and_keeps_the_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("bingux-text-editor-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.sh");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let format = FileFormat {
            line_ending: bingux_text::LineEnding::CrLf,
            ..FileFormat::default()
        };
        write(&path, "one\ntwo\n", &format).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"one\r\ntwo\r\n");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unmappable_text_is_refused() {
        let format = FileFormat {
            encoding: encoding_rs::WINDOWS_1252,
            ..FileFormat::default()
        };
        let error = write(Path::new("/nonexistent/x"), "日本", &format).unwrap_err();
        assert_eq!(
            error,
            SaveError::Unmappable {
                count: 2,
                first: '日'
            }
        );
    }
}
