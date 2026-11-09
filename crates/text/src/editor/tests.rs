use super::*;
use crate::language;

/// An editor on `text`, where `|` marks the caret (several `|` make
/// several carets) and `[`..`]` a selection.
fn ed(marked: &str) -> Editor {
    let mut text = String::new();
    let mut ranges = Vec::new();
    let mut open = None;
    for ch in marked.chars() {
        let at = text.chars().count();
        match ch {
            '|' => ranges.push(Range::caret(at)),
            '[' => open = Some(at),
            ']' => ranges.push(Range::new(open.take().unwrap(), at)),
            ch => text.push(ch),
        }
    }
    let mut editor = Editor::new(&text);
    let primary = ranges.len().saturating_sub(1);
    editor.set_selection(Selection::new(ranges, primary));
    editor
}

/// The text with the carets and selections marked again.
fn show(editor: &Editor) -> String {
    let text: Vec<char> = editor.text().chars().collect();
    let mut out = String::new();
    for (index, ch) in text.iter().chain(std::iter::once(&'\0')).enumerate() {
        for range in editor.selection().ranges() {
            if range.is_empty() && range.head == index {
                out.push('|');
            } else if !range.is_empty() && range.start() == index {
                out.push('[');
            }
            if !range.is_empty() && range.end() == index {
                out.push(']');
            }
        }
        if *ch != '\0' {
            out.push(*ch);
        }
    }
    out
}

fn rust(marked: &str) -> Editor {
    let mut editor = ed(marked);
    editor.set_language(language::by_name("rust").unwrap());
    editor
}

#[test]
fn typing_and_undo_in_one_burst() {
    let mut editor = ed("|");
    for ch in "hello".chars() {
        editor.type_text(&ch.to_string());
    }
    assert_eq!(show(&editor), "hello|");
    assert!(editor.undo());
    assert_eq!(show(&editor), "|");
    assert!(editor.redo());
    assert_eq!(show(&editor), "hello|");
}

#[test]
fn several_carets_type_at_once() {
    let mut editor = ed("a|\nb|\nc|");
    editor.type_text("!");
    assert_eq!(show(&editor), "a!|\nb!|\nc!|");
    editor.backspace();
    assert_eq!(show(&editor), "a|\nb|\nc|");
}

#[test]
fn brackets_close_and_type_over() {
    let mut editor = rust("let x = |");
    editor.type_text("(");
    assert_eq!(show(&editor), "let x = (|)");
    editor.type_text("1");
    editor.type_text(")");
    assert_eq!(show(&editor), "let x = (1)|");
    // Backspace in an empty pair removes both.
    let mut editor = rust("f(|)");
    editor.backspace();
    assert_eq!(show(&editor), "f|");
}

#[test]
fn quotes_do_not_close_after_a_word() {
    let mut editor = ed("don|");
    editor.type_text("'");
    assert_eq!(show(&editor), "don'|");
    let mut editor = rust("let s = |");
    editor.type_text("\"");
    assert_eq!(show(&editor), "let s = \"|\"");
}

#[test]
fn a_selection_is_wrapped() {
    let mut editor = rust("call [x] now");
    editor.type_text("(");
    assert_eq!(show(&editor), "call ([x]) now");
}

#[test]
fn enter_keeps_and_deepens_indentation() {
    let mut editor = rust("fn a() {|}");
    editor.newline();
    assert_eq!(show(&editor), "fn a() {\n    |\n}");
    editor.type_text("x");
    editor.newline();
    assert_eq!(show(&editor), "fn a() {\n    x\n    |\n}");
}

#[test]
fn enter_on_an_indent_only_line_empties_it() {
    let mut editor = rust("a\n    |");
    editor.newline();
    assert_eq!(show(&editor), "a\n\n    |");
}

#[test]
fn a_closer_on_an_empty_line_outdents() {
    let mut editor = rust("fn a() {\n    x\n    |");
    editor.type_text("}");
    assert_eq!(show(&editor), "fn a() {\n    x\n}|");
}

