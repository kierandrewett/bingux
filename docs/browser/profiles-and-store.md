# Browser: profiles and the profile store

Design of 2026-11-26 for parity slice 2 ("The profile store and settings layer") and the data side of the Profiles
section of `parity.md`. It answers every question those two list, before any code.

Paths:

- `B/` is `apps/browser/src`.
- `UI/` is `crates/ui/src`.
- `VV/` is `~/dev/vivarium` at `e4fe1e8`.

In scope:

- The profile registry: create, rename, delete, default, ask at start (data only).
- The per-profile store and the migration of today's directory into it.
- History and today's settings kept across restarts.
- The private-window guarantee, and every failure as a model state.

Out of scope: the profile picker and profile pages (slice 8), Settings and Library as windows (slice 7), bookmarks and
the history manager (slice 5), passwords (slice 11).

## Decisions in one table

| Question                | Answer                                                                                                                                                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| What is a profile       | A folder with its own engine profile, its own browser store and its own engine process. Not a container                                                 |
| Where history lives     | A browser-side SQLite store in the profile folder, registered as the bingux-ui searchable `history`. Places in the engine keeps link colours only       |
| Settings format         | `settings.json`, replaced atomically. Readable by a private window without opening a database                                                           |
| Relational data         | `browser.sqlite`: history now; bookmarks, reading list, custom search engines and the download list later, as schema steps                              |
| Registry                | `profiles.json` next to the profile folders, replaced atomically, written under `profiles.lock`                                                         |
| Versioning              | A `version` number in each JSON file, `PRAGMA user_version` in the database. Steps only forward; a newer file is never written                          |
| Migration               | First start with no registry: today's `bingux-browser/profile` is moved (one `rename`) into a new profile "Personal"                                    |
| Who may write a profile | One process at a time: it holds `flock` on the profile's `lock` file for its life                                                                       |
| Private window          | Opens no store, takes no lock, creates no file outside its throwaway engine profile in `$XDG_RUNTIME_DIR`. A test walks the tree before and after       |
| Permissions on disk     | Folders 0700, files 0600, set at creation whatever the umask. No secrets in the store                                                                   |
| Owner split             | Engine: web data. Browser: registry, store, migration, failure states. bingux-ui: no storage; its web view takes the profile's folders from the browser |

## What a profile is

