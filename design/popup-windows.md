# Popup windows

Status: popup windows work on sway for menus, submenus and popovers (see "Verified"); the layering model
exists with tests, but not every window kind registers in it yet (see "Not verified / not done").

Kieran's report: a Files context menu is drawn inside the window, clipped by the window bounds. GTK
lets a menu leave its window. The library's overlays (`shell/overlays.rs`, `shell/panel.rs`,
`shell/menu.rs`, `controls/dropdown.rs`) are in-window layers, and `window/popup.rs` popups are
in-window sub-windows, so none of them can.

## Route decision, with evidence

Wayland gives a client three ways to draw something that is not inside its toplevel's rectangle.

| Option | Verdict |
|---|---|
| `xdg_popup` (xdg-shell): a surface placed by the compositor against an anchor rectangle of its parent, with gravity and constraint adjustment (flip, slide), a seat grab and a dismissal event (`popup_done`). This is what GTK uses for menus and popovers. | **Chosen.** |
| `wl_subsurface` of the parent: no placement help, no grab, no flip at the output edge, and the compositor decides whether it is clipped to the parent's bounds. | Rejected. |
| Child `xdg_toplevel` with `set_parent`: clients cannot position toplevels. | Rejected for menus. Used for dialogs and tool windows (`WindowConfig::with_parent`), where stacking above the parent is what is wanted. |
| `wlr-layer-shell` popups: not available to ordinary clients on mutter/Gnoblin. | Rejected. |

winit 0.30.13 (crates.io) has no `xdg_popup`. It exposes the `xdg_toplevel` pointer only
(`WindowExtWayland::xdg_toplevel`); the `xdg_surface` is not reachable from outside (the raw handle
gives `wl_surface` and `wl_display`). Reaching it from outside would need a second Wayland
connection layered over winit's and a hand-written input path. Patching winit is smaller and keeps
winit's seat, scale, IME and frame-callback machinery, so winit is vendored
(`vendor/winit`, listed in `vendor/README.md`) and gains a popup role.

### Proof of feasibility (run on sway 1.11, headless, `grim` screenshots)

`/var/tmp/PopupWindows/poc` is a 150-line client: winit (patched) plus softbuffer, a 500x400
toplevel floated at 100,100. Results, screenshots looked at:

- A click at the window's bottom right opened a 320x420 popup placed by `xdg_positioner`
  (anchor rect 1x1 at the pointer, anchor TopLeft, gravity BottomRight). It extended past the
  parent's right and bottom edges, outside the window, up to the output edge.
- With the parent moved to the output's lower right, the same request came back flipped on both
  axes (`placed at -19,-119` relative to the parent): the compositor flipped at the **output** edge,
  not the window edge, which an in-window overlay cannot do.
- A click inside the popup opened a popup of the popup (three surfaces chained, each outside the
  one before).
- A click on the desktop outside the app dismissed the grabbed popup: `CloseRequested` arrived
  (`popup_done`).
- **sway delivers keys to the parent toplevel, not the popup.** The Escape press arrived at the
  root window id. Compositors differ (mutter gives the popup the keyboard), so the renderer
  delivers every key a popup receives to the window it grew from, and keeps that window looking
  focused. Menu key handling therefore stays in the window that opened the menu.
- **sway delivers presses on the parent while a popup is open to the parent** (wlroots only
  dismisses on a press from another client). The opener therefore keeps its clear shield over its
  content: a press on it dismisses and is swallowed, as the in-window menu does.

Not run: nested Gnoblin (the devkit prefix `~/.local/opt/gnoblin` from `history://GnoblinTear`),
mutter proper. `xdg_popup` with grab is core xdg-shell; mutter implements it for GTK. Their
differences (keyboard focus on the popup, geometry-based placement) are handled as above. Verify on
Gnoblin before shipping.

## What was built

### winit (vendored, `vendor/winit`, 0.30.13 + Bingux patch)

- `platform::wayland::WindowAttributesExtWayland::with_popup(PopupAttributes)`:
  parent window, anchor rect, anchor, gravity, offset, constraint adjustment, card size, `margin`,
  `grab`, `reactive`. Creating the window blocks until the first configure, as for toplevels, and
  fails (returns `Err`) when the parent is gone or the compositor dismisses the popup first.
- `margin`: the surface is the card plus a transparent margin for its shadow. The window geometry
  and the input region are the card, so the compositor places the card and a press on a shadow is
  not a press on the popup. winit reports the surface size.
- The compositor's answer arrives as `WindowEvent::Moved` (card position in the parent geometry);
  `popup_done` arrives as `CloseRequested`; `request_inner_size` on a popup is
  `xdg_popup.reposition`.
- The grab names the seat and serial of the last button press on the parent.
- Destruction order: an `xdg_popup` destroyed before a popup that grew from it is a protocol error
  that ends the connection. The event loop destroys the deepest popup first and defers a window
  that still has an open popup.
- `Role` (`window/role.rs`): the toplevel or popup behind a `WindowState`; toplevel-only requests
  are no-ops on a popup.
- `WindowAttributes::with_parent_window` now parents a Wayland toplevel (`xdg_toplevel.set_parent`).
  It was ignored.
- Files: `platform/wayland.rs`, `platform_impl/linux/mod.rs`, `wayland/{state.rs, event_loop/mod.rs,
  window/{mod,state,role,popup}.rs}`. `Cargo.toml` is trimmed (no examples, tests, dev-deps).
  `[patch.crates-io] winit` added to the four app manifests.

### freya-core

