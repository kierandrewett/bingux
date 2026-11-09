//! Text editing: the editor surface, its find and go-to-line bars, the
//! Markdown view, and the held modifier keys that pointer gestures read.

use bingux_text::{Editor, Language};
use bingux_ui::input::held_modifiers;
use bingux_ui::prelude::*;
use bingux_ui::text_editor::{
    find_bar, go_to_line_bar, markdown_view, text_editor, use_editor, use_find,
};
use freya::prelude::*;

use crate::model::Spec;

const RUST: &str = "/// Greets someone, politely.\nfn greet(name: &str) -> String {\n    let count = 3;\n    format!(\"Hello, {name}! ({count})\")\n}\n\nfn main() {\n    for _ in 0..2 {\n        println!(\"{}\", greet(\"world\"));\n    }\n}\n";

const PROSE: &str = "A long line of prose wraps at the edge of the view, between words, and a\tTab and  two spaces show as marks when whitespace is shown.\n";

const MARKDOWN: &str = "# Release notes\n\nText Editor opens **any** text file, in *any* encoding, with `syntax` colours.\n\n## What is new\n\n- Find and replace with regular expressions\n- [x] Markdown preview\n- [ ] Spell checking\n\n> A quote is set apart.\n\n```rust\nfn main() { println!(\"hi\"); }\n```\n\n| Key | Does |\n|:--|:--|\n| Ctrl+D | Duplicate the line |\n";

fn rust_editor(text: &'static str) -> bingux_ui::text_editor::EditorHandle {
    use_editor(move || {
        let mut editor = Editor::new(text);
        editor.set_language(bingux_text::language::by_name("rust").unwrap_or_else(Language::plain));
        editor
    })
}

fn framed(child: impl IntoElement) -> Element {
    let tokens = use_tokens();
    rect()
        .tw(&tokens, &[w_full(), h_full(), flex_col()])
        .child(child)
        .into_element()
}

pub const TEXT_EDITOR: &[Spec] = &[
    Spec::live("Rust, line numbers, current line, type and select", || {
        let handle = rust_editor(RUST);
        framed(text_editor(handle).placeholder("Start typing"))
    })
    .wide()
    .page(300.0),
    Spec::forced("Wrapped prose, whitespace shown", || {
        let handle = use_editor(|| Editor::new(PROSE));
        framed(
            text_editor(handle)
                .wrap(true)
                .show_whitespace(true)
                .line_numbers(false),
        )
    })
    .wide()
    .page(120.0),
    Spec::forced("Several carets", || {
        let handle = use_editor(|| {
            let mut editor = Editor::new("let a = 1;\nlet b = 2;\nlet c = 3;\n");
            editor.select_ranges(&[(4, 5), (15, 16), (26, 27)], 2);
            editor
        });
        framed(text_editor(handle))
    })
    .wide()
    .page(120.0),
    Spec::forced("Empty, with a placeholder", || {
        let handle = use_editor(|| Editor::new(""));
        framed(text_editor(handle).placeholder("Start typing"))
    })
    .wide()
    .page(80.0),
];

pub const FIND_BAR: &[Spec] = &[
    Spec::live("Find with hits marked, Enter for the next", || {
        let tokens = use_tokens();
        let handle = rust_editor(RUST);
        let find = use_find();
        use_hook(move || {
            let (mut query, mut open) = (find.query, find.open);
            query.set("greet".into());
            open.set(true);
        });
        let marks = find.marks();
        let view = match marks {
            Some((matches, current)) => text_editor(handle.clone()).matches(matches, current),
            None => text_editor(handle.clone()),
        };
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col()])
            .child(find_bar(find, handle))
            .child(view)
            .into_element()
    })
    .wide()
    .page(300.0),
    Spec::forced("Replace row open", || {
        let tokens = use_tokens();
        let handle = rust_editor(RUST);
        let find = use_find();
        use_hook(move || {
            let (mut query, mut open, mut replacing, mut replacement) =
                (find.query, find.open, find.replacing, find.replacement);
            query.set("count".into());
            replacement.set("total".into());
            open.set(true);
            replacing.set(true);
        });
        rect()
            .tw(&tokens, &[w_full(), flex_col()])
            .child(find_bar(find, handle))
            .into_element()
    })
    .wide(),
    Spec::forced("Go to line", || {
        let handle = rust_editor(RUST);
        let open = use_state(|| true);
        go_to_line_bar(handle, open).into_element()
    })
    .wide(),
];

pub const MARKDOWN_VIEW: &[Spec] =
    &[
        Spec::live("Headings, lists, tasks, a quote, code and a table", || {
            framed(markdown_view(MARKDOWN))
        })
        .wide()
        .page(520.0),
    ];

pub const HELD_MODIFIERS: &[Spec] = &[Spec::live(
    "Turn the wheel here with Ctrl, Shift or Alt held",
    || {
        let tokens = use_tokens();
        let mut held = use_state(|| "Nothing held".to_string());
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::Row),
                    flex_row(),
                    items(Align::Center),
                    px(S4),
                ],
            )
            .tw(&tokens, BOX)
            .on_wheel(move |_: Event<WheelEventData>| {
                let modifiers = held_modifiers();
                let mut names = Vec::new();
                if modifiers.contains(Modifiers::CONTROL) {
                    names.push("Ctrl");
                }
                if modifiers.contains(Modifiers::SHIFT) {
                    names.push("Shift");
                }
                if modifiers.contains(Modifiers::ALT) {
                    names.push("Alt");
                }
                held.set(if names.is_empty() {
                    "Nothing held".into()
                } else {
                    names.join(", ")
                });
            })
            .child(typography::BODY.label(&tokens, held.read().clone()))
            .into_element()
    },
)];
