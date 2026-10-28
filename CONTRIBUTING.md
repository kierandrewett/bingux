# Contributing to Bingux

Bingux is a desktop shell. Keep changes focused on the shell, its native
helpers and the protocols between them. Machine images, host networking and
distribution profiles do not belong in this repository.

## Before you change code

Read the relevant guide in `docs/`. Keep user-facing text short and concrete.
Preserve the existing settings format unless a migration is included. Do not
commit generated build output, local configuration or core dumps.

## Checks

Use the native build entry point:

```sh
make
make check
```

`make check` builds both Rust daemons, runs their library and QML socket tests,
checks native text layout and OSD event routing, and verifies a staged install.

To verify native OSD event routing and status-only rendering in a nested
Gnoblin session, build the status daemon and run:

```sh
BINGUX_STATUSD_BIN="$PWD/build/cargo/release/bingux-statusd" \
GNOBLIN_PREFIX=/path/to/gnoblin/install \
GNOBLIN_DEVKIT_EXEC="python3 $PWD/tests/osd-native-event.py" \
/path/to/gnoblin/scripts/run-gnoblin-devkit.sh
```

The check uses a test compositor peer to send a Mutter-shaped OSD event. It
exercises the production status daemon and OSD surface in a real nested
Gnoblin session; it does not trigger Mutter's hardware event producer.

To verify native window-menu events in the same nested session, run:

```sh
GNOBLIN_PREFIX=/path/to/gnoblin/install \
GNOBLIN_DEVKIT_EXEC="python3 $PWD/tests/window-menu-native-event.py" \
/path/to/gnoblin/scripts/run-gnoblin-devkit.sh
```

This check uses a test compositor peer to cover event subscription, stale
window rejection, actions bound to the requested window, and action failures.
It does not generate a real title-bar click in Mutter.

When the full Qt development set is unavailable, run the checks that do not
need it and state that limitation in the change description. Useful focused
checks include:

```sh
python3 tests/standalone-install.py
python3 tests/extensions.test.py
node --test tests/desktop-layout.test.mjs tests/control-layout.test.mjs
```

For the packaged Gnoblin autostart integration, see
[`tests/bingux-autostart-config.md`](tests/bingux-autostart-config.md).

A change to a Bingux Settings page must keep its layout still while its data
loads. Build `packages/bingux-settings` with `--features snapshot`, then run
`python3 scripts/layout-stability.py`: it fails when a row separator or card
border moves between the loading and the loaded frames, at desktop and phone
size. `bingux-config` must be built beside the binary (or named by
`BINGUX_CONFIG_BIN`). See "Loading contract" in `docs/settings.md`.

A change to the phone layout, a sheet, a dialog, a toast or a page transition must keep
`python3 scripts/mobile-walk.py` passing (build with `--features snapshot` first): it records
every frame of each transition at 390x800, 360x640 and 800x390 and fails when a sheet
reflows, a frame flashes or blanks, or content touches an edge. See `docs/settings.md`.
Every size token that is a hit area passes the touch minimum at compact density
(`Dim::is_touch_target`, tested in `style/tokens.rs`).

Nothing may block the UI thread: no process, file read, sleep or lock held
across work in `render`, an event handler, an effect or the initial value of a
`use_state(|| ..)`. Use the workers in `bingux_ui::runtime` and
`bingux_ui::task`; they start their jobs after a page slide has settled, and
a page opens from its cache or a placeholder. Keep a hook-bearing element (a
focus stop, a card) in a `Component` of its own so a page's hook count does
not change when its data lands. `scripts/ui-lint.py` rejects the raw calls,
`BINGUX_UI_WATCHDOG=1` names a blocked frame with its stack, and
`python3 scripts/frame-time.py --slow-sources 500` fails when a page
transition, scroll, drag or typing drops frames with every command slowed;
`python3 scripts/nav-slide.py` fails when a push or Back does not slide. See
"Nothing blocks the UI thread" in `docs/bingux-ui.md`.

## Changing bingux-ui

Every change to `packages/bingux-ui` (a new or changed component, control
builder, icon, style token or behaviour) must land with its entry in the
`bingux-taxonomy` app, in the same commit:

1. Add or update the entry in `packages/bingux-taxonomy/src/registry.rs` and
   the states it shows in `packages/bingux-taxonomy/src/demos/`. Every public
   item you add is named in some entry's `covers` list, or, for an internal
   that has nothing to look at, in `packages/bingux-taxonomy/coverage-allowlist.txt`
   with its reason. A new icon needs nothing more: the Icons page lists the
   `icons/` directory.
