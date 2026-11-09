//! The document manager in the headless runner: what a person sees and what
//! lands on disk when they open, edit, save and close documents, when another
//! program changes a file under them, and when the app starts after a crash.
//!
//! Failure modes, before the tests (the repository's rule):
//!
//! - a save that races an edit must not mark the newer content saved;
//! - closing with unsaved changes must ask, and Cancel must leave everything
//!   as it was, including the draft;
//! - discarding must remove the draft (or it is offered again after a clean
//!   exit), and saving must too;
//! - another program changing a file under an unedited document must reload it
//!   silently, and under an edited one must never lose the edit without
//!   asking;
//! - a deleted file must leave its document open, unsaved and savable;
//! - a file that cannot be opened must say so and leave no document;
//! - a crash's drafts must be offered, recovered with their unsaved mark, and
//!   removed when discarded; a draft of a file that changed since must say so;
//! - the same file opened twice is one document;
//! - the window's close guard must ask, and a refused close must be askable
//!   again.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;

use super::*;
use crate::document::scene::for_tests::{current_documents_for_test, preload_for_test};
use crate::document::scene::{SceneData, SceneDocument};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (900.0, 600.0);
/// The longest a test waits for something to appear.
const CAP: Duration = Duration::from_secs(10);

/// The app's side of a document: its text.
struct Text {
    text: Rc<RefCell<String>>,
    loads: Rc<RefCell<Vec<LoadReason>>>,
    /// A load that marks the document edited, as a careless app's would.
    marks_edited_when_loaded: bool,
    document: RefCell<Option<Document>>,
    /// What the manager told the content: `will_save`, `saved`, `prepared`.
    calls: Rc<RefCell<Vec<String>>>,
    /// Shares the decoding with a worker (upper-cases the text there).
    prepares: bool,
    /// Adds `!` on the way to disk, as format on save does.
    appends_on_save: bool,
    /// Offers a file name for Save as.
    suggests: bool,
    /// The content cannot be turned into bytes until the fallback has run.
    broken: Rc<std::cell::Cell<bool>>,
    /// Offers "Save in UTF-8" when it is broken.
    offers: bool,
}

impl DocumentContent for Text {
    fn suggested_name(&self) -> Option<String> {
        self.suggests.then(|| "notes.txt".to_string())
    }

    fn snapshot(&self) -> DocumentSnapshot {
        if self.broken.get() {
            return DocumentSnapshot::encode(|| Err("It has a character this cannot hold.".into()));
        }
        DocumentSnapshot::bytes(self.text.borrow().clone().into_bytes())
    }

    fn save_fallback(&self) -> Option<SaveFallback> {
        let broken = self.broken.clone();
        self.offers.then(|| SaveFallback {
            label: "Save in UTF-8".into(),
            apply: Rc::new(move || broken.set(false)),
        })
    }

    fn load(&self, bytes: Vec<u8>, reason: LoadReason) {
        *self.text.borrow_mut() = String::from_utf8_lossy(&bytes).into_owned();
        self.loads.borrow_mut().push(reason);
        if self.marks_edited_when_loaded
            && let Some(document) = *self.document.borrow()
        {
            document.mark_edited();
        }
    }

    fn attached(&self, document: Document) {
        *self.document.borrow_mut() = Some(document);
    }

    fn will_save(&self) {
        self.calls.borrow_mut().push("will_save".into());
        if self.appends_on_save {
            self.text.borrow_mut().push('!');
            if let Some(document) = *self.document.borrow() {
                document.mark_edited();
            }
        }
    }

    fn saved(&self, path: &Path, clean: bool) {
        let name = path.file_name().unwrap().to_string_lossy();
        self.calls.borrow_mut().push(format!(
            "saved {name} {}",
            if clean { "clean" } else { "dirty" }
        ));
    }

    fn prepare(&self, reason: LoadReason) -> Option<Preparer> {
        self.prepares.then(|| {
            self.calls.borrow_mut().push(format!("prepare {reason:?}"));
            Box::new(|bytes: Vec<u8>| Prepared::new(String::from_utf8_lossy(&bytes).to_uppercase()))
                as Preparer
        })
    }

    fn load_prepared(&self, prepared: Prepared, reason: LoadReason) {
        *self.text.borrow_mut() = prepared.take::<String>().expect("the string prepared");
        self.loads.borrow_mut().push(reason);
    }
}

/// A content of another type, for `content_as`.
struct Other;

impl DocumentContent for Other {
    fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::bytes(Vec::new())
    }

    fn load(&self, _bytes: Vec<u8>, _reason: LoadReason) {}
}

#[derive(Default)]
struct World {
    docs: Option<Documents>,
    texts: HashMap<u64, Rc<RefCell<String>>>,
    loads: HashMap<u64, Rc<RefCell<Vec<LoadReason>>>>,
    inits: Vec<DocumentInit>,
    events: Vec<String>,
    careless: bool,
    calls: HashMap<u64, Rc<RefCell<Vec<String>>>>,
    prepares: bool,
    appends_on_save: bool,
    suggests: bool,
    broken: bool,
    offers: bool,
    /// A second window's manager is made too: `other`.
    second: bool,
    other: Option<Documents>,
}