`ContextSnapshot` (`snapshot_contexts`, `provide_context_snapshot`): every context visible from a
scope, copied to another runner's tree. A popup is a window with a runner of its own; its content
reads the tokens, direction, router and overlays of the window it came from. Types the new runner's
root already has (ticker, platform, fonts) are not overwritten. State is a shared handle, so a row
in the popup writes the state the opener reads.

### freya-winit

`popup` module (`supported()`, `placed()`, `PopupMarker`), `WindowConfig::with_popup` and
`with_parent`, `WinitPlatformExt::try_launch_window` (`None` instead of a panic when the compositor
refuses), keys and modifiers of a popup redirected to the window it grew from, focus changes between
a window and its popups swallowed, `Moved` mapped to `popup::placed`.

### bingux-ui `popup_windows` (`Panel` drives it)

- `place.rs`: `request(PanelTrigger, size, direction)` builds the `xdg_positioner` request (the gap
  is part of the anchor rectangle, not an offset, so a flipped popup keeps it); `resolve` is the
  compositor's algorithm from `xdg-shell.xml` (anchor, gravity, flip then slide per axis). 14 unit
  tests: context menu below/after the pointer, RTL, flip on each axis and both, slide, a popup
  bigger than the output, an output at negative coordinates, a parent off the origin, a control's
  menu flipping above, a submenu flipping to the other side.
- `backend.rs`: `PopupBackend` (open, resize, close, window), `WaylandBackend` (Freya windows made
  as `xdg_popup`s, children closed before parents, a task that is not tied to the asking component
  so a popup made after its panel is gone is closed), `FakeBackend` (records calls, plays refusal and
  dismissal).
- `portal.rs`: `NativePopup`/`use_native_popup` (the lifecycle a `Panel` drives: sync each render,
  refused/dismissed events), `popup_root` (the popup window's root: context snapshot, an error
  boundary of its own), `ParentSlot` (what a submenu and popup content open their own popups from).
- `mod.rs`: route table, `PopupContent` (the cell the opener writes and the popup renders),
  `PopupOrigin` (pointer positions of a popup translated to the opener's coordinates).

## Rule: what is a popup window, what stays in the window

Evidence: libadwaita (`AdwDialog`, in-window) for dialogs; GTK4 popovers/menus (`xdg_popup`, always)
for menus; GTK4 tooltips are popups too, but a window per hover is heavy.

| Surface | Route |
|---|---|
| Menu, context menu, hamburger/path/history menu, dropdown list, popover, submenu | Popup window, always, when Wayland and not a phone-sized window |
| Phone-sized window (`is_compact`) Choice | In-window bottom sheet (touch design) |
| Dialog, About, forms, chooser | In-window modal card over a scrim (it belongs to its window) |
| Text edit bar | In-window (pinned to a caret) |
| Tooltip, hover card | In-window while it fits inside the window; **to do**: a popup window only when it would be clipped |
| X11, snapshot harness, any refusal | In-window (unchanged; refusal switches the whole session) |

## Verified with real input (sway 1.11, headless, Files at 700x500 and 1000x650, screenshots looked at)

- Context menu on a row near the window's bottom right: the card leaves the window past the right and
  bottom edges, rounded and transparent, down to the output edge (slid up, as the flip would not fit).
- Open with: hovering the row opens a child popup beside it, top aligned with the row.
- Escape closes the submenu, then the menu; the app survives (no protocol error).
- A press on the desktop outside the app dismisses it (compositor grab); a press on the window while a
  menu is open dismisses it and is swallowed by the opener's shield.
- Hamburger menu and the view/sort menu extend past the window's right edge.
- 700x500 is phone-sized: path dropdowns are the in-window bottom sheet (the rule); moving the window
  moves the sheet with it.
- Tab context menu: a popup window above the tab strip; `BINGUX_DEBUG_LAYERS=1` printed the stack
  (`Popup parent=#<window>` above the `Toplevel`) and emptied it on Escape.
- A context menu over the docked preview pane is above it. Moving the window with a menu open: the
  popup follows the window (the compositor keeps the relative place).

## Not verified / not done

- Nested Gnoblin and mutter: not run (disk was at 3 GB). Expected differences: keys arrive on the popup
  (redirected), geometry-based placement.
- Not driven with real input: a dropdown list on a regular-size window, the colour picker popover, a
  history menu with entries, a menu opened from inside a popup window's content (needs the opener
  window on `Overlays` menus), a menu over the modal preview card (covered by the in-window ordering
  test `menu_over_dialog_tests`; a popup window is above its window by construction).
- Popup origin is the layout's assumption, not the compositor's after a flip (`popup::placed` is
  available); fractional scale and multi-monitor runs.
- Tooltips stay in-window (rule above); the "popup only when clipped" upgrade is not built.

## Layering (done)

- `layers.rs`: the table (relative layers, overlay levels, named floating-card layers replacing every
  `Layer::Overlay` site) and `WindowStack` (rules in the module doc, tests).
- Registered in the stack: popup windows (both backends), in-window sub-windows (`WindowManager`, modal
  children of the popup or window they were opened from), windows made by `app::open_window` /
  `open_child_window` (removed on close). The inspector and the Files tear-off window are independent
  toplevels: the inspector is not registered, the tear-off window is (through `open_window`).
- Child toplevels: `app::open_child_window(parent, ..)` and `WindowConfig::with_parent` set
  `xdg_toplevel.set_parent`. Nothing in the apps needs it yet: choosers, properties and About are
  in-window sub-windows or dialogs, not real windows.
- Tests: `layers::tests` (model), `popup_windows::route_tests::stacking` (submenu chain, menu vs dialog,
  popup from a dialog and a chooser over it, parent focus raises children, refusal, an in-window
  sub-window registering and leaving).
