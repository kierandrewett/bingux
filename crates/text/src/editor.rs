//! The editing engine: a rope, its carets and selections, its undo history
//! and every command a person gives it. No drawing and no keys: the view
//! (`bingux_ui::text_editor`) turns keys and pointer presses into these
//! commands, and tests drive them directly.
//!
//! Positions are char indices. Every edit goes through [`Editor::apply`],
//! which replaces non-overlapping ranges, records one undo step and moves
//! the selection, so multiple carets work for every command at once.

use std::borrow::Cow;

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{Change, EditKind, History};
use crate::indent::Indent;
use crate::language::Language;
use crate::layout::LineLayout;
use crate::selection::{Range, Selection};

/// How the document is edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub indent: Indent,
    /// The width of a tab character in cells.
    pub tab_width: usize,
    /// Typing an opening bracket or quote adds its closer.
    pub auto_close: bool,
    /// A new line starts at the indentation of the one before.
    pub auto_indent: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            indent: Indent::Spaces(4),
            tab_width: 4,
            auto_close: true,
            auto_indent: true,
        }
    }
}

/// One replacement of the text `start..end` (chars, in the document as it
/// is before the edit) by `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Edit {
    pub fn new(start: usize, end: usize, text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            text: text.into(),
        }
    }

    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self::new(at, at, text)
    }

    pub fn delete(start: usize, end: usize) -> Self {
        Self::new(start, end, String::new())
    }

    fn delta(&self) -> isize {
        self.text.chars().count() as isize - (self.end - self.start) as isize
    }
}

/// Case for [`Editor::transform_case`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Title,
}

/// What [`Editor::copy`] put on the clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copied {
    pub text: String,
    /// Whole lines, copied with no selection: pasting puts them above the
    /// caret's line instead of at the caret.
    pub linewise: bool,
}

/// How many revisions the change log keeps for views that read it.
const CHANGE_LOG: usize = 512;

/// Bracket pairs, for matching and auto-closing.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
const QUOTES: [char; 3] = ['"', '\'', '`'];

/// How far bracket matching looks, in chars.
const BRACKET_SCAN: usize = 50_000;

