# Browser: implicit state machines

Audit of 2026-11-26 of `apps/browser/src` and `crates/ui/src/web_view`, at `988d5229` on `feat/apps`. Kieran's
brief: "clean up any implicit state machines". This is the audit and the plan. No code changed yet.

Paths:

- `B/` is `apps/browser/src`.
- `W/` is `crates/ui/src/web_view`.
- `file:N` is a line in that file at `988d5229`, the commit the audit read. Read it with `git show 988d5229:PATH`.
  Other agents have landed 22 commits since. "Changes since the audit" below says what that moved.

## What counts

An explicit machine is one enum per lifecycle. The variant carries the data that is true in that state.
Transitions are methods on the type that take or return the state. A combination that cannot happen cannot be
written.

An implicit machine is any of these:

- Several `bool`s, `Option`s or flags on one struct whose combinations encode a lifecycle.
- A field that is set in one place and cleared in another (`pending_*`, `is_*`, `has_*`).
- A counter, a timer or a mirror of another field that stands in for a state.
- A string or an integer used as a status code.

Kieran ruled that the engine is a stable target. Branches on `Capabilities` are being deleted, so they are not
states to keep. They are listed in "Not counted" so that no refactor preserves them by accident.

## Method

I read the state types of every feature in `B/` and `W/` in full, and every writer and reader of their state.

- Every `bool`, `Option<_>`, `State<_>`, `Cell<_>` and counter field was found by search.
- Each was traced to every place that writes it and every place that reads it.
- A finding is listed only when the fields can disagree.
- A claim I could not prove from the code is marked `[INFERENCE]`.

Not read in full, only searched: the tests, the snapshot scenarios, `history/`, `W/convert.rs`, `W/config.rs`,
`W/report.rs`, `W/input.rs` and most of the test double.

No code was built or run. There is no cargo on this machine. The red tests below were checked against the test
double's API (`W/test_double/mod.rs`), not executed.

## Changes since the audit

Twenty-two commits landed on `feat/apps` between `988d5229` and the push of this file (`04036f35`). I read the
diffs of the files this audit cites. Three things change a finding. The rest only moves line numbers.

| Change                                                                                    | Effect on the audit                                                                                     |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `Browser::escape_leaves_window_fullscreen` and the engine granting page fullscreen        | F4: Escape no longer does nothing after a switch back. The F11 and mirror findings stand                |
| `open_devtools` writes `tab.load.loading = true` for its new tab, and reads `tab.private` | F2 has one more optimistic write. F5: `Tab.private` has a second reader                                 |
| `Browser.profiles` and `Profile.known` (the profiles for the main menu)                   | F5: still `profile` as a pair, and the private-window bug is unchanged (`ui/ctx.rs` `NewPrivateWindow`) |

Also new, and not a state machine:

- `Command::ClearHistory` with `UiRequest::ConfirmClearHistory`, a dialog.
- The print job map in `W/engine/state.rs` (`prints`, keyed by tag, so explicit).
- `UiRequest::OpenProfile` and `TabEvent::Printed`.

`menus/main.rs` and `menus/phone.rs` are new and were not audited. They belong to BrowserMainMenu. Re-check
them before the chrome commit.

## Findings at a glance

Rank is value over risk. "Real bug" means I found a path to wrong behaviour in the code. "Latent" means the
fields can disagree but I found no path.

| ID  | Machine                              | Where                                                                  | Verdict  | Rank            |
| --- | ------------------------------------ | ---------------------------------------------------------------------- | -------- | --------------- |
| F1  | Questions a page waits on            | `tabs/tab.rs:63`, `browser/prompts.rs`, `prompts/host.rs`              | Real bug | 1               |
| F2  | What a tab's page is doing           | `tabs/tab.rs:48,58,62`, `browser/commands.rs`, `engine/events.rs`      | Real bug | 2               |
| F3  | Engine loss and the tabs it owes     | `browser/mod.rs:140,143`, `tabs/tab.rs:43,44`, `engine/lifecycle.rs`   | Real bug | 3               |
| F4  | Fullscreen                           | `browser/mod.rs:137`, `tabs/tab.rs:61`, `ui/ctx.rs:108`                | Real bug | 4               |
| F5  | Who the window is (profile, private) | `browser/mod.rs:72-82,129-132`, `tabs/tab.rs:57`                       | Real bug | 5               |
| F6  | Find in page                         | `find/mod.rs:17-25`                                                    | Real bug | 6               |
| F7  | Download news                        | `downloads/mod.rs:84-88`, `engine/events.rs:42-72`                     | Real bug | 7               |
| F8  | Finger to mouse translation          | `W/touch.rs:24-39`                                                     | Real bug | 8               |
| F9  | Address field and its popovers       | `address/field.rs:26-38`, `downloads/state.rs`, `site_info/popover.rs` | Latent   | 9               |
| F10 | History file in the store thread     | `store/mod.rs:84-100`                                                  | Latent   | 10              |
| F11 | Engine backend lifecycle             | `W/engine/state.rs:41-57`, `W/engine/runtime.rs:40-51`                 | Latent   | 11              |
| F12 | Dead or copied state                 | `tabs/tab.rs:44,57`, `downloads/mod.rs:87`, `ui/ctx.rs:25-30`          | Clean-up | with F3, F5, F7 |

F1 ranks first because a wrong answer to a permission or sign-in prompt is a trust problem. F2 and F3 ranks
follow from how many files read the fields.

## F1. Questions a page waits on

### Where

| Part                   | Place                                                                |
| ---------------------- | -------------------------------------------------------------------- |
| The queue              | `tabs/tab.rs:63` `prompts: VecDeque<Prompt>`                         |
| Pushed                 | `engine/events.rs:294-297` `queue_prompt`                            |
| Answered               | `browser/prompts.rs:23-89` (five `answer_*`), each reads the front   |
| Removed by the engine  | `browser/lifecycle.rs:91-115` `request_closed`                       |
| Removed on a crash     | `engine/events.rs:205` `prompts.clear()`                             |
| Removed on engine loss | `engine/lifecycle.rs:29` `prompts.clear()`                           |
| Shown                  | `prompts/host.rs:17-43`: a dialog per `(tab key, request id)` change |
| Shared text            | `prompts/state.rs:7-10` `DialogFields { text, password }`            |

### The real states

A tab has no question, or one question on screen and zero or more behind it. The first in the queue is on
screen. A question leaves in one of four ways: the person answers, the engine closes it, the process goes, or
the engine goes.

### Invalid combinations the fields allow

- A dialog on screen for a question that is no longer in the queue. Reachable. `request_closed` removes the
  prompt (`browser/lifecycle.rs:95`). Nothing dismisses the overlay. `grep dismiss_dialog` finds no call in
  `apps/browser`. `Overlays::dismiss_dialog` exists (`crates/ui/src/shell/overlays.rs:170`).
- A button on that dialog answers a different question. Reachable. Every handler captures only the tab key
  (`prompts/dialog.rs:28-32`, `prompts/auth.rs:54,65`, `prompts/select.rs:36,44`, `prompts/picker.rs:26`).
  `answer_dialog` reads the current front (`browser/prompts.rs:41`). With two queued dialogs A and B, if the
  engine closes A, the stale dialog for A answers B.
- The same hazard for a permission. The banner reads the front at render time (`permissions/banner.rs:22-27`)
  but its buttons capture only the key (`:44-55`). [INFERENCE] The window between a front change and the next
  render is a frame, so it is narrow. The dialog case has no such limit.
