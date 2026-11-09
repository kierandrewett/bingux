//! The editor's side of a document managed by `bingux_ui::document`: the text
//! (an [`EditorHandle`]), how its file was written (encoding, byte order mark,
//! line endings), and the notices that go with what was read.
//!
//! The shared manager owns the file: it opens, saves, watches, recovers and
//! closes it. This type turns the file's bytes into text on a worker
//! ([`decode`]), the text into bytes ([`encode`]), tells the manager when the
//! text is edited, and does to the text what a save of this editor does to it
//! (trailing spaces, the final new line).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bingux_text::editorconfig::{self, EditorConfig};
use bingux_text::encoding::{self, FileFormat, LineEnding};
use bingux_text::encoding_rs::{self, Encoding};
use bingux_text::{Editor, Indent, Language, Rope};
use bingux_ui::document::{
    Document, DocumentContent, DocumentInit, DocumentSnapshot, LoadReason, Prepared, Preparer,
};
use bingux_ui::prelude::tr;
use bingux_ui::text_editor::{EditorHandle, ScrollPos};
use freya::prelude::State;
use serde::{Deserialize, Serialize};

use crate::positions;
use crate::prefs::Prefs;

thread_local! {
    /// One signal for every document: only the document on show is drawn.
    static SIGNAL: State<u64> = State::create_global(0);
}

pub fn signal() -> State<u64> {
    SIGNAL.with(|signal| *signal)
}

/// A notice shown over a document until it is dealt with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// UTF-8 with stray bytes of another encoding: read as both, saved as UTF-8.
    Mixed(&'static Encoding),
    /// Some bytes could not be read in the encoding and were replaced.
    Lossy,
    /// More than one kind of line ending: saving writes one.
    MixedLineEndings,
    /// The file looks binary; showing it as text could damage it on save.
    Binary,
}

/// What the editor keeps with a document for the scene and for drafts
/// (`Document::set_app_value`): where the caret was, a language the person
/// chose, and how the file is written.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    caret: usize,
    /// A language the person chose, by id; none when it is the detected one.
    language: Option<String>,
    format: Option<StoredFormat>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StoredFormat {
    encoding: String,
    bom: bool,
    line_ending: String,
}

impl StoredFormat {
    fn of(format: &FileFormat) -> Self {
        Self {
            encoding: format.encoding.name().to_owned(),
            bom: format.bom,
            line_ending: format.line_ending.label().to_owned(),
        }
    }

    fn format(&self) -> Option<FileFormat> {
        let line_ending = LineEnding::ALL
            .iter()
            .copied()
            .find(|ending| ending.label() == self.line_ending)?;
        Some(FileFormat {
            encoding: Encoding::for_label(self.encoding.as_bytes())?,
            bom: self.bom,
            line_ending,
        })
    }
}

/// How to read the bytes of a file.
struct How {
    /// The path, for the language and the project's `.editorconfig`.
    path: Option<PathBuf>,
    /// The encoding the person chose (Read again as).
    force: Option<&'static Encoding>,
    /// A file that looks binary is read as text anyway (Open anyway).
    allow_binary: bool,
    /// How the file was written when a draft of it was made.
    format: Option<FileFormat>,
}

/// What a worker made of the bytes of a file.
pub struct Read {
    pub rope: Rope,
    pub format: FileFormat,
    pub notices: Vec<Notice>,
    pub language: Language,
    pub indent: Option<Indent>,
    pub config: EditorConfig,
}

impl Read {
    /// A file that looks binary: nothing of it is shown.
    pub fn is_binary(&self) -> bool {
        self.notices.contains(&Notice::Binary)
    }
}

/// Reads `bytes` as text: the encoding, the language, the indentation, the
/// project's settings. Runs on a worker. The window reads through
/// [`EditorContent::prepare`]; this is the same for the tests and the
/// snapshot mode's timings.
#[cfg(any(test, feature = "snapshot"))]
pub fn decode(
    bytes: Vec<u8>,
    path: Option<&Path>,
    force: Option<&'static Encoding>,
    allow_binary: bool,
) -> Read {
    decode_how(
        bytes,
        &How {
            path: path.map(Path::to_path_buf),
            force,
            allow_binary,
            format: None,
        },
    )
}

