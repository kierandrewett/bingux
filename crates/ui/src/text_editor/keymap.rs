//! The editor's keys: which command a key press is, and running it on an
//! [`Editor`]. Pure, so the bindings are tested without a window; the view
//! only adds the clipboard and scrolling.
//!
//! | Keys | Command |
//! | ---- | ------- |
//! | arrows, Home, End, Page Up, Page Down, Ctrl+Home, Ctrl+End | move; with Shift, select |
//! | Ctrl+Left, Ctrl+Right | by words |
//! | Home | the first character after the indentation, then the line's start |
//! | Backspace, Delete; Ctrl+Backspace, Ctrl+Delete | a character; a word |
//! | Ctrl+Shift+Backspace | to the start of the line |
//! | Enter; Ctrl+Enter; Ctrl+Shift+Enter | a new line; a line below; a line above |
//! | Tab, Shift+Tab | indent, outdent |
//! | Ctrl+Z; Ctrl+Shift+Z, Ctrl+Y | undo; redo |
//! | Ctrl+X, Ctrl+C, Ctrl+V | cut, copy, paste (the whole line with nothing selected) |
//! | Ctrl+A; Ctrl+L | select all; select the line |
//! | Ctrl+D | duplicate the line or the selection |
//! | Ctrl+Shift+K | delete the line |
//! | Alt+Up, Alt+Down | move the line |
//! | Ctrl+J | join the next line |
//! | Ctrl+/ | comment or uncomment |
//! | Ctrl+M | go to the matching bracket |
//! | Ctrl+Alt+Up, Ctrl+Alt+Down | add a caret above or below |
//! | Ctrl+Shift+D | add the next occurrence of the selection |
//! | Ctrl+Shift+L | select every occurrence |
//! | Escape | back to one caret, or drop the selection |
//! | Ctrl+=, Ctrl+-, Ctrl+0 | zoom in, out, back to 100 % |

use freya::prelude::*;

use bingux_text::Editor;

/// One thing the editor can be asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Type(String),
    Newline,
    LineBelow,
    LineAbove,
    Backspace,
    Delete,
    DeleteWord {
        backward: bool,
    },
    DeleteToLineStart,
    Indent,
    Outdent,
    Move {
        forward: bool,
        word: bool,
        extend: bool,
    },
    Vertical {
        rows: isize,
        extend: bool,
    },
    Page {
        down: bool,
        extend: bool,
    },
    Home {
        extend: bool,
    },
    End {
        extend: bool,
    },
    DocumentStart {
        extend: bool,
    },
    DocumentEnd {
        extend: bool,
    },
    SelectAll,
    SelectLine,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    DeleteLine,
    MoveLines {
        up: bool,
    },
    JoinLines,
    ToggleComment,
    MatchingBracket,
    AddCursor {
        up: bool,
    },
    AddNextOccurrence,
    SelectAllOccurrences,
    Collapse,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

impl Command {
    /// Whether the command changes the text, so a read-only editor ignores it.
    pub fn edits(&self) -> bool {
        matches!(
            self,
            Command::Type(_)
                | Command::Newline
                | Command::LineBelow
                | Command::LineAbove
                | Command::Backspace
                | Command::Delete
                | Command::DeleteWord { .. }
                | Command::DeleteToLineStart
                | Command::Indent
                | Command::Outdent
                | Command::Undo
                | Command::Redo
                | Command::Cut
                | Command::Paste
                | Command::Duplicate
                | Command::DeleteLine
                | Command::MoveLines { .. }
                | Command::JoinLines
                | Command::ToggleComment
        )
    }
}

