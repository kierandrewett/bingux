# Tearing a tab off

Dragging a tab down (or up) out of its strip takes it to a window of its
own, the window follows the pointer, and dropping it on another window of the
same application puts the tab there. Files is the first application with tabs;
the code is the SDK's, so every application that has tabs gets it. The strip's
part is in `controls/tab_tear.rs` and `tabs.rs`; the window's is in
`bingux_ui::tabs` (`tearoff.rs`, `windows.rs`, `set.rs`, `actions.rs`) and the
compositor questions are in `bingux_ui::window::compositor`. This note is what
was asked of the compositor, what each answered, and what was built because of
it.

## What works where

|                                                                            | sway 1.11 (headless)                                                                                              | Gnoblin 0.1.10 (nested in headless sway)                                                                                                                                                                                                                                                                                                       | GNOME Shell, KDE Plasma, other compositors                                                                             |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Tab past the threshold is torn off, the window it leaves keeps the rest    | yes                                                                                                               | yes                                                                                                                                                                                                                                                                                                                                            | yes (same client code; not run)                                                                                        |
| The torn tab's window follows the pointer                                  | yes, if the window is floating (a tiled window ignores a move, so it stays and the tab is in a window of its own) | yes (`xdg_toplevel.move`; mutter jumps the window so the press place is under the pointer)                                                                                                                                                                                                                                                     | by the same request; not run                                                                                           |
| Where the windows are, live during the move                                | sway IPC `GET_TREE`                                                                                               | control socket `windows.list`, frames follow the moving window                                                                                                                                                                                                                                                                                 | not available to a client                                                                                              |
| Marker in another window's strip while the carried window is over it       | yes (carried window drawn at 40%, the marker shows through)                                                       | the marker is set at the right index but was **not seen**: the faded window was not composited translucent. Suspected cause, fixed in `crates/ui/src/app.rs`: freya cleared a transparent window's canvas to opaque white before drawing, so the 0.4 opacity mixed with white; the clear is now transparent too. **Not yet re-run on Gnoblin** | no                                                                                                                     |
| Dropping on that strip joins the tab at the marker, carrying window closes | yes                                                                                                               | yes: Pictures dropped between Documents and Music joined at index 1 and the carried window closed                                                                                                                                                                                                                                              | **no**: dropping onto a strip is unavailable                                                                           |
| Let go anywhere else: the window stays where it is                         | yes                                                                                                               | yes (no marker, no fade left)                                                                                                                                                                                                                                                                                                                  | yes                                                                                                                    |
| Let go beyond the strip without a drag (a flick): the tab opens a window   | yes                                                                                                               | yes (same code; not run on Gnoblin)                                                                                                                                                                                                                                                                                                            | yes (not run)                                                                                                          |
| No compositor to ask                                                       | n/a                                                                                                               | n/a                                                                                                                                                                                                                                                                                                                                            | no worker thread, no marker, no fade; the carry ends with the first pointer event any Files window gets, or after 90 s |

Gnoblin was run as the private 0.1.10 devkit prefix (`~/.local/opt/gnoblin`)
nested in a headless sway, Files inside it, real pointer input through the
devkit viewer (the viewer's second monitor is at x = 1280 in the nested layout,
so every window in the runs was on a monitor with an offset). Not run: Kieran's
live Gnoblin session, a fractional scale (the nested monitors were scale 1.0;
units are covered by tests), Bingux's frame rule (`extents {36, 0, 0, 0}`;
covered by a test of `Frame::content`), Gnoblin's z-order (a window covering
the target is not looked through), and GNOME Shell or KDE at all.

## Thresholds ([`tab_tear`])

The pointer's distance from the strip's nearest edge, in strip heights, up or
down. `ARM_AT` 0.6: the held tab is drawn faint, to say going on tears it off.
It is put down again only below `DISARM_AT` 0.4, so a hand that drifts round
0.6 does not flicker. `TEAR_AT` 1.0: torn off; the state does not end (a tab
brought back is a new drag). A **lone tab** has nothing to be reordered among,
so any drag is a window drag: it tears after `LONE_SLOP` 6 px in any direction,
which is Chrome's behaviour. Let go beyond `TEAR_AT` in one move (the button is
already up), the tear is reported with `released` set.

