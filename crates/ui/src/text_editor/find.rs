//! Find and replace, and Go to line: bars docked over an editor.
//!
//! [`FindState`] is the search of one window: the query and its options,
//! the replacement, and the matches in a document. [`find_bar`] draws it
//! and [`TextEditorView::matches`](super::TextEditorView::matches) marks
//! the hits. A search of a large document runs on a worker over a snapshot
//! of the rope (a pointer copy), so typing in the field never waits.
//!
//! | Key in the bar | Does |
//! | -------------- | ---- |
//! | Enter, Shift+Enter | next, previous match |
//! | Alt+Enter | select every match (one caret each) |
//! | Alt+C, Alt+W, Alt+R | match case, whole words, regular expression |
//! | Escape | close the bar and go back to the text |

use std::rc::Rc;

use freya::prelude::*;

use crate::component::ComponentName;

use bingux_text::search::{self, MATCH_LIMIT, Matches};
use bingux_text::{Query, Selection};

use super::EditorHandle;
use crate::controls::{action_button, icon_button, search_field, text_field};
use crate::icons::Icon;
use crate::shell::{menu_button, menu_item};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// Documents larger than this are searched on a worker.
const SEARCH_ON_WORKER: usize = 2 << 20;

/// The search of one window. Every field is a state, so it is `Copy`.
#[derive(Clone, Copy, PartialEq)]
pub struct FindState {
    pub open: State<bool>,
    /// The replace row is shown.
    pub replacing: State<bool>,
    pub query: State<String>,
    pub replacement: State<String>,
    pub regex: State<bool>,
    pub case_sensitive: State<bool>,
    pub whole_word: State<bool>,
    matches: State<Rc<Matches>>,
    current: State<Option<usize>>,
    /// What the matches are for: the document's revision and the query.
    searched: State<Option<(u64, usize, Query)>>,
    error: State<Option<String>>,
    busy: State<bool>,
}

/// A search for the component's window.
pub fn use_find() -> FindState {
    FindState {
        open: use_state(|| false),
        replacing: use_state(|| false),
        query: use_state(String::new),
        replacement: use_state(String::new),
        regex: use_state(|| false),
        case_sensitive: use_state(|| false),
        whole_word: use_state(|| false),
        matches: use_state(|| Rc::new(Matches::default())),
        current: use_state(|| None),
        searched: use_state(|| None),
        error: use_state(|| None),
        busy: use_state(|| false),
    }
}

/// A key for a handle, so a search of one document is not taken for
/// another's.
fn document_key(handle: &EditorHandle) -> usize {
    Rc::as_ptr(&handle.editor) as usize
}

impl FindState {
    /// The query as the options say.
    pub fn query(&self) -> Query {
        Query {
            text: self.query.read().clone(),
            regex: *self.regex.read(),
            case_sensitive: *self.case_sensitive.read(),
            whole_word: *self.whole_word.read(),
        }
    }

    /// The matches to mark and the current one, while the bar is open.
    pub fn marks(&self) -> Option<(Rc<Matches>, Option<usize>)> {
        (*self.open.read() && !self.query.read().is_empty())
            .then(|| (self.matches.read().clone(), *self.current.read()))
    }

    /// Searches `handle` again when the text or the query changed since the
    /// last search. Called by the bar as it draws.
    pub fn refresh(&self, handle: &EditorHandle) {
        let query = self.query();
        let editor = handle.read();
        let key = (editor.revision(), document_key(handle), query.clone());
        if self.searched.peek().as_ref() == Some(&key) {
            return;
        }
        let mut searched = self.searched;
        searched.set(Some(key.clone()));
        let mut matches = self.matches;
        let mut current = self.current;
        let mut error = self.error;
        if query.is_empty() {
            matches.set(Rc::new(Matches::default()));
            current.set(None);
            error.set(None);
            return;
        }
        let pattern = match query.compile() {
            Ok(pattern) => pattern,
            Err(message) => {
                matches.set(Rc::new(Matches::default()));
                current.set(None);
                error.set(Some(message));
                return;
            }
        };
        error.set(None);
        let head = editor.selection().primary().start();
        if editor.rope().len_bytes() < SEARCH_ON_WORKER {
            let found = search::find_all(editor.rope(), &pattern);
            current.set(found.next_from(head));
            matches.set(Rc::new(found));
            return;
        }
        let rope = editor.rope().clone();
        let mut busy = self.busy;
        busy.set(true);
        spawn(async move {
            let found =
                crate::task::spawn_blocking(move || search::find_all(&rope, &pattern)).await;
            busy.set(false);
            // A newer search replaced this one while it ran.
            if searched.peek().as_ref() != Some(&key) {
                return;
            }
            current.set(found.next_from(head));
            matches.set(Rc::new(found));
        });
    }