thread_local! {
    static WORLD: RefCell<World> = RefCell::new(World::default());
}

fn world<R>(use_it: impl FnOnce(&mut World) -> R) -> R {
    WORLD.with(|world| use_it(&mut world.borrow_mut()))
}

#[derive(PartialEq)]
struct Harness {
    sandbox: PathBuf,
}

impl Component for Harness {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        Element::from(OverlayHost::new(Root {
            sandbox: self.sandbox.clone(),
        }))
    }
}

#[derive(PartialEq)]
struct Root {
    sandbox: PathBuf,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let config = DocumentsConfig::new("test-app", "Test App")
            .mime("text/plain")
            .autosave_after(Duration::from_millis(150))
            .sandboxed(&self.sandbox);
        let documents = use_documents(
            config,
            |init| {
                let text = Rc::new(RefCell::new(String::new()));
                let loads = Rc::new(RefCell::new(Vec::new()));
                let calls = Rc::new(RefCell::new(Vec::new()));
                world(|world| {
                    world.texts.insert(init.id.get(), text.clone());
                    world.loads.insert(init.id.get(), loads.clone());
                    world.calls.insert(init.id.get(), calls.clone());
                    world.inits.push(init.clone());
                });
                Rc::new(Text {
                    text,
                    loads,
                    marks_edited_when_loaded: world(|world| world.careless),
                    document: RefCell::new(None),
                    calls,
                    prepares: world(|world| world.prepares),
                    appends_on_save: world(|world| world.appends_on_save),
                    suggests: world(|world| world.suggests),
                    broken: Rc::new(std::cell::Cell::new(world(|world| world.broken))),
                    offers: world(|world| world.offers),
                })
            },
            |event| {
                world(|world| {
                    world.events.push(match event {
                        DocumentEvent::Opened(document) => {
                            format!("opened {}", document.id().get())
                        }
                        DocumentEvent::Reveal(document) => {
                            format!("reveal {}", document.id().get())
                        }
                        DocumentEvent::Closed(id) => format!("closed {}", id.get()),
                    });
                });
            },
        );
        world(|world| world.docs = Some(documents));
        if world(|world| world.second) {
            let other = use_documents(
                DocumentsConfig::new("test-app-two", "Test App")
                    .mime("text/plain")
                    .sandboxed(&self.sandbox)
                    .restore(false)
                    .blank_when_empty(false),
                |_| {
                    Rc::new(Text {
                        text: Rc::default(),
                        loads: Rc::default(),
                        marks_edited_when_loaded: false,
                        document: RefCell::new(None),
                        calls: Rc::default(),
                        prepares: false,
                        appends_on_save: false,
                        suggests: false,
                        broken: Rc::default(),
                        offers: false,
                    })
                },
                |_| {},
            );
            world(|world| world.other = Some(other));
        }
        let mut column = rect().vertical();
        for document in documents.list() {
            column = column.child(label().text(document.display_title()));
        }
        column
    }
}

struct Session {
    shot: Snapshot,
    dir: PathBuf,
}

impl Session {
    fn new(name: &str) -> Session {
        Session::with(name, |_| {})
    }

    /// A session whose sandbox `before` prepares: drafts a crash left, and
    /// files to open.
    fn with(name: &str, before: impl FnOnce(&Path)) -> Session {
        let dir =
            std::env::temp_dir().join(format!("bingux-documents-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sandbox")).unwrap();
        before(&dir);
        let sandbox = dir.join("sandbox");
        let mut shot = Snapshot::new(
            move || {
                Harness {
                    sandbox: sandbox.clone(),
                }
                .into_element()
            },
            SIZE,
        );
        shot.settle(Duration::from_millis(100));
        Session { shot, dir }
    }

    fn docs(&self) -> Documents {
        world(|world| world.docs).expect("the root has rendered")
    }

    /// Runs `act` inside the app, as an event handler does.
    fn run<R>(&mut self, act: impl FnOnce(Documents) -> R) -> R {
        let docs = self.docs();
        self.shot.runner().run_in(|| act(docs))
    }

    fn texts(&mut self) -> Vec<String> {
        let mut all: Vec<String> = self.shot.runner().find_many(|_, element| {
            Label::try_downcast(element).map(|label| label.text.to_string())
        });
        all.extend(self.shot.runner().find_many(|_, element| {
            Paragraph::try_downcast(element).map(|paragraph| {
                paragraph
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect::<String>()
            })
        }));
        all
    }

    fn shows(&mut self, text: &str) -> bool {
        self.texts().iter().any(|shown| shown.contains(text))
    }

    /// Waits for `text` to be on screen.
    fn wait_for(&mut self, text: &str) {
        let found = self.shot.settle_until(CAP, |shot| {
            let mut all: Vec<String> = shot.runner().find_many(|_, element| {
                Label::try_downcast(element).map(|label| label.text.to_string())
            });
            all.extend(shot.runner().find_many(|_, element| {
                Paragraph::try_downcast(element).map(|paragraph| {
                    paragraph
                        .spans
                        .iter()
                        .map(|span| span.text.to_string())
                        .collect::<String>()
                })
            }));
            all.iter().any(|shown| shown.contains(text))
        });
        assert!(found, "never saw {text:?}; on screen: {:?}", self.texts());
    }