- `DialogFields.text` is a plain string reused by three kinds. `SelectPrompt` stores the chosen index as a
  decimal string and parses it back (`prompts/select.rs:26-27,43`). `DialogPrompt` answers every kind with
  whatever the field holds (`prompts/dialog.rs:30-31`). The backend ignores the text unless the dialog was a
  `prompt()`. Tolerated by luck.

### User-visible bugs

- An engine-closed `alert` stays on screen. Pressing OK answers the next question the page asked, or nothing.
- A permission request from site B can be granted by a click meant for site A. [INFERENCE] Needs the click to
  land between a close and a render.

### Proposed explicit form

The queue becomes a type that addresses questions by id. The UI remembers which dialog it opened.

```rust
/// The questions of one tab, oldest first. The first is the one on screen.
pub struct Prompts(VecDeque<Prompt>);

impl Prompts {
    pub fn push(&mut self, prompt: Prompt);
    pub fn front(&self) -> Option<&Prompt>;
    /// Take question `id` off the queue. `None` when the engine closed it first.
    pub fn take(&mut self, id: RequestId) -> Option<Prompt>;
    /// The engine ended `id` itself. True when what is on screen changed.
    pub fn closed(&mut self, id: RequestId) -> bool;
    /// Nothing can be answered any more: a crash, the engine, the tab.
    pub fn drop_all(&mut self);
}

/// What the person typed for the question on screen, by kind.
pub enum PromptInput {
    None,
    Text(String),
    Credentials { name: String, password: String },
    Choice(usize),
}
```

### What callers change

- `Tab.prompts` becomes `Tab.prompts: Prompts`. `queue_prompt`, `request_closed`, the crash arm and
  `engine_failed` call its methods.
- Each `answer_*` takes the `RequestId` of the dialog it was built for. It does nothing when `take(id)` is `None`.
- `PromptHost` keeps `opened: Option<RequestId>` in a hook. When the front id differs from it and a dialog is
  open, it calls `overlays.dismiss_dialog()`.
- `DialogFields` becomes `PromptInput`. `SelectPrompt` stores a `usize`, not a string.

### Red tests first

| Test                                                   | Where              | Red because                                                                                            |
| ------------------------------------------------------ | ------------------ | ------------------------------------------------------------------------------------------------------ |
| `a_dialog_the_engine_closed_is_taken_off_the_screen`   | `prompts/tests.rs` | Snapshot rig: the overlay stays open today                                                             |
| `an_answer_for_a_closed_question_answers_nothing_else` | `prompts/tests.rs` | Queue A, B; close A; answer A by id; no `Respond` for B. Does not compile until `answer_*` takes an id |
| `a_select_keeps_its_choice_as_a_number`                | `prompts/tests.rs` | Fails to compile until `PromptInput::Choice`                                                           |

## F2. What a tab's page is doing

### Where

One lifecycle is spread over five fields of `Tab` and the `LoadState` it holds.

| Field           | Place                            | Meaning                                         |
| --------------- | -------------------------------- | ----------------------------------------------- |
| `load.loading`  | `W/page.rs:45`, `tabs/tab.rs:48` | A request is under way                          |
| `load.progress` | `W/page.rs:47`                   | "Only meaningful while `loading`" (its own doc) |
| `load.error`    | `W/page.rs:49`                   | The last load failed with this                  |
| `gone`          | `tabs/tab.rs:58`                 | The content process went away                   |
| `cert`          | `tabs/tab.rs:62`                 | A certificate question the engine waits on      |

The event that drives them is `TabEvent::Load { started, loading, progress, error }` (`W/events.rs:23-28`).

### Every place that writes them

| Write                                        | Place                                               |
| -------------------------------------------- | --------------------------------------------------- |
| `loading = !url.is_empty()` on a new tab     | `browser/commands.rs:62`, `browser/lifecycle.rs:76` |
| Settings tab becomes a web tab               | `browser/commands.rs:311`                           |
| `go`: clears gone, cert, error, sets loading | `browser/commands.rs:335-339`                       |
| `reload`: clears error, sets loading         | `browser/commands.rs:402-404`                       |
| `stop`: clears loading                       | `browser/commands.rs:417`                           |
| Engine `Load` event                          | `engine/events.rs:145-170`                          |
| Engine `ProcessGone`                         | `engine/events.rs:198-211`                          |
| Engine `CertError` request                   | `engine/events.rs:270-275`                          |
| Cert go back and proceed                     | `browser/prompts.rs:94,121-126`                     |
| Engine closed the cert question              | `browser/lifecycle.rs:96-98`                        |
| Engine lost                                  | `engine/lifecycle.rs:26,30`                         |
| Restart                                      | `engine/lifecycle.rs:80-83`                         |

Three places read them and each ranks them separately:

- `Tab::page()` (`tabs/tab.rs:118-135`): settings, crash, cert, failed load, blank, web.
- `Strip::status_of` (`tabs/strip.rs:18-32`): crash, attention, loading, muted, audio.
- `Browser::reload` (`browser/commands.rs:388-392`): `gone.is_some() || cert.is_some()` means recover.

### The real states

| State     | In words                                                                  |
| --------- | ------------------------------------------------------------------------- |
| Settled   | Nothing under way. The page is shown (or the blank page).                 |
| Requested | The browser asked the engine to load. The engine has not said it started. |
| Loading   | The engine says a load is under way, with a progress.                     |
| Failed    | The last load ended with an error. The next load replaces it.             |
| Crashed   | The content process is gone. A new load makes a page again.               |
| Untrusted | The engine waits for Back or Proceed on a certificate error.              |

### Invalid combinations the fields allow

| Combination                                     | Reachable?                                                                                                                                                                                   | Effect                                                                                                          |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `loading` true, but the engine refused the call | Yes. `reload` sets it (`browser/commands.rs:403`) and then calls (`:406`). `go` sets it after the call whatever it returned (`:324-338`). A refused call becomes a toast and the state stays | A load bar stuck at 5% (`ui/page.rs:134`), a Stop button that stops nothing, a spinner on the tab               |
| `gone` and `cert` both set                      | Yes. `ProcessGone` does not clear `cert` (`engine/events.rs:198-211`)                                                                                                                        | `page()` hides `cert` by priority; `cert_proceed` still answers a dead request                                  |
| `gone` and `loading` both set                   | [INFERENCE] A late progress event after `ProcessGone`. `Load` does not look at `gone` (`engine/events.rs:161`)                                                                               | The load bar draws over the crash page (`ui/page.rs:73` ignores the page)                                       |
| `error` set and `cert` cleared by Back          | Yes. `cert_go_back` takes `cert` (`browser/prompts.rs:94`) but leaves `load.error`                                                                                                           | The error page shows until Back completes                                                                       |
| `progress` non-zero and not `loading`           | Yes. A stop sets `progress` 1.0 (`W/engine/events.rs:73-75`)                                                                                                                                 | Harmless only because readers check `loading` first                                                             |
| `started` false and `loading` false             | Yes. It is the Stopped event                                                                                                                                                                 | The test double labels every `started: false` as "stopped" (`W/test_double/mod.rs:1183`), even a progress event |
| `started` true and `loading` false              | Representable. Never sent                                                                                                                                                                    | None                                                                                                            |

### User-visible bugs

- A load bar and a Stop button that never go away after a refused reload or navigation.
- A load bar over a crash page.
- A one-frame error page on certificate Back.
- Three places that must be edited together to add a state (a new error kind needs `page()`, `status_of` and `reload`).

### Proposed explicit form

The event first. The engine sends one of three things, so the type says so.

