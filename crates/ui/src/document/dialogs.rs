//! The questions a document asks: unsaved changes, recovered work, a file
//! that changed or went away, a save that would replace someone else's
//! changes, and a save or open that failed.
//!
//! Each is a function that builds a [`Dialog`] for `overlays.show_dialog`
//! and tells one callback which response the person chose. The
//! [`Documents`](super::Documents) manager shows them at the right moments,
//! one at a time; they are public so that an app with a flow of its own, and
//! the taxonomy, can show the same dialogs.
//!
//! The rules they follow:
//!
//! - **A dialog that can lose work defaults to the safe answer.** Enter runs
//!   the response that keeps the work (Save, Keep, Recover); Escape is the
//!   answer that changes nothing (Cancel, Keep Open, Not Now). A response
//!   that loses work is marked destructive and is never the default.
//! - **The buttons say what happens.** Never "Yes", "No" or "OK".
//! - **Names are quoted** with “ ” so that a name that ends in a full stop or
//!   starts with a space reads as one.

use std::rc::Rc;

use crate::format::{List, Moment, Relative};
use crate::shell::{CANCEL_ID, Dialog, DialogIcon, ResponseKind, dialog};
use crate::tr;

use super::io::OpenError;
use super::write::SaveError;

/// How many names a dialog lists before it says "and 2 more".
const LISTED: usize = 3;

fn quoted(names: &[String]) -> String {
    let mut shown: Vec<String> = names
        .iter()
        .take(LISTED)
        .map(|name| format!("“{name}”"))
        .collect();
    let more = names.len().saturating_sub(LISTED);
    if more > 0 {
        shown.push(tr!("ui-document-more", count = more));
    }
    List::and(shown).format()
}

// ---- Unsaved changes -----------------------------------------------------

/// What the person chose in [`unsaved_changes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsaved {
    /// Save the document (every one of them, for a list), then go on.
    Save,
    /// Go on and lose the changes.
    Discard,
    /// Do not go on: keep the document open.
    Cancel,
}

/// "Save changes to “name”?", asked when a document with unsaved changes is
/// closed. With several names it is the question of closing a window that
/// holds them.
///
/// Save is the default and the suggested response; Discard is destructive and
/// is never focused; Escape is Cancel.
pub fn unsaved_changes(names: &[String], answer: impl Fn(Unsaved) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let many = names.len() > 1;
    let (title, body) = if many {
        (
            tr!("ui-document-unsaved-title-many", count = names.len()),
            tr!("ui-document-unsaved-body-many", names = quoted(names)),
        )
    } else {
        let name = names.first().map(String::as_str).unwrap_or_default();
        (
            tr!("ui-document-unsaved-title", name = name),
            tr!("ui-document-unsaved-body"),
        )
    };
    let (cancel, discard, save) = (answer.clone(), answer.clone(), answer);
    dialog(title, body)
        .icon(DialogIcon::Warning)
        .action(
            CANCEL_ID,
            tr!("ui-document-cancel"),
            ResponseKind::Default,
            move |_| cancel(Unsaved::Cancel),
        )
        .action(
            "discard",
            if many {
                tr!("ui-document-discard-all")
            } else {
                tr!("ui-document-discard")
            },
            ResponseKind::Destructive,
            move |_| discard(Unsaved::Discard),
        )
        .action(
            "save",
            if many {
                tr!("ui-document-save-all")
            } else {
                tr!("ui-document-save")
            },
            ResponseKind::Suggested,
            move |_| save(Unsaved::Save),
        )
        .default_action("save")
        .cancel_action(CANCEL_ID)
}

// ---- Recovered work ------------------------------------------------------

/// One document that a crash left behind, as [`recover_work`] lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Recoverable {
    /// The name the document had ("notes.txt", "Untitled").
    pub title: String,
    /// When its last copy was written.
    pub saved: Moment,
}

/// What the person chose in [`recover_work`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Open the documents again, with their unsaved changes.
    Recover,
    /// Delete the copies.
    Discard,
    /// Leave the copies where they are and ask again next time.
    Later,
}