    /// Selects the next match after the caret (or the one before it).
    pub fn step(&self, handle: &EditorHandle, forward: bool) {
        let matches = self.matches.peek().clone();
        let selection = handle.peek().selection().primary();
        let index = match (
            forward,
            matches.index_of((selection.start(), selection.end())),
        ) {
            (true, Some(index)) => Some((index + 1) % matches.ranges.len()),
            (false, Some(index)) => Some((index + matches.ranges.len() - 1) % matches.ranges.len()),
            (true, None) => matches.next_from(selection.end()),
            (false, None) => matches.previous_from(selection.start()),
        };
        let Some(index) = index else { return };
        let (start, end) = matches.ranges[index];
        let mut current = self.current;
        current.set(Some(index));
        handle.update(|editor| {
            editor.set_selection(Selection::single(bingux_text::Range::new(start, end)))
        });
    }

    /// Every match becomes a selection with its own caret.
    pub fn select_all(&self, handle: &EditorHandle) {
        let matches = self.matches.peek().clone();
        if matches.ranges.is_empty() {
            return;
        }
        let primary = self.current.peek().unwrap_or(0);
        handle.update(|editor| editor.select_ranges(&matches.ranges, primary));
    }

    /// Replaces the selected match and goes to the next.
    pub fn replace_one(&self, handle: &EditorHandle) {
        let query = self.query();
        let Ok(pattern) = query.compile() else { return };
        let matches = self.matches.peek().clone();
        let selection = handle.peek().selection().primary();
        let Some(index) = matches.index_of((selection.start(), selection.end())) else {
            self.step(handle, true);
            return;
        };
        let range = matches.ranges[index];
        let replacement = self.replacement.peek().clone();
        handle.update(|editor| {
            let text =
                search::replacement_for(editor.rope(), range, &pattern, &query, &replacement);
            editor.replace_ranges(vec![(range, text)]);
        });
        // Search again at once, then move on from the replacement.
        self.refresh_now(handle);
        self.step(handle, true);
    }

    /// Replaces every match, as one undo step. Returns how many.
    pub fn replace_all(&self, handle: &EditorHandle) -> usize {
        let query = self.query();
        let Ok(pattern) = query.compile() else {
            return 0;
        };
        let replacement = self.replacement.peek().clone();
        let count = handle.update(|editor| {
            let found = search::find_all(editor.rope(), &pattern);
            let replacements: Vec<((usize, usize), String)> = found
                .ranges
                .iter()
                .map(|range| {
                    (
                        *range,
                        search::replacement_for(
                            editor.rope(),
                            *range,
                            &pattern,
                            &query,
                            &replacement,
                        ),
                    )
                })
                .collect();
            let count = replacements.len();
            editor.replace_ranges(replacements);
            count
        });
        self.refresh_now(handle);
        count
    }

    fn refresh_now(&self, handle: &EditorHandle) {
        let mut searched = self.searched;
        searched.set(None);
        let query = self.query();
        let Ok(pattern) = query.compile() else { return };
        let found = search::find_all(handle.peek().rope(), &pattern);
        let mut matches = self.matches;
        let mut current = self.current;
        current.set(None);
        let editor = handle.peek();
        searched.set(Some((editor.revision(), document_key(handle), query)));
        matches.set(Rc::new(found));
    }