/// The command for a key press, or `None` for a key the editor leaves to
/// the window (Ctrl+F, Ctrl+S, Ctrl+Tab and the rest).
pub fn command_for(key: &Key, modifiers: Modifiers) -> Option<Command> {
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let shift = modifiers.contains(Modifiers::SHIFT);
    let alt = modifiers.contains(Modifiers::ALT);
    let meta = modifiers.contains(Modifiers::META);
    if meta {
        return None;
    }
    Some(match key {
        Key::Named(named) => match named {
            NamedKey::ArrowLeft => Command::Move {
                forward: false,
                word: ctrl,
                extend: shift,
            },
            NamedKey::ArrowRight => Command::Move {
                forward: true,
                word: ctrl,
                extend: shift,
            },
            NamedKey::ArrowUp if alt && !ctrl => Command::MoveLines { up: true },
            NamedKey::ArrowDown if alt && !ctrl => Command::MoveLines { up: false },
            NamedKey::ArrowUp if alt && ctrl => Command::AddCursor { up: true },
            NamedKey::ArrowDown if alt && ctrl => Command::AddCursor { up: false },
            // Ctrl+Up and Down scroll in other editors; here they move as
            // the arrows do, so the key is never dead.
            NamedKey::ArrowUp => Command::Vertical {
                rows: -1,
                extend: shift,
            },
            NamedKey::ArrowDown => Command::Vertical {
                rows: 1,
                extend: shift,
            },
            NamedKey::Home if ctrl => Command::DocumentStart { extend: shift },
            NamedKey::End if ctrl => Command::DocumentEnd { extend: shift },
            NamedKey::Home => Command::Home { extend: shift },
            NamedKey::End => Command::End { extend: shift },
            NamedKey::PageUp if !ctrl => Command::Page {
                down: false,
                extend: shift,
            },
            NamedKey::PageDown if !ctrl => Command::Page {
                down: true,
                extend: shift,
            },
            NamedKey::Backspace if ctrl && shift => Command::DeleteToLineStart,
            NamedKey::Backspace if ctrl || alt => Command::DeleteWord { backward: true },
            NamedKey::Backspace => Command::Backspace,
            NamedKey::Delete if ctrl && shift => Command::DeleteLine,
            NamedKey::Delete if ctrl || alt => Command::DeleteWord { backward: false },
            NamedKey::Delete => Command::Delete,
            NamedKey::Enter if ctrl && shift => Command::LineAbove,
            NamedKey::Enter if ctrl => Command::LineBelow,
            NamedKey::Enter if !alt => Command::Newline,
            NamedKey::Tab if ctrl || alt => return None,
            NamedKey::Tab if shift => Command::Outdent,
            NamedKey::Tab => Command::Indent,
            NamedKey::Escape if !ctrl && !alt && !shift => Command::Collapse,
            NamedKey::F3 if alt => Command::SelectAllOccurrences,
            _ => return None,
        },
        Key::Character(text) if ctrl && !alt => {
            let lower = text.to_lowercase();
            match (lower.as_str(), shift) {
                ("z", false) => Command::Undo,
                ("z", true) | ("y", false) => Command::Redo,
                ("x", false) => Command::Cut,
                ("c", false) => Command::Copy,
                ("v", false) => Command::Paste,
                ("a", false) => Command::SelectAll,
                ("l", false) => Command::SelectLine,
                ("l", true) => Command::SelectAllOccurrences,
                ("d", false) => Command::Duplicate,
                ("d", true) => Command::AddNextOccurrence,
                ("k", true) => Command::DeleteLine,
                ("j", false) => Command::JoinLines,
                // Ctrl+Shift+? is the window's (About); only Ctrl+/ comments.
                ("/", false) => Command::ToggleComment,
                ("m", false) => Command::MatchingBracket,
                ("=", _) | ("+", _) => Command::ZoomIn,
                ("-", false) | ("_", true) => Command::ZoomOut,
                ("0", false) => Command::ZoomReset,
                _ => return None,
            }
        }
        // Text, including what AltGr makes (it reports Alt on some layouts).
        Key::Character(text) if !ctrl && !text.chars().any(char::is_control) => {
            Command::Type(text.to_string())
        }
        _ => return None,
    })
}

/// What running a command asks of the view: the clipboard, and nothing
/// else (scrolling follows the caret by itself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Put this text on the clipboard.
    Copy(String),
    /// Read the clipboard and paste it.
    Paste,
    Zoom(ZoomStep),
    /// The command did nothing (Escape with one caret): the key may go on to
    /// the window.
    Unhandled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomStep {
    In,
    Out,
    Reset,
}

