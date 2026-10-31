# Search in Files

Contributor design note. Part 1 answers Kieran's questions about the Files
search as it was at commit `2b4710c`, with the code it rests on. Part 2 is the
search that replaces it: what it is built from, what it does, how it is tested
and what is not done. User-facing text is in `docs/files.md` and
`docs/bingux-ui.md` ("Search in the file views"); the query language is
documented in `crates/files/src/find/query.rs`.

> "the search system in files could do with some work to be honest. does it
> use the searchables system under the hood? and also the search box
> chip/badges thing for like "in pictures" is kinda naff. how do i access
> more options and filters and stuff for search."

# Part 1: what it was

## 1.1 How it worked, end to end

Paths below are at `2b4710c`; the files named here were replaced in Part 2.

| Step           | Where                                                   | What happened                                                                                                                                                                                                                                                                                            |
| -------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Field          | `crates/ui/src/files/address_edit.rs`                   | The **address bar** was the search box. One text field took a path or a search. Ctrl+F or a letter typed on the list set `Find.scoped` and turned the bar's field into a search field with a chip `in <Folder>` inside it.                                                                               |
| Path or search | `crates/files/src/search.rs` `classify`                 | Without touching the disk: text starting `/`, `~`, `./`, `../` or a scheme, or naming an entry of the folder, was a path; anything else a query.                                                                                                                                                         |
| Model          | `crates/files/src/search.rs` `Query`                    | A `Query` was the trimmed text plus its whitespace-split words. No operators, no filters, no quotes. `Query::matches(name)` gave a `Tier`: exact, prefix, word start, substring, all words in any order, fuzzy (3+ letters in order, within a window of `2n + 4`). Case-insensitive, file **name** only. |
| Stage 1        | `chooser.rs` `select_shown`                             | The loaded folder listing was filtered in memory on every keystroke (`rank`), sorted by tier; ties kept the person's sort. No disk read.                                                                                                                                                                 |
| Stage 2        | `files/search.rs` `use_subfolder_search`, `follow_walk` | Only on request (the row "Search subfolders for …", Ctrl+Enter or Shift+Enter, or Enter with no match) a thread ran `bingux_files::search::walk`.                                                                                                                                                        |
| Results        | `follow_walk` → `chooser.rs`                            | The thread sent batches over an `mpsc` channel; a task polled every 60 ms, rebuilt a whole `Listing` of the matches (cloning every entry: about 1 ms per publish at 10,000 results, measured) and put it in a `State`. The file view showed it as a folder with a Location column.                       |

Details the question asked about:

- **Recursion.** Off until asked. Then `walk` read the folder on show and
  everything below it: one thread, breadth first (shallow matches first), one
  directory read at a time.