/// A document being edited. See the [module](self).
#[derive(Clone)]
pub struct Editor {
    rope: Rope,
    selection: Selection,
    history: History,
    pub settings: Settings,
    language: Language,
    revision: u64,
    /// `(revision, first line it changed)`, oldest first.
    changes: Vec<(u64, usize)>,
    /// The text last copied as whole lines, to recognise it on paste.
    linewise: Option<String>,
    /// The row width in cells for moving up and down by rows.
    wrap: Option<usize>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new("")
    }
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Self::from_rope(Rope::from_str(text))
    }

    pub fn from_rope(rope: Rope) -> Self {
        Self {
            rope,
            selection: Selection::default(),
            history: History::new(),
            settings: Settings::default(),
            language: Language::plain(),
            revision: 0,
            changes: Vec::new(),
            linewise: None,
            wrap: None,
        }
    }

    // ---------------------------------------------------------------------
    // Reading
    // ---------------------------------------------------------------------

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
        self.bump(0);
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// Runs `edit` and, when it made an undo step, names the step `label`
    /// ("duplicate line"), so a menu can say "Undo duplicate line". A step
    /// that already has a name keeps it.
    pub fn label_edit<R>(&mut self, label: &'static str, edit: impl FnOnce(&mut Self) -> R) -> R {
        let before = self.history.stamp();
        let result = edit(self);
        if self.history.stamp() != before {
            self.history.label_last(label);
        }
        result
    }

    pub fn is_modified(&self) -> bool {
        self.history.is_modified()
    }

    /// A number that changes on every edit, language change and reload.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The first line changed since `revision`, or `None` when nothing was.
    /// A revision older than the log means "everything".
    pub fn changed_since(&self, revision: u64) -> Option<usize> {
        if revision >= self.revision {
            return None;
        }
        match self.changes.first() {
            Some((oldest, _)) if *oldest <= revision + 1 => self
                .changes
                .iter()
                .filter(|(changed, _)| *changed > revision)
                .map(|(_, line)| *line)
                .min(),
            _ => Some(0),
        }
    }

    /// Line `line` without its line break.
    pub fn line(&self, line: usize) -> Cow<'_, str> {
        if line >= self.rope.len_lines() {
            return Cow::Borrowed("");
        }
        let slice = self.rope.line(line);
        let text: Cow<'_, str> = slice.into();
        match text {
            Cow::Borrowed(text) => Cow::Borrowed(text.strip_suffix('\n').unwrap_or(text)),
            Cow::Owned(mut text) => {
                if text.ends_with('\n') {
                    text.pop();
                }
                Cow::Owned(text)
            }
        }
    }

    /// The number of chars in line `line`, without its line break.
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let len = slice.len_chars();
        if len > 0 && slice.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.rope.len_lines()))
    }

    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_len(line)
    }

    pub fn line_of(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.rope.len_chars()))
    }

    /// Line and column (in chars) of `pos`, from 0.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let line = self.line_of(pos);
        (line, pos.min(self.rope.len_chars()) - self.line_start(line))
    }

    pub fn char_at(&self, pos: usize) -> Option<char> {
        (pos < self.rope.len_chars()).then(|| self.rope.char(pos))
    }

    fn char_before(&self, pos: usize) -> Option<char> {
        (pos > 0 && pos <= self.rope.len_chars()).then(|| self.rope.char(pos - 1))
    }

    /// The selected text of each range, in order.
    pub fn selected_texts(&self) -> Vec<String> {
        self.selection
            .ranges()
            .iter()
            .map(|range| self.rope.slice(range.start()..range.end()).to_string())
            .collect()
    }

    /// The text of the primary selection, if it selects anything.
    pub fn primary_text(&self) -> Option<String> {
        let range = self.selection.primary();
        (!range.is_empty()).then(|| self.rope.slice(range.start()..range.end()).to_string())
    }

    /// The layout used for moving by rows.
    pub fn set_wrap(&mut self, wrap: Option<usize>) {
        self.wrap = wrap;
    }

    pub fn layout(&self, line: usize) -> LineLayout {
        self.layout_wrapped(line, self.wrap)
    }

    /// The layout of `line` at row width `wrap`, without copying a line
    /// longer than [`crate::layout::LONG_LINE`].
    pub fn layout_wrapped(&self, line: usize, wrap: Option<usize>) -> LineLayout {
        let len = self.line_len(line);
        if len > crate::layout::LONG_LINE {
            return LineLayout::long(len, wrap);
        }
        LineLayout::new(&self.line(line), self.settings.tab_width, wrap)
    }

    // ---------------------------------------------------------------------
    // Changing the text
    // ---------------------------------------------------------------------

    /// Replaces the ranges of `edits` (sorted by start, not overlapping, in
    /// the coordinates before the edit) and sets `selection`, which is in the
    /// coordinates after it. One undo step of `kind`.
    pub fn apply(&mut self, mut edits: Vec<Edit>, selection: Selection, kind: EditKind) {
        edits.retain(|edit| edit.start != edit.end || !edit.text.is_empty());
        edits.sort_by_key(|edit| (edit.start, edit.end));
        if edits.is_empty() {
            self.set_selection(selection);
            return;
        }
        let before = self.selection.clone();
        let len = self.rope.len_chars();
        let mut changes = Vec::with_capacity(edits.len());
        let first_line = self.line_of(edits[0].start.min(len));
        for edit in edits.iter().rev() {
            let start = edit.start.min(len);
            let end = edit.end.clamp(start, len);
            let removed = self.rope.slice(start..end).to_string();
            self.rope.remove(start..end);
            self.rope.insert(start, &edit.text);
            changes.push(Change {
                at: start,
                removed,
                inserted: edit.text.clone(),
            });
        }
        self.selection = selection.clamped(self.rope.len_chars());
        self.history
            .record(changes, before, self.selection.clone(), kind);
        self.bump(first_line);
    }

    /// Replaces the whole text, as one undo step (or, for a large text,
    /// forgetting the history, which would hold two copies of it).
    pub fn set_text(&mut self, text: &str) {
        let keep = self.selection.primary().head.min(text.chars().count());
        if self.rope.len_bytes() + text.len() > 8 << 20 {
            self.rope = Rope::from_str(text);
            self.selection = Selection::caret(keep);
            self.history.clear();
            self.bump(0);
            return;
        }
        let len = self.rope.len_chars();
        self.apply(
            vec![Edit::new(0, len, text)],
            Selection::caret(keep),
            EditKind::Other,
        );
    }

    /// Replaces the text without an undo step and marks it unmodified: a
    /// file just read.
    pub fn load(&mut self, rope: Rope) {
        self.rope = rope;
        self.selection = Selection::caret(0);
        self.history.clear();
        self.history.mark_saved();
        self.bump(0);
    }

    fn bump(&mut self, line: usize) {
        self.revision += 1;
        self.changes.push((self.revision, line));
        if self.changes.len() > CHANGE_LOG {
            self.changes.remove(0);
        }
    }

    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    /// The next edit is a new undo step.
    pub fn seal(&mut self) {
        self.history.seal();
    }

    pub fn set_selection(&mut self, selection: Selection) {
        let selection = selection.clamped(self.rope.len_chars());
        if selection != self.selection {
            self.history.seal();
            self.selection = selection;
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.history.pop_undo() else {
            return false;
        };
        for change in step.changes.iter().rev() {
            let end = change.at + change.inserted_chars();
            self.rope.remove(change.at..end.min(self.rope.len_chars()));
            self.rope.insert(change.at, &change.removed);
        }
        let line = step.first_change().map_or(0, |at| self.line_of(at));
        self.selection = step.before.clamped(self.rope.len_chars());
        self.history.undone(step);
        self.bump(line);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.history.pop_redo() else {
            return false;
        };
        for change in &step.changes {
            let end = change.at + change.removed_chars();
            self.rope.remove(change.at..end.min(self.rope.len_chars()));
            self.rope.insert(change.at, &change.inserted);
        }
        let line = step.first_change().map_or(0, |at| self.line_of(at));
        self.selection = step.after.clamped(self.rope.len_chars());
        self.history.redone(step);
        self.bump(line);
        true
    }

    /// Each range replaced by the text `f` gives for it, carets after the
    /// new text.
    fn replace_each(&mut self, kind: EditKind, mut f: impl FnMut(&Self, Range) -> Option<String>) {
        let mut edits = Vec::new();
        let mut carets = Vec::new();
        let mut shift: isize = 0;
        for range in self.selection.ranges() {
            let Some(text) = f(self, *range) else {
                let at = (range.head as isize + shift) as usize;
                carets.push(Range::caret(at));
                continue;
            };
            let edit = Edit::new(range.start(), range.end(), text);
            let caret = (range.start() as isize + shift) as usize + edit.text.chars().count();
            shift += edit.delta();
            carets.push(Range::caret(caret));
            edits.push(edit);
        }
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(carets, primary), kind);
    }

    /// Typing `text` at every caret, replacing what is selected. A single
    /// character gets the typing helpers: auto-closed brackets and quotes,
    /// typing over a closer that is already there, wrapping a selection in a
    /// pair, and outdenting a closing bracket typed on an empty line.
    pub fn type_text(&mut self, text: &str) {
        let mut chars = text.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(ch), None) => Some(ch),
            _ => None,
        };
        let Some(ch) = single else {
            self.replace_each(EditKind::Typing, |_, _| Some(text.to_owned()));
            return;
        };
        let mut edits = Vec::new();
        let mut ranges = Vec::new();
        let mut shift: isize = 0;
        for range in self.selection.ranges().to_vec() {
            let start = range.start();
            let end = range.end();
            let at = |pos: usize, shift: isize| (pos as isize + shift) as usize;
            let closer = closer_of(ch);
            // Wrap a selection.
            if !range.is_empty()
                && self.settings.auto_close
                && closer.is_some()
                && (ch != '\'' || !self.language.prose)
            {
                let closer = closer.unwrap_or(ch);
                edits.push(Edit::insert(start, ch.to_string()));
                edits.push(Edit::insert(end, closer.to_string()));
                ranges.push(Range::new(at(start + 1, shift), at(end + 1, shift)));
                shift += 2;
                continue;
            }
            // Type over a closer already there.
            if range.is_empty()
                && self.settings.auto_close
                && (is_closer(ch) || QUOTES.contains(&ch))
                && self.char_at(start) == Some(ch)
                && self.closer_was_paired(start, ch)
            {
                ranges.push(Range::caret(at(start + 1, shift)));
                continue;
            }
            // Outdent a closer typed on a line of indentation only.
            if range.is_empty() && is_closer(ch) && self.settings.auto_indent {
                let line = self.line_of(start);
                let line_start = self.line_start(line);
                let before: String = self.rope.slice(line_start..start).to_string();
                if !before.is_empty() && before.chars().all(|c| c == ' ' || c == '\t') {
                    let target = self.indent_of_opener(start, ch).unwrap_or_else(|| {
                        let width = self.settings.indent.width(self.settings.tab_width);
                        let cols = indent_cols(&before, self.settings.tab_width);
                        make_indent(
                            cols.saturating_sub(width),
                            self.settings.indent,
                            self.settings.tab_width,
                        )
                    });
                    let text = format!("{target}{ch}");
                    let caret = at(line_start, shift) + text.chars().count();
                    shift += text.chars().count() as isize - (start - line_start) as isize;
                    edits.push(Edit::new(line_start, start, text));
                    ranges.push(Range::caret(caret));
                    continue;
                }
            }
            // Auto-close an opener or a quote.
            if let Some(closer) = closer.filter(|_| range.is_empty() && self.settings.auto_close)
                && self.may_close(start, ch)
            {
                edits.push(Edit::insert(start, format!("{ch}{closer}")));
                ranges.push(Range::caret(at(start + 1, shift)));
                shift += 2;
                continue;
            }
            let edit = Edit::new(start, end, ch.to_string());
            ranges.push(Range::caret(at(start, shift) + 1));
            shift += edit.delta();
            edits.push(edit);
        }
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Typing);
    }

    /// Whether to add `ch`'s closer at `pos`: the next character is a space,
    /// the end of the line or a closer, and a quote does not follow a word
    /// (an apostrophe in "don't").
    fn may_close(&self, pos: usize, ch: char) -> bool {
        let next = self.char_at(pos);
        let next_ok = matches!(
            next,
            None | Some('\n') | Some(' ') | Some('\t') | Some(',') | Some(';') | Some(':')
        ) || next.is_some_and(is_closer);
        if !next_ok {
            return false;
        }
        if QUOTES.contains(&ch) {
            if self.language.prose && ch != '"' {
                return false;
            }
            if ch == '`' && self.language.prose && !self.language.is_markdown() {
                return false;
            }
            let before = self.char_before(pos);
            if before.is_some_and(|c| c.is_alphanumeric() || c == ch || c == '\\') {
                return false;
            }
        }
        true
    }

    /// A closer at `pos` is typed over only when it belongs to an opener on
    /// the same line, so typing `)` after a lone `)` adds another.
    fn closer_was_paired(&self, pos: usize, ch: char) -> bool {
        let line = self.line_of(pos);
        let line_start = self.line_start(line);
        let before: String = self.rope.slice(line_start..pos).to_string();
        match opener_of(ch) {
            Some(opener) => {
                let mut depth = 0isize;
                for c in before.chars() {
                    if c == opener {
                        depth += 1;
                    } else if c == ch {
                        depth -= 1;
                    }
                }
                depth > 0
            }
            None => before.chars().filter(|c| *c == ch).count() % 2 == 1,
        }
    }

    /// The indentation of the line that opens the bracket a closer `ch`
    /// typed at `pos` would close.
    fn indent_of_opener(&self, pos: usize, ch: char) -> Option<String> {
        let opener = opener_of(ch)?;
        let found = self.scan_bracket(pos, opener, ch, false)?;
        let line = self.line_of(found);
        let text = self.line(line);
        Some(
            text.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect(),
        )
    }

    /// Pasting: whole lines copied with no selection go above each caret's
    /// line; as many lines as there are carets go one to each caret.
    pub fn paste(&mut self, text: &str) {
        self.label_edit("paste", |editor| editor.paste_text(text));
    }

    fn paste_text(&mut self, text: &str) {
        let text = crate::encoding::normalise_line_endings(text);
        let all_carets = !self.selection.has_selection();
        if all_carets && self.linewise.as_deref() == Some(text.as_str()) {
            let mut edits = Vec::new();
            let mut ranges = Vec::new();
            let mut shift: isize = 0;
            let added = text.chars().count() as isize;
            let mut seen = std::collections::HashSet::new();
            for range in self.selection.ranges() {
                let line = self.line_of(range.head);
                if seen.insert(line) {
                    edits.push(Edit::insert(self.line_start(line), text.clone()));
                    shift += added;
                }
                ranges.push(Range::caret((range.head as isize + shift) as usize));
            }
            let primary = self.selection.primary_index();
            self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
            return;
        }
        let pieces: Vec<&str> = text.split('\n').collect();
        let count = self.selection.len();
        let trimmed: Vec<&str> = if pieces.last() == Some(&"") {
            pieces[..pieces.len() - 1].to_vec()
        } else {
            pieces.clone()
        };
        if count > 1 && trimmed.len() == count {
            let mut index = 0;
            self.replace_each(EditKind::Other, |_, _| {
                let piece = trimmed[index].to_owned();
                index += 1;
                Some(piece)
            });
            return;
        }
        self.replace_each(EditKind::Other, |_, _| Some(text.clone()));
    }

    /// What Copy puts on the clipboard: the selections, joined by new lines,
    /// or with nothing selected the whole lines of the carets.
    pub fn copy(&mut self) -> Option<Copied> {
        if self.selection.has_selection() {
            let texts: Vec<String> = self
                .selection
                .ranges()
                .iter()
                .filter(|range| !range.is_empty())
                .map(|range| self.rope.slice(range.start()..range.end()).to_string())
                .collect();
            self.linewise = None;
            return Some(Copied {
                text: texts.join("\n"),
                linewise: false,
            });
        }
        if self.rope.len_chars() == 0 {
            return None;
        }
        let mut text = String::new();
        for (first, last) in self.line_blocks() {
            for line in first..=last {
                text.push_str(&self.line(line));
                text.push('\n');
            }
        }
        self.linewise = Some(text.clone());
        Some(Copied {
            text,
            linewise: true,
        })
    }

    /// Copy, then remove what was copied: the selections, or the carets'
    /// whole lines.
    pub fn cut(&mut self) -> Option<Copied> {
        let copied = self.copy()?;
        if copied.linewise {
            self.delete_lines();
        } else {
            self.replace_each(EditKind::Other, |_, range| {
                (!range.is_empty()).then(String::new)
            });
        }
        Some(copied)
    }

    /// Enter: a new line at the indentation of this one, one level deeper
    /// after an opening bracket (or a `:` in Python), with the closer on a
    /// line of its own when the caret was between a pair.
    pub fn newline(&mut self) {
        let unit = self.settings.indent.unit();
        let mut edits = Vec::new();
        let mut ranges = Vec::new();
        let mut shift: isize = 0;
        for range in self.selection.ranges().to_vec() {
            let start = range.start();
            let end = range.end();
            let line = self.line_of(start);
            let line_start = self.line_start(line);
            let before: String = self.rope.slice(line_start..start).to_string();
            let indent: String = if self.settings.auto_indent {
                before
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect()
            } else {
                String::new()
            };
            let trimmed = before.trim_end();
            let opens = self.settings.auto_indent
                && (trimmed.ends_with(['{', '[', '('])
                    || (trimmed.ends_with(':')
                        && matches!(
                            self.language.id.as_str(),
                            "python" | "starlark" | "yaml" | "nim"
                        )));
            let next = self.char_at(end);
            let between = opens
                && trimmed
                    .chars()
                    .last()
                    .and_then(closer_of)
                    .is_some_and(|closer| next == Some(closer));
            // Leaving a line of indentation only leaves it empty.
            let edit_start = if !before.is_empty() && before.trim().is_empty() && range.is_empty() {
                line_start
            } else {
                start
            };
            let mut text = String::new();
            text.push('\n');
            text.push_str(&indent);
            if opens {
                text.push_str(&unit);
            }
            let caret_offset = text.chars().count();
            if between {
                text.push('\n');
                text.push_str(&indent);
            }
            let caret = (edit_start as isize + shift) as usize + caret_offset;
            let edit = Edit::new(edit_start, end, text);
            shift += edit.delta();
            edits.push(edit);
            ranges.push(Range::caret(caret));
        }
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
    }

    /// Backspace: the selection, else one character (a whole grapheme), an
    /// empty pair at once, or one level of space indentation.
    pub fn backspace(&mut self) {
        let width = self.settings.indent.width(self.settings.tab_width);
        let spaces = matches!(self.settings.indent, Indent::Spaces(_));
        self.delete_each(EditKind::Deleting, |editor, pos| {
            if pos == 0 {
                return None;
            }
            let line = editor.line_of(pos);
            let line_start = editor.line_start(line);
            if spaces && pos > line_start {
                let before: String = editor.rope.slice(line_start..pos).to_string();
                if before.chars().all(|c| c == ' ') {
                    let cols = before.chars().count();
                    let back = match cols % width {
                        0 => width,
                        rest => rest,
                    };
                    return Some((pos - back.min(cols), pos));
                }
            }
            let previous = editor.char_before(pos);
            let next = editor.char_at(pos);
            if let (Some(open), Some(close)) = (previous, next)
                && editor.settings.auto_close
                && (closer_of(open) == Some(close))
            {
                return Some((pos - 1, pos + 1));
            }
            Some((editor.prev_grapheme(pos), pos))
        });
    }

    /// Delete: the selection, else the character after the caret.
    pub fn delete_forward(&mut self) {
        self.delete_each(EditKind::Deleting, |editor, pos| {
            (pos < editor.rope.len_chars()).then(|| (pos, editor.next_grapheme(pos)))
        });
    }

    /// Ctrl+Backspace and Ctrl+Delete: to the word boundary.
    pub fn delete_word(&mut self, backward: bool) {
        self.delete_each(EditKind::Other, |editor, pos| {
            if backward {
                (pos > 0).then(|| (editor.word_boundary(pos, true), pos))
            } else {
                (pos < editor.rope.len_chars()).then(|| (pos, editor.word_boundary(pos, false)))
            }
        });
    }

    /// Ctrl+Shift+Backspace: to the start of the line.
    pub fn delete_to_line_start(&mut self) {
        self.delete_each(EditKind::Other, |editor, pos| {
            let start = editor.line_start(editor.line_of(pos));
            if pos == start {
                (pos > 0).then(|| (pos - 1, pos))
            } else {
                Some((start, pos))
            }
        });
    }

    /// Deletes each selection, or for a caret the range `f` gives.
    fn delete_each(&mut self, kind: EditKind, f: impl Fn(&Self, usize) -> Option<(usize, usize)>) {
        let mut edits: Vec<Edit> = Vec::new();
        for range in self.selection.ranges() {
            let (start, end) = if range.is_empty() {
                match f(self, range.head) {
                    Some(span) => span,
                    None => continue,
                }
            } else {
                (range.start(), range.end())
            };
            // Two carets deleting into each other delete once.
            if let Some(last) = edits.last_mut()
                && start < last.end
            {
                last.end = last.end.max(end);
                continue;
            }
            edits.push(Edit::delete(start, end));
        }
        let selection = self.selection_after_deletes(&edits);
        self.apply(edits, selection, kind);
    }

    fn selection_after_deletes(&self, edits: &[Edit]) -> Selection {
        let map = |pos: usize| map_through(edits, pos);
        let ranges = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                let deleted = edits.iter().find(|edit| {
                    edit.start <= range.start() && range.end() <= edit.end.max(edit.start)
                });
                match deleted {
                    Some(edit) => Range::caret(map(edit.start)),
                    None => Range::caret(map(range.head)),
                }
            })
            .collect();
        Selection::new(ranges, self.selection.primary_index())
    }

    /// The groups of whole lines the ranges cover, merged: `(first, last)`.
    /// A selection that ends at the start of a line does not cover it.
    pub fn line_blocks(&self) -> Vec<(usize, usize)> {
        let mut blocks: Vec<(usize, usize)> = Vec::new();
        for range in self.selection.ranges() {
            let first = self.line_of(range.start());
            let mut last = self.line_of(range.end());
            if !range.is_empty() && last > first && self.line_start(last) == range.end() {
                last -= 1;
            }
            match blocks.last_mut() {
                Some(block) if first <= block.1 + 1 && first >= block.0 => {
                    block.1 = block.1.max(last)
                }
                _ => blocks.push((first, last)),
            }
        }
        blocks
    }

    /// Ctrl+D: each caret's line is copied below it, and the caret goes with
    /// the copy; a selection is copied after itself and the copy selected.
    pub fn duplicate(&mut self) {
        if self.selection.has_selection()
            && self
                .selection
                .ranges()
                .iter()
                .all(|range| self.line_of(range.start()) == self.line_of(range.end()))
        {
            let mut edits = Vec::new();
            let mut ranges = Vec::new();
            let mut shift: isize = 0;
            for range in self.selection.ranges() {
                let text = self.rope.slice(range.start()..range.end()).to_string();
                let len = text.chars().count();
                edits.push(Edit::insert(range.end(), text));
                let start = (range.end() as isize + shift) as usize;
                ranges.push(Range::new(start, start + len));
                shift += len as isize;
            }
            let primary = self.selection.primary_index();
            self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
            return;
        }
        let mut edits = Vec::new();
        let mut shift: isize = 0;
        let mut moves: Vec<((usize, usize), isize)> = Vec::new();
        for (first, last) in self.line_blocks() {
            let start = self.line_start(first);
            let end = self.line_end(last);
            let block = self.rope.slice(start..end).to_string();
            let text = format!("\n{block}");
            let added = text.chars().count() as isize;
            edits.push(Edit::insert(end, text));
            shift += added;
            moves.push(((start, end), shift));
        }
        let ranges = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                let offset = moves
                    .iter()
                    .find(|((start, end), _)| *start <= range.start() && range.end() <= *end + 1)
                    .map_or(0, |(_, shift)| *shift);
                Range::new(
                    (range.anchor as isize + offset) as usize,
                    (range.head as isize + offset) as usize,
                )
            })
            .collect();
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
    }

    /// Ctrl+Shift+K: removes the carets' lines.
    pub fn delete_lines(&mut self) {
        let mut edits = Vec::new();
        let mut carets = Vec::new();
        let last_line = self.rope.len_lines() - 1;
        for (first, last) in self.line_blocks() {
            let (start, end) = if last < last_line {
                (self.line_start(first), self.line_start(last + 1))
            } else if first > 0 {
                (self.line_end(first - 1), self.rope.len_chars())
            } else {
                (0, self.rope.len_chars())
            };
            edits.push(Edit::delete(start, end));
            carets.push(start);
        }
        let ranges = carets
            .iter()
            .map(|at| Range::caret(map_through(&edits, *at)))
            .collect();
        self.apply(edits, Selection::new(ranges, 0), EditKind::Other);
        // The caret lands on the start of the line that moved up.
    }

    /// Alt+Up and Alt+Down: the carets' lines swap with the line above or
    /// below them, and the carets go with them.
    pub fn move_lines(&mut self, up: bool) {
        let blocks = self.line_blocks();
        let lines = self.rope.len_lines();
        let trailing_empty = lines > 1 && self.line_len(lines - 1) == 0;
        let last_movable = if trailing_empty { lines - 2 } else { lines - 1 };
        if up && blocks.first().is_some_and(|block| block.0 == 0) {
            return;
        }
        if !up && blocks.last().is_some_and(|block| block.1 >= last_movable) {
            return;
        }
        let mut edits = Vec::new();
        let mut moves: Vec<((usize, usize), isize)> = Vec::new();
        for (first, last) in blocks {
            let block: Vec<String> = (first..=last)
                .map(|line| self.line(line).into_owned())
                .collect();
            let (start, end, text, offset) = if up {
                let above = self.line(first - 1).into_owned();
                let mut lines = block.clone();
                lines.push(above.clone());
                (
                    self.line_start(first - 1),
                    self.line_end(last),
                    lines.join("\n"),
                    -((above.chars().count() + 1) as isize),
                )
            } else {
                let below = self.line(last + 1).into_owned();
                let mut lines = vec![below.clone()];
                lines.extend(block.clone());
                (
                    self.line_start(first),
                    self.line_end(last + 1),
                    lines.join("\n"),
                    (below.chars().count() + 1) as isize,
                )
            };
            moves.push(((self.line_start(first), self.line_end(last)), offset));
            edits.push(Edit::new(start, end, text));
        }
        let ranges = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                let offset = moves
                    .iter()
                    .find(|((start, end), _)| *start <= range.start() && range.end() <= *end + 1)
                    .map_or(0, |(_, offset)| *offset);
                Range::new(
                    (range.anchor as isize + offset) as usize,
                    (range.head as isize + offset) as usize,
                )
            })
            .collect();
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
    }

    /// Tab: with a selection over lines, every line goes one level in;
    /// otherwise one level of indentation at each caret (spaces up to the
    /// next stop when indenting with spaces).
    pub fn indent(&mut self) {
        let multi_line = self
            .selection
            .ranges()
            .iter()
            .any(|range| self.line_of(range.start()) != self.line_of(range.end()));
        if multi_line {
            let unit = self.settings.indent.unit();
            self.edit_line_starts(|_, line| (!line.trim().is_empty()).then(|| (0, unit.clone())));
            return;
        }
        let indent = self.settings.indent;
        self.replace_each(EditKind::Typing, |editor, range| {
            Some(match indent {
                Indent::Tabs => "\t".into(),
                Indent::Spaces(width) => {
                    let line = editor.line_of(range.start());
                    let col = editor
                        .layout(line)
                        .col(range.start() - editor.line_start(line));
                    let width = width.max(1) as usize;
                    " ".repeat(width - col % width)
                }
            })
        });
    }

    /// Shift+Tab: every line of the selection one level out.
    pub fn outdent(&mut self) {
        let width = self.settings.indent.width(self.settings.tab_width);
        self.edit_line_starts(|_, line| {
            let mut removed = 0;
            let mut cols = 0;
            for ch in line.chars() {
                match ch {
                    ' ' if cols < width => {
                        removed += 1;
                        cols += 1;
                    }
                    '\t' if cols == 0 => {
                        removed += 1;
                        break;
                    }
                    _ => break,
                }
            }
            (removed > 0).then(|| (removed, String::new()))
        });
    }

    /// For each line of the line blocks, `f` returns how many chars at the
    /// line's start to replace and with what. Selections keep covering the
    /// same text.
    fn edit_line_starts(&mut self, f: impl Fn(usize, &str) -> Option<(usize, String)>) {
        let mut edits = Vec::new();
        for (first, last) in self.line_blocks() {
            for line in first..=last {
                let text = self.line(line);
                if let Some((remove, insert)) = f(line, &text) {
                    let start = self.line_start(line);
                    edits.push(Edit::new(start, start + remove, insert));
                }
            }
        }
        let ranges = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                let anchor = map_line_start(&edits, range.anchor);
                let head = map_line_start(&edits, range.head);
                Range::new(anchor, head)
            })
            .collect();
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
    }

    /// Ctrl+/: comments the carets' lines out, or back in when they all are.
    pub fn toggle_comment(&mut self) {
        let language = self.language.clone();
        if let Some(token) = language.line_comment {
            let blocks = self.line_blocks();
            let lines: Vec<usize> = blocks
                .iter()
                .flat_map(|(first, last)| *first..=*last)
                .collect();
            let texts: Vec<String> = lines
                .iter()
                .map(|line| self.line(*line).into_owned())
                .collect();
            let non_blank: Vec<&String> = texts
                .iter()
                .filter(|text| !text.trim().is_empty())
                .collect();
            if non_blank.is_empty() {
                return;
            }
            let commented = non_blank
                .iter()
                .all(|text| text.trim_start().starts_with(token));
            let column = non_blank
                .iter()
                .map(|text| text.chars().take_while(|c| *c == ' ' || *c == '\t').count())
                .min()
                .unwrap_or(0);
            self.edit_line_starts(|_, text| {
                if text.trim().is_empty() {
                    return None;
                }
                let indent: String = text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let lead = indent.chars().count();
                if commented {
                    let rest = &text[indent.len()..];
                    let after = rest.strip_prefix(token).unwrap_or(rest);
                    let space = usize::from(after.starts_with(' '));
                    Some((lead + token.chars().count() + space, indent))
                } else {
                    let head: String = text.chars().take(column).collect();
                    Some((column, format!("{head}{token} ")))
                }
            });
            return;
        }
        if let Some((open, close)) = language.block_comment {
            let mut edits = Vec::new();
            let mut ranges = Vec::new();
            let mut shift: isize = 0;
            for range in self.selection.ranges().to_vec() {
                let (start, end) = if range.is_empty() {
                    let line = self.line_of(range.head);
                    let text = self.line(line);
                    let lead = text.chars().take_while(|c| c.is_whitespace()).count();
                    (self.line_start(line) + lead, self.line_end(line))
                } else {
                    (range.start(), range.end())
                };
                let inner = self.rope.slice(start..end).to_string();
                let trimmed = inner.trim();
                let text = if trimmed.starts_with(open)
                    && trimmed.ends_with(close)
                    && trimmed.len() >= open.len() + close.len()
                {
                    let body = &trimmed[open.len()..trimmed.len() - close.len()];
                    body.strip_prefix(' ')
                        .unwrap_or(body)
                        .strip_suffix(' ')
                        .unwrap_or(body.strip_prefix(' ').unwrap_or(body))
                        .to_owned()
                } else {
                    format!("{open} {inner} {close}")
                };
                let len = text.chars().count();
                let new_start = (start as isize + shift) as usize;
                shift += len as isize - (end - start) as isize;
                edits.push(Edit::new(start, end, text));
                ranges.push(if range.is_empty() {
                    Range::caret(new_start + len)
                } else {
                    Range::new(new_start, new_start + len)
                });
            }
            let primary = self.selection.primary_index();
            self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
        }
    }

    /// Changes the case of each selection, or of the word at each caret.
    pub fn transform_case(&mut self, case: Case) {
        let mut edits = Vec::new();
        let mut ranges = Vec::new();
        for range in self.selection.ranges().to_vec() {
            let (start, end) = if range.is_empty() {
                self.word_at(range.head)
            } else {
                (range.start(), range.end())
            };
            let text = self.rope.slice(start..end).to_string();
            let changed = match case {
                Case::Upper => text.to_uppercase(),
                Case::Lower => text.to_lowercase(),
                Case::Title => title_case(&text),
            };
            if changed.chars().count() != text.chars().count() {
                // Rare (ß to SS): keep it simple and select the result.
            }
            let len = changed.chars().count();
            edits.push(Edit::new(start, end, changed));
            ranges.push((start, len, range));
        }
        let mut shift: isize = 0;
        let mut out = Vec::new();
        for (edit, (start, len, range)) in edits.iter().zip(ranges) {
            let new_start = (start as isize + shift) as usize;
            out.push(if range.is_empty() {
                Range::caret((range.head as isize + shift) as usize)
            } else {
                Range::new(new_start, new_start + len)
            });
            shift += edit.delta();
        }
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(out, primary), EditKind::Other);
    }

    /// Sorts the selected lines, ignoring case, numbers in order.
    pub fn sort_lines(&mut self) {
        let blocks = self.line_blocks();
        let mut edits = Vec::new();
        for (first, last) in &blocks {
            if first == last {
                continue;
            }
            let mut lines: Vec<String> = (*first..=*last)
                .map(|line| self.line(line).into_owned())
                .collect();
            lines.sort_by(|a, b| natural_cmp(a, b));
            edits.push(Edit::new(
                self.line_start(*first),
                self.line_end(*last),
                lines.join("\n"),
            ));
        }
        let selection = self.selection.clone();
        self.apply(edits, selection, EditKind::Other);
    }

    /// Ctrl+J: the next line joins this one with one space between.
    pub fn join_lines(&mut self) {
        let mut edits = Vec::new();
        let last_line = self.rope.len_lines() - 1;
        for (first, last) in self.line_blocks() {
            let last = if first == last { last + 1 } else { last };
            for line in first..last.min(last_line) {
                let end = self.line_end(line);
                let next = self.line(line + 1);
                let lead = next.chars().take_while(|c| c.is_whitespace()).count();
                let before_trailing = self.line(line).trim_end().chars().count();
                let start = self.line_start(line) + before_trailing;
                let joiner = if next.trim().is_empty() || before_trailing == 0 {
                    ""
                } else {
                    " "
                };
                edits.push(Edit::new(start, end + 1 + lead, joiner));
            }
        }
        let carets: Vec<Range> = edits
            .iter()
            .map(|edit| Range::caret(map_through(&edits, edit.start) + edit.text.chars().count()))
            .collect();
        let selection = if carets.is_empty() {
            self.selection.clone()
        } else {
            Selection::new(carets, 0)
        };
        self.apply(edits, selection, EditKind::Other);
    }

    /// Removes spaces and tabs at the ends of lines. Returns whether
    /// anything was removed.
    pub fn trim_trailing_whitespace(&mut self) -> bool {
        let mut edits = Vec::new();
        for line in 0..self.rope.len_lines() {
            let text = self.line(line);
            let trimmed = text.trim_end_matches([' ', '\t']).chars().count();
            let len = text.chars().count();
            if trimmed < len {
                let start = self.line_start(line);
                edits.push(Edit::delete(start + trimmed, start + len));
            }
        }
        if edits.is_empty() {
            return false;
        }
        let ranges = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                Range::new(
                    map_through(&edits, range.anchor),
                    map_through(&edits, range.head),
                )
            })
            .collect();
        let primary = self.selection.primary_index();
        self.apply(edits, Selection::new(ranges, primary), EditKind::Other);
        true
    }

    /// Adds a line break at the end when the text does not end with one.
    pub fn ensure_final_newline(&mut self) -> bool {
        let len = self.rope.len_chars();
        if len == 0 || self.rope.char(len - 1) == '\n' {
            return false;
        }
        let selection = self.selection.clone();
        self.apply(vec![Edit::insert(len, "\n")], selection, EditKind::Other);
        true
    }

    /// Replaces each of `ranges` with its text: Replace All, one undo step.
    /// The caret goes to the end of the last replacement.
    pub fn replace_ranges(&mut self, replacements: Vec<((usize, usize), String)>) {
        if replacements.is_empty() {
            return;
        }
        let edits: Vec<Edit> = replacements
            .into_iter()
            .map(|((start, end), text)| Edit::new(start, end, text))
            .collect();
        let last = edits
            .last()
            .map(|edit| map_through(&edits, edit.start) + edit.text.chars().count())
            .unwrap_or(0);
        self.label_edit("replace", |editor| {
            editor.apply(edits, Selection::caret(last), EditKind::Other)
        });
    }

    /// Text dropped at `at` (from another program, or copied from this
    /// document): inserted, and selected, as one undo step.
    pub fn drop_text(&mut self, at: usize, text: &str) {
        let text = crate::encoding::normalise_line_endings(text);
        if text.is_empty() {
            return;
        }
        let at = at.min(self.rope.len_chars());
        let selection = Selection::single(Range::new(at, at + text.chars().count()));
        self.label_edit("drop text", |editor| {
            editor.apply(vec![Edit::insert(at, text)], selection, EditKind::Other)
        });
    }

    /// Moves the text of `source` (char indices) to `to`, as one undo step,
    /// and selects it there. Returns false, changing nothing, when `to` is
    /// inside the source: dropping text on itself.
    pub fn move_text(&mut self, source: (usize, usize), to: usize) -> bool {
        let len = self.rope.len_chars();
        let (start, end) = (source.0.min(len), source.1.min(len));
        if start >= end || (start..=end).contains(&to) {
            return false;
        }
        let text = self.rope.slice(start..end).to_string();
        let count = end - start;
        let landed = if to > end { to - count } else { to };
        let selection = Selection::single(Range::new(landed, landed + count));
        self.label_edit("move text", |editor| {
            editor.apply(
                vec![Edit::delete(start, end), Edit::insert(to, text)],
                selection,
                EditKind::Other,
            )
        });
        true
    }

    /// Removes `range` if it still holds `text`: the source of a drag that
    /// another program took as a move. A document edited since the drag
    /// began is left alone.
    pub fn remove_if_unchanged(&mut self, range: (usize, usize), text: &str) -> bool {
        let len = self.rope.len_chars();
        let (start, end) = range;
        if start >= end || end > len || self.rope.slice(start..end) != text {
            return false;
        }
        let selection = Selection::caret(start);
        self.label_edit("move text", |editor| {
            editor.apply(vec![Edit::delete(start, end)], selection, EditKind::Other)
        });
        true
    }

    // ---------------------------------------------------------------------
    // Moving
    // ---------------------------------------------------------------------

    fn prev_grapheme(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let line = self.line_of(pos);
        let start = self.line_start(line);
        if pos == start {
            return pos - 1;
        }
        let text = self.rope.slice(start..pos).to_string();
        let last = text
            .graphemes(true)
            .next_back()
            .map_or(1, |g| g.chars().count());
        pos - last
    }

    fn next_grapheme(&self, pos: usize) -> usize {
        let len = self.rope.len_chars();
        if pos >= len {
            return len;
        }
        let line = self.line_of(pos);
        let end = self.line_end(line);
        if pos >= end {
            return pos + 1;
        }
        let text = self.rope.slice(pos..end).to_string();
        let first = text.graphemes(true).next().map_or(1, |g| g.chars().count());
        pos + first
    }

    /// The next (or, `backward`, previous) word boundary from `pos`: over
    /// spaces, then over one run of word characters or of punctuation. A
    /// line break is a stop of its own.
    pub fn word_boundary(&self, pos: usize, backward: bool) -> usize {
        let len = self.rope.len_chars();
        let class = |ch: char| {
            if ch == '\n' {
                0
            } else if ch.is_whitespace() {
                1
            } else if ch.is_alphanumeric() || ch == '_' {
                2
            } else {
                3
            }
        };
        let mut at = pos;
        if backward {
            if at == 0 {
                return 0;
            }
            if self.rope.char(at - 1) == '\n' {
                return at - 1;
            }
            while at > 0 && class(self.rope.char(at - 1)) == 1 {
                at -= 1;
            }
            if at > 0 && self.rope.char(at - 1) != '\n' {
                let kind = class(self.rope.char(at - 1));
                while at > 0 && class(self.rope.char(at - 1)) == kind {
                    at -= 1;
                }
            }
            at
        } else {
            if at >= len {
                return len;
            }
            if self.rope.char(at) == '\n' {
                return at + 1;
            }
            while at < len && class(self.rope.char(at)) == 1 {
                at += 1;
            }
            if at < len && self.rope.char(at) != '\n' {
                let kind = class(self.rope.char(at));
                while at < len && class(self.rope.char(at)) == kind {
                    at += 1;
                }
            }
            at
        }
    }

    /// The word around `pos`: letters, digits and underscores, or a run of
    /// punctuation, or of spaces.
    pub fn word_at(&self, pos: usize) -> (usize, usize) {
        let len = self.rope.len_chars();
        let kind = |ch: char| {
            if ch.is_alphanumeric() || ch == '_' {
                2
            } else if ch == '\n' {
                0
            } else if ch.is_whitespace() {
                1
            } else {
                3
            }
        };
        let probe = if pos < len && self.rope.char(pos) != '\n' {
            pos
        } else if pos > 0 {
            pos - 1
        } else {
            return (pos, pos);
        };
        let target = kind(self.rope.char(probe));
        if target == 0 {
            return (pos, pos);
        }
        // Prefer a word to the punctuation or space beside it.
        let probe =
            if target != 2 && probe + 1 == pos && pos < len && kind(self.rope.char(pos)) == 2 {
                pos
            } else {
                probe
            };
        let target = kind(self.rope.char(probe));
        let mut start = probe;
        while start > 0 && kind(self.rope.char(start - 1)) == target {
            start -= 1;
        }
        let mut end = probe + 1;
        while end < len && kind(self.rope.char(end)) == target {
            end += 1;
        }
        (start, end)
    }

    fn move_each(&mut self, extend: bool, f: impl Fn(&Self, Range) -> usize) {
        let selection = self
            .selection
            .map(|range| range.moved(f(self, range), extend));
        self.set_selection(selection);
    }

    /// Left and Right. Without `extend`, a selection collapses to its edge.
    pub fn move_horizontal(&mut self, forward: bool, word: bool, extend: bool) {
        self.move_each(extend, |editor, range| {
            if !extend && !range.is_empty() && !word {
                return if forward { range.end() } else { range.start() };
            }
            match (forward, word) {
                (true, false) => editor.next_grapheme(range.head),
                (false, false) => editor.prev_grapheme(range.head),
                (true, true) => editor.word_boundary(range.head, false),
                (false, true) => editor.word_boundary(range.head, true),
            }
        });
    }

    /// Up and Down by `rows` visual rows, keeping the column the move
    /// started from.
    pub fn move_vertical(&mut self, rows: isize, extend: bool) {
        let ranges: Vec<Range> = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                let (head, goal) = self.vertical_target(range.head, range.goal, rows);
                let mut moved = range.moved(head, extend);
                moved.goal = Some(goal);
                moved
            })
            .collect();
        let selection = Selection::new(ranges, self.selection.primary_index());
        self.set_selection(selection);
    }

    /// Where moving `rows` rows from `pos` lands, and the goal column.
    pub fn vertical_target(&self, pos: usize, goal: Option<usize>, rows: isize) -> (usize, usize) {
        let mut line = self.line_of(pos);
        let mut layout = self.layout(line);
        let (mut row, col) = layout.col_in_row(pos - self.line_start(line));
        let goal = goal.unwrap_or(col);
        let mut remaining = rows;
        while remaining != 0 {
            if remaining > 0 {
                if row + 1 < layout.rows.len() {
                    row += 1;
                } else if line + 1 < self.rope.len_lines() {
                    line += 1;
                    layout = self.layout(line);
                    row = 0;
                } else {
                    return (self.rope.len_chars(), goal);
                }
                remaining -= 1;
            } else {
                if row > 0 {
                    row -= 1;
                } else if line > 0 {
                    line -= 1;
                    layout = self.layout(line);
                    row = layout.rows.len() - 1;
                } else {
                    return (0, goal);
                }
                remaining += 1;
            }
        }
        (self.line_start(line) + layout.index_at(row, goal), goal)
    }

    /// Home: the first character after the indentation, then the start of
    /// the line. On a wrapped row, the row's start first.
    pub fn home(&mut self, extend: bool) {
        self.move_each(extend, |editor, range| {
            let line = editor.line_of(range.head);
            let start = editor.line_start(line);
            let layout = editor.layout(line);
            let (row, _) = layout.col_in_row(range.head - start);
            if row > 0 {
                let row_start = start + layout.rows[row].start;
                if range.head != row_start {
                    return row_start;
                }
            }
            let text = editor.line(line);
            let first = start + text.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if range.head == first { start } else { first }
        });
    }

    /// End: the end of the row, then of the line.
    pub fn end(&mut self, extend: bool) {
        self.move_each(extend, |editor, range| {
            let line = editor.line_of(range.head);
            let start = editor.line_start(line);
            let layout = editor.layout(line);
            let (row, _) = layout.col_in_row(range.head - start);
            let end = editor.line_end(line);
            if row + 1 < layout.rows.len() {
                let row_end = start + layout.rows[row].end - 1;
                if range.head != row_end {
                    return row_end;
                }
            }
            end
        });
    }

    pub fn document_start(&mut self, extend: bool) {
        self.move_each(extend, |_, _| 0);
    }

    pub fn document_end(&mut self, extend: bool) {
        let len = self.rope.len_chars();
        self.move_each(extend, move |_, _| len);
    }

    /// Puts a single caret at `pos`, or with `extend` moves the primary
    /// range's head there.
    pub fn set_cursor(&mut self, pos: usize, extend: bool) {
        let pos = pos.min(self.rope.len_chars());
        let selection = if extend {
            Selection::single(self.selection.primary().moved(pos, true))
        } else {
            Selection::caret(pos)
        };
        self.set_selection(selection);
    }

    pub fn select_all(&mut self) {
        self.set_selection(Selection::single(Range::new(0, self.rope.len_chars())));
    }

    /// Ctrl+L: selects the carets' whole lines; again, the next line too.
    pub fn select_line(&mut self) {
        let ranges: Vec<Range> = self
            .selection
            .ranges()
            .iter()
            .map(|range| {
                // A selection of whole lines ends at the start of the line
                // after them, so taking that line's end takes one more.
                let first = self.line_of(range.start());
                let last = self.line_of(range.end());
                let end = if last + 1 < self.rope.len_lines() {
                    self.line_start(last + 1)
                } else {
                    self.rope.len_chars()
                };
                Range::new(self.line_start(first), end)
            })
            .collect();
        let selection = Selection::new(ranges, self.selection.primary_index());
        self.set_selection(selection);
    }

    pub fn select_word_at(&mut self, pos: usize, add: bool) {
        let (start, end) = self.word_at(pos);
        let range = Range::new(start, end);
        let selection = if add {
            self.selection.with(range)
        } else {
            Selection::single(range)
        };
        self.set_selection(selection);
    }

    pub fn select_line_at(&mut self, pos: usize) {
        let line = self.line_of(pos);
        let start = self.line_start(line);
        let end = if line + 1 < self.rope.len_lines() {
            self.line_start(line + 1)
        } else {
            self.rope.len_chars()
        };
        self.set_selection(Selection::single(Range::new(start, end)));
    }

    /// Alt+click: a caret at `pos` added to the others, or the caret there
    /// taken away.
    pub fn toggle_cursor_at(&mut self, pos: usize) {
        if let Some(fewer) = self.selection.without_at(pos) {
            self.set_selection(fewer);
            return;
        }
        let selection = self
            .selection
            .with(Range::caret(pos.min(self.rope.len_chars())));
        self.set_selection(selection);
    }

    /// Ctrl+Alt+Up and Down: a caret on the row above or below the
    /// outermost one, at the same column.
    pub fn add_cursor_vertical(&mut self, up: bool) {
        let ranges = self.selection.ranges();
        let edge = if up {
            ranges[0]
        } else {
            ranges[ranges.len() - 1]
        };
        let (head, goal) = self.vertical_target(edge.head, edge.goal, if up { -1 } else { 1 });
        if head == edge.head {
            return;
        }
        let mut caret = Range::caret(head);
        caret.goal = Some(goal);
        let selection = self.selection.with(caret);
        self.set_selection(selection);
    }

    /// Ctrl+Shift+L or Alt+Enter in find: a selection on every one of
    /// `ranges`.
    pub fn select_ranges(&mut self, ranges: &[(usize, usize)], primary: usize) {
        if ranges.is_empty() {
            return;
        }
        let ranges = ranges
            .iter()
            .map(|(start, end)| Range::new(*start, *end))
            .collect();
        self.set_selection(Selection::new(ranges, primary));
    }

    /// Ctrl+Shift+D: the next occurrence of the primary selection (the
    /// word at the caret, first) is added as a selection. Returns whether
    /// one was found.
    pub fn add_next_occurrence(&mut self) -> bool {
        let primary = self.selection.primary();
        if primary.is_empty() {
            let (start, end) = self.word_at(primary.head);
            if start == end {
                return false;
            }
            let ranges: Vec<Range> = self
                .selection
                .ranges()
                .iter()
                .map(|range| {
                    if *range == primary {
                        Range::new(start, end)
                    } else {
                        *range
                    }
                })
                .collect();
            let selection = Selection::new(ranges, self.selection.primary_index());
            self.set_selection(selection);
            return true;
        }
        let needle = self.rope.slice(primary.start()..primary.end()).to_string();
        let len = needle.chars().count();
        let last = self
            .selection
            .ranges()
            .iter()
            .map(|range| range.end())
            .max()
            .unwrap_or(0);
        let haystack = self.rope.to_string();
        let found =
            char_find(&haystack, &needle, last).or_else(|| char_find(&haystack, &needle, 0));
        let Some(start) = found else { return false };
        if self
            .selection
            .ranges()
            .iter()
            .any(|range| range.start() == start)
        {
            return false;
        }
        let selection = self.selection.with(Range::new(start, start + len));
        self.set_selection(selection);
        true
    }

    /// Escape: back to one caret, the primary one.
    pub fn collapse(&mut self) -> bool {
        if self.selection.is_multi() {
            let selection = self.selection.only_primary();
            self.set_selection(selection);
            return true;
        }
        let primary = self.selection.primary();
        if !primary.is_empty() {
            self.set_selection(Selection::caret(primary.head));
            return true;
        }
        false
    }

    /// Go to line `line` (from 1) and column `col` (from 1, optional).
    pub fn go_to(&mut self, line: usize, col: Option<usize>) {
        let line = line.clamp(1, self.rope.len_lines()) - 1;
        let col = col
            .map_or(0, |col| col.saturating_sub(1))
            .min(self.line_len(line));
        let pos = self.line_start(line) + col;
        self.set_selection(Selection::caret(pos));
    }

    /// The bracket that matches the one at or just before `pos`, as
    /// `(this bracket, its match)`.
    pub fn matching_bracket(&self, pos: usize) -> Option<(usize, usize)> {
        let candidates = [Some(pos), pos.checked_sub(1)];
        for at in candidates.into_iter().flatten() {
            let Some(ch) = self.char_at(at) else { continue };
            if let Some(closer) = PAIRS
                .iter()
                .find(|(open, _)| *open == ch)
                .map(|(_, close)| *close)
            {
                return self
                    .scan_bracket(at + 1, ch, closer, true)
                    .map(|found| (at, found));
            }
            if let Some(opener) = opener_of(ch) {
                return self
                    .scan_bracket(at, opener, ch, false)
                    .map(|found| (at, found));
            }
        }
        None
    }

    /// The unmatched `open` before `from` (`forward` false) or the unmatched
    /// `close` from `from` on (`forward` true).
    fn scan_bracket(&self, from: usize, open: char, close: char, forward: bool) -> Option<usize> {
        let mut depth = 0usize;
        if forward {
            let end = (from + BRACKET_SCAN).min(self.rope.len_chars());
            for (offset, ch) in self.rope.slice(from..end).chars().enumerate() {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    if depth == 0 {
                        return Some(from + offset);
                    }
                    depth -= 1;
                }
            }
        } else {
            let start = from.saturating_sub(BRACKET_SCAN);
            let slice = self.rope.slice(start..from);
            let mut at = from;
            for ch in slice.chars_at(slice.len_chars()).reversed() {
                at -= 1;
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    if depth == 0 {
                        return Some(at);
                    }
                    depth -= 1;
                }
            }
        }
        None
    }
}

