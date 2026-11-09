//! The window: a header bar with the documents as tabs, the notices of the
//! document on show, the find or go-to-line bar, the editor (beside its
//! Markdown preview for a Markdown file) and a status bar.
//!
//! Everything drawn comes from bingux-ui: the tabs (`bingux_ui::tabs`), the
//! editor, find bar and Markdown view (`bingux_ui::text_editor`), the file
//! chooser (`bingux_ui::files::choose`), banners, menus and dialogs. This
//! file decides what they do for a text editor.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use bingux_text::encoding::{self, LineEnding};
use bingux_text::encoding_rs;
use bingux_text::{Case, Editor, Indent, Language};
use bingux_ui::app::use_window_seed;
use bingux_ui::files::{ChooserRequest, Filter, choose};
use bingux_ui::prelude::*;
use bingux_ui::tabs::{Birth, Tab, TabContent, TabSet, Tabs, tab_items, use_tab_window};
use bingux_ui::text_editor::{
    EditorHandle, ScrollPos, ZoomStep, caret_caption, find_bar, go_to_line_bar, markdown_view,
    status_caption, text_editor, use_find,
};
use freya::prelude::*;

use crate::document::{self, Notice, Phase, SaveError};
use crate::launch::{self, Request};
use crate::prefs::{self, Prefs};
use crate::session::{self, Position as Place, SavedTab, Session};
use bingux_ui::shell::MenuItem;

/// How often drafts are written and files are checked for changes on disk.
const TICK: Duration = Duration::from_secs(2);
/// Markdown documents larger than this are not previewed.
const PREVIEW_LIMIT: usize = 2 << 20;

// ---------------------------------------------------------------------------
// Start-up
// ---------------------------------------------------------------------------

thread_local! {
    static START: RefCell<Option<(Prefs, Session)>> = const { RefCell::new(None) };
    /// The preferences as the window last set them, for work outside it.
    static PREFS: RefCell<Prefs> = RefCell::new(Prefs::default());
}

/// Keeps what `main` read before the window, for the first window to take.
pub fn prepare(prefs: Prefs, session: Session) {
    PREFS.with(|current| *current.borrow_mut() = prefs.clone());
    // Read the small state files now, not on the first frame.
    let _ = session::recent();
    START.with(|start| *start.borrow_mut() = Some((prefs, session)));
}

pub fn prefs_now() -> Prefs {
    PREFS.with(|prefs| prefs.borrow().clone())
}

pub fn window_size() -> (f32, f32) {
    match prefs_now().window {
        Some((width, height)) if width >= 480.0 && height >= 360.0 => (width, height),
        _ => (1000.0, 720.0),
    }
}

/// A tab shows one document.
#[derive(Clone, PartialEq)]
pub struct DocTab {
    pub doc: u64,
}

impl TabContent for DocTab {
    fn duplicate(&self) -> Self {
        DocTab {
            doc: document::duplicate(self.doc, &prefs_now()),
        }
    }
}

/// The tabs of the last session, or one new document.
fn restore(prefs: &Prefs, session: Session) -> TabSet<DocTab> {
    let mut docs = Vec::new();
    for saved in &session.tabs {
        if let Some(id) = restore_tab(prefs, saved) {
            docs.push(id);
        }
    }
    // Drafts no tab names, from a run that ended before it wrote its session.
    for draft in session::orphan_drafts(&session) {
        if let Some(id) = restore_tab(
            prefs,
            &SavedTab {
                draft: Some(draft),
                ..SavedTab::default()
            },
        ) {
            docs.push(id);
        }
    }
    let mut ids = docs.into_iter();
    let Some(first) = ids.next() else {
        return TabSet::new(DocTab {
            doc: document::new_untitled(prefs),
        });
    };
    let mut set = TabSet::new(DocTab { doc: first });
    for id in ids {
        set.open(DocTab { doc: id });
    }
    set.nth(session.active);
    set
}

fn restore_tab(prefs: &Prefs, saved: &SavedTab) -> Option<u64> {
    let draft_text = saved.draft.as_deref().and_then(session::read_draft);
    let language = saved
        .language
        .as_deref()
        .and_then(bingux_text::language::by_name);
    match (draft_text, &saved.path) {
        (Some(text), path) => {
            let mut editor = Editor::new(&text);
            editor.settings.indent =
                bingux_text::indent::detect(editor.rope()).unwrap_or_else(|| prefs.indent());
            editor.settings.tab_width = prefs.tab_width;
            editor.settings.auto_close = prefs.auto_close;
            let head: String = text.chars().take(4096).collect();
            editor.set_language(
                language.unwrap_or_else(|| bingux_text::language::detect(path.as_deref(), &head)),
            );
            editor.history_mut().mark_unsaved();
            editor.set_cursor(saved.caret, false);
            let id = document::insert(
                editor,
                path.clone(),
                encoding::FileFormat::default(),
                Phase::Ready,
                saved.draft.clone().unwrap_or_else(document::new_draft_name),
            );
            document::update(id, |doc| {
                if let Some(name) = &saved.untitled {
                    doc.untitled = name.clone();
                }
                doc.draft_revision = doc.handle.peek().revision();
                doc.handle.set_scroll(ScrollPos {
                    line: saved.line,
                    ..ScrollPos::default()
                });
            });
            // The file's format and stamp, so a save writes what it had.
            if let Some(path) = path {
                refresh_format(id, path.clone());
            }
            Some(id)
        }
        (None, Some(path)) => {
            let id = document::insert(
                Editor::new(""),
                Some(path.clone()),
                encoding::FileFormat::default(),
                Phase::Loading { len: 0 },
                document::new_draft_name(),
            );
            load(
                id,
                path.clone(),
                None,
                false,
                Some(Place {
                    caret: saved.caret,
                    line: saved.line,
                }),
                language,
            );
            Some(id)
        }
        (None, None) => None,
    }
}

/// Reads the format and stamp of `path` for a document restored from a
/// draft, without touching its text.
fn refresh_format(id: u64, path: PathBuf) {
    spawn(async move {
        let read = bingux_ui::task::spawn_blocking(move || document::read(&path, None, true)).await;
        if let Ok(loaded) = read {
            document::update(id, |doc| {
                doc.format = loaded.format;
                doc.stamp = Some(loaded.stamp);
                doc.read_only = !loaded.writable;
            });
        }
    });
}

