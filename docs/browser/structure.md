# Browser: source structure

Design of 2026-11-26 for how `apps/browser/src` is laid out: by feature, with each feature owning its state, its
behaviour, its components, its tests and its strings. This is the plan for the move. Nothing here has moved yet.

Paths:

- `B/` is `apps/browser/src`.
- `UI/` is `crates/ui/src`.
- "Today" means the tree at `d9d95f19`.

Kieran's brief (2026-11-26): structure the browser "impl based and component/feature driven, so everything is
structured nicely and it isn't all messy and nasty". Browser strings are localised from day one with Project Fluent
(`docs/bingux-ui.md`, "Localisation").

## What is wrong today

`B/` is 13.7k lines in layers: `model/` (state and rules), `ui/` (components), `profile/` (storage). A change to one
feature touches all three layers, and nothing says which code belongs to which feature.

| Symptom                                                                | Where                                                               |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------- |
| One 21-field `Browser` that every feature writes to                    | `model/browser/mod.rs`                                              |
| Its methods spread over five files by kind, not by feature             | `commands.rs`, `events.rs`, `kept.rs`, `lifecycle.rs`, `prompts.rs` |
| One 150-line `match` that touches tabs, history, find, prompts         | `events.rs` `apply_tab`                                             |
| 27 public free functions in the model that take a feature's type first | `address`, `downloads`, `failure`, `security`, `shortcuts`          |
| Stateful pieces built by functions that return an `Element`            | `ui/pages.rs`, `ui/toolbar.rs`, `ui/popovers.rs`                    |
| A `Chrome` bag of 14 unrelated `State`s                                | `ui/ctx.rs`                                                         |
| 748 lines of snapshot scenarios for every feature in one file          | `snapshot_mode.rs`                                                  |
| 786 + 267 lines of tests for every feature in two files                | `model/tests.rs`, `model/engine_tests.rs`                           |
| Strings written in model and UI code                                   | `failure.rs`, `security.rs`, `ui/*`                                 |

## Options considered

Five ways to structure it. The last column is why each was kept or dropped.

| Option                                  | Pros                                                                                                          | Cons                                                                                                                         | Verdict                                 |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| A. Keep the layers, split the big files | Cheapest. No behaviour risk                                                                                   | Does not answer "feature driven". A feature still lives in three folders                                                     | Dropped                                 |
| B. Feature folders, keep `impl Browser` | Small moves. Features visible in the tree                                                                     | `Browser` stays a god object: any feature can write any field. Folders hide, not bound                                       | Dropped                                 |
| C. Feature folders that own their state | A feature is one folder. Fields private. Tests, strings, scenarios beside the code. A new feature is a folder | A thin aggregate and an outbox are needed for what crosses features. More types to name                                      | **Chosen**                              |
| D. One crate per feature                | The compiler enforces every boundary                                                                          | A tab holds state of eight features and events touch all of them: cycles. 20 manifests. Packaging and `crate_dirs.py` change | Later, if a second app embeds a feature |
| E. Elm style: one `Msg`, one `update`   | One place for every change. Effects explicit                                                                  | Fights bingux-ui, where handlers call `ctx.with(..)` on a `State`. A rewrite, so behaviour risk                              | Dropped                                 |

Why C. It is the one option that answers the brief without a rewrite. Every move is mechanical or a local
refactor, so each can be proved with the snapshots and the tests. Option D stays open: C's rules (private
modules, one public API per folder, an engine adapter) are what a crate split would need.

## Target layout

```text
src/
  main.rs               thin: parse the launch, open the profile, start the engine, run
  locale.rs             registers browser.ftl; the only free function besides main

  app/                  the shell: window, layout, context, launch
  browser/              the aggregate: Browser, Outbox, flows across features
  engine/               the adapter boundary: the only code that speaks to BrowserBackend

  tabs/                 the tab list, closed tabs, the strip, pop-ups, the tab menu, the phone sheet
  address/              what was typed, search engines, suggestions, the address field
  navigation/           a tab's location and history; Back, Forward, Reload, Stop
  page/                 the content area: the web view, link status, load bar, size, fullscreen
  error_pages/          a load that failed, a tab that crashed, a certificate problem
  new_tab/              the new tab page
  downloads/            the list, its button, its popover
  find/                 find in page
  permissions/          remembered site decisions and the permission banner
  prompts/              what a page asks and waits for: dialogs, sign-in, select, file picker
  site_info/            the connection summary, its button and popover
  zoom/                 the zoom ladder and chip
  commands/             the Command vocabulary, key bindings, dispatch
  menus/                main menu, page context menu, shortcut text
  history/              the history record, its database, its searchable
  settings/             the values, their file, the Settings page
  profiles/             the registry, lock, places, opening a profile, the profile gate
  store/                the store thread, atomic files, what is kept
  first_run/            the move of the old profile into "Personal"
  private/              private windows
  snapshot/             the headless harness (feature `snapshot` only)
```

`model/`, `ui/` and `profile/` are gone. No folder is called `util`, `helpers`, `common` or `shared`.

## A feature folder

Every feature folder has the same shape. Not every file exists in every folder.

```text
downloads/
  mod.rs        the public API: `pub use` of the state type and the components
  state.rs      `Downloads`, the state type, fields private, behaviour in `impl Downloads`
  transfer.rs   a type for one thing the feature talks about, with its methods
  button.rs     `DownloadsButton`: an `impl Component`
  popover.rs    `DownloadsPopover`, `DownloadRow`: one `impl Component` each
  tests.rs      `#[cfg(test)]`: what the feature promises, over the engine test double
  snapshot.rs   `#[cfg(feature = "snapshot")]`: the scenarios the feature owns