## Who follows the pointer

A client cannot place a window on Wayland. The only way to move one is
`xdg_toplevel.move(seat, serial)`, which winit exposes as
`Window::drag_window()`. It sends the seat's _latest button serial_; winit does
not check which surface the press began in. The compositor does:

- **mutter** (`subprojects/mutter/src/wayland/meta-wayland-pointer.c`,
  `meta_wayland_pointer_can_grab_surface`): the serial must be the pointer's
  grab or click serial _and_ the pointer's focus surface must be the surface
  asking, or one of its subsurfaces. A window made a moment ago is not the focus
  surface (the one the press began in is), so **it cannot be given the move**.
- **sway 1.11**, tried: a new window asked for the move 300 ms after it was
  created, button held. `drag_window()` returned `Ok(())`, the pointer moved
  about 170 px and the window stayed where the compositor had put it (centred).
  The probe was removed after the run.
- **`xdg-toplevel-drag-v1`** is the protocol for this (a window attached to a
  data-device drag, which mutter implements in `meta-wayland-toplevel-drag.c`).
  It needs a `wl_data_device` drag source, and winit has none. Not used.

So the window that follows the pointer is the one the press began in. When it
holds more than the torn tab, the others go to a **new window** (opened at the
same size, wherever the compositor puts it) and the window under the pointer
keeps only the torn tab: Chrome's multi-tab tear, mirrored. The pointer keeps
the place the compositor holds it at (sway: where it was when the move was
asked for, a strip height or more below the strip; mutter: where the button went
down, on the tab, so the window jumps to put its strip under the pointer). A
window with one tab is dragged as it is. A move that the compositor refuses (a
tiled window in sway) leaves the tab in a window of its own where it was.

## Putting it back

While the compositor moves a window the client hears nothing, and Wayland tells
a client no window's place, its own included. The place has to come from the
compositor:

- **sway** answers `GET_TREE` (over `SWAYSOCK`) during a move with every
  window's rectangle, title and pid. A worker thread asks about 40 times a
  second (`window/compositor`, never on the UI thread). The pointer's place in the
  carried window is constant for the whole move, so the pointer's place on the
  screen is that window's content origin plus it. Windows are told apart by
  title: a later window's title ends in its number (`Home – Files (2)`).
- Each Files window publishes where its tabs are (`StripProbe`), so the pointer
  over a strip, and between which tabs, is plain arithmetic (`tearoff::hover`,
  `StripLayout::index_at`). The target draws a **marker** (`TabStrip::insertion`)
  and the carried window is drawn at 40% (`Application::transparent`, windows
  have an alpha channel; their content is still opaque), because it is on top of
  the strip that holds the marker.
- The move ends when the compositor gives the pointer back: the first pointer
  event any Files window gets after the move began (the carried window may be
  under another, so it is not necessarily its own). Over a strip, the tabs
  join it at the marker, the carried window closes, the target is raised;
  anywhere else nothing happens.