    fn wait_until(&mut self, what: &str, mut done: impl FnMut(&Probe) -> bool) {
        let Session { shot, dir } = self;
        let probe = Probe { dir };
        let found = shot.settle_until(CAP, |_| done(&probe));
        assert!(found, "never: {what}");
    }

    /// Waits until the drafts folder holds `count` files (two for a draft: its
    /// content and its record).
    fn wait_drafts(&mut self, count: usize) {
        self.wait_until(&format!("{count} draft files"), |probe| {
            probe.drafts().len() == count
        });
    }

    fn click(&mut self, text: &str) {
        assert!(
            self.shot.click_text(text),
            "no {text:?} to click; on screen: {:?}",
            self.texts()
        );
        self.shot.settle(Duration::from_millis(80));
    }

    fn set_text(&mut self, document: Document, text: &str) {
        world(|world| *world.texts[&document.id().get()].borrow_mut() = text.to_string());
        self.run(|_| document.mark_edited());
    }

    fn open(&mut self, path: &Path) -> Document {
        let document = self.run(|docs| docs.open_path(path));
        let wanted = document;
        self.wait_until("the file is read", |_| {
            wanted.phase() == Phase::Ready || !wanted.is_open()
        });
        document
    }

    fn drafts(&self) -> Vec<String> {
        Probe { dir: &self.dir }.drafts()
    }

    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

/// What a wait may look at: the session's folder, not its window.
struct Probe<'a> {
    dir: &'a Path,
}

impl Probe<'_> {
    fn drafts(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir.join("sandbox/drafts"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn text_of(document: Document) -> String {
    world(|world| world.texts[&document.id().get()].borrow().clone())
}

fn titles() -> Vec<String> {
    world(|world| world.docs)
        .expect("the root has rendered")
        .list()
        .into_iter()
        .map(Document::title)
        .collect()
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn reset() {
    world(|world| *world = World::default());
    preload_for_test(SceneData::default());
}

/// A process id that is gone, for the owner of a draft a crash left.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("`true` starts");
    let pid = child.id();
    child.wait().unwrap();
    pid
}

/// Writes a draft as a crashed copy of the app would have left it.
fn crashed_draft(dir: &Path, id: &str, meta: &DraftMeta, content: &str) {
    Drafts::in_dir(dir.join("sandbox/drafts"))
        .write(
            id,
            meta,
            content.as_bytes(),
            crate::format::Moment::now(),
            dead_pid(),
        )
        .unwrap();
}

// ---- Opening, editing, saving ---------------------------------------------

#[test]
fn a_new_document_is_untitled_and_the_next_ones_are_numbered() {
    reset();
    let mut session = Session::new("untitled");
    session.shot.settle(Duration::from_millis(300));
    // The app starts with one blank document.
    assert_eq!(titles(), ["Untitled"]);
    let (second, third) = session.run(|docs| (docs.new_document(), docs.new_document()));
    assert_eq!(
        (second.title(), third.title()),
        ("Untitled 2".into(), "Untitled 3".into())
    );
    // A number that is free again is the next one used.
    session.run(|_| second.close());
    let again = session.run(|docs| docs.new_document());
    assert_eq!(again.title(), "Untitled 2");
    assert!(
        second.title().is_empty() && !second.is_open(),
        "a closed handle reads as nothing"
    );
}

#[test]
fn opening_a_file_reads_it_off_the_ui_thread_and_does_not_mark_it_edited() {
    reset();
    let mut session = Session::new("open");
    let path = session.file("a.txt", "hello");
    let document = session.run(|docs| docs.open_path(&path));
    assert_eq!(
        document.phase(),
        Phase::Loading,
        "the file is read on a worker"
    );
    session.wait_until("the file is read", |_| document.phase() == Phase::Ready);
    assert_eq!(text_of(document), "hello");
    assert_eq!(document.title(), "a.txt");
    assert!(!document.is_unsaved() && !document.is_untitled());
    assert_eq!(document.display_title(), "a.txt");
    let loads = world(|world| world.loads[&document.id().get()].borrow().clone());
    assert_eq!(loads, [LoadReason::Open]);
    assert!(world(|world| world
        .events
        .contains(&format!("opened {}", document.id().get()))));
}

#[test]
fn an_app_that_marks_the_document_edited_while_loading_does_not_make_it_unsaved() {
    reset();
    world(|world| world.careless = true);
    let mut session = Session::new("careless");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    session.shot.settle(Duration::from_millis(300));
    assert!(!document.is_unsaved(), "a load is not an edit");
    assert!(session.drafts().is_empty(), "and so writes no draft");
}

#[test]
fn the_same_file_opened_twice_is_one_document_that_is_shown() {
    reset();
    let mut session = Session::new("twice");
    let path = session.file("a.txt", "x");
    let first = session.open(&path);
    let second = session.run(|docs| docs.open_path(&path));
    assert_eq!(first.id(), second.id());
    assert_eq!(titles().iter().filter(|title| *title == "a.txt").count(), 1);
    assert!(world(|world| world
        .events
        .contains(&format!("reveal {}", first.id().get()))));
}

#[test]
fn a_file_that_cannot_be_opened_is_reported_and_leaves_no_document() {
    reset();
    let mut session = Session::new("missing");
    session.shot.settle(Duration::from_millis(300));
    let missing = session.dir.join("nope.txt");
    session.run(|docs| docs.open_path(&missing));
    session.wait_for("Could not open “nope.txt”");
    assert!(session.shows("It is no longer there"));
    assert!(!titles().contains(&"nope.txt".to_string()));
    session.click("Close");
    // A folder is not a file either.
    let folder = session.dir.clone();
    session.run(|docs| docs.open_path(&folder));
    session.wait_for("It is a folder, not a file");
}

#[test]
fn a_name_that_is_not_there_yet_opens_as_a_new_document_that_saves_there() {
    reset();
    let mut session = Session::new("create");
    let path = session.dir.join("fresh.txt");
    let document = session.run(|docs| docs.open_or_create(&path));
    session.wait_until("ready", |_| document.phase() == Phase::Ready);
    assert_eq!(document.title(), "fresh.txt");
    assert!(!document.is_unsaved());
    session.set_text(document, "made here");
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "made here");
}

#[test]
fn save_then_runs_its_callback_once_the_save_has_worked() {
    reset();
    let mut session = Session::new("save-then");
    let path = session.dir.join("then.txt");
    let document = session.run(|docs| docs.open_or_create(&path));
    session.wait_until("ready", |_| document.phase() == Phase::Ready);
    session.set_text(document, "then");
    let ran = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = ran.clone();
    session.run(|_| document.save_then(move || flag.set(true)));
    session.wait_until("saved", |_| !document.is_unsaved());
    assert!(ran.get(), "the callback runs after the save");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "then");
}