fn decode_how(bytes: Vec<u8>, how: &How) -> Read {
    if !how.allow_binary && how.force.is_none() && encoding::looks_binary(&bytes) {
        return Read {
            rope: Rope::new(),
            format: FileFormat::default(),
            notices: vec![Notice::Binary],
            language: Language::plain(),
            indent: None,
            config: EditorConfig::default(),
        };
    }
    let decoded = match (how.force, &how.format) {
        (Some(encoding), _) => encoding::decode_as(
            &bytes,
            encoding,
            encoding_rs::Encoding::for_bom(&bytes).is_some(),
        ),
        (None, Some(format)) => {
            let mut decoded = encoding::decode_as(&bytes, format.encoding, format.bom);
            decoded.format.line_ending = format.line_ending;
            decoded
        }
        (None, None) => encoding::decode_vec(bytes),
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
    let head: String = decoded.text.chars().take(4096).collect();
    let tail_start = decoded.text.len().saturating_sub(2048);
    let tail_start = (tail_start..decoded.text.len())
        .find(|index| decoded.text.is_char_boundary(*index))
        .unwrap_or(decoded.text.len());
    let modeline_text = format!("{head}\n{}", &decoded.text[tail_start..]);
    let language = bingux_text::language::detect(how.path.as_deref(), &modeline_text);
    let rope = Rope::from_str(&decoded.text);
    drop(decoded.text);
    let indent = bingux_text::indent::detect(&rope);
    let config = how
        .path
        .as_deref()
        .map(editorconfig::for_file)
        .unwrap_or_default();
    Read {
        rope,
        format: decoded.format,
        notices,
        language,
        indent,
        config,
    }
}

/// `rope` as the bytes of a file in `format`. The text for the person when it
/// cannot be: what is wrong and what to change.
pub fn encode(rope: &Rope, format: &FileFormat) -> Result<Vec<u8>, String> {
    encoding::encode(&rope.to_string(), format).map_err(|error| {
        let encoding = format.encoding.name();
        let others = error.count.saturating_sub(1);
        tr!(
            "text-editor-unmappable",
            encoding = encoding,
            first = error.first.to_string(),
            others = others
        )
    })
}

/// Trims trailing spaces and adds the final new line that the file's settings
/// or the preferences ask for: what a save does to the text.
pub fn tidy(editor: &mut Editor, config: &EditorConfig, prefs: &Prefs) {
    let trim = config
        .trim_trailing_whitespace
        .unwrap_or(prefs.trim_on_save);
    let final_newline = config.insert_final_newline.unwrap_or(prefs.final_newline);
    if trim {
        editor.trim_trailing_whitespace();
    }
    if final_newline && editor.len_chars() > 0 {
        editor.ensure_final_newline();
    }
}

/// What the content keeps besides its text. It does not hold the handle, so
/// the observer on the handle can hold it.
struct Shared {
    format: Cell<FileFormat>,
    notices: RefCell<Vec<Notice>>,
    config: RefCell<EditorConfig>,
    /// The id of the language detected from the file, to tell a language the
    /// person chose from it.
    detected: RefCell<String>,
    /// The person chose an encoding to read the file as.
    force: Cell<Option<&'static Encoding>>,
    open_anyway: Cell<bool>,
    binary: Cell<bool>,
    hide_read_only: Cell<bool>,
    document: Cell<Option<Document>>,
    /// The revision and unsaved state of the text as last seen.
    seen: Cell<(u64, bool)>,
    stored: RefCell<Stored>,
}

impl Shared {
    fn stored_for(&self, editor: &Editor) -> Stored {
        let language = editor.language();
        Stored {
            caret: editor.selection().primary().head,
            language: (language.id != *self.detected.borrow()).then(|| language.id.clone()),
            format: Some(StoredFormat::of(&self.format.get())),
        }
    }
}

pub struct EditorContent {
    pub handle: EditorHandle,
    shared: Rc<Shared>,
}

impl EditorContent {
    pub fn new(init: &DocumentInit) -> Self {
        let prefs = crate::app::prefs_now();
        let mut editor = Editor::new("");
        editor.settings.indent = prefs.indent();
        editor.settings.tab_width = prefs.tab_width;
        editor.settings.auto_close = prefs.auto_close;
        if let Some(path) = &init.path {
            editor.set_language(bingux_text::language::detect(Some(path), ""));
        }
        let detected = editor.language().id.clone();
        let revision = editor.revision();
        Self {
            handle: EditorHandle::new(editor, signal()),
            shared: Rc::new(Shared {
                format: Cell::new(FileFormat::default()),
                notices: RefCell::new(Vec::new()),
                config: RefCell::new(EditorConfig::default()),
                detected: RefCell::new(detected),
                force: Cell::new(None),
                open_anyway: Cell::new(false),
                binary: Cell::new(false),
                hide_read_only: Cell::new(false),
                document: Cell::new(None),
                seen: Cell::new((revision, false)),
                stored: RefCell::new(Stored::default()),
            }),
        }
    }

    pub fn format(&self) -> FileFormat {
        self.shared.format.get()
    }

    /// Changes how the file is written. It is a change to save.
    pub fn change_format(&self, change: impl FnOnce(&mut FileFormat)) {
        let mut format = self.shared.format.get();
        change(&mut format);
        self.shared.format.set(format);
        self.handle
            .update(|editor| editor.history_mut().mark_unsaved());
        if let Some(document) = self.shared.document.get() {
            document.mark_edited();
        }
        self.sync();
    }

    pub fn notices(&self) -> Vec<Notice> {
        self.shared.notices.borrow().clone()
    }

    pub fn dismiss(&self, notice: &Notice) {
        self.shared
            .notices
            .borrow_mut()
            .retain(|kept| kept != notice);
        self.handle.refresh();
    }

    /// The file looks binary and is not shown.
    pub fn is_binary(&self) -> bool {
        self.shared.binary.get()
    }

    /// The notice that the file cannot be written was dismissed.
    pub fn read_only_hidden(&self) -> bool {
        self.shared.hide_read_only.get()
    }

    pub fn hide_read_only(&self) {
        self.shared.hide_read_only.set(true);
        self.handle.refresh();
    }

    /// Reads the file again as `encoding` (`None`: as it is found) and, with
    /// `anyway`, shows a file that looks binary as text. A document with
    /// unsaved changes asks first: they are lost.
    pub fn read_again(&self, encoding: Option<&'static Encoding>, anyway: bool) {
        self.shared.force.set(encoding);
        if anyway {
            self.shared.open_anyway.set(true);
        }
        if let Some(document) = self.shared.document.get() {
            document.reload();
        }
    }

    /// Gives the manager the caret and language to keep in the scene and in
    /// drafts, and where the view is scrolled to. Called now and then.
    pub fn sync(&self) {
        let Some(document) = self.shared.document.get() else {
            return;
        };
        let stored = self.shared.stored_for(&self.handle.peek());
        if stored != *self.shared.stored.borrow() {
            document.set_app_value(&stored);
            *self.shared.stored.borrow_mut() = stored;
        }
        document.set_scroll(self.handle.scroll().line as f32, 0.0);
    }

    /// Makes this document a copy of `other`'s text, language and
    /// indentation: Duplicate tab. It is unsaved.
    pub fn copy_of(&self, other: &EditorContent) {
        let (rope, settings, language, selection) = {
            let editor = other.handle.peek();
            (
                editor.rope().clone(),
                editor.settings,
                editor.language().clone(),
                editor.selection().clone(),
            )
        };
        self.handle.update(|editor| {
            editor.load(rope);
            editor.settings = settings;
            editor.set_language(language);
            editor.set_selection(selection);
            editor.history_mut().mark_unsaved();
        });
        if let Some(document) = self.shared.document.get() {
            document.mark_edited();
        }
    }

    fn preparer(&self, reason: LoadReason) -> Preparer {
        let document = self.shared.document.get();
        let stored: Stored = match (reason, document) {
            (LoadReason::Recover, Some(document)) => document.app_value_as().unwrap_or_default(),
            _ => Stored::default(),
        };
        let how = How {
            path: document.and_then(Document::path),
            force: self.shared.force.take(),
            allow_binary: self.shared.open_anyway.get(),
            format: stored.format.as_ref().and_then(StoredFormat::format),
        };
        Box::new(move |bytes| Prepared::new(decode_how(bytes, &how)))
    }

    /// Shows what a worker read.
    fn show(&self, read: Read, reason: LoadReason) {
        let prefs = crate::app::prefs_now();
        let shared = &self.shared;
        let document = shared.document.get();
        shared.binary.set(read.is_binary());
        *shared.notices.borrow_mut() = read.notices;
        *shared.config.borrow_mut() = read.config.clone();
        *shared.detected.borrow_mut() = read.language.id.clone();
        let mut format = read.format;
        if let Some(ending) = read.config.line_ending
            && read.rope.len_chars() == 0
        {
            format.line_ending = ending;
        }
        shared.format.set(format);
        let indent = read.config.indent.or(read.indent).unwrap_or_else(|| {
            if read.language.tabs {
                Indent::Tabs
            } else {
                prefs.indent()
            }
        });
        let tab_width = read.config.tab_width.unwrap_or(prefs.tab_width);
        let mut language = read.language;
        let stored: Option<Stored> = document
            .filter(|_| reason != LoadReason::Reload)
            .map(Document::app_value)
            .filter(|value| !value.is_null())
            .and_then(|value| serde_json::from_value(value).ok());
        if let Some(chosen) = stored
            .as_ref()
            .and_then(|stored| stored.language.as_deref())
            .and_then(bingux_text::language::by_name)
        {
            language = chosen;
        }
        match reason {
            LoadReason::Reload => {
                // As one undo step, with the caret and the scroll kept.
                let caret = self.handle.peek().selection().primary().head;
                let scroll = self.handle.scroll();
                let text = read.rope.to_string();
                self.handle.update(|editor| {
                    editor.set_text(&text);
                    editor.set_cursor(caret.min(editor.len_chars()), false);
                    editor.mark_saved();
                });
                self.handle.set_scroll(scroll);
            }
            LoadReason::Open | LoadReason::Recover => {
                self.handle.update(|editor| {
                    editor.load(read.rope);
                    editor.settings.indent = indent;
                    editor.settings.tab_width = tab_width;
                    editor.settings.auto_close = prefs.auto_close;
                    editor.set_language(language);
                    if reason == LoadReason::Recover {
                        editor.history_mut().mark_unsaved();
                    }
                });
                // Where the scene left it, or where the file was last left.
                let place = match (&stored, document) {
                    (Some(stored), Some(document)) => Some(positions::Position {
                        caret: stored.caret,
                        line: document.scroll().0 as usize,
                    }),
                    _ => document
                        .and_then(Document::path)
                        .and_then(|path| positions::position(&path)),
                };
                if let Some(place) = place {
                    self.handle.update(|editor| {
                        editor.set_cursor(place.caret.min(editor.len_chars()), false);
                    });
                    self.handle.set_scroll(ScrollPos {
                        line: place.line,
                        ..ScrollPos::default()
                    });
                }
            }
        }
        let editor = self.handle.peek();
        shared.seen.set((editor.revision(), editor.is_modified()));
        drop(editor);
        self.sync();
    }
}

impl DocumentContent for EditorContent {
    fn snapshot(&self) -> DocumentSnapshot {
        if self.shared.binary.get() {
            return DocumentSnapshot::encode(|| Err(tr!("text-editor-binary-not-saved")));
        }
        let rope = self.handle.peek().rope().clone();
        let format = self.shared.format.get();
        DocumentSnapshot::encode(move || encode(&rope, &format))
    }

    fn load(&self, bytes: Vec<u8>, reason: LoadReason) {
        let prepare = self.preparer(reason);
        if let Some(read) = prepare(bytes).take::<Read>() {
            self.show(read, reason);
        }
    }

    fn prepare(&self, reason: LoadReason) -> Option<Preparer> {
        Some(self.preparer(reason))
    }

    fn load_prepared(&self, prepared: Prepared, reason: LoadReason) {
        if let Some(read) = prepared.take::<Read>() {
            self.show(read, reason);
        }
    }

    fn attached(&self, document: Document) {
        let shared = self.shared.clone();
        shared.document.set(Some(document));
        // Every edit, wherever it comes from (keys, commands, menus), is the
        // document's: the unsaved mark, the draft.
        self.handle.on_update(move |editor| {
            let Some(document) = shared.document.get() else {
                return;
            };
            let revision = editor.revision();
            let modified = editor.is_modified();
            let (seen_revision, seen_modified) = shared.seen.replace((revision, modified));
            if modified {
                if revision != seen_revision {
                    document.mark_edited();
                }
            } else if seen_modified {
                // An undo back to the text that was saved.
                document.set_edited(false);
            }
        });
    }

    /// The first words of the text and `.md` or `.txt`, so a note saves under
    /// a name that says what it is.
    fn suggested_name(&self) -> Option<String> {
        let editor = self.handle.peek();
        let first: String = editor.line(0).chars().take(60).collect();
        let words = first
            .split(|ch: char| !ch.is_alphanumeric() && ch != '-' && ch != '_')
            .filter(|word| !word.is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join(" ");
        let stem = if words.is_empty() {
            self.shared.document.get()?.title()
        } else {
            words
        };
        let extension = if editor.language().is_markdown() {
            "md"
        } else {
            "txt"
        };
        Some(format!("{stem}.{extension}"))
    }

    fn will_save(&self) {
        let prefs = crate::app::prefs_now();
        let config = self.shared.config.borrow().clone();
        if self.shared.binary.get() {
            return;
        }
        self.handle.update(|editor| tidy(editor, &config, &prefs));
    }

    fn saved(&self, path: &Path, clean: bool) {
        self.shared
            .notices
            .borrow_mut()
            .retain(|notice| *notice == Notice::Binary);
        self.shared.hide_read_only.set(false);
        self.handle.update(|editor| {
            if clean {
                editor.mark_saved();
            }
            if editor.language().is_plain() {
                let head: String = editor
                    .rope()
                    .slice(..editor.len_chars().min(4096))
                    .to_string();
                editor.set_language(bingux_text::language::detect(Some(path), &head));
            }
        });
        let editor = self.handle.peek();
        self.shared
            .seen
            .set((editor.revision(), editor.is_modified()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The messages the content says, as the app registers them.
    fn catalogue() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| bingux_ui::i18n::register(bingux_ui::catalogue!("text-editor")));
    }

    fn text(read: &Read) -> String {
        read.rope.to_string()
    }

    #[test]
    fn utf8_with_and_without_a_byte_order_mark_is_read_and_written_back_the_same() {
        let plain = decode("héllo\n".as_bytes().to_vec(), None, None, false);
        assert_eq!(text(&plain), "héllo\n");
        assert!(!plain.format.bom);
        assert_eq!(
            encode(&plain.rope, &plain.format).unwrap(),
            "héllo\n".as_bytes()
        );

        let mut marked = vec![0xEF, 0xBB, 0xBF];
        marked.extend_from_slice("héllo\n".as_bytes());
        let read = decode(marked.clone(), None, None, false);
        assert!(read.format.bom);
        assert_eq!(text(&read), "héllo\n");
        assert_eq!(encode(&read.rope, &read.format).unwrap(), marked);
    }

    #[test]
    fn utf16_is_detected_and_written_back_as_utf16() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "hi\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let read = decode(bytes.clone(), None, None, false);
        assert_eq!(read.format.encoding, encoding_rs::UTF_16LE);
        assert_eq!(text(&read), "hi\n");
        assert_eq!(encode(&read.rope, &read.format).unwrap(), bytes);
    }

    #[test]
    fn a_legacy_encoding_is_written_back_in_the_same_encoding() {
        // "café" in Windows-1252, then a line ending, so it is not valid UTF-8.
        let bytes = vec![b'c', b'a', b'f', 0xE9, b'\r', b'\n'];
        let read = decode(bytes.clone(), None, Some(encoding_rs::WINDOWS_1252), false);
        assert_eq!(text(&read), "café\n");
        assert_eq!(read.format.line_ending, LineEnding::CrLf);
        assert_eq!(encode(&read.rope, &read.format).unwrap(), bytes);
    }

    #[test]
    fn line_endings_are_kept_when_the_text_is_written() {
        let read = decode(b"one\r\ntwo\r\n".to_vec(), None, None, false);
        assert_eq!(read.format.line_ending, LineEnding::CrLf);
        assert_eq!(text(&read), "one\ntwo\n");
        assert_eq!(encode(&read.rope, &read.format).unwrap(), b"one\r\ntwo\r\n");
    }

    #[test]
    fn text_the_encoding_cannot_hold_is_refused_with_what_to_do() {
        catalogue();
        let format = FileFormat {
            encoding: encoding_rs::WINDOWS_1252,
            ..FileFormat::default()
        };
        let advice = encode(&Rope::from_str("日本"), &format).unwrap_err();
        assert!(advice.contains("windows-1252"), "{advice}");
        assert!(advice.contains('日'), "{advice}");
        assert!(advice.contains("one other character"), "{advice}");
        assert!(advice.contains("UTF-8"), "{advice}");
    }

    #[test]
    fn a_binary_file_is_not_shown_unless_the_person_says_so() {
        let bytes = vec![0x7F, b'E', b'L', b'F', 0, 0, 1, 0, 0, 0];
        let read = decode(bytes.clone(), None, None, false);
        assert!(read.is_binary());
        assert_eq!(read.rope.len_chars(), 0);
        assert!(!decode(bytes, None, None, true).is_binary());
    }

    #[test]
    fn utf8_with_stray_latin1_bytes_says_so() {
        let mut bytes = "naïve text\n".as_bytes().to_vec();
        bytes.extend_from_slice(&[b'c', b'a', b'f', 0xE9, b'\n']);
        let read = decode(bytes, None, None, false);
        assert!(
            read.notices
                .iter()
                .any(|notice| matches!(notice, Notice::Mixed(_))),
            "{:?}",
            read.notices
        );
    }

    #[test]
    fn the_language_comes_from_the_name_and_a_shebang() {
        let by_name = decode(
            b"fn main() {}\n".to_vec(),
            Some(Path::new("a.rs")),
            None,
            false,
        );
        assert_eq!(by_name.language.id, "rust");
        let by_line = decode(
            b"#!/usr/bin/env python3\nprint(1)\n".to_vec(),
            Some(Path::new("script")),
            None,
            false,
        );
        assert_eq!(by_line.language.id, "python");
    }

    #[test]
    fn a_save_trims_and_ends_the_file_with_a_new_line_as_asked() {
        let mut editor = Editor::new("one  \ntwo");
        let mut prefs = Prefs::default();
        prefs.trim_on_save = true;
        prefs.final_newline = true;
        tidy(&mut editor, &EditorConfig::default(), &prefs);
        assert_eq!(editor.text(), "one\ntwo\n");

        // The project's settings win over the preferences.
        let mut editor = Editor::new("one  \ntwo");
        let config = EditorConfig {
            trim_trailing_whitespace: Some(false),
            insert_final_newline: Some(false),
            ..EditorConfig::default()
        };
        tidy(&mut editor, &config, &prefs);
        assert_eq!(editor.text(), "one  \ntwo");

        // An empty document stays empty.
        let mut empty = Editor::new("");
        tidy(&mut empty, &EditorConfig::default(), &prefs);
        assert_eq!(empty.text(), "");
    }

    #[test]
    fn how_a_file_was_written_survives_a_draft() {
        let format = FileFormat {
            encoding: encoding_rs::WINDOWS_1252,
            bom: false,
            line_ending: LineEnding::CrLf,
        };
        let stored = StoredFormat::of(&format);
        let json = serde_json::to_value(&stored).unwrap();
        let back: StoredFormat = serde_json::from_value(json).unwrap();
        assert_eq!(back.format(), Some(format));
        // A draft of such a file is read as that encoding, whatever the bytes look like.
        let bytes = vec![b'c', b'a', b'f', 0xE9, b'\r', b'\n'];
        let read = decode_how(
            bytes,
            &How {
                path: None,
                force: None,
                allow_binary: false,
                format: Some(format),
            },
        );
        assert_eq!(text(&read), "café\n");
        assert_eq!(read.format, format);
    }
}
