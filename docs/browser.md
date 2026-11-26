# Browser

`apps/browser` (package `bingux-browser`) is the browser: a thin frame (tabs, toolbar, address field, menus, settings) around
bingux-ui's web view control (`bingux_ui::web_view`, cargo feature `web-view`), the way iexplore.exe was a frame
around IE's WebBrowser control. Any Bingux app can embed the same control (Mail's account sign-in is the second use;
`docs/browser/plan.md`). The window is built from bingux-ui and nothing else. Pages are drawn by the engine (an
embeddable Gecko, see the Vivarium repository) into the control's frame view.

```
bingux-browser          one folder per feature (tabs, address, downloads, find, ...: state, behaviour, components,
                        tests, words) over app/ (launch), ui/ (the window's layout) and engine/ (the only code that
                        speaks to the backend); `docs/browser/structure.md`
      |
bingux_ui::web_view     WebView control + BrowserBackend (one trait)
                          ->  engine (feature web-view-vivarium or web-view-dynamic-load, over `vivarium` 0.1)
                          ->  test_double (features web-view + testing: tests and snapshots only)
```

## Run it

```sh
VIVARIUM_SDK_DIR=/path/to/sdk apps/browser/run.sh [--private] [--profile NAME] [ADDRESS | WORDS | FILE]...
```

`run.sh` checks the SDK folder (`vivarium-engine`, `libvivarium.so`, `libvivarium.so.0`), puts it on
`LD_LIBRARY_PATH`, builds with `--features vivarium` (`BINGUX_BROWSER_BUILD=release` for an optimised
build; `--no-build` as the first argument starts what is built) and starts the window. `BINGUX_BROWSER_PROFILE`
names a profile folder outside the browser's own, for tests: the engine's profile is its `engine/` folder and its
cache, dumps and sockets stay below it too.
The engine only starts from a folder nobody but you or root can write to (and no ancestor
either, sticky ones like `/tmp` excepted): `run.sh` warns when the SDK folder is group or world writable
(`chmod -R go-w`; a development tree may set `VV_TRUST_ENGINE_PATH=1`). Cargo output goes to
`build/cargo` unless `CARGO_TARGET_DIR` is set. By hand it is
`VIVARIUM_SDK_DIR=... cargo build --features vivarium` and `LD_LIBRARY_PATH=$VIVARIUM_SDK_DIR bingux-browser`.

A second launch hands its address to the running window, which opens a tab. `--private` starts a
process of its own (application id `bingux-browser-private`) whose tabs are private sessions and whose
profile is deleted when it closes. The engine's configuration is the host's: `BINGUX_BROWSER_PROFILE`,
`BINGUX_BROWSER_REMOTE_SETTINGS` (`off`, `security`, `mozilla`), `BINGUX_BROWSER_REMOTE_SETTINGS_SERVER`
(the mirror `security` reads from) and `VIVARIUM_SDK_DIR`. With no choice the security data (revocation)
comes from Mozilla. The settings page shows what was chosen and does not change it.

Where things live (`docs/browser/profiles-and-store.md`, "Where things live"). Each profile is a folder with its
own engine profile, store and engine process; `--profile NAME` picks one, and one process at a time holds a profile
(`flock` on its `lock`). A named profile (`BINGUX_BROWSER_PROFILE`) keeps every folder below it, and a private
window keeps its engine profile in the runtime folder and removes it when it closes:

| What                               | Where                                                                                      |
| ---------------------------------- | ------------------------------------------------------------------------------------------ |
| the list of profiles and its lock  | `$XDG_DATA_HOME/bingux-browser/profiles.json`, `profiles.lock`                             |
| a profile: lock, settings, history | `$XDG_DATA_HOME/bingux-browser/profiles/<id>/` (`lock`, `settings.json`, `browser.sqlite`) |
| its engine profile                 | `$XDG_DATA_HOME/bingux-browser/profiles/<id>/engine`                                       |
| its engine cache                   | `$XDG_CACHE_HOME/bingux-browser/profiles/<id>`                                             |
| its crash dumps, never uploaded    | `$XDG_STATE_HOME/bingux-browser/profiles/<id>/crashes`                                     |
| its runtime sockets                | `$XDG_RUNTIME_DIR/bingux-browser/<id>`                                                     |
| downloads                          | `~/Downloads`                                                                              |

