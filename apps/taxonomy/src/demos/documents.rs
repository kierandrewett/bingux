//! Documents: the questions a document asks, the manager behind them, and
//! what an app is told when a file or a save fails. Every dialog specimen is
//! the button that opens it, and the script opens it and photographs it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use bingux_ui::document::dialogs::{
    Recoverable, already_open, file_changed, file_removed, open_failed, recover_work,
    revert_changes, save_conflict, save_failed, save_not_possible, unsaved_changes,
};
use bingux_ui::document::{
    DocumentContent, DocumentEvent, DocumentId, DocumentSnapshot, DocumentsConfig, LoadReason,
    OpenError, SaveError, use_document_title, use_documents,
};
use bingux_ui::format::Moment;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

const OPEN: &str = "click-text Open dialog; wait 500; save {out}";

fn names(count: usize) -> Vec<String> {
    [
        "Shopping list.txt",
        "Meeting notes.md",
        "todo.txt",
        "draft.txt",
        "Untitled",
    ]
    .iter()
    .take(count)
    .map(|name| (*name).to_string())
    .collect()
}

/// The moment the recovery specimens say their copies were written: a fixed
/// minute before the specimen's "now", so a screenshot reads the same every day.
fn minutes_ago(minutes: i64) -> Moment {
    Moment::from_unix(Moment::now().unix_seconds() - minutes * 60)
}