From `parity.md` "Profiles": a profile is a whole browser identity ("Each profile keeps its own history, cookies,
passwords and extensions", Settings 10). Vivarium has one `profile_dir` per engine and locks it
(`.parentlock`, `VV/docs/abi-design-v2.md` LC9). A contextual identity is only a cookie and storage partition inside
one profile; it shares extensions, prefs, certificate exceptions and Places. So:

- **A profile is a folder** holding an engine profile and the browser's store, run by **one engine process**.
- **Containers** (contextual identities) stay a separate, later feature inside a profile.
- **Switching profile** opens a window of the other profile, in its own process. One window never shows two
  profiles' tabs.

## Where things live

```text
$XDG_DATA_HOME/bingux-browser/            0700
  profiles.json                           0600  the registry
  profiles.lock                           0600  flock while the registry is read, changed and replaced
  profiles/<id>/                          0700  one profile: delete this folder and the profile is gone
    lock                                  0600  flock, held by the one process that has the profile open
    settings.json                         0600  the profile's settings
    browser.sqlite (-wal, -shm)           0600  history; later bookmarks, reading list, search engines
    engine/                               0700  Vivarium's profile_dir: cookies, storage, permissions, Places
$XDG_CACHE_HOME/bingux-browser/profiles/<id>/        the engine's cache
$XDG_STATE_HOME/bingux-browser/profiles/<id>/crashes  crash dumps (never uploaded)
$XDG_RUNTIME_DIR/bingux-browser/<id>/                 the engine's sockets
```

- **The store is a sibling of `engine/`, not inside it.** The engine walks its whole profile before Gecko starts and
  refuses what it does not know (`VV/src/vivarium/app/VivariumProfileCheck.h`: owner, mode, links, planted files,
  add-on folders). Gecko owns everything below `profile_dir`; the browser writes nothing there.
- **`<id>` is random** (eight lowercase base32 characters from `getrandom`), never the name. Renaming a profile
  changes one field in the registry and moves no folder.
- **Cache, dumps and sockets** follow the XDG split the browser uses today (`UI/web_view/engine/config.rs`), one
  folder per profile, so two profiles share nothing and deleting one removes all three.
- **Downloads** stay `~/Downloads` for every profile until the Downloads setting exists (Settings 01).

### `BINGUX_BROWSER_PROFILE`

- **Today** it names an engine profile folder, and `run.sh` sets it by default to the old path.
- **From this slice** it names a profile folder outside the registry, in the `profiles/<id>/` shape above: `engine/`,
  the store and the lock below it, and cache, dumps and sockets below it too (as for a named profile today).
- **It is for tests and development:** not registered, not migrated, not listed.
- **`run.sh` stops setting it**, so a normal start uses the registry.

## The registry

`profiles.json`:

```json
{
    "version": 1,
    "default": "k3p9x2qa",
    "ask_at_start": false,
    "profiles": [
        { "id": "k3p9x2qa", "name": "Personal", "colour": "blue", "created": 1795651200 },
        { "id": "w7m4c8tz", "name": "Work", "colour": "orange", "created": 1795737600, "deleting": true }
    ]
}
```

| Field          | Meaning                                                                                                                                              |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `version`      | Registry format. This slice writes 1                                                                                                                 |
| `default`      | The profile a start with no `--profile` opens, unless `ask_at_start` says to ask                                                                     |
| `ask_at_start` | With two or more profiles, a start with no `--profile` shows the picker (slice 8) instead of opening `default`                                       |
| `name`         | What the person calls it. Trimmed, 1 to 64 characters, unique without regard to case                                                                 |
| `colour`       | A GNOME accent name from bingux-ui's `theme::ACCENTS` (`blue`, `teal`, `orange`, ...), never a colour value: `Paint::Swatch` tints it for the scheme |
| `created`      | Unix seconds, for the profile page and the order of the list                                                                                         |
| `deleting`     | Set before a delete removes folders; a start finishes any profile marked so. Such a profile is never listed or opened                                |

Operations (all in `B/model/profiles.rs`, all off the UI thread, each one read-change-replace under `profiles.lock`):

| Operation                | Rule                                                                                                                                |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `create(name, colour)`   | New id, folder `profiles/<id>/` with `engine/` (0700), entry added. The first profile ever becomes the default                      |
| `rename(id, name)`       | Name rules above; a running profile may be renamed, nothing moves                                                                   |
| `set_colour(id, colour)` | As rename                                                                                                                           |
| `set_default(id)`        | The id must be listed                                                                                                               |
| `set_ask_at_start(bool)` | Stored as given; it only takes effect with two or more profiles                                                                     |
| `delete(id)`             | Refused for the last profile and for an open one (its `lock` is held). Marks `deleting`, removes the three folders, drops the entry |
| (delete of the default)  | The oldest remaining profile becomes the default                                                                                    |
| `size(id)`               | Bytes of the profile's data and cache folders, for the confirm's disk-size note                                                     |
| `startup(asked)`         | `--profile` (a name or an id) wins; else `Ask` when `ask_at_start` and two or more profiles; else `Open(default)`                   |

Readers that do not change the registry (a private window, the picker) read `profiles.json` without the lock: every
write is a whole new file renamed over the old, so a reader sees the old file or the new one.

## The per-profile store

### `settings.json`

```json
{ "version": 1, "search_engine": "duckduckgo" }
```

- Today's only setting is the address bar's search engine (`B/ui/settings.rs`, an index into `address::ENGINES`).
  It is stored by a **stable id** (`duckduckgo`, `startpage`, `wikipedia`, `google`), not the index: reordering the
  list must not change anyone's choice. An unknown id reads as the default and is kept as written until the person
  picks another, so a newer browser's engine is not lost.
- Unknown fields are kept on write (a newer browser's settings survive an older one that is allowed to write).
- The two actions on the page today (Clear history, Forget site permissions) are not settings: Clear history acts
  on the store, Forget permissions on the engine (see "Owner split").
- **Why JSON and not a table:** the settings are one small document read whole. A private window must read them (its
  search engine) and must write nothing. A plain `read` of a file writes nothing; SQLite in WAL mode creates `-shm`
  and `-wal` on a read-only open when they are absent, and `immutable=1` is unsafe while the profile's process writes.

### `browser.sqlite`

Version 1:

```sql
CREATE TABLE pages (
    id          INTEGER PRIMARY KEY,
    url         TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL DEFAULT '',
    visit_count INTEGER NOT NULL,
    last_visit  INTEGER NOT NULL          -- unix milliseconds
);
CREATE TABLE visits (
    id      INTEGER PRIMARY KEY,
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    at      INTEGER NOT NULL              -- unix milliseconds
);
CREATE INDEX visits_at ON visits(at);
CREATE INDEX pages_last_visit ON pages(last_visit);
```

- `pages` is what the address field's suggestions read (`B/model/history.rs`: url, title, count, last). `visits`
  keeps each visit's time, which the Library needs (Library 01: visits per day, filter by date, clear a range).
- Opened with `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `secure_delete=ON` (deleted history is
  overwritten, not left in free pages) and a busy timeout. The same pattern as `crates/data-usage/src/schema.rs`:
  steps in a list, each run once in one immediate transaction, never edited after release.
- **What is recorded:** what `History::record` records today, the top-level loads of a non-private tab; `about:`,
  `bingux:` and empty addresses are not. A title that arrives later updates the page.
- **Cap:** the 50,000 most recently visited pages and their visits. A larger history is trimmed by the store's
  thread after start, oldest first. No age expiry: clearing a range is the person's tool (Library, slice 5).
- **Later steps** (each its own schema version, each with its slice): bookmarks and folders, reading list, custom
  search engines, the download list across restarts, recently closed tabs.

### History: Places or the browser

The question (`parity.md`, "History and frecency", open; `api-parity.md` H2 and open question 2): should history and
bookmarks use Gecko's own Places inside the engine, or a browser-side store?

Evidence from Vivarium:

| Fact                                                                                                                                                          | Where                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Places is always built: one of `MOZ_PLACES` and GeckoView history is mandatory, and Vivarium takes Places                                                     | `VV/docs/research/build-and-app-model.md` (moz.configure row)                         |
| Places runs: its expiry, frecency and idle maintenance are kept "on purpose" ("history must expire")                                                          | `VV/src/vivarium/components/VivariumStartup.cpp`                                      |
| Places records visits: at quit the engine waits for queued visits to reach `places.sqlite` (`vv_engine_shutdown` spends up to 75 percent of its time on them) | `VV/src/vivarium/modules/VivariumSessions.sys.mjs` `waitForVisits`, `vivarium_core.h` |
| `places.sqlite` and `favicons.sqlite` are known profile files                                                                                                 | `VV/src/vivarium/app/VivariumProfileRules.cpp`                                        |
| The ABI comment: the engine owns "visited links through Places"; the host owns "credentials, bookmarks, search engines, the download list across restarts"    | `vivarium_services.h` header comment                                                  |
| There is no call to read, search or delete Places data. `VV_EVENT_VISIT` (59) was removed because the engine never sent it                                    | `vivarium_base.h`; commit `05a69eb`                                                   |

So Places runs as Gecko's internal store for `:visited` link colours, and nothing outside the engine can read it.

Kieran's test: a primitive goes upstream to Vivarium only when many kinds of embedder use it.

- **A history query API** (read, search, frecency, observe visits) serves a browser and nothing else: Mail's sign-in
  sheet, a help viewer, a documentation app or a kiosk shows no history manager. It **fails the test**.
- **Forgetting history** passes it: every embedder that keeps a profile and offers "sign out", "clear data" or
  "reset" must make the engine forget visits and link colours along with cookies and storage. That is the clear-data
  primitive P12, which `parity.md` already lists.
- **Turning visit recording off** passes it too: an embedder that is not a browser (Mail's sign-in sheet, a help
  viewer) has no reason to grow `places.sqlite` with every page it shows.

**Decision: history is a browser-side store** (`browser.sqlite`). Bookmarks follow (slice 5), as the ABI comment
already says. Places stays inside the engine for link colours. The cost is a second copy of the visit list inside
the engine, and it has one consequence the browser must be honest about:

- **Clear history clears the browser's store, not the engine's Places.** Until Vivarium has the clear-data call
  below, visited-link colours (and `engine/places.sqlite`) keep what the person cleared.
- **That is model state, not copy.** `Browser::history_clear_scope()` says what Clear history reaches: the browser's
  history always, the engine's link colours only when the backend can clear visits. The settings row's subtitle and
  the toast after Clear are built from it ("Link colours on pages are kept until the engine can clear them"), so they
  change by themselves when V1 lands.
- `parity.md` W15 is closed by this slice; this gap is recorded there as a Vivarium dependency.

### History as a searchable

The browser model's principle 2 ("One search", `bingux-doop/docs/apps/browser-model.md`): the browser registers its
searchables with the bingux-ui search service, and every search box is the one omnibox with a scope. History is the
first searchable this slice builds; open tabs, bookmarks, reading list, commands, settings pages and downloads follow
with their slices, each as its own index.

The service today (`UI/search.rs`, `crates/search`, `design/search.md`):

- An app implements `Searchable` (`name`, `schema`, `documents()`), calls `search::register` once, and the process-wide
  `SearchService` loads it on the worker pool. `search::index(name)` gives the `Index` for `upsert`, `delete` and
  `clear` when the data changes.
- A component asks with `use_search_with(Search, active)`: off the UI thread, a newer query drops the older one, the
  previous hits stay while the next run is going.
- Ranking is the engine's: words, typo, proximity, attribute (field weight, then position), exactness, then BM25F.
  A request `sort` comes before those rules; `_score` is a sort key, so `sort("_score:desc")` followed by a field sort
  keeps relevance first and uses the field only to break ties.
- `Schema::external` defaults to false: nothing leaves the process unless an index asks.

The history searchable, built in step 6:

| Part            | Shape                                                                                                                       |
| --------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Index name      | `history`. One per process, and a process has one profile                                                                   |
| Document id     | The page's URL (unique in `pages`)                                                                                          |
| Searched fields | `host` 10 (without `www.`), `title` 8, `path` 2 (the rest of the address)                                                   |
| Other fields    | `uri` (the URL: what choosing the hit opens), `visit_count`, `last_visit` (unix ms), both sortable and filterable           |
| Loading         | `documents()` reads `pages` on the worker through its own read-only connection (WAL lets it read beside the store's writer) |
| Changes         | The store's thread upserts the one page after each recorded visit or title, and clears the index on Clear history           |
| Exposure        | `external(false)`: Spotlight never sees history                                                                             |

The address field's query:

```rust
Search::new(typed).index("history").limit(suggest::LIMIT).sort("_score:desc").sort("last_visit:desc").sort("visit_count:desc")
```

- **Ranking now:** relevance first, so a page whose host begins with what is typed comes before one that only has it
  in the title or path; then the latest visit, then the most visits.
- **Ranking later:** `visit_count`, `last_visit` and every visit's time in `visits` are the inputs Firefox's frecency
  uses (a visit count weighted by how recent each visit is). Blending a number into relevance needs a ranking rule the
  search engine does not have (`design/search.md` section 6.1 rejects `function_score`). That is a bingux-ui change
  with a second user to name (Files' recent places, Media's play counts), made when the omnibox needs it, not here.
- **Limits:** the address field asks for `suggest::LIMIT` (7) hits; a Library page pages with `offset` up to the
  engine's `MAX_PAGE` (1000). The index holds at most the store's cap of 50,000 pages, well inside the engine's
  default `Limits`.
- **One path:** `History::matching` and its scoring (`B/model/history.rs`, `parity.md` P9) are deleted. The pure
  `suggest()` takes the history hits as rows instead of the `History` struct, so it stays tested without a window.
- **A private or guest window registers no history index.** It records no visit (as today), so its address field
  shows the typed row and open tabs only. A profile whose store cannot be kept (`NotSaved`) still registers the
  index: the visits of this run are found, they are only not written.

### Vivarium proposals (not built; nothing in `~/dev/vivarium` is changed)

**V1. Clear data by kind, site and time** (extends P12).

```c
typedef enum {
    VV_CLEAR_HISTORY      = 1 << 0,  /* Places visits and link colours, session history of open sessions */
    VV_CLEAR_COOKIES      = 1 << 1,
    VV_CLEAR_STORAGE      = 1 << 2,  /* localStorage, IndexedDB, Cache API, service workers */
    VV_CLEAR_CACHE        = 1 << 3,
    VV_CLEAR_PERMISSIONS  = 1 << 4,
    VV_CLEAR_SITE_SETTINGS = 1 << 5, /* content prefs: zoom */
} vv_clear_kind;
typedef struct vv_clear_data {
    uint32_t struct_size;
    uint32_t kinds;          /* vv_clear_kind bits */
    vv_str base_domain;      /* empty: every site */
    int64_t since_unix_ms;   /* 0: from the beginning */
    int64_t until_unix_ms;   /* 0: until now */
} vv_clear_data;
/* An operation: completes with VV_EVENT_OPERATION_DONE. nsIClearDataService.deleteDataFromBaseDomain and
 * deleteDataInTimeRange, which the engine already calls in VivariumContexts.cpp. */
VV_API vv_status vv_engine_clear_data(vv_engine*, const vv_clear_data*, vv_operation_id* out);
```

Who uses it: the browser (Clear history, Forget this site, Library "Clear history…" with a range), Mail and any app
with a sign-in sheet (sign out), kiosk and shared-device apps (reset between people), any embedder with a "clear
data" or "reset" command. Rust: `Engine::clear_data(ClearData) -> Result<Operation>` in `crates/vivarium`.

**V2. Visit recording off** for an engine.

```c
/* vv_engine_config.flags */
#define VV_ENGINE_NO_VISIT_HISTORY (1u << N)  /* places.history.enabled = false, locked: no visits, no :visited */
```

Who uses it: every embedder that is not a browser and shows pages it did not choose to remember: Mail's sign-in
sheet, a help viewer, a documentation app, a kiosk. The browser itself does not set it.

Neither proposal blocks this slice. Status (2026-11-26): V1 passes Kieran's test (Mail and other embedders must clear a
site's data) and Main takes it to Vivarium. V2 stays a proposal with the embedders named above.

## Migration from today's single directory

Today the browser writes no file of its own (`parity.md` W15). The only data on disk is the engine's profile at
`$XDG_DATA_HOME/bingux-browser/profile` (made by `run.sh` or the engine), its cache at
`$XDG_CACHE_HOME/bingux-browser/engine` and dumps at `$XDG_STATE_HOME/bingux-browser/crashes`.

First start of this version, with no `profiles.json` (under `profiles.lock`, so two first starts serialise and the
second finds the registry written):

1. **No old profile folder:** create "Personal" (accent `blue`), make it the default, write the registry.
2. **An old profile folder:** check it is not in use (below), then:
    - Create `profiles/<id>/` and `rename(profile, profiles/<id>/engine)`: one atomic step on the same filesystem, no
      copy.
    - Move the old crash folder to `$XDG_STATE_HOME/bingux-browser/profiles/<id>/crashes` the same way.
    - The old engine cache is a cache: the store's thread removes it; it is not moved.
    - Write the registry with "Personal" as the default.

    Nothing else changes: cookies, storage, permissions and Places are Gecko's files and move with the folder.
    Vivarium keeps no installed add-on across runs (`VV/docs/abi.md`, add-ons), so no absolute path inside the
    profile points at the old place.

3. **The old folder is in use:** an older Browser still runs from before the update. Gecko holds an `fcntl` write lock
   on `profile/.parentlock` while it runs. The migration tries a non-blocking `F_OFD_SETLK` on that file; if it is
   held, nothing moves, no registry is written, and the window shows the state `ProfileMoveWaiting` ("Browser is
   still open from before the update. Close it, then try again"), with Try again.
4. **The rename fails** (`EXDEV`: the old folder is its own mount; `EACCES`, `EROFS`): nothing is moved and nothing is
   written; the state is `ProfileMoveFailed { reason }` with the folder named. A bind-mounted profile is a choice the
   person made; the browser does not copy gigabytes on its own.

After a registry exists, an old `profile` folder that reappears is ignored. A registry the browser cannot parse is
never overwritten by a fresh one (that would orphan every profile): see "Failure modes".

## Isolation: the private window writes nothing

A private window is a process of its own today (`BINGUX_BROWSER_PRIVATE`, application id `bingux-browser-private`,
an engine profile under `$XDG_RUNTIME_DIR` removed with the window, `config.rs` `forget`). This slice adds:

- **It belongs to a profile for its settings only.** Opened from a window it gets that profile's id
  (`--profile ID`); from the command line, the default profile's. It reads that profile's `settings.json` and the
  registry with plain reads. It opens no SQLite file, takes no `flock`, creates no lock file and writes no setting.
- **Its history:** it records none (as today: `Options::private` keeps visits out), and it registers no `history`
  searchable.
- **A setting changed in a private window** applies to that window and is not saved; the model's settings state is
  `NotSaved(Private)` so the page can say "Changes here last until this window closes".
- **A guest window** (Settings 10) is a private window that reads no profile at all: default settings, no registry
  read. Data side: `Options { private: true, profile: None }`. The menu item is slice 8.
- **The test:** a private `Browser` over the test double, on a fresh `XDG_DATA_HOME` with one profile in it, visits
  pages, changes the search engine and clears history. The test lists every path below the data, cache and state
  folders (path, size, mtime, mode) before and after: the two lists are equal. The real-engine check adds the
  runtime folder: after the window closes, nothing of it is left.

## Concurrency and locking

- **One process per profile.** `profiles/<id>/lock` is `flock(LOCK_EX | LOCK_NB)`ed when the profile opens and held
  until the process exits. The kernel drops it on a crash, so there is no stale lock to clean up and no PID check.
  The engine's own `.parentlock` on `engine/` agrees, but it is taken later (when Gecko starts) and only for the
  engine's folder; the browser's lock covers the store and is taken before anything is read.
- **A second process for the same profile** is normally impossible: a second launch hands its addresses to the
  running window by application id. When it happens anyway (a launch with another application id, a race at start),
  the loser opens no store and no engine and shows `ProfileInUse { name }` ("Personal is open in another Browser
  window"), with Close.
- **The registry** is changed by any profile's process (rename Work from Personal's Settings). Every change is
  read-change-replace under `flock` on `profiles.lock`, held only for that operation.
- **Inside a process** the store has one thread that owns the SQLite connection. The model sends it commands
  (record a visit, set a title, clear, load) over a channel and takes its results through `Browser::poll`, the same
  way engine events arrive (`B/model/browser/events.rs`). Nothing touches a file on the UI thread
  (`CONTRIBUTING.md`, "Nothing may block the UI thread").
- **Writes are batched:** the thread commits what has queued at most once a second, and on quit (bounded, as the
  engine's own quit is). A crash loses at most the last second of history.
- **Startup order:** in `main`, before the window: resolve the profile (registry read, migration rename, `flock`).
  Each is a handful of system calls. Opening the database, migrating its schema and loading history happen on the
  store's thread after the window shows; until history arrives, suggestions use the visits of this run.
- **Process model and application id per profile** (one app id for every profile, or one per profile, and what the
  dock shows) are slice 8 decisions. The lock makes any of those answers safe; this slice keeps `bingux-browser` for
  a window opened without `--profile`.

## Failure modes and the states the UI shows

Each failure is a value in the model, never a panic and never a silent loss. There are two levels:

- **The profile cannot be opened at all.** The window shows a page of the window-wide kind, like "engine stopped".
- **One part of the store cannot be kept.** The window works; the settings page and a one-time toast say what is not
  kept.

`ProfileState` (in `B/model/profiles.rs`):

| State                                 | When                                                          | What the window shows                                                                                                               | Actions           |
| ------------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| `Open { id, name }`                   | Lock taken                                                    | The browser                                                                                                                         | -                 |
| `InUse { name }`                      | `flock` held by another process                               | "Personal is open in another Browser window"                                                                                        | Close             |
| `RegistryDamaged { kept_as }`         | `profiles.json` does not parse                                | "Browser's list of profiles is damaged". The file is kept as `profiles.json.damaged-<time>` only when the person chooses Start over | Start over, Close |
| `RegistryTooNew { found, supported }` | `version` above this build's                                  | "Profiles were set up by a newer Browser". Nothing is written                                                                       | Close             |
| `ProfileMoveWaiting`                  | Migration found the old folder in use                         | As in "Migration" step 3                                                                                                            | Try again, Close  |
| `ProfileMoveFailed { reason }`        | Migration rename failed                                       | The folder and the reason                                                                                                           | Close             |
| `Unwritable { path, reason }`         | Folder cannot be created or locked: read-only mount, `EACCES` | "Browser cannot keep anything in <path>: <reason>"                                                                                  | Close             |
| `UnknownProfile { asked }`            | `--profile` names no profile                                  | "There is no profile called <asked>"                                                                                                | Close             |

`Keeping` (one for settings, one for history, in the `Browser` model):

| State                                   | When                                                                                              | What the person sees (settings page row subtitle; one toast the first time)                                                                     |
| --------------------------------------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `Saved`                                 | The last write succeeded                                                                          | Nothing                                                                                                                                         |
| `Loading`                               | History not read yet                                                                              | Nothing: the page keeps its layout (loading contract)                                                                                           |
| `NotSaved(Private)`                     | A private or guest window                                                                         | "Changes here last until this window closes"                                                                                                    |
| `NotSaved(DiskFull)`                    | `ENOSPC` / `SQLITE_FULL` on a write                                                               | "The disk is full: history is not being saved". Kept in memory; the next write tries again; the state clears when a write succeeds              |
| `NotSaved(ReadOnly)`                    | `EROFS`, `EACCES`, `SQLITE_READONLY`                                                              | "History cannot be saved: <folder> is read-only"                                                                                                |
| `NotSaved(TooNew { found, supported })` | `user_version` or `version` above this build's                                                    | "This profile was used by a newer Browser. Update Browser to keep history". Read nothing, write nothing                                         |
| `NotSaved(Damaged { kept_as })`         | `SQLITE_CORRUPT`, `SQLITE_NOTADB`, a failed `quick_check`, or `settings.json` that does not parse | "History could not be read". Action Start over: the damaged file is renamed `browser.sqlite.damaged-<time>` and a new one made. Never automatic |
| `NotSaved(Failed(text))`                | Any other error                                                                                   | The error text                                                                                                                                  |

Rules that hold in every row:

- **Nothing is overwritten that could not be read.** A damaged or newer file is left as it is; only Start over moves
  it aside, and only after the person chose it.
- **The window always works.** A store that cannot keep anything still gives a browser with history for this run.
- **Writes of the JSON files** go to a new file in the same folder, `fsync`, `rename` over the old one, `fsync` the
  folder. A full disk leaves the old file whole.

## Security

- **Modes.** `bingux-browser/`, `profiles/` and each profile folder 0700; every file 0600. Set with `mode` at
  `open`/`mkdir` and an `fchmod` after, so a loose umask cannot widen them. The engine already refuses a profile
  folder, or a parent of it, that group or others can write (`VV_ERR_PROFILE_UNSAFE`); the store's folders are those
  parents.
- **No secrets.** History and settings only. Passwords are slice 11 (Gecko's `key4.db` or the system keyring: open,
  `api-parity.md` A2), never this store.
- **History is private data.** It is never logged: store errors name the file and the SQLite code, not a URL.
  `secure_delete` makes Clear history overwrite the rows.
- **Paths from the registry are not trusted as paths.** An `id` must match `[a-z2-7]{8}`; the folder is always
  `profiles/<id>`. A hand-edited registry cannot point the browser at another folder, or make delete remove one.
- **Delete** removes only the three folders derived from the id, without following symbolic links.

## Owner split: engine, browser, bingux-ui

| Owner                      | Holds                                                                                                                                                                                                       |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Engine (Vivarium, Gecko)   | Cookies, site storage, cache, permissions (`permissions.sqlite`), certificate exceptions, site zoom (content prefs, P14), visited links (Places), extensions; the engine profile lock and its safety checks |
| Browser (`bingux-browser`) | The registry, profile folders and their lock, `settings.json`, `browser.sqlite` (history now, bookmarks and the rest later), migration, the failure states, the private-window guarantee                    |
| bingux-ui                  | No storage code. Its web view takes the profile's folders from the host; it no longer reads `BINGUX_BROWSER_PROFILE` itself                                                                                 |

The bingux-ui change:

- `UI/web_view/engine/config.rs` `Config::from_environment(private)` becomes `Config::from_environment(folders)`.
- `folders` is an `EngineFolders`: `Private`, or `Profile { profile, cache, crash, runtime }`.
- The browser builds it from the registry, or from `BINGUX_BROWSER_PROFILE`.
- Everything else config reads from the environment stays: Remote Settings, file folders, WebRTC policy, locale, SDK.

It is a general shape: any embedder that keeps more than one profile, or keeps its engine profile inside its own data
folder, must name the folders itself.

## Build order, tests first

One commit per step, each with its red and green runs in the body.

1. **Registry.** create; rename (rules, duplicates); set colour; default; ask at start; `startup` resolution. Delete:
   the last refused, an open one refused, the default moves, `deleting` finished at the next start. Modes 0700 and 0600. A hand-edited id refused. Damaged and too-new registry.
2. **Engine folders from the host.** `UI/web_view` config: a profile's folders reach the engine directories; private
   unchanged; bingux-ui no longer reads `BINGUX_BROWSER_PROFILE`.
3. **Migration.** No old folder. Old folder moved with its files and crash folder. Old folder in use (an OFD lock held
   by the test) leaves everything. A failed rename leaves everything. Two migrations racing.
4. **Profile lock.** A second open of the same profile is `InUse`; the lock goes when the holder drops; a read-only
   folder is `Unwritable`.
5. **Settings persisted.** The search engine survives a new `Browser`; stable ids; unknown fields and an unknown
   engine kept. Damaged, too new and disk full (a real `ENOSPC` from `/dev/full`) each give their `Keeping` state; a
   later good write clears it.
6. **History persisted and searchable.** Visits and late titles survive a new store; Clear history empties the file and
   the index, and its scope says the engine's link colours stay; the cap trims. The `history` searchable loads what
   the file holds and ranks a host prefix first. Damaged (garbage bytes), too new (`user_version` 99), full
   (`max_page_count`, a real `SQLITE_FULL`) and locked each give their state. Start over keeps the damaged file.
7. **Private window writes nothing.** The tree walk of "Isolation".
8. **Settings page and docs.** The page shows each `Keeping` state. `docs/browser.md` ("where things live") and
   `run.sh` updated; `parity.md` rows W15, W19 and Settings 10 updated.

Checks: `cargo test -p bingux-browser --features snapshot`, the `UI/web_view` tests, `scripts/ui-lint.py` and
`scripts/taxonomy-check.py` on the touched files. Real engine, headless sway: visit pages, quit, start again, the
pages are in the suggestions; a private window leaves no file. Evidence in
`~/.cache/a37-box-scripts/browser-profiles-2026-11-26/`.