/// Runs `command`. `page_rows` is how many rows Page Up and Down move.
pub fn run(editor: &mut Editor, command: &Command, page_rows: usize, read_only: bool) -> Effect {
    if read_only && command.edits() {
        // A read-only document still copies.
        return match command {
            Command::Cut => editor
                .copy()
                .map_or(Effect::None, |copied| Effect::Copy(copied.text)),
            _ => Effect::None,
        };
    }
    match command {
        Command::Type(text) => editor.type_text(text),
        Command::Newline => editor.newline(),
        Command::LineBelow => {
            editor.end(false);
            editor.newline();
        }
        Command::LineAbove => {
            let line = editor.line_of(editor.selection().primary().head);
            if line == 0 {
                editor.set_cursor(0, false);
                editor.newline();
                editor.move_vertical(-1, false);
            } else {
                editor.move_vertical(-1, false);
                editor.end(false);
                editor.newline();
            }
        }
        Command::Backspace => editor.backspace(),
        Command::Delete => editor.delete_forward(),
        Command::DeleteWord { backward } => editor.delete_word(*backward),
        Command::DeleteToLineStart => editor.delete_to_line_start(),
        Command::Indent => editor.indent(),
        Command::Outdent => editor.outdent(),
        Command::Move {
            forward,
            word,
            extend,
        } => editor.move_horizontal(*forward, *word, *extend),
        Command::Vertical { rows, extend } => editor.move_vertical(*rows, *extend),
        Command::Page { down, extend } => {
            let rows = page_rows.max(1) as isize;
            editor.move_vertical(if *down { rows } else { -rows }, *extend);
        }
        Command::Home { extend } => editor.home(*extend),
        Command::End { extend } => editor.end(*extend),
        Command::DocumentStart { extend } => editor.document_start(*extend),
        Command::DocumentEnd { extend } => editor.document_end(*extend),
        Command::SelectAll => editor.select_all(),
        Command::SelectLine => editor.select_line(),
        Command::Undo => {
            editor.undo();
        }
        Command::Redo => {
            editor.redo();
        }
        Command::Cut => {
            return editor
                .cut()
                .map_or(Effect::None, |copied| Effect::Copy(copied.text));
        }
        Command::Copy => {
            return editor
                .copy()
                .map_or(Effect::None, |copied| Effect::Copy(copied.text));
        }
        Command::Paste => return Effect::Paste,
        Command::Duplicate => editor.duplicate(),
        Command::DeleteLine => editor.delete_lines(),
        Command::MoveLines { up } => editor.move_lines(*up),
        Command::JoinLines => editor.join_lines(),
        Command::ToggleComment => editor.toggle_comment(),
        Command::MatchingBracket => {
            let head = editor.selection().primary().head;
            if let Some((_, other)) = editor.matching_bracket(head) {
                // Land on the side of the bracket the caret came from.
                let target = if other < head { other } else { other + 1 };
                editor.set_cursor(target, false);
            }
        }
        Command::AddCursor { up } => editor.add_cursor_vertical(*up),
        Command::AddNextOccurrence => {
            editor.add_next_occurrence();
        }
        Command::SelectAllOccurrences => select_all_occurrences(editor),
        Command::Collapse => {
            if !editor.collapse() {
                return Effect::Unhandled;
            }
        }
        Command::ZoomIn => return Effect::Zoom(ZoomStep::In),
        Command::ZoomOut => return Effect::Zoom(ZoomStep::Out),
        Command::ZoomReset => return Effect::Zoom(ZoomStep::Reset),
    }
    Effect::None
}

/// Every occurrence of the primary selection (or the word at the caret)
/// becomes a selection.
pub fn select_all_occurrences(editor: &mut Editor) {
    let primary = editor.selection().primary();
    let (start, end) = if primary.is_empty() {
        editor.word_at(primary.head)
    } else {
        (primary.start(), primary.end())
    };
    if start == end {
        return;
    }
    let needle = editor.rope().slice(start..end).to_string();
    let query = bingux_text::Query {
        text: needle,
        case_sensitive: true,
        whole_word: primary.is_empty(),
        regex: false,
    };
    let Ok(pattern) = query.compile() else { return };
    let matches = bingux_text::search::find_all(editor.rope(), &pattern);
    let primary_index = matches
        .ranges
        .iter()
        .position(|range| range.0 == start)
        .unwrap_or(0);
    editor.select_ranges(&matches.ranges, primary_index);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn bindings() {
        assert_eq!(
            command_for(&key("d"), Modifiers::CONTROL),
            Some(Command::Duplicate)
        );
        assert_eq!(
            command_for(&Key::Named(NamedKey::ArrowUp), Modifiers::ALT),
            Some(Command::MoveLines { up: true })
        );
        assert_eq!(
            command_for(&Key::Named(NamedKey::Backspace), Modifiers::CONTROL),
            Some(Command::DeleteWord { backward: true })
        );
        assert_eq!(
            command_for(&key("/"), Modifiers::CONTROL),
            Some(Command::ToggleComment)
        );
        assert_eq!(
            command_for(&key("é"), Modifiers::empty()),
            Some(Command::Type("é".into()))
        );
        // AltGr text is typed.
        assert_eq!(
            command_for(&key("@"), Modifiers::ALT),
            Some(Command::Type("@".into()))
        );
        // The window's keys pass.
        assert_eq!(command_for(&key("s"), Modifiers::CONTROL), None);
        assert_eq!(command_for(&key("f"), Modifiers::CONTROL), None);
        assert_eq!(
            command_for(&Key::Named(NamedKey::Tab), Modifiers::CONTROL),
            None
        );
    }

    #[test]
    fn read_only_ignores_edits_but_copies() {
        let mut editor = Editor::new("abc");
        editor.select_all();
        assert_eq!(
            run(&mut editor, &Command::Type("x".into()), 10, true),
            Effect::None
        );
        assert_eq!(editor.text(), "abc");
        assert_eq!(
            run(&mut editor, &Command::Cut, 10, true),
            Effect::Copy("abc".into())
        );
        assert_eq!(editor.text(), "abc");
    }

    #[test]
    fn all_occurrences() {
        let mut editor = Editor::new("let a = a + ab;");
        editor.set_cursor(4, false);
        select_all_occurrences(&mut editor);
        assert_eq!(editor.selection().len(), 2);
    }

    #[test]
    fn escape_with_one_caret_goes_to_the_window() {
        let mut editor = Editor::new("abc");
        assert_eq!(
            run(&mut editor, &Command::Collapse, 10, false),
            Effect::Unhandled
        );
    }
}