```rust
// W/events.rs
pub enum LoadEvent {
    /// A new load began. It clears whatever the last load left.
    Started,
    /// How much has arrived, 0 to 1.
    Progress(f32),
    /// The load ended. `None` is success, or a load the person cut short.
    Stopped(Option<LoadError>),
}
// TabEvent::Load(LoadEvent)
```

Then the tab's phase, owned by `navigation/`. It replaces `load`, `gone` and `cert` on `Tab`.

```rust
// B/navigation/phase.rs
pub enum Phase {
    Settled,
    Requested,
    Loading { progress: f32 },
    Failed(LoadError),
    Crashed(GoneReason),
    Untrusted { request: RequestId, error: CertError },
}

impl Phase {
    /// The browser asked the engine to load and it took the call.
    pub fn requested(self) -> Phase;
    pub fn on_load(self, event: LoadEvent) -> Phase;
    pub fn on_process_gone(self, reason: GoneReason) -> Phase;
    pub fn on_cert_error(self, request: RequestId, error: CertError) -> Phase;
    /// The engine closed question `request` itself.
    pub fn on_request_closed(self, request: RequestId) -> Phase;
    /// Back or Proceed. Returns the request to answer.
    pub fn untrusted_answered(self) -> (Phase, Option<RequestId>);
    pub fn stopped(self) -> Phase;
    /// What the reload button does: a new load, or a recovery.
    pub fn needs_recovery(&self) -> bool;
}
```

- `Started` is the only event that leaves `Crashed`. A late `Progress` cannot revive a crashed tab.
- A call the engine refuses is not followed by `requested()`. There is no state to roll back.
- `Tab::page()` and `Strip::status_of` become `match`es on `Phase` and `TabKind`.
- `Page<'_>` stays as the view type.

### What callers change

- `W/engine/events.rs:63-80` `load()` returns `LoadEvent`. Its tests (`W/engine/events/tests.rs:30-110`) and the
  four constructors in `W/test_double/mod.rs` (`:420,:479,:758,:1008`) change shape.
- `engine/events.rs` (browser) `Load`, `ProcessGone`, `CertError` arms call `phase.on_*`.
- `browser/commands.rs` `go`, `reload`, `stop`, `create_web` and `browser/lifecycle.rs` `accept_new_tab` call
  `phase.requested()` only when the call succeeded.
- `browser/prompts.rs` `cert_go_back` and `cert_proceed` call `untrusted_answered()`.
- `ui/page.rs:73` reads `Phase::Loading`. `ui/toolbar.rs:38` and `navigation/mod.rs:57` read `phase.is_busy()`.

### Red tests first

| Test                                                                | Where                      | Red because                                                                                                                  |
| ------------------------------------------------------------------- | -------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `a_reload_the_engine_refuses_does_not_leave_the_tab_loading`        | `browser/tests.rs`         | `fail_engine`, no poll, `reload`: `load.loading` is true today. `with_tab` returns `EngineGone` (`W/test_double/mod.rs:358`) |
| `a_late_progress_event_does_not_revive_a_crashed_tab`               | `error_pages/tests.rs`     | Today `loading` becomes true under the crash page                                                                            |
| `a_crash_ends_a_certificate_question`                               | `error_pages/tests.rs`     | `cert` survives `ProcessGone` today                                                                                          |
| `going_back_from_a_certificate_error_shows_no_error_page_meanwhile` | `error_pages/tests.rs`     | `load.error` is still set today                                                                                              |
| `a_load_reports_its_start_its_progress_and_its_end` (rewritten)     | `W/engine/events/tests.rs` | Does not compile until `LoadEvent` exists                                                                                    |

## F3. Engine loss and the tabs it owes

### Where

| Part                           | Place                                                           |
| ------------------------------ | --------------------------------------------------------------- |
| Where the engine is            | `engine/status.rs:7-20` `Status::{Starting, Ready, Failed{..}}` |
| Tabs the failure took          | `browser/mod.rs:140` `lost: Vec<(u64, TabId)>`                  |
| What the window was to open    | `browser/mod.rs:143` `start: Option<Vec<String>>`               |
| A tab's session                | `tabs/tab.rs:43` `session: Option<TabId>`                       |
| "The session accepts commands" | `tabs/tab.rs:44` `ready: bool`                                  |
| Fill `lost`                    | `engine/lifecycle.rs:20-31`                                     |
| Drain `lost` and `start`       | `engine/lifecycle.rs:56-92`                                     |
| Set `start`                    | `browser/lifecycle.rs:16-19`                                    |
| Set `session` to `None`        | `engine/events.rs:103-107`, `engine/lifecycle.rs:21`            |
| Set `ready`                    | `engine/events.rs:98-101`, `engine/lifecycle.rs:25`             |

`Status` is an enum, which is good. What the window owes while the engine is down lives next to it as two loose
fields that only make sense in one variant.

### The real states

A web tab is live (it has a session), or it is detached (the engine is gone, or has not yet made one).
The engine is starting, ready, or failed. While it is failed, the window owes it the tabs it lost and the
addresses it was launched with. A restart pays that debt tab by tab.

### Invalid combinations the fields allow

- `lost` non-empty while `status` is `Ready`. Reachable after a failed restart. `restart_engine` moves `lost`
  out with `mem::take` (`engine/lifecycle.rs:70`), then `create_tab` can fail for one tab (`:86`). That tab keeps
  `session: None`. It is in neither `lost` nor live. The next `engine_failed` skips it (`:21`, `continue` when
  `session` is `None`). No later restart reopens it.
- `start` set while a tab exists. `open_first` sets `start` only when `status` is `Failed` (`browser/lifecycle.rs:17`).
  It holds only the first launch's addresses.
- `ready` is written twice and read nowhere (`grep '\.ready\b'`). It is dead state.

### User-visible bugs

- **A tab that never comes back.** The restart reopens three tabs. One `create_tab` fails. The window shows a
  tab whose page area says "starting" for ever (`Page::Web` with no session, `ui/page.rs:62,96`). Closing it is
  the only way out.
- **A lost launch.** A second launch hands an address to a window whose engine is down. `ui/app.rs:186-199`
  calls `open_tab`. `create_tab` fails with `EngineGone`. The address becomes a toast and is gone. Only the first
  launch's addresses are kept (`start`).

### Proposed explicit form

Each tab says whether it has a session. The engine's `Failed` and `Starting` variants carry what is owed.

```rust
// tabs/tab.rs
/// How a web tab is joined to the engine.
pub enum Link {
    /// The engine has a session for it.
    Live(TabId),
    /// No session: the engine is gone, or has not made one yet. `was` is the
    /// session it had, so a restart can restore its history.
    Detached { was: Option<TabId> },
}

// engine/status.rs
pub enum Status {
    /// A restart asked for a new engine. `owed` is what to open once it is ready.
    Starting { owed: Owed },
    Ready,
    Failed { failure: Failure, owed: Owed },
}

/// What the window still has to open for the person.
pub struct Owed {
    /// Addresses from launches that arrived while the engine was down.
    pub launches: Vec<String>,
}
```

- `Tab.ready` goes.
- `lost` goes. A restart opens every tab that is `Detached`. A tab whose `create_tab` failed is still `Detached`
  and still owed.
- `start` goes. `open_first` and the second-launch path both call `Status::owe_launch(url)` when the engine is
  not `Ready`.

```rust
impl Status {
    /// The engine failed. Every live tab becomes detached; the caller does that.
    pub fn failed(self, failure: Failure) -> Status;
    /// A restart began. Returns what is owed, or `None` when it is not allowed.
    pub fn restarting(self) -> Option<(Status, Owed)>;
    pub fn ready(self) -> Status;
    /// Remember an address to open after the restart. True when kept.
    pub fn owe_launch(&mut self, url: &str) -> bool;
}
```