/// "Recover unsaved work?", asked when the app starts and finds the copies a
/// crash or a power cut left. The wording says how long ago and which
/// documents, so that the person can tell whether it is what they remember.
///
/// Recover is the default and the suggested response; Discard is destructive;
/// Escape is Not Now, which deletes nothing.
pub fn recover_work(
    app_name: &str,
    items: &[Recoverable],
    answer: impl Fn(Recovery) + 'static,
) -> Dialog {
    let answer = Rc::new(answer);
    let titles: Vec<String> = items.iter().map(|item| item.title.clone()).collect();
    let newest = items.iter().map(|item| item.saved).max();
    let when = newest
        .map(|moment| Relative::new(moment).to_string())
        .unwrap_or_default();
    let (title, body) = match items {
        [only] => (
            tr!("ui-document-recover-title", name = only.title.as_str()),
            tr!("ui-document-recover-body", app = app_name, when = when),
        ),
        _ => (
            tr!("ui-document-recover-title-many", count = items.len()),
            tr!(
                "ui-document-recover-body-many",
                app = app_name,
                names = quoted(&titles),
                when = when
            ),
        ),
    };
    let (later, discard, recover) = (answer.clone(), answer.clone(), answer);
    dialog(title, body)
        .icon(DialogIcon::Info)
        .action(
            CANCEL_ID,
            tr!("ui-document-not-now"),
            ResponseKind::Default,
            move |_| later(Recovery::Later),
        )
        .action(
            "discard",
            tr!("ui-document-discard"),
            ResponseKind::Destructive,
            move |_| discard(Recovery::Discard),
        )
        .action(
            "recover",
            tr!("ui-document-recover"),
            ResponseKind::Suggested,
            move |_| recover(Recovery::Recover),
        )
        .default_action("recover")
        .cancel_action(CANCEL_ID)
}

// ---- A file that changed or went away ------------------------------------

/// What the person chose in [`file_changed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changed {
    /// Read the file again. The document's unsaved changes are lost.
    Reload,
    /// Keep the document as it is. Its next save replaces the file.
    Keep,
}

/// "“name” changed on disk", asked when another program changed a file whose
/// document has unsaved changes. A document with none is reloaded without
/// asking.
///
/// Keep is the default; Reload is destructive, because it replaces the
/// person's changes.
pub fn file_changed(name: &str, answer: impl Fn(Changed) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let (keep, reload) = (answer.clone(), answer);
    dialog(
        tr!("ui-document-changed-title", name = name),
        tr!("ui-document-changed-body"),
    )
    .icon(DialogIcon::Warning)
    .action(
        CANCEL_ID,
        tr!("ui-document-keep-my-version"),
        ResponseKind::Default,
        move |_| keep(Changed::Keep),
    )
    .action(
        "reload",
        tr!("ui-document-reload"),
        ResponseKind::Destructive,
        move |_| reload(Changed::Reload),
    )
    .default_action(CANCEL_ID)
    .cancel_action(CANCEL_ID)
}

/// What the person chose in [`file_removed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gone {
    /// Choose a place to save the document.
    SaveAs,
    /// Leave the document open, unsaved.
    KeepOpen,
}

/// "“name” is no longer there", asked when a file is deleted or moved while
/// its document is open. The document stays open either way.
pub fn file_removed(name: &str, answer: impl Fn(Gone) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let (keep, save_as) = (answer.clone(), answer);
    dialog(
        tr!("ui-document-removed-title", name = name),
        tr!("ui-document-removed-body"),
    )
    .icon(DialogIcon::Warning)
    .action(
        CANCEL_ID,
        tr!("ui-document-keep-open"),
        ResponseKind::Default,
        move |_| keep(Gone::KeepOpen),
    )
    .action(
        "save_as",
        tr!("ui-document-save-as"),
        ResponseKind::Suggested,
        move |_| save_as(Gone::SaveAs),
    )
    .default_action("save_as")
    .cancel_action(CANCEL_ID)
}

/// What the person chose in [`save_conflict`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Write the document over the file, losing the other program's changes.
    Replace,
    /// Choose another place and leave the file as it is.
    SaveAs,
    /// Do not save.
    Cancel,
}

/// "Replace “name”?", asked when a save finds that the file is not the one
/// the document read: another program changed it and the document has not
/// been told yet (or the person kept their version and saved later).
pub fn save_conflict(name: &str, answer: impl Fn(Conflict) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let (cancel, save_as, replace) = (answer.clone(), answer.clone(), answer);
    dialog(
        tr!("ui-document-conflict-title", name = name),
        tr!("ui-document-conflict-body"),
    )
    .icon(DialogIcon::Warning)
    .action(
        CANCEL_ID,
        tr!("ui-document-cancel"),
        ResponseKind::Default,
        move |_| cancel(Conflict::Cancel),
    )
    .action(
        "replace",
        tr!("ui-document-replace"),
        ResponseKind::Destructive,
        move |_| replace(Conflict::Replace),
    )
    .action(
        "save_as",
        tr!("ui-document-save-as"),
        ResponseKind::Suggested,
        move |_| save_as(Conflict::SaveAs),
    )
    .default_action("save_as")
    .cancel_action(CANCEL_ID)
}