The first start moves an older browser's `$XDG_DATA_HOME/bingux-browser/profile` into a profile called Personal.
Settings (the search engine) are written when they change; history is written at most a second after a visit, on
the profile's store thread. The settings page says when either is not being kept and why. A private window reads
its profile's settings and writes nothing.

`file://` pages may read `Documents`, `Desktop`, `Pictures`, `Videos` and `Music` of the home folder (those that
exist) and the folders `BINGUX_BROWSER_FILE_DIRS` names (a path list). A folder inside one of the engine's own
folders above, or holding one, is left out, the download folder included: the engine refuses to start when a
folder pages may read overlaps one of its own.

`BINGUX_BROWSER_WEBRTC_IP_POLICY` (`default`, `default-public`, `public-only`, `relay-only`) asks for a WebRTC
address policy. ABI 2.0 has no call that sets one, so the engine keeps its own fixed policy (no local or TCP
addresses) and the settings page says the choice was not applied.

Checked on the real engine on 2026-11-25 (a37 box, headless sway with the pixman renderer, a development SDK
built that day: Gecko 160, ABI 2.0, content sandbox level 6), built with
`VIVARIUM_SDK_DIR=$SDK cargo build --features vivarium` and started as a normal user with
`VIVARIUM_SDK_DIR=$SDK LD_LIBRARY_PATH=$SDK BINGUX_BROWSER_PROFILE=$PROFILE BINGUX_BROWSER_FILE_DIRS=$PAGES
bingux-browser ADDRESS`: a local `file://` page, a link clicked, an address typed into the bar
(`https://example.com/`), a second tab, the window at 1100x760 and at 390x800, the engine's context menu on a
link and a `confirm()` answered through the browser's dialog. The engine accepted every request kind the
browser answers.

`bingux-browser --snapshot DIR [SCENARIO,...]` (a `--features snapshot` build) renders 36 states at the
desktop size and at 390x800 over the test double. `--snapshot --list` names them.

## The model