- **Hidden files.** Skipped (entries starting with `.` and names in the
  folder's `.hidden` file) unless "Show hidden files" was on, in the walk as
  in the filter. Hidden _folders_ were not entered.
- **Indexing.** None. Nothing was kept between searches; every walk read the
  folders again.
- **Cancellation.** A `Cancel` flag checked before every folder and every 32
  entries. A changed query, another folder, Stop, Escape (while the toolbar
  showed Stop) or the dialog closing set it (`StopOnDrop`,
  `TaskHandle::try_cancel`).
- **Streaming.** Batches of 64 matches or 50 ms, whichever came first; the UI
  picked them up every 60 ms.
- **Ranking.** By tier, for results as for the folder. Because
  `select_shown` ranked the _whole_ result list on every publish, rows
  already on show **moved** as better matches arrived: the list reordered
  while it streamed, and the selection and scroll position had nothing stable
  to hold on to.
- **Limits.** 10,000 results (then "limit reached"). Never followed a symbolic
  link (a link is a result, never a way in), stayed on the filesystem it
  started on (no mounted drives, no `/proc`), never entered a folder twice,
  entered `node_modules`, `target` and the like unless a view-menu switch was
  turned off.
- **Performance, 100,000 files** (a fixture of 5,321 folders on tmpfs, warm
  cache, release build): a word that matches one name in twenty found the
  first result after 18 to 27 ms and hit the 10,000 limit after 200 to 300
  ms; a word that matches nothing read all 5,321 folders in 150 to 220 ms.
  On a cold disk or a network share the same walk is bounded by directory
  reads, not by the matcher, and nothing was kept to make the second search
  faster.

## 1.2 Does it use the "searchables" system, searchd or the search crate?

**No. None of the three.** What each is:

- **`Searchable`** (`crates/search/src/service.rs`, re-exported as
  `bingux_ui::search::Searchable`) is the trait an app implements to register
  _app-owned structured records_ (a name, a `Schema`, a `documents()`
  iterator) with the process-wide `SearchService`. Settings uses it for its
  pages and rows (`apps/settings/src/settings_index.rs`,
  `pages/search.rs`). `design/search.md` section 2 lists "full-text search
  over file contents; the Files app and searchd keep their own walkers" as a
  **non-goal**, and section 12 says "The file view's address-bar search is
  not migrated".
- **`bingux-search`** (`crates/search`, commit `7d877b0`) is the
  UI-free engine under it: documents, an in-memory inverted index with prefix
  and typo tolerance, BM25F ranking, filters, OpenSearch-shaped responses.
- **`bingux-searchd`** (`services/searchd`, commit `4e5e397` and
  earlier) is the Spotlight daemon. The shell talks to it over
  `$XDG_RUNTIME_DIR/bingux/search-v1.sock`. Its file provider
  (`providers.rs`) reads a _fixed list of roots_ (`config.rs` `file_roots`),
  keeps its own cached file list (`index_files_with_progress`) and asks
  `plocate` for up to 256 paths per seed; it drops every hidden path. Results
  are launcher-sized (`ProviderResult { title, subtitle, icon, score }`).

At `2b4710c` Files linked none of them: its matcher (`Tier`) and walker
(`walk`) were a third implementation of "find a file by name", next to the
engine's tokenizer and searchd's walker. The gap, plainly:

1. Three matchers that disagree (what `rep` finds in each differs).
2. No index in Files, so every search read the disk again.
3. searchd cannot be the backend for Files as it is: it has no notion of
   "below this folder", returns launcher-sized answers over a private socket,
   skips hidden files and does not say which folders it covered. Asking it
   would have given an incomplete answer that looked complete.

Part 2 closes 1 and 2 by building Files' index on the shared **engine**
(`bingux-search`), and leaves 3 as it is (see section 2.9).

## 1.3 What the chip was

The "chip" was `TextField::leading_chip`: a removable token drawn inside the
address field in search mode (`address_edit.rs`; text from
`SearchLink::chip`). It said `in <name of the folder on show>`: the last
component of the current path (`in Pictures`) and nothing else. It was a
label, not a control: it could not be pressed to change where to search, it
had only a close button that ended the search, and the scope was always "the
folder on show", then "and below" after the second step. `Find.scoped` was
just the flag "the bar is a search box".

## 1.4 Filters, and where more options were

There were none in the search itself. What a person could reach:

| Option                              | Where                                                                    | What it did                                                                                        |
| ----------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| Show hidden files                   | view menu, Ctrl+H                                                        | Also decided whether the walk read hidden entries.                                                 |
| Search build and dependency folders | view menu (lasted for the window)                                        | Whether the walk entered `node_modules`, `target`, `.git` and seven more.                          |
| Sort                                | column headers                                                           | Ignored while a search was on: the tier order replaced it.                                         |
| File type dropdown                  | toolbar, **file dialogs only** (`shows_filters` required `Host::Dialog`) | The chooser's own filter (`*.png`, `image/*`); the walker honoured it. Not shown in the Files app. |

Nothing offered type, date, size, name matching, case or where to look, and
nothing in the interface said there were options at all: no search button,
no menu, no hint. Ctrl+F or typing was the only way in.

# Part 2: what replaces it

## 2.1 Parts

| Layer          | Where                                                                                                  | What                                                                                                                                                                                                                                                                               |
| -------------- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Query model    | `crates/files/src/find/query.rs`                                                                       | Typed `Query { terms, filters }`; `parse`, `parse_checked`, `Display`; `split_pending` (a `key:value` still being typed). Round trip tested on 20,000 random queries and 30,000 random strings.                                                                                    |
| Matcher        | `find/matcher.rs`                                                                                      | `Matcher::new(&Query)`; name tiers (exact, prefix, word, substring, words, fuzzy), `match:starts                                                                                                                                                                                   | exact | regex`, `-word`, `name:`globs,`ext:`, `type:`, `size:`, `modified:`(the person's local day, via`localtime_r`), `hidden:`, `case:`; `Rank` = tier, penalty, depth. |
| Index          | `find/index.rs`                                                                                        | `NameIndex`: the names under home in a `bingux_search::Index` (searchable `name`; filterable `ext`, `kind`, `under`). Crawled on a nice'd thread, built aside and swapped, at most 500,000 names, stale after 5 minutes. No sizes or dates: a hit is read from disk when returned. |
| Walk           | `find/walk.rs`                                                                                         | The old walker, over several roots, with the matcher and a `known` set.                                                                                                                                                                                                            |
| Engine         | `find/engine.rs`                                                                                       | `run(plan, cancel, emit)`: index answer first (verified on disk, ranked, one batch), then the walk skipping what was sent. Cancel stops between index hits and within 32 entries of the walk.                                                                                      |
| Scope, history | `find/scope.rs`, `find/history.rs`                                                                     | Scope roots; eight recent searches in `$XDG_STATE_HOME/bingux/files/recent-searches`.                                                                                                                                                                                              |
| UI             | `crates/ui/src/files/search.rs`, `omnibox.rs`, `omnibox_model.rs`, `crates/ui/src/controls/omnibox.rs` | `Find` (scope and filters), `use_search_run`, the location field and its card (scope and filter chips, results), the `Omnibox` control. There is no search bar, no Filters panel and no in-list result view: the card is where a search is made and shown.                         |

## 2.2 Streaming and stability

A walk produces results in discovery order and a consumer cannot know the best
match is still to come, so ranking while streaming moves rows. Rows are
appended in arrival order while the search runs and the index's batch (already
in relevance order) comes first. The card's height follows what it holds over
150 ms, so streamed rows make it grow a little at a time; what is under the card
never moves (`motion_stress::omnibox`: the line under the field is constant on
every frame of an open, a close, and rows streaming in).

## 2.3 What the index is and is not

It matches words (the start of a word, near misses by one or two edits). It
does not match the middle of a word or letters in order; the walker does, and
always runs, so the index is the quick first answer plus near-miss matches,
never the only source and never the authority: a file made after the crawl is
found by the walk and a file removed is dropped when read.

## 2.4 Measured (100,000 files, 5,321 folders, tmpfs, warm, release)

|                             | Walk only       | With index ready                             |
| --------------------------- | --------------- | -------------------------------------------- |
| first result, a common word | 20 to 27 ms     | 7 to 13 ms                                   |
| complete (10,000 results)   | 200 to 300 ms   | 215 to 275 ms                                |
| a word that matches nothing | 150 to 220 ms   | 160 to 240 ms                                |
| near miss (`reprot`)        | only fuzzy hits | 1,000 from the index in 12 ms, then the walk |

Index build: 105,320 names in 2.3 s on one nice'd thread; the process held 104 MB.
On tmpfs with a warm cache the walk is already fast, so the index's gain here is
modest; it matters on a cold disk or a large home, which was not measured.

## 2.5 Not done, stated plainly

- **Not on the searchd path, by decision.** Files neither queries
  `bingux-searchd` nor publishes its index to it, and registers no `Searchable`.
  Evidence, both directions:
    - _Files asking searchd_: the shell protocol's request is `query` with a
      `limit: u8` and no scope (`services/searchd/src/protocol.rs`); its file
      provider reads `file_roots` from config (at most 32), takes up to 256 paths
      per `plocate` seed and drops every path with a hidden component
      (`providers.rs` `query_os_index`). It cannot answer "below this folder,
      hidden included, up to 10,000". Doing it would need a protocol v2 with a
      scope and paging; until then searchd would give an incomplete answer that
      looks complete.
    - _searchd asking Files_ (one index for both): `app_search` only takes hits
      that carry a `uri` and a `title` (`app_search.rs`), so every document would
      grow by both (about 100 bytes of the 280 per document measured in
      `design/search.md`), the index would have to be `external` (`design/search.md`
      section 8.6: nothing is exposed by default, and a listing of every file name
      under home is exactly what a person must opt in to), a `bingux.search-app`
      manifest and a served socket (`bingux_search::ipc::serve`) are needed, and
      the index only exists after a first Files search, so the launcher would
      see nothing in a fresh session. The launcher already has its own file index
      (walker + plocate). Sharing is worth it only with a persisted snapshot
      (`design/search.md` 8.7) and an opt-in; neither is built.
      The names index is in this process only and is not written to disk.
- **No file contents search.** The option is omitted, not disabled.
- **No saved searches** (a places entry would need a new place kind). Recent
  searches exist.
- Everywhere = home and mounted drives, not system folders; only home is indexed.
- Not run: the headless-sway real-input pass on the 5k fixture and the phone and
  RTL screenshots; the specimens were added to the taxonomy but not looked at.
  Screenshots looked at came from the software renderer (`bingux-files --snapshot`)
  on a 4,900-file fixture: the bar with results, the open Filters panel, active
  tokens, and the no-results state with its two buttons.