```

What a feature owns:

| Part                | Form                                                                                              |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| State               | One public type with private fields. A window-wide one lives in `Browser`; a per-tab one in `Tab` |
| Behaviour           | `impl` blocks of that type. Rules, engine calls, what an engine event does to it                  |
| Messages            | Its `Command` variants, its `UiRequest` variants, its `Notice`s, its `Outcome` returns            |
| Components          | One `impl Component` for each card, row, button or popover that holds a hook                      |
| UI state            | A small `Copy` struct of `State`s, built by the feature's own `use_` hook, held by `Ctx`          |
| Words               | Messages `browser-<feature>-*` in `locale/en-US/browser.ftl`, asked for by the type's methods     |
| Tests and scenarios | `tests.rs` and `snapshot.rs` beside the code                                                      |

## Rules

1. **A feature is a folder.** Its `mod.rs` names the public API with `pub use`. Submodules are `mod`, never
   `pub mod`, so `crate::downloads::popover::x` does not compile outside the folder.
2. **State has an owner and private fields.** Behaviour is in `impl` blocks of the owner. Others call methods.
3. **No feature reaches into another's internals.** `tabs` does not read `find`'s fields. It calls `Find`'s methods.
4. **Cross-feature work has two homes.** `browser/flows.rs` for what a person's action does to several features.
   `engine/events.rs` for what an engine event does. Each function there is a few calls into owners' methods.
5. **A function whose first argument is a feature's type is a method of it.** Free functions are allowed for
   `main`, `locale::register`, tests, snapshot scenarios, and a private helper beside its one user.
6. **No free-floating helper modules.** If a helper has no owner, find the type it is about, or make that type.
7. **One component per piece with hooks.** A card, a row or a button with state is its own `impl Component`
   (`docs/bingux-ui.md`, "Nothing blocks the UI thread"). A function that returns an `Element` is for a stateless
   fragment of one component only.
8. **The shell is thin.** `app/` lays features out and holds the context. It has no feature logic.
9. **The engine stays behind `engine/`.** Only `engine/` names `BrowserBackend`, `BrowserEvent`, `TabEvent`,
   `Request`, `Response` or `TabConfig`. Features may use the engine's data types (`LoadError`, `Security`,
   `DownloadInfo`) as vocabulary.
10. **Words come from the owner.** A user-visible string is a Fluent message asked for through `tr!` or
    `i18n_id!` inside the owner's method. A component never builds a sentence.
11. **Tests and scenarios live beside what they test.** A test that needs a whole `Browser` uses
    `browser::testing`.
12. **A new feature is a folder, one `Browser` field, one `Ctx` field if it has UI state, one `Command` variant if
    it has a command, one line in the scenario table, one group in `browser.ftl`.**

Bookmarks, the history manager and passwords, which `parity.md` plans, follow rule 12. None exists yet, so none
gets an empty folder.

## How features talk

### Engine adapter

`engine::Engine` is one handle: `Arc<dyn BrowserBackend>`, the capabilities, and the status. It replaces
`ui::Engine`, `Browser::backend`, `Browser::capabilities` and `EngineStatus`.

```rust
impl Engine {
    /// Runs an engine call. A failure other than "no such tab" becomes a notice in `out`.
    pub fn call(&self, out: &mut Outbox, call: impl FnOnce(&dyn BrowserBackend) -> Done);
    pub fn can(&self) -> Capabilities;
    pub fn status(&self) -> &Status;
}
```

A feature method that talks to the engine takes `&Engine` and `&mut Outbox`. Both are fields of `Browser`, so a
call such as `self.find.start(tab.session, &self.engine, &mut self.out, text)` borrows disjoint fields.

`engine/events.rs` is the only router: `Engine::poll` turns each `BrowserEvent` into one call on the owner
(`tab.navigation.on_location(..)`, `tab.find.on_result(..)`, `downloads.on_info(..)`, `prompts.on_request(..)`).
The owner decides what the event means. `engine/` knows which feature, never what it does.

### Outbox

`Outbox` is what a feature says to the person and to the window, taken by `Ctx::with` after each call:

- `Notice`: a toast, with an optional action that runs a `Command`.
- `UiRequest`: something only the window can do (`FocusAddress`, `OpenFind`, `Fullscreen(bool)`, ..).

It replaces `Browser::notices` and `Browser::ui`. `UiRequest` stays one enum in `browser/outbox.rs`. `Ctx` hands
each variant to the owner's UI state (`ctx.address.focus()`, `ctx.find.focus()`).

### Per-tab features

A tab is a composition, owned by `tabs/`. Each substate belongs to one feature and keeps its fields private.

| Field of `Tab`              | Type         | Owner         | Holds today                                                                                       |
| --------------------------- | ------------ | ------------- | ------------------------------------------------------------------------------------------------- |
| `key`, `kind`, `session`    | plain        | `tabs`        | identity and engine session                                                                       |
| `pinned`, `opener`, `audio` | plain        | `tabs`        | strip state                                                                                       |
| `navigation`                | `Navigation` | `navigation`  | `url`, `title`, `favicon`, `load`, `can_back`, `can_forward`, `history`, `history_index`, `hover` |
| `site`                      | `Security`   | `site_info`   | `security`                                                                                        |
| `zoom`                      | `Zoom`       | `zoom`        | `zoom: f64`                                                                                       |
| `find`                      | `Find`       | `find`        | `find`                                                                                            |
| `prompts`                   | `Prompts`    | `prompts`     | `prompts: VecDeque<Prompt>`                                                                       |
| `problem`                   | `Problem`    | `error_pages` | `gone`, `cert`                                                                                    |
| `fullscreen`                | `bool`       | `page`        | the page asked for fullscreen                                                                     |

A per-tab method takes the tab's session, so it never needs the `Tab`:
`find.start(session, &engine, &mut out, text)`. `Tab::page()` becomes `page::Page::of(&Tab)`, reading the
substates' public accessors.

### Flows

`browser/flows.rs` is the one file where a function writes more than one feature. Today's flows:

| Flow                         | Features it joins                                                    |
| ---------------------------- | -------------------------------------------------------------------- |
| `open_tab`                   | `address` (what was typed), `tabs`, `page` (size), `engine`          |
| `close_tab`, `reopen_closed` | `tabs`, `menus` (drops its menu), `engine`, `outbox` (`CloseWindow`) |
| `select`                     | `tabs`, `page` (show and hide sessions)                              |
| `go`                         | `address`, `tabs` (a settings tab becomes a page), `navigation`      |
| `accept_new_tab`             | `tabs` (pop-ups), `engine`                                           |
| `answer_permission`          | `prompts`, `permissions`, `engine`                                   |
| `restart_engine`             | `engine`, `tabs`, `page`                                             |
| `open_first`                 | `tabs`, `engine`                                                     |
| `set_search_engine`          | `address`, `settings`, `store`                                       |

If `flows.rs` passes 300 lines it splits by flow into `browser/flows/`. A function there that fits one owner
moves into it.

### Context

`Ctx` is `Copy`: the `State<Browser>`, one UI-state struct per feature that has one, `Overlays`, `Window`,
`Platform`. It replaces `Chrome`.

| `Chrome` field today                                            | Owner       | Becomes                   |
| --------------------------------------------------------------- | ----------- | ------------------------- |
| `address`, `typing`, `focus_address`, `highlight`, `field_area` | `address`   | `address::FieldState`     |
| `focus_find`                                                    | `find`      | `find::BarState`          |
| `info_open`, `info_anchor`                                      | `site_info` | `site_info::PopoverState` |
| `downloads_open`, `downloads_anchor`                            | `downloads` | `downloads::PopoverState` |
| `menu_anchor`                                                   | `menus`     | `menus::MainState`        |
| `page_origin`                                                   | `page`      | `page::AreaState`         |
| `dialog_text`, `dialog_password`                                | `prompts`   | `prompts::FieldState`     |

A component reads `ctx.browser` through its feature's public API and its own UI state from `ctx`.
Each feature builds its states with a `use_` hook that `app/content.rs` calls once, unconditionally, so the hook
order never changes.

## Feature by feature

What each feature owns, and what others may call. "Messages" are its `Command` variants and outbox entries.

| Feature       | State type                                                | Components                                                                | Messages                                        | Others call                                           |
| ------------- | --------------------------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------- | ----------------------------------------------------- |
| `tabs`        | `Tabs` (list, active, closed), `Tab`, `Place`             | `Strip`, `TabsButton`, `TabsSheet`, `TabMenu`                             | New, Close, Reopen, Next, Previous, Go to, Move | `open`, `close`, `select`, `active`, `get`            |
| `address`     | `Typed`, `SearchEngine`, `Address<'_>`, `Suggestion`      | `AddressBar`                                                              | Focus address; `FocusAddress`                   | `Typed::of`, `SearchEngine::resolve`, `Address::host` |
| `navigation`  | `Navigation` (a tab's location, load, history)            | `HistoryButtons`, `ReloadButton`                                          | Back, Forward, Reload, Stop                     | `go`, `back`, `forward`, `reload`, `stop`             |
| `page`        | `Page`, `View` (size, scale), `Environment`, `Fullscreen` | `PageArea`, `TabPage`, `LinkStatus`, `LoadProgress`                       | Fullscreen; `Fullscreen(bool)`                  | `Page::of`, `View::resize`                            |
| `error_pages` | `Problem` (crash, certificate), `ErrorPage`               | `LoadFailedPage`, `CrashedPage`, `CertErrorPage`                          | none; cert answers are methods                  | `Problem::page`, `proceed`, `go_back`                 |
| `new_tab`     | none (reads `history`)                                    | `NewTabPage`                                                              | none                                            | none                                                  |
| `downloads`   | `Downloads`, `Transfer<'_>`                               | `DownloadsButton`, `DownloadsPopover`, `DownloadList`, `DownloadControls` | Downloads; `OpenDownloads`; "Show" toast        | `on_info`, `list`, `clear_finished`, `unseen`         |
| `find`        | `Find` (per tab)                                          | `FindBar`                                                                 | Find, Find next, Find previous; `OpenFind`      | `start`, `step`, `close`, `on_result`                 |
| `permissions` | `Permissions` (decisions by origin)                       | `PermissionBanner`                                                        | none                                            | `decided`, `remember`, `for_origin`, `forget`         |
| `prompts`     | `Prompts` (per tab queue), `Prompt`                       | `PromptHost`, `SelectPrompt`                                              | none                                            | `push`, `front`, `answer_*`, `forget`                 |
| `site_info`   | `Summary`, `Tone`                                         | `ConnectionButton`, `SiteInfoPopover`                                     | Page info; `OpenPageInfo`                       | `Summary::of`                                         |
| `zoom`        | `Zoom` (per tab)                                          | `ZoomChip`                                                                | Zoom in, Zoom out, Reset                        | `stepped_in`, `stepped_out`, `percent`                |
| `commands`    | `Command`, `Platform`                                     | none                                                                      | the vocabulary itself                           | `Browser::run`, `Platform::window_command`            |
| `menus`       | `ContextMenu` (pending menu)                              | `MainMenu`, `MenuButton`, `ContextMenuHost`                               | Main menu; `OpenMainMenu`                       | `ContextMenu::open`, `dismiss`                        |
| `history`     | `History`, `HistoryDb`, `Page`, `Change`                  | none yet (the manager is slice 5)                                         | none                                            | `record`, `len`, `clear`, `suggestions`               |
| `settings`    | `Settings` (values and file)                              | `SettingsPage`, `EngineGroup`, `SandboxGroup`, `PrivacyGroup`             | Settings                                        | `search`, `set_search`, `save`                        |
| `profiles`    | `Registry`, `Profiles`, `ProfileLock`, `Places`           | `GateWindow`                                                              | none                                            | `Opened::open`, `Profiles::list`                      |
| `store`       | `Store`, `Keeping`, `AtomicFile`, `Folder`                | none                                                                      | `Notice` when keeping changes                   | `save_settings`, `record_visit`, `take_events`        |
| `first_run`   | `Migration`                                               | none                                                                      | none                                            | `Migration::first_run`                                |
| `private`     | `PrivateWindow`                                           | none                                                                      | New private window; `NewPrivateWindow`          | `spawn`, `read_settings`                              |
| `engine`      | `Engine`, `Status`, `EnginePage`                          | `EngineFailedPage`, `Pump`                                                | none                                            | `call`, `can`, `status`, `restart`                    |

## What stays shared, and why

| Item                               | Home        | Why it is shared                                                                               |
| ---------------------------------- | ----------- | ---------------------------------------------------------------------------------------------- |
| `Browser`, `Options`, `Profile`    | `browser/`  | A window has one browser. Features are its fields. It is the only aggregate                    |
| `Outbox`, `Notice`, `UiRequest`    | `browser/`  | Every feature talks to the person and to the window the same way                               |
| `Command`                          | `commands/` | Menus, shortcuts and toast actions all name commands. One vocabulary, owned by one folder      |
| `Engine`                           | `engine/`   | The adapter every feature calls and no feature bypasses                                        |
| `Ctx`                              | `app/`      | One `Copy` handle for components, as bingux-ui apps use. It holds feature UI states, not logic |
| `Keeping`, `AtomicFile`, `Folder`  | `store/`    | Settings, history and the registry all ask what is kept and write files the same way           |
| `Address<'_>`                      | `address/`  | `site_info`, `permissions` and `tabs` need a host or an origin. They ask `Address`             |
| `browser::testing`                 | `browser/`  | The fixture (`Browser` over the test double) every feature test starts from                    |
| Engine data types, bingux-ui types | upstream    | Vocabulary, not behaviour. Features name them; they do not wrap them to hide them              |

Foreign types cannot take methods. Where a function took one (`&DownloadInfo`, `&LoadError`), the feature adds a
small view type that borrows it (`Transfer<'_>`, `CertProblem<'_>`) and gives the behaviour to that.

## Where every file goes

### Crate root

| Today                           | Becomes                                                                                                                                         |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `main.rs` options               | `OwnOptions`, `own_options` to `Launch::parse`; `PRIVATE`, `PROFILE`, `USAGE` to `Launch` constants; test to `app/launch.rs`                    |
| `main.rs` restart               | `relaunch`, `restart` to `Launch::relaunch`, `Launch::restart` (`GateWindow` calls them)                                                        |
| `main.rs` windows               | `application` to `App::application`; `run_browser` to `App::run`; `run_gate` to `GateWindow::run`                                               |
| `main.rs` engine                | `start_or_exit`, `start_engine` (both `cfg` arms) to `Engine::start_or_exit`, `Engine::start` in `engine/start.rs`                              |
| `main.rs` `Opened::Profile` arm | `OpenProfile::into_window`, which returns the `Profile`, the folders and the lock                                                               |
| `main.rs` `main`                | stays, about 40 lines: parse, open, start, run                                                                                                  |
| `locale.rs`                     | stays: `register`                                                                                                                               |
| `target.rs`                     | `app/target.rs`: `SCHEME` to `Target::SCHEME`; `address_of`, `normalise` to `Target::address_of`, `Target::normalise`; `clean` private; 3 tests |
| `snapshot_mode.rs`              | split into `snapshot/` and each feature's `snapshot.rs` (below)                                                                                 |

### `model/`

| Today                    | Becomes                                                                                                                                  |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `address.rs` engines     | `SearchEngine`, `ENGINES`, `url_for`, `encode` to `address/search_engine.rs`; `resolve(input, engine)` to `SearchEngine::resolve`        |
| `address.rs` typed       | `Typed`, `classify`, `scheme_of`, `looks_like_host`, `default_scheme` to `address/typed.rs`: `Typed::of(input)`                          |
| `address.rs` urls        | `display`, `host`, `origin`, `is_plain_http`, `initial` to `Address<'_>` methods in `address/url.rs`; 11 tests to `address/tests.rs`     |
| `suggest.rs`             | `address/suggest.rs`: `Source`, `Suggestion`, `OpenTab`, `Visited`, `LIMIT`; `suggest(..)` to `Suggestion::list(..)`                     |
| `downloads.rs`           | `Downloads` to `downloads/state.rs`; `fraction`, `status_line` to `Transfer` methods; `size`, `time_left` private to `Transfer`; 6 tests |
| `failure.rs` pages       | `ErrorPage`, `for_load_error`, `for_process_gone` to `error_pages/`: `ErrorPage::for_load`, `ErrorPage::for_crash`                       |
| `failure.rs` certificate | `cert_problem_text`, `CERT_RISK` to `CertProblem<'_>::text` in `error_pages/certificate.rs`                                              |
| `failure.rs` engine      | `EnginePage`, `for_engine_failure`, `exit_text` to `engine/failure.rs`: `EnginePage::of(..)`; 9 tests split by owner                     |
| `keeping.rs`             | `Keeping`, `NotSaved`, `Part`, `damaged_name` to `store/keeping.rs`; `ClearScope` to `history/`; 2 tests                                 |
| `permissions.rs`         | `Permissions` to `permissions/state.rs`; `label`, `prompt_text` to `Permissions::label`, `Permissions::prompt_text`; 3 tests             |
| `security.rs`            | `Tone`, `Summary`, `summarize` to `site_info/`: `Summary::of(security, url)`; 4 tests                                                    |
| `shortcuts.rs` keys      | `Platform`, `Held`, `is`, `named`, `digit` to `commands/keys.rs`; `window_command`, `page_command` to `Platform` methods                 |
| `shortcuts.rs` words     | `Command` to `commands/mod.rs`; `label(platform, command)` to `Command::label(self, platform)`; 10 tests                                 |
| `tab.rs`                 | `Tab`, `TabKind` to `tabs/tab.rs`; `Prompt` to `prompts/`; `Find` to `find/`; `Page`, `Tab::page` to `page/`; 3 tests                    |
| `tabs.rs`                | `TabList`, `Place` to `tabs/list.rs` (`Tabs`); 10 tests to `tabs/tests.rs`                                                               |
| `zoom.rs`                | `Zoom(f64)`: `LEVELS`, `DEFAULT`, `percent`, `zoom_in`, `zoom_out`, `is_default` become `Zoom` items; 4 tests                            |
| `mod.rs`                 | deleted                                                                                                                                  |

### `model/browser/`

| Today                         | Becomes                                                                                                                                                                                                                                                                                 |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mod.rs` types                | `EngineStatus` to `engine::Status`; `Options` to `browser/options.rs`; `Notice`, `UiRequest` to `browser/outbox.rs`; `ClosedTab` to `tabs/closed.rs`; `PendingMenu` to `menus/context.rs`; `ViewSize` to `page/view.rs`; `Profile` to `browser/profile.rs`                              |
| `mod.rs` `Browser`            | `browser/mod.rs`, fields are feature states; `new`, `with_profile` stay                                                                                                                                                                                                                 |
| `mod.rs` accessors            | `backend`, `capabilities` to `Engine`; `search`, `set_search_engine` to `Settings` and a flow; `session_of`, `key_of`, `active`, `config` to `Tabs`; `notify`, `fail`, `take_notices`, `take_ui` to `Outbox`; `engine_call` to `Engine::call`; `set_environment` to `page::Environment` |
| `commands.rs` tabs            | `open_tab`, `create_web`, `open_settings`, `select`, `shown`, `close_tab`, `reopen_closed`, `duplicate_tab`, `set_pinned`, `move_tab`, `close_others`, `close_after`, `toggle_mute`, `step`, `shift_tab`, `CLOSED_LIMIT` to `Tabs`; cross-feature parts to flows                        |
| `commands.rs` navigation      | `navigate`, `go`, `shown_again`, `back`, `forward`, `go_to_history`, `reload`, `stop` to `Navigation`; `go` also a flow; `Internal` to `Typed`                                                                                                                                          |
| `commands.rs` page            | `resize`, `set_window_focus`, `send` to `page::View`; `Fullscreen` arm of `run` to `page::Fullscreen`                                                                                                                                                                                   |
| `commands.rs` others          | `set_zoom`, `zoom_by` to `Zoom`; `open_find`, `close_find`, `find`, `set_find_options`, `find_step` to `Find`; `clear_finished_downloads` to `Downloads`                                                                                                                                |
| `commands.rs` `run`           | `Browser::run` to `commands/mod.rs`: one `match`, each arm one call; `send_edit` to `page::View::edit`                                                                                                                                                                                  |
| `events.rs`                   | `poll`, `apply`, `apply_tab`, `apply_request`, `queue_prompt`, `refuse` to `engine/events.rs`; every arm calls the owner                                                                                                                                                                |
| `kept.rs`                     | `take_store_events`, `kept_*`, `by_design` to `Store` and `Keeping`; `has_history`, `history_len`, `history_clear_scope`, `clear_history`, `start_over_history` to `History`; `start_over_settings` to `Settings`; `set_store_waker`, `flush_store` to `Store`                          |
| `lifecycle.rs`                | `engine_failed`, `engine_ready`, `restart_engine` to `engine/lifecycle.rs`; `open_first` to a flow; `accept_new_tab` to `tabs/popups.rs`; `request_closed` to `engine/events.rs`                                                                                                        |
| `prompts.rs`                  | `prompt`, `take_prompt`, `answer_dialog`, `answer_auth`, `answer_select`, `answer_picker` to `Prompts`; `answer_permission` a flow; `cert_go_back`, `cert_proceed` to `error_pages`; `dismiss_menu`, `open_local_menu` to `ContextMenu`                                                 |
| `engine_tests.rs`, `tests.rs` | to the feature tests (table below)                                                                                                                                                                                                                                                      |

### `ui/`

| Today         | Becomes                                                                                                                                                                                                                                                                                            |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `address.rs`  | `AddressBar` to `address/field.rs`; `icon_of` to `AddressBar` private; `move_highlight` to `FieldState::move_highlight`; 1 test                                                                                                                                                                    |
| `app.rs`      | `Handoff`, `App` to `app/mod.rs`; `Content` split into `app/content.rs`, `app/layout.rs`, `Pump`, `TitleSync`; `opened_by` to `Launch::addresses`; `first_is_blank` to `Tabs::first_is_blank`; `ContextMenuHost` to `menus/context.rs`                                                             |
| `ctx.rs`      | `Engine` to `engine/mod.rs`; `Chrome` to feature UI states; `Ctx`, `with`, `run`, `settle`, `pump`, `act` to `app/ctx.rs`; each `act` arm to the owner                                                                                                                                             |
| `find.rs`     | `FindBar` to `find/bar.rs`                                                                                                                                                                                                                                                                         |
| `gate.rs`     | `Gate`, `GateWindow` to `profiles/gate.rs`; `again` to `GateWindow::again`                                                                                                                                                                                                                         |
| `menus.rs`    | `command_item` to `Command::menu_item`; `main_menu` to `MainMenu::items`; `shortcut_text` to `ShortcutSheet::text`; `tab_items` to `TabMenu::items`; `copy`, `context_items` to `ContextMenu`                                                                                                      |
| `page.rs`     | `Shown`, `PageArea` to `page/mod.rs`; link status and load bar to `LinkStatus`, `LoadProgress` components                                                                                                                                                                                          |
| `pages.rs`    | `new_tab`, `NewTab`, `RECENT` to `NewTabPage`; `failed`, `crashed`, `cert_error` to `LoadFailedPage`, `CrashedPage`, `CertErrorPage`; `engine_failed`, `EngineFailed` to `engine/failed_page.rs`                                                                                                   |
| `phone.rs`    | `open_tabs`, `TabsSheet` to `tabs/sheet.rs`: `TabsSheet::open(ctx)`                                                                                                                                                                                                                                |
| `popovers.rs` | `Popovers` split into `SiteInfoPopover` (`page_info`) and `DownloadsPopover` (`download_list`, `download_controls`); `width` to `Ctx::popover_width`; `tone_of` to `Tone::style`                                                                                                                   |
| `private.rs`  | `open` to `PrivateWindow::spawn`                                                                                                                                                                                                                                                                   |
| `prompts.rs`  | `PermissionBanner` to `permissions/banner.rs`; `PromptHost`, `prompt_id` to `prompts/host.rs`; `show_dialog`, `show_auth`, `show_select`, `show_picker`, `field` to `DialogPrompt`, `AuthPrompt`, `SelectPrompt`, `PickerPrompt`                                                                   |
| `settings.rs` | `SettingsPage` to `settings/page.rs`; `settings_text`, `history_text` to `Keeping`, `History` methods; `remote_settings_text`, `engine_state_text`, `user_agent_text`, `status_look`, `overall_text`, `process_group`, `checks_group`, `security_groups` to `EngineGroup`, `SandboxGroup`; 4 tests |
| `strip.rs`    | `strip`, `status_of` to `Strip` component and method; `icon_of` to `Tab::icon`                                                                                                                                                                                                                     |
| `toolbar.rs`  | `Toolbar`, `PhoneBottom` to `app/toolbar.rs`; `history_buttons`, `reload_button`, `connection_button`, `zoom_chip`, `downloads_button`, `menu_control`, `tabs_button` each to its feature's component; `Reading`, `reading` deleted; `tone_ink` to `Tone::ink`                                     |
| `web.rs`      | `Surface`, `TabPage` to `page/web.rs`                                                                                                                                                                                                                                                              |
| `mod.rs`      | deleted                                                                                                                                                                                                                                                                                            |

### `profile/`

| Today         | Becomes                                                                                                                                                                                                                                                                               |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `files.rs`    | `Problem` to `store/files.rs`; `replace`, `write_synced`, `temporary_beside`, `sync_dir`, `read_if_present` to `AtomicFile`; `private_dir`, `size_of`, `remove_tree` to `Folder`; `random_u64` to `Id::random`; 5 tests                                                               |
| `history.rs`  | `Page`, `Change`, `is_recorded`, `document`, `page_of`, `schema`, constants to `history/page.rs`; `open`, `migrate`, `trim`, `pages`, `write`, `clear` to `HistoryDb`; `suggestions`, `recent`, `everything`, `visited` to `History` and `Visited::of`; `not_saved` to `NotSaved::of` |
| `lock.rs`     | `ProfileLock`, `LockError`, `Held` to `profiles/lock.rs`; `flock` private to it; 2 tests                                                                                                                                                                                              |
| `migrate.rs`  | `first_run/mod.rs`: `Migration::first_run`, `Migration::in_use`; `Migrated`, `MigrationError`; 5 tests                                                                                                                                                                                |
| `open.rs`     | `ProfileState`, `OpenProfile`, `Opened` to `profiles/open.rs`; `open` to `Opened::open`; `registry_state` to `ProfileState::of_registry`; `app_id` to `Entry::app_id`; `open_private` to `PrivateWindow::read_settings`; 8 tests                                                      |
| `places.rs`   | `profiles/places.rs`, already methods; 3 tests                                                                                                                                                                                                                                        |
| `registry.rs` | `profiles/registry.rs`; `io` to `RegistryProblem::of_io`; `parse` to `Registry::parse`; `now` private; 13 tests to `profiles/tests.rs`                                                                                                                                                |
| `settings.rs` | `settings/mod.rs`, already methods; 3 tests                                                                                                                                                                                                                                           |
| `store.rs`    | `Store`, `Event` to `store/mod.rs`; `Writer`, `Outbox` to `store/writer.rs` (`Pending`); `keeping_of`, `not_saved`, `writable` to `Keeping`; `sibling`, `move_aside` to `Folder`; `now_ms` private; 6 tests                                                                           |
| `*_tests.rs`  | `history_tests` to `history/tests.rs`; `private_tests` to `private/tests.rs`; `store_tests` to `store/tests.rs`                                                                                                                                                                       |

### Tests and scenarios

`model/tests.rs` (36 tests) and `model/engine_tests.rs` (15) go to the feature each one is about.

| Tests, by name in the file                                                                                  | Destination                |
| ----------------------------------------------------------------------------------------------------------- | -------------------------- |
| typing an address, back and forward, a page that loads and stops                                            | `navigation/tests.rs`      |
| words are searched for, the pages of the browser are reached by name                                        | `address/tests.rs`         |
| closing tabs, a hidden tab shown at the view size, reopening, closing others, muting, a page's window       | `tabs/tests.rs`            |
| a new appearance reaches the engine, page input reaches its tab                                             | `page/tests.rs`            |
| a crashed tab, a failed load, a certificate error refused or proceeded                                      | `error_pages/tests.rs`     |
| a permission asked and remembered, refused and asked again                                                  | `permissions/tests.rs`     |
| dialogs queue, a crashing tab drops its questions, a request for a gone tab is refused                      | `prompts/tests.rs`         |
| find asks the engine, says so when it cannot, is read again when the bar opens                              | `find/tests.rs`            |
| downloads listed, private downloads, clearing the list                                                      | `downloads/tests.rs`       |
| zoom steps along the ladder                                                                                 | `zoom/tests.rs`            |
| the context menu is held for the window                                                                     | `menus/tests.rs`           |
| settings is one tab                                                                                         | `settings/tests.rs`        |
| a private window has no history                                                                             | `private/tests.rs`         |
| print says it is unsupported                                                                                | `commands/tests.rs`        |
| first tab blank and the engine ready; every engine failure and restart test; capabilities follow the engine | `engine/tests.rs`          |
| the four pop-up tests of `engine_tests.rs`                                                                  | `tabs/tests.rs` (`popups`) |
| the engine ending a question takes it off the screen                                                        | `prompts/tests.rs`         |

Snapshot scenarios, in the order of today's table, each beside the feature it shows:

| Scenarios                                                                               | Owner         |
| --------------------------------------------------------------------------------------- | ------------- |
| `newtab`                                                                                | `new_tab`     |
| `page`, `fullscreen`                                                                    | `page`        |
| `tabs`, `tab-menu`, `tab-list`, `popup`                                                 | `tabs`        |
| `loading`, `hover`                                                                      | `navigation`  |
| `permission`, `dialog`, `auth`                                                          | `prompts`     |
| `cert`, `cert-details`, `cert-pinned`, `error`, `crash`                                 | `error_pages` |
| `find`, `find-missing`                                                                  | `find`        |
| `downloads`                                                                             | `downloads`   |
| `info`, `info-plain`, `security`, `security-pending`                                    | `site_info`   |
| `menu`, `context`                                                                       | `menus`       |
| `settings`                                                                              | `settings`    |
| `suggestions`                                                                           | `address`     |
| `zoom`                                                                                  | `zoom`        |
| `private`                                                                               | `private`     |
| `rtl`                                                                                   | `app`         |
| `capabilities`, `engine-failed`, `engine-restart`, `engine-violation`, `profile-unsafe` | `engine`      |

The harness stays in `snapshot/`. It holds:

- `Rig`, with `session`, `key_of` and `tell` as methods.
- `Scenario` and the table, in today's order.
- `sizes`, `run_real`, `run` and `publish`.

The 7 downloads tests in `snapshot_mode.rs` go to `downloads/tests.rs` over a `snapshot::window()` fixture.
Scenario and test functions are free functions by rule 5.

## Words

Every user-visible string moves to `locale/en-US/browser.ftl` with its feature's move. Three kinds of source:

| Where the words are today                                                     | How they move                                                                                    |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Component literals (labels, tooltips, button text)                            | `tr!("browser-<feature>-<thing>")` in the component; tooltips and names are attributes           |
| Model sentences (`failure`, `security`, `downloads`, `permissions`, `Notice`) | The owner's method returns `tr!(..)`. Tests call `locale::register()` through `browser::testing` |
| Tables (`ENGINES`, `Command` labels, `Tone` text)                             | A `const` keeps `i18n_id!` ids; the text is asked for when drawn                                 |

- A sentence with a count is one message with a plural: downloads running, tabs, find "x of y", history pages.
- A sentence with a name is one message with a variable. No `format!` joins a translated part.
- A search engine's own name, a file name and an address are not translated.
- A `Notice` is written when it is raised. A toast already up does not change language. Nothing on screen for
  longer is stored as finished text: pages and rows ask at draw time.
- IDs are `browser-<feature>-<place>-<thing>`, so each feature owns one `##` group in the file.
- The first Fluent commit adds `#[test] fn the_catalogue_is_clean()` in `locale.rs`, unless one exists. It fails on an unused message,
  so each commit adds only the messages its feature uses.

## Phase 2: how the move goes

### Three kinds of commit, per feature

Each feature is three commits, in this order. A regression then names its kind, and git keeps rename history.

| Kind | Subject                                              | Contains                                                                                                                 | Proof                                                               |
| ---- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| a    | `refactor(browser): move <feature> ..`               | `git mv` and the `use` and `mod` lines that follow, and the path updates in `docs/browser/*.md`. No other edit           | Snapshots byte-identical. Test count unchanged                      |
| b    | `refactor(browser): <feature> behaviour on its type` | Free functions to methods, `impl Browser` bodies to the owner, components split, state made private. No behaviour change | Snapshots byte-identical. Tests unchanged except for call syntax    |
| c    | `feat(browser): <feature> strings in Fluent`         | Messages in `browser.ftl`, `tr!` at the use, plurals                                                                     | Catalogue check, pseudo-locale render, snapshots identical in en-US |

A feature with no strings has no kind c. A commit never mixes kinds.

### Order

Leaves first, so each move only updates imports of what is already moved. `Browser` stays in `model/browser/`
until its features have gone; then it moves last.

1. `zoom`, `address`, `commands`, `site_info`, `permissions`
2. `downloads`, `find`, `prompts`, `error_pages`
3. `store`, `history`, `settings`, `profiles`, `first_run`, `private`
4. `navigation`, `page`, `new_tab`, `menus`, `tabs`
5. `engine` (the handle, events, lifecycle, the failure page, start)
6. `browser` (aggregate, outbox, flows, testing) and the context split of `Chrome`
7. `app` (content, layout, context, launch, target, toolbar) and `snapshot/`
8. The structure test (below), docs, the lint entry

### Gates

Run after every commit. `CARGO_TARGET_DIR=/var/tmp/BrowserStructure`, deleted at the end.

```sh
M=apps/browser/Cargo.toml
cargo build  --manifest-path $M
cargo test   --manifest-path $M --features snapshot
cargo clippy --manifest-path $M --all-targets --features snapshot -- -D warnings
cargo fmt    --manifest-path $M -- --check
```

A commit that touches a `cfg(feature = ..)` arm (`engine/start.rs`, `snapshot/real.rs`) also runs
`cargo check --features dynamic-load` with `VIVARIUM_INCLUDE_DIR` and `LIBCLANG_PATH` set as `run.sh` does.
If another agent's uncommitted file fails `fmt`, only the files this commit touched are checked.

### Snapshots

```sh
B=/var/tmp/BrowserStructure/debug/bingux-browser   # built with --features snapshot
$B --snapshot /var/tmp/BrowserStructure-shots/base          # 36 scenarios, 2 sizes: 72 images
$B --snapshot /var/tmp/BrowserStructure-shots/after         # after the commit
cmp -s base/NAME-SIZE.png after/NAME-SIZE.png               # for every pair
```

- A baseline is taken before the first move, then twice, to find any scenario that is not stable. Those are
  named in the first commit message and compared by pixels, not bytes.
- A pair that differs is decoded and the differing pixels counted, with their bounding box. A difference in kind
  a or b is a bug in the commit. In kind c it is a message whose en-US text is not what the literal was.
- If another agent commits to `apps/browser` or `crates/ui` meanwhile, the baseline is taken again from a
  detached `git worktree` at the commit before mine.
- Kind c also renders the feature's scenarios with `BINGUX_PSEUDO_LOCALE=accented` (any plain English left is a
  literal) and `rtl`. They are looked at, not compared.

