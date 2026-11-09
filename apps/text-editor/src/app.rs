//! The window: a header bar with the documents as tabs, the notices of the
//! document on show, the find or go-to-line bar, the editor (beside its
//! Markdown preview for a Markdown file) and a status bar.
//!
//! Everything drawn comes from bingux-ui: the tabs (`bingux_ui::tabs`), the
//! editor, find bar and Markdown view (`bingux_ui::text_editor`), the file
//! chooser (`bingux_ui::files::choose`), banners, menus and dialogs. This
//! file decides what they do for a text editor.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::content::{EditorContent, Notice};
use bingux_text::encoding::{self, LineEnding};
use bingux_text::encoding_rs;
use bingux_text::{Indent, Language};
use bingux_ui::app::use_window_seed;
use bingux_ui::commands::{
    Command, CommandGroup, command_scope, use_commands, use_window_commands,
};
use bingux_ui::controls::TabItem;
use bingux_ui::dnd::{DragActions, DropTarget, Dropped, Format, use_drop_target};
use bingux_ui::document::{
    Document, DocumentEvent, DocumentId, Documents, DocumentsConfig, Phase, UNSAVED_MARK,
    use_documents,
};
use bingux_ui::files::Filter;
use bingux_ui::prelude::*;
use bingux_ui::tabs::{Birth, TabContent, TabSet, Tabs, use_tab_window};
use bingux_ui::text_editor::{
    EditorHandle, FindState, ZoomStep, caret_caption, editor_commands, find_bar, go_to_line_bar,
    markdown_view, status_caption, text_editor, use_find,
};
use freya::prelude::*;

use crate::launch::{self, Request};
use crate::positions;
use crate::prefs::{self, Prefs};
use bingux_ui::shell::MenuItem;

/// How often where each file was left, and the preferences, are written.
const TICK: Duration = Duration::from_secs(2);
/// Markdown documents larger than this are not previewed.
const PREVIEW_LIMIT: usize = 2 << 20;

// ---------------------------------------------------------------------------
// Start-up
// ---------------------------------------------------------------------------

thread_local! {
    static START: RefCell<Option<Prefs>> = const { RefCell::new(None) };
    /// The preferences as the window last set them, for work outside it.
    static PREFS: RefCell<Prefs> = RefCell::new(Prefs::default());
}

