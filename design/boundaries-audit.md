# Boundaries audit, 2026-11

Every Bingux app and the shared window components, checked against
[the boundary guideline](boundaries.md). The scan in
`crates/ui/src/shell/boundary_scan_tests.rs` holds the result: its `PENDING`
list names any file still out of line.

## Violations and what was done

| App or component         | Control                                | Was                                                              | Verdict                           | Now                                                                 |
| ------------------------ | -------------------------------------- | ---------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------- |
| Calculator (Doop frames) | Basic, Scientific, Programmer, Convert | First row of the content                                         | Chooses content                   | Header switcher in the frames                                       |
| Calculator (app)         | The same modes                         | Header title slot as a value `segmented`                         | Right place, wrong control        | `HeaderBar::switcher(view_switcher(..))`; a phone row under the bar |
| Calculator               | History pane                           | Hand-painted `Paint::Sidebar` box                                | Shell's job                       | `Screen::aside`, `Pane::Aside` on a phone                           |
| Calendar                 | Day, Week, Month, Year, Agenda         | Trailing cluster on desktop; a hand-built content row on a phone | Chooses content                   | Header switcher; Previous, Next and Today ride with it              |
| Calendar                 | Sidebar tone, window background        | Painted by the app                                               | Shell's job                       | `Screen::sidebar`                                                   |
| Weather                  | Places                                 | List only; one place at a time                                   | Chooses content, no paging        | Carousel of places, page dots in `Screen::pager`                    |
| Weather                  | Sky                                    | A card inside the content                                        | Should be the whole window        | `Screen::immersive`; sky behind the page, glass chrome              |
| Media                    | Now-playing bar                        | Its own surface and hairline under the split view                | Persistent chrome                 | `NavigationSplitView::footer`; the sidebar ends above it            |
| Media                    | Full player                            | Header filled `Paint::Bg` by hand                                | Immersive                         | `Screen::immersive`                                                 |
| Settings, Sound effects  | Output and Input                       | `segmented(..).tabs()` in a hand-built bar                       | Chooses content                   | A toolbar `HeaderBar` with `.switcher`                              |
| Settings, Data usage     | Period (Today, Week, Month, Custom)    | A row of the page                                                | Filters the whole page            | `use_page_switcher`, in the page header                             |
| Text Editor              | Document tabs                          | The header bar itself                                            | Strip                             | `Screen::strip`; the toolbar is the header                          |
| Text Editor              | Find, Go to line bars                  | Between header and text                                          | Acts on content                   | `Screen::content_bar`                                               |
| Text Editor              | Status bar                             | Painted `Paint::Surface` by hand                                 | Persistent chrome                 | `Screen::footer`                                                    |
| Preview                  | Window frame                           | Hand-built column on `Paint::Bg`                                 | Shell's job                       | `Screen` (plain content in full screen)                             |
| Files                    | Strip, toolbar, places                 | Hand-painted tiers in the chooser                                | Shell's job                       | `Screen::strip`, header, `sidebar_width(Dim::PlacesColumn)`         |
| `Segmented`              | `.tabs()`                              | Public                                                           | A value picker could switch views | Crate-private; apps use `view_switcher`                             |

## Checked and left as they are

| App or component     | Control                                         | Why it stays                                                                                                   |
| -------------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Calculator           | Programmer base (HEX, DEC, OCT, BIN), word size | Values of the calculation: they set how the keypad types and how the value wraps. Marked `// boundary: value`. |
| Settings, Equalizer  | Mid and Side                                    | A row value of the EQ group; it picks which channel the strips under it edit, at that group's top edge.        |
| Settings             | Cursor size, desktop rows                       | Values of a setting, in rows.                                                                                  |
| Preview, PDF sidebar | Pages and Outline                               | At the top edge of the pane it switches, as the GNOME sidebar guideline asks.                                  |
| Media                | Column headers that sort                        | Part of the table.                                                                                             |
| Weather              | Units, refresh, add place                       | Already in the header and its menu.                                                                            |

## Not done

- Text Editor's Zed-style layout (project panel, breadcrumb row, bottom and
  outline panels): a follow-up after its Doop redesign. Prose is per file
  type, not a mode: see [the note on prose](text-editor-writing-mode.md).
- Maps and Browser: designed in Doop only; their shell gaps are collected
  from their `DESIGN.md` files when they are built.
