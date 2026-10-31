# Where Files' code lives

Files is a Bingux application, and a Bingux application contains _its_ window
and nothing else: look, layout, controls and behaviour that another
application could use belong to the SDK (`bingux-ui`, `crates/ui`), and the
file model to `bingux-files` (`crates/files`). `AGENTS.md` ("bingux-ui first")
is the rule; this note is the audit of `apps/files` against it, and the map of
where each part went.

## The rule applied

A part is **generic** when a second application, or a third-party developer,
plausibly needs it, and the second use can be named. It is **Files-specific**
when it only makes sense with folders, files, places or the Trash.

| Part                                                                                                                                       | Second user                                                                                          | Verdict                                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| A set of tabs: open beside the one on show, close and select the neighbour, reopen, duplicate, reorder, take out and put in another window | an editor with a tab per document, a terminal with tabs                                              | generic                                                                                        |
| The tab keys, menu entries, strip wiring and "close the last tab closes the window"                                                        | the same                                                                                             | generic                                                                                        |
| Windows of one application that know each other, so a tab can move between them, and a window's title that follows its tab                 | the same                                                                                             | generic                                                                                        |
| Tearing a tab off into a window that follows the pointer, and dropping it on another strip                                                 | the same                                                                                             | generic                                                                                        |
| Asking the compositor where windows are (sway IPC, Gnoblin's control socket)                                                               | anything that places windows relative to each other                                                  | generic                                                                                        |
| The random walks that prove no tab or history is lost                                                                                      | a test of the generic set                                                                            | split: the set's walk is generic (plain pages), the walk of histories is Files'                |
| What a command line names (`~/x`, `file://`, `recent:///`, `trash:///`), and what the running window shows for it                          | a second file manager, a "open folder here" helper                                                   | file specific: `bingux_ui::files`, beside `Where`, `Roots` and the folder routes it is made of |
| The folder a tab is titled, iconed and told apart by (`Documents — kieran`), the question before a copy is stopped, the Files menu         | none                                                                                                 | Files-specific                                                                                 |
| The remembered window size, kept in Files' state file                                                                                      | none (it is stored with the folder views)                                                            | Files-specific                                                                                 |
| The headless `--snapshot` harness wiring                                                                                                   | each app has its own (`apps/settings`, `apps/taxonomy`); the harness itself is `bingux_ui::snapshot` | app-specific wiring of a generic tool                                                          |

## Before and after

Line counts, with tests, from `7454642f` (before) and this branch (after).

| File                                |     Lines | What it does                                                                                                                                 | Generic?                                            | Destination                                                                              | Lines after |
| ----------------------------------- | --------: | -------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------: |
| `apps/files/src/main.rs`            |        54 | `Application` set-up: routes, title, size, usage text                                                                                        | Files                                               | stays                                                                                    |          55 |
| `apps/files/src/app.rs`             |       492 | the window; a private `Tabs` (open, select, close, duplicate, reopen, step, reorder, tear), the strip wiring, shortcuts and menu, tab titles | split                                               | `bingux_ui::tabs::{Tabs, use_tab_window}` and `FileTab`; titles, icons, guard, menu stay |         276 |
| `apps/files/src/tabset.rs`          |       757 | the tab set, its closed stack and moves between windows, with tests                                                                          | generic                                             | `crates/ui/src/tabs/set.rs` (`TabSet<P: TabContent>`)                                    |       1,043 |
| `apps/files/src/windows.rs`         |       135 | registry of the application's windows, birth of a window with torn-off tabs, window titles                                                   | generic                                             | `crates/ui/src/tabs/windows.rs`                                                          |         291 |
| `apps/files/src/tearoff.rs`         |       613 | a tab torn off, the window that follows the pointer, the drop on another strip                                                               | generic                                             | `crates/ui/src/tabs/tearoff.rs`                                                          |         837 |
| `apps/files/src/compositor.rs`      |       756 | where the compositor has put the windows (sway, Gnoblin)                                                                                     | generic                                             | `crates/ui/src/window/compositor/mod.rs`                                                 |         758 |
| `apps/files/src/compositor/fake.rs` |       170 | a fake Gnoblin control socket for tests                                                                                                      | generic                                             | `crates/ui/src/window/compositor/fake.rs`                                                |         170 |
| `apps/files/src/target.rs`          |       479 | what a command line argument names, and what the window shows for it                                                                         | file specific, shared by any app that opens folders | `crates/ui/src/files/launch.rs`                                                          |         481 |
| `apps/files/src/fuzz.rs`            |       416 | random walks of windows, tabs and histories                                                                                                  | Files' histories over the generic set               | `crates/ui/src/files/tabs_fuzz.rs`; the set's own walk is in `tabs/set.rs`               |         416 |
| `apps/files/src/snapshot_mode.rs`   |        93 | `--snapshot DIR`: the window to PNG, `BINGUX_FILES_SNAPSHOT_*`                                                                               | app wiring                                          | stays                                                                                    |          93 |
| `apps/files/src/window_size.rs`     |        26 | remembered window size in Files' state file                                                                                                  | Files                                               | stays                                                                                    |          26 |
| **`apps/files/src` total**          | **3,991** |                                                                                                                                              |                                                     |                                                                                          |     **450** |

New in the SDK: `crates/ui/src/tabs/actions.rs` (335 lines: `Tabs`, the handlers,
keys and menu entries every tabbed window needs), `tabs/mod.rs` (36) and
`FileTab` in `files/browser.rs` (what a Files tab holds). `files::ClosedTabs`
became `tabs::ClosedTabs<P>`.

The folder structure now says what each thing is:

```text
crates/files/                  the file model (listing, find, filter, location, places, ops)
crates/ui/src/tabs/            tabs of any application
    set.rs                       TabSet<P>, ClosedTabs<P>, Moved<P>: plain data, tested without a window
    actions.rs                   Tabs<P>: the set on screen (strip, keys, menu, close the window)
    windows.rs                   use_tab_window: the application's windows know each other
    tearoff.rs                   a tab torn off, carried, dropped
crates/ui/src/window/compositor/ where the desktop's windows are (sway, Gnoblin)
crates/ui/src/files/           the file views, shared by the Files window and the chooser
    launch.rs                    what a launch of a file manager names
    browser.rs                   TabMemory, FileTab, browse()
    tabs_fuzz.rs                 random walks of tabs and histories
    omnibox.rs                   the location field: a path completes, words search, chips for scope and filters
    omnibox_model.rs             what the field offers for a text, as plain data (is it a path, places, chips, status)
    search.rs                    Find (scope and filters) and use_search_run, the streaming search
crates/ui/src/controls/omnibox.rs the Omnibox control: a field that grows a card of suggestions out of itself
apps/files/                    the Files window: titles, icons, menu, window size, snapshot wiring
```

## The location field

The address bar's below-the-bar list, the search bar that slid down under the
toolbar and the Filters button are one field now: `Omnibox` (a control any app
can use) and, on top of it, the Files provider.

| Was                                                                                                                                                                                                    | Now                                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| `files/address_edit.rs` (a field with a list under it)                                                                                                                                                 | `files/omnibox.rs` (`LocationField`) over `controls/omnibox.rs`                            |
| `files/search_bar.rs`, `files/search_filters.rs` (the bar, the Filters panel, its presets)                                                                                                             | scope and filter chips inside the card; presets are `files/omnibox_model.rs`               |
| `files/search_tests.rs` (the bar's tests)                                                                                                                                                              | `files/omnibox_tests.rs`, `motion_stress/omnibox.rs`, `crates/files/src/location_tests.rs` |
| the list narrowed by a search: `SearchView`, `DeepStatus`, `Marks`, `SearchTop`, the Location column of results, the status pill's search line, `no_results_state`, `Scope::Folder`, Stop for a search | removed: the card is where a search is made and shown                                      |
| `Find { open, scope, filters, filters_open, focus }`                                                                                                                                                   | `Find { scope, filters }`                                                                  |

## Proof that nothing changed

(For Part A, the move of the tab code.)

- Every moved test runs from its new place and passes (`cargo test --lib
--features testing` in `crates/ui`: `tabs::`, `window::compositor`,
  `files::launch`, `files::tabs_fuzz`, `files::browser`).
- `apps/files --snapshot` at 1100x760 and 390x800, on the home folder's
  `Documents` and on a 120-file folder, with the tab, the preview and the plain
  window: the captures from `7454642f` and from this branch are identical pixel
  for pixel except the sidebar's free-space text of the drive the machine is
  on (105 GB and 100 GB free: the disk, not the code).
- `python3 scripts/scroll-continuity.py --only files` passes before and after.