    /// Opens the bar, with the selected text (one line of it) as the query.
    pub fn open_from(&self, handle: &EditorHandle, replacing: bool) {
        if let Some(text) = handle.peek().primary_text()
            && !text.contains('\n')
            && text.chars().count() < 200
        {
            let mut query = self.query;
            let text = if *self.regex.peek() {
                search::escape(&text)
            } else {
                text
            };
            query.set(text);
        }
        let mut open = self.open;
        open.set(true);
        if replacing {
            let mut replacing_state = self.replacing;
            replacing_state.set(true);
        }
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set(false);
    }

    /// What the count says: `3 of 12`, `No results`, `10,000+`.
    fn summary(&self) -> String {
        if self.query.read().is_empty() {
            return String::new();
        }
        if let Some(error) = self.error.read().clone() {
            return error;
        }
        if *self.busy.read() {
            return "Searching…".into();
        }
        let matches = self.matches.read();
        let count = matches.ranges.len();
        if count == 0 {
            return "No results".into();
        }
        let total = if matches.truncated {
            format!("{}+", group(MATCH_LIMIT))
        } else {
            group(count)
        };
        match *self.current.read() {
            Some(index) => format!("{} of {total}", group(index + 1)),
            None => format!("{total} results"),
        }
    }
}

/// `12345` as `12,345`.
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

/// The find bar for `handle`. See the [module](self).
#[derive(Clone, PartialEq)]
pub struct FindBar {
    find: FindState,
    handle: EditorHandle,
    read_only: bool,
    on_close: Option<EventHandler<()>>,
    on_replaced: Option<EventHandler<usize>>,
}

pub fn find_bar(find: FindState, handle: EditorHandle) -> FindBar {
    FindBar {
        find,
        handle,
        read_only: false,
        on_close: None,
        on_replaced: None,
    }
}

impl FindBar {
    /// No replace row: the document cannot change.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Called after the bar closes, to give the keyboard back to the text.
    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }

    /// Told how many matches Replace All replaced, for a toast.
    pub fn on_replaced(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_replaced = Some(handler.into());
        self
    }
}

