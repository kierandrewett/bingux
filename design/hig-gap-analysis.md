# Grade A gap analysis: bingux-ui against mature design systems

This document lists what Apple's Human Interface Guidelines, SwiftUI and its sister frameworks, GNOME and libadwaita, and
the larger design systems cover that bingux-ui does not. It then maps accessibility gaps to WCAG 2.2, the WAI-ARIA
Authoring Practices and AccessKit, and gives feature checklists for a file manager and six more apps.

It is a research note, not a spec. Nothing here changes code. Each gap has a priority and a suggested Rust API name so
the lead can turn the table into tickets.

Written on 2026-11-09 against `ga/research` (base `origin/main`). Reference pages were read on that date. Claims about
bingux-ui come from `docs/bingux-ui.md`, `docs/files.md`, `docs/settings.md`, `design/*.md`, `apps/taxonomy/src/registry.rs`
and the source under `crates/ui/src/`.

## How to read it

Status words describe bingux-ui today:

- **Has**: found in the source or in a doc that matches the source.
- **Partial**: exists for one app or one case, or covers part of what the reference does.
- **None**: no sign in the source or docs.
- **[INFERENCE]**: not checked directly. Verify before acting.

| Priority | Meaning                                                                                    |
| -------- | ------------------------------------------------------------------------------------------ |
| P0       | Blocks a grade A app, or fails an accessibility floor. Do first.                           |
| P1       | A polished app expects it. An app can work around it, but not in the shared look and feel. |
| P2       | Breadth and delight. Schedule after P0 and P1.                                             |

Suggested API names follow the SDK's Rust style: snake_case builder functions (`data_table(..)`) and CamelCase types
(`UndoStack`). None ends in `Kit`. They are proposals, not decisions.

## The 15 gaps that matter most

| #   | Gap                                                                         | Why it ranks here                                                                 |
| --- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| 1   | Drag and drop with other applications, spring loading, typed clipboard      | HIG says support it everywhere. `docs/files.md` says Freya has none on Wayland.   |
| 2   | One undo and redo manager with named actions                                | Only Files has an undo, it is private, and it has no Redo.                        |
| 3   | Commands: menu bar, shortcut registry, shortcuts dialog, command palette    | HIG wants every toolbar item as a command. Only `Ctrl+Shift+?` About exists.      |
| 4   | System accessibility preferences read by every widget                       | Tokens read only `BINGUX_REDUCED_MOTION`, the accent and the scheme.              |
| 5   | Text scaling as a real user setting, with relative type styles              | WCAG 1.4.4 and HIG want 200 %. Today it is a test switch.                         |
| 6   | Accessibility semantics: headings, relations, errors, tables, live regions  | Name and role exist for controls. Most AccessKit properties are never set.        |
| 7   | Generic data table with sort, resize, selection and a grid role             | Files has file-only columns. Every list app needs this.                           |
| 8   | Text editor widget: multi-line, find and replace, IME, spelling, text runs  | `text_field` multi-line is three rows tall. The editor app needs a real one.      |
| 9   | Date, time and calendar pickers, with time-zone aware formatting            | None found. The calendar app and every form with a date need them.                |
| 10  | Toolbar system: groups, overflow, customising, window titles                | Rows of icon buttons exist. HIG, GNOME and Fluent define a real toolbar.          |
| 11  | Document model: autosave, recents, unsaved mark, state restore              | Files restores window size only. "Restoring the tabs" is listed as not done.      |
| 12  | Media surface, MPRIS, now playing, picture in picture                       | Preview host plays a file. A media app needs the service side.                    |
| 13  | Desktop notifications and a notice taxonomy                                 | Toasts and banners exist. Nothing reaches other apps or survives a closed window. |
| 14  | Localisation: plurals, CLDR number and date formats, IME verified           | One number builder has a locale. No plural or date formatting API was found.      |
| 15  | Inspector pane, generic list with selection modes, search scopes and tokens | Split views exist. The trailing inspector and edit modes do not.                  |

Close behind: chart accessibility, a gauge, a share action, printing, a rich text or Markdown view, tips, spring
animation and a map view. They are in the full table.

## 1. Gap table

Columns: what the reference systems do, what bingux-ui has, priority, and a suggested name. Evidence for each row is in
sections 3 and 4.

### A. Input, platform and system services

| ID  | Gap                        | Reference systems do                                                                           | bingux-ui today                                                             | Pri | Suggested API                                        |
| --- | -------------------------- | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | --- | ---------------------------------------------------- |
| A1  | Drag and drop between apps | Multi-item drag, drag image, badge, valid-target feedback, autoscroll, Option to copy, undo    | **Partial.** In-window `PointerDrag` and `FileDrag` only                    | P0  | `dnd::DragSource`, `dnd::DropTarget`, `Payload`      |
| A2  | Spring-loaded targets      | Hover a drag over a button or folder to activate it                                            | **None**                                                                    | P1  | `.spring_loaded(Duration)`                           |
| A3  | Typed clipboard            | Copy and paste several representations; paste validators                                       | **None** in `crates/ui` (text edit menu only)                               | P1  | `Clipboard::write(Payload)`, `.on_paste(f)`          |
| A4  | Undo and redo              | One manager, named steps, Edit menu, multi-step, redo                                          | **Partial.** Private `UndoStack` in Files; no Redo                          | P0  | `UndoStack`, `use_undo()`, `.undo_name("Rename")`    |
| A5  | Command registry           | Menu bar, shortcut list, toolbar items as commands, command palette                            | **None.** `MenuItem::shortcut` is a hint only                               | P0  | `Commands`, `Command::new(id)`, `command_palette()`  |
| A6  | Shortcuts dialog           | libadwaita `ShortcutsDialog`; GNOME `Ctrl+?`                                                   | **Partial.** `Ctrl+Shift+?` opens About                                     | P1  | `shortcuts_dialog(Commands)`                         |
| A7  | System accessibility prefs | Reduce Motion, Increase Contrast, Reduce Transparency, Differentiate Without Colour, text size | **Partial.** `BINGUX_REDUCED_MOTION` only; scheme and accent by `gsettings` | P0  | `AccessPrefs`, `use_access_prefs()`                  |
| A8  | Desktop notifications      | System notifications with actions and urgency; Time Sensitive levels                           | **None**                                                                    | P1  | `notify::Notification::new(..).action(..)`           |
| A9  | Share                      | Share button, share sheet, collaboration                                                       | **None**                                                                    | P2  | `share_link(Payload)`                                |
| A10 | Printing                   | Print dialog, page setup, preview                                                              | **None**                                                                    | P2  | `print::Job`, `Commands::print`                      |
| A11 | Input methods              | Composition, candidate window, dictation                                                       | **[INFERENCE]** depends on winit text-input; not verified                   | P1  | `text_field(..).input_purpose(..)`                   |
| A12 | Locale services            | Plural rules, number, date and unit formats                                                    | **Partial.** `AnimatedNumber::locale` only                                  | P1  | `Locale`, `format::{Number, Date, Bytes}`, `plural!` |
| A13 | Full screen                | System full screen, auto-hide chrome, resume                                                   | **Partial.** Preview `F`                                                    | P2  | `window.full_screen(bool)`, `.hides_chrome()`        |

### B. Structure and navigation

| ID  | Gap                       | Reference systems do                                                              | bingux-ui today                                              | Pri | Suggested API                                            |
| --- | ------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------ | --- | -------------------------------------------------------- |
| B1  | Toolbar                   | Groups, overflow "More", customising, prominent action, window title and subtitle | **Partial.** `HeaderBar`, `action_strip`, ad hoc rows        | P1  | `toolbar(..).group(..).overflow()`, `ToolbarItem`        |
| B2  | Menu bar                  | Standard menus and order, dynamic items, Window and Help menus                    | **None.** Header app menu only                               | P1  | `menu_bar(Commands)`, `app_menu(..)` (exists)            |
| B3  | Inspector or utility pane | Trailing pane that follows the selection; toggle key; width memory                | **None.** `split_pane`, `master_detail` exist                | P1  | `inspector(pane).width(min, ideal, max)`                 |
| B4  | Sidebar customising       | Show, hide, reorder items; section collapse; auto-hide on resize                  | **Partial.** Files bookmarks; app sidebars fixed             | P2  | `NavigationSplitView::customisable(..)`                  |
| B5  | Document model            | Autosave, versions, recents, unsaved dot, title proxy                             | **None**                                                     | P1  | `Document` trait, `document_window(..)`                  |
| B6  | State restoration         | Reopen where you left off                                                         | **Partial.** Window size and per-folder view only            | P1  | `use_scene_state(key)`, `Restorable`                     |
| B7  | Multi-window              | New window command, window menu, tab merge                                        | **Partial.** Tab tear-off; "new window from a menu" not done | P2  | `window.open(..)` (exists), `Commands::new_window`       |
| B8  | Search model              | Scope bar, tokens, suggestions, recents with clear, system-wide index             | **Partial.** `Omnibox`, `use_search`; chips in Files only    | P1  | `search_field(..).scopes(..).tokens(..).suggestions(..)` |
| B9  | Tips and onboarding       | Contextual tips with rules; no blocking tours                                     | **None**                                                     | P2  | `tip(id).rule(..)`                                       |
| B10 | Help                      | Help menu, context help, tooltips of 60 to 75 characters                          | **Partial.** `tooltip` exists; no help menu                  | P2  | `help_menu(..)`                                          |

### C. Collections and data

| ID  | Gap                     | Reference systems do                                                                    | bingux-ui today                             | Pri | Suggested API                                             |
| --- | ----------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------- | --- | --------------------------------------------------------- |
| C1  | Data table              | Sort, resize, reorder columns, selection, batch bar, row heights, skeleton state, pages | **Partial.** Files `ColumnHeaders` only     | P0  | `data_table(rows, columns)`, `Column::new(..).sortable()` |
| C2  | Outline table           | Hierarchy in the first column, remembered expansion                                     | **Partial.** `tree_view` without columns    | P1  | `data_table(..).tree(children)`                           |
| C3  | Generic list with modes | Selection and edit modes, swipe actions, reorder, section headers                       | **Partial.** `sortable`, `rows`, Files view | P1  | `list_view(..).selection(Selection::Multiple)`            |
| C4  | Selection model         | Range, toggle, select by pattern, invert, Ctrl+Space                                    | **Partial.** In `bingux_files::selection`   | P1  | `SelectionSet`, `Selection::{None, Single, Multiple}`     |
| C5  | Collection grid         | Adaptive grid, `Grid`, `LazyVGrid`, grid views                                          | **Partial.** `file_grid`, `tile_rows`       | P1  | `grid(..).columns(Adaptive(min))`                         |
| C6  | Column (Miller) view    | Root in first column, preview of leaf                                                   | **None**                                    | P2  | `column_view(..)`                                         |
| C7  | Token field             | Text becomes tokens; menus on tokens; suggestions                                       | **Partial.** `omni_chip` in Files           | P1  | `token_field(tokens, on_change)`                          |
| C8  | Pagination and refresh  | Pages above 50 rows; pull to refresh                                                    | **None**                                    | P2  | `pagination(..)`, `.refreshable(f)`                       |

### D. Controls

| ID  | Gap                         | Reference systems do                                                     | bingux-ui today                                       | Pri | Suggested API                                             |
| --- | --------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------- | --- | --------------------------------------------------------- |
| D1  | Text editor                 | Long text, undo, find bar, line numbers, spelling, IME, text runs for AT | **Partial.** `text_field(..).multiline` is three rows | P0  | `text_editor(State<TextBuffer>)`                          |
| D2  | Rich text and Markdown view | Styled text, links, selection                                            | **None** (`styled_paragraph` only)                    | P1  | `rich_text(AttributedText)`, `markdown(&str)`             |
| D3  | Date and time pickers       | Compact, inline, graphical, ranges, minute steps                         | **None**                                              | P0  | `date_picker(..)`, `time_picker(..)`, `calendar_grid(..)` |
| D4  | Combo box                   | Text field plus list; custom value                                       | **Partial.** `dropdown`, `Omnibox`                    | P2  | `combo_box(..)`                                           |
| D5  | Range slider and ticks      | Two thumbs; tick marks with labels                                       | **Partial.** One tick at 100 %                        | P2  | `slider(..).ticks(..)`, `range_slider(..)`                |
| D6  | Gauge                       | Continuous or discrete, tinted ranges, min and max labels                | **Partial.** `usage_meter`, `level_meter`             | P2  | `gauge(value, range).style(..)`                           |
| D7  | Rating indicator            | Stars, read-only or editable                                             | **None**                                              | P2  | `rating(value, max)`                                      |
| D8  | Carousel and page control   | Paged content with dots                                                  | **None**                                              | P2  | `carousel(..)`, `page_dots(..)`                           |
| D9  | Image well                  | Drop or paste an image, revert to default                                | **None**                                              | P2  | `image_well(..)`                                          |
| D10 | Popover with arrow          | Attached or detached popover, one at a time                              | **Partial.** Popup windows; no arrow                  | P2  | `popover(anchor, content).detachable()`                   |