### What callers change

- `Tab.session` readers use `Tab::session() -> Option<TabId>`, which is `Some` only for `Live`. There are about
  twenty uses (`grep -n 'session' browser/*.rs engine/*.rs`). The accessor keeps them unchanged.
- `Browser::session_of`, `by_session` and `by_session_mut` (`tabs/list.rs:84-92`) match on `Link::Live`.
- `ui/app.rs:203-224` destructures `Status::Failed` into a tuple. It takes `&Failure` instead.
- `ui/app.rs:186-199` and `open_first` call `owe_launch` when the engine is not `Ready`.

### Red tests first

| Test                                                               | Where             | Red because                                                             |
| ------------------------------------------------------------------ | ----------------- | ----------------------------------------------------------------------- |
| `a_second_launch_while_the_engine_is_down_opens_after_the_restart` | `engine/tests.rs` | `open_tab` fails with `EngineGone` and the address is dropped           |
| `a_tab_whose_reopen_failed_is_reopened_by_the_next_restart`        | `engine/tests.rs` | Needs a double hook `fail_next_creates(n)`. The tab is in no list today |
| `a_restart_with_no_owed_tabs_opens_nothing_by_itself`              | `engine/tests.rs` | Guards the refactor: `owed` empty means no stray tab                    |

The double hook is a small addition to `W/test_double/mod.rs`. It is a test fixture, not product code.

## F4. Fullscreen

### Where

| Part                | Place                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| The window's belief | `browser/mod.rs:137` `fullscreen: bool`                                                                |
| The page's request  | `tabs/tab.rs:61` `fullscreen: bool`                                                                    |
| The window itself   | `ui/ctx.rs:108-112` `UiRequest::Fullscreen(on)`, one way                                               |
| F11                 | `commands/run.rs:48-56`                                                                                |
| The page asks       | `engine/events.rs:189-197`                                                                             |
| Tab switch          | `browser/commands.rs:127-130` (`shown`), `:177-180` (`close_tab`)                                      |
| Escape              | `ui/app.rs:265`, `ui/web.rs:61`; since `988d5229` both call `Browser::escape_leaves_window_fullscreen` |
| Chrome that hides   | `ui/app.rs:276,286,298,302,309`                                                                        |

### The real states

Three different things share the word.

- Off.
- The person pressed F11: the window is fullscreen, the page is not.
- A page asked for fullscreen on one tab, and that tab is on show.

A background tab can also hold a page's request without the window changing.

### Invalid combinations the fields allow

- `Browser.fullscreen` false while the tab on show has `fullscreen` true. Reachable:
    - Page on tab A asks for fullscreen. Both flags are true.
    - Switch to B. `shown` sets `Browser.fullscreen` false and asks the window to leave
      (`browser/commands.rs:127-130`). `A.fullscreen` stays true.
    - Switch back to A. Nothing re-enters. The window is windowed and A still thinks it is fullscreen.
    - At `988d5229` Escape did nothing here, because it tested `Browser.fullscreen` (`ui/app.rs:265`, `ui/web.rs:61`).
      Since `escape_leaves_window_fullscreen` it is forwarded to the page, which leaves and says so.
      The window is still windowed, with its chrome showing, until then.
- F11 is dropped by a tab switch. F11 sets `Browser.fullscreen` but not the tab's flag (`commands/run.rs:51`).
  `shown` then sees "fullscreen but the new tab is not" and leaves fullscreen. The person chose window
  fullscreen. Firefox keeps it across tabs.
- The window leaves fullscreen without the browser knowing (a compositor shortcut). [INFERENCE] `UiRequest::Fullscreen`
  has no way back, so `Browser.fullscreen` stays true.
- `Command::Fullscreen` calls `backend.set_fullscreen`, which is a no-op (`W/backend.rs:327`,
  `W/engine/mod.rs:541`). The page is never told to leave by F11. The page's own fullscreen is now granted
  for a visible tab (`W/engine/events.rs` `on_fullscreen_request`, new since `988d5229`).
- `escape_leaves_window_fullscreen` (`browser/commands.rs`, new) is `self.fullscreen && !active.fullscreen`. It is
  a predicate over the two flags, so the combination logic is already spreading. `Fullscreen::escape_leaves_window`
  would own it.

### User-visible bugs

- F11 fullscreen vanishes when you change tab.
- A page that was fullscreen and came back is a fullscreen page in a windowed window. Escape ends it.
- No test covers any of it. `grep fullscreen` in tests finds only a snapshot scenario (`app/snapshot.rs:7`).

### Proposed explicit form

The window's state is one enum. The page's request stays on the tab as plain engine truth.

```rust
// page/fullscreen.rs (or browser/fullscreen.rs until `page/` exists)
/// Why the window is fullscreen, if it is.
pub enum Fullscreen {
    Off,
    /// The person chose it (F11). It outlives a tab switch.
    Window,
    /// The page on show asked. It ends when that page leaves, or the person
    /// leaves the tab.
    Page { tab: u64 },
}

impl Fullscreen {
    pub fn is_on(&self) -> bool;
    /// F11 or the menu item.
    pub fn toggled(self) -> Fullscreen;
    /// The page of `tab` asked, or left. `active` is the tab on show.
    pub fn page_asked(self, tab: u64, entering: bool, active: u64) -> Fullscreen;
    /// The tab on show changed. Returns whether the page that was fullscreen must be told to leave.
    pub fn tab_changed(self, now: u64) -> (Fullscreen, Option<u64>);
}
```

The window request is sent when `is_on()` changes, by one method, not by four call sites. The tab's own flag is
cleared when a page's fullscreen ends (it is told to leave on a switch).

Decision for Kieran: should a tab switch end page fullscreen (the Firefox way)? I propose yes, and F11 stays.

### What callers change

- `Browser.fullscreen: bool` becomes `Fullscreen`. `commands/run.rs:48-56`, `engine/events.rs:189-197`,
  `browser/commands.rs:127-130,177-180` call the methods.
- `ui/app.rs:210,265,276,286` and `ui/web.rs:61` call `is_on()`.
- `set_fullscreen` stays what it is. The engine now ends a page's fullscreen when Escape reaches it, so F11 and
  page fullscreen need no call from the browser. `tab_changed` returns the tab whose page should be told to
  leave, for when the engine has a call for it.

### Red tests first

| Test                                                                        | Where              | Red because                                                                              |
| --------------------------------------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------------- |
| `f11_survives_a_tab_switch`                                                 | `browser/tests.rs` | `shown` clears it today                                                                  |
| `a_page_that_was_fullscreen_is_left_when_you_leave_its_tab`                 | `browser/tests.rs` | `Tab.fullscreen` stays true today                                                        |
| `escape_leaves_only_window_fullscreen`                                      | `browser/tests.rs` | Pins `escape_leaves_window_fullscreen` as a `Fullscreen` method. Green first, then moved |
| `a_page_in_the_background_asking_for_fullscreen_does_not_change_the_window` | `browser/tests.rs` | Guards the refactor                                                                      |

## F5. Who the window is

### Where