#[test]
fn the_content_hears_of_a_save_before_and_after_it_and_a_draft_is_not_a_save() {
    reset();
    let mut session = Session::new("hooks");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    let calls = world(|world| world.calls[&document.id().get()].clone());
    session.set_text(document, "hello world");
    session.wait_drafts(2);
    assert!(calls.borrow().is_empty(), "a draft is not a save");
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(*calls.borrow(), ["will_save", "saved a.txt clean"]);
}

#[test]
fn what_the_content_does_to_itself_before_a_save_is_in_the_file_and_leaves_the_document_clean() {
    reset();
    world(|world| world.appends_on_save = true);
    let mut session = Session::new("will-save");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    session.set_text(document, "hello world");
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved() && !document.is_saving());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world!");
    assert_eq!(text_of(document), "hello world!");
}

#[test]
fn a_content_that_prepares_has_its_decoding_done_on_the_worker_for_open_and_reload() {
    reset();
    world(|world| world.prepares = true);
    let mut session = Session::new("prepare");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    assert_eq!(
        text_of(document),
        "HELLO",
        "the worker's work is what is shown"
    );
    assert!(!document.is_unsaved());
    session.shot.settle(Duration::from_millis(400));
    std::fs::write(&path, "again").unwrap();
    session.wait_until("reloaded", |_| text_of(document) == "AGAIN");
    let loads = world(|world| world.loads[&document.id().get()].borrow().clone());
    assert_eq!(loads, [LoadReason::Open, LoadReason::Reload]);
    let calls = world(|world| world.calls[&document.id().get()].borrow().clone());
    assert_eq!(calls, ["prepare Open", "prepare Reload"]);
}

#[test]
fn recovered_work_is_prepared_on_the_worker_too() {
    reset();
    world(|world| world.prepares = true);
    let mut session = Session::with("prepare-recover", |dir| {
        let meta = DraftMeta {
            title: "Untitled".into(),
            path: None,
            base: None,
            extra: serde_json::json!({"app": null, "untitled": 1, "scroll": [0.0, 0.0]}),
        };
        crashed_draft(dir, "69f-3-0", &meta, "unsaved thoughts");
    });
    session.wait_for("Recover “Untitled”?");
    session.click("Recover");
    session.wait_until("the text is back", |_| {
        world(|world| {
            world
                .texts
                .values()
                .any(|text| *text.borrow() == "UNSAVED THOUGHTS")
        })
    });
}

#[test]
fn the_content_of_a_document_is_found_by_its_type() {
    reset();
    let mut session = Session::new("content-as");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    assert!(document.content_as::<Text>().is_some());
    assert!(document.content_as::<Other>().is_none());
}

#[test]
fn save_as_for_a_document_with_no_file_offers_the_name_its_content_suggests() {
    reset();
    world(|world| world.suggests = true);
    let mut session = Session::new("suggested-name");
    let document = session.run(|docs| docs.new_document());
    session.set_text(document, "some words");
    session.run(|_| document.save());
    session.wait_for("Save as");
    assert!(
        session.shows("notes.txt"),
        "on screen: {:?}",
        session.texts()
    );
}