| Part                  | What it holds                                                                                                                                                                                                                                                             |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Tab`                 | url, title, load progress and error, security, history, zoom, audio, pinned, private, crash, the questions waiting (`Prompt`), find state, certificate error, opener                                                                                                      |
| `TabList`             | order (pinned first, as a block), the one on show, close rule (opener, else the neighbour that takes its place), reorder, pin                                                                                                                                             |
| `Browser`             | tabs, closed tabs, downloads, permissions, engine state, the settings and whether they and the history are kept (`Keeping`), what only the window can do (`UiRequest`), toasts (`Notice`). It calls the backend and the profile's store itself; the UI calls its commands |
| `address`             | `classify` (address, words, nothing; `javascript:` is words), `display`, `origin`, search engines                                                                                                                                                                         |
| `suggest`             | the rows under the address bar: typed text, open tabs, then the hits of the `history` searchable                                                                                                                                                                          |
| `profile`             | the registry, the profile lock, migration, and the store: `settings.json` and `browser.sqlite` on a thread of their own, and the bingux-ui searchable `history` (`profile/history.rs`)                                                                                    |
| `shortcuts`           | key and modifiers to `Command`, per platform (Ctrl on Linux and Windows, Command on macOS; Alt+1-9 on Linux)                                                                                                                                                              |
| `failure`, `security` | the words of error pages, certificate problems and the page info                                                                                                                                                                                                          |

Every rule has a test (`cargo test`), the commands over `TestDoubleBackend`.

## Event flows

```
engine wake hook ──► UI task ──► poll_events: Dispatcher ──► Host (delegates) ──► EventQueue ──► Browser::apply ──► State<Browser> ──► render
UI handler ──► Ctx::with ──► Browser::<command> ──► BrowserBackend call (queued, never waits)
frame view ──► BrowserBackend::send (not through the state: a pointer move is not news)
engine frame ──► Host::on_frame_available ──► PageSurface (newest wins, damage folded) ──► FrameSource ──► frame_view
```

The engine's library calls the wake hook on its own thread when its event queue goes from empty to not empty;
the hook only wakes the UI's task. That task calls `poll_events`, which runs the `vivarium` crate's `Dispatcher`
on the UI thread: at most 256 events a call, never blocking, one delegate method of the backend's `Host` per
event. No thread of the browser waits on the engine.

- A request the engine asks (permission, dialog, auth, certificate error, select, file picker, context
  menu) arrives at a delegate method with the question and a typed responder. The backend keeps the
  responder under the `RequestId` of a control `Request`, which becomes a `Prompt` on its tab, answered once
  with a control `Response` that the backend turns into the responder's reply. A request for a tab that is
  gone is refused, and a responder that is dropped answers the default of its kind.
- Keys typed over the page go to the browser's shortcuts first (a page cannot take Ctrl+T) and Ctrl+C,
  X, V, A, Z become the engine's edit commands; the key-up of a taken key is not the page's either.
- `window.open` becomes a tab beside its opener (`accept_tab`); `window.close()` closes it and goes
  back to the opener. Popups open as tabs.

## Which bingux-ui piece for each part

| Part                                                                                    | Primitive                                                                                                   |
| --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Tab strip: add, close, reorder, pinned, tab list, loading/audio/crashed/attention marks | `tab_frame` + `tab_strip` (`TabItem::pinned`, `status`, `tab_menu`, `on_status`: added)                     |
| Toolbar, Back/Forward/Reload/Stop, downloads, zoom chip                                 | `icon_button`, `action_button`, `menu_button`; the toolbar is a row on `Paint::Bg` with a bottom hairline   |
| Address bar                                                                             | `text_field` (leading glyph, `revert_to`, `on_arrow`, `on_enter_modifiers`)                                 |
| Suggestions                                                                             | `popover(..).attached()` and `suggestion_row` (added)                                                       |
| Page info, downloads                                                                    | `popover` (added) with `Group`, `row`, `property_row`, `progress_bar`                                       |
| Find bar                                                                                | `text_field(..).flat()` (made for a docked strip), `icon_button`, `checkbox`                                |
| Permission prompt                                                                       | `banner` with three actions (Block, Allow this time, Allow); on a phone the actions take a row of their own |
| alert, confirm, prompt, beforeunload, authentication, `<select>`                        | `dialog` with `content`; the certificate details are a dialog whose way on is a destructive response        |
| `<input type=file>`                                                                     | `files::choose`                                                                                             |
| Error, crash, certificate, new tab pages                                                | `status_page`                                                                                               |
| Context menus (link, image, selection, editable, page)                                  | `Overlays::show_menu`                                                                                       |
| Settings page                                                                           | `page_column`, `Group`, `row`, `dropdown`, `property_row`                                                   |
| Phone                                                                                   | no strip: address row on top, history row below, tabs as a `popup` sheet                                    |
| Page                                                                                    | `frame_view` with `FocusEscape::Producer` (Tab goes into the page, F6 gets out)                             |

## What is missing

- **ABI/engine:** ABI 2.0 announces no lists of request or event kinds and no named features. The real
  backend subscribes to the requests it answers and reads `Capabilities` from which of them the engine
  accepts, never from a list in the browser; a kind it refuses as unsupported keeps its control disabled or
  answered with a notice (the "capabilities" snapshot shows the main menu that way). Several things stay off
  whatever the engine accepts, because the safe `vivarium` crate has no call for them: find in page, audio
  state and muting, composition (input methods commit text), pausing and cancelling downloads, the
  permission store, and touch input (fingers become a click, a scroll and a held press, `touch.rs`). A file
  picker opens files only: the engine asks for no folder and no place to save. Right click opens the
  browser's own page menu (Back, Forward, Reload, Select all, source) while the engine does not ask for its
  context menu. Favicons arrive as addresses only, so tabs show a glyph. There is no print and no "focus
  order ran out" event. A page that asked for fullscreen is not told when the person leaves it: ABI 2.0 has
  no call for that (the window's own fullscreen is the window's).
- **A long address** is cut at the edge of the address field (bingux-ui's `TextField` clips; there is no
  ellipsis or fade), so on a phone `news.bingux.dev/popup` ends mid-letter.
- **Clear history** clears the browser's history and its index and asks the engine to forget its visits
  (`Engine::clear_data`, `VV_CLEAR_HISTORY`), so visited links in open pages stop looking visited. An engine without
  the call is said so on the settings page and in the toast.
- **Not built:** the profile picker and profile pages, bookmarks, the history manager (Library), the engine's
  settings as editable controls, tear-off tabs (a Bingux window has no window to drop into), right-to-left mirroring
  (see the request below).

## The engine's life

The engine is a process of its own; the window outlives it.

- **It dies** (a crash, `kill -9`, or the library ending it because it broke the protocol): every session
  closes with the reason `EngineFailed`, then one `EngineFailed` event arrives with the exit code (or 128 plus
  the signal) and whether a new engine on the same profile is safe. The window keeps its tabs, drops what
  they were waiting for, and shows a page for the whole window ("The browser engine stopped", or "was
  stopped" for a protocol violation) with **Start again**. That recreates the engine and opens every tab
  again at its address: ABI 2.0 hands the host no saved session state, so history, scroll position and form
  data are not taken up. If the engine still does not start, the page says so again with the new words.
- **It does not start.** `engine::start` never fails: it returns a backend whose first event is `Failed`
  with the cause, and the window opens with the page. A profile folder the engine does not trust
  (`VV_ERR_PROFILE_UNSAFE`: wrong owner, writable by others, a link out of it, planted files) names the folder
  and the rule and offers to try again once it is fixed; a sandbox the engine cannot reach
  (`VV_ERR_SANDBOX_FLOOR`) offers only to close; a profile another engine has locked offers to try again.
- **It reports on itself.** The settings page shows the engine's own security report (`vv_engine_security_report`):
  the sandbox of each kind of process (level, seccomp, no new privileges, capabilities) and every other
  check by name, with a warning in amber and a failure in red. Until the engine has produced one
  (`VV_ERR_NOT_READY`) the page says "Not reported yet" and shows no report, never an empty one that reads as
  healthy. It is the engine's account of itself: it finds mistakes in how it was set up, and says nothing a
  compromised engine would not also say.
- **A question the engine ends** (its deadline passed, the page went on) takes its prompt off the screen
  (`BrowserEvent::RequestClosed`); an answer that comes late is not an error.
- **`window.open`** is a tab beside its opener, in front or behind as the page asked. A window the page
  opened as it loaded, with no action of the person (`VV_NEW_WINDOW_FROM_SCRIPT_ON_LOAD`), is a pop-up and
  is blocked with a toast. Only what the engine says counts: a request that says nothing about a gesture, as
  the engine of today sends (CR-8), is opened, because not knowing is not a verdict.

The ABI 2.0 concepts and how they map onto this state are in `docs/browser-port.md` of the Vivarium
repository, with the checks that only a real engine can answer.

## Performance

`BINGUX_FRAME_STATS=1` prints one line a second while pages change (stderr):

```
[frame-stats] presented 58/s | arrival 16.7 mean 18.2 p95 ... max (dropped 2) | shown ... | paint ... | input->frame ... | layout ... | render ... | present ...
```

`arrival` is the time between two frames reaching the tab's mailbox (the engine's pace; `dropped` frames
were replaced before the UI took them), `shown` between two frames the view put on screen, `paint` the
frame view's own recording into the canvas, `input->frame` from the last input event forwarded to the next
frame shown, and `layout`, `render`, `present` the window's frame (`bingux_ui::frame_view::stats`,
`freya_winit::bingux_frame`): `render` records the draw of the whole tree, `present` is the driver's whole
present (render, the flush that rasterises and uploads, the swap). Off, each probe is one atomic load.

Measured on the build machine (Xorg dummy 1920x1080, software GL, the engine's content processes on the
same 8 vCPUs and other agents' jobs running: **run to run it varies by a third**, so read the sizes, not the
decimals). `long.html` is 400 sections scrolled by 60 wheel notches three times down and twice up;
`anim.html` is a 40-circle canvas animation at `requestAnimationFrame`; all at scale 1:

| build   | page      | window    | frames shown/s (median of the seconds that had any) | render | present | input->frame |
| ------- | --------- | --------- | --------------------------------------------------- | ------ | ------- | ------------ |
| release | scroll    | 1100x760  | 59.5                                                | 1.4 ms | 6.8 ms  | 7 ms         |
| release | animation | 1100x760  | 59                                                  | 1.6 ms | 8.7 ms  |              |
| release | scroll    | 3440x1304 | 43.5                                                | 5.0 ms | 21.8 ms | 17 ms        |
| release | animation | 3440x1304 | 46 (an earlier run: 57)                             | 5.3 ms | 22.7 ms |              |
| debug   | scroll    | 1100x760  | 60                                                  | 1.1 ms | 3.9 ms  | 10 ms        |
| debug   | animation | 1100x760  | 60                                                  | 1.0 ms | 3.4 ms  |              |
| debug   | scroll    | 3440x1304 | 51                                                  | 4.2 ms | 17.2 ms | 13 ms        |
| debug   | animation | 3440x1304 | 36 (an earlier run: 30)                             | 6.8 ms | 38 ms   |              |

What limits the frame rate:

- **The engine is not the limit.** Frames reach the mailbox every 16.7 ms on average at both sizes (the
  engine's own 60 Hz clock; p95 23-54 ms while the machine was busy).
- **At 1100x760 nothing is: both builds show 60/s.** The frame view's work is small (`paint` 0.7-1 ms).
- **At 3440x1304 the window's present is.** One frame is 18 MB; `render` (recording) stays at 5 ms but
  `present` (rasterise, upload, swap) is 17-38 ms here, past the 16.7 ms budget, so frames are shown at
  36-51/s while the engine makes 60 and the mailbox drops the surplus. That stage is the GL driver's
  (llvmpipe on the build machine), not bingux code; on a GPU it should be a fraction of this. The numbers
  from Kieran's machine are the ones that decide this: run with `BINGUX_FRAME_STATS=1` and read `present`
  against `render`. A `present` near 16.7 ms whatever `render` costs is the swap waiting for vertical blank;
  a `present` far above it is the rasteriser or the upload.
- **Debug versus release.** Dependencies and bingux-ui are optimised in a dev build already (see the
  profile in `Cargo.toml`), so the difference is the app's own code and the missing LTO: `paint`+`render`
  were within 1.5 ms of each other at both sizes. The 3440 animation was the only run where the debug build
  fell clearly behind (36/s against 46-57/s), by the host thread being late for frames. A release build
  (`BINGUX_BROWSER_BUILD=release apps/browser/run.sh`, thin LTO, stripped, 41 MB against 573 MB)
  is what to run.
- **One copy.** The measurements above are from the earlier crate, which handed the engine's buffer to Skia
  in place. The current `vivarium` crate reads a frame only through its checked view, which copies the
  shared memory once per frame (an engine could write a buffer it handed over; the copy cannot tear under a
  reader), and the backend hands that copy to Skia without another one. The numbers have not been measured
  again since.

## Resizing

A window that grows or shrinks never stretches the page. `FrameFit::Anchored` (the default of `frame_view`)
draws the frame it has at its own size in the top left corner, clipped to the view, with the backdrop
colour over any area it leaves bare, until a frame of the new size arrives; a frame is scaled only when
its `scale` is not the view's. The size is reported to the engine at once on the first change, then at
most once per 16 ms with the last always delivered. Tested in `frame_view::tests` (a view that grows
shows the old frame 1:1 over the backdrop, one that shrinks clips it, a new-size frame fills it) and
shown in the taxonomy page "External content" ("A resize: the old frame stays 1:1 until the producer
answers"). `PixelFrame::resize_id` is the hook for the engine's answer to a numbered size report; nothing
reads it yet.

## Scrolling

The engine owns scrolling (its smooth scrolling, momentum and overscroll); the host forwards what the
device sent:

- A wheel notch is `WheelUnit::Line` with the count of notches (the window's 64 px step is divided out
  again by the view). The engine backend tells the engine `3 * notches` lines with `ticks` = notches, which is
  what Firefox on GTK reports for a notch (`deltaY` 3 in `DOM_DELTA_LINE`). Measured on the real engine
  (`pages/scroll.html`, release build): **57 px per notch** (3 lines of 19 px) with 41 animation frames for
  570 px. Before this, a notch was told as one line and scrolled 19 px, a third of Firefox's.
- A touchpad is `WheelUnit::Pixel` in the view's logical pixels (the window's device pixels divided by
  the display scale and by `BINGUX_TOUCHPAD_FACTOR`) with the gesture phase: `Began`, `Changed`, `Ended`,
  `Cancelled`. Upstream Freya dropped every phase but `Changed`, which left the engine without the start and
  the lift of the fingers (no fling, no overscroll); the vendored `freya-winit` now delivers them
  (`WheelPhase`, see `vendor/README.md`).
- None of `BINGUX_WHEEL_STEP`, the wheel feel of `ScrollView` (`wheel_accel`) or the touchpad factor
  accelerates the page: they apply to Freya's own scroll views, and the view divides them out.
- Not verified: a real touchpad (the build machine has none; xdotool sends notches, and on X11 winit
  delivers each notch twice, so a `scroll.html` run there reads twice the distance a Wayland session gives).

## Appearance

`PageEnvironment` (`bingux_ui::web_view::environment`) carries the window's look to the engine and is sent on
`EngineEvent::Ready` and whenever the tokens change (`use_side_effect_with_deps` in `ui/app.rs`); new tabs
are made in the current scheme. ABI 2.0's engine-wide call (`Engine::set_environment`) has three fields: the
colour scheme, the accent colour with the text drawn on it (black or white, by luminance) and a palette of
system colours, which the browser does not set. An engine that does not take the call answers
`Unsupported`, which is not an error: the colour scheme also goes to each session
(`Session::set_color_scheme`), and new sessions are made in it.

| bingux-ui                                        | the page sees                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------- |
| `Tokens::scheme` `PreferDark`                    | `prefers-color-scheme: dark`; `Default` and `PreferLight` are light |
| `Tokens::accent`                                 | `AccentColor` system colour                                         |
| `LANG`                                           | `navigator.languages` (set at engine start)                         |
| `Tokens::reduced_motion`, `stress::text_scale()` | nothing: ABI 2.0 has no field for them                              |
| (no token)                                       | `prefers-contrast: no-preference`; `forced-colors: none`            |
| the interface font                               | nothing: the ABI has no default-font setting                        |

## User agent

The engine builds the user agent itself: the host names the product `Firefox` (`Identity::ua_name`) and
the engine adds its own Gecko version, so pages see the standard
`Mozilla/5.0 (X11; Linux x86_64; rv:159.0) Gecko/20100101 Firefox/159.0` with no suffix. The browser reads
no file for it (it used to read the `Version=` of the SDK's `application.ini`), and the settings page shows
the string from `vv_engine_get_info`, or "Not reported yet" while the engine does not give one. The
application's own name and version (`bingux-browser`, `Bingux`) are `Services.appinfo`'s; the single-instance
name stays `bingux-browser`, so the browser never answers to a real Firefox's remote commands.

## Scrollbars and the link under the pointer (design; waiting for the engine)

Both are drawn by the host, in the Bingux look, from numbers the engine reports. The engine draws no
scrollbar into the page (a Gecko build without its widget scrollbars, which is the ABI's job) and no
status bubble.

**Scrollbars.** The page scrolls inside the engine (APZ owns offset, momentum and snap), so the host
cannot use a `ScrollView`: Freya's `ScrollBar` (vendored `freya-components`, `ScrollBarContext`) takes its
numbers from a `ScrollController` of a view it scrolls itself. bingux-ui needs a standalone primitive,
`scroll_indicator(axis, ScrollMetrics { offset, viewport, content }).on_scroll_to(f).on_drag(f)`: the same
thumb, track, hover growth, idle fade and `Dim` sizes as the scroll views, stateless about what scrolls.
The browser lays it over the right and bottom edge of the `frame_view` (a `Stack`, not a sibling: it must
not shrink the page), shows it while the pointer is over the page edge or the page scrolled in the
last second, and turns thumb drags and track clicks into engine commands.

What the ABI must offer (sent to InputC):

1. `vv_event_scroll { session, root_scroller_id, offset_x, offset_y, viewport_w, viewport_h, content_w,
content_h, scale }` for the root scroller of the top frame, in CSS pixels, on every change, coalesced
   to the frame rate and always sent once after the load finishes and after a resize. A flag for
   `overflow: hidden` axes (no bar) and for `scrollbar-width: none`.
2. `vv_session_scroll_to(session, x, y, behaviour)` with `behaviour` = instant (a thumb drag), smooth
   (a track click, a page step), so the host does not synthesise wheel events for a drag.
3. `vv_session_scroll_by_page(session, direction)` or the key forms through `send` already: Space, PageUp
   and the arrows keep going as keys, which the engine scrolls with its own smooth scrolling.
4. A way to say "this session draws no scrollbars" (a session or engine config bit), so the page and the
   host do not both show one; plus `color-scheme` and the accent through `vv_environment` for any
   scrollbar the page styles itself.
5. Nested scrollers: only the root scroller is reported; a scrollable sub-area keeps the engine's
   overlay bar until the ABI reports it too (say so in the answer).

**Link under the pointer.** `BrowserEvent::Tab(HoverLink(Option<String>))` exists and the model keeps it
in `Tab::hover`; `Capabilities::hover_links` is false for the real engine, so the bubble never shows.
The host draws a status card at the bottom left of the page (the address elided in the middle with
`elide_middle`, opposite corner when the pointer is there), 300 ms after the pointer settled on the
link, immediately when the keyboard focuses one.

6. `vv_event_hover_link { session, url (UTF-8, absolute), target ("_blank" or empty), is_download }`
   on enter and `url = ""` on leave, from the engine's own hit test (`nsIDOMWindowUtils` hover target),
   not from the host re-testing positions.
7. The same for keyboard focus on a link (`focus = true`).
8. Context-menu target (link, image, selection, editable) with the same hit test, so right click can
   stop using the browser's own page menu.

## Request to bingux-ui: right-to-left

The browser does not mirror its chrome in a right-to-left locale because bingux-ui cannot say which way
its layout runs, and an app must not invent that privately (Settings and Files need the same). What
the library has to provide, in the order an app would use it:

1. **A direction in the environment.** `Direction::{Ltr, Rtl}` next to the scheme and the text scale,
   from the locale (`LANG`/`LC_MESSAGES` language: ar, he, fa, ur, ...) with an override
   (`BINGUX_TEXT_DIRECTION=rtl`, and a snapshot/lint switch for the same), read through `use_tokens()`
   so a change repaints. Today there is no such value: `grep -ri rtl crates/ui/src` finds only
   unrelated words.
2. **Logical edges in the layout vocabulary.** `ps/pe` (padding start/end), `ms/me`, `start()`/`end()`
   for alignment and absolute offsets, and `flex_row()` laying children out right to left in `Rtl`
   (the `tw` helpers `px`, `pl`, `pr`, `items(Align::Start)`, `justify` map through the direction). Every
   `Row`, `Group`, `TabStrip`, `Toolbar`, `HeaderBar` (window controls go to the other side), `popover` placement
   (anchor edge, "opens towards the start"), `NavigationSplitView` (sidebar on the right), `Dropdown`,
   `Segmented`, `Stepper` and `Switch` (knob travel) has to use them.
3. **Directional icons.** A per-icon "mirrors in RTL" flag (Back, Forward, chevrons, "next/previous",
   list-with-bullet, undo/redo, send; not Reload, Search, Play, a clock or a check mark) applied by the
   `icon` element, so `Icon::GoPrevious` is drawn pointing right in `Rtl` without each app flipping.
4. **Text.** The text layout (`bingux-text-layout`, Skia paragraph) takes a base direction per paragraph:
   `Auto` by the first strong character (the default for user content such as titles and addresses),
   `Inherit` for chrome labels. `text_field` needs the matching caret and selection movement (Left
   moves to the visual left), a start-aligned placeholder, and Home/End mean start/end of the line.
   Ellipsis goes on the end side, a URL stays left-to-right inside an RTL field (an isolate).
5. **Keys and gestures.** Left/Right in lists, tabs, segmented controls, sliders and trees step
   toward the start/end, swipe-back goes the other way, and horizontal scroll starts at the right edge.
6. **Proof.** `BINGUX_TEXT_DIRECTION=rtl` in `layout-lint.py`, in `focus-ring-check.py` and in the
   taxonomy's snapshot run, with a mirrored specimen for each component above.

Until then the browser's `rtl` snapshot scenario shows an RTL _page_ (drawn by the engine), with the
chrome unmirrored; that is the honest state, not a design choice.