| Part                                    | Place                                                                                    |
| --------------------------------------- | ---------------------------------------------------------------------------------------- |
| A private window                        | `browser/mod.rs:39` `Options.private`                                                    |
| Registered profile id and name          | `browser/mod.rs:74-75` `Profile.id`, `Profile.name`                                      |
| Copied into the browser                 | `browser/mod.rs:132,170` `profile: Option<(String, String)>` via `id.zip(name)`          |
| Whether the profile is kept             | `browser/mod.rs:78,122,127` `settings_keeping`, `history_keeping`                        |
| The writer                              | `browser/mod.rs:81,129` `store: Option<Store>`                                           |
| A copy of `Options.private` on each tab | `tabs/tab.rs:57`, set at `browser/commands.rs:50,310`, `browser/lifecycle.rs:74`         |
| Where the private window is built       | `browser/mod.rs:100-108` `Profile::private`, `app/launch.rs:117-125`                     |
| Who reads it                            | `ui/ctx.rs:113-119` (New private window), `tabs/strip.rs:70` (icon), `new_tab/mod.rs:32` |

### The real states

A window is one of:

- Bare: a test or a snapshot. Nothing is kept.
- A profile that is kept, with a writer.
- A profile that is not kept, with the reason (the store failed to open, or the folder is not writable).
- Private, started from a profile's settings. It keeps nothing.

### Invalid combinations the fields allow

- `id` set and `name` unset. Reachable. `Profile::private` gives `id: Some(..), name: None`
  (`browser/mod.rs:102-104`), so `id.zip(name)` is `None` (`:170`).
- `store` set while `settings_keeping` is `NotSaved::Private`. Not reached, but nothing prevents it.
- `options.private` false with `NotSaved::Private`, or the reverse. Not reached.
- `Tab.private` differs from `Options.private`. Not reached. It is a copy.

### User-visible bug

**A new private window from a private window opens the default profile.** `UiRequest::NewPrivateWindow` passes
`browser.profile.map(|(id, _)| id)` (`ui/ctx.rs:113-119`). In a private window `profile` is `None`. The new
window starts from the default profile's settings, not the profile the first one came from. It shows only when
the profile differs from the default, for example in the search engine.

### Proposed explicit form

```rust
// browser/profile.rs
pub enum Profile {
    /// A test or a snapshot: nothing is kept.
    Bare,
    /// A private window. `from` is the profile whose settings it started with.
    Private { from: Option<Id> },
    /// A window of a profile. `who` is `None` for a folder named by the environment.
    Kept { who: Option<Who>, store: Store, settings: Keeping, history: Keeping },
    /// A window of a profile that is not being kept, and why.
    Unkept { who: Option<Who>, why: NotSaved },
}

pub struct Who { pub id: Id, pub name: String }

impl Profile {
    pub fn is_private(&self) -> bool;
    pub fn store(&self) -> Option<&Store>;
    /// The profile a new private window starts from.
    pub fn private_child(&self) -> Option<&Id>;
}
```

- `Options` goes away. `Options.private` is `profile.is_private()`.
- `Tab.private` goes. The strip asks the browser.
- `settings_keeping` and `history_keeping` stay as the live views of `Kept`.
- The keeping state of a part is `Keeping`, which is already explicit (see "Already explicit and fine").

### What callers change

- `app/launch.rs:82-125` builds the right variant. The error branch of `Store::open` builds `Unkept`.
- `ui/ctx.rs:113-119` calls `private_child()`.
- `browser/commands.rs:50,310`, `browser/lifecycle.rs:74` stop setting `tab.private`. `tabs/strip.rs:70` asks the
  browser (the strip already holds `ctx.browser`).
- `Browser::new`, `with_profile` and the snapshot rig (`snapshot/mod.rs:189`) take `Profile`.

### Red test first

| Test                                                                   | Where              | Red because                                                                                        |
| ---------------------------------------------------------------------- | ------------------ | -------------------------------------------------------------------------------------------------- |
| `a_private_window_opens_the_next_private_window_from_the_same_profile` | `private/tests.rs` | `Browser::with_profile(.., Profile::private(Some("work"), ..))`; `browser.profile` is `None` today |

## F6. Find in page

### Where

`find/mod.rs:17-25`: `open: bool`, `text: String`, three option bools, `result: Option<(u32, u32)>`.

### The real states

| State       | In words                                                             |
| ----------- | -------------------------------------------------------------------- |
| Closed      | No bar. The query is kept for F3.                                    |
| Open, empty | The bar is open. Nothing to search for.                              |
| Searching   | A query is out. The engine has not counted yet.                      |
| Counted     | The engine said `current` of `total`. A `total` of 0 is "not found". |

### Invalid combinations the fields allow

| Combination                               | Reachable?                                                                    | Effect                                                                                                                       |
| ----------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| New `text`, old `result`                  | Yes. `search` sets `text` (`find/mod.rs:99`) and leaves `result` (`:100-113`) | Type "zz" (0 hits), then edit to "z": `not_found()` (`:49-51`) is still true until the engine answers. The field flashes red |
| `open` true, tab has no session           | Yes. `close` returns early when `session` is `None` (`:78-80`)                | The bar cannot be closed while the engine is down                                                                            |
| `open` false, `result` set                | Yes. `on_result` always stores (`:59-61`), even after `close`                 | A count from a closed bar shows on reopen                                                                                    |
| `open` false, `text` non-empty, searching | Yes. `step` searches without opening the bar when text exists (`:137-143`)    | F3 highlights with no bar. May be intended; it is undocumented                                                               |

### Proposed explicit form

```rust
/// What the person asked for. It survives closing the bar.
pub struct Query { text: String, match_case: bool, whole_words: bool, highlight_all: bool }

pub enum Find {
    Closed { query: Query },
    Open { query: Query, progress: Progress },
}

pub enum Progress {
    /// No text, or the text just changed.
    Idle,
    Searching,
    Counted { current: u32, total: u32 },
}
```

Transitions: `open(self)`, `typed(self, text)` (goes to `Searching` or `Idle`), `on_result(self, current,
total)` (ignored unless `Searching` or `Counted`), `close(self)`, `options_changed(self, ..)`. `close` always
closes, then asks the engine if there is a session. `Find::open` loses its capability check (see "Not counted").

### What callers change

`browser/commands.rs:469-512` and `:566-571`, `engine/events.rs` `FindResult` arm, `find/bar.rs:20-31`,
`ui/app.rs:221` (`is_open`). `FindBar` reads `progress` for the result and for not-found.

### Red tests first

| Test                                                   | Where           | Red because                           |
| ------------------------------------------------------ | --------------- | ------------------------------------- |
| `an_edited_query_does_not_show_the_old_count`          | `find/tests.rs` | `not_found()` stays true after typing |
| `a_find_bar_closes_when_its_tab_has_no_session`        | `find/tests.rs` | `close` returns early                 |
| `a_count_that_arrives_after_the_bar_closed_is_ignored` | `find/tests.rs` | `on_result` stores it                 |

## F7. Download news

### Where

| Part                                 | Place                                                     |
| ------------------------------------ | --------------------------------------------------------- |
| The list and a counter               | `downloads/mod.rs:84-88` `list`, `unseen: u32`            |
| "Is this new?" by searching the list | `engine/events.rs:49-52` `started = !list.any(id)`        |
| "Did it finish?" by comparing state  | `engine/events.rs:54` `finished = info.state == Finished` |
| A second diff, inside `update`       | `downloads/mod.rs:99-100` `existing.state != info.state`  |
| Reset by the window                  | `ui/ctx.rs:105` `mark_seen`                               |

### The real states

A download is running, paused, finished, failed or cancelled. `DownloadState` is explicit. What is implicit is
the _transition_ the toast reports. Two places work it out by diffing, in two different ways.

### Invalid combinations the fields allow