### E. Feedback and status

| ID  | Gap                   | Reference systems do                                                        | bingux-ui today                                  | Pri | Suggested API                                   |
| --- | --------------------- | --------------------------------------------------------------------------- | ------------------------------------------------ | --- | ----------------------------------------------- |
| E1  | Notice taxonomy       | Inline, toast, banner, callout, panel, modal by disruption (Carbon, Primer) | **Partial.** Toast, banner, dialog, `status`     | P1  | `Notice::{Inline, Toast, Banner, Callout}`      |
| E2  | Degraded mode pattern | Render the page without the failed part (Primer)                            | **None** as a pattern                            | P2  | `degraded(part, fallback)`                      |
| E3  | Job list and progress | Several jobs, ETA, pause, cancel with consequence text                      | **Partial.** Files status pill and busy line     | P1  | `JobList`, `Job::progress(..)`                  |
| E4  | Saving pattern        | Auto-save versus explicit save; unsaved warning                             | **Partial.** `pending_bar` for forms             | P1  | `SaveMode::{Auto, Explicit}`                    |
| E5  | Empty-state variants  | First use, no results, error, cleared (Carbon, Polaris)                     | **Has** `status_page`, `status`; no variant enum | P2  | `status_page(..).variant(Empty::NoResults)`     |
| E6  | Time-boxed UI control | Avoid auto-dismiss; pause on hover or focus                                 | **Partial.** Toast with action                   | P1  | `Toast::persistent()`, pause on hover and focus |

### F. Content and media

| ID  | Gap                 | Reference systems do                                                           | bingux-ui today                                 | Pri | Suggested API                             |
| --- | ------------------- | ------------------------------------------------------------------------------ | ----------------------------------------------- | --- | ----------------------------------------- |
| F1  | Chart accessibility | Descriptions, summary, audio graph, keyboard navigation, no colour-only series | **Partial.** Charts exist; no descriptor        | P1  | `Chart::describe(..)`, `ChartDescriptor`  |
| F2  | Chart interaction   | Selection, hover read-out, ranges, annotations, mark types                     | **Partial.** Line, bar, donut, sparkline, flame | P1  | `Chart::selection(..)`, `.annotation(..)` |
| F3  | Video surface       | System player controls, aspect ratio, PiP, captions, AirPlay-like cast         | **Partial.** Preview host plays in a pane       | P0  | `video_view(Player)`, `PlayerControls`    |
| F4  | Media session       | Now Playing, hardware keys, MPRIS                                              | **None**                                        | P0  | `mpris::Session::new(player)`             |
| F5  | PDF view            | Annotations, forms, signatures, page edits, text selection                     | **Partial.** Render, zoom, find, select         | P1  | `pdf_view(Document).annotations(..)`      |
| F6  | Map view            | Tiles, annotations, overlays, look-around                                      | **None**                                        | P2  | `map_view(..).annotation(..)`             |
| F7  | Image view          | Zoom, rotate, markup, instant alpha                                            | **Partial.** Preview image viewer               | P1  | `image_view(..).zoomable()`, `Markup`     |
| F8  | Text views          | Selectable useful text; link detection                                         | **Has** `select_text`                           | P2  | `.detect_links()`                         |

### G. Foundations and governance

| ID  | Gap                      | Reference systems do                                                 | bingux-ui today                                         | Pri | Suggested API                                     |
| --- | ------------------------ | -------------------------------------------------------------------- | ------------------------------------------------------- | --- | ------------------------------------------------- |
| G1  | Dynamic Type             | Text styles scale with a system setting; icons scale; layout reflows | **Partial.** `BINGUX_TEXT_SCALE` test switch            | P0  | `TextScale`, `TextStyle::relative_to(..)`         |
| G2  | High contrast theme      | Increase Contrast variant of every colour; forced-colours behaviour  | **None.** Contrast is measured, not switchable          | P0  | `Scheme::high_contrast`, `Tokens::new(.., prefs)` |
| G3  | Reduce transparency      | Opaque materials                                                     | **None** found                                          | P1  | `AccessPrefs::reduce_transparency`                |
| G4  | User density             | Compact versus comfortable (Carbon five sizes, Windows Compact view) | **Partial.** `SizeClass` follows window width and touch | P1  | `Density::{Comfortable, Compact}`                 |
| G5  | Spring and shared motion | Spring tokens, shared element, phase and keyframe animation          | **Partial.** Tween tokens, `icon_morph`                 | P2  | `Spring`, `.shared_element(id)`                   |
| G6  | Token tiers              | Reference, system and component tokens; themes as data               | **Partial.** One `Tokens` struct and `Paint`            | P2  | `Tokens::component(..)`, `Theme::from_data(..)`   |
| G7  | Content guidelines       | Voice, capitalisation, actionable language, error copy               | **Has** `scripts/copy-lint.py`; no UI copy API          | P2  | `copy::lint` in tests                             |
| G8  | Focus management APIs    | Focus sections, default focus, programmatic focus, focus state       | **Partial.** `FocusCore`, F6 regions, `autofocus()`     | P1  | `focus_scope(..)`, `.default_focus(..)`           |
| G9  | Access keys              | Alt+letter mnemonics in menus and dialogs                            | **None** found                                          | P2  | `.access_key('f')`                                |

## 2. What bingux-ui already does well

Do not rebuild these. They already match or exceed the references.

| Area            | Evidence in the repository                                                                                 |
| --------------- | ---------------------------------------------------------------------------------------------------------- |
| Contrast        | Every pair measured against WCAG 2.2 and APCA; `ensure_contrast`; contrast matrix (`docs/bingux-ui.md`)    |
| Keyboard focus  | One Tab stop per control, roving groups, F6 regions, modal scopes, a ring checked by `focus-ring-check.py` |
| Motion          | Tokens of 80 to 200 ms, reduced-motion path in every animated control, in-motion tests                     |
| Adaptive layout | `SizeClass`, 390 px and desktop checks, `BINGUX_TEXT_SCALE=2` and pseudo-locale stress runs                |
| Right to left   | Mirroring rules and checks (`design/rtl.md`)                                                               |
| Navigation      | URI routes, history, predictive back, history menus (`design/navigation.md`)                               |
| Search          | Omnibox with chips, typed filters, streaming results (`design/files-search.md`)                            |
| Files engine    | Trash, undo, conflicts, compress, properties, tabs, Quick Look, previews                                   |
| Inspector       | A11y checks for naming, focus, target size, contrast, motion (`inspector/a11y_checks.rs`)                  |
| Governance      | Taxonomy coverage check, layout lint, state check, `copy-lint.py`, `design/rules.md`                       |

## 3. What each reference system adds

### 3.1 Apple Human Interface Guidelines

Each bullet is a rule bingux-ui does not yet meet or cannot express. All links go to the HIG page that states it.

**Drag and drop.** [Drag and drop][hig-dnd] asks for support throughout the app, multi-item drags, a drag image after about
three points, and visible feedback for valid and invalid drops. It asks for autoscroll in the destination, Option to force a
copy, a count badge, undo of a drop, and spring loading. Files has the in-window half only (A1, A2).

**Undo and redo.** [Undo and redo][hig-undo] asks for unlimited steps, named results ("Undo Rename"), showing the result
of an undo, and standard Edit menu keys. Files names its undo, has no Redo, and shows the result by toast (A4).

**Menus, the menu bar and commands.** [The menu bar][hig-menubar] asks for the standard menus in the standard order, a View
menu even when small, state-aware Show and Hide titles, and a Window and Help menu.

[Toolbars][hig-toolbars] says every toolbar item must also be a menu command, because people hide or customise the toolbar.
[Menus][hig-menus] sets an ellipsis for actions that need more input, checkmarks for state, and changeable labels.

[Context menus][hig-ctx] says hide unavailable items instead of dimming, and keep shortcuts out of context menus. Files does
these two well. The rest needs A5, A6 and B2.

**Toolbars and windows.** [Toolbars][hig-toolbars] lists a More menu, customising, at most three groups, one prominent
action on the trailing side, a window title under about 15 characters, and no app name as the title.

[Windows][hig-windows] asks windows to adapt fluidly and to avoid custom window chrome. Bingux draws its own chrome by
design, so it must match the system's behaviour with more care (B1).

**Sidebars, split views, column views, path controls.** [Sidebars][hig-sidebars] asks for user customising, at most two
levels, and hide on resize. [Split views][hig-split] asks for persistent selection highlight in each pane, minimum and
maximum pane sizes, several ways to reveal a hidden pane, and drag between panes. [Column views][hig-columns] and
[Path controls][hig-path] describe views Files lacks (C6).

**Lists, tables and outlines.** [Lists and tables][hig-lists] asks for sortable headings, resizable columns, optional zebra
rows and a middle ellipsis. [Outline views][hig-outline] adds hierarchy in the first column only, remembered expansion, and
a search field for long outlines (C1, C2).

**Search.** [Search fields][hig-searchfields] and [Searching][hig-search] ask for a scope bar, tokens, suggestions, recent
searches with a way to clear them, and a clear statement of the current scope (B8).

**Status.** [Progress indicators][hig-progress] asks for determinate bars where possible, no switch from spinner to bar,
a cancel with a warning when cancelling loses work, and a consistent place. [Gauges][hig-gauges] defines continuous and
discrete styles with tinted ranges (D6). [Feedback][hig-feedback] and [Loading][hig-loading] ask to show something at
once and let people work while content loads.

**Text and content.** [Text views][hig-textviews] asks for selectable useful text. [Charts][hig-charts] asks for a
description, a summary, audio graphs, no colour-only series, and keyboard navigation of marks (F1, F2).

**Settings, help, onboarding.** [Settings][hig-settings] asks to restore the last pane and put the pane name in the title.
[Offering help][hig-help] asks for tooltips of 60 to 75 characters in sentence case. [Onboarding][hig-onboarding] asks to
teach through interaction and postpone setup. Settings already follows the first. B9 and B10 are the rest.

**Writing.** [Writing][hig-writing] asks for action-oriented labels, a next step on every blank screen and clear errors.
`scripts/copy-lint.py` and the copy rules in the context file cover most of it (G7).

**Motion, materials, full screen.** [Motion][hig-motion] says make motion optional and cancellable. [Materials][hig-materials]
asks for legibility over blur. [Going full screen][hig-fullscreen] asks to keep controls reachable and let people choose
when to exit (A13, G3).

**Media.** [Playing video][hig-video] asks to keep the original aspect ratio, resume without asking, and start playback
at once. [Playing audio][hig-audio] asks for system volume, rerouting, and well-defined interruption handling. Section 6.1
turns these into a checklist.

**Files.** [File management][hig-files] asks to hide extensions by default with a way to show them, to autosave, and to
offer a Quick Look viewer even for files the app cannot open. Files has Quick Look. It has no extension toggle (section 5).

### 3.2 SwiftUI and Apple framework catalogue