impl Component for FindBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let find = self.find;
        let handle = self.handle.clone();
        find.refresh(&handle);
        let summary = find.summary();
        let has_matches = !find.matches.read().ranges.is_empty();
        let replacing = *find.replacing.read() && !self.read_only;
        let error = find.error.read().is_some();

        let close = {
            let on_close = self.on_close.clone();
            move || {
                find.close();
                if let Some(on_close) = &on_close {
                    on_close.call(());
                }
            }
        };

        // On a desktop the three options are toggles beside the field, as in
        // other editors; on a phone they fold into one menu.
        let (r, c, word) = (
            *find.regex.read(),
            *find.case_sensitive.read(),
            *find.whole_word.read(),
        );
        let options: Element = {
            let mut regex = find.regex;
            let mut case = find.case_sensitive;
            let mut whole = find.whole_word;
            if crate::style::is_compact() {
                menu_button(
                    "Search options",
                    vec![
                        menu_item("Match case", move |_| case.set(!c))
                            .checked(c)
                            .shortcut("Alt+C"),
                        menu_item("Whole words", move |_| whole.set(!word))
                            .checked(word)
                            .shortcut("Alt+W"),
                        menu_item("Regular expression", move |_| regex.set(!r))
                            .checked(r)
                            .shortcut("Alt+R"),
                    ],
                )
                .icon(Icon::Filter)
                .into()
            } else {
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center)])
                    .child(
                        icon_button(Icon::FindMatchCase, move |_| case.set(!c))
                            .active(c)
                            .tooltip("Match case (Alt+C)"),
                    )
                    .child(
                        icon_button(Icon::FindWholeWord, move |_| whole.set(!word))
                            .active(word)
                            .tooltip("Whole words (Alt+W)"),
                    )
                    .child(
                        icon_button(Icon::FindRegex, move |_| regex.set(!r))
                            .active(r)
                            .tooltip("Regular expression (Alt+R)"),
                    )
                    .into()
            }
        };
        let options_on = crate::style::is_compact() && (r || c || word);

        // Enter's modifiers arrive just before the submit they belong to.
        let mut enter_modifiers = use_state(Modifiers::empty);
        let submit_handle = handle.clone();
        let previous_handle = handle.clone();
        let next_handle = handle.clone();
        let close_escape = close.clone();
        let field = search_field(find.query, |_: String| {})
            .flat()
            .fill_width()
            .placeholder(if r { "Regular expression" } else { "Find" })
            .autofocus()
            .on_escape(move |_| close_escape())
            .on_enter_modifiers(move |modifiers: Modifiers| enter_modifiers.set(modifiers))
            .on_submit(move |_: String| {
                let modifiers = *enter_modifiers.peek();
                if modifiers.contains(Modifiers::ALT) {
                    find.select_all(&submit_handle);
                } else {
                    find.step(&submit_handle, !modifiers.contains(Modifiers::SHIFT));
                }
            });

        let count_ink = if error { Ink::Danger } else { Ink::Muted };
        let mut replacing_state = find.replacing;
        let disclosure = (!self.read_only).then(|| {
            icon_button(
                if replacing {
                    Icon::PanDown
                } else {
                    Icon::PanEnd
                },
                move |_| {
                    replacing_state.set(!replacing);
                },
            )
            .tooltip(if replacing {
                "Hide replace"
            } else {
                "Show replace"
            })
        });

        let find_row = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S1),
                ],
            )
            .maybe_child(disclosure)
            .child(rect().tw(&tokens, &[w_flex()]).child(field))
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), px(S1)])
                    .child(
                        CAPTION
                            .in_ink(count_ink)
                            .label(&tokens, summary)
                            .font_tabular()
                            .max_lines(1),
                    ),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_row()])
                    .child(options)
                    .maybe_child(options_on.then(|| {
                        rect()
                            .tw(&tokens, &[w(Dim::Box5)])
                            .child(CAPTION.in_ink(Ink::Accent).label(&tokens, "•"))
                    })),
            )
            .child(
                icon_button(Icon::PanUp, move |_| find.step(&previous_handle, false))
                    .tooltip("Previous result (Shift+Enter)")
                    .enabled(has_matches),
            )
            .child(
                icon_button(Icon::PanDown, move |_| find.step(&next_handle, true))
                    .tooltip("Next result (Enter)")
                    .enabled(has_matches),
            )
            .child(icon_button(Icon::WindowClose, move |_| close()).tooltip("Close (Escape)"));

        let replace_row = replacing.then(|| {
            let one_handle = handle.clone();
            let all_handle = handle.clone();
            let submit_handle = handle.clone();
            let on_replaced = self.on_replaced.clone();
            let on_replaced_all = self.on_replaced.clone();
            let field = text_field(find.replacement, move |_: String| {
                find.replace_one(&submit_handle)
            })
            .flat()
            .fill_width()
            .placeholder("Replace with");
            rect()
                .tw(
                    &tokens,
                    &[
                        w_full(),
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S1),
                    ],
                )
                .child(rect().tw(&tokens, &[w(Dim::Control)]))
                .child(rect().tw(&tokens, &[w_flex()]).child(field))
                .child(
                    action_button("Replace", move |_| {
                        find.replace_one(&one_handle);
                        if let Some(handler) = &on_replaced {
                            handler.call(1);
                        }
                    })
                    .compact()
                    .enabled(has_matches),
                )
                .child(
                    action_button("Replace all", move |_| {
                        let count = find.replace_all(&all_handle);
                        if let Some(handler) = &on_replaced_all {
                            handler.call(count);
                        }
                    })
                    .compact()
                    .enabled(has_matches),
                )
        });

        let mut regex = find.regex;
        let mut case = find.case_sensitive;
        let mut whole = find.whole_word;
        rect()
            .component("FindBar")
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_col(),
                    gap(S1),
                    px(S2),
                    py(S1),
                    bg(Paint::Surface),
                ],
            )
            .a11y_role(AccessibilityRole::Search)
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if !event.modifiers.contains(Modifiers::ALT)
                    || event.modifiers.contains(Modifiers::CONTROL)
                {
                    return;
                }
                if let Key::Character(text) = &event.key {
                    match text.to_lowercase().as_str() {
                        "c" => {
                            let on = !*case.peek();
                            case.set(on);
                        }
                        "w" => {
                            let on = !*whole.peek();
                            whole.set(on);
                        }
                        "r" => {
                            let on = !*regex.peek();
                            regex.set(on);
                        }
                        _ => return,
                    }
                    event.stop_propagation();
                }
            })
            .child(find_row)
            .maybe_child(replace_row)
    }
}