2. Run `python3 scripts/taxonomy-check.py`. It fails on an exported item no
   entry covers, a stale `covers` name, an `Icon` variant missing from
   `Icon::ALL`, and a doc anchor that is not a heading. `make check` runs it
   and `cargo test` in the taxonomy crate.
3. Look at the change running: `bingux-taxonomy --snapshot /tmp/taxonomy
   <category>/<page>` writes the page at desktop size and at 390x800, and
   `bingux-taxonomy <category>/<page>` opens it with the theme, density and
   scale switcher in the header. Put the screenshot paths, or what you saw,
   in the change description.
4. Run `python3 scripts/layout-lint.py` (after building the settings and
   taxonomy binaries, or with `--build`) before committing a UI change. It runs
   the inspector's Layout checks headlessly over every Settings page, popup and
   taxonomy page at desktop size and at 390x800 and fails on uneven padding,
   dead space, off-centre or unequal controls, clipping, overlap, cut-off text
   and misaligned baselines. Fix the component or token; an entry in its
   `ALLOWED` list needs a reason.
5. Run `python3 scripts/overflow-check.py` (with `--build`, or after building
   the same binaries) before committing a UI change. It renders every
   Settings page, popup and taxonomy page again under pseudo-localised
   strings (`BINGUX_PSEUDO_LOCALE=expand`, 40 % longer), the 200 % text
   scale, very long unbroken names, 390x800, 320x600 and every popup at 320 px,
   and fails on text that wraps a letter to a line, is cut off without an
   ellipsis, leaves its box, covers other text, scrolls sideways or hides a
   control's label. A label and its control in a form line follow the rule in
   `controls::adaptive`; do not give a field a fixed width in a wrapper. An
   `ALLOWED` entry needs a reason. See "Text must be readable" in
   `docs/bingux-ui.md`.
6. Run `python3 scripts/focus-ring-check.py` (with `--build`, or after building
   the same binaries) before committing a change to a control, a row or the
   focus ring. It renders every Settings page, popup and taxonomy page at
   desktop size and at 390x800 and fails when a focus ring is not drawn around
   the box its control paints, or does not follow that box's corner radius. A
   control ends `render` with `.focusable(..)` on the node that carries its
   fill and passes that node's radius; see "Focus" in `docs/bingux-ui.md`. The
   same rule is the inspector's Layout rule "Focus ring off its face", so
   `layout-lint.py` fails on it too.

A menu, context menu or dropdown that lists applications must show each app's
icon, through the app list API: `AppChoice` with `app_menu_items` or
`app_dropdown`. `cargo test -p bingux-ui --lib app_icon` fails on a list built
any other way.

See "Taxonomy app" in `docs/bingux-ui.md`.

## Panics are never fatal

A panic in UI code must never end the process. The vendored `freya-core`
catches it where the code ran, and `bingux_ui::resilience` shows an error card
(a render), a toast (a handler, effect, task or worker) or relaunches the app
(out of memory, a lost GPU). See "Panics are never fatal" in
`docs/bingux-ui.md`. What that asks of a change:

- Do not set `panic = "abort"`, call `process::abort`, or wrap UI code in your
  own `catch_unwind`. `scripts/ui-lint.py` checks the app manifests: every app
  patches `freya-core` and unwinds in `[profile.dev]` and `[profile.release]`.
- Pages, popups, dialogs and the window already sit in an `ErrorBoundary`. Put
  your own (`error_boundary`, `error_boundary_with`) around a part that can fail
  on its own: one card, one list row, a view of external data.
- A hook in the wrong place still is a bug; the boundary only makes it
  survivable. A change that touches hook order, or code that can panic on
  input, comes with a test in the `resilience/tests.rs` pattern: the card shows,
  Retry remounts, the siblings keep their state.
- Panic messages are shown to the user and copied into bug reports: say what
  failed and with what, not only `unwrap()` on `None`.

## Packaging

Builds use `packaging/rpm/bingux.spec` and the source archive produced by
`make source-archive`. A package must pass a clean Fedora build and a staged
install before it is published to COPR.

Use small commits with a conventional subject and a body that explains the
user-visible result. Keep unrelated working-tree changes out of a commit.

## Lint and format

See [Code quality](docs/code-quality.md) for tool setup, language coverage,
whole-repository checks and formatting selected files.
