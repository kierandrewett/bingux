//! The editor's commands: what [`keymap`] answers with keys, as
//! [`Command`]s, so the menus, the command palette and the shortcuts sheet
//! list them and teach their keys.
//!
//! Wrap the editor in a [command scope](crate::commands::command_scope):
//!
//! ```ignore
//! command_scope("Editor", editor_commands(&handle, false), text_editor(handle.clone()))
//!     .fill()
//! ```
//!
//! The view answers every one of these keys itself, so the commands are
//! marked [`keys_handled_by_view`](Command::keys_handled_by_view): the
//! router shows their shortcuts and never runs them a second time. Their
//! enabled states follow the document (Undo is dimmed with nothing to undo),
//! and Undo and Redo are named after the edit ("Undo typing", "Redo
//! duplicate line") from the engine's own history, which stays per document.

use freya::prelude::*;

use bingux_text::{Case, Editor};

use super::EditorHandle;
use super::keymap::{self, Effect};
use crate::commands::{Command, CommandGroup};

/// The group of the commands that work on lines and comments.
pub const TEXT_GROUP: CommandGroup = CommandGroup::Other("Text");

/// Runs `command` on the document, with the clipboard it may need.
fn run(handle: &EditorHandle, command: &keymap::Command, read_only: bool) {
    let effect = handle.update(|editor| keymap::run(editor, command, 20, read_only));
    match effect {
        Effect::Copy(text) => {
            let _ = Clipboard::set(text);
        }
        Effect::Paste => {
            if let Ok(text) = Clipboard::get() {
                handle.update(|editor| editor.paste(&text));
            }
        }
        Effect::Zoom(_) | Effect::None | Effect::Unhandled => {}
    }
}

/// A command that runs `key` on the document.
fn command(
    handle: &EditorHandle,
    read_only: bool,
    id: &'static str,
    title: impl Into<String>,
    key: keymap::Command,
) -> Command {
    let handle = handle.clone();
    Command::new(id, title, move |_| run(&handle, &key, read_only)).keys_handled_by_view()
}

/// A command with no key that changes the text with `change`, as one undo
/// step named `label`.
fn action(
    handle: &EditorHandle,
    read_only: bool,
    id: &'static str,
    title: &str,
    label: &'static str,
    change: fn(&mut Editor),
) -> Command {
    let handle = handle.clone();
    Command::new(id, title, move |_| {
        if !read_only {
            handle.update(|editor| editor.label_edit(label, change));
        }
    })
    .enabled(!read_only)
    .group(TEXT_GROUP)
}

/// "Undo typing", "Redo": the verb, then what the step did.
fn titled(verb: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => format!("{verb} {label}"),
        None => verb.to_owned(),
    }
}