#[test]
fn a_blank_nobody_typed_in_does_not_keep_its_number_from_recovered_work() {
    reset();
    let mut session = Session::with("blank-yields", |dir| {
        let meta = DraftMeta {
            title: "Untitled".into(),
            path: None,
            base: None,
            extra: serde_json::json!({"app": null, "untitled": 1, "scroll": [0.0, 0.0]}),
        };
        crashed_draft(dir, "69f-2-0", &meta, "unsaved thoughts");
    });
    session.wait_for("Recover “Untitled”?");
    // The app made its blank document while the question was on screen.
    let blank = session.run(|docs| docs.new_document());
    assert_eq!(blank.title(), "Untitled");
    session.click("Recover");
    session.wait_until("the work is back", |_| titles().len() == 2);
    assert_eq!(titles(), ["Untitled", "Untitled"]);
}

// ---- Between windows ---------------------------------------------------------

#[test]
fn a_document_moves_to_the_window_that_claims_it_with_its_text_and_unsaved_changes() {
    reset();
    world(|world| world.second = true);
    let mut session = Session::new("claim");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    session.set_text(document, "hello there");
    let other = world(|world| world.other).expect("a second window");
    let moved = session
        .run(|_| other.claim(document.id()))
        .expect("the document is claimed");
    assert_eq!(
        moved.id(),
        document.id(),
        "the id is the same in every window"
    );
    assert!(
        !document.is_open(),
        "it left the window it was in, and nobody was asked"
    );
    assert!(moved.is_unsaved());
    assert_eq!(moved.path().as_deref(), Some(path.as_path()));
    assert_eq!(text_of(moved), "hello there");
    // Saving in the new window writes what the content holds.
    session.run(|_| moved.save());
    session.wait_until("saved", |_| !moved.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello there");
    // A document that nobody has is nobody's to claim.
    assert!(session.run(|_| other.claim(DocumentId(u64::MAX))).is_none());
}

#[test]
fn a_closed_documents_file_is_opened_again_by_claim_once_and_a_failed_open_is_not_remembered() {
    reset();
    let mut session = Session::new("reopen");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    let id = document.id();
    session.run(|_| document.close());
    assert!(!document.is_open());
    let again = session
        .run(|docs| docs.claim(id))
        .expect("the file is opened again");
    assert_ne!(again.id(), id, "it is a new document");
    session.wait_until("read", |_| again.phase() == Phase::Ready);
    assert_eq!(text_of(again), "hello");
    assert!(
        session.run(|docs| docs.claim(id)).is_none(),
        "once: the document is open again"
    );
    let missing = session.dir.join("none.txt");
    let failed = session.run(|docs| docs.open_path(&missing));
    session.wait_until("gone", |_| !failed.is_open());
    assert!(session.run(|docs| docs.claim(failed.id())).is_none());
}

#[test]
fn editing_marks_the_title_and_saving_writes_the_file_clears_the_mark_and_removes_the_draft() {
    reset();
    let mut session = Session::new("save");
    let path = session.file("a.txt", "hello");
    let document = session.open(&path);
    session.set_text(document, "hello world");
    assert!(document.is_unsaved());
    assert_eq!(document.display_title(), "a.txt");
    assert_eq!(
        document.window_title("Test App"),
        "\u{2022} a.txt \u{2013} Test App"
    );
    session.wait_for("a.txt");
    // A few moments after the edit the draft is on disk.
    session.wait_drafts(2);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "hello",
        "the file is untouched"
    );
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    assert_eq!(document.display_title(), "a.txt");
    session.wait_drafts(0);
}

#[test]
fn an_edit_made_while_the_file_is_being_written_keeps_the_document_unsaved() {
    reset();
    let mut session = Session::new("race");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    // Both in one turn of the UI: the save has taken its bytes, and the edit
    // comes before the write's result does.
    session.run(|_| {
        document.save();
        world(|world| *world.texts[&document.id().get()].borrow_mut() = "three".to_string());
        document.mark_edited();
    });
    session.wait_until("the save finished", |_| !document.is_saving());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "two",
        "the file has what the save saw"
    );
    assert!(document.is_unsaved(), "the later edit is not in the file");
    // It saves again, and then it is whole.
    session.run(|_| document.save());
    session.wait_until("saved again", |_| !document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "three");
}

#[test]
fn a_save_that_fails_says_why_and_leaves_the_file_and_the_unsaved_mark() {
    reset();
    let mut session = Session::new("fail");
    let folder = session.dir.join("gone");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("a.txt");
    std::fs::write(&path, "one").unwrap();
    let document = session.open(&path);
    session.set_text(document, "two");
    // The folder goes away under the document: the save cannot be made.
    std::fs::remove_dir_all(&folder).unwrap();
    session.run(|_| document.save());
    session.wait_for("Could not save “a.txt”");
    assert!(session.shows("Its folder is no longer there"));
    assert!(document.is_unsaved(), "nothing was saved");
    session.click("Cancel");
    assert!(document.is_unsaved());
}

// ---- Closing ---------------------------------------------------------------

