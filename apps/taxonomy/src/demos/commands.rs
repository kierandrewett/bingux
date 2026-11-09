//! Commands and undo: a view that declares commands, the menus, palette and
//! sheet built from them, key caps, and the undo stack.

use bingux_ui::commands::{key_caps, open_palette, open_shortcuts};
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::Spec;

/// A counter view whose commands apply while the focus is inside it: press
/// one of its buttons, then its keys.
#[derive(PartialEq)]
struct Counter;

impl Component for Counter {
    fn render(&self) -> impl IntoElement {
        let count = use_state(|| 0i32);
        let mut bold = use_state(|| false);
        let undo = use_undo();
        // Every change goes through here, so each one can be taken back.
        let change = move |name: &'static str, to: i32| {
            let mut count = count;
            let from = *count.peek();
            if from == to {
                return;
            }
            count.set(to);
            undo.push(
                UndoStep::new(name, move || count.set(from), move || count.set(to)).coalesce(name),
            );
        };
        let commands = vec![
            Command::new("counter.up", "Increase", move |_| {
                change("Increase", *count.peek() + 1)
            })
            .shortcut("Ctrl+Up")
            .group(CommandGroup::Edit)
            .section("count"),
            Command::new("counter.down", "Decrease", move |_| {
                change("Decrease", *count.peek() - 1)
            })
            .shortcut("Ctrl+Down")
            .group(CommandGroup::Edit)
            .section("count"),
            Command::new("counter.reset", "Reset", move |_| change("Reset", 0))
                .shortcut("Ctrl+0")
                .enabled(count() != 0)
                .group(CommandGroup::Edit)
                .section("reset")
                .destructive(),
            Command::new("counter.bold", "Bold number", move |_| bold.toggle())
                .shortcut("Ctrl+B")
                .checked(bold())
                .group(CommandGroup::View)
                .keywords(["weight", "strong"]),
        ];
        let commands_handle = use_commands();
        let tokens = use_tokens();
        let style = if bold() {
            typography::TITLE
        } else {
            typography::BODY
        };
        let body = vstack(S3)
            .child(style.label(&tokens, format!("Count: {}", count())))
            .child(
                hstack(S2)
                    .child(action_button("Increase", move |_| change("Increase", *count.peek() + 1)))
                    .child(action_button("Decrease", move |_| change("Decrease", *count.peek() - 1)))
                    .child(
                        menu_button("More", Vec::new()).items_on_open(move || {
                            commands_handle.context_menu(&[&["counter.reset"], &["counter.bold"]])
                        }),
                    ),
            )
            .child(caption(
                "Press a button, then Ctrl+Up, Ctrl+Down, Ctrl+0 or Ctrl+B. Ctrl+Z undoes, Ctrl+Shift+P finds them.",
            ));
        command_scope("Counter", commands, body)
    }
}

/// Buttons that open the palette and the shortcuts sheet of the window.
fn openers() -> Element {
    let overlays = use_overlays();
    let commands = use_commands();
    hstack(S2)
        .child(action_button("Open palette", move |_| {
            open_palette(overlays, commands)
        }))
        .child(action_button("Show shortcuts", move |_| {
            open_shortcuts(overlays, commands)
        }))
        .child(app_menu(Vec::new()))
        .into_element()
}

pub const COMMANDS: &[Spec] = &[
    Spec::live("A view's commands", || Counter.into_element()),
    Spec::live("The main menu, built from commands", || openers())
        .script("click-text Increase; click-name Main menu; wait 500; save {out}"),
    Spec::forced("Key caps", || {
        let caps =
            |text: &str| key_caps(&Shortcut::parse(text).expect("a shortcut")).into_element();
        hstack(S4)
            .child(caps("Ctrl+Shift+P"))
            .child(caps("F2"))
            .child(caps("Ctrl+?"))
            .child(caps("Alt+Left"))
            .child(caps("Ctrl+Plus"))
            .into_element()
    }),
];

pub const PALETTE: &[Spec] = &[
    Spec::live("Command palette", || openers())
        .wide()
        .script("click-text Open palette; wait 600; save {out}"),
    Spec::live("Filtered", || openers())
        .wide()
        .script("click-text Open palette; wait 400; type shrt; wait 400; save {out}"),
];

pub const SHEET: &[Spec] = &[Spec::live("Keyboard shortcuts", || openers())
    .wide()
    .script("click-text Show shortcuts; wait 600; save {out}")];

/// A list of words with Add and Remove, its own Undo and Redo buttons, and
/// the window's Ctrl+Z.
#[derive(PartialEq)]
struct Words;

const WORDS: [&str; 6] = ["Apple", "Birch", "Cedar", "Damson", "Elder", "Fig"];

impl Component for Words {
    fn render(&self) -> impl IntoElement {
        let mut words = use_state(Vec::<&'static str>::new);
        let undo = use_undo();
        let add = move |_| {
            let next = WORDS[words.peek().len() % WORDS.len()];
            words.write().push(next);
            undo.push(UndoStep::new(
                format!("Add {next}"),
                move || {
                    words.write().pop();
                },
                move || words.write().push(next),
            ));
        };
        let remove = move |_| {
            let Some(last) = words.write().pop() else {
                return;
            };
            undo.push(UndoStep::new(
                format!("Remove {last}"),
                move || words.write().push(last),
                move || {
                    words.write().pop();
                },
            ));
        };
        let both = move |_| {
            undo.group("Add two", || {
                for _ in 0..2 {
                    let next = WORDS[words.peek().len() % WORDS.len()];
                    words.write().push(next);
                    undo.push(UndoStep::new(
                        "Add",
                        move || {
                            words.write().pop();
                        },
                        move || words.write().push(next),
                    ));
                }
            });
        };
        let list = words.read().join(", ");
        vstack(S3)
            .child(row("Words").value(if list.is_empty() {
                "None".to_owned()
            } else {
                list
            }))
            .child(
                hstack(S2)
                    .child(action_button("Add", add))
                    .child(action_button("Add two", both))
                    .child(action_button("Remove", remove).enabled(!words.read().is_empty())),
            )
            .child(
                hstack(S2)
                    .child(
                        action_button(undo.undo_title(), move |_| {
                            undo.undo();
                        })
                        .enabled(undo.can_undo()),
                    )
                    .child(
                        action_button(undo.redo_title(), move |_| {
                            undo.redo();
                        })
                        .enabled(undo.can_redo()),
                    ),
            )
            .into_element()
    }
}

pub const UNDO: &[Spec] = &[
    Spec::live("Named steps, a group, redo", || Words.into_element()),
    Spec::live("After two changes and an undo", || Words.into_element())
        .script("click-text Add; click-text Add two; wait 200; click-text Undo add two; wait 300; save {out}"),
];