// ---- A save or an open that failed ---------------------------------------

/// What the person chose in [`save_failed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retry {
    /// Save to the same place again.
    TryAgain,
    /// Choose another place.
    SaveAs,
    /// Leave the document unsaved.
    Cancel,
}

/// "Could not save “name”", with what to do about it as the body: choose
/// another place when this one will not do, try again when it may work.
pub fn save_failed(name: &str, error: &SaveError, answer: impl Fn(Retry) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let (cancel, other) = (answer.clone(), answer);
    let base = dialog(
        tr!("ui-document-save-failed-title", name = name),
        error.advice(),
    )
    .icon(DialogIcon::Error)
    .action(
        CANCEL_ID,
        tr!("ui-document-cancel"),
        ResponseKind::Default,
        move |_| cancel(Retry::Cancel),
    );
    match error {
        SaveError::Full | SaveError::Other(_) => base
            .action(
                "try_again",
                tr!("ui-document-try-again"),
                ResponseKind::Suggested,
                {
                    let again = other.clone();
                    move |_| again(Retry::TryAgain)
                },
            )
            .action(
                "save_as",
                tr!("ui-document-save-as"),
                ResponseKind::Default,
                move |_| other(Retry::SaveAs),
            )
            .default_action("try_again"),
        _ => base
            .action(
                "save_as",
                tr!("ui-document-save-as"),
                ResponseKind::Suggested,
                move |_| other(Retry::SaveAs),
            )
            .default_action("save_as"),
    }
    .cancel_action(CANCEL_ID)
}

/// "Could not save “name”" for a document the app could not turn into bytes.
/// `advice` is the app's own text: what is wrong and what to change.
pub fn save_not_possible(name: &str, advice: &str) -> Dialog {
    dialog(tr!("ui-document-save-failed-title", name = name), advice)
        .icon(DialogIcon::Error)
        .cancel(tr!("ui-document-close"))
}

/// Like [`save_not_possible`], with the one way out the app offers: a button
/// labelled `label` (the default) that runs `apply`, which the manager follows
/// with a new save. The app's text is what the button does, such as "Save in
/// UTF-8".
pub fn save_not_possible_offering(
    name: &str,
    advice: &str,
    label: &str,
    apply: impl Fn() + 'static,
) -> Dialog {
    dialog(tr!("ui-document-save-failed-title", name = name), advice)
        .icon(DialogIcon::Error)
        .action(
            CANCEL_ID,
            tr!("ui-document-close"),
            ResponseKind::Default,
            |_| {},
        )
        .action("fallback", label, ResponseKind::Suggested, move |_| apply())
        .default_action("fallback")
        .cancel_action(CANCEL_ID)
}

/// "Could not open “name”", with what to do about it as the body.
pub fn open_failed(name: &str, error: &OpenError) -> Dialog {
    dialog(
        tr!("ui-document-open-failed-title", name = name),
        error.advice(),
    )
    .icon(DialogIcon::Error)
    .cancel(tr!("ui-document-close"))
}

/// What the person chose in [`revert_changes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revert {
    /// Read the file again and lose the unsaved changes.
    Revert,
    /// Keep editing.
    Cancel,
}

/// "Revert “name”?", asked when the person reloads a document that has
/// unsaved changes. Cancel is the default.
pub fn revert_changes(name: &str, answer: impl Fn(Revert) + 'static) -> Dialog {
    let answer = Rc::new(answer);
    let (cancel, revert) = (answer.clone(), answer);
    dialog(
        tr!("ui-document-revert-title", name = name),
        tr!("ui-document-revert-body"),
    )
    .icon(DialogIcon::Warning)
    .action(
        CANCEL_ID,
        tr!("ui-document-cancel"),
        ResponseKind::Default,
        move |_| cancel(Revert::Cancel),
    )
    .action(
        "revert",
        tr!("ui-document-revert"),
        ResponseKind::Destructive,
        move |_| revert(Revert::Revert),
    )
    .default_action(CANCEL_ID)
    .cancel_action(CANCEL_ID)
}