#[test]
fn closing_a_document_with_unsaved_changes_asks_and_cancel_changes_nothing() {
    reset();
    let mut session = Session::new("cancel");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    session.wait_drafts(2);
    session.run(|_| document.close());
    session.wait_for("Save changes to “a.txt”?");
    session.click("Cancel");
    assert!(document.is_open() && document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
    assert_eq!(session.drafts().len(), 2, "the draft is still there");
}

#[test]
fn discarding_closes_the_document_and_removes_its_draft() {
    reset();
    let mut session = Session::new("discard");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    session.wait_drafts(2);
    session.run(|_| document.close());
    session.wait_for("Save changes to “a.txt”?");
    session.click("Discard");
    session.wait_until("closed", |_| !document.is_open());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one",
        "the file is as it was"
    );
    session.wait_drafts(0);
    assert!(world(|world| world
        .events
        .contains(&format!("closed {}", document.id().get()))));
}

#[test]
fn saving_in_the_dialog_saves_and_then_closes() {
    reset();
    let mut session = Session::new("save-close");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    session.run(|_| document.close());
    session.wait_for("Save changes to “a.txt”?");
    session.click("Save");
    session.wait_until("closed", |_| !document.is_open());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
    session.wait_drafts(0);
}

#[test]
fn a_document_without_changes_closes_without_asking() {
    reset();
    let mut session = Session::new("clean-close");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.run(|_| document.close());
    assert!(!document.is_open());
    assert!(!session.shows("Save changes"));
}

#[test]
fn the_window_guard_asks_about_every_unsaved_document_and_a_refused_close_can_be_asked_again() {
    reset();
    let mut session = Session::new("guard");
    let (a, b) = (session.file("a.txt", "1"), session.file("b.txt", "2"));
    let (first, second) = (session.open(&a), session.open(&b));
    session.set_text(first, "11");
    session.set_text(second, "22");
    // The compositor's close request and the window's own button both come
    // through the guard registered for the window.
    let held = session.run(|_| guard::must_wait(u64::from(Platform::window_id())));
    assert!(held, "a window with a guard is held");
    session.wait_for("Save changes to 2 documents?");
    assert!(session.shows("“a.txt” and “b.txt” have unsaved changes"));
    session.click("Cancel");
    assert!(first.is_unsaved() && second.is_unsaved());
    // The refusal is forgotten: the next close asks again.
    session.run(|_| guard::must_wait(u64::from(Platform::window_id())));
    session.wait_for("Save changes to 2 documents?");
}

// ---- Another program changes the file ----------------------------------------

#[test]
fn another_program_changing_an_unedited_file_reloads_it_and_says_so() {
    reset();
    let mut session = Session::new("reload-quiet");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    // The watch is set up on a worker: give it a moment before the change.
    session.shot.settle(Duration::from_millis(400));
    std::fs::write(&path, "changed elsewhere").unwrap();
    session.wait_until("reloaded", |_| text_of(document) == "changed elsewhere");
    assert!(!document.is_unsaved());
    session.wait_for("was reloaded after another program changed it");
    let loads = world(|world| world.loads[&document.id().get()].borrow().clone());
    assert_eq!(loads, [LoadReason::Open, LoadReason::Reload]);
}

#[test]
fn another_program_changing_an_edited_file_asks_and_keeping_goes_on_to_replace_it() {
    reset();
    let mut session = Session::new("reload-keep");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "mine");
    session.shot.settle(Duration::from_millis(400));
    std::fs::write(&path, "theirs").unwrap();
    session.wait_for("“a.txt” changed on disk");
    assert_eq!(
        text_of(document),
        "mine",
        "nothing is lost before the person answers"
    );
    session.click("Keep my version");
    assert_eq!(text_of(document), "mine");
    assert!(document.is_unsaved() && document.change_on_disk().is_none());
    // The person kept their version: saving replaces the file without asking again.
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
}

#[test]
fn reloading_in_the_dialog_takes_the_file_and_drops_the_unsaved_changes() {
    reset();
    let mut session = Session::new("reload-take");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "mine");
    session.wait_drafts(2);
    session.shot.settle(Duration::from_millis(400));
    std::fs::write(&path, "theirs").unwrap();
    session.wait_for("“a.txt” changed on disk");
    session.click("Reload");
    session.wait_until("reloaded", |_| text_of(document) == "theirs");
    assert!(!document.is_unsaved());
    session.wait_drafts(0);
}

#[test]
fn a_file_that_is_deleted_keeps_its_document_open_and_unsaved() {
    reset();
    let mut session = Session::new("deleted");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.shot.settle(Duration::from_millis(400));
    std::fs::remove_file(&path).unwrap();
    session.wait_for("“a.txt” is no longer there");
    assert!(document.is_open());
    session.click("Keep open");
    assert!(document.is_open());
    assert!(document.is_unsaved(), "closing it would lose the only copy");
    assert_eq!(text_of(document), "one");
    // Saving makes the file again.
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
}

#[test]
fn a_change_noticed_only_when_the_window_is_looked_at_again_is_handled_the_same() {
    reset();
    let mut session = Session::new("focus");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    // No watch event: the file is compared when the window takes focus.
    session.run(|_| {
        let _ = std::fs::write(&path, "changed with no event, longer");
    });
    session.run(|docs| docs.check_changes());
    session.wait_until("reloaded", |_| {
        text_of(document) == "changed with no event, longer"
    });
}