/// The commands of a text editor on `handle`: Undo and Redo, the clipboard,
/// selecting, and the line and comment edits. `read_only` dims the ones that
/// change the text. Pass them to [`command_scope`](crate::commands::command_scope)
/// around the editor.
///
/// Reading the document subscribes the caller, so the titles of Undo and
/// Redo and the dimmed states follow every edit.
pub fn editor_commands(handle: &EditorHandle, read_only: bool) -> Vec<Command> {
    let editor = handle.read();
    let history = editor.history();
    let can_undo = history.can_undo() && !read_only;
    let can_redo = history.can_redo() && !read_only;
    let undo_title = titled("Undo", history.undo_label());
    let redo_title = titled("Redo", history.redo_label());
    let has_selection = editor.selection().has_selection();
    let has_text = editor.len_chars() > 0;
    drop(editor);

    let edit = |id, title: &str, key, shortcut: &str| {
        command(handle, read_only, id, title, key)
            .shortcut(shortcut)
            .enabled(!read_only)
    };
    vec![
        command(
            handle,
            read_only,
            "edit.undo",
            undo_title,
            keymap::Command::Undo,
        )
        .shortcut("Ctrl+Z")
        .enabled(can_undo)
        .group(CommandGroup::Edit)
        .section("history"),
        command(
            handle,
            read_only,
            "edit.redo",
            redo_title,
            keymap::Command::Redo,
        )
        .shortcut("Ctrl+Shift+Z")
        .shortcut("Ctrl+Y")
        .enabled(can_redo)
        .group(CommandGroup::Edit)
        .section("history"),
        command(handle, read_only, "edit.cut", "Cut", keymap::Command::Cut)
            .shortcut("Ctrl+X")
            .enabled(!read_only && (has_selection || has_text))
            .group(CommandGroup::Edit)
            .section("clipboard"),
        command(
            handle,
            read_only,
            "edit.copy",
            "Copy",
            keymap::Command::Copy,
        )
        .shortcut("Ctrl+C")
        .enabled(has_selection || has_text)
        .group(CommandGroup::Edit)
        .section("clipboard"),
        command(
            handle,
            read_only,
            "edit.paste",
            "Paste",
            keymap::Command::Paste,
        )
        .shortcut("Ctrl+V")
        .enabled(!read_only)
        .group(CommandGroup::Edit)
        .section("clipboard"),
        command(
            handle,
            read_only,
            "edit.select-all",
            "Select all",
            keymap::Command::SelectAll,
        )
        .shortcut("Ctrl+A")
        .group(CommandGroup::Edit)
        .section("select"),
        command(
            handle,
            read_only,
            "edit.select-line",
            "Select line",
            keymap::Command::SelectLine,
        )
        .shortcut("Ctrl+L")
        .group(CommandGroup::Edit)
        .section("select"),
        command(
            handle,
            read_only,
            "edit.add-next-occurrence",
            "Add next occurrence",
            keymap::Command::AddNextOccurrence,
        )
        .shortcut("Ctrl+Shift+D")
        .keywords(["multiple carets", "cursors"])
        .hidden_from_menu()
        .group(CommandGroup::Edit)
        .section("select"),
        command(
            handle,
            read_only,
            "edit.select-all-occurrences",
            "Select every occurrence",
            keymap::Command::SelectAllOccurrences,
        )
        .shortcut("Ctrl+Shift+L")
        .keywords(["multiple carets", "cursors"])
        .hidden_from_menu()
        .group(CommandGroup::Edit)
        .section("select"),
        command(
            handle,
            read_only,
            "edit.caret-above",
            "Add caret above",
            keymap::Command::AddCursor { up: true },
        )
        .shortcut("Ctrl+Alt+Up")
        .keywords(["cursor", "column"])
        .hidden_from_menu()
        .group(CommandGroup::Edit)
        .section("select"),
        command(
            handle,
            read_only,
            "edit.caret-below",
            "Add caret below",
            keymap::Command::AddCursor { up: false },
        )
        .shortcut("Ctrl+Alt+Down")
        .keywords(["cursor", "column"])
        .hidden_from_menu()
        .group(CommandGroup::Edit)
        .section("select"),
        Command::key_hint("edit.caret-click", "Add or remove a caret", "Alt+click")
            .group(CommandGroup::Edit)
            .section("select"),
        command(
            handle,
            read_only,
            "go.matching-bracket",
            "Go to matching bracket",
            keymap::Command::MatchingBracket,
        )
        .shortcut("Ctrl+M")
        .group(CommandGroup::Go)
        .section("brackets"),
        edit(
            "text.comment",
            "Comment or uncomment",
            keymap::Command::ToggleComment,
            "Ctrl+/",
        )
        .keywords(["toggle comment", "uncomment"])
        .group(TEXT_GROUP)
        .section("lines"),
        edit(
            "text.duplicate-line",
            "Duplicate line",
            keymap::Command::Duplicate,
            "Ctrl+D",
        )
        .keywords(["copy line"])
        .group(TEXT_GROUP)
        .section("lines"),
        edit(
            "text.delete-line",
            "Delete line",
            keymap::Command::DeleteLine,
            "Ctrl+Shift+K",
        )
        .group(TEXT_GROUP)
        .section("lines"),
        edit(
            "text.move-line-up",
            "Move line up",
            keymap::Command::MoveLines { up: true },
            "Alt+Up",
        )
        .group(TEXT_GROUP)
        .section("lines"),
        edit(
            "text.move-line-down",
            "Move line down",
            keymap::Command::MoveLines { up: false },
            "Alt+Down",
        )
        .group(TEXT_GROUP)
        .section("lines"),
        edit(
            "text.join-lines",
            "Join lines",
            keymap::Command::JoinLines,
            "Ctrl+J",
        )
        .group(TEXT_GROUP)
        .section("lines"),
        Command::key_hint("text.indent", "Indent or outdent", "Tab, Shift+Tab")
            .group(TEXT_GROUP)
            .section("lines"),
        Command::key_hint("text.delete-word", "Delete a word", "Ctrl+Backspace")
            .group(TEXT_GROUP)
            .section("lines"),
        action(
            handle,
            read_only,
            "text.sort-lines",
            "Sort lines",
            "sort lines",
            |editor| editor.sort_lines(),
        )
        .section("lines"),
        action(
            handle,
            read_only,
            "text.trim-trailing",
            "Remove trailing spaces",
            "remove trailing spaces",
            |editor| {
                editor.trim_trailing_whitespace();
            },
        )
        .keywords(["whitespace", "trim"])
        .section("lines"),
        action(
            handle,
            read_only,
            "text.uppercase",
            "Uppercase",
            "uppercase",
            |editor| editor.transform_case(Case::Upper),
        )
        .keywords(["capitals", "case"])
        .section("case"),
        action(
            handle,
            read_only,
            "text.lowercase",
            "Lowercase",
            "lowercase",
            |editor| editor.transform_case(Case::Lower),
        )
        .keywords(["case"])
        .section("case"),
        action(
            handle,
            read_only,
            "text.title-case",
            "Title case",
            "title case",
            |editor| editor.transform_case(Case::Title),
        )
        .keywords(["capitalise", "case"])
        .section("case"),
    ]
}