#[test]
fn backspace_removes_a_level_of_spaces() {
    let mut editor = ed("        |x");
    editor.backspace();
    assert_eq!(show(&editor), "    |x");
    let mut editor = ed("      |x");
    editor.backspace();
    assert_eq!(show(&editor), "    |x");
}

#[test]
fn word_deletion() {
    let mut editor = ed("one two  three|");
    editor.delete_word(true);
    assert_eq!(show(&editor), "one two  |");
    editor.delete_word(true);
    assert_eq!(show(&editor), "one |");
    let mut editor = ed("|foo.bar");
    editor.delete_word(false);
    assert_eq!(show(&editor), "|.bar");
}

#[test]
fn duplicate_lines_and_selections() {
    let mut editor = ed("one|\ntwo");
    editor.duplicate();
    assert_eq!(show(&editor), "one\none|\ntwo");
    let mut editor = ed("a [bc] d");
    editor.duplicate();
    assert_eq!(show(&editor), "a bc[bc] d");
}

#[test]
fn move_lines_up_and_down() {
    let mut editor = ed("one\ntw|o\nthree\n");
    editor.move_lines(true);
    assert_eq!(show(&editor), "tw|o\none\nthree\n");
    editor.move_lines(true);
    assert_eq!(show(&editor), "tw|o\none\nthree\n");
    editor.move_lines(false);
    editor.move_lines(false);
    assert_eq!(show(&editor), "one\nthree\ntw|o\n");
    // The empty line after the last line break stays last.
    editor.move_lines(false);
    assert_eq!(show(&editor), "one\nthree\ntw|o\n");
}

#[test]
fn smart_home() {
    let mut editor = ed("    let x|");
    editor.home(false);
    assert_eq!(show(&editor), "    |let x");
    editor.home(false);
    assert_eq!(show(&editor), "|    let x");
    editor.home(false);
    assert_eq!(show(&editor), "    |let x");
}

#[test]
fn indent_and_outdent_a_selection() {
    let mut editor = ed("[a\nb]\n");
    editor.indent();
    assert_eq!(editor.text(), "    a\n    b\n");
    editor.outdent();
    assert_eq!(editor.text(), "a\nb\n");
    let mut editor = ed("ab|");
    editor.indent();
    assert_eq!(show(&editor), "ab  |");
}

#[test]
fn comments_toggle() {
    let mut editor = rust("[fn a() {\n    b();\n}]");
    editor.toggle_comment();
    assert_eq!(editor.text(), "// fn a() {\n//     b();\n// }");
    editor.toggle_comment();
    assert_eq!(editor.text(), "fn a() {\n    b();\n}");
    let mut editor = rust("    x|\n    y");
    editor.toggle_comment();
    assert_eq!(editor.text(), "    // x\n    y");
}

#[test]
fn html_uses_block_comments() {
    let mut editor = ed("  <p>hi</p>|");
    editor.set_language(language::by_name("html").unwrap());
    editor.toggle_comment();
    assert_eq!(editor.text(), "  <!-- <p>hi</p> -->");
    editor.toggle_comment();
    assert_eq!(editor.text(), "  <p>hi</p>");
}

#[test]
fn copy_without_a_selection_copies_the_line() {
    let mut editor = ed("one\ntw|o\n");
    let copied = editor.copy().unwrap();
    assert_eq!(copied.text, "two\n");
    assert!(copied.linewise);
    editor.set_cursor(1, false);
    editor.paste("two\n");
    assert_eq!(show(&editor), "two\no|ne\ntwo\n");
}

#[test]
fn paste_spreads_lines_over_carets() {
    let mut editor = ed("|\n|\n|");
    editor.paste("a\nb\nc");
    assert_eq!(show(&editor), "a|\nb|\nc|");
}

#[test]
fn cut_whole_lines() {
    let mut editor = ed("one\ntw|o\nthree");
    editor.cut();
    assert_eq!(editor.text(), "one\nthree");
}

