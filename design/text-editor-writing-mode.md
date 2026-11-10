# Text Editor: prose

Contributor note. The Text Editor has no Code and Writing modes. Prose is
handled by settings per file type, so a Markdown file or a plain text file
reads well without a switch.

| Setting                      | Markdown, plain text                    | Code                |
| ---------------------------- | --------------------------------------- | ------------------- |
| Soft wrap                    | On                                      | The person's choice |
| Readable column width        | Optional, about 80 characters, centered | Off                 |
| Word count in the status bar | Shown                                   | Hidden              |
| Proportional font            | Later, as a per-language setting        | Monospace           |

## Why the proportional font is later

`crates/ui/src/text_editor/view.rs` draws the text as a grid of monospace
cells. Caret placement, hit testing, selection rectangles and wrapping assume
one advance per character, so a proportional font needs a layout behind a
`TextLayout` trait (a cell layout and a shaped layout) first. That is about
2,000 lines over about five working days, and it is not scheduled.