- `events.rs` toasts "finished" for every `Finished` info that is not new (`:66`). `update` counts a transition
  only when the state changed (`downloads/mod.rs:99`). The two disagree. [INFERENCE] The engine's
  `DownloadEvent` contract does not promise one terminal event (`W/engine/events.rs:451-469` forwards each one),
  so a repeated `Finished` gives a second toast and no `unseen`.
- `unseen` counts `Finished` and `Failed`, not `Cancelled` (`downloads/mod.rs:100`). The toast says "finished"
  only for `Finished`.
- `unseen` is written and reset but never read by any UI (`grep unseen`: tests only). The downloads button has no
  badge (`downloads/button.rs:15-38`). It is dead state.

### Proposed explicit form

```rust
pub enum Change { Started, Progress, Paused, Resumed, Finished, Failed, Cancelled }

impl Downloads {
    /// Take in `info`. Says what happened, once, however many times the engine says it.
    pub fn update(&mut self, info: DownloadInfo) -> Change;
}
```

The toast follows the returned `Change`, not a second search of the list. `unseen` and `mark_seen` go (or a
badge is drawn, which is a product decision, not a refactor).

### What callers change

`engine/events.rs:42-72` matches on `Change`. `ui/ctx.rs:103-106` drops `mark_seen`. `downloads/tests.rs:100-119`
loses the `unseen` assertions.

### Red test first

| Test                                                 | Where                | Red because                             |
| ---------------------------------------------------- | -------------------- | --------------------------------------- |
| `a_finished_download_the_engine_repeats_toasts_once` | `downloads/tests.rs` | Emit `Finished` twice: two toasts today |

## F8. Finger to mouse translation

### Where

`W/touch.rs:24-39`: `finger: Option<Finger>`, `Finger.scrolling: bool`, `spoiled: bool`. The module's own
comment says "as a state machine of its own". It is one, and implicit.

### The real states

| State     | In words                                                                        |
| --------- | ------------------------------------------------------------------------------- |
| Idle      | No finger.                                                                      |
| Pressing  | A finger is down and still. A lift is a click.                                  |
| Scrolling | The finger moved past the slop. Moves scroll. A lift does nothing.              |
| Held      | The finger rested for `LONG_PRESS` and the menu was asked. A lift does nothing. |
| Spoiled   | A second finger came down. Nothing the first does counts, until it lifts.       |

### Invalid combinations the fields allow

- `scrolling` has two meanings. `held` sets it to mean "the press is spent" (`:67`). `touch` sets it to mean
  "this is a scroll" (`:104`). After a long press, the next move of one pixel scrolls the page, because
  `!finger.scrolling && travelled < TAP_SLOP` is false (`:101`). Reachable.
- `spoiled` true with `finger` `None`. Not reached: every path that clears one clears the other, except an `Up`
  for the second finger, which leaves both set until the first lifts (`:124`). Safe, but only by reading three
  arms.

### User-visible bug

A held finger opens the page menu. A tremor of the finger then scrolls the page under the open menu.

### Proposed explicit form

```rust
enum Touch {
    Idle,
    Pressing { id: u64, start: (f64, f64), since: Instant },
    Scrolling { id: u64, last: (f64, f64) },
    Held { id: u64 },
    /// A second finger came down. `first` is the one that decides when it ends.
    Spoiled { first: u64 },
}
```

`TouchScroller::touch` and `held` become `match (self.state, event.kind)`. `Held` ignores moves under the slop.

### Dependency

`TouchScroller` runs only when `Capabilities::touch` is false (`W/view.rs:201-234`), and the real engine backend says
`touch: false` (`W/engine/caps.rs:97`) with the comment "the engine announces no touch input". That comment is out of
date. Vivarium has touch points, pan and pinch (`vivarium_input.h:137`, `vv_session_send_touch`, engine test
`t_input_touch.c`), and its `docs/rust.md` says touch, gestures and IME are no longer engine gaps. DOM touch events
are not built yet (`vivarium_input.h:137`).

So F8 has two parts. The enum fixes the tremor bug in the scroller as it is today. Proposal, not yet verified: send
fingers to the engine (`engine/convert.rs` already converts them) and let its scroll code pan, as Firefox does. That
needs proof on the real engine that a tap clicks, a drag pans and a long press opens the context menu. Until then
`Capabilities::touch` stays as the switch between the two paths, and the tests cover both. Do not delete it on the
claim that the engine has no touch.

### Red test first

| Test                                          | Where              | Red because                                     |
| --------------------------------------------- | ------------------ | ----------------------------------------------- |
| `a_held_finger_that_trembles_does_not_scroll` | `W/touch.rs` tests | `held()` then a 1 px move returns a wheel event |

## F9. Address field, and the toolbar's cards

### Where

| Part                 | Place                                                                             |
| -------------------- | --------------------------------------------------------------------------------- |
| Typing, highlight    | `address/field.rs:30,35` `typing: State<bool>`, `highlight: State<Option<usize>>` |
| The pointer on a row | `address/field.rs:95` `hovered: State<Option<usize>>`                             |
| Alt on Enter         | `address/field.rs:96` `Rc<Cell<bool>>`, set `:210`, taken `:164`                  |
| Focus request        | `address/field.rs:33`, `find/state.rs:8`: counters                                |
| Cards                | `site_info/popover.rs:15` `open`, `downloads/state.rs:8` `open`                   |

### The real states