Test counts come from `cargo test`: the total before and after goes in each commit message. Kinds a and b keep it
equal. The first kind c adds one test.

### Last steps

1. The structure test in `app/structure_tests.rs`: no `pub mod` in a feature's `mod.rs` (rule 1); top-level `fn`
   only in the allowed files (rule 5); `BrowserBackend`, `BrowserEvent`, `TabEvent` named only under `engine/`
   (rule 9). It makes the rules fail a build instead of a review.
2. `docs/browser.md` (module table, `ui/app.rs` reference), `parity.md` and `profiles-and-store.md` paths. Kind a
   commits already carry theirs; this catches what is left.
3. Remove `apps/browser/src` from the allow list in `scripts/ui-lint.py` and show `python3 scripts/ui-lint.py`,
   `python3 scripts/ftl-check.py` and `python3 scripts/copy-lint.py` passing.
4. Delete `/var/tmp/BrowserStructure` and the shot folders.

## Risks

| Risk                                                   | Handling                                                                                |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Private fields need many accessors                     | Prefer a method that answers the question (`tab.can_go_back()`) over a getter per field |
| `Chrome` split changes the order of `use_state`        | Hooks stay unconditional and in one place, `app/content.rs`; the order is fixed once    |
| Borrow conflicts: `&mut self.tabs` with `&self.engine` | They are disjoint fields; a flow takes the pieces apart explicitly                      |
| A snapshot differs only because of timing              | Found by the double baseline before any move                                            |
| `dynamic-load` arms compile only with the SDK headers  | Checked by the gate above when a commit touches them                                    |
| Other agents edit `apps/browser` during the move       | Stage only own paths; re-baseline when a foreign commit lands; never revert theirs      |
| `flows.rs` becomes the new `commands.rs`               | Rule 4: every function is calls into owners; over 300 lines it splits                   |