pub const DIALOGS: &[Spec] = &[
    Spec::live(
        "Unsaved changes: Save is the default, Discard is never focused",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(unsaved_changes(&names(1), |_| {}))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live(
        "Unsaved changes: Tab moves the focus, the default mark stays",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(unsaved_changes(&names(1), |_| {}))
            })
            .into_element()
        },
    )
    .script("click-text Open dialog; wait 500; key tab; wait 300; save {out}"),
    Spec::live(
        "Unsaved changes: closing a window that holds several",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(unsaved_changes(&names(5), |_| {}))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live("Recover: work a crash left, one document", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(recover_work(
                "Text Editor",
                &[Recoverable {
                    title: "Meeting notes.md".into(),
                    saved: minutes_ago(4),
                }],
                |_| {},
            ))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live(
        "Recover: several documents, Not now deletes nothing",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(recover_work(
                    "Text Editor",
                    &names(4)
                        .into_iter()
                        .map(|title| Recoverable {
                            title,
                            saved: minutes_ago(12),
                        })
                        .collect::<Vec<_>>(),
                    |_| {},
                ))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live(
        "Reload: another program changed the file, and there are unsaved changes",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(file_changed("Meeting notes.md", |_| {}))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live("The file was deleted or moved", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(file_removed("Meeting notes.md", |_| {}))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("A save would replace another program's changes", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(save_conflict("Meeting notes.md", |_| {}))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live(
        "A save failed: the drive is full (try again, or save elsewhere)",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(save_failed("Meeting notes.md", &SaveError::Full, |_| {}))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live(
        "A save failed: no permission (only Save as can work)",
        || {
            let overlays = use_overlays();
            action_button("Open dialog", move |_| {
                overlays.show_dialog(save_failed(
                    "Meeting notes.md",
                    &SaveError::Permission,
                    |_| {},
                ))
            })
            .into_element()
        },
    )
    .script(OPEN),
    Spec::live("The content could not be turned into a file", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(save_not_possible(
                "Meeting notes.txt",
                "The text has characters that Latin-1 cannot hold. Save it as UTF-8.",
            ))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("A file could not be opened", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(open_failed("Meeting notes.md", &OpenError::Missing))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Revert to the saved version", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(revert_changes("Meeting notes.md", |_| {}))
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("A name another document already has", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(already_open("Meeting notes.md"))
        })
        .into_element()
    })
    .script(OPEN),
];

// ---- The manager, live ---------------------------------------------------------

/// The app's side of a document in the specimen: a string.
struct Note {
    text: Rc<RefCell<String>>,
}

impl DocumentContent for Note {
    fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::bytes(self.text.borrow().clone().into_bytes())
    }

    fn load(&self, bytes: Vec<u8>, _reason: LoadReason) {
        *self.text.borrow_mut() = String::from_utf8_lossy(&bytes).into_owned();
    }
}

/// A scratch folder with one file in it, so that the specimen has a document
/// that has a file and one that has none. Made once, when the specimen mounts.
fn scratch() -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("bingux-taxonomy-documents-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("Shopping list.txt");
    if !file.exists() {
        let _ = std::fs::write(&file, "Oat milk\nCoffee\nBatteries\n");
    }
    dir
}

#[derive(PartialEq)]
struct Scratch;

impl Component for Scratch {
    fn render(&self) -> impl IntoElement {
        let notes = use_hook(|| {
            Rc::new(RefCell::new(
                HashMap::<DocumentId, Rc<RefCell<String>>>::new(),
            ))
        });
        let folder = use_hook(scratch);
        let made = notes.clone();
        let documents = use_documents(
            DocumentsConfig::new("bingux-taxonomy-documents", "Taxonomy")
                .restore(false)
                .blank_when_empty(false)
                .autosave_after(Duration::from_secs(2))
                .sandboxed(folder.join("state")),
            move |init| {
                let text = Rc::new(RefCell::new(String::new()));
                made.borrow_mut().insert(init.id, text.clone());
                Rc::new(Note { text })
            },
            |_: DocumentEvent| {},
        );
        use_document_title(documents, "Taxonomy");
        use_hook({
            let file = folder.join("Shopping list.txt");
            move || {
                documents.open_path(file);
                documents.new_document();
            }
        });
        let mut group = Group::new("Documents".to_string()).trailing(
            action_button("New", move |_| {
                documents.new_document();
            })
            .flat()
            .compact(),
        );
        for document in documents.list() {
            let text = notes.borrow().get(&document.id()).cloned();
            group = group.row(
                row(document.display_title())
                    .subtitle(if document.is_unsaved() {
                        "Unsaved changes"
                    } else {
                        "Saved"
                    })
                    .trailing(
                        hstack(S2)
                            .child(
                                action_button("Type", move |_| {
                                    if let Some(text) = &text {
                                        text.borrow_mut().push_str("more ");
                                    }
                                    document.mark_edited();
                                })
                                .compact(),
                            )
                            .child(action_button("Save", move |_| document.save()).compact())
                            .child(action_button("Close", move |_| document.close()).compact()),
                    ),
            );
        }
        group.into_element()
    }
}

pub const MANAGER: &[Spec] = &[Spec::live(
    "A document that has a file and one that has none: Type marks them, Close asks",
    || Scratch.into_element(),
)
.script("wait 400; click-text Type; wait 300; click-text Close; wait 500; save {out}")];

// ---- What goes wrong, in words ----------------------------------------------------

pub const FAILURES: &[Spec] = &[
    Spec::forced("Why a save fails, and what the person is told", || {
        rows(
            [
                ("Full", SaveError::Full),
                ("Permission", SaveError::Permission),
                ("FolderMissing", SaveError::FolderMissing),
                ("ReadOnly", SaveError::ReadOnly),
                ("BadName", SaveError::BadName),
                ("Other", SaveError::Other("EIO".into())),
            ]
            .into_iter()
            .map(|(name, error)| property_row(name, error.advice()).into_element())
            .collect(),
        )
    }),
    Spec::forced(
        "Why a file does not open, and what the person is told",
        || {
            rows(
                [
                    ("Missing", OpenError::Missing),
                    ("Permission", OpenError::Permission),
                    ("NotAFile", OpenError::NotAFile),
                    ("TooLarge", OpenError::TooLarge(1 << 30)),
                    ("Other", OpenError::Other("EIO".into())),
                ]
                .into_iter()
                .map(|(name, error)| property_row(name, error.advice()).into_element())
                .collect(),
            )
        },
    ),
];