- Address field: Following (shows the page's address) or Editing with an optional highlighted row.
- Cards: none open, page info open, downloads open.

### Invalid combinations the fields allow

- `highlight` set while `typing` is false. `on_dismiss` clears `typing` only (`address/field.rs:250`). The stale
  highlight is harmless because rows are built only while editing (`:123`) and `on_change` clears it.
- Both cards open. Reachable by keyboard: `Command::Downloads` and `Command::PageInfo` emit independent
  `UiRequest`s (`commands/run.rs:57,61`; `ui/ctx.rs:103-107`) and `Popovers` renders both (`ui/popovers.rs:23-24`).
  [INFERENCE] A pointer press would dismiss the first, a key would not.
- `enter_with_alt` set by the Enter-modifiers callback and cleared by submit. [INFERENCE] If Alt+Enter does not
  reach `submit`, the flag stays and the next plain Enter opens a new tab.

### Proposed explicit form

```rust
enum Entry { Following, Editing { highlight: Option<usize> } }   // address/field.rs

pub enum Card { None, PageInfo, Downloads }                      // ui, one State<Card>
```

`Card` replaces the two `open` states. The anchors stay with their buttons. `enter_with_alt` becomes the
`Alt` flag on the submit event if the SDK gives it, else stays.

Both focus counters stay. `find_card(..).focus_requests(u32)` is the SDK's API, not this app's state.

### Overlap and order

`MainMenuButton` and the main menu belong to BrowserMainMenu. At `988d5229` the button was a stub in
`ui/toolbar.rs:46-54`. It is now in `menus/`, which I have not audited.

A `Card` that also covers the main menu is better. Do not start F9 until that agent's work has landed and been
read.

### Red tests first

| Test                                            | Where                               | Red because                   |
| ----------------------------------------------- | ----------------------------------- | ----------------------------- |
| `opening_the_downloads_closes_the_page_info`    | `downloads/tests.rs` (Snapshot rig) | Both stay open today          |
| `escape_ends_editing_and_forgets_the_highlight` | `address/tests.rs`                  | Only if `Entry` is introduced |

## F10. The history file in the store thread

### Where

`store/mod.rs:84-100`: `history: Keeping`, `database: Option<Database>`, `pending: Vec<Change>`,
`due: Option<Instant>`, `stale_index: bool`.

### The real states

Loading, then either Ready (a connection, a batch with a due time), Retrying (the last write failed, the batch
stays queued), or Unwritable (damaged, or from a newer browser: never written over).

### Invalid combinations the fields allow

- `due` is `Some` only when `pending` is non-empty. Held by `queue` (`:226-232`), `commit` (`:247-267`) and
  `clear_history` (`:269-284`) together. Nothing says so.
- `database` is `None` with `history` `Saved`. Not reached today. `can_write` (`:174-191`) reopens when it is
  `None`, so it is tolerated.
- `stale_index` is a flag set in one method (`:182`) and consumed in another (`:235-244`).
- After a failed `commit`, `due` is `None` and `pending` is not (`:248`). The retry waits for the next change,
  a flush or a drop. This is documented ("the next change tries again") and may be wanted.

### Why it ranks low

It runs on one thread, in one file, with tests (`store/tests.rs`). I found no path to a wrong result. The
explicit form would be `enum HistoryFile { Opening, Ready { database, batch: Batch }, Retrying { .. },
Unwritable(NotSaved) }`. Do it after F1 to F8, and only if the store changes again.

## F11. The engine backend's lifecycle

### Where

`W/engine/state.rs:41-57` `runtime: Option<Arc<Runtime>>`, `W/engine/runtime.rs:50` `failed: AtomicBool`,
`W/engine/events.rs:54` `Host.failed`.

### The real states

Not started (`runtime` is `None` after a failed start), running, failed (the runtime is kept so that `info()`
can still describe it, `state.rs:43-45`).

### Invalid combinations the fields allow

`runtime` is `Some` and `failed` is true. Every call then fails inside the engine library. This is intended.

- `runtime()` (`W/engine/mod.rs:154-159`) returns `Ok` for it. The failure is found one call later.
- `restart` takes the runtime and starts again (`:337-343`).
- If `run()` fails, `runtime` is `None`.

### Why it ranks low

Three representations, all consistent. The one `OnceLock<bool>` (`find`) is a capability, listed below. If the
backend is touched for another reason, make it `enum Lifecycle { StartFailed, Running(Arc<Runtime>),
Failed(Arc<Runtime>) }`.

## F12. Dead or copied state

Go with the finding whose commit touches them.

| Field                                     | Evidence                                                                                                                                                                 | With             |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------- |
| `Tab.ready`                               | Written `engine/events.rs:100`, `engine/lifecycle.rs:25`. Never read                                                                                                     | F3               |
| `Tab.private`                             | Copy of `Options.private`. Readers: `tabs/strip.rs:70` and, since `988d5229`, `Browser::open_devtools` (`browser/lifecycle.rs`)                                          | F5               |
| `Downloads.unseen`, `mark_seen`           | No UI reads it (F7)                                                                                                                                                      | F7               |
| `Chrome` with one field                   | `ui/ctx.rs:25-30`. Its doc comment is two fragments joined                                                                                                               | any `ui/` change |
| `Tab.history`, `history_index`, `favicon` | Written from events. Read only by tests (`browser/tests.rs:72`, `engine/tests.rs:135`). These are engine mirrors for planned features (`parity.md`), not machines. Leave | none             |

## Already explicit and fine

The refactor must not churn these.

| Type                                                                                                                                  | Place                                                               | Why it is fine                                  |
| ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------------------------- |
| `TabKind`                                                                                                                             | `tabs/tab.rs:17`                                                    | Settings or web, set once, changed only by `go` |
| `Page<'_>`                                                                                                                            | `tabs/tab.rs:26`                                                    | A derived view. Stays, now derived from `Phase` |
| `Place`                                                                                                                               | `tabs/list.rs:15`                                                   | An argument, not a state                        |
| `Prompt`                                                                                                                              | `prompts/mod.rs:27`                                                 | One variant per kind, with its id               |
| `Status`                                                                                                                              | `engine/status.rs:7`                                                | Explicit. F3 only adds what it owes             |
| `Keeping`, `NotSaved`, `Part`                                                                                                         | `store/keeping.rs:15,26,47`                                         | The model the rest should copy                  |
| `store::Event`, `store::Command`                                                                                                      | `store/mod.rs:43,48`                                                | Messages                                        |
| `ProfileState`, `Opened`, `Gate`, `Startup`                                                                                           | `profiles/open.rs:22,116`, `gate.rs:17`, `registry.rs:240`          | One variant per outcome, data in the variant    |
| `LockError`, `RegistryProblem`, `NameProblem`, `ChangeError`, `MigrationError`, `Migrated`, `SettingsProblem`, `Problem`              | various                                                             | Error and outcome enums                         |
| `Typed`, `Internal`, `Source`                                                                                                         | `address/typed.rs:6,125`, `suggest.rs:9`                            | A parse result, no lifecycle                    |
| `Command`, `UiRequest`, `Platform`                                                                                                    | `commands/mod.rs:21`, `browser/outbox.rs:19`, `commands/keys.rs:10` | Vocabulary                                      |
| `Tone`, `Summary`                                                                                                                     | `site_info/mod.rs:18,28`                                            | Derived each render                             |
| `Shown`                                                                                                                               | `ui/page.rs:24`                                                     | A copy of the page for one render               |
| `Zoom`                                                                                                                                | `zoom/mod.rs:15`                                                    | A value, steps are pure                         |
| `BrowserEvent`, `TabEvent` (but `Load`), `EngineEvent`, `FailureCause`, `GoneReason`, `SecurityState`, `DownloadState`, `Disposition` | `W/events.rs`, `W/page.rs`, `W/requests.rs`                         | Engine vocabulary                               |
| `RequestKind`, `Response`, `Open`                                                                                                     | `W/requests.rs`, `W/engine/requests.rs:309`                         | One variant per question                        |
| `PageInput`, `TouchKind`, `PointerKind`                                                                                               | `W/input.rs`                                                        | Input vocabulary                                |
| `BackendError`, `EngineState`, `CheckStatus`                                                                                          | `W/backend.rs`, `W/report.rs`                                       | Outcomes                                        |

Plain data with flags that are not a lifecycle, so they stay:

| Type                                               | Why                                                                                  |
| -------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `Audio { playing, muted }`                         | Two independent facts                                                                |
| `Security { state, mixed_*, extended_validation }` | The flags qualify `state`. `Summary::of` reads them in the right arms                |
| `ClearScope { history, link_colours }`             | Two independent facts. `link_colours` is a capability (see below)                    |
| `Entry.deleting` in `profiles.json`                | A persisted marker whose format is a promise. Changing it needs a migration          |
| `PageSurface` mailbox                              | `waiting`, `folded`, counters are kept in step under one lock (`W/surface.rs:21-33`) |
| `Signal.pending`, `EventQueue`                     | A wake latch. Standard                                                               |
| `Waiting` in `profiles/open.rs`                    | A deadline loop with an explicit result (`Opened`)                                   |
| `ProfileLock`, `Held`                              | RAII. The lock is the state                                                          |
| `taken: HashSet<String>` in `W/view.rs:110`        | The keys the host took. One entry per key, explicit                                  |
| `PermissionBanner`'s `shown` text                  | A memo so the banner does not empty while it closes                                  |
| `Handoff`                                          | Take-once                                                                            |

## Not counted: engine-capability branches

Kieran ruled these are being deleted. Do not turn them into states.

| Branch                                                                                | Place                                                                                        |
| ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `Capabilities` and its 13 flags                                                       | `W/backend.rs:59-85`, `W/engine/caps.rs`                                                     |
| `Engine::refresh`, `capabilities()`                                                   | `engine/mod.rs:71-79`                                                                        |
| Find capability, learned from the first session                                       | `find/mod.rs:64-72`, `W/engine/mod.rs:234-247`, `W/engine/runtime.rs:47`                     |
| Local context menu: `open_local_menu`, `PendingMenu.request: Option`, `on_local_menu` | `browser/prompts.rs:153-166`, `browser/mod.rs:55-57`, `ui/web.rs:75`, `W/view.rs:92,146-156` |
| Download controls and the button's `can`                                              | `downloads/button.rs:23`, `downloads/popover.rs:31`, `browser/commands.rs:531`               |
| `ClearScope.link_colours` (now `capabilities().clear_history`; was always false)      | `browser/kept.rs:67-72`, `W/engine/runtime.rs` `clear_history: OnceLock` (new)               |
| Print and devtools availability checks                                                | `commands/run.rs` `print`, `browser/lifecycle.rs` `open_devtools` (new since `988d5229`)     |
| Touch: the scroller versus the engine's touch                                         | `W/view.rs:201-234`                                                                          |
| Composition                                                                           | `W/view.rs:189`                                                                              |

When the context menu branch goes, `PendingMenu.request` becomes a plain `RequestId`. That is a one-line change
in `browser/mod.rs:57` and `menus/host.rs:20,27`, which BrowserMainMenu is editing. Leave it to them.

## Needs verifying before it is called a bug

| Claim                                                                                                       | How to check                                                    |
| ----------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| A second launch with the same address as the last is ignored (`ui/app.rs:190` compares `Location` by value) | Read `nav/router.rs`; test with the same `open` pair twice      |
| A load bar over a crash page                                                                                | Send `Load { loading: true }` after `ProcessGone` in the double |
| A repeated `Finished` event                                                                                 | Ask whether the engine can send two                             |
| The window leaves fullscreen on its own                                                                     | Compositor shortcut on a real session                           |

## Plan

### Ranking

| Rank | Commit                    | Value  | Risk   | Size (files) | Needs from others                                                  |
| ---- | ------------------------- | ------ | ------ | ------------ | ------------------------------------------------------------------ |
| 1    | `prompts`                 | High   | Low    | 8            | Leave `dismiss_menu` and `open_local_menu` alone                   |
| 2a   | `web_view`: `LoadEvent`   | Medium | Low    | 5            | None                                                               |
| 2b   | `navigation`: `Phase`     | High   | Medium | 12           | After 2a. After GeckoNaming's renames in `engine/`                 |
| 3    | `engine`: `Link`, `Owed`  | High   | Medium | 14           | After 2b. After GeckoNaming. Needs double hook `fail_next_creates` |
| 4    | `fullscreen`              | Medium | Low    | 8            | After BrowserSlice3 touches `commands/run.rs`                      |
| 5    | `profile`                 | Medium | Medium | 9            | None                                                               |
| 6    | `find`                    | Medium | Low    | 7            | After BrowserSlice3 (capability check at `find/mod.rs:64`)         |
| 7    | `downloads`               | Low    | Low    | 6            | After BrowserSlice3 (`clear_finished_downloads`)                   |
| 8    | `touch`                   | Low    | Low    | 1            | Keep the touch switch; prove engine tap, pan, long press first     |
| 9    | `chrome`: `Card`, `Entry` | Low    | Medium | 6            | After BrowserMainMenu                                              |
| 10   | `store`                   | Low    | Medium | 1            | Only if the store is edited again                                  |
| 11   | `backend`                 | Low    | Low    | 3            | Only if the backend is edited again                                |

### One commit per feature

Each commit changes one feature and its callers. Each has its red test first, then the change, then green.
Subjects follow the log (`refactor(browser): ...`). Each is pushed alone so a regression bisects to one.

1. `refactor(browser): a tab's questions are a Prompts queue answered by request id`
2. `refactor(web_view): a load event is Started, Progress or Stopped`
3. `refactor(browser): a tab's page is one Phase, not load, gone and cert`
4. `refactor(browser): a tab is Live or Detached, and the engine's debt is in Status`
5. `refactor(browser): fullscreen is Off, Window or Page`
6. `refactor(browser): the window's profile is one enum`
7. `refactor(browser): find in page is Closed or Open, with a progress`
8. `refactor(browser): a download update says what changed`
9. `refactor(web_view): the touch scroller is an enum`
10. `refactor(browser): one card open at a time; the address field is Following or Editing`

Commits 1, 2, 5, 6 can go in parallel. Commits 3 and 4 are strictly ordered (3 changes the fields 4 moves).
No commit changes a visible string or a layout, so the snapshots should not move. Where a fix changes behaviour
(F1, F2, F3, F4, F5, F6, F7) the commit body says so.

### Overlap with the agents at work

Their files are inferred from their task names. Ask each before editing a shared file.

| Agent           | Likely files                                                                                                                               | Commits it touches                                                      |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| BrowserSlice3   | Landed since the audit: print, devtools and clear history in `commands/run.rs`, `browser/lifecycle.rs`, `browser/kept.rs`. More may follow | fullscreen (other arm of `run.rs`), find, downloads. Rebase before each |
| BrowserMainMenu | `menus/` (`main.rs`, `phone.rs`, `host.rs`), `PendingMenu`, `browser/prompts.rs` `dismiss_menu`                                            | prompts (I touch only the answers), chrome (wait)                       |
| GeckoNaming     | A naming doc, and renames. [INFERENCE] `engine/` and `Engine` are likely targets                                                           | `Phase` and `Link`. Ask them to land first                              |
| UiSuiteGreen2   | [INFERENCE] `crates/ui` tests, from its task name                                                                                          | `LoadEvent` and touch (`W/` files)                                      |

I will not edit any of these until Main says go, and not a file another agent has uncommitted.

### Rules for phase 2

- **Comments.** Main relayed Kieran: "more comments please". Every file a commit touches ends with:
    - A module comment that says what the feature does and how it fits.
    - A doc comment on every type, every enum variant and every non-trivial method. It says what the thing is for,
      its invariants and why it has this shape. It does not narrate the next line.
    - A comment at each state transition, and at any non-obvious branch, wait or ordering.
    - Plain ASCII, British English, short sentences.
- **Verification.** No cargo here. Ask Main for a ga-box slug per commit. Run `cargo test -p bingux-browser`,
  `cargo test -p bingux-ui --features web-view,testing`, and `make check` (which runs `taxonomy-check.py` and the
  rules checks). Run the browser's snapshot scenarios for F2, F4 and F6.
- **Docs.** Each commit updates `docs/browser/structure.md` where it names a field this audit moves
  (`structure.md` "Per-tab features" lists `gone`, `cert`, `fullscreen`, `prompts`). Add a short "State" line to
  each feature's section, naming its enum.
- **bingux-ui first.** `Phase`, `Link`, `Find` and `Fullscreen` are the browser's. `LoadEvent` and `Touch` are in
  `crates/ui`, so each needs its taxonomy entry only if it is exported past the `web_view` module. They are
  already exported with `TabEvent`, so check `scripts/taxonomy-check.py` before pushing commit 2.

### Questions for Main

1. Should a tab switch end a page's fullscreen (my proposal), or keep it until the person leaves (F4)?
2. Partly answered: the engine has touch (`t_input_touch.c`). Whether fingers go to it depends on a real-engine check.
3. May I add `fail_next_creates` to the test double (F3)? It is a test fixture in `crates/ui`.
4. Order with GeckoNaming: do they land `engine/` renames before commits 3 and 4?