Documentation base: [SwiftUI](https://developer.apple.com/documentation/swiftui). The "bingux-ui" column is the nearest
existing component.

| SwiftUI or framework                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | bingux-ui nearest                        | Status  | Pri |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------- | ------- | --- |
| [`NavigationSplitView`](https://developer.apple.com/documentation/swiftui/navigationsplitview)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `NavigationSplitView`                    | Has     | -   |
| [`Table`](https://developer.apple.com/documentation/swiftui/table), [`TableColumn`](https://developer.apple.com/documentation/swiftui/tablecolumn), [column customisation](https://developer.apple.com/documentation/swiftui/tablecolumncustomization)                                                                                                                                                                                                                                                                                                                                                                                                       | Files `ColumnHeaders`                    | Partial | P0  |
| [`Grid`](https://developer.apple.com/documentation/swiftui/grid), [`LazyVGrid`](https://developer.apple.com/documentation/swiftui/lazyvgrid), [`Layout`](https://developer.apple.com/documentation/swiftui/layout), [`ViewThatFits`](https://developer.apple.com/documentation/swiftui/viewthatfits)                                                                                                                                                                                                                                                                                                                                                         | `tile_rows`, `controls/adaptive.rs`      | Partial | P1  |
| [`ContentUnavailableView`](https://developer.apple.com/documentation/swiftui/contentunavailableview)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | `status_page`                            | Has     | -   |
| [`Gauge`](https://developer.apple.com/documentation/swiftui/gauge)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | `usage_meter`                            | Partial | P2  |
| [`ProgressView`](https://developer.apple.com/documentation/swiftui/progressview)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | `progress`, `spinner`                    | Has     | -   |
| [`DatePicker`](https://developer.apple.com/documentation/swiftui/datepicker)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | none                                     | None    | P0  |
| [`ColorPicker`](https://developer.apple.com/documentation/swiftui/colorpicker)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `colour_picker`, `colour_button`         | Has     | -   |
| [`TextEditor`](https://developer.apple.com/documentation/swiftui/texteditor), [`findNavigator`](<https://developer.apple.com/documentation/swiftui/view/findnavigator(ispresented:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `text_field` multi-line                  | Partial | P0  |
| [`ShareLink`](https://developer.apple.com/documentation/swiftui/sharelink)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | none                                     | None    | P2  |
| [`searchable`](<https://developer.apple.com/documentation/swiftui/view/searchable(text:placement:prompt:)>), [`searchSuggestions`](<https://developer.apple.com/documentation/swiftui/view/searchsuggestions(_:)>), [`searchScopes`](<https://developer.apple.com/documentation/swiftui/view/searchscopes(_:activation:_:)>)                                                                                                                                                                                                                                                                                                                                 | `NavigationSplitView::search`, `Omnibox` | Partial | P1  |
| [`refreshable`](<https://developer.apple.com/documentation/swiftui/view/refreshable(action:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | none                                     | None    | P2  |
| [`inspector`](<https://developer.apple.com/documentation/swiftui/view/inspector(ispresented:content:)>), [`inspectorColumnWidth`](<https://developer.apple.com/documentation/swiftui/view/inspectorcolumnwidth(min:ideal:max:)>)                                                                                                                                                                                                                                                                                                                                                                                                                             | none                                     | None    | P1  |
| [`toolbar`](<https://developer.apple.com/documentation/swiftui/view/toolbar(content:)>), [`toolbar(id:)`](<https://developer.apple.com/documentation/swiftui/view/toolbar(id:content:)>), [`ToolbarItem`](https://developer.apple.com/documentation/swiftui/toolbaritem)                                                                                                                                                                                                                                                                                                                                                                                     | `HeaderBar`, `action_strip`              | Partial | P1  |
| [`Commands`](https://developer.apple.com/documentation/swiftui/commands), [`keyboardShortcut`](<https://developer.apple.com/documentation/swiftui/view/keyboardshortcut(_:modifiers:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `MenuItem::shortcut`                     | Partial | P0  |
| [`draggable`](<https://developer.apple.com/documentation/swiftui/view/draggable(_:)>), [`dropDestination`](<https://developer.apple.com/documentation/swiftui/view/dropdestination(for:action:istargeted:)>), [`springLoadingBehavior`](<https://developer.apple.com/documentation/swiftui/view/springloadingbehavior(_:)>)                                                                                                                                                                                                                                                                                                                                  | `PointerDrag`, `FileDrag`                | Partial | P0  |
| [`copyable`](<https://developer.apple.com/documentation/swiftui/view/copyable(_:)>), [`pasteDestination`](<https://developer.apple.com/documentation/swiftui/view/pastedestination(for:action:validator:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                  | text edit menu                           | Partial | P1  |
| [`UndoManager` environment](https://developer.apple.com/documentation/swiftui/environmentvalues/undomanager)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Files `UndoStack`                        | Partial | P0  |
| [`fileImporter`](<https://developer.apple.com/documentation/swiftui/view/fileimporter(ispresented:allowedcontenttypes:allowsmultipleselection:oncompletion:)>), [`fileExporter`](<https://developer.apple.com/documentation/swiftui/view/fileexporter(ispresented:document:contenttype:defaultfilename:oncompletion:)>)                                                                                                                                                                                                                                                                                                                                      | `files::chooser`                         | Has     | -   |
| [`quickLookPreview`](<https://developer.apple.com/documentation/swiftui/view/quicklookpreview(_:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `preview::open`                          | Has     | -   |
| [`DocumentGroup`](https://developer.apple.com/documentation/swiftui/documentgroup), [`FileDocument`](https://developer.apple.com/documentation/swiftui/filedocument)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | none                                     | None    | P1  |
| [`Settings`](https://developer.apple.com/documentation/swiftui/settings) scene                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | Settings app pattern                     | Has     | -   |
| [`popover`](<https://developer.apple.com/documentation/swiftui/view/popover(ispresented:attachmentanchor:arrowedge:content:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | popup windows                            | Partial | P2  |
| [`confirmationDialog`](<https://developer.apple.com/documentation/swiftui/view/confirmationdialog(_:ispresented:titlevisibility:actions:)>), [`alert`](<https://developer.apple.com/documentation/swiftui/view/alert(_:ispresented:actions:message:)>)                                                                                                                                                                                                                                                                                                                                                                                                       | `dialog`                                 | Has     | -   |
| [`contextMenu`](<https://developer.apple.com/documentation/swiftui/view/contextmenu(menuitems:)>), [`Menu`](https://developer.apple.com/documentation/swiftui/menu), [`ControlGroup`](https://developer.apple.com/documentation/swiftui/controlgroup)                                                                                                                                                                                                                                                                                                                                                                                                        | `menu_button`, `action_strip`            | Has     | -   |
| [`Form`](https://developer.apple.com/documentation/swiftui/form), [`LabeledContent`](https://developer.apple.com/documentation/swiftui/labeledcontent), [`GroupBox`](https://developer.apple.com/documentation/swiftui/groupbox)                                                                                                                                                                                                                                                                                                                                                                                                                             | `form_row`, `Group`                      | Has     | -   |
| [`DisclosureGroup`](https://developer.apple.com/documentation/swiftui/disclosuregroup), [`OutlineGroup`](https://developer.apple.com/documentation/swiftui/outlinegroup)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | expander row, `tree_view`                | Has     | -   |
| [`swipeActions`](<https://developer.apple.com/documentation/swiftui/view/swipeactions(edge:allowsfullswipe:content:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | none                                     | None    | P2  |
| [`textSelection`](<https://developer.apple.com/documentation/swiftui/view/textselection(_:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | `select_text`                            | Has     | -   |
| [`help`](<https://developer.apple.com/documentation/swiftui/view/help(_:)>)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | `tooltip`                                | Has     | -   |
| [TipKit](https://developer.apple.com/documentation/tipkit)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | none                                     | None    | P2  |
| [`FocusState`](https://developer.apple.com/documentation/swiftui/focusstate), [`defaultFocus`](<https://developer.apple.com/documentation/swiftui/view/defaultfocus(_:_:priority:)>), [`focusSection`](<https://developer.apple.com/documentation/swiftui/view/focussection()>)                                                                                                                                                                                                                                                                                                                                                                              | `FocusCore`, F6 regions                  | Partial | P1  |
| [`dynamicTypeSize`](<https://developer.apple.com/documentation/swiftui/view/dynamictypesize(_:)>), [`ScaledMetric`](https://developer.apple.com/documentation/swiftui/scaledmetric)                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `BINGUX_TEXT_SCALE`                      | Partial | P0  |
| [`accessibilityReduceMotion`](https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilityreducemotion), [`accessibilityReduceTransparency`](https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilityreducetransparency), [`colorSchemeContrast`](https://developer.apple.com/documentation/swiftui/environmentvalues/colorschemecontrast), [`accessibilityDifferentiateWithoutColor`](https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilitydifferentiatewithoutcolor), [`legibilityWeight`](https://developer.apple.com/documentation/swiftui/environmentvalues/legibilityweight) | `Tokens::reduced_motion`                 | Partial | P0  |
| [`phaseAnimator`](<https://developer.apple.com/documentation/swiftui/view/phaseanimator(_:content:animation:)>), [`keyframeAnimator`](<https://developer.apple.com/documentation/swiftui/view/keyframeanimator(initialvalue:trigger:content:keyframes:)>), [`matchedGeometryEffect`](<https://developer.apple.com/documentation/swiftui/view/matchedgeometryeffect(id:in:properties:anchor:issource:)>), [`scrollTransition`](<https://developer.apple.com/documentation/swiftui/view/scrolltransition(_:axis:transition:)>)                                                                                                                                 | tween tokens, `icon_morph`               | Partial | P2  |
| [`Chart`](https://developer.apple.com/documentation/charts/chart) ([Charts](https://developer.apple.com/documentation/charts))                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `charts` module                          | Partial | P1  |
| [MapKit](https://developer.apple.com/documentation/mapkit)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | none                                     | None    | P2  |
| [`VideoPlayer`](https://developer.apple.com/documentation/avkit/videoplayer) (AVKit)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | `preview::viewers::media`                | Partial | P0  |
| [PDFKit](https://developer.apple.com/documentation/pdfkit)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | `preview::viewers::pdf`                  | Partial | P1  |
| [`PhotosPicker`](https://developer.apple.com/documentation/photosui/photospicker) (PhotosUI)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `files::chooser` with a type filter      | Partial | P2  |
| [`Canvas`](https://developer.apple.com/documentation/swiftui/canvas), [`TimelineView`](https://developer.apple.com/documentation/swiftui/timelineview)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Skia drawing in `charts`                 | Partial | P2  |
| [`MenuBarExtra`](https://developer.apple.com/documentation/swiftui/menubarextra)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | shell panel (QML)                        | N/A     | -   |

SwiftUI's accessibility modifiers are a second catalogue. Section 4.3 maps them to AccessKit.

### 3.3 GNOME HIG and libadwaita

GNOME is a Linux reference, not a dependency. Sources: [GNOME HIG][gnome-hig] and the [libadwaita widget gallery][adw-gallery].

| Pattern or widget                                                                                                             | What it gives                                                 | bingux-ui                                                     | Pri |
| ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------- | --- |
| [Shortcuts dialog][gnome-shortcuts]                                                                                           | Auto-listed shortcuts, `Ctrl+?`, standard key table           | About only (A6)                                               | P1  |
| [Selection and edit modes][gnome-hig]                                                                                         | A mode for choosing several items, listed under Containers    | None (C3)                                                     | P1  |
| [List and column views][gnome-list]                                                                                           | Same model as grid views; headers sort                        | Partial (C1)                                                  | P0  |
| [Grid views][gnome-grid]                                                                                                      | Unique thumbnail, most recent first                           | `file_grid`                                                   | -   |
| [Utility panes][gnome-utility]                                                                                                | Side pane, `F9` toggle, left or right                         | None (B3)                                                     | P1  |
| [Search][gnome-search]                                                                                                        | Search versus find; search bar; type to search                | Partial (B8)                                                  | P1  |
| [Placeholder pages][gnome-placeholder]                                                                                        | Illustration or symbolic placeholder                          | `status_page`                                                 | -   |
| [Notifications][gnome-notif] and [toasts][gnome-toasts]                                                                       | System notification versus in-app toast with Undo             | Toast only (A8)                                               | P1  |
| [Dialogs][gnome-dialogs]                                                                                                      | Alert dialog with one to three buttons; confirmation pattern  | `dialog`                                                      | -   |
| [Keyboard][gnome-kbd]                                                                                                         | Shortcut keys, access keys, navigation keys                   | No access keys (G9)                                           | P2  |
| [Adaptive][gnome-adaptive]                                                                                                    | Breakpoints, from phone width up                              | Has (`SizeClass`)                                             | -   |
| `AdwToggleGroup`, `AdwCarousel`, `AdwBottomSheet`, `AdwTabOverview`, `AdwWrapBox`, `AdwSpinRow`, `AdwEntryRow`, `AdwComboRow` | Widgets in the [gallery][adw-gallery]                         | Entry and combo rows via `form_row`; no carousel, no overview | P2  |
| [`AdwStyleManager`][adw-style]                                                                                                | Exposes colour scheme, accent colour and a high-contrast flag | Reads `gsettings`; no contrast                                | P0  |

### 3.4 What Material 3, Fluent 2, Carbon, Primer and Polaris add

Only items beyond Apple's coverage are listed.

| System                                                         | Adds                                                                                            | bingux-ui                            | Pri |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------ | --- |
| [Carbon data table][carbon-dt]                                 | Batch action bar, five row heights, expandable rows, skeleton state, pagination                 | Files columns only (C1)              | P0  |
| [Carbon notifications][carbon-notif]                           | Seven types by disruption: inline, toast, actionable, callout, banner, panel, modal             | Four of them (E1)                    | P1  |
| [Carbon empty states][carbon-empty]                            | A design checklist: what the page looks like with no content, and the steps a user can take     | `status_page`; no checklist          | P2  |
| [Carbon spacing][carbon-spacing] and [content][carbon-content] | Spacing scale and content standards as a system                                                 | `Space` tokens; `copy-lint.py`       | -   |
| [Primer saving][primer-saving]                                 | Do not mix auto-saving and explicit controls in one form; keep data on a failed save            | `pending_bar` (E4)                   | P1  |
| [Primer degraded experiences][primer-degraded]                 | Render a page without the failed secondary part; error page only if the primary part fails      | None (E2)                            | P2  |
| [Primer loading][primer-loading]                               | Skeletons, determinate and indeterminate indicators, subtle motion                              | `skeleton`, `progress`               | -   |
| [Primer notification messaging][primer-notif]                  | Banner versus inline message by purpose: system, feedback, awareness                            | Partial (E1)                         | P1  |
| [Primer progressive disclosure][primer-disclosure]             | Icon rules: chevron, fold, ellipsis, kebab                                                      | Expander row; no icon rules          | P2  |
| [Fluent 2 components][fluent-comps]                            | `Tag`, `Tag picker`, `Persona`, `Message bar`, `Info label`, `Toolbar`, `Tree`, `Overflow`      | Tags only in Files chips; no persona | P2  |
| [Fluent 2 tokens][fluent-tokens]                               | Global and alias tokens with themes                                                             | One `Tokens` struct (G6)             | P2  |
| [Material 3 canonical layouts][m3-layouts]                     | List-detail, supporting pane and feed as named layouts                                          | `master_detail`; no supporting pane  | P2  |
| [Material 3 motion tokens][m3-motion]                          | Easing and duration pairs; spring tokens in the motion physics system                           | Tween tokens only (G5)               | P2  |
| [Polaris index table][polaris-index]                           | Rows act on click; sort and filter on long lists; paginate over 50; verb-plus-noun bulk actions | Fits C1 and the copy rules           | P0  |
| [Polaris content fundamentals][polaris-content]                | Content standards per surface                                                                   | `copy-lint.py`                       | -   |

Fluent's [component list][fluent-comps] and Primer's [component index][primer-comps] are also useful as completeness checks
when planning the taxonomy.

## 4. Accessibility gap list

### 4.1 Bingux today, from the source

- Every colour pair is measured. Ratios are 4.5:1 for text and 3:1 for icons and control edges (`docs/bingux-ui.md`,
  "Accessibility and contrast").
- Focus ring: 2 px line, 2 px gap, 3:1 against every layer. One Tab stop per control.
- Inspector check "Target size" fails below 24 px (`MIN_TARGET` in `inspector/a11y_checks.rs`). Tokens are 36 px regular,
  44 px compact for controls.
- `BINGUX_TEXT_SCALE=2` and `BINGUX_PSEUDO_LOCALE=expand` stress layout. Neither is a user setting.
- The Settings app has Large text, High contrast and Reduce animations switches (`docs/settings.md`, "Accessibility"). They
  write GNOME keys. `load_tokens` in `theme.rs` reads only the accent and scheme from `gsettings`, and reduced motion from
  `BINGUX_REDUCED_MOTION`. The switches do not reach bingux-ui widgets (A7, G1, G2).
- `gsettings` is a GNOME service. Gnoblin's Rust compositor must not assume it. The Settings portal defines `contrast` and
  `reduced-motion` under [`org.freedesktop.appearance`][portal-settings], but Bingux's portal serves only Screenshot today.
  Bingux needs its own source with the portal and `gsettings` as optional fallbacks.

### 4.2 WCAG 2.2 criteria for native apps

[WCAG2ICT][wcag2ict] explains how to read WCAG for software that is not a web page. The table lists the AA criteria and the
two A criteria added in 2.2 that bear on native apps. "State" is bingux-ui today.

| SC                                                                                                                                                         | Level | What a native app must do                                         | State in bingux-ui                                                     | Pri |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ----------------------------------------------------------------- | ---------------------------------------------------------------------- | --- |
| [1.1.1 Non-text content](https://www.w3.org/TR/WCAG22/#non-text-content)                                                                                   | A     | Name every icon-only control                                      | `a11y_alt` on 57 sites; inspector Naming check                         | -   |
| [1.3.1 Info and relationships](https://www.w3.org/TR/WCAG22/#info-and-relationships)                                                                       | A     | Expose headings, lists, tables, groups and error links            | No `Role::Heading`, `Table`, `Row`, `Cell`; no `described_by`          | P0  |
| [1.4.1 Use of colour](https://www.w3.org/TR/WCAG22/#use-of-color)                                                                                          | A     | Not colour alone for state or series                              | State icons present; chart series rely on a palette [INFERENCE]        | P1  |
| [1.4.3 Contrast (minimum)](https://www.w3.org/TR/WCAG22/#contrast-minimum)                                                                                 | AA    | 4.5:1 text, 3:1 large                                             | Measured and enforced                                                  | -   |
| [1.4.4 Resize text](https://www.w3.org/TR/WCAG22/#resize-text)                                                                                             | AA    | Text to 200 % without loss                                        | Layout survives 200 % in tests; no user setting                        | P0  |
| [1.4.11 Non-text contrast](https://www.w3.org/TR/WCAG22/#non-text-contrast)                                                                                | AA    | 3:1 for controls and focus                                        | Measured and enforced                                                  | -   |
| [1.4.12 Text spacing](https://www.w3.org/TR/WCAG22/#text-spacing)                                                                                          | AA    | Survive line height 1.5, paragraph spacing 2, letter spacing 0.12 | Not tested. Pseudo-locale expands strings only                         | P1  |
| [1.4.13 Content on hover or focus](https://www.w3.org/TR/WCAG22/#content-on-hover-or-focus)                                                                | AA    | Dismissible, hoverable, persistent                                | Tooltips go down on scroll; Escape and hover-to-keep not verified      | P1  |
| [2.1.1 Keyboard](https://www.w3.org/TR/WCAG22/#keyboard)                                                                                                   | A     | Every function by keyboard                                        | Strong. Gaps: column resize, split divider [INFERENCE]                 | P1  |
| [2.1.4 Character key shortcuts](https://www.w3.org/TR/WCAG22/#character-key-shortcuts)                                                                     | A     | Single-key shortcuts only when the component has focus            | Preview keys (`K`, `J`, `L`, `M`) act only while the pane has the keys | -   |
| [2.2.1 Timing adjustable](https://www.w3.org/TR/WCAG22/#timing-adjustable)                                                                                 | A     | No unextendable timeouts                                          | Toasts auto-dismiss; no pause on focus (E6)                            | P1  |
| [2.4.3 Focus order](https://www.w3.org/TR/WCAG22/#focus-order)                                                                                             | A     | Order follows meaning                                             | Visual order, F6 regions                                               | -   |
| [2.4.7 Focus visible](https://www.w3.org/TR/WCAG22/#focus-visible)                                                                                         | AA    | Visible indicator                                                 | Ring on every control; `focus-ring-check.py`                           | -   |
| [2.4.11 Focus not obscured (minimum)](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)                                                            | AA    | A focused item is not fully hidden by other content               | Status pill, toasts and sticky headers can cover rows; not checked     | P1  |
| [2.4.13 Focus appearance](https://www.w3.org/TR/WCAG22/#focus-appearance)                                                                                  | AAA   | Area of a 2 px perimeter, 3:1 change                              | 2 px ring at 3:1; formula not tested                                   | P2  |
| [2.5.1 Pointer gestures](https://www.w3.org/TR/WCAG22/#pointer-gestures)                                                                                   | A     | Single-pointer alternative to path or multi-point gestures        | Pinch has `Ctrl` plus and minus; swipe back has Back                   | -   |
| [2.5.2 Pointer cancellation](https://www.w3.org/TR/WCAG22/#pointer-cancellation)                                                                           | A     | Activate on up; allow abort                                       | Not verified                                                           | P1  |
| [2.5.3 Label in name](https://www.w3.org/TR/WCAG22/#label-in-name)                                                                                         | A     | Visible label is in the accessible name                           | Presence checked; match not checked                                    | P2  |
| [2.5.7 Dragging movements](https://www.w3.org/TR/WCAG22/#dragging-movements)                                                                               | AA    | Single-pointer alternative to any drag                            | Sortable has grips with keys; column resize and split drag do not      | P1  |
| [2.5.8 Target size (minimum)](https://www.w3.org/TR/WCAG22/#target-size-minimum)                                                                           | AA    | 24 by 24 CSS px, with a spacing exception                         | 24 px check; larger tokens                                             | -   |
| [3.2.6 Consistent help](https://www.w3.org/TR/WCAG22/#consistent-help)                                                                                     | A     | Help in the same place on every page                              | No help surface (B10)                                                  | P2  |
| [3.3.1 Error identification](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3 Error suggestion](https://www.w3.org/TR/WCAG22/#error-suggestion) | A     | Name the field and say what to fix                                | Visible error ring and text; no `error_message` relation               | P0  |
| [4.1.2 Name, role, value](https://www.w3.org/TR/WCAG22/#name-role-value)                                                                                   | A     | Role, name, state and value for each control                      | Many roles set; see 4.4                                                | P0  |
| [4.1.3 Status messages](https://www.w3.org/TR/WCAG22/#status-messages)                                                                                     | AA    | Announce without taking focus                                     | `Role::Status` on 8 sites, `Role::Alert` on 3                          | -   |

### 4.3 WAI-ARIA Authoring Practices patterns

[Patterns][apg-patterns] and the [keyboard interface][apg-keyboard] page give the key contract a screen reader user
expects. The roles come from the source.

| Pattern                      | Contract                                                         | bingux-ui                                                        | Pri |
| ---------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------- | --- |
| Tabs, Radio group            | Roving focus; arrows, Home, End                                  | Has                                                              | -   |
| Tree view                    | Arrows, Enter, type-ahead, expand siblings                       | Has the first three                                              | P2  |
| Listbox                      | Arrows, Shift and Ctrl selection, type-ahead                     | Files list has it                                                | -   |
| Menu and menubar             | Arrows, Home, End, type-ahead, Escape                            | Menus have it. No menubar                                        | P1  |
| Combobox                     | Focus stays in the field; the list item is the active descendant | `active_descendant` is never set [INFERENCE]                     | P1  |
| Dialog (modal), Alert dialog | Focus trap; Escape closes; focus returns                         | Has                                                              | -   |
| Slider, Spinbutton           | Arrows; Page keys by ten; Home, End                              | Has                                                              | -   |
| Slider (multi-thumb)         | Each thumb is a stop                                             | None (D5)                                                        | P2  |
| Toolbar                      | One stop; arrows move between items                              | Tab walks each item. Differs                                     | P1  |
| Grid, Treegrid, Table        | Cell navigation; Ctrl+Home, Ctrl+End; row and column counts      | None (C1)                                                        | P0  |
| Window splitter              | Arrows resize; Enter collapses                                   | Drag only [INFERENCE]                                            | P1  |
| Accordion, Disclosure        | Enter and Space toggle                                           | Expander rows                                                    | -   |
| Breadcrumb                   | Navigation landmark; current page marked                         | `Role::Navigation`; current page mark not set                    | P2  |
| Meter                        | `Role::Meter` with value and range                               | `usage_meter` has no meter role                                  | P2  |
| Landmarks                    | Banner, navigation, main, search, complementary                  | One `Navigation`; F6 regions are the native substitute           | P1  |
| Carousel, Feed               | Rotation control; paging                                         | None                                                             | P2  |
| Tooltip                      | Shown on focus and hover; Escape                                 | Tooltips exist; focus trigger not verified                       | P1  |
| Link                         | Link role                                                        | No `Role::Link` in the source; links read as buttons [INFERENCE] | P1  |

### 4.4 AccessKit roles, properties and actions

bingux-ui pins `accesskit` 0.24 and drives it through Freya's `a11y_*` builders. The schema is in the
[AccessKit repository][accesskit]; the 0.24 [roles][accesskit-role], [actions][accesskit-action] and [node properties][accesskit-node]
are the checklist below. Counts are from a search of `crates/ui/src`.

| Item       | Set by bingux-ui                                                                                                                                                                                                                                                                                                             | Not set                                                                                                                                                                                                                                                                                                                                          |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Roles      | `Button`, `Status`, `ProgressIndicator`, `Tab`, `TabList`, `Slider`, `Dialog`, `AlertDialog`, `Alert`, `MenuItem`, `ListBoxOption`, `ListBox`, `List`, `Tree`, `TreeItem`, `Switch`, `CheckBox`, `RadioButton`, `ComboBox`, `SpinButton`, `SearchInput`, `Splitter`, `Navigation`, `ColumnHeader`, `Image`, `Label`, `Group` | `Heading`, `Table`, `Row`, `Cell`, `Grid`, `Toolbar`, `Menu`, `MenuBar`, `Meter`, `Link`, `Document`, `Main`, `Search`, `Complementary`, `Tooltip`, `ScrollBar`, `Article`, `Application`                                                                                                                                                        |
| Properties | `label`, `value`, `description`, `toggled`, `expanded`, `selected`, `hidden`, `numeric_value` with min, max and step, `live`, `modal`, `size_of_set`, `position_in_set`, `class_name`, `author_id`                                                                                                                           | `described_by`, `labelled_by`, `error_message`, `invalid`, `required`, `read_only`, `has_popup`, `busy`, `level`, `orientation`, `sort_direction`, `row_count`, `column_count`, `row_index`, `keyboard_shortcut`, `access_key`, `role_description`, `state_description`, `active_descendant`, `controls`, `details`, `flow_to`, `custom_actions` |
| Text       | Inline text from Freya                                                                                                                                                                                                                                                                                                       | Character positions and lengths for Orca's character and word reading [INFERENCE]                                                                                                                                                                                                                                                                |
| Actions    | Whatever Freya's adapter routes                                                                                                                                                                                                                                                                                              | Verify `Click`, `Focus`, `Increment`, `Decrement`, `SetValue`, `Expand`, `Collapse`, `ScrollIntoView`, `ShowContextMenu`, `SetTextSelection`, `CustomAction` per control                                                                                                                                                                         |

Suggested API: a typed `Semantics` builder so apps never touch `accesskit` directly.

```rust
// One place that sets name, relations and state for AT.
text_field(state)
    .semantics(Semantics::new().labelled_by(title_id).described_by(hint_id).invalid(Some(error_id)));
data_table(rows, columns).semantics(Semantics::grid()); // sets Table, Row, Cell, counts, sort
```

SwiftUI modifier to AccessKit map, for people who know the Apple names:

| SwiftUI                                                                                                                                                                                                                                                                                                                                     | AccessKit                                                 | bingux-ui today        |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ---------------------- |
| [`accessibilityLabel`](<https://developer.apple.com/documentation/swiftui/view/accessibilitylabel(_:)>)                                                                                                                                                                                                                                     | `label`                                                   | `a11y_alt`             |
| [`accessibilityValue`](<https://developer.apple.com/documentation/swiftui/view/accessibilityvalue(_:)>), [`accessibilityHint`](<https://developer.apple.com/documentation/swiftui/view/accessibilityhint(_:)>)                                                                                                                              | `value`, `description`                                    | `value`, `description` |
| [`accessibilityHeading`](<https://developer.apple.com/documentation/swiftui/view/accessibilityheading(_:)>)                                                                                                                                                                                                                                 | `Role::Heading`, `level`                                  | None                   |
| [`accessibilityElement(children:)`](<https://developer.apple.com/documentation/swiftui/view/accessibilityelement(children:)>)                                                                                                                                                                                                               | role and `children`                                       | Partial                |
| [`accessibilityHidden`](<https://developer.apple.com/documentation/swiftui/view/accessibilityhidden(_:)>)                                                                                                                                                                                                                                   | `hidden`                                                  | Has (`AnimatedNumber`) |
| [`accessibilityAction`](<https://developer.apple.com/documentation/swiftui/view/accessibilityaction(named:_:)>), [`adjustableAction`](<https://developer.apple.com/documentation/swiftui/view/accessibilityadjustableaction(_:)>), [`scrollAction`](<https://developer.apple.com/documentation/swiftui/view/accessibilityscrollaction(_:)>) | `custom_actions`, `Increment`, `Decrement`, `ScrollUp`... | None found             |
| [`accessibilityRotor`](<https://developer.apple.com/documentation/swiftui/view/accessibilityrotor(_:entries:)>), [`accessibilityLinkedGroup`](<https://developer.apple.com/documentation/swiftui/view/accessibilitylinkedgroup(id:in:)>)                                                                                                    | Landmarks, headings, `flow_to`                            | None                   |
| [`accessibilitySortPriority`](<https://developer.apple.com/documentation/swiftui/view/accessibilitysortpriority(_:)>)                                                                                                                                                                                                                       | Tree order                                                | None                   |
| [`accessibilityCustomContent`](<https://developer.apple.com/documentation/swiftui/view/accessibilitycustomcontent(_:_:importance:)>)                                                                                                                                                                                                        | `description`, `details`                                  | None                   |
| [`accessibilityChartDescriptor`](<https://developer.apple.com/documentation/swiftui/view/accessibilitychartdescriptor(_:)>)                                                                                                                                                                                                                 | Table of values behind the chart                          | None (F1)              |
| [`accessibilityInputLabels`](<https://developer.apple.com/documentation/swiftui/view/accessibilityinputlabels(_:)>)                                                                                                                                                                                                                         | Alternate names for voice control                         | None                   |
| [`accessibilityFocused`](<https://developer.apple.com/documentation/swiftui/view/accessibilityfocused(_:)>)                                                                                                                                                                                                                                 | `Focus` action                                            | None                   |
| [`accessibilityRepresentation`](<https://developer.apple.com/documentation/swiftui/view/accessibilityrepresentation(representation:)>)                                                                                                                                                                                                      | A standard control behind a custom drawing                | None                   |

### 4.5 Linux: AT-SPI and Orca

- AccessKit's Linux adapter is [`accesskit_unix`][accesskit-unix], which speaks AT-SPI over D-Bus. Its latest release
  depends on `accesskit` 0.25, while bingux-ui pins 0.24. Check Freya's pin before an upgrade [INFERENCE: the adapter used by
  Freya's winit path was not read].
- Orca needs the compositor, not only the app. KWin ships an `a11ymanager` for Orca's key grabs and had to fix stuck
  modifiers ([KWin commit][kwin-a11y]). Gnoblin's Rust plan lists a `gnoblin-a11y` crate with the keyboard monitor and an
  "Orca key grabs" lane (`design/rust-compositor.md`, `origin/rust-compositor`). Tie the app checks below to that lane.
- The [at-spi2-core design notes][atspi-protocol] and the [freedesktop AT-SPI2 page][atspi-wiki] describe the bus. A real
  Orca run on a Wayland session is the only proof that names, roles, focus changes and live regions are spoken.
- A new app needs three things from the toolkit: the accessible tree, focus events when Tab moves, and text selection and
  caret events in editable text. The last matters for the text editor and for search fields.
- GNOME's own advice is short: every element has a [short, descriptive accessible name][gnome-a11y] and the app is tested
  with high contrast, large text and reduced motion on.

### 4.6 Targets, focus, text, colour, motion

| Topic            | Reference                                                                            | bingux-ui                                     | Pri |
| ---------------- | ------------------------------------------------------------------------------------ | --------------------------------------------- | --- |
| Target size      | Apple: default 28, minimum 20 on macOS ([HIG][hig-a11y]). WCAG: 24 px                | 36 and 44 px controls; 24 px check            | -   |
| Control spacing  | Apple: about 12 pt round bezelled items ([HIG][hig-a11y])                            | Space tokens; no spacing rule                 | P2  |
| Focus appearance | Apple: focus ring in fields, highlight in lists ([HIG][hig-focus]). WCAG 2.4.13      | Ring everywhere; lists use the same ring      | P2  |
| Text size        | Apple: allow 200 % ([HIG][hig-a11y]); defaults 13 pt macOS, 17 pt iOS                | `TextSize::Md` 14 px. No user scale           | P0  |
| Text spacing     | WCAG 1.4.12                                                                          | Not tested                                    | P1  |
| Contrast         | Apple: Increase Contrast variant when base is under 4.5:1 ([HIG][hig-a11y])          | Meets AA; no variant                          | P0  |
| Colour alone     | Apple: shapes or icons as well ([HIG][hig-a11y])                                     | Mostly met; charts unclear                    | P1  |
| Motion           | Apple: Reduce Motion replaces slides with fades, no blur animation ([HIG][hig-a11y]) | Instant under the flag. No cross-fade variant | P2  |
| Time-boxed UI    | Apple: avoid auto-dismiss ([HIG][hig-a11y])                                          | Toasts auto-dismiss                           | P1  |
| Captions         | Apple: captions, subtitles, audio description, transcripts ([HIG][hig-a11y])         | `.srt` and `.vtt` sidecar in preview          | P1  |
| Auto-play        | Apple: never autoplay without controls ([HIG][hig-a11y])                             | Preview pane never plays by itself            | -   |
| Assistive Access | Simplified mode with two-step confirmation of risky actions ([HIG][hig-a11y])        | None                                          | P2  |

### 4.7 Verification plan

1. Add a tree snapshot test per component in the taxonomy: role, name, value, state and relations.
2. Add an inspector check for headings, form errors and unlabelled groups (extend `IssueKind::Naming`).
3. Run Orca on a Gnoblin session for Settings and Files. Record what is spoken for each page and keep the notes in the repo.
4. Run each app with the four preferences on: large text, high contrast, reduced motion, reduced transparency.
5. Add a Text spacing stress mode next to `BINGUX_TEXT_SCALE` and `BINGUX_PSEUDO_LOCALE`.

## 5. File managers

Reference systems: [Finder][finder-use], [GNOME Files][gnome-files], [Dolphin][dolphin-handbook] and [File Explorer][win-fe].
Status is for Bingux Files, from `docs/files.md` and `crates/ui/src/files/`. Finder rows cite Apple Support pages.

### 5.1 Navigation

| Feature                           | Reference                                                      | Bingux Files                                             | Pri |
| --------------------------------- | -------------------------------------------------------------- | -------------------------------------------------------- | --- |
| Back, Forward, Up, history menus  | All four                                                       | **Has**, 450 ms hold menu, Alt keys                      | -   |
| Editable path, completion         | Nautilus `Ctrl+L` ([help][gnome-path]); Explorer `Alt+D`       | **Has**, with completion card                            | -   |
| Crumb path bar                    | Finder path bar, Dolphin breadcrumbs ([bar][dolphin-location]) | **Has**, with elision (`design/path-elision.md`)         | -   |
| Crumbs as drop targets            | Finder, Dolphin                                                | **None**. Drops go to list rows, sidebar places and tabs | P1  |
| Tabs, tear off, reopen            | Finder, Dolphin, Explorer (`Ctrl+T`)                           | **Has**                                                  | -   |
| Restore tabs of last session      | Finder, Dolphin                                                | **None** (listed in "Not done")                          | P1  |
| Split view (two panes)            | Dolphin ([view][dolphin-view])                                 | **None**                                                 | P2  |
| New window from a menu            | All                                                            | **None** (listed in "Not done")                          | P2  |
| Sidebar: places, drives, Trash    | All                                                            | **Has**, with menus and free-space bars                  | -   |
| Sidebar: customise built-in items | Finder ([sidebar][finder-use])                                 | **None**                                                 | P2  |
| Pin to quick access               | Explorer ([Quick access][win-fe])                              | **Has** as bookmarks                                     | -   |
| Recents                           | All                                                            | **Has**                                                  | -   |
| Network, servers, MTP             | Nautilus ([Other Locations][gnome-connect])                    | **None** (listed in "Not done")                          | P1  |
| `FileManager1` D-Bus              | Nautilus, Dolphin                                              | **None**; callers fall back to `xdg-open`                | P1  |

### 5.2 Selection and views

| Feature                              | Reference                                                       | Bingux Files                                                       | Pri |
| ------------------------------------ | --------------------------------------------------------------- | ------------------------------------------------------------------ | --- |
| Rubber band, Ctrl and Shift click    | All                                                             | **Has**; autoscroll while dragging                                 | -   |
| Select all, invert                   | All                                                             | **Has** (`Invert selection`)                                       | -   |
| Select by pattern                    | Nautilus `Ctrl+S` ([help][gnome-select])                        | **None**                                                           | P2  |
| Toggle with `Ctrl+Space`             | Dolphin ([keys][dolphin-view]), Explorer                        | Not verified                                                       | P2  |
| Type to select                       | Finder, Explorer                                                | **Partial.** Typing starts a search; jump only where search is off | P1  |
| Filter bar                           | Dolphin `Ctrl+I` ([tips][dolphin-tips])                         | **Partial.** Search chips cover it                                 | -   |
| List and grid, zoom                  | All                                                             | **Has**, per-folder memory                                         | -   |
| Column (Miller) view                 | Finder ([views][finder-use])                                    | **None**                                                           | P2  |
| Gallery view                         | Finder ([views][finder-use])                                    | **None**                                                           | P2  |
| Group by kind or date                | Finder ([sort and group][finder-sort]); Explorer                | **None**                                                           | P1  |
| Columns: show, hide, reorder, resize | All                                                             | **Has**; double-click fits                                         | -   |
| Folders first, natural sort          | Finder ([keep folders on top][finder-sort])                     | **Has** (`folders_first`, natural order)                           | -   |
| Show file extensions toggle          | Finder Settings ([settings][finder-settings]); [HIG][hig-files] | **None**                                                           | P1  |
| Compact density                      | Explorer ([Compact view][win-fe])                               | **None** (G4)                                                      | P2  |
| Hover detail card                    | Finder, Explorer                                                | **Has** (700 ms)                                                   | -   |
| Show hidden, `Ctrl+H`                | All                                                             | **Has**                                                            | -   |

### 5.3 Operations

| Feature                                     | Reference                                                                                         | Bingux Files                                         | Pri |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | --- |
| Copy, cut, paste; Ctrl copies drag          | All                                                                                               | **Has**                                              | -   |
| Move with paste (Finder `Option-Command-V`) | Finder ([cut and paste][finder-cutpaste])                                                         | **Has** as cut and paste                             | -   |
| Create a link while dragging                | Explorer `Alt`+drag ([keys][win-keys])                                                            | **None**                                             | P2  |
| Conflict dialog                             | Finder: Keep Both, Stop, Replace, Merge ([merge][finder-merge])                                   | **Has**: Skip, Keep both, Replace, "do this for all" | -   |
| Merge folders                               | Finder `Option`                                                                                   | **None** found                                       | P1  |
| Progress with cancel                        | All                                                                                               | **Has**: busy line, cancel                           | -   |
| Job list with ETA and pause                 | Nautilus, Dolphin, Explorer                                                                       | **None**                                             | P1  |
| Undo of file operations                     | Dolphin: "Undo: Rename" ([tips][dolphin-tips]); Nautilus toast ([delete][gnome-delete])           | **Has** (`Ctrl+Z`, named); **no Redo**               | P0  |
| Trash, restore, empty                       | All                                                                                               | **Has**, very thorough                               | -   |
| Inline rename, stem selected                | All                                                                                               | **Has** (`stem_chars`)                               | -   |
| Batch rename                                | Finder ([rename][finder-rename]); Nautilus ([help][gnome-rename]); Dolphin ([tips][dolphin-tips]) | **None**                                             | P1  |
| Duplicate                                   | Finder `Command-D` ([shortcuts][mac-keys])                                                        | **None**                                             | P2  |
| New file from template                      | Nautilus ([templates][gnome-templates])                                                           | **Has** (XDG Templates)                              | -   |
| Compress, extract                           | Finder ([zip][finder-use]); all                                                                   | **Has**, beyond the references (AES, volumes, media) | -   |
| Open with, default app                      | Nautilus ([open][gnome-open])                                                                     | **Has**; "View all apps" not verified                | P2  |
| Send to, share                              | Nautilus ([share][gnome-share]); Finder AirDrop                                                   | **None** (A9)                                        | P2  |
| Open in terminal                            | Dolphin Terminal panel ([panels][dolphin-panels])                                                 | **None**                                             | P2  |
| Compare two files                           | Dolphin ([tips][dolphin-tips])                                                                    | **None**                                             | P2  |
| Copy path                                   | Explorer, Nautilus                                                                                | **Has**                                              | -   |

### 5.4 Preview, metadata and search

| Feature                            | Reference                                                             | Bingux Files                            | Pri |
| ---------------------------------- | --------------------------------------------------------------------- | --------------------------------------- | --- |
| Quick Look on Space                | Finder ([Quick Look][finder-ql]); Nautilus ([preview][gnome-preview]) | **Has**, with arrows choosing the file  | -   |
| Quick Look: rotate, markup, trim   | Finder ([Quick Look][finder-ql])                                      | **None** (F7)                           | P2  |
| Quick Look full screen             | Finder; Nautilus `F`                                                  | **Has** (`F`)                           | -   |
| Docked preview or details pane     | Explorer `Alt+P` ([keys][win-keys]); Finder                           | **Has** (`Alt+P`, `F3`)                 | -   |
| Properties with permissions, hash  | Finder Get Info; Nautilus                                             | **Has**                                 | -   |
| Tags and colour labels             | Finder ([tags][finder-tags])                                          | **None**                                | P1  |
| Smart folders, saved searches      | Finder ([smart folder][finder-smart])                                 | **None** (listed in `docs/files.md`)    | P1  |
| Search with typed filters          | Finder, Dolphin                                                       | **Has**, beyond the references          | -   |
| Content search                     | Dolphin ([tips][dolphin-tips]); Spotlight                             | **None** (names only)                   | P2  |
| Quick actions in the preview pane  | Finder ([quick actions][finder-actions])                              | **Partial** via the menu                | P2  |
| Thumbnails for video and documents | All                                                                   | **Partial.** Cache only for those types | P1  |

### 5.5 Integration

| Feature                             | Reference                                 | Bingux Files               | Pri |
| ----------------------------------- | ----------------------------------------- | -------------------------- | --- |
| Drag to and from other apps         | [HIG][hig-dnd]; all                       | **None** (Freya limit)     | P0  |
| Mount, eject, unmount safely        | All                                       | **Has**, durable writes    | -   |
| Encrypted volumes                   | Nautilus, Dolphin                         | **None**                   | P2  |
| Open from other apps (file chooser) | Portal                                    | **Has** (`files::chooser`) | -   |
| Settings for confirmations          | Dolphin ([confirmations][dolphin-config]) | **None**                   | P2  |

### 5.6 Hidden details a grade A file manager has

These are the small things people feel rather than name. Status is for Bingux Files.

- Spring-loaded folders: hold a drag over a folder or sidebar place and it opens ([HIG][hig-dnd], [Finder tip][macpaw-spring]).
  **None.**
- A count badge and a translucent drag image while dragging many files ([HIG][hig-dnd]). Not verified.
- Escape cancels a drag. **Has** (`view.rs`).
- Autoscroll near the edge while dragging. **Has** (`AUTOSCROLL_EDGE`).
- New folder starts in rename. **Has.** Rename selects the stem only. **Has.**
- Per-folder scroll position and selection survive Back. **Has** (32 folders per tab).
- A copied file keeps its place after a paste: the pasted items end selected ([HIG][hig-dnd] "maintain selection"). Not verified.
- Delete moves the selection to the next row instead of clearing it. Not verified.
- Names are bytes, so a Latin-1 name opens the right folder. **Has**, rare and well done.
- Conflict names ("Keep both") follow one pattern across copy, move and extract. **Has.**
- A dimmed or "cut" look on files waiting to be moved. Not verified.
- Free-space bar on drives. **Has.**
- Stop button that turns into Reload and back without flicker. **Has**, a standout.
- Option-drag a folder onto a same-named folder to merge ([Finder merge][finder-merge]). **None.**
- Middle-click a folder to open it in a tab. **Has** (history menus); folder rows not verified.
- Remember window and column state per folder. **Has.**

## 6. App checklists

Bingux has none of these apps yet. Each table says which bingux-ui parts exist, and what the app would still need from the
SDK. Rows marked "SDK" belong in `crates/ui` by the rule in `AGENTS.md` ("bingux-ui first").

### 6.1 Media player

References: [QuickTime Player][qt-play], Music ([queue][music-queue], [lyrics][music-lyrics], [sound quality][music-sound],
[AutoMix and Crossfade][music-xfade]), [mpv manual][mpv], the [MPRIS specification][mpris], and the HIG pages on
[video][hig-video] and [audio][hig-audio].

| Must have                                                            | Reference                              | bingux-ui today                                        | Pri |
| -------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------------ | --- |
| Play, pause, seek, volume, speed, full screen, keyboard              | QuickTime, mpv                         | **Has** in `preview::viewers::media` (keys documented) | -   |
| MPRIS server: Play, Pause, Seek, SetPosition, OpenUri, metadata, art | [MPRIS Player][mpris]                  | **None** (SDK: `mpris::Session`)                       | P0  |
| Hardware media keys, now-playing controls                            | [MPRIS][mpris], [HIG audio][hig-audio] | **None**                                               | P0  |
| Queue: play next, play later, reorder by drag                        | [Music queue][music-queue]             | `sortable` list; no queue model                        | P1  |
| Library with metadata, albums, artists, playlists                    | Music                                  | **None** (`bingux-search` indexes files)               | P1  |
| Subtitles and captions, with styling                                 | [QuickTime][qt-sub], [HIG][hig-a11y]   | Sidecar `.srt`, `.vtt`, key `C`                        | P1  |
| Subtitle and audio track menus                                       | [QuickTime][qt-sub], [mpv][mpv]        | **None**                                               | P1  |
| Resume where you left off, without asking                            | [HIG video][hig-video]                 | Volume kept; position not documented                   | P1  |
| Original aspect ratio, letterbox                                     | [HIG video][hig-video]                 | Not verified                                           | P1  |
| Picture in picture, mini player                                      | Music MiniPlayer, QuickTime            | **None** (needs compositor support)                    | P1  |
| Empty state with drop target and an open button                      | Context file                           | `status_page`; drop target needs A1                    | P0  |
| Drag files in, drag tracks out                                       | HIG                                    | **None**                                               | P0  |

Hidden details:

- Gapless playback and crossfade or AutoMix between songs ([Apple][music-xfade]).
- A sound enhancer and an equaliser ([Apple][music-sound]).
- Time-synced lyrics that follow the song, with full-screen view ([Apple][music-lyrics]).
- Frame step, A-B loop, audio and subtitle delay, and per-file speed ([mpv][mpv]).
- Seek to the previous or next subtitle, and screenshot of the current frame ([mpv][mpv]).
- Trim a clip in place ([QuickTime][qt-trim]).
- Space on a connected Bluetooth keyboard plays or pauses ([HIG video][hig-video]).
- Output rerouting, and a clear rule for audio interruptions ([HIG audio][hig-audio]).
- Dim flashing lights when the system asks ([HIG][hig-a11y]).

### 6.2 Text editor

References: [TextEdit][te-find] and its [settings][te-settings], [GNOME Text Editor][gte], [Sublime Text][sublime-edit] and
[BBEdit][bbedit-text].

| Must have                                                  | Reference                          | bingux-ui today                          | Pri |
| ---------------------------------------------------------- | ---------------------------------- | ---------------------------------------- | --- |
| Large-file editing buffer, undo and redo, selection, caret | All                                | **None** (SDK: `text_editor`)            | P0  |
| Find and replace with regex, whole word, case, find bar    | [GNOME][gte-search]; TextEdit      | **None**                                 | P0  |
| Autosave, versions, recover unsaved work, session restore  | [GNOME][gte]: "session management" | **None** (B5, B6)                        | P0  |
| Syntax highlighting, line numbers, wrap, tab width         | GNOME, Sublime                     | **None** (preview shows plain monospace) | P1  |
| Inline spell check                                         | GNOME, TextEdit                    | **None**                                 | P1  |
| `.editorconfig` and modelines                              | GNOME                              | **None**                                 | P2  |
| Plain text and rich text                                   | [TextEdit][te-settings]            | **None** (D2)                            | P1  |
| Encoding detection and line ending choice                  | BBEdit, Sublime                    | **None**                                 | P1  |
| Print                                                      | GNOME                              | **None** (A10)                           | P2  |
| Draft folder for new unsaved files                         | [GNOME][gte]                       | **None**                                 | P1  |

Hidden details:

- Multiple selections: add the next match, skip, undo a selection ([Sublime][sublime-edit]).
- Goto Anything for files and symbols ([Sublime][sublime-nav]); a command palette needs A5.
- Text transforms: change case, sort lines, strip or add prefixes, hard wrap ([BBEdit][bbedit-text]).
- Smart quotes, dashes, links and text replacement as settings, with an option to limit smart quotes to rich text ([TextEdit][te-settings]).
- Vim key bindings as an option ([GNOME][gte]).
- Large-text and high-contrast support in the buffer, and Orca reads text by character and word (section 4.4).
- Restore the caret, scroll and undo history on reopen.
- Show unsaved changes with a dot, and warn only when autosave is off ([HIG][hig-files]).

### 6.3 Calculator

References: [mode choice][calc-modes], [scientific][calc-sci], and [unit and currency conversion][calc-convert].

| Must have                                                       | Reference                         | bingux-ui today                                                   | Pri |
| --------------------------------------------------------------- | --------------------------------- | ----------------------------------------------------------------- | --- |
| Basic, scientific, programmer modes with switch keys            | [Apple][calc-modes]               | **None**; buttons and `segmented` exist                           | P0  |
| Expression entry with order of operations and parentheses       | [Apple][calc-sci]                 | **None** (app logic)                                              | P0  |
| History of past results that can be copied and reused           | Common in calculators [INFERENCE] | `sortable`, list rows                                             | P1  |
| Unit and currency conversion, with offline rates cached         | [Apple][calc-convert]             | **None**                                                          | P1  |
| Binary, octal, decimal, hex and bitwise; ASCII and Unicode view | [Apple][calc-modes]               | **None**                                                          | P1  |
| Copy result; paste a number from anywhere                       | Apple                             | `select_text`, clipboard                                          | P0  |
| Full keyboard entry and Orca-readable results                   | WCAG                              | `AnimatedNumber` is a live region; digits-only keypad needs names | P0  |

Hidden details:

- Results update as you type, in both fields of the converter ([Apple][calc-convert]).
- Stored values (memory) in the scientific mode ([Apple][calc-sci]).
- Tabular digits so the result never jitters (bingux-ui `AnimatedNumber` has them).
- Locale decimal and grouping marks, with a real minus sign.

### 6.4 Weather

References: Apple's [Weather User Guide][wx-guide], [WeatherKit attribution][wk-attr], and [Open-Meteo terms][open-meteo].

| Must have                                                                  | Reference                                | bingux-ui today                                | Pri |
| -------------------------------------------------------------------------- | ---------------------------------------- | ---------------------------------------------- | --- |
| Current, hourly (24 h) and 10-day forecast                                 | [Apple][wx-guide]                        | `charts` (line, bar), `AnimatedNumber`, `icon` | P0  |
| Location list, current location, search by name                            | Apple                                    | `omnibox`, `places`-style list; no geocoder    | P0  |
| Severe weather alerts, with official text                                  | Apple ("not in all countries")           | `banner`; no source                            | P1  |
| Weather maps: precipitation, temperature, air quality, wind                | [Apple][wx-guide]                        | **None** (F6)                                  | P2  |
| Details: sunrise and sunset, UV index, air quality, wind, versus yesterday | [Apple][wx-guide]                        | `stat_tile`                                    | P1  |
| Units and locale: temperature, wind, 24-hour clock                         | Apple                                    | `AnimatedNumber::locale`; no units model       | P1  |
| Menu bar or panel glance                                                   | Apple ("weather status in the menu bar") | Shell panel (QML)                              | P2  |
| Notifications for rain and alerts                                          | Apple                                    | **None** (A8)                                  | P1  |
| Data source credit and licence                                             | WeatherKit; Open-Meteo CC BY 4.0         | `about` credits                                | P0  |

Hidden details:

- Say where the data comes from, in the app, as the provider's terms require ([Apple][wk-attr], [Open-Meteo][open-meteo]).
  Open-Meteo's free tier is for non-commercial use with call limits. Check the licence fits the package before choosing.
- Moon phase, and today compared with yesterday ([Apple][wx-guide]).
- A scrub on the hourly chart that reads out values (needs F1 and F2).
- Sunrise and sunset markers on the day chart, and day or night art for the condition.
- Work offline: show the last forecast with its age, never blank (Primer's [degraded pattern][primer-degraded]).
- Never block on location. A portal call can fail; fall back to a typed place (context file).

### 6.5 Preview (PDF and image viewer)

References: Apple's Preview guide ([annotate][pv-annotate], [forms and signatures][pv-forms], [combine][pv-combine],
[page edits][pv-pages]) and the [HIG on file management][hig-files].

| Must have                                                       | Reference                   | bingux-ui today             | Pri |
| --------------------------------------------------------------- | --------------------------- | --------------------------- | --- |
| View PDF: thumbnails, contact sheet, zoom, find, text selection | Preview                     | **Has** in the preview host | -   |
| Annotate: highlight, note, shapes, text, draw                   | [Preview][pv-annotate]      | **None** (F5)               | P1  |
| Fill forms, sign with a signature                               | [Preview][pv-forms]         | **None**                    | P1  |
| Page edits: insert, delete, rotate, reorder by drag             | [Preview][pv-pages]         | **None**                    | P1  |
| Combine PDFs by dragging thumbnails between windows             | [Preview][pv-combine]       | **None**; needs A1          | P1  |
| Image view: zoom, rotate, crop, adjust size, export             | [Preview guide][pv-guide]   | Zoom and rotate in preview  | P1  |
| Autosave, with Duplicate before a destructive edit              | [Preview][pv-combine] warns | **None** (B5)               | P1  |
| Print and Export as PDF                                         | Preview                     | **None** (A10)              | P2  |

Hidden details:

- Edits to a PDF save automatically, so the app warns before a combine and offers Duplicate ([Apple][pv-combine]).
- Annotations stay editable if saved as PDF, and are flattened only on request ([Apple][pv-annotate]).
- Enhanced AutoFill of contact details into form fields ([Apple][pv-forms]).
- Interact with text in a photo, listed in the [Preview guide][pv-guide]. Needs OCR; check the licence.
- Fit width, fit page and 100 % keys exist in the Files preview pane (`9`, `0`, `1`). Reuse them.

### 6.6 Calendar

References: Apple's [Calendar guide][cal-add], and the standards iCalendar (RFC 5545) and CalDAV (RFC 4791) for data.
The shell popout today reads events through Evolution Data Server (`docs/calendar.md`).

| Must have                                                            | Reference        | bingux-ui today                               | Pri |
| -------------------------------------------------------------------- | ---------------- | --------------------------------------------- | --- |
| Day, week, month and year views, with today and keys                 | Apple            | Popout month grid in QML; none in `crates/ui` | P0  |
| Create an event from a sentence, such as "Lunch on Thursday at noon" | [Apple][cal-add] | **None**                                      | P1  |
| Drag in the grid to set start and end; drag to move                  | [Apple][cal-add] | `PointerDrag`; no calendar grid               | P0  |
| Recurrence, exceptions, all-day events, alerts                       | RFC 5545         | Expansion is in the service                   | P0  |
| Multiple calendars, accounts, subscriptions                          | Apple            | Evolution Data Server via D-Bus               | P1  |
| Invitations and replies                                              | Apple            | **None**                                      | P2  |
| Time-zone override and per-event zone                                | [Apple][cal-tz]  | **None**                                      | P1  |
| Date and time pickers                                                | Apple            | **None** (D3)                                 | P0  |
| Notifications                                                        | Apple            | **None** (A8)                                 | P1  |
| Search events                                                        | Apple            | `use_search` can register                     | P1  |

Hidden details:

- Travel time and a suggested leave time ([AppleInsider overview][cal-tips]).
- A birthdays calendar ([AppleInsider][cal-tips]).
- Quick create parses names and places, and shows what it understood before saving.
- Drag an event between calendars, and drag out to a file ([HIG][hig-dnd] on Calendar).
- Clock and calendar share the same tabular digits and the same date formatter (A12).

## 7. Suggested order of work

1. **Platform floor.** A7, G1, G2, A4, A5. These touch every app and are cheap to add early.
2. **Semantics.** The `Semantics` builder, heading and relation roles, and the Orca run.
3. **Data and text.** C1, D1, D3. Unblocks Calendar, Text editor and Settings lists.
4. **Drag and drop.** A1 depends on Freya and winit. Check whether a Wayland `data_device` path can be added in the vendored winit.
5. **Media and services.** F3, F4, A8, B5.
6. **Breadth.** Everything marked P2, picked by what the next app needs.

Each new export needs a taxonomy entry in `apps/taxonomy/src/registry.rs` and a doc entry in `docs/bingux-ui.md`, as
`AGENTS.md` requires.

## 8. Not verified

- Whether Freya's AT-SPI path sets text character positions, and which `Action`s it routes. Read the vendored
  `freya-core` accessibility code and run Orca.
- Whether winit's Wayland backend forwards `text-input` for IME in this vendored copy.
- Whether Escape dismisses a tooltip, and whether a tooltip stays while the pointer is over it (WCAG 1.4.13).
- Whether the split divider and column edge have a keyboard path.
- Material 3 component pages are rendered by script, so only the pages whose text could be read are cited. Chips, Snackbar
  and Search were not readable.
- Weather data licences other than Open-Meteo and WeatherKit were not read.
- The BBEdit feature tour could be read only in part. Details come from its Text Handling page.

## Sources

Apple Human Interface Guidelines:

- [Accessibility][hig-a11y], [VoiceOver][hig-vo], [Typography][hig-type], [Writing][hig-writing], [Motion][hig-motion],
  [Materials][hig-materials]
- [Drag and drop][hig-dnd], [Undo and redo][hig-undo], [Keyboards][hig-kbd], [Focus and selection][hig-focus],
  [Pointing devices][hig-pointing]
- [Toolbars][hig-toolbars], [The menu bar][hig-menubar], [Menus][hig-menus], [Context menus][hig-ctx],
  [Windows][hig-windows], [Sheets][hig-sheets], [Popovers][hig-popovers], [Alerts][hig-alerts], [Panels][hig-panels]
- [Sidebars][hig-sidebars], [Split views][hig-split], [Column views][hig-columns], [Path controls][hig-path],
  [Lists and tables][hig-lists], [Outline views][hig-outline], [Collections][hig-collections]
- [Searching][hig-search], [Search fields][hig-searchfields], [Token fields][hig-tokens], [Pickers][hig-pickers]
- [Progress indicators][hig-progress], [Gauges][hig-gauges], [Feedback][hig-feedback], [Loading][hig-loading],
  [Modality][hig-modality], [Settings][hig-settings], [Offering help][hig-help], [Onboarding][hig-onboarding]
- [File management][hig-files], [Playing video][hig-video], [Playing audio][hig-audio], [Charts][hig-charts],
  [Text views][hig-textviews], [Managing notifications][hig-notif], [Going full screen][hig-fullscreen],
  [Collaboration and sharing][hig-collab]

Apple frameworks and support pages:

- [SwiftUI documentation](https://developer.apple.com/documentation/swiftui) (each symbol is linked in section 3.2)
- [Quick Look][finder-ql], [Finder][finder-use], [Finder tags][finder-tags], [sort and group][finder-sort],
  [rename items][finder-rename], [Finder settings][finder-settings], [Smart Folder][finder-smart],
  [quick actions][finder-actions], [merge on copy][finder-merge]
- [Calculator modes][calc-modes], [scientific][calc-sci], [conversion][calc-convert]
- [Music queue][music-queue], [lyrics][music-lyrics], [sound quality][music-sound], [AutoMix and Crossfade][music-xfade]
- [QuickTime open and play][qt-play], [QuickTime trim][qt-trim], [QuickTime subtitles][qt-sub]
- [Preview guide][pv-guide], [annotate][pv-annotate], [forms][pv-forms], [combine][pv-combine], [pages][pv-pages]
- [Calendar events][cal-add], [time zones][cal-tz], [Weather guide][wx-guide], [WeatherKit][wk-attr]
- [TextEdit find and replace][te-find], [TextEdit settings][te-settings]

Linux and other design systems:

- [GNOME HIG][gnome-hig], [accessibility][gnome-a11y], [keyboard][gnome-kbd], [standard shortcuts][gnome-shortcuts],
  [list and column views][gnome-list], [grid views][gnome-grid], [utility panes][gnome-utility], [search][gnome-search],
  [notifications][gnome-notif], [toasts][gnome-toasts], [placeholders][gnome-placeholder], [dialogs][gnome-dialogs],
  [adaptive][gnome-adaptive], [libadwaita widget gallery][adw-gallery]
- GNOME Help: [preview][gnome-preview], [select by pattern][gnome-select], [rename several][gnome-rename],
  [templates][gnome-templates], [open with][gnome-open], [delete and undo][gnome-delete], [share][gnome-share],
  [network][gnome-connect], [path as text][gnome-path], [files overview][gnome-files]
- [Dolphin handbook][dolphin-handbook]: [view][dolphin-view], [location bar][dolphin-location], [panels][dolphin-panels],
  [quick tips][dolphin-tips], [configuring][dolphin-config]
- [File Explorer][win-fe] and its [keyboard shortcuts][win-keys]
- [Carbon data table][carbon-dt], [notifications][carbon-notif], [empty states][carbon-empty], [spacing][carbon-spacing],
  [content][carbon-content]
- [Primer saving][primer-saving], [degraded experiences][primer-degraded], [loading][primer-loading],
  [notification messaging][primer-notif], [progressive disclosure][primer-disclosure], [components][primer-comps]
- [Fluent 2 components][fluent-comps], [Fluent 2 design tokens][fluent-tokens]
- [Material 3 canonical layouts][m3-layouts], [Material 3 easing and duration][m3-motion]
- [Polaris index table][polaris-index], [Polaris content fundamentals][polaris-content]

Accessibility and platform:

- [WCAG 2.2][wcag22], [WCAG2ICT 2.2][wcag2ict]
- [ARIA Authoring Practices patterns][apg-patterns], [keyboard interface][apg-keyboard]
- [AccessKit][accesskit], [`Role`][accesskit-role], [`Action`][accesskit-action], [`Node`][accesskit-node],
  [`accesskit_unix`][accesskit-unix]
- [at-spi2-core next-generation protocol notes][atspi-protocol], [freedesktop AT-SPI2][atspi-wiki],
  [KWin accessibility manager commit][kwin-a11y]
- [XDG Settings portal][portal-settings], [MPRIS Player interface][mpris], [mpv manual][mpv]
- [Sublime editing][sublime-edit], [Sublime navigation][sublime-nav], [GNOME Text Editor][gte],
  [GNOME Text Editor search][gte-search], [BBEdit text handling][bbedit-text], [Open-Meteo terms][open-meteo]

[hig-a11y]: https://developer.apple.com/design/human-interface-guidelines/accessibility
[hig-vo]: https://developer.apple.com/design/human-interface-guidelines/voiceover
[hig-type]: https://developer.apple.com/design/human-interface-guidelines/typography
[hig-writing]: https://developer.apple.com/design/human-interface-guidelines/writing
[hig-motion]: https://developer.apple.com/design/human-interface-guidelines/motion
[hig-materials]: https://developer.apple.com/design/human-interface-guidelines/materials
[hig-dnd]: https://developer.apple.com/design/human-interface-guidelines/drag-and-drop
[hig-undo]: https://developer.apple.com/design/human-interface-guidelines/undo-and-redo
[hig-kbd]: https://developer.apple.com/design/human-interface-guidelines/keyboards
[hig-focus]: https://developer.apple.com/design/human-interface-guidelines/focus-and-selection

<!-- Apple HIG, continued -->

[hig-pointing]: https://developer.apple.com/design/human-interface-guidelines/pointing-devices
[hig-toolbars]: https://developer.apple.com/design/human-interface-guidelines/toolbars
[hig-menubar]: https://developer.apple.com/design/human-interface-guidelines/the-menu-bar
[hig-menus]: https://developer.apple.com/design/human-interface-guidelines/menus
[hig-ctx]: https://developer.apple.com/design/human-interface-guidelines/context-menus
[hig-windows]: https://developer.apple.com/design/human-interface-guidelines/windows
[hig-sheets]: https://developer.apple.com/design/human-interface-guidelines/sheets
[hig-popovers]: https://developer.apple.com/design/human-interface-guidelines/popovers
[hig-alerts]: https://developer.apple.com/design/human-interface-guidelines/alerts
[hig-panels]: https://developer.apple.com/design/human-interface-guidelines/panels
[hig-sidebars]: https://developer.apple.com/design/human-interface-guidelines/sidebars
[hig-split]: https://developer.apple.com/design/human-interface-guidelines/split-views

<!-- Apple HIG, continued -->

[hig-columns]: https://developer.apple.com/design/human-interface-guidelines/column-views
[hig-path]: https://developer.apple.com/design/human-interface-guidelines/path-controls
[hig-lists]: https://developer.apple.com/design/human-interface-guidelines/lists-and-tables
[hig-outline]: https://developer.apple.com/design/human-interface-guidelines/outline-views
[hig-collections]: https://developer.apple.com/design/human-interface-guidelines/collections
[hig-search]: https://developer.apple.com/design/human-interface-guidelines/searching
[hig-searchfields]: https://developer.apple.com/design/human-interface-guidelines/search-fields
[hig-tokens]: https://developer.apple.com/design/human-interface-guidelines/token-fields
[hig-pickers]: https://developer.apple.com/design/human-interface-guidelines/pickers
[hig-progress]: https://developer.apple.com/design/human-interface-guidelines/progress-indicators

<!-- Apple HIG, continued -->

[hig-gauges]: https://developer.apple.com/design/human-interface-guidelines/gauges
[hig-feedback]: https://developer.apple.com/design/human-interface-guidelines/feedback
[hig-loading]: https://developer.apple.com/design/human-interface-guidelines/loading
[hig-modality]: https://developer.apple.com/design/human-interface-guidelines/modality
[hig-settings]: https://developer.apple.com/design/human-interface-guidelines/settings
[hig-help]: https://developer.apple.com/design/human-interface-guidelines/offering-help
[hig-onboarding]: https://developer.apple.com/design/human-interface-guidelines/onboarding
[hig-files]: https://developer.apple.com/design/human-interface-guidelines/file-management
[hig-video]: https://developer.apple.com/design/human-interface-guidelines/playing-video
[hig-audio]: https://developer.apple.com/design/human-interface-guidelines/playing-audio

<!-- Apple support, continued -->

[hig-charts]: https://developer.apple.com/design/human-interface-guidelines/charts
[hig-textviews]: https://developer.apple.com/design/human-interface-guidelines/text-views
[hig-notif]: https://developer.apple.com/design/human-interface-guidelines/managing-notifications
[hig-fullscreen]: https://developer.apple.com/design/human-interface-guidelines/going-full-screen
[hig-collab]: https://developer.apple.com/design/human-interface-guidelines/collaboration-and-sharing
[finder-ql]: https://support.apple.com/guide/mac-help/view-and-edit-files-with-quick-look-mh14119/mac
[finder-use]: https://support.apple.com/guide/mac-help/change-the-way-you-view-files-and-folders-mchlp2605/mac
[finder-tags]: https://support.apple.com/guide/mac-help/use-tags-to-organize-files-on-mac-mchlp15236/mac
[finder-sort]: https://support.apple.com/guide/mac-help/sort-and-arrange-items-in-the-finder-on-mac-mchlp1745/mac
[finder-rename]: https://support.apple.com/guide/mac-help/rename-files-folders-and-disks-on-mac-mchlp1144/mac

<!-- continued -->

[finder-settings]: https://support.apple.com/guide/mac-help/change-finder-settings-on-mac-mchlp2803/mac
[finder-smart]: https://support.apple.com/guide/mac-help/create-or-change-a-smart-folder-on-mac-mchlp2804/mac
[finder-actions]: https://support.apple.com/guide/mac-help/perform-quick-actions-in-the-finder-on-mac-mchl97ff9142/mac
[finder-merge]: https://www.publicspace.net/blog/merge-folders-mac
[macpaw-spring]: https://macpaw.com/how-to/use-spring-loading-on-mac
[calc-modes]: https://support.apple.com/guide/calculator/choose-the-right-mode-calc22d50970/mac
[calc-sci]: https://support.apple.com/guide/calculator/use-the-scientific-calculator-calcf964141e/mac
[calc-convert]: https://support.apple.com/en-gu/guide/calculator/calc2bbb5464/mac
[music-queue]: https://support.apple.com/guide/music/queue-your-songs-musb1e6d1c76/mac
[music-lyrics]: https://support.apple.com/guide/music/view-and-enter-lyrics-musf438ffc97/mac

<!-- continued -->

[music-sound]: https://support.apple.com/guide/music/adjust-the-sound-quality-museb684a3de/mac
[music-xfade]: https://support.apple.com/en-us/105067
[qt-play]: https://support.apple.com/guide/quicktime-player/open-and-play-a-file-qtp6cee0761b/mac
[qt-trim]: https://support.apple.com/guide/quicktime-player/trim-a-movie-or-clip-qtpf2115f6fd/mac
[pv-annotate]: https://support.apple.com/guide/preview/annotate-a-pdf-prvw11580/mac
[pv-forms]: https://support.apple.com/en-my/guide/preview/prvw35725/mac
[pv-combine]: https://support.apple.com/guide/preview/combine-pdfs-prvw43696/mac
[pv-pages]: https://support.apple.com/en-ca/guide/preview/prvw11793/mac
[cal-add]: https://support.apple.com/guide/calendar/add-modify-or-delete-events-icalwr13-events/mac
[cal-tips]: https://appleinsider.com/inside/ios-26/tips/inside-apple-calendar---creating-events-setting-reminders-remembering-birthdays

<!-- continued -->

[wx-guide]: https://support.apple.com/guide/weather-mac/view-weather-conditions-apdw93f0ea3e/mac
[wk-attr]: https://developer.apple.com/weatherkit/get-started
[te-find]: https://support.apple.com/guide/textedit/find-and-replace-text-txtef6cfde1a/mac
[te-settings]: https://support.apple.com/guide/textedit/change-textedit-settings-txted1063/mac
[gnome-hig]: https://developer.gnome.org/hig/
[gnome-a11y]: https://developer.gnome.org/hig/guidelines/accessibility.html
[gnome-kbd]: https://developer.gnome.org/hig/guidelines/keyboard.html
[gnome-shortcuts]: https://developer.gnome.org/hig/reference/keyboard.html
[gnome-list]: https://developer.gnome.org/hig/patterns/containers/list-column-views.html
[gnome-grid]: https://developer.gnome.org/hig/patterns/containers/grid-views.html

<!-- continued -->

[gnome-utility]: https://developer.gnome.org/hig/patterns/containers/utility-panes.html
[gnome-search]: https://developer.gnome.org/hig/patterns/nav/search.html
[gnome-notif]: https://developer.gnome.org/hig/patterns/feedback/notifications.html
[gnome-toasts]: https://developer.gnome.org/hig/patterns/feedback/toasts.html
[gnome-placeholder]: https://developer.gnome.org/hig/patterns/feedback/placeholders.html
[gnome-dialogs]: https://developer.gnome.org/hig/patterns/feedback/dialogs.html
[gnome-adaptive]: https://developer.gnome.org/hig/guidelines/adaptive.html
[adw-gallery]: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/widget-gallery.html
[gnome-files]: https://help.gnome.org/gnome-help/files.html
[gnome-preview]: https://help.gnome.org/gnome-help/files-preview.html

<!-- continued -->

[gnome-select]: https://help.gnome.org/gnome-help/files-select.html
[gnome-rename]: https://help.gnome.org/gnome-help/files-rename-multiple.html
[gnome-templates]: https://help.gnome.org/gnome-help/files-templates.html
[gnome-open]: https://help.gnome.org/gnome-help/files-open.html
[gnome-delete]: https://help.gnome.org/gnome-help/files-delete.html
[gnome-share]: https://help.gnome.org/gnome-help/files-share.html
[gnome-connect]: https://help.gnome.org/gnome-help/nautilus-connect.html
[gnome-path]: https://help.gnome.org/gnome-help/files-show-directory-path.html
[dolphin-handbook]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/index.html
[dolphin-view]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/dolphin-view.html

<!-- continued -->

[dolphin-location]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/location-bar.html
[dolphin-panels]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/panels.html
[dolphin-tips]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/quick-tips.html
[dolphin-config]: https://docs.kde.org/stable_kf6/en/dolphin/dolphin/configuring-dolphin.html
[win-fe]: https://support.microsoft.com/en-us/windows/find-and-open-file-explorer-ef370130-1cca-9dc5-e0df-2f7416fe1cb1
[win-keys]: https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
[carbon-dt]: https://carbondesignsystem.com/components/data-table/usage/
[carbon-notif]: https://carbondesignsystem.com/patterns/notification-pattern/
[carbon-empty]: https://carbondesignsystem.com/patterns/empty-states-pattern/
[carbon-spacing]: https://carbondesignsystem.com/elements/spacing/overview/

<!-- continued -->

[carbon-content]: https://carbondesignsystem.com/guidelines/content/overview/
[primer-saving]: https://primer.style/product/ui-patterns/saving
[primer-degraded]: https://primer.style/product/ui-patterns/degraded-experiences
[primer-loading]: https://primer.style/product/ui-patterns/loading
[primer-notif]: https://primer.style/product/ui-patterns/notification-messaging
[primer-disclosure]: https://primer.style/product/ui-patterns/progressive-disclosure
[primer-comps]: https://primer.style/product/components/
[fluent-comps]: https://fluent2.microsoft.design/components/web/react
[fluent-tokens]: https://fluent2.microsoft.design/design-tokens
[m3-layouts]: https://m3.material.io/foundations/layout/canonical-layouts

<!-- continued -->

[m3-motion]: https://m3.material.io/styles/motion/easing-and-duration
[polaris-index]: https://raw.githubusercontent.com/Shopify/polaris/main/polaris.shopify.com/content/components/tables/index-table.mdx
[polaris-content]: https://polaris.shopify.com/content/fundamentals
[wcag22]: https://www.w3.org/TR/WCAG22/
[wcag2ict]: https://www.w3.org/TR/wcag2ict-22/
[apg-patterns]: https://www.w3.org/WAI/ARIA/apg/patterns/
[apg-keyboard]: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
[accesskit]: https://github.com/AccessKit/accesskit
[accesskit-role]: https://docs.rs/accesskit/0.24.0/accesskit/enum.Role.html
[accesskit-action]: https://docs.rs/accesskit/0.24.0/accesskit/enum.Action.html

<!-- continued -->

[accesskit-node]: https://docs.rs/accesskit/0.24.0/accesskit/struct.Node.html
[accesskit-unix]: https://docs.rs/accesskit_unix/latest/accesskit_unix/
[atspi-protocol]: https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/new-protocol.html
[atspi-wiki]: https://www.freedesktop.org/wiki/Accessibility/AT-SPI2
[kwin-a11y]: https://invent.kde.org/saueseb/kwin/-/commit/ce1085758d5ce8922ff8f688c835fcb9141e7a1a
[portal-settings]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
[mpris]: https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html
[mpv]: https://mpv.io/manual/stable
[sublime-edit]: https://docs.sublimetext.io/guide/usage/editing.html
[sublime-nav]: https://docs.sublimetext.io/guide/usage/file-management/navigation.html

<!-- continued -->

[gte]: https://github.com/GNOME/gnome-text-editor
[gte-search]: https://help.gnome.org/gnome-text-editor/edit-search-and-replace.html
[bbedit-text]: https://www.barebones.com/products/bbedit/featurestext.html
[open-meteo]: https://open-meteo.com/en/terms
[adw-style]: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.StyleManager.html
[finder-cutpaste]: https://www.productive.tips/tipy/mac-presun-souboru
[mac-keys]: https://support.apple.com/en-us/102650
[qt-sub]: https://support.apple.com/guide/quicktime-player/show-subtitles-or-captions-qtped9fbc349/mac
[pv-guide]: https://support.apple.com/guide/preview/welcome/mac
[cal-tz]: https://support.apple.com/guide/iphone/schedule-display-events-a-time-zone-iph69525c028/ios