// ---- Recovery ----------------------------------------------------------------

#[test]
fn work_a_crash_left_is_offered_and_recovering_brings_it_back_unsaved() {
    reset();
    let mut session = Session::with("recover", |dir| {
        let path = dir.join("notes.txt");
        std::fs::write(&path, "saved text").unwrap();
        let meta = DraftMeta {
            title: "notes.txt".into(),
            path: Some(path.clone()),
            base: Stamp::now(&path),
            extra: serde_json::json!({"app": {"caret": 4}, "untitled": 0, "scroll": [0.0, 120.0]}),
        };
        crashed_draft(dir, "69f-1-0", &meta, "typed but never saved");
    });
    session.wait_for("Recover “notes.txt”?");
    assert!(session.shows("Test App closed before you saved this document"));
    session.click("Recover");
    session.wait_until("the document is back", |_| {
        titles().contains(&"notes.txt".to_string())
    });
    let document = session
        .docs()
        .list()
        .into_iter()
        .find(|d| d.title() == "notes.txt")
        .unwrap();
    session.wait_until("its content is back", |_| {
        text_of(document) == "typed but never saved"
    });
    assert!(document.is_unsaved(), "recovered work is still unsaved");
    assert_eq!(document.display_title(), "notes.txt");
    assert_eq!(
        document.app_value(),
        serde_json::json!({"caret": 4}),
        "the app's value came back"
    );
    assert_eq!(document.scroll(), (0.0, 120.0));
    let init = world(|world| {
        world
            .inits
            .iter()
            .find(|init| init.origin == Origin::Recovered)
            .cloned()
    })
    .unwrap();
    assert_eq!(init.app_value, serde_json::json!({"caret": 4}));
    let loads = world(|world| world.loads[&document.id().get()].borrow().clone());
    assert_eq!(loads, [LoadReason::Recover]);
    // The file itself has not been touched.
    assert_eq!(
        std::fs::read_to_string(session.dir.join("notes.txt")).unwrap(),
        "saved text"
    );
    // Saving it writes the recovered text and ends the draft.
    session.run(|_| document.save());
    session.wait_until("saved", |_| !document.is_unsaved());
    assert_eq!(
        std::fs::read_to_string(session.dir.join("notes.txt")).unwrap(),
        "typed but never saved"
    );
    session.wait_drafts(0);
}

#[test]
fn discarding_recovered_work_deletes_the_drafts_and_starts_blank() {
    reset();
    let mut session = Session::with("recover-discard", |dir| {
        let meta = DraftMeta {
            title: "Untitled".into(),
            ..DraftMeta::default()
        };
        crashed_draft(dir, "69f-2-0", &meta, "lost");
    });
    session.wait_for("Recover “Untitled”?");
    session.click("Discard");
    session.wait_drafts(0);
    session.wait_until("a blank document", |_| titles() == ["Untitled"]);
    let blank = session.docs().list()[0];
    assert_eq!(text_of(blank), "");
}

#[test]
fn not_now_leaves_the_drafts_for_the_next_start() {
    reset();
    let mut session = Session::with("recover-later", |dir| {
        let meta = DraftMeta {
            title: "Untitled".into(),
            ..DraftMeta::default()
        };
        crashed_draft(dir, "69f-3-0", &meta, "kept");
    });
    session.wait_for("Recover “Untitled”?");
    session.click("Not now");
    session.wait_until("a blank document", |_| titles() == ["Untitled"]);
    assert_eq!(session.drafts().len(), 2, "nothing was deleted");
}

#[test]
fn a_draft_of_a_file_that_changed_since_says_so_after_it_is_recovered() {
    reset();
    let mut session = Session::with("recover-changed", |dir| {
        let path = dir.join("notes.txt");
        std::fs::write(&path, "v1").unwrap();
        let meta = DraftMeta {
            title: "notes.txt".into(),
            path: Some(path.clone()),
            base: Stamp::now(&path),
            extra: serde_json::Value::Null,
        };
        crashed_draft(dir, "69f-4-0", &meta, "my edit of v1");
        // Another program saved the file while the app was closed.
        std::fs::write(&path, "version 2 by someone else").unwrap();
    });
    session.wait_for("Recover “notes.txt”?");
    session.click("Recover");
    session.wait_for("“notes.txt” changed on disk");
}

#[test]
fn drafts_of_a_running_copy_are_not_offered() {
    reset();
    let mut session = Session::with("recover-running", |dir| {
        // Written by this process, which is running: another window's work.
        let meta = DraftMeta {
            title: "Untitled".into(),
            ..DraftMeta::default()
        };
        Drafts::in_dir(dir.join("sandbox/drafts"))
            .put("69f-5-0", &meta, b"sibling")
            .unwrap();
    });
    session.shot.settle(Duration::from_millis(500));
    assert!(!session.shows("Recover"));
    assert_eq!(titles(), ["Untitled"]);
}

// ---- The scene ---------------------------------------------------------------