/// "“name” is already open", asked when Save as picks a file that another
/// document has open: two documents for one file would overwrite each other.
pub fn already_open(name: &str) -> Dialog {
    dialog(
        tr!("ui-document-open-elsewhere-title", name = name),
        tr!("ui-document-open-elsewhere-body"),
    )
    .icon(DialogIcon::Warning)
    .cancel(tr!("ui-document-close"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn labels(dialog: &Dialog) -> Vec<(&str, ResponseKind)> {
        dialog
            .responses
            .iter()
            .map(|response| (response.label.as_str(), response.kind))
            .collect()
    }

    fn default_id(dialog: &Dialog) -> &str {
        &dialog.responses[dialog.plan().default.expect("a default")].id
    }

    fn cancel_id(dialog: &Dialog) -> &str {
        &dialog.responses[dialog.plan().cancel.expect("a cancel")].id
    }

    fn names(count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("note {n}.txt")).collect()
    }

    #[test]
    fn unsaved_changes_defaults_to_saving_and_never_to_discarding() {
        let one = unsaved_changes(&names(1), |_| {});
        assert_eq!(one.title, "Save changes to “note 1.txt”?");
        assert_eq!(
            labels(&one),
            [
                ("Cancel", ResponseKind::Default),
                ("Discard", ResponseKind::Destructive),
                ("Save", ResponseKind::Suggested),
            ]
        );
        assert_eq!(default_id(&one), "save");
        assert_eq!(cancel_id(&one), CANCEL_ID);
        let plan = one.plan();
        assert_eq!(
            plan.initial, plan.default,
            "the default is not destructive, so it is focused"
        );
    }

    #[test]
    fn several_unsaved_documents_are_counted_and_listed_with_a_cap() {
        let three = unsaved_changes(&names(3), |_| {});
        assert_eq!(three.title, "Save changes to 3 documents?");
        assert!(
            three
                .body
                .starts_with("“note 1.txt”, “note 2.txt” and “note 3.txt” have")
        );
        let five = unsaved_changes(&names(5), |_| {});
        assert!(
            five.body.contains("“note 3.txt” and 2 more have"),
            "{}",
            five.body
        );
        assert_eq!(five.responses[1].label, "Discard all");
        assert_eq!(five.responses[2].label, "Save all");
    }

    #[test]
    fn each_response_tells_the_caller_which_it_was() {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let asked = unsaved_changes(&names(1), move |choice| sink.borrow_mut().push(choice));
        for response in &asked.responses {
            response.on_press.call(());
        }
        assert_eq!(
            *heard.borrow(),
            [Unsaved::Cancel, Unsaved::Discard, Unsaved::Save]
        );
    }

    #[test]
    fn recovery_offers_not_now_as_the_way_out_and_deletes_nothing_by_default() {
        let one = recover_work(
            "Text Editor",
            &[Recoverable {
                title: "notes.txt".into(),
                saved: Moment::now(),
            }],
            |_| {},
        );
        assert_eq!(one.title, "Recover “notes.txt”?");
        assert!(
            one.body
                .starts_with("Text Editor closed before you saved this document.")
        );
        assert_eq!(default_id(&one), "recover");
        assert_eq!(cancel_id(&one), CANCEL_ID);
        assert_eq!(one.responses[0].label, "Not now");
        assert_eq!(one.responses[1].kind, ResponseKind::Destructive);
    }

    #[test]
    fn changed_on_disk_keeps_the_persons_version_unless_told_to_reload() {
        let asked = file_changed("a.txt", |_| {});
        assert_eq!(default_id(&asked), CANCEL_ID);
        assert_eq!(
            labels(&asked),
            [
                ("Keep my version", ResponseKind::Default),
                ("Reload", ResponseKind::Destructive),
            ]
        );
    }

    #[test]
    fn a_failed_save_offers_what_can_work() {
        let full = save_failed("a.txt", &SaveError::Full, |_| {});
        assert_eq!(full.title, "Could not save “a.txt”");
        assert!(full.responses.iter().any(|r| r.label == "Try again"));
        let denied = save_failed("a.txt", &SaveError::Permission, |_| {});
        assert!(!denied.responses.iter().any(|r| r.label == "Try again"));
        assert_eq!(default_id(&denied), "save_as");
    }

    #[test]
    fn no_dialog_asks_with_yes_no_or_ok() {
        let all = [
            unsaved_changes(&names(1), |_| {}),
            file_changed("a", |_| {}),
            file_removed("a", |_| {}),
            save_conflict("a", |_| {}),
            save_failed("a", &SaveError::Full, |_| {}),
            open_failed("a", &OpenError::Missing),
        ];
        for dialog in &all {
            for response in &dialog.responses {
                assert!(
                    !["Yes", "No", "OK", "Ok"].contains(&response.label.as_str()),
                    "{}: {}",
                    dialog.title,
                    response.label
                );
            }
        }
    }
}