// Failure modes guarded: Undo and Redo keeping a stale title after an edit;
// the menu's Undo and the key undoing different things; a command running
// twice for one key (the view answers it, the router must not); Redo enabled
// with nothing to redo.
#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use bingux_text::Editor;
    use freya::prelude::*;

    use super::*;
    use crate::commands::{command_scope, use_commands};
    use crate::controls::action_button;
    use crate::shell::overlays::OverlayHost;
    use crate::snapshot::Snapshot;
    use crate::text_editor::{text_editor, use_editor};
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    thread_local! {
        static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static HANDLE: RefCell<Option<EditorHandle>> = const { RefCell::new(None) };
    }

    #[derive(PartialEq)]
    struct Scene;

    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let handle = use_editor(|| Editor::new("one\ntwo"));
            HANDLE.with(|slot| *slot.borrow_mut() = Some(handle.clone()));
            let commands = use_commands();
            let scope = editor_commands(&handle, false);
            let report = move |_: ()| {
                let state = |id: &str| {
                    commands
                        .get(id)
                        .map(|command| {
                            format!(
                                "{}{}",
                                command.title(),
                                if command.is_enabled() { "" } else { " (off)" }
                            )
                        })
                        .unwrap_or_else(|| format!("{id} missing"))
                };
                LOG.with(|log| {
                    log.borrow_mut().push(format!(
                        "{} / {}",
                        state("edit.undo"),
                        state("edit.redo")
                    ))
                });
            };
            rect()
                .expanded()
                .child(
                    rect()
                        .width(Size::px(400.0))
                        .height(Size::px(200.0))
                        .child(command_scope("Editor", scope, text_editor(handle.clone())).fill()),
                )
                .child(action_button("Report", report))
        }
    }

    #[derive(PartialEq)]
    struct Root;

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            WindowHost::new(OverlayHost::new(Scene))
        }
    }

    fn text() -> String {
        HANDLE.with(|slot| slot.borrow().as_ref().expect("a handle").peek().text())
    }

    fn report(shot: &mut Snapshot) -> String {
        assert!(shot.click_text("Report"));
        shot.settle(Duration::from_millis(200));
        LOG.with(|log| log.borrow_mut().pop().expect("a report"))
    }

    #[test]
    fn undo_and_redo_are_named_after_the_edit_and_follow_the_keys() {
        LOG.with(|log| log.borrow_mut().clear());
        let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
        shot.settle(Duration::from_millis(300));
        // The editor's scope holds the focus once the editor is pressed.
        shot.click(100.0, 20.0);
        shot.settle(Duration::from_millis(200));
        shot.key(Key::Character("x".into()));
        shot.settle(Duration::from_millis(200));
        assert_eq!(
            report(&mut shot),
            "Undo typing / Redo (off)",
            "typing names the step, and there is nothing to redo"
        );
        // The report button took the keyboard: press the editor again.
        shot.click(100.0, 20.0);
        shot.settle(Duration::from_millis(200));
        shot.key_with(Key::Character("d".into()), Modifiers::CONTROL);
        shot.settle(Duration::from_millis(200));
        assert_eq!(
            report(&mut shot),
            "Undo duplicate line / Redo (off)",
            "the key ran the command once: one step, named for it"
        );
        let doubled = text();
        shot.click(100.0, 20.0);
        shot.settle(Duration::from_millis(200));
        shot.key_with(Key::Character("z".into()), Modifiers::CONTROL);
        shot.settle(Duration::from_millis(200));
        assert_ne!(text(), doubled, "Ctrl+Z undid the duplicate");
        assert_eq!(
            report(&mut shot),
            "Undo typing / Redo duplicate line",
            "the next step back, and what Redo would do"
        );
    }
}