#[test]
fn vertical_moves_keep_the_column() {
    let mut editor = ed("long line|\nab\nlonger line");
    editor.move_vertical(1, false);
    assert_eq!(show(&editor), "long line\nab|\nlonger line");
    editor.move_vertical(1, false);
    assert_eq!(show(&editor), "long line\nab\nlonger li|ne");
}

#[test]
fn vertical_moves_by_wrapped_rows() {
    let mut editor = ed("|aaaa bbbb cccc");
    editor.set_wrap(Some(5));
    editor.move_vertical(1, false);
    assert_eq!(show(&editor), "aaaa |bbbb cccc");
    editor.end(false);
    assert_eq!(show(&editor), "aaaa bbbb| cccc");
}

#[test]
fn cursors_above_and_below() {
    let mut editor = ed("abc\nab|c\nabc");
    editor.add_cursor_vertical(true);
    editor.add_cursor_vertical(false);
    assert_eq!(show(&editor), "ab|c\nab|c\nab|c");
    editor.collapse();
    assert_eq!(editor.selection().len(), 1);
}

#[test]
fn next_occurrence() {
    let mut editor = ed("fo|o bar foo baz foo");
    assert!(editor.add_next_occurrence());
    assert_eq!(show(&editor), "[foo] bar foo baz foo");
    assert!(editor.add_next_occurrence());
    assert!(editor.add_next_occurrence());
    assert_eq!(show(&editor), "[foo] bar [foo] baz [foo]");
    assert!(!editor.add_next_occurrence());
}

#[test]
fn bracket_matching() {
    // `[` and `]` mark selections in these tests, so square brackets are left out.
    let editor = ed("f(a(1), {b})|");
    assert_eq!(editor.matching_bracket(editor.len_chars()), Some((11, 1)));
    assert_eq!(editor.matching_bracket(3), Some((3, 5)));
}

#[test]
fn trim_and_final_newline() {
    let mut editor = ed("a  \nb\t|");
    assert!(editor.trim_trailing_whitespace());
    assert_eq!(show(&editor), "a\nb|");
    assert!(editor.ensure_final_newline());
    assert_eq!(editor.text(), "a\nb\n");
}

#[test]
fn modified_follows_save_and_undo() {
    let mut editor = ed("|");
    editor.type_text("a");
    assert!(editor.is_modified());
    editor.mark_saved();
    assert!(!editor.is_modified());
    editor.type_text("b");
    editor.undo();
    assert!(!editor.is_modified());
}

#[test]
fn changes_are_logged_for_views() {
    let mut editor = ed("one\ntwo\nthree|");
    let seen = editor.revision();
    editor.type_text("!");
    assert_eq!(editor.changed_since(seen), Some(2));
    assert_eq!(editor.changed_since(editor.revision()), None);
}

#[test]
fn sort_and_join() {
    let mut editor = ed("[file10\nFile2\nfile1]");
    editor.sort_lines();
    assert_eq!(editor.text(), "file1\nFile2\nfile10");
    let mut editor = ed("a|\n    b");
    editor.join_lines();
    assert_eq!(show(&editor), "a |b");
}

#[test]
fn case_changes() {
    let mut editor = ed("[hello world]");
    editor.transform_case(Case::Title);
    assert_eq!(editor.text(), "Hello World");
    editor.transform_case(Case::Upper);
    assert_eq!(editor.text(), "HELLO WORLD");
}

#[test]
fn replace_all_is_one_step() {
    let mut editor = ed("a a a|");
    editor.replace_ranges(vec![
        ((0, 1), "bb".into()),
        ((2, 3), "bb".into()),
        ((4, 5), "bb".into()),
    ]);
    assert_eq!(editor.text(), "bb bb bb");
    editor.undo();
    assert_eq!(editor.text(), "a a a");
}

#[test]
fn graphemes_move_as_one() {
    let mut editor = ed("|e\u{301}x");
    editor.move_horizontal(true, false, false);
    assert_eq!(editor.selection().primary().head, 2);
    editor.backspace();
    assert_eq!(editor.text(), "x");
}