/// A bar that asks for a line, and a column after a colon (`120:8`).
#[derive(Clone, PartialEq)]
pub struct GoToLineBar {
    handle: EditorHandle,
    open: State<bool>,
    on_close: Option<EventHandler<()>>,
}

pub fn go_to_line_bar(handle: EditorHandle, open: State<bool>) -> GoToLineBar {
    GoToLineBar {
        handle,
        open,
        on_close: None,
    }
}

impl GoToLineBar {
    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

/// `120`, `120:8`, `+5` and `-5` (lines from the caret's), from 1.
pub fn parse_line_target(
    text: &str,
    current_line: usize,
    lines: usize,
) -> Option<(usize, Option<usize>)> {
    let text = text.trim();
    let (line, col) = match text.split_once([':', ',']) {
        Some((line, col)) => (line.trim(), Some(col.trim())),
        None => (text, None),
    };
    let line = if let Some(rest) = line.strip_prefix('+') {
        current_line + rest.parse::<usize>().ok()?
    } else if let Some(rest) = line.strip_prefix('-') {
        current_line
            .saturating_sub(rest.parse::<usize>().ok()?)
            .max(1)
    } else {
        line.parse::<usize>().ok()?
    };
    let col = match col {
        Some(col) if !col.is_empty() => Some(col.parse::<usize>().ok()?),
        _ => None,
    };
    Some((line.clamp(1, lines.max(1)), col))
}

impl Component for GoToLineBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let value = use_state(String::new);
        let handle = self.handle.clone();
        let (lines, current) = {
            let editor = handle.read();
            (
                editor.len_lines(),
                editor.line_of(editor.selection().primary().head) + 1,
            )
        };
        let target = parse_line_target(&value.read(), current, lines);
        let hint = match (&target, value.read().trim().is_empty()) {
            (_, true) => format!("Line {current} of {lines}"),
            (Some((line, Some(col))), _) => format!("Line {line}, column {col}"),
            (Some((line, None)), _) => format!("Line {line}"),
            (None, false) => "Type a line number, or line:column".into(),
        };
        let open = self.open;
        let on_close = self.on_close.clone();
        let close = move || {
            let mut open = open;
            open.set(false);
            if let Some(on_close) = &on_close {
                on_close.call(());
            }
        };
        let submit_close = close.clone();
        let escape_close = close.clone();
        let button_close = close.clone();
        let go_handle = handle.clone();
        let go = move |text: String| {
            let editor_lines = go_handle.peek().len_lines();
            let current = {
                let editor = go_handle.peek();
                editor.line_of(editor.selection().primary().head) + 1
            };
            if let Some((line, col)) = parse_line_target(&text, current, editor_lines) {
                go_handle.update(|editor| editor.go_to(line, col));
                submit_close();
            }
        };
        rect()
            .component("GoToLineBar")
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S2),
                    px(S2),
                    py(S1),
                    bg(Paint::Surface),
                ],
            )
            .child(
                rect().tw(&tokens, &[w(Dim::Field)]).child(
                    text_field(value, go)
                        .flat()
                        .fill_width()
                        .placeholder("Go to line")
                        .autofocus()
                        .on_escape(move |_| escape_close()),
                ),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_flex()])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, hint).max_lines(1)),
            )
            .child(
                icon_button(Icon::WindowClose, move |_| button_close()).tooltip("Close (Escape)"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_targets() {
        assert_eq!(parse_line_target("12", 1, 100), Some((12, None)));
        assert_eq!(parse_line_target("12:4", 1, 100), Some((12, Some(4))));
        assert_eq!(parse_line_target("+5", 10, 100), Some((15, None)));
        assert_eq!(parse_line_target("-50", 10, 100), Some((1, None)));
        assert_eq!(parse_line_target("999", 1, 100), Some((100, None)));
        assert_eq!(parse_line_target("x", 1, 100), None);
    }

    #[test]
    fn grouping() {
        assert_eq!(group(12345), "12,345");
        assert_eq!(group(999), "999");
    }
}