/// Reads `path` into document `id` on a worker.
fn load(
    id: u64,
    path: PathBuf,
    encoding: Option<&'static encoding_rs::Encoding>,
    allow_binary: bool,
    position: Option<Place>,
    language: Option<Language>,
) {
    let len = std::fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    document::update(id, |doc| doc.phase = Phase::Loading { len });
    spawn(async move {
        let reading = path.clone();
        let read = bingux_ui::task::spawn_blocking(move || {
            document::read(&reading, encoding, allow_binary)
        })
        .await;
        match read {
            Ok(mut loaded) => {
                if let Some(language) = language {
                    loaded.language = language;
                }
                document::finish_loading(id, loaded, &prefs_now());
                let position = position.or_else(|| session::position(&path));
                if let (Some(position), Some(handle)) = (position, document::handle(id)) {
                    handle.update(|editor| {
                        let caret = position.caret.min(editor.len_chars());
                        editor.set_cursor(caret, false);
                    });
                    handle.set_scroll(ScrollPos {
                        line: position.line,
                        ..ScrollPos::default()
                    });
                }
            }
            Err(message) => {
                document::update(id, |doc| doc.phase = Phase::Failed(message));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

/// What the window does to its documents: every field is `Copy`.
#[derive(Clone, Copy)]
struct Actions {
    tabs: Tabs<DocTab>,
    window: Window,
    overlays: Overlays,
    prefs: State<Prefs>,
    editor_id: AccessibilityId,
}

impl Actions {
    fn current(&self) -> Option<u64> {
        self.tabs.set().peek().current().map(|tab| tab.content.doc)
    }

    fn focus_editor(&self) {
        let id = self.editor_id;
        spawn(async move {
            id.request_focus();
        });
    }

    fn new_document(&self) {
        let id = document::new_untitled(&self.prefs.peek());
        self.tabs.open(DocTab { doc: id });
        self.focus_editor();
    }

    /// Opens `path` in a tab, or goes to the tab that has it. An empty,
    /// untouched new document on show gives its tab to the file.
    fn open_path(&self, path: PathBuf) {
        let path = launch::absolute(&path);
        if let Some(existing) = document::find_path(&path) {
            let tab = self
                .tabs
                .set()
                .peek()
                .tabs()
                .iter()
                .find(|tab| tab.content.doc == existing)
                .map(|tab| tab.id);
            if let Some(tab) = tab {
                self.tabs.select(tab);
                return;
            }
        }
        let prefs = self.prefs.peek().clone();
        let exists = path.exists();
        let id = if exists {
            let id = document::insert(
                Editor::new(""),
                Some(path.clone()),
                encoding::FileFormat::default(),
                Phase::Loading { len: 0 },
                document::new_draft_name(),
            );
            load(id, path.clone(), None, false, None, None);
            id
        } else {
            // A name that is not there yet: a new document saved there.
            let mut editor = Editor::new("");
            editor.settings.indent = prefs.indent();
            editor.set_language(bingux_text::language::detect(Some(&path), ""));
            document::insert(
                editor,
                Some(path.clone()),
                encoding::FileFormat::default(),
                Phase::Ready,
                document::new_draft_name(),
            )
        };
        session::add_recent(&path);
        let blank = self.current().filter(|current| {
            document::with(*current, |doc| {
                doc.path.is_none() && !doc.is_modified() && doc.handle.peek().len_chars() == 0
            })
            .unwrap_or(false)
        });
        let current_tab = self.tabs.set().peek().current().map(|tab| tab.id);
        match (blank, current_tab) {
            (Some(old), Some(tab)) => {
                self.tabs
                    .update_if(tab, |_| true, move |content| content.doc = id);
                document::remove(old);
            }
            _ => {
                self.tabs.open(DocTab { doc: id });
            }
        }
        self.focus_editor();
    }

    fn open_dialog(&self) {
        let actions = *self;
        let folder = self.current().and_then(|id| {
            document::with(id, |doc| {
                doc.path
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .flatten()
        });
        let mut request = ChooserRequest::open()
            .title("Open")
            .multiple(true)
            .filter(Filter::new("Text files").mime("text/*"))
            .filter(Filter::new("All files"));
        if let Some(folder) = folder {
            request = request.current_folder(folder);
        }
        choose(self.window, request, move |chosen| {
            if let Some(chosen) = chosen {
                for path in chosen.paths {
                    actions.open_path(path);
                }
            }
        });
    }

    /// Saves document `id`, asking for a name first when it has none. `then`
    /// runs after a save that worked.
    fn save(&self, id: u64, then: Option<Rc<dyn Fn()>>) {
        let Some((path, read_only)) = document::with(id, |doc| (doc.path.clone(), doc.read_only))
        else {
            return;
        };
        match path {
            Some(path) if !read_only => self.write(id, path, then),
            _ => self.save_as(id, then),
        }
    }

    fn save_as(&self, id: u64, then: Option<Rc<dyn Fn()>>) {
        let actions = *self;
        let Some((name, folder)) = document::with(id, |doc| {
            let name = match &doc.path {
                Some(path) => document::display_name(path),
                None => suggested_name(doc),
            };
            let folder = doc
                .path
                .as_ref()
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .filter(|folder| !doc.read_only || folder.exists());
            (name, folder)
        }) else {
            return;
        };
        let mut request = ChooserRequest::save(name).title("Save as");
        if let Some(folder) = folder {
            request = request.current_folder(folder);
        }
        choose(self.window, request, move |chosen| {
            let Some(path) = chosen.and_then(|chosen| chosen.paths.into_iter().next()) else {
                return;
            };
            document::update(id, |doc| {
                doc.path = Some(path.clone());
                doc.read_only = false;
                doc.notices
                    .retain(|notice| *notice != Notice::ReadOnly && *notice != Notice::Deleted);
                let language = doc.handle.peek().language().clone();
                if language.is_plain() {
                    let head: String = doc
                        .handle
                        .peek()
                        .rope()
                        .slice(..doc.handle.peek().len_chars().min(4096))
                        .to_string();
                    let detected = bingux_text::language::detect(Some(&path), &head);
                    doc.handle.update(|editor| editor.set_language(detected));
                }
            });
            session::add_recent(&path);
            actions.write(id, path, then.clone());
        });
    }

    fn write(&self, id: u64, path: PathBuf, then: Option<Rc<dyn Fn()>>) {
        let prefs = self.prefs.peek().clone();
        let Some((handle, format, config, draft)) = document::with(id, |doc| {
            (
                doc.handle.clone(),
                doc.format,
                doc.config.clone(),
                doc.draft.clone(),
            )
        }) else {
            return;
        };
        let trim = config
            .trim_trailing_whitespace
            .unwrap_or(prefs.trim_on_save);
        let final_newline = config.insert_final_newline.unwrap_or(prefs.final_newline);
        handle.update(|editor| {
            if trim {
                editor.trim_trailing_whitespace();
            }
            if final_newline && editor.len_chars() > 0 {
                editor.ensure_final_newline();
            }
            editor.mark_saved();
        });
        let rope = handle.peek().rope().clone();
        let revision = handle.peek().revision();
        let overlays = self.overlays;
        let actions = *self;
        document::update(id, |doc| doc.saving = true);
        spawn(async move {
            let writing = path.clone();
            let result = bingux_ui::task::spawn_blocking(move || {
                document::write(&writing, &rope.to_string(), &format)
            })
            .await;
            document::update(id, |doc| doc.saving = false);
            match result {
                Ok(stamp) => {
                    document::update(id, |doc| {
                        doc.stamp = Some(stamp);
                        doc.draft_revision = revision;
                        doc.notices
                            .retain(|notice| matches!(notice, Notice::Binary));
                    });
                    session::remove_draft(draft);
                    if let Some(then) = then {
                        then();
                    }
                }
                Err(error) => {
                    handle.update(|editor| editor.history_mut().mark_unsaved());
                    let name = document::display_name(&path);
                    let encoding_name = encoding::encoding_label(format.encoding);
                    let message = error.message(&name, &encoding_name);
                    let title = format!("Could not save “{name}”");
                    let dialog_ = dialog(title, message)
                        .icon(DialogIcon::Error)
                        .cancel("Cancel");
                    let dialog_ = match error {
                        SaveError::Unmappable { .. } => dialog_.response(
                            "Save in UTF-8",
                            ResponseKind::Suggested,
                            move |_: ()| {
                                document::update(id, |doc| {
                                    doc.format.encoding = encoding_rs::UTF_8
                                });
                                actions.save(id, then.clone());
                            },
                        ),
                        SaveError::Permission | SaveError::Full => {
                            dialog_.response("Save as…", ResponseKind::Suggested, move |_: ()| {
                                actions.save_as(id, then.clone())
                            })
                        }
                        SaveError::Other => {
                            dialog_.response("Try again", ResponseKind::Suggested, move |_: ()| {
                                actions.save(id, then.clone())
                            })
                        }
                    };
                    overlays.show_dialog(dialog_);
                }
            }
        });
    }

    fn save_all(&self) {
        for id in document::ids() {
            let dirty = document::with(id, |doc| {
                doc.is_modified() && doc.path.is_some() && !doc.read_only
            })
            .unwrap_or(false);
            if dirty {
                self.save(id, None);
            }
        }
    }

    /// Reads the file again, as one undo step (Undo brings the text back).
    fn reload(&self, id: u64, encoding: Option<&'static encoding_rs::Encoding>) {
        let Some(path) = document::with(id, |doc| doc.path.clone()).flatten() else {
            return;
        };
        let handle = match document::handle(id) {
            Some(handle) => handle,
            None => return,
        };
        let caret = handle.peek().selection().primary().head;
        let scroll = handle.scroll();
        spawn(async move {
            let reading = path.clone();
            let read =
                bingux_ui::task::spawn_blocking(move || document::read(&reading, encoding, true))
                    .await;
            let Ok(loaded) = read else { return };
            let text = loaded.rope.to_string();
            handle.update(|editor| {
                editor.set_text(&text);
                editor.set_cursor(caret.min(editor.len_chars()), false);
                editor.mark_saved();
            });
            handle.set_scroll(scroll);
            document::update(id, |doc| {
                doc.format = loaded.format;
                doc.stamp = Some(loaded.stamp);
                doc.notices = loaded.notices;
                doc.read_only = !loaded.writable;
                doc.draft_revision = doc.handle.peek().revision();
            });
        });
    }

    fn update_prefs(&self, change: impl FnOnce(&mut Prefs)) {
        let mut prefs = self.prefs;
        change(&mut prefs.write());
        let now = prefs.peek().clone();
        PREFS.with(|current| *current.borrow_mut() = now.clone());
        prefs::save(&now);
    }

    fn zoom(&self, step: ZoomStep) {
        let next = self.prefs.peek().zoom_step(step);
        self.update_prefs(|prefs| prefs.zoom = next);
    }

    fn show_statistics(&self, id: u64) {
        let overlays = self.overlays;
        let Some((handle, title)) = document::with(id, |doc| (doc.handle.clone(), doc.title()))
        else {
            return;
        };
        let rope = handle.peek().rope().clone();
        let selected = handle.peek().primary_text();
        spawn(async move {
            let stats =
                bingux_ui::task::spawn_blocking(move || bingux_text::stats::count(&rope)).await;
            let selection =
                selected.map(|text| bingux_text::stats::count(&bingux_text::Rope::from_str(&text)));
            let mut lines = vec![
                format!("Words: {}", group(stats.words)),
                format!("Characters: {}", group(stats.characters)),
                format!(
                    "Characters without spaces: {}",
                    group(stats.characters_no_spaces)
                ),
                format!("Lines: {}", group(stats.lines)),
                format!("Paragraphs: {}", group(stats.paragraphs)),
                format!("Reading time: {}", minutes(stats.reading_minutes())),
                format!("Size: {}", size(stats.bytes)),
            ];
            if let Some(selection) = selection {
                lines.push(format!(
                    "Selected: {} words, {} characters",
                    group(selection.words),
                    group(selection.characters)
                ));
            }
            overlays.show_dialog(
                dialog(format!("Statistics of “{title}”"), lines.join("\n")).cancel("OK"),
            );
        });
    }
}

fn group(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn minutes(count: usize) -> String {
    match count {
        0 => "None".into(),
        1 => "1 minute".into(),
        count => format!("{count} minutes"),
    }
}

fn size(bytes: usize) -> String {
    match bytes {
        bytes if bytes < 1000 => format!("{bytes} bytes"),
        bytes if bytes < 1_000_000 => format!("{:.1} kB", bytes as f64 / 1000.0),
        bytes => format!("{:.1} MB", bytes as f64 / 1_000_000.0),
    }
}

/// A file name for an untitled document: its first words, or "Untitled".
fn suggested_name(doc: &document::Doc) -> String {
    let editor = doc.handle.peek();
    let first: String = editor.line(0).chars().take(60).collect();
    let words: String = first
        .split(|ch: char| !ch.is_alphanumeric() && ch != '-' && ch != '_')
        .filter(|word| !word.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join(" ");
    let stem = if words.is_empty() {
        doc.untitled.clone()
    } else {
        words
    };
    let extension = if editor.language().is_markdown() {
        "md"
    } else {
        "txt"
    };
    format!("{stem}.{extension}")
}

fn unsaved(tab: &DocTab) -> bool {
    document::with(tab.doc, |doc| doc.is_modified()).unwrap_or(false)
}

#[derive(PartialEq)]
struct Content;

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let location = use_location();
        let maximized = freya::borderless::use_maximized();
        let birth = use_window_seed::<Birth<DocTab>>();
        let start = use_hook(|| START.with(|start| start.borrow_mut().take()));
        let prefs = use_state({
            let start = start.clone();
            move || {
                start
                    .as_ref()
                    .map(|(prefs, _)| prefs.clone())
                    .unwrap_or_else(prefs_now)
            }
        });
        let set = use_state({
            let start = start.clone();
            move || {
                birth
                    .and_then(|birth| birth.into_set())
                    .unwrap_or_else(|| match start {
                        Some((prefs, session)) => restore(&prefs, session),
                        None => TabSet::new(DocTab {
                            doc: document::new_untitled(&prefs_now()),
                        }),
                    })
            }
        });
        let asks = use_state(Vec::<u64>::new);
        let tabs = Tabs::new(set, window, overlays).guard_tabs(unsaved, asks);
        let editor_id = use_a11y();
        let actions = Actions {
            tabs,
            window,
            overlays,
            prefs,
            editor_id,
        };
        let find = use_find();
        let mut go_to_open = use_state(|| false);
        let preview_on_phone = use_state(|| false);
        let mut hovering_files = use_state(|| false);
        let preview_ratio = use_state(|| 0.5f32);
        // The document the keyboard was last given to: a document that is
        // shown, or that finishes loading, takes the keyboard once.
        let focused_doc = use_state(|| None::<u64>);
        let _facts = document::watch();

        // A launch hands its file to this window.
        let handled = use_hook(|| Rc::new(RefCell::new(None::<Location>)));
        if handled.borrow().as_ref() != Some(&location) {
            *handled.borrow_mut() = Some(location.clone());
            let request = launch::request(&location);
            spawn(async move {
                match request {
                    Request::Open(path) => actions.open_path(path),
                    Request::New => actions.new_document(),
                    Request::Nothing => {}
                }
            });
        }

        // The background work of the window: drafts, the session, and files
        // changed on disk. One loop per window.
        use_hook(move || {
            spawn(async move {
                loop {
                    timer(TICK).await;
                    tick(actions);
                }
            });
            actions.focus_editor();
        });

        // A tab with unsaved changes that was asked to close.
        let pending = asks.read().first().copied();
        if let Some(tab_id) = pending {
            let mut asks = asks;
            spawn(async move {
                asks.write().retain(|id| *id != tab_id);
                ask_to_close(actions, tab_id);
            });
        }

        let (shown, current_tab) = {
            let set = set.read();
            (set.tabs().to_vec(), set.current().cloned())
        };
        let Some(current_tab) = current_tab else {
            return rect().into_element();
        };
        let doc_id = current_tab.content.doc;
        let Some((handle, phase, notices, read_only, format, path, doc_title, modified, saving)) =
            document::with(doc_id, |doc| {
                (
                    doc.handle.clone(),
                    doc.phase.clone(),
                    doc.notices.clone(),
                    doc.read_only,
                    doc.format,
                    doc.path.clone(),
                    doc.title(),
                    doc.is_modified(),
                    doc.saving,
                )
            })
        else {
            return rect().into_element();
        };
        let title = if modified {
            format!("• {doc_title}")
        } else {
            doc_title.clone()
        };
        let tab_window = use_tab_window(set, &title);
        let prefs_now = prefs.read().clone();
        let compact = is_compact();
        if phase == Phase::Ready && *focused_doc.peek() != Some(doc_id) {
            let mut focused_doc = focused_doc;
            spawn(async move {
                focused_doc.set(Some(doc_id));
                editor_id.request_focus();
            });
        }

        // ---- Header ----------------------------------------------------
        let titles: Vec<String> = shown
            .iter()
            .map(|tab| {
                document::with(tab.content.doc, |doc| {
                    if doc.is_modified() {
                        format!("• {}", doc.title())
                    } else {
                        doc.title()
                    }
                })
                .unwrap_or_default()
            })
            .collect();
        let header = HeaderBar::new(title.clone())
            .leading(
                icon_button(Icon::DocumentOpen, move |_| actions.open_dialog())
                    .tooltip("Open (Ctrl+O)"),
            )
            .trailing(
                icon_button(Icon::DocumentSave, move |_| {
                    if let Some(id) = actions.current() {
                        actions.save(id, None);
                    }
                })
                .tooltip("Save (Ctrl+S)")
                .busy(saving),
            )
            .trailing(
                menu_button("Main menu", main_menu(actions, find, go_to_open, doc_id))
                    .icon(Icon::OpenMenu),
            )
            .trailing(WindowControls)
            .tab_slot(tabs.strip(
                tab_items(&shown, titles, |_| Icon::TextXGeneric),
                &tab_window,
                move |_: ()| actions.new_document(),
            ));

        // ---- Notices ---------------------------------------------------
        let notice_banners: Vec<Element> = notices
            .iter()
            .filter_map(|notice| notice_banner(actions, doc_id, &doc_title, notice, modified))
            .collect();

        // ---- Bars ------------------------------------------------------
        let focus_back = move |_: ()| actions.focus_editor();
        let bar: Option<Element> = if *find.open.read() {
            Some(
                find_bar(find, handle.clone())
                    .read_only(read_only)
                    .on_close(focus_back)
                    .on_replaced(move |count: usize| {
                        if count > 1 {
                            overlays.toast(format!("Replaced {count} matches"));
                        }
                    })
                    .into(),
            )
        } else if *go_to_open.read() {
            Some(
                go_to_line_bar(handle.clone(), go_to_open)
                    .on_close(focus_back)
                    .into(),
            )
        } else {
            None
        };

        // ---- The document ----------------------------------------------
        let is_markdown = handle.read().language().is_markdown();
        let body: Element = match &phase {
            Phase::Loading { len } => {
                let what = path
                    .as_deref()
                    .map(document::display_name)
                    .unwrap_or_default();
                let text = if *len > 8 << 20 {
                    format!("Opening “{what}” ({})…", size(*len as usize))
                } else {
                    format!("Opening “{what}”…")
                };
                rect()
                    .tw(
                        &tokens,
                        &[w_full(), h_full(), center(), flex_col(), gap(S3)],
                    )
                    .child(spinner(SpinnerSize::Medium))
                    .child(status_caption(&tokens, text))
                    .into()
            }
            Phase::Failed(message) => {
                let retry_path = path.clone();
                status_page(Icon::DialogWarning, "Could not open this file")
                    .description(message.clone())
                    .action("Try again", move |_| {
                        if let Some(path) = retry_path.clone() {
                            load(doc_id, path, None, false, None, None);
                        }
                    })
                    .into()
            }
            Phase::Ready if notices.contains(&Notice::Binary) => {
                let retry_path = path.clone();
                status_page(Icon::ApplicationXGeneric, "This file is not text")
                    .description("It holds data that is not text. Saving it here can damage it.")
                    .action("Open anyway", move |_| {
                        if let Some(path) = retry_path.clone() {
                            document::update(doc_id, |doc| doc.notices.clear());
                            load(doc_id, path, None, true, None, None);
                        }
                    })
                    .into()
            }
            Phase::Ready => {
                let marks = find.marks();
                let view = text_editor(handle.clone())
                    .line_numbers(prefs_now.line_numbers && !compact)
                    .wrap(prefs_now.wrap)
                    .show_whitespace(prefs_now.show_whitespace)
                    .highlight_line(prefs_now.highlight_line)
                    .zoom(prefs_now.zoom, move |step: ZoomStep| actions.zoom(step))
                    .placeholder("Start typing or drop a file")
                    .label(doc_title.clone())
                    .a11y_id(editor_id)
                    .menu_extra(text_menu(handle.clone()));
                let view = match marks {
                    Some((matches, current)) => view.matches(matches, current),
                    None => view,
                };
                let editor: Element = view.into();
                let small = handle.read().rope().len_bytes() <= PREVIEW_LIMIT;
                if is_markdown && prefs_now.markdown_preview && small {
                    let source: Rc<str> = handle.read().text().into();
                    let (top, _) = handle.shown_lines();
                    let preview = markdown_view(source)
                        .follow(top)
                        .on_link(move |url: String| open_link(&url, overlays));
                    if compact {
                        if *preview_on_phone.read() {
                            preview.into()
                        } else {
                            editor
                        }
                    } else {
                        split_pane(editor, preview, preview_ratio).into()
                    }
                } else {
                    editor
                }
            }
        };

        // ---- Status bar --------------------------------------------------
        let status = status_bar(
            actions,
            doc_id,
            handle.clone(),
            format,
            compact,
            is_markdown,
            preview_on_phone,
        );

        // ---- Keys --------------------------------------------------------
        let tab_keys = tabs.shortcuts(move |_: ()| actions.new_document());
        let key_handle = handle.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let ctrl = event.modifiers.contains(Modifiers::CONTROL);
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            let alt = event.modifiers.contains(Modifiers::ALT);
            let mut find_open = find.open;
            let handled = match &event.key {
                Key::Character(text) if ctrl && !alt => match (text.to_lowercase().as_str(), shift)
                {
                    ("n", false) => {
                        actions.new_document();
                        true
                    }
                    ("o", false) => {
                        actions.open_dialog();
                        true
                    }
                    ("s", false) => {
                        actions.save(doc_id, None);
                        true
                    }
                    ("s", true) => {
                        actions.save_as(doc_id, None);
                        true
                    }
                    ("f", false) => {
                        go_to_open.set(false);
                        find.open_from(&key_handle, false);
                        true
                    }
                    ("h", false) => {
                        go_to_open.set(false);
                        find.open_from(&key_handle, true);
                        true
                    }
                    ("i", false) => {
                        find_open.set(false);
                        go_to_open.set(true);
                        true
                    }
                    ("g", _) => {
                        if !*find.open.peek() && !find.query.peek().is_empty() {
                            find_open.set(true);
                        }
                        find.step(&key_handle, !shift);
                        true
                    }
                    ("v", true) => {
                        actions
                            .update_prefs(|prefs| prefs.markdown_preview = !prefs.markdown_preview);
                        true
                    }
                    _ => false,
                },
                Key::Character(text) if alt && !ctrl && text.eq_ignore_ascii_case("z") => {
                    actions.update_prefs(|prefs| prefs.wrap = !prefs.wrap);
                    true
                }
                Key::Named(NamedKey::F3) if !ctrl && !alt => {
                    find.step(&key_handle, !shift);
                    true
                }
                Key::Named(NamedKey::Escape) if *find.open.peek() || *go_to_open.peek() => {
                    find_open.set(false);
                    go_to_open.set(false);
                    actions.focus_editor();
                    true
                }
                _ => false,
            };
            if handled {
                event.stop_propagation();
                return;
            }
            tab_keys(event);
        };

        let drop_overlay = hovering_files.read().then(|| {
            rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .tw(&tokens, &[w_full(), h_full(), center(), bg(Paint::Scrim)])
                .layer(Layer::Overlay)
                .child(
                    rect()
                        .tw(&tokens, BOX)
                        .tw(
                            &tokens,
                            &[
                                p(S6),
                                flex_col(),
                                items(Align::Center),
                                gap(S2),
                                bg(Paint::Popover),
                            ],
                        )
                        .child(icon(
                            Icon::DocumentOpen,
                            IconSize::App,
                            Paint::Accent.color(&tokens),
                        ))
                        .child(typography::SECTION.label(&tokens, "Drop to open")),
                )
        });

        tab_window
            .root(rect())
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .on_key_down(on_key_down)
            .on_global_file_hover(move |_| hovering_files.set_if_modified(true))
            .on_global_file_hover_cancelled(move |_| hovering_files.set_if_modified(false))
            .on_file_drop(move |event: Event<FileEventData>| {
                hovering_files.set_if_modified(false);
                for path in event.file_paths.clone() {
                    actions.open_path(path);
                }
            })
            .on_sized(move |event: Event<SizedEventData>| {
                tab_window.sized(event.area.width(), event.area.height());
                if !is_compact() && !*maximized.peek() {
                    let size = (event.area.width().round(), event.area.height().round());
                    if prefs_now_window() != Some(size) {
                        PREFS.with(|prefs| prefs.borrow_mut().window = Some(size));
                    }
                }
            })
            .child(header)
            .children(notice_banners)
            .maybe_child(bar)
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(body))
            .child(status)
            .maybe_child(drop_overlay)
            .into_element()
    }
}

fn prefs_now_window() -> Option<(f32, f32)> {
    PREFS.with(|prefs| prefs.borrow().window)
}

/// Every two seconds: write drafts of changed documents, the session and
/// the remembered positions, and look for files changed on disk.
fn tick(actions: Actions) {
    let set = actions.tabs.set();
    let tabs: Vec<Tab<DocTab>> = set.peek().tabs().to_vec();
    let active = set
        .peek()
        .tabs()
        .iter()
        .position(|tab| tab.id == set.peek().active())
        .unwrap_or(0);
    let mut saved = Vec::new();
    for tab in &tabs {
        let id = tab.content.doc;
        let Some((handle, path, draft, draft_revision, untitled, phase)) =
            document::with(id, |doc| {
                (
                    doc.handle.clone(),
                    doc.path.clone(),
                    doc.draft.clone(),
                    doc.draft_revision,
                    doc.untitled.clone(),
                    doc.phase.clone(),
                )
            })
        else {
            continue;
        };
        if phase != Phase::Ready {
            if let Some(path) = path {
                saved.push(SavedTab {
                    path: Some(path),
                    ..SavedTab::default()
                });
            }
            continue;
        }
        let editor = handle.peek();
        let modified = editor.is_modified();
        let revision = editor.revision();
        let caret = editor.selection().primary().head;
        let line = handle.scroll().line;
        let empty = editor.len_chars() == 0;
        if modified && revision != draft_revision {
            session::write_draft(draft.clone(), editor.text());
            drop(editor);
            document::update(id, |doc| doc.draft_revision = revision);
        } else {
            drop(editor);
        }
        if let Some(path) = &path {
            session::remember_position(path, Place { caret, line });
        }
        // An empty new document is not worth restoring.
        if path.is_none() && empty && !modified {
            continue;
        }
        saved.push(SavedTab {
            path: path.clone(),
            draft: modified.then(|| draft.clone()),
            untitled: path.is_none().then_some(untitled),
            caret,
            line,
            language: None,
        });
        if let Some(path) = path {
            check_disk(actions, id, path);
        }
    }
    session::save_session(&Session {
        tabs: saved,
        active,
    });
    let prefs = prefs_now();
    prefs::save(&prefs);
}

/// Compares the file with what was read or written last.
fn check_disk(actions: Actions, id: u64, path: PathBuf) {
    let Some((known, saving)) = document::with(id, |doc| (doc.stamp, doc.saving)) else {
        return;
    };
    let Some(known) = known else { return };
    if saving {
        return;
    }
    spawn(async move {
        let now = bingux_ui::task::spawn_blocking(move || document::stamp_now(&path)).await;
        let Some((modified, notices)) =
            document::with(id, |doc| (doc.is_modified(), doc.notices.clone()))
        else {
            return;
        };
        match now {
            None => {
                if !notices.contains(&Notice::Deleted) {
                    document::update(id, |doc| doc.notices.push(Notice::Deleted));
                }
            }
            Some(stamp) if stamp != known => {
                if modified {
                    if !notices.contains(&Notice::ChangedOnDisk) {
                        document::update(id, |doc| {
                            doc.stamp = Some(stamp);
                            doc.notices.push(Notice::ChangedOnDisk);
                        });
                    }
                } else {
                    // Nothing to lose: take the new text, as one undo step.
                    document::update(id, |doc| {
                        doc.stamp = Some(stamp);
                        doc.notices
                            .retain(|notice| !matches!(notice, Notice::Deleted | Notice::Reloaded));
                        doc.notices.push(Notice::Reloaded);
                    });
                    actions.reload(id, None);
                }
            }
            Some(_) => {
                if notices.contains(&Notice::Deleted) {
                    document::update(id, |doc| {
                        doc.notices.retain(|notice| *notice != Notice::Deleted)
                    });
                }
            }
        }
    });
}

/// The question before a tab with unsaved changes closes.
fn ask_to_close(actions: Actions, tab_id: u64) {
    let Some(doc_id) = actions.tabs.set().peek().content(tab_id).map(|tab| tab.doc) else {
        return;
    };
    let Some(name) = document::with(doc_id, |doc| doc.title()) else {
        return;
    };
    let close = move || {
        if let Some(draft) = document::with(doc_id, |doc| doc.draft.clone()) {
            session::remove_draft(draft);
        }
        actions.tabs.close_now(tab_id);
    };
    let save_close: Rc<dyn Fn()> = Rc::new(close.clone());
    actions.overlays.show_dialog(
        dialog(
            format!("Save changes to “{name}”?"),
            "If you do not save, your changes are lost.",
        )
        .icon(DialogIcon::Warning)
        .response("Discard", ResponseKind::Destructive, move |_: ()| close())
        .cancel("Cancel")
        .response("Save", ResponseKind::Suggested, move |_: ()| {
            actions.save(doc_id, Some(save_close.clone()))
        })
        .default_action("save"),
    );
}

fn open_link(url: &str, overlays: Overlays) {
    // Until bingux_ui::platform::open lands on this branch: the desktop's
    // opener, for web and mail addresses only.
    if !["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        overlays.toast("Only web and mail links open from the preview");
        return;
    }
    let url = url.to_owned();
    bingux_ui::task::spawn_detached("text-editor-link", move || {
        let _ = std::process::Command::new("xdg-open")
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// The banner for one notice, or none.
fn notice_banner(
    actions: Actions,
    id: u64,
    name: &str,
    notice: &Notice,
    modified: bool,
) -> Option<Element> {
    let dismiss = move |gone: Notice| {
        move |_: ()| {
            let gone = gone.clone();
            document::update(id, move |doc| doc.notices.retain(|notice| *notice != gone));
        }
    };
    let banner_: Banner = match notice {
        Notice::Reloaded => banner(format!("“{name}” changed on disk and was read again."))
            .kind(BannerKind::Info)
            .action("Undo", move |_| {
                if let Some(handle) = document::handle(id) {
                    handle.update(|editor| editor.undo());
                }
                document::update(id, |doc| {
                    doc.notices.retain(|notice| *notice != Notice::Reloaded)
                });
            })
            .dismissible(dismiss(Notice::Reloaded)),
        Notice::ChangedOnDisk => banner(format!(
            "“{name}” changed on disk. Your changes are not saved."
        ))
        .kind(BannerKind::Warning)
        .action("Read again", move |_| {
            document::update(id, |doc| {
                doc.notices
                    .retain(|notice| *notice != Notice::ChangedOnDisk)
            });
            actions.reload(id, None);
        })
        .action("Keep mine", dismiss(Notice::ChangedOnDisk)),
        Notice::Deleted => banner(format!("“{name}” was moved or deleted."))
            .kind(BannerKind::Warning)
            .action("Save", move |_| actions.save(id, None))
            .dismissible(dismiss(Notice::Deleted)),
        Notice::ReadOnly => banner("You can read this file but cannot save changes to it.")
            .kind(BannerKind::Info)
            .action("Save as…", move |_| actions.save_as(id, None))
            .dismissible(dismiss(Notice::ReadOnly)),
        Notice::Mixed(legacy) => banner(format!(
            "This file mixes UTF-8 with {} text. Saving writes it all as UTF-8.",
            encoding::encoding_label(legacy)
        ))
        .kind(BannerKind::Info)
        .dismissible(dismiss(notice.clone())),
        Notice::Lossy => {
            banner("Some characters do not fit this encoding and show as �. Saving replaces them.")
                .kind(BannerKind::Warning)
                .dismissible(dismiss(Notice::Lossy))
        }
        Notice::MixedLineEndings => {
            if !modified {
                banner("This file mixes line endings. Saving writes one kind.")
                    .kind(BannerKind::Info)
                    .dismissible(dismiss(Notice::MixedLineEndings))
            } else {
                return None;
            }
        }
        Notice::Binary => return None,
    };
    Some(banner_.into())
}

/// The editor's context menu after Cut, Copy, Paste and Select all.
fn text_menu(handle: EditorHandle) -> Vec<MenuItem> {
    let comment = handle.clone();
    let duplicate = handle.clone();
    vec![
        menu_item("Comment or uncomment", move |_| {
            comment.update(|editor| editor.toggle_comment())
        })
        .shortcut("Ctrl+/"),
        menu_item("Duplicate line", move |_| {
            duplicate.update(|editor| editor.duplicate())
        })
        .shortcut("Ctrl+D"),
        submenu("Change case", case_items(handle)),
    ]
}

fn case_items(handle: EditorHandle) -> Vec<MenuItem> {
    let upper = handle.clone();
    let lower = handle.clone();
    let title = handle;
    vec![
        menu_item("Uppercase", move |_| {
            upper.update(|editor| editor.transform_case(Case::Upper))
        }),
        menu_item("Lowercase", move |_| {
            lower.update(|editor| editor.transform_case(Case::Lower))
        }),
        menu_item("Title case", move |_| {
            title.update(|editor| editor.transform_case(Case::Title))
        }),
    ]
}

/// The main menu.
fn main_menu(
    actions: Actions,
    find: bingux_ui::text_editor::FindState,
    go_to_open: State<bool>,
    id: u64,
) -> Vec<MenuItem> {
    let prefs = actions.prefs.read().clone();
    let handle = document::handle(id);
    let has_path = document::with(id, |doc| doc.path.is_some()).unwrap_or(false);
    let mut items = actions.tabs.menu_items(move |_: ()| actions.new_document());
    items.push(menu_separator());
    items.push(menu_item("Open…", move |_| actions.open_dialog()).shortcut("Ctrl+O"));
    let recent = session::recent();
    let mut recent_items: Vec<MenuItem> = recent
        .iter()
        .take(12)
        .map(|path| {
            let open = path.clone();
            menu_item(document::display_name(path), move |_| {
                actions.open_path(open.clone())
            })
        })
        .collect();
    if recent_items.is_empty() {
        recent_items.push(menu_item("No recent files", |_| {}).disabled());
    } else {
        recent_items.push(menu_separator());
        recent_items.push(menu_item("Clear recent files", |_| session::clear_recent()));
    }
    items.push(submenu("Open recent", recent_items));
    items.push(menu_item("Save", move |_| actions.save(id, None)).shortcut("Ctrl+S"));
    items.push(menu_item("Save as…", move |_| actions.save_as(id, None)).shortcut("Ctrl+Shift+S"));
    items.push(menu_item("Save all", move |_| actions.save_all()));
    let reload = menu_item("Read again from disk", move |_| actions.reload(id, None));
    items.push(if has_path { reload } else { reload.disabled() });
    items.push(menu_separator());
    if let Some(handle) = handle.clone() {
        let find_handle = handle.clone();
        let replace_handle = handle.clone();
        let mut go_to = go_to_open;
        items.push(
            menu_item("Find…", move |_| find.open_from(&find_handle, false)).shortcut("Ctrl+F"),
        );
        items.push(
            menu_item("Find and replace…", move |_| {
                find.open_from(&replace_handle, true)
            })
            .shortcut("Ctrl+H"),
        );
        items.push(menu_item("Go to line…", move |_| go_to.set(true)).shortcut("Ctrl+I"));
        let sort = handle.clone();
        let join = handle.clone();
        let trim = handle.clone();
        let comment = handle.clone();
        let duplicate = handle.clone();
        let mut text = vec![
            menu_item("Comment or uncomment", move |_| {
                comment.update(|editor| editor.toggle_comment())
            })
            .shortcut("Ctrl+/"),
            menu_item("Duplicate line", move |_| {
                duplicate.update(|editor| editor.duplicate())
            })
            .shortcut("Ctrl+D"),
            menu_item("Join lines", move |_| {
                join.update(|editor| editor.join_lines())
            })
            .shortcut("Ctrl+J"),
            menu_item("Sort lines", move |_| {
                sort.update(|editor| editor.sort_lines())
            }),
            menu_item("Remove trailing spaces", move |_| {
                trim.update(|editor| editor.trim_trailing_whitespace());
            }),
            menu_separator(),
        ];
        text.extend(case_items(handle.clone()));
        items.push(submenu("Text", text));
    }
    let view = vec![
        menu_item("Line numbers", move |_| {
            actions.update_prefs(|prefs| prefs.line_numbers = !prefs.line_numbers)
        })
        .checked(prefs.line_numbers),
        menu_item("Wrap lines", move |_| {
            actions.update_prefs(|prefs| prefs.wrap = !prefs.wrap)
        })
        .checked(prefs.wrap)
        .shortcut("Alt+Z"),
        menu_item("Show spaces and tabs", move |_| {
            actions.update_prefs(|prefs| prefs.show_whitespace = !prefs.show_whitespace)
        })
        .checked(prefs.show_whitespace),
        menu_item("Highlight current line", move |_| {
            actions.update_prefs(|prefs| prefs.highlight_line = !prefs.highlight_line)
        })
        .checked(prefs.highlight_line),
        menu_item("Markdown preview", move |_| {
            actions.update_prefs(|prefs| prefs.markdown_preview = !prefs.markdown_preview)
        })
        .checked(prefs.markdown_preview)
        .shortcut("Ctrl+Shift+V"),
        menu_separator(),
        menu_item("Zoom in", move |_| actions.zoom(ZoomStep::In)).shortcut("Ctrl+="),
        menu_item("Zoom out", move |_| actions.zoom(ZoomStep::Out)).shortcut("Ctrl+-"),
        menu_item("Actual size", move |_| actions.zoom(ZoomStep::Reset)).shortcut("Ctrl+0"),
    ];
    items.push(submenu("View", view));
    let editing = vec![
        menu_item("Close brackets and quotes", move |_| {
            actions.update_prefs(|prefs| prefs.auto_close = !prefs.auto_close);
            let on = prefs_now().auto_close;
            for id in document::ids() {
                if let Some(handle) = document::handle(id) {
                    handle.update(|editor| editor.settings.auto_close = on);
                }
            }
        })
        .checked(prefs.auto_close),
        menu_item("Trim spaces on save", move |_| {
            actions.update_prefs(|prefs| prefs.trim_on_save = !prefs.trim_on_save)
        })
        .checked(prefs.trim_on_save),
        menu_item("End with a new line", move |_| {
            actions.update_prefs(|prefs| prefs.final_newline = !prefs.final_newline)
        })
        .checked(prefs.final_newline),
    ];
    items.push(submenu("Editing", editing));
    items.push(menu_item("Statistics", move |_| {
        actions.show_statistics(id)
    }));
    items.push(menu_separator());
    let overlays = actions.overlays;
    items.push(
        menu_item("Keyboard shortcuts", move |_| {
            overlays.show_dialog(dialog("Keyboard shortcuts", SHORTCUTS).cancel("OK"))
        })
        .shortcut("Ctrl+?"),
    );
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}

/// The status bar: where the caret is, and the document's language,
/// indentation, encoding and line endings, each a menu.
fn status_bar(
    actions: Actions,
    id: u64,
    handle: EditorHandle,
    format: encoding::FileFormat,
    compact: bool,
    markdown: bool,
    mut preview_on_phone: State<bool>,
) -> Element {
    let tokens = use_tokens();
    let editor = handle.read();
    let caret = caret_caption(&editor);
    let language = editor.language().clone();
    let indent = editor.settings.indent;
    let tab_width = editor.settings.tab_width;
    drop(editor);
    let language_menu = language_items(handle.clone(), &language);
    let indent_menu = indent_items(handle.clone(), indent, tab_width);
    let encoding_menu = encoding_items(actions, id, format);
    let ending_menu: Vec<MenuItem> = LineEnding::ALL
        .iter()
        .map(|ending| {
            let ending = *ending;
            menu_item(ending.describe(), move |_| {
                document::update(id, |doc| doc.format.line_ending = ending);
                if let Some(handle) = document::handle(id) {
                    // A change of line ending is a change to save.
                    handle.update(|editor| editor.history_mut().mark_unsaved());
                }
            })
            .checked(format.line_ending == ending)
        })
        .collect();
    let row = rect()
        .tw(
            &tokens,
            &[
                w_full(),
                h(Dim::Control),
                flex_row(),
                grow_children(),
                items(Align::Center),
                gap(S1),
                px(S2),
                bg(Paint::Surface),
            ],
        )
        .child(
            rect()
                .tw(&tokens, &[w_flex()])
                .child(status_caption(&tokens, caret)),
        );
    if compact {
        let mut items = vec![
            submenu(language.name.clone(), language_menu),
            submenu(indent.label(), indent_menu),
            submenu(format.encoding_label(), encoding_menu),
            submenu(format.line_ending.label(), ending_menu),
        ];
        if markdown {
            let on = *preview_on_phone.read();
            items.insert(
                0,
                menu_item(if on { "Show text" } else { "Show preview" }, move |_| {
                    preview_on_phone.set(!on)
                }),
            );
        }
        return row
            .child(menu_button("Document", items).icon(Icon::ViewMore))
            .into();
    }
    row.child(menu_button(language.name.clone(), language_menu))
        .child(menu_button(indent.label(), indent_menu))
        .child(menu_button(format.encoding_label(), encoding_menu))
        .child(menu_button(format.line_ending.label(), ending_menu))
        .into()
}

/// The languages a status bar menu offers: the common ones, then all the
/// rest by first letter.
fn language_items(handle: EditorHandle, current: &Language) -> Vec<MenuItem> {
    const COMMON: [&str; 16] = [
        "plain",
        "markdown",
        "rust",
        "python",
        "javascript",
        "typescript",
        "c",
        "cpp",
        "go",
        "java",
        "shell",
        "html",
        "css",
        "json",
        "yaml",
        "toml",
    ];
    let pick = |language: Language, handle: EditorHandle, current: &Language| {
        let checked = language.id == current.id;
        menu_item(language.name.clone(), move |_| {
            let language = language.clone();
            handle.update(|editor| editor.set_language(language));
        })
        .checked(checked)
    };
    let mut items: Vec<MenuItem> = COMMON
        .iter()
        .filter_map(|id| bingux_text::language::by_name(id))
        .map(|language| pick(language, handle.clone(), current))
        .collect();
    let all = bingux_text::language::all();
    let mut groups: Vec<(char, Vec<MenuItem>)> = Vec::new();
    for language in all
        .into_iter()
        .filter(|language| !COMMON.contains(&language.id.as_str()))
    {
        let letter = language
            .name
            .chars()
            .next()
            .map_or('#', |ch| ch.to_ascii_uppercase());
        let letter = if letter.is_ascii_alphabetic() {
            letter
        } else {
            '#'
        };
        let item = pick(language, handle.clone(), current);
        match groups.last_mut() {
            Some((at, list)) if *at == letter => list.push(item),
            _ => groups.push((letter, vec![item])),
        }
    }
    items.push(menu_separator());
    items.push(submenu(
        "Other languages",
        groups
            .into_iter()
            .map(|(letter, list)| submenu(letter.to_string(), list))
            .collect(),
    ));
    items
}

fn indent_items(handle: EditorHandle, indent: Indent, tab_width: usize) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for choice in [
        Indent::Spaces(2),
        Indent::Spaces(4),
        Indent::Spaces(8),
        Indent::Tabs,
    ] {
        let handle = handle.clone();
        items.push(
            menu_item(choice.label(), move |_| {
                handle.update(|editor| editor.settings.indent = choice)
            })
            .checked(indent == choice),
        );
    }
    items.push(menu_separator());
    let mut widths = Vec::new();
    for width in [2usize, 4, 8] {
        let handle = handle.clone();
        widths.push(
            menu_item(format!("{width} columns"), move |_| {
                handle.update(|editor| editor.settings.tab_width = width)
            })
            .checked(tab_width == width),
        );
    }
    items.push(submenu("Tab width", widths));
    let convert = handle.clone();
    items.push(menu_item("Detect indentation", move |_| {
        convert.update(|editor| {
            if let Some(found) = bingux_text::indent::detect(editor.rope()) {
                editor.settings.indent = found;
            }
        });
    }));
    items
}

fn encoding_items(actions: Actions, id: u64, format: encoding::FileFormat) -> Vec<MenuItem> {
    let has_path = document::with(id, |doc| doc.path.is_some()).unwrap_or(false);
    let groups = encoding::encodings();
    let save_with: Vec<MenuItem> = groups
        .iter()
        .map(|(group, list)| {
            submenu(
                *group,
                list.iter()
                    .map(|encoding| {
                        let encoding = *encoding;
                        menu_item(encoding::encoding_label(encoding), move |_| {
                            document::update(id, |doc| doc.format.encoding = encoding);
                            if let Some(handle) = document::handle(id) {
                                handle.update(|editor| editor.history_mut().mark_unsaved());
                            }
                        })
                        .checked(format.encoding == encoding)
                    })
                    .collect(),
            )
        })
        .collect();
    let reopen: Vec<MenuItem> = groups
        .iter()
        .map(|(group, list)| {
            submenu(
                *group,
                list.iter()
                    .map(|encoding| {
                        let encoding = *encoding;
                        menu_item(encoding::encoding_label(encoding), move |_| {
                            actions.reload(id, Some(encoding))
                        })
                    })
                    .collect(),
            )
        })
        .collect();
    // Labels short enough for the narrow menu (`Dim::Menu`), which cuts longer ones off.
    let mut items = vec![submenu("Encode as", save_with)];
    let reopen_item = submenu("Read again as", reopen);
    items.push(if has_path {
        reopen_item
    } else {
        reopen_item.disabled()
    });
    if format.encoding == encoding_rs::UTF_8 {
        let bom = format.bom;
        items.push(
            menu_item("Byte order mark", move |_| {
                document::update(id, |doc| doc.format.bom = !bom);
                if let Some(handle) = document::handle(id) {
                    handle.update(|editor| editor.history_mut().mark_unsaved());
                }
            })
            .checked(bom),
        );
    }
    items
}

const SHORTCUTS: &str = "\
Ctrl+N, Ctrl+T  New document\n\
Ctrl+O  Open\n\
Ctrl+S, Ctrl+Shift+S  Save, save as\n\
Ctrl+W  Close the document\n\
Ctrl+Shift+T  Reopen the document closed last\n\
Ctrl+F, Ctrl+H  Find, find and replace\n\
Ctrl+G, F3  Next result (with Shift, the previous)\n\
Ctrl+I  Go to line\n\
Ctrl+D  Duplicate the line\n\
Ctrl+Shift+K  Delete the line\n\
Alt+Up, Alt+Down  Move the line\n\
Ctrl+/  Comment or uncomment\n\
Ctrl+J  Join lines\n\
Ctrl+L  Select the line\n\
Ctrl+Shift+D  Add the next occurrence\n\
Ctrl+Shift+L  Select every occurrence\n\
Ctrl+Alt+Up, Ctrl+Alt+Down  Add a caret above or below\n\
Alt+click  Add or remove a caret\n\
Ctrl+Backspace  Delete a word\n\
Ctrl+M  Go to the matching bracket\n\
Alt+Z  Wrap lines\n\
Ctrl+Shift+V  Markdown preview\n\
Ctrl+=, Ctrl+-, Ctrl+0  Zoom";