- **Gnoblin** answers on its control socket, `$GNOBLIN_COMPOSITOR_SOCKET` or
  `$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock` (newline-delimited JSON; the
  greeting is one `hello` line, then each request gets one `reply` or `error`
  line with its `id`). `windows.list` (API 1.37, answered from Gnoblin's Lua
  supervisor's window snapshot, so it errors with "Lua supervisor is not
  connected" until that is up) returns one record per window with `id`,
  `title`, `app_id`, `workspace_id`, `sticky`, `minimized`, and `frame`
  (`x`, `y`, `width`, `height`: signed integers in the logical pixels of the
  whole layout, so a monitor left of the first has negative ones and a scaled
  monitor is in the pixels it shows). That is enough, and no addition to Gnoblin
  was needed. What it does not have: a process id (a Files window is told by
  `app_id` and title), the pointer's place (it follows from the carried
  window's frame and where the compositor holds the pointer in it, below), and
  the extents of a frame Gnoblin draws around a window (`frame` includes them,
  so `Frame::content` takes them off using the window's own content size).
  `window/compositor` keeps one connection and asks about 40 times a second on the
  worker thread; an error reply is no answer, a closed or silent connection is
  made again, and after three misses the learned places are forgotten so no
  marker is shown for a compositor that has stopped answering.
- **Where mutter holds the pointer.** `xdg_toplevel_move` anchors the move at
  `pointer->grab_x`/`grab_y` (`meta_wayland_pointer_get_grab_info`): the place
  the _button went down_, not where the pointer is when the move is asked for.
  Seen on Gnoblin 0.1.10: a tab pressed at (215, 22) of its window was torn when
  the pointer was at (215, 94); the window jumped so that (215, 22) was under
  the pointer (the pointer stays over the tab's place in a strip, as in
  Chrome), the client got `wl_pointer.leave` at the move's start and
  `wl_pointer.enter` at (215, 22) when the button came up. sway starts the move
  where the pointer is. `Detach::press` carries the press and
  `Locator::anchor` says which a compositor uses, so the pointer's place in the
  carried window is right on both.
- **Choosing between them.** `Locator::detect` takes Gnoblin when
  `GNOBLIN_COMPOSITOR_SOCKET` is set or `XDG_CURRENT_DESKTOP` names Gnoblin
  (also `GNOME:Gnoblin`), even if `SWAYSOCK` is inherited from a sway terminal
  that started a nested Gnoblin; else sway when `SWAYSOCK` or `I3SOCK` is set;
  else Gnoblin when its default socket exists; else nothing is asked.
- **Everything else (GNOME Shell, KDE Plasma, any other compositor):** nothing
  is asked. GNOME Shell's `org.gnome.Shell.Introspect.GetWindows` answers
  `AccessDenied: GetWindows is not allowed` to ordinary clients
  ([GNOME Discourse](https://discourse.gnome.org/t/unable-to-call-the-remote-getwindows-gnome-method-via-dbus/21201)),
  and none of KWin's interfaces was used. A tab can still be torn off, its
  window follows the pointer, and letting go leaves it where it is; there is no
  marker and no drop onto a strip. Nothing waits: with no one to ask there is
  no worker thread, no marker, no fade, and the carry ends at the first pointer
  event any Files window gets after the move (the compositor giving the pointer
  back). If the move ends over something that is not a Files window, no event
  comes, and the carry is dropped after 90 seconds (`MAX_CARRY`), polling and
  fade included.
- Not built because it cannot be: one more process per window. Files is one
  process (a second launch hands its folder to the running one), so every
  window is on one thread and the windows are reached through a registry
  (`tabs/windows.rs`); no socket between processes was needed.
- Not usable: `files::provide_file_drag` is a drag inside one window's state;
  it never sees a pointer that is over another window.

## What moves

`TabMemory` is the tab (a Files tab, `FileTab`, is its place and this memory;
the SDK's `TabContent::depart` is what calls it): its history (the `NavModel`), the number the view keys
its scroll and selection by, and, now, `Away`: what its body knows. `depart()`
records where the view is scrolled (`scroll_memory::flush`, which asks the view
on show to record), snapshots the states the body keeps (selection, sort, icon
size, hidden files, preview pane, the path or search being typed), and ends the
tab's _residence_, so the window it leaves lets go of its states. The window it
joins starts its states from the snapshot, once. The memory is the same `Rc`
before and after, so nothing is copied and nothing is left behind; `TabSet`
(`tabs/set.rs`: `detach`, `detach_others`, `detach_all`, `insert_moved`, `adopt`) hands it over
and gives the tab a number of its own in the window that takes it. A moved tab
is not a closed one: it is not in _Reopen closed tab_.

## Files

`controls/tab_tear.rs`, `controls/tabs.rs` (`on_detach`, `insertion`, `probe`),
`app.rs` (`open_window`, `use_window_seed`, `Application::transparent`),
`files/browser.rs`, `files/chooser.rs`, `files/view.rs`,
`files/scroll_memory.rs` (the carry); `tabs/set.rs`, `tabs/actions.rs`,
`tabs/tearoff.rs`, `tabs/windows.rs`, `window/compositor/mod.rs` and
`window/compositor/fake.rs` (a fake Gnoblin control socket for the tests); in
the Files app only `app.rs`, which puts a `FileTab` in a `TabSet`.