#[test]
fn the_documents_of_the_last_run_come_back_in_order_with_their_values() {
    reset();
    let mut session = Session::with("scene", |dir| {
        std::fs::write(dir.join("a.txt"), "A").unwrap();
        std::fs::write(dir.join("b.txt"), "B").unwrap();
        std::fs::write(dir.join("c.txt"), "C").unwrap();
        let scened = |name: &str, caret: u64, current: bool| SceneDocument {
            path: Some(dir.join(name)),
            title: name.into(),
            scroll: (0.0, caret as f32 * 10.0),
            app: serde_json::json!({"caret": caret}),
            current,
            ..SceneDocument::default()
        };
        preload_for_test(SceneData {
            window: Some((1000.0, 700.0)),
            documents: vec![
                scened("a.txt", 1, false),
                scened("b.txt", 2, true),
                scened("c.txt", 3, false),
            ],
            ..SceneData::default()
        });
    });
    session.wait_until("all three are open", |_| titles().len() == 3);
    assert_eq!(titles(), ["a.txt", "b.txt", "c.txt"]);
    let b = session.docs().list()[1];
    session.wait_until("b is read", |_| b.phase() == Phase::Ready);
    assert_eq!(text_of(b), "B");
    assert_eq!(b.app_value(), serde_json::json!({"caret": 2}));
    assert_eq!(b.scroll(), (0.0, 20.0));
    let init = world(|world| {
        world
            .inits
            .iter()
            .find(|init| init.origin == Origin::Restored)
            .cloned()
    })
    .unwrap();
    assert_eq!(init.title, "a.txt");
    // The one that was showing is asked for.
    assert!(world(|world| world
        .events
        .contains(&format!("reveal {}", b.id().get()))));
    assert_eq!(titles().len(), 3, "no blank document beside them");
}

#[test]
fn a_file_that_was_open_and_is_gone_is_left_out_without_a_dialog() {
    reset();
    let mut session = Session::with("scene-gone", |dir| {
        std::fs::write(dir.join("kept.txt"), "K").unwrap();
        preload_for_test(SceneData {
            documents: vec![
                SceneDocument {
                    path: Some(dir.join("gone.txt")),
                    ..SceneDocument::default()
                },
                SceneDocument {
                    path: Some(dir.join("kept.txt")),
                    ..SceneDocument::default()
                },
            ],
            ..SceneData::default()
        });
    });
    session.wait_until("the kept one is open", |_| titles() == ["kept.txt"]);
    session.shot.settle(Duration::from_millis(300));
    assert!(!session.shows("Could not open"));
}

#[test]
fn the_scene_follows_the_documents_that_are_open_and_which_is_showing() {
    reset();
    let mut session = Session::new("scene-follows");
    let path = session.file("a.txt", "A");
    let document = session.open(&path);
    session.run(|docs| docs.set_current(Some(document)));
    session.run(|_| document.set_scroll(0.0, 55.0));
    let published = session.shot.settle(Duration::from_millis(400));
    let _ = published;
    let kept = current_documents_for_test();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].path.as_deref(), Some(path.as_path()));
    assert!(kept[0].current);
    assert_eq!(kept[0].scroll, (0.0, 55.0));
    // A document nobody typed in has nothing to bring back.
    let blank = session.run(|docs| docs.new_document());
    let _ = blank;
    session.shot.settle(Duration::from_millis(100));
    assert_eq!(current_documents_for_test().len(), 1);
}

// ---- Recents -------------------------------------------------------------------

#[test]
fn opening_and_saving_register_the_file_in_the_shared_list_and_the_menu_offers_it() {
    reset();
    let mut session = Session::new("recents");
    let path = session.file("a.txt", "A");
    session.open(&path);
    let docs = session.docs();
    session.wait_until("the menu has it", |_| docs.recent_menu().len() == 1);
    let xbel = std::fs::read_to_string(session.dir.join("sandbox/recently-used.xbel")).unwrap();
    assert!(
        xbel.contains("a.txt") && xbel.contains("name=\"Test App\""),
        "{xbel}"
    );
    // A file this app did not register is not in its menu.
    assert_eq!(
        recents::list_of_app_at(
            &session.dir.join("sandbox/recently-used.xbel"),
            "Other App",
            10
        ),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_save_that_cannot_be_encoded_offers_the_contents_fix_and_saves_again_when_it_is_taken() {
    reset();
    world(|world| {
        world.broken = true;
        world.offers = true;
    });
    let mut session = Session::new("fallback");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    session.run(|_| document.save());
    session.wait_for("Save in UTF-8");
    assert!(session.shows("It has a character this cannot hold."));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one",
        "nothing was written"
    );
    session.click("Save in UTF-8");
    session.wait_until("saved", |_| !document.is_unsaved() && !document.is_saving());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
}

#[test]
fn a_save_that_cannot_be_encoded_offers_nothing_when_the_content_has_no_fix() {
    reset();
    world(|world| world.broken = true);
    let mut session = Session::new("no-fallback");
    let path = session.file("a.txt", "one");
    let document = session.open(&path);
    session.set_text(document, "two");
    session.run(|_| document.save());
    session.wait_for("It has a character this cannot hold.");
    assert!(!session.shows("Save in UTF-8"));
    session.click("Close");
    assert!(document.is_unsaved());
}