/// `pos` (before `edits`) mapped to after them; a position inside a
/// replaced range goes to the end of its replacement.
pub fn map_through(edits: &[Edit], pos: usize) -> usize {
    let mut shift: isize = 0;
    for edit in edits {
        if edit.end <= pos && !(edit.start == edit.end && edit.start == pos) {
            shift += edit.delta();
        } else if edit.start < pos {
            return (edit.start as isize + shift) as usize + edit.text.chars().count();
        } else {
            break;
        }
    }
    (pos as isize + shift).max(0) as usize
}

/// Like [`map_through`], but an insertion at the start of a line moves a
/// position at that start with it, so a caret at column 0 keeps its place
/// in the text when the line is indented.
fn map_line_start(edits: &[Edit], pos: usize) -> usize {
    let mut shift: isize = 0;
    for edit in edits {
        if edit.end <= pos {
            shift += edit.delta();
        } else if edit.start < pos {
            return (edit.start as isize + shift) as usize
                + edit.text.chars().count().min(pos - edit.start);
        } else {
            break;
        }
    }
    (pos as isize + shift).max(0) as usize
}

fn closer_of(ch: char) -> Option<char> {
    PAIRS
        .iter()
        .find(|(open, _)| *open == ch)
        .map(|(_, close)| *close)
        .or_else(|| QUOTES.contains(&ch).then_some(ch))
}