/// Keeps what `main` read before the window, for the first window to take.
pub fn prepare(prefs: Prefs) {
    PREFS.with(|current| *current.borrow_mut() = prefs.clone());
    START.with(|start| *start.borrow_mut() = Some(prefs));
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

/// A tab shows one document, by the id the shared manager gave it. The id
/// means the same in every window, so a tab moved to another window keeps its
/// document (see [`reconcile`]).
#[derive(Clone, PartialEq)]
pub struct DocTab {
    pub doc: DocumentId,
}

impl TabContent for DocTab {
    /// Not what the strip's Duplicate does: that is [`Actions::duplicate`],
    /// which makes the copy in the window's documents.
    fn duplicate(&self) -> Self {
        self.clone()
    }
}

/// Every tab is closed through the manager, which asks about unsaved changes
/// and closes the document: its `Closed` event closes the tab.
fn through_the_manager(_: &DocTab) -> bool {
    true
}

/// The tab that shows document `id`.
fn tab_of(tabs: Tabs<DocTab>, id: DocumentId) -> Option<u64> {
    tabs.set()
        .peek()
        .tabs()
        .iter()
        .find(|tab| tab.content.doc == id)
        .map(|tab| tab.id)
}

/// A new document nobody has typed in.
fn is_blank(document: Document) -> bool {
    document.path().is_none()
        && !document.is_unsaved()
        && document
            .content_as::<EditorContent>()
            .is_some_and(|content| content.handle.peek().len_chars() == 0)
}

/// Keeps the tabs in step with the manager's list of documents.
fn follow(actions: Actions, event: DocumentEvent) {
    let tabs = actions.tabs;
    match event {
        DocumentEvent::Opened(document) => {
            if tab_of(tabs, document.id()).is_some() {
                return;
            }
            // A blank nobody typed in gives its tab to a file, or to work
            // that came back from the last run.
            let takes_over = document.path().is_some() || document.is_unsaved();
            let blank = actions
                .current()
                .filter(|current| takes_over && is_blank(*current));
            let current_tab = tabs.set().peek().current().map(|tab| tab.id);
            match (blank, current_tab) {
                (Some(old), Some(tab)) => {
                    tabs.update_if(tab, |_| true, move |content| content.doc = document.id());
                    old.close_discarding();
                }
                _ => {
                    tabs.open(DocTab { doc: document.id() });
                }
            }
            actions.focus_editor();
        }
        DocumentEvent::Reveal(document) => {
            if let Some(tab) = tab_of(tabs, document.id()) {
                tabs.select(tab);
            }
        }
        DocumentEvent::Closed(id) => {
            let mut closing = actions.closing;
            let by_person = closing.peek().contains(&id);
            closing.write().retain(|closed| *closed != id);
            let Some(tab) = tab_of(tabs, id) else {
                return;
            };
            if !by_person && tabs.set().peek().len() == 1 {
                // The manager closed the only document (a file that could not
                // be opened): the window stays, with a new one.
                let blank = actions.documents.new_document();
                tabs.update_if(tab, |_| true, move |content| content.doc = blank.id());
            } else {
                tabs.close_now(tab);
            }
        }
    }
}

/// Finds the document of every tab this window has none for: a tab that came
/// from another window brings its document with it, and a tab that was closed
/// and brought back opens its file again. A tab that is neither is closed.
fn reconcile(actions: Actions) {
    let tabs = actions.tabs;
    let missing: Vec<(u64, DocumentId)> = tabs
        .set()
        .peek()
        .tabs()
        .iter()
        .filter(|tab| actions.documents.get(tab.content.doc).is_none())
        .map(|tab| (tab.id, tab.content.doc))
        .collect();
    for (tab, doc) in missing {
        match actions.documents.claim(doc) {
            Some(found) if found.id() == doc => {}
            Some(found) => match tab_of(tabs, found.id()) {
                // The file is open in another tab: go there.
                Some(other) => {
                    tabs.close_now(tab);
                    tabs.select(other);
                }
                None => tabs.update_if(tab, |_| true, move |content| content.doc = found.id()),
            },
            None => tabs.close_now(tab),
        }
    }
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
    overlays: Overlays,
    prefs: State<Prefs>,
    editor_id: AccessibilityId,
    documents: Documents,
    /// The documents the person is closing, to tell a close they asked for
    /// from one the manager made (a file that could not be opened).
    closing: State<Vec<DocumentId>>,
}

impl Actions {
    /// The document of the tab on show.
    fn current(&self) -> Option<Document> {
        let id = self
            .tabs
            .set()
            .peek()
            .current()
            .map(|tab| tab.content.doc)?;
        self.documents.get(id)
    }

    fn focus_editor(&self) {
        let id = self.editor_id;
        spawn(async move {
            id.request_focus();
        });
    }

    /// A new document. The manager's `Opened` event opens its tab.
    fn new_document(&self) {
        self.documents.new_document();
    }

    /// Opens `path` in a tab, or goes to the tab that has it. A name that is
    /// not there yet opens as a new document that is saved there.
    fn open_path(&self, path: PathBuf) {
        self.documents.open_or_create(launch::absolute(&path));
    }

    /// The Bingux file chooser, then each file it names opens in a tab.
    fn open_dialog(&self) {
        self.documents.open_dialog();
    }

    /// A new unsaved document with the text, language and indentation of the
    /// one in tab `tab_id`.
    fn duplicate(&self, tab_id: u64) {
        let Some(source) = self
            .tabs
            .set()
            .peek()
            .content(tab_id)
            .and_then(|tab| self.documents.get(tab.doc))
        else {
            return;
        };
        let Some(from) = source.content_as::<EditorContent>() else {
            return;
        };
        let copy = self.documents.new_document();
        if let Some(to) = copy.content_as::<EditorContent>() {
            to.copy_of(&from);
        }
        self.tabs.open(DocTab { doc: copy.id() });
        self.focus_editor();
    }

    /// Closes `document` at the person's request: the manager asks about
    /// unsaved changes, and its `Closed` event closes the tab.
    fn close(&self, document: Document) {
        let mut closing = self.closing;
        closing.write().push(document.id());
        document.close();
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

    fn show_statistics(&self, document: Document) {
        let overlays = self.overlays;
        let Some(content) = document.content_as::<EditorContent>() else {
            return;
        };
        let (handle, title) = (content.handle.clone(), document.title());
        let rope = handle.peek().rope().clone();
        let selected = handle.peek().primary_text();
        spawn(async move {
            let stats =
                bingux_ui::task::spawn_blocking(move || bingux_text::stats::count(&rope)).await;
            let selection =
                selected.map(|text| bingux_text::stats::count(&bingux_text::Rope::from_str(&text)));
            let mut lines = vec![
                tr!("text-editor-stats-words", count = group(stats.words)),
                tr!(
                    "text-editor-stats-characters",
                    count = group(stats.characters)
                ),
                tr!(
                    "text-editor-stats-characters-no-spaces",
                    count = group(stats.characters_no_spaces)
                ),
                tr!("text-editor-stats-lines", count = group(stats.lines)),
                tr!(
                    "text-editor-stats-paragraphs",
                    count = group(stats.paragraphs)
                ),
                tr!(
                    "text-editor-stats-reading-time",
                    time = minutes(stats.reading_minutes())
                ),
                tr!("text-editor-stats-size", size = size(stats.bytes)),
            ];
            if let Some(selection) = selection {
                lines.push(tr!(
                    "text-editor-stats-selected",
                    words = group(selection.words),
                    characters = group(selection.characters)
                ));
            }
            overlays.show_dialog(
                dialog(
                    tr!("text-editor-stats-title", title = title),
                    lines.join("\n"),
                )
                .cancel(tr!("text-editor-ok")),
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
        0 => tr!("text-editor-none"),
        count => tr!("text-editor-minutes", count = count),
    }
}

fn size(bytes: usize) -> String {
    match bytes {
        bytes if bytes < 1000 => tr!("text-editor-size-bytes", count = bytes),
        bytes if bytes < 1_000_000 => tr!(
            "text-editor-size-kilobytes",
            size = format!("{:.1}", bytes as f64 / 1000.0)
        ),
        bytes => tr!(
            "text-editor-size-megabytes",
            size = format!("{:.1}", bytes as f64 / 1_000_000.0)
        ),
    }
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
        let prefs = use_state(move || start.unwrap_or_else(prefs_now));
        let events = use_state(Vec::<DocumentEvent>::new);
        let documents = use_documents(
            DocumentsConfig::new("bingux-text-editor", "Text Editor")
                .mime("text/plain")
                .filter(Filter::new(tr!("text-editor-filter-text")).mime("text/*"))
                // The window needs a tab before the start has finished: it
                // makes its own first document.
                .blank_when_empty(false),
            |init| Rc::new(EditorContent::new(init)),
            move |event| {
                let mut events = events;
                events.write().push(event);
            },
        );
        let set = use_state(move || {
            birth.and_then(|birth| birth.into_set()).unwrap_or_else(|| {
                TabSet::new(DocTab {
                    doc: documents.new_document().id(),
                })
            })
        });
        let asks = use_state(Vec::<u64>::new);
        let closing = use_state(Vec::<DocumentId>::new);
        let tabs = Tabs::new(set, window, overlays).guard_tabs(through_the_manager, asks);
        let editor_id = use_a11y();
        let actions = Actions {
            tabs,
            overlays,
            prefs,
            editor_id,
            documents,
            closing,
        };
        let commands = use_commands();
        let find = use_find();
        let mut go_to_open = use_state(|| false);
        let preview_on_phone = use_state(|| false);
        // Files dragged in from other programs open in tabs.
        let files_over = use_drop_target(
            DropTarget::new()
                .accepts([Format::Uris])
                .actions(DragActions::COPY)
                .highlight(false)
                .on_drop(move |dropped: Dropped| {
                    for path in dropped.data.files() {
                        actions.open_path(path);
                    }
                }),
        );
        let preview_ratio = use_state(|| 0.5f32);
        // The document the keyboard was last given to: a document that is
        // shown, or that finishes loading, takes the keyboard once.
        let focused_doc = use_state(|| None::<DocumentId>);

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

        // The background work of the window: where each file was left, the
        // caret and scroll the manager keeps, and the preferences. One loop
        // per window.
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

        // What the manager says of the list of documents: the tabs follow.
        let pending_event = events.read().first().copied();
        if let Some(event) = pending_event {
            let mut events = events;
            spawn(async move {
                if !events.peek().is_empty() {
                    events.write().remove(0);
                }
                follow(actions, event);
            });
        }

        // A tab this window has no document for: it came from another window,
        // or it was closed and brought back.
        let reconciling = use_hook(|| Rc::new(Cell::new(false)));
        if !reconciling.get()
            && set
                .read()
                .tabs()
                .iter()
                .any(|tab| documents.get(tab.content.doc).is_none())
        {
            reconciling.set(true);
            let reconciling = reconciling.clone();
            spawn(async move {
                reconcile(actions);
                reconciling.set(false);
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
        let view = documents
            .get(doc_id)
            .and_then(|document| Some((document, document.content_as::<EditorContent>()?)));
        let title = view
            .as_ref()
            .map(|(document, _)| {
                let name = document.display_title();
                if document.is_unsaved() {
                    format!("{UNSAVED_MARK}{name}")
                } else {
                    name
                }
            })
            .unwrap_or_default();
        let tab_window = use_tab_window(set, &title);
        // The window title and the scene follow the tab on show.
        use_side_effect_with_deps(&doc_id, move |doc_id| {
            documents.set_current(documents.get(*doc_id));
        });
        let Some((document, content)) = view else {
            // The document of a tab that has just arrived: not here yet.
            use_window_commands(Vec::new());
            return rect().into_element();
        };
        let handle = content.handle.clone();
        let phase = document.phase();
        let read_only = document.is_read_only();
        let path = document.path();
        let doc_title = document.display_title();
        let modified = document.is_unsaved();
        let saving = document.is_saving();
        let format = content.format();
        let notices = content.notices();
        let binary = content.is_binary();
        let prefs_now = prefs.read().clone();
        let compact = is_compact();

        // ---- Commands --------------------------------------------------
        // Everything the window can do is a command: the main menu, the
        // palette and the shortcuts sheet are built from them, and the keys
        // run them. The editor's own come with its scope, below.
        let shown_doc = ShownDoc {
            document,
            handle: handle.clone(),
            has_path: path.is_some(),
            ready: phase == Phase::Ready && !binary,
            any_unsaved: documents.has_unsaved(),
        };
        use_window_commands(window_commands(
            actions, find, go_to_open, &shown_doc, &prefs_now,
        ));
        if phase == Phase::Ready && *focused_doc.peek() != Some(doc_id) {
            let mut focused_doc = focused_doc;
            spawn(async move {
                focused_doc.set(Some(doc_id));
                editor_id.request_focus();
            });
        }

        // ---- Header ----------------------------------------------------
        let strip_items: Vec<TabItem> = shown
            .iter()
            .map(|tab| {
                let document = documents.get(tab.content.doc);
                TabItem::new(
                    tab.id,
                    document.map(Document::display_title).unwrap_or_default(),
                )
                .icon(Icon::TextXGeneric)
                .edited(document.is_some_and(Document::is_unsaved))
            })
            .collect();
        // A phone has no room for Open and Save beside the tabs and the
        // window's buttons: they are in the main menu, and a document names
        // itself in its tab at a readable width.
        let mut header = HeaderBar::new(title.clone());
        if compact {
            header = header.busy(saving);
        } else {
            header =
                header
                    .leading(
                        icon_button(Icon::DocumentOpen, move |_| actions.open_dialog())
                            .tooltip(&tip(commands, "file.open", "Open")),
                    )
                    .trailing(
                        icon_button(Icon::DocumentSave, move |_| document.save())
                            .tooltip(&tip(commands, "file.save", "Save"))
                            .busy(saving),
                    );
        }
        let header = header
            .trailing(app_menu(Vec::new()))
            .trailing(WindowControls)
            .tab_slot(
                tabs.strip(strip_items, &tab_window, move |_: ()| {
                    actions.new_document()
                })
                .on_duplicate(move |id: u64| actions.duplicate(id)),
            );

        // ---- Notices ---------------------------------------------------
        let mut notice_banners: Vec<Element> = Vec::new();
        if read_only && !content.read_only_hidden() {
            let hide = content.clone();
            notice_banners.push(
                banner(tr!("text-editor-read-only"))
                    .kind(BannerKind::Info)
                    .action(tr!("text-editor-save-as"), move |_| document.save_as())
                    .dismissible(move |_: ()| hide.hide_read_only())
                    .into(),
            );
        }
        notice_banners.extend(
            notices
                .iter()
                .filter_map(|notice| notice_banner(&content, notice, modified)),
        );

        // ---- Bars ------------------------------------------------------
        let focus_back = move |_: ()| actions.focus_editor();
        let bar: Option<Element> = if *find.open.read() {
            Some(
                find_bar(find, handle.clone())
                    .read_only(read_only)
                    .on_close(focus_back)
                    .on_replaced(move |count: usize| {
                        if count > 1 {
                            overlays.toast(tr!("text-editor-replaced", count = count));
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
        let body: Element = match phase {
            Phase::Loading => rect()
                .tw(
                    &tokens,
                    &[w_full(), h_full(), center(), flex_col(), gap(S3)],
                )
                .child(spinner(SpinnerSize::Medium))
                .child(status_caption(
                    &tokens,
                    tr!("text-editor-opening", name = doc_title.clone()),
                ))
                .into(),
            Phase::Ready if binary => {
                let content = content.clone();
                status_page(Icon::ApplicationXGeneric, tr!("text-editor-binary-title"))
                    .description(tr!("text-editor-binary-description"))
                    .action(tr!("text-editor-open-anyway"), move |_| {
                        content.read_again(None, true)
                    })
                    .into()
            }
            Phase::Ready => {
                let marks = find.marks();
                let scope = editor_commands(&handle, false);
                let mut context =
                    commands.context_menu_of(&scope, &[&["text.comment", "text.duplicate-line"]]);
                let case = commands.context_menu_of(
                    &scope,
                    &[&["text.uppercase", "text.lowercase", "text.title-case"]],
                );
                if !case.is_empty() {
                    context.push(submenu(tr!("text-editor-change-case"), case));
                }
                let view = text_editor(handle.clone())
                    .line_numbers(prefs_now.line_numbers && !compact)
                    .wrap(prefs_now.wrap)
                    .show_whitespace(prefs_now.show_whitespace)
                    .highlight_line(prefs_now.highlight_line)
                    .zoom(prefs_now.zoom, move |step: ZoomStep| actions.zoom(step))
                    .placeholder(tr!("text-editor-drop-hint"))
                    .label(doc_title.clone())
                    .a11y_id(editor_id)
                    .menu_extra(context);
                let view = match marks {
                    Some((matches, current)) => view.matches(matches, current),
                    None => view,
                };
                // The editor's keys are its own; the scope lists them in the
                // menus, the palette and the shortcuts sheet.
                let editor: Element = command_scope(tr!("text-editor-scope-editor"), scope, view)
                    .fill()
                    .into();
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
            content.clone(),
            document,
            handle.clone(),
            format,
            compact,
            is_markdown,
            preview_on_phone,
        );

        // ---- Keys --------------------------------------------------------
        // The commands answer their own shortcuts. What is left here: the
        // tab keys that are not commands (Alt+1 to 9) and Escape, which
        // closes the find or go to line bar from the text too.
        let tab_keys = tabs.shortcuts(move |_: ()| actions.new_document());
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let mut find_open = find.open;
            if event.key == Key::Named(NamedKey::Escape)
                && (*find.open.peek() || *go_to_open.peek())
            {
                find_open.set(false);
                go_to_open.set(false);
                actions.focus_editor();
                event.stop_propagation();
                return;
            }
            tab_keys(event);
        };

        let drop_overlay = files_over.is_targeted().then(|| {
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
                        .child(typography::SECTION.label(&tokens, tr!("text-editor-drop-to-open"))),
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
            .on_sized(move |event: Event<SizedEventData>| {
                files_over.set_area(event.visible_area);
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

/// Every two seconds: tell the manager where each caret and scroll is (the
/// scene and the drafts keep it), remember where files were left, and keep
/// the preferences.
fn tick(actions: Actions) {
    for document in actions.documents.list() {
        if document.phase() != Phase::Ready {
            continue;
        }
        let Some(content) = document.content_as::<EditorContent>() else {
            continue;
        };
        if content.is_binary() {
            continue;
        }
        content.sync();
        if let Some(path) = document.path() {
            let caret = content.handle.peek().selection().primary().head;
            let line = content.handle.scroll().line;
            positions::remember(&path, positions::Position { caret, line });
        }
    }
    prefs::save(&prefs_now());
}

/// The question before a tab closes is the manager's: it asks about unsaved
/// changes, and closes the document, which closes the tab.
fn ask_to_close(actions: Actions, tab_id: u64) {
    let Some(doc) = actions.tabs.set().peek().content(tab_id).map(|tab| tab.doc) else {
        return;
    };
    match actions.documents.get(doc) {
        Some(document) => actions.close(document),
        None => actions.tabs.close_now(tab_id),
    }
}

fn open_link(url: &str, overlays: Overlays) {
    // Until bingux_ui::platform::open lands on this branch: the desktop's
    // opener, for web and mail addresses only.
    if !["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        overlays.toast(tr!("text-editor-link-not-web"));
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
fn notice_banner(content: &Rc<EditorContent>, notice: &Notice, modified: bool) -> Option<Element> {
    let dismiss = {
        let content = content.clone();
        let notice = notice.clone();
        move |_: ()| content.dismiss(&notice)
    };
    let banner_: Banner = match notice {
        Notice::Mixed(legacy) => banner(tr!(
            "text-editor-mixed-encoding",
            encoding = encoding::encoding_label(legacy)
        ))
        .kind(BannerKind::Info)
        .dismissible(dismiss),
        Notice::Lossy => banner(tr!("text-editor-lossy"))
            .kind(BannerKind::Warning)
            .dismissible(dismiss),
        Notice::MixedLineEndings => {
            if modified {
                return None;
            }
            banner(tr!("text-editor-mixed-endings"))
                .kind(BannerKind::Info)
                .dismissible(dismiss)
        }
        Notice::Binary => return None,
    };
    Some(banner_.into())
}

/// What the window's commands need to know of the document on show.
struct ShownDoc {
    document: Document,
    handle: EditorHandle,
    has_path: bool,
    ready: bool,
    any_unsaved: bool,
}

/// "Open (Ctrl+O)": `name` and the first shortcut of the command `id`, so a
/// tooltip never disagrees with the key.
fn tip(commands: bingux_ui::commands::Commands, id: &str, name: &str) -> String {
    match commands
        .get(id)
        .and_then(|command| command.shortcuts().first().map(|shortcut| shortcut.label()))
    {
        Some(keys) => format!("{name} ({keys})"),
        None => name.to_owned(),
    }
}

/// The commands that work wherever the keyboard is: files, find, view and
/// editing choices. The keys of the editor itself (Undo, the clipboard, the
/// line edits) come with its scope.
fn window_commands(
    actions: Actions,
    find: FindState,
    go_to_open: State<bool>,
    shown: &ShownDoc,
    prefs: &Prefs,
) -> Vec<Command> {
    let document = shown.document;
    let ready = shown.ready;
    let find_handle = shown.handle.clone();
    let replace_handle = shown.handle.clone();
    let next_handle = shown.handle.clone();
    let previous_handle = shown.handle.clone();
    let mut find_open = find.open;
    let mut go_to_open = go_to_open;
    let mut go_to_close = go_to_open;
    let mut next_open = find.open;
    let mut previous_open = find.open;
    let auto_close = prefs.auto_close;

    let toggle =
        |id: &'static str, title: String, on: bool, group: CommandGroup, change: fn(&mut Prefs)| {
            Command::new(id, title, move |_| actions.update_prefs(change))
                .checked(on)
                .group(group)
        };

    let mut list = vec![
        Command::new("file.new", tr!("text-editor-cmd-new"), move |_| {
            actions.new_document()
        })
        .shortcut("Ctrl+N")
        .shortcut("Ctrl+T")
        .group(CommandGroup::File)
        .section("new"),
        Command::new("file.open", tr!("text-editor-cmd-open"), move |_| {
            actions.open_dialog()
        })
        .shortcut("Ctrl+O")
        .icon(Icon::DocumentOpen)
        .group(CommandGroup::File)
        .section("new"),
        Command::submenu(
            "file.open-recent",
            tr!("text-editor-cmd-open-recent"),
            actions.documents.recent_menu(),
        )
        .group(CommandGroup::File)
        .section("new"),
        Command::new("file.save", tr!("text-editor-cmd-save"), move |_| {
            document.save()
        })
        .shortcut("Ctrl+S")
        .icon(Icon::DocumentSave)
        .enabled(ready)
        .group(CommandGroup::File)
        .section("save"),
        Command::new("file.save-as", tr!("text-editor-cmd-save-as"), move |_| {
            document.save_as()
        })
        .shortcut("Ctrl+Shift+S")
        .enabled(ready)
        .group(CommandGroup::File)
        .section("save"),
        Command::new(
            "file.save-all",
            tr!("text-editor-cmd-save-all"),
            move |_| actions.documents.save_all(),
        )
        .enabled(shown.any_unsaved)
        .group(CommandGroup::File)
        .section("save"),
        Command::new("file.reload", tr!("text-editor-cmd-reload"), move |_| {
            document.reload()
        })
        .keywords(["revert", "reload"])
        .enabled(ready && shown.has_path)
        .group(CommandGroup::File)
        .section("save"),
        Command::new("find.find", tr!("text-editor-cmd-find"), move |_| {
            go_to_close.set(false);
            find.open_from(&find_handle, false);
        })
        .shortcut("Ctrl+F")
        .icon(Icon::Search)
        .enabled(ready)
        .group(CommandGroup::Edit)
        .section("find"),
        Command::new("find.replace", tr!("text-editor-cmd-replace"), move |_| {
            go_to_close.set(false);
            find.open_from(&replace_handle, true);
        })
        .shortcut("Ctrl+H")
        .keywords(["substitute"])
        .enabled(ready)
        .group(CommandGroup::Edit)
        .section("find"),
        Command::new("find.next", tr!("text-editor-cmd-find-next"), move |_| {
            if !*find.open.peek() && !find.query.peek().is_empty() {
                next_open.set(true);
            }
            find.step(&next_handle, true);
        })
        .shortcut("Ctrl+G")
        .shortcut("F3")
        .enabled(ready)
        .group(CommandGroup::Edit)
        .section("find"),
        Command::new(
            "find.previous",
            tr!("text-editor-cmd-find-previous"),
            move |_| {
                if !*find.open.peek() && !find.query.peek().is_empty() {
                    previous_open.set(true);
                }
                find.step(&previous_handle, false);
            },
        )
        .shortcut("Ctrl+Shift+G")
        .shortcut("Shift+F3")
        .enabled(ready)
        .group(CommandGroup::Edit)
        .section("find"),
        Command::new("go.line", tr!("text-editor-cmd-go-line"), move |_| {
            find_open.set(false);
            go_to_open.set(true);
        })
        .shortcut("Ctrl+I")
        .keywords(["jump", "line number"])
        .enabled(ready)
        .group(CommandGroup::Go)
        .section("lines"),
        toggle(
            "view.line-numbers",
            tr!("text-editor-cmd-line-numbers"),
            prefs.line_numbers,
            CommandGroup::View,
            |prefs| prefs.line_numbers = !prefs.line_numbers,
        )
        .section("show"),
        toggle(
            "view.wrap",
            tr!("text-editor-cmd-wrap"),
            prefs.wrap,
            CommandGroup::View,
            |prefs| prefs.wrap = !prefs.wrap,
        )
        .shortcut("Alt+Z")
        .keywords(["word wrap"])
        .section("show"),
        toggle(
            "view.whitespace",
            tr!("text-editor-cmd-whitespace"),
            prefs.show_whitespace,
            CommandGroup::View,
            |prefs| prefs.show_whitespace = !prefs.show_whitespace,
        )
        .keywords(["invisibles", "whitespace"])
        .section("show"),
        toggle(
            "view.highlight-line",
            tr!("text-editor-cmd-highlight-line"),
            prefs.highlight_line,
            CommandGroup::View,
            |prefs| prefs.highlight_line = !prefs.highlight_line,
        )
        .section("show"),
        toggle(
            "view.markdown-preview",
            tr!("text-editor-cmd-markdown-preview"),
            prefs.markdown_preview,
            CommandGroup::View,
            |prefs| prefs.markdown_preview = !prefs.markdown_preview,
        )
        .shortcut("Ctrl+Shift+V")
        .section("show"),
        Command::new("view.zoom-in", tr!("text-editor-cmd-zoom-in"), move |_| {
            actions.zoom(ZoomStep::In)
        })
        .shortcut("Ctrl+Plus")
        .shortcut("Ctrl+=")
        .group(CommandGroup::View)
        .section("zoom"),
        Command::new(
            "view.zoom-out",
            tr!("text-editor-cmd-zoom-out"),
            move |_| actions.zoom(ZoomStep::Out),
        )
        .shortcut("Ctrl+-")
        .group(CommandGroup::View)
        .section("zoom"),
        Command::new(
            "view.zoom-reset",
            tr!("text-editor-cmd-actual-size"),
            move |_| actions.zoom(ZoomStep::Reset),
        )
        .shortcut("Ctrl+0")
        .group(CommandGroup::View)
        .section("zoom"),
        Command::new(
            "text.statistics",
            tr!("text-editor-cmd-statistics"),
            move |_| actions.show_statistics(document),
        )
        .keywords(["word count", "characters", "reading time"])
        .enabled(ready)
        .group(bingux_ui::text_editor::TEXT_GROUP)
        .section("info"),
        Command::new(
            "editing.auto-close",
            tr!("text-editor-cmd-auto-close"),
            move |_| {
                actions.update_prefs(|prefs| prefs.auto_close = !auto_close);
                for document in actions.documents.list() {
                    if let Some(content) = document.content_as::<EditorContent>() {
                        content
                            .handle
                            .update(|editor| editor.settings.auto_close = !auto_close);
                    }
                }
            },
        )
        .checked(auto_close)
        .group(CommandGroup::Other(bingux_ui::prelude::i18n_id!(
            "text-editor-group-editing"
        )))
        .section("typing"),
        toggle(
            "editing.trim-on-save",
            tr!("text-editor-cmd-trim-on-save"),
            prefs.trim_on_save,
            CommandGroup::Other(bingux_ui::prelude::i18n_id!("text-editor-group-editing")),
            |prefs| prefs.trim_on_save = !prefs.trim_on_save,
        )
        .section("saving"),
        toggle(
            "editing.final-newline",
            tr!("text-editor-cmd-final-newline"),
            prefs.final_newline,
            CommandGroup::Other(bingux_ui::prelude::i18n_id!("text-editor-group-editing")),
            |prefs| prefs.final_newline = !prefs.final_newline,
        )
        .section("saving"),
    ];
    // New tab is New document here, with both keys; the rest of the tab
    // commands (Close, Reopen, Next, Previous) are the tab set's own.
    list.extend(
        actions
            .tabs
            .commands(move |_: ()| actions.new_document())
            .into_iter()
            .filter(|command| command.id() != "tabs.new"),
    );
    list
}

/// The status bar: where the caret is, and the document's language,
/// indentation, encoding and line endings, each a menu.
fn status_bar(
    content: Rc<EditorContent>,
    document: Document,
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
    let encoding_menu = encoding_items(&content, document, format);
    let ending_menu: Vec<MenuItem> = LineEnding::ALL
        .iter()
        .map(|ending| {
            let ending = *ending;
            let content = content.clone();
            menu_item(ending.describe(), move |_| {
                content.change_format(|format| format.line_ending = ending)
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
                menu_item(
                    if on {
                        tr!("text-editor-show-text")
                    } else {
                        tr!("text-editor-show-preview")
                    },
                    move |_| preview_on_phone.set(!on),
                ),
            );
        }
        return row
            .child(menu_button(tr!("text-editor-document-menu"), items).icon(Icon::ViewMore))
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
        tr!("text-editor-other-languages"),
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
    items.push(submenu(tr!("text-editor-tab-width"), widths));
    let convert = handle.clone();
    items.push(menu_item(tr!("text-editor-detect-indent"), move |_| {
        convert.update(|editor| {
            if let Some(found) = bingux_text::indent::detect(editor.rope()) {
                editor.settings.indent = found;
            }
        });
    }));
    items
}

fn encoding_items(
    content: &Rc<EditorContent>,
    document: Document,
    format: encoding::FileFormat,
) -> Vec<MenuItem> {
    let has_path = document.path().is_some();
    let groups = encoding::encodings();
    let save_with: Vec<MenuItem> = groups
        .iter()
        .map(|(group, list)| {
            submenu(
                *group,
                list.iter()
                    .map(|encoding| {
                        let encoding = *encoding;
                        let content = content.clone();
                        menu_item(encoding::encoding_label(encoding), move |_| {
                            content.change_format(|format| format.encoding = encoding)
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
                        let content = content.clone();
                        menu_item(encoding::encoding_label(encoding), move |_| {
                            content.read_again(Some(encoding), false)
                        })
                    })
                    .collect(),
            )
        })
        .collect();
    // Labels short enough for the narrow menu (`Dim::Menu`), which cuts longer ones off.
    let mut items = vec![submenu(tr!("text-editor-encode-as"), save_with)];
    let reopen_item = submenu(tr!("text-editor-read-again-as"), reopen);
    items.push(if has_path {
        reopen_item
    } else {
        reopen_item.disabled()
    });
    if format.encoding == encoding_rs::UTF_8 {
        let bom = format.bom;
        let content = content.clone();
        items.push(
            menu_item(tr!("text-editor-bom"), move |_| {
                content.change_format(|format| format.bom = !bom)
            })
            .checked(bom),
        );
    }
    items
}