fn opener_of(ch: char) -> Option<char> {
    PAIRS
        .iter()
        .find(|(_, close)| *close == ch)
        .map(|(open, _)| *open)
}

fn is_closer(ch: char) -> bool {
    opener_of(ch).is_some()
}

fn indent_cols(text: &str, tab_width: usize) -> usize {
    let mut cols = 0;
    for ch in text.chars() {
        match ch {
            '\t' => cols += tab_width - cols % tab_width.max(1),
            ' ' => cols += 1,
            _ => break,
        }
    }
    cols
}

fn make_indent(cols: usize, indent: Indent, tab_width: usize) -> String {
    match indent {
        Indent::Tabs => {
            let tabs = cols / tab_width.max(1);
            format!(
                "{}{}",
                "\t".repeat(tabs),
                " ".repeat(cols - tabs * tab_width.max(1))
            )
        }
        Indent::Spaces(_) => " ".repeat(cols),
    }
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if start {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            start = false;
        } else {
            out.push(ch);
            start = ch.is_whitespace() || ch == '-' || ch == '_';
        }
    }
    out
}

/// Compares lines as a person sorts them: ignoring case, with runs of
/// digits compared as numbers (`file2` before `file10`).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut first = String::new();
                while let Some(digit) = left.peek().copied().filter(char::is_ascii_digit) {
                    first.push(digit);
                    left.next();
                }
                let mut second = String::new();
                while let Some(digit) = right.peek().copied().filter(char::is_ascii_digit) {
                    second.push(digit);
                    right.next();
                }
                let order = first
                    .trim_start_matches('0')
                    .len()
                    .cmp(&second.trim_start_matches('0').len())
                    .then(
                        first
                            .trim_start_matches('0')
                            .cmp(second.trim_start_matches('0')),
                    );
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

/// The char index of `needle` in `haystack` at or after char `from`.
fn char_find(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let byte_from = haystack
        .char_indices()
        .nth(from)
        .map_or(haystack.len(), |(byte, _)| byte);
    let found = haystack[byte_from..].find(needle)? + byte_from;
    Some(haystack[..found].chars().count())
}

#[cfg(test)]
mod tests;
