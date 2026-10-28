# Navigation in bingux-ui

Contributor design note. The usage docs are the "Navigation" section of
`docs/bingux-ui.md`; this file says what existed, what was compared, what was
decided and why, and what is deliberately not done. Update it when the design
changes.

The one-sentence version: **a location is a URI, the back stack is a list of
URIs, a route table maps URI patterns to pages, and everything the person sees
about navigation (Back, the header title, the breadcrumb, the history menu,
the window title, the predictive-back preview, search jumps, the inspector)
is derived from that one list.** An app declares its routes as data and gets
all of it by construction.

## 1. What exists today (audit)

| Piece | Where | What it does | Why it is not the answer |
| ----- | ----- | ------------ | ------------------------ |
| `Address` | `bingux-ui/src/address.rs` | `scheme://seg/seg?k=v`, segments percent-escaped, `parent()` drops a segment. No fragment, no canonical form (query order and `..` matter), no pattern matching. | Right shape, too little: nothing resolves an address to a page, and equal places can compare unequal. |
| Window address | `bingux-ui/src/app.rs` (`provide_address`, `navigate`, `use_address`, the unix-socket hand-off) | One `State<Address>` per window. `navigate(addr)` overwrites it. | It is the *current* place only. No stack, so no Back that remembers, no history menu, no per-entry state, no way for a gesture to show the previous screen. |
| Settings `Route` | `bingux-settings/src/routes.rs` | A 40-variant enum with three hand-kept matches (`title`, `address`, `from_address`, plus `render`), a `TITLES` thread-local that remembers the names rows gave, `canonical_address` for old links, `back_target` = nearest parent address. | The route table exists, but as code: adding a page means editing four matches. Back is "go to the parent address", not "go back": opening Wi-Fi network *Home* from the Network status row and pressing Back lands on Wi-Fi, a page the person never saw. Names live in a side table. |
| `NavigationSplitView` | `bingux-ui/src/shell/navigation.rs` | Takes `selected`, `content`, `subpage(title, on_back)`, `list(shown, on_show_list)`. Derives `PageKey = (page index, subpage title, list shown)` and guesses push/pop/fade from two successive keys (`page_motion`); a `trail` of keys tells a step back from a step in; `PageMemory` keeps scroll controllers per key; `Slide` holds the outgoing element. | The key contains a *title*, so two subpages with one title are the same page and a rename is a navigation. The motion is inferred after the fact. The previous screen exists only while a slide runs, so a gesture cannot show it. `on_back` is a closure each caller must write. |
| Page-level plumbing | `bingux-settings/src/pages/*` | `navigator.push(Route::X { name, .. })`, `navigator.pop()` after delete/forget, `ask_uninstall(.., Some(navigator))`. | Pages know how Back works. |
| Popups | `bingux-ui/src/shell/overlays.rs`, `shell/panel.rs` | `Overlays` holds one dialog, one menu, a toast queue. `Panel` draws every floating surface; its shield turns Escape and an outside press into `on_dismiss`. | Escape closes the popup, but Alt+Left, the mouse Back button and a swipe go to the *page* under it. A menu is tied to the window address, so it is dropped when the address moves. |
| File chooser / Files | `bingux-ui/src/files/chooser.rs` (`Nav { history, forward }: State<Vec<Where>>`), `bingux-files-app` (`Address` `bingux-files://home/me/Documents`) | Hand-written back and forward vectors, `up()` by `path.parent()`. | A second, private implementation of back/forward. No history menu. |
| Taxonomy | `bingux-taxonomy` (`app.rs`, `model.rs`: `PageId::from_segments`) | The window address names `category/page`; the taxonomy keeps its own address mapping. | Same hand-written plumbing. |

Things worth keeping: the percent-escaped URI form, the single-instance
hand-off, the slide look (320 ms expo-out, card corners and shadow), the
per-page scroll memory, `use_page_busy`, `PageHead`.

## 2. Options compared

| | A. Stack of routes in a central router | B. Per-page local navigators | C. URI strings only (what exists) |
| - | - | - | - |
| Back remembers where you came from | yes | each navigator, separately | no (parent only) |
| The previous screen can be drawn under a gesture | yes: the stack holds it, the table renders it | only if every page exposes a render hook | no |
| Deep link, search jump, `xdg-open` | needs an address form | needs one per navigator | native |
| Modals dismiss before pages | one stack, one rule | each navigator decides | not modelled |
| Cost to an app | declare a table | wire a navigator per container | declare a `match` |
| Failure mode | the table and the pages drift (fixed by tests: every route resolves and renders) | the app has N histories | Back lies |

**Decision: A with C as its addressing.** The runtime state is a stack of
*locations*; a location is a URI (C); a route table maps URI patterns to pages
(the data). B is rejected: it is what the Files chooser and the old subpage
code do, and it is why Back cannot be uniform.

Entries hold a `Location`, not a "route id + params" pair. The pair is
*derived*: `table.resolve(&location)` returns the route and its typed params.
One value to store, compare, log, hand to another process and put in a menu;
equality is URI equality after canonicalisation.

## 3. The model

### 3.1 Location (RFC 3986)

```
bingux-settings://network/wifi/network/Home%20Net?tab=saved#password
\_______________/ \___________________________/ \________/ \_______/
     scheme              path segments            query     fragment
```

* **Path segments = hierarchy.** Going into a subpage adds a segment. The
  first segment of a `scheme://host/...` form is a segment like any other (the
  authority is not a host here). An empty authority, `file:///home/me`, is a
  *rooted* location; both forms round trip.
* **Query = one-off state that does not change the hierarchy**: a filter, the
  selected tab, a search query, a nonce. Changing only the query never slides.
* **Fragment = an anchor inside the page.** The page scrolls to the row with
  that anchor and flashes it. Search results use it.
* Segments and query values are percent-encoded (`Display`) and decoded
  (`parse`); a segment may hold any text.
* Canonical form, applied by `Location::parse` and every constructor so equal
  places compare equal: scheme lower-case; empty and `.` segments dropped,
  `..` pops (RFC 3986 section 5.2.4); query sorted by key; empty fragment
  dropped. The route table drops declared default query values and rewrites
  legacy aliases (`Settings: wifi/saved` is now `network/wifi/saved`).
* `up()` drops the last segment (and the query and fragment). The table can
  override it for a *virtual parent* (the Advanced page of a profile opened
  from the profile list goes up to the list, not to a path ancestor).

No `url` crate: it is not in the dependency graph, and the parts needed (parse,
canonicalise, percent coding) are 200 lines with round-trip tests.

### 3.2 The route table

```rust
RouteTable::new("bingux-settings")
    .index("Settings", |_| sidebar_list())                       // scheme://  (compact: the list)
    .root("network").title("Network").icon(Icon::Network)        // sidebar page
        .group("Network & devices").keywords("wifi ethernet vpn").page(|_| network::page())
    .route("network/wifi").title("Wi-Fi").page(|_| wifi())
    .route("network/wifi/network/:ssid").title_param("ssid").page(|p| wifi_network(p.str("ssid")))
    .route("display/:index:int").title_with(display_name).up(|p| ...)  // typed param, dynamic title
    .route("applications/:id").query("tab", "info").ephemeral()
```

A route is data: `id` (the pattern text, stable, shown in the inspector),
`pattern` (literal segments, `:name` string params, `:name:int` integer
params, one trailing `*rest`), `kind`, `title`, `icon`, `parent`/`up`
override, `transition`, `ephemeral`, `query` declarations, `keywords`, and the
`page` constructor `fn(&Params) -> Element`.

Kinds:

| Kind | Meaning |
| ---- | ------- |
| `Index` | The app's root location `scheme://`. On a compact window it is the screen that lists the roots; on a wide window it shows the first root. |
| `Root` | A sidebar destination. Listed in the sidebar in declaration order, grouped by `group`. Never has a parent. |
| `Page` | A subpage: pushed over its parent, slides in. (Default.) |
| `Tab` | A sibling view of one page. Replaces its sibling instead of stacking (`network/wifi` to `network/wifi/saved` as tabs would not stack). |
| `Sheet`, `Dialog` | Modal routes: drawn over the entry beneath, which stays visible; Back dismisses them first. |

Per-route options:

* `title`: `Static`, `Param(name)` (a segment of the URI is the title: a Wi-Fi
  name), `Fn(&Params) -> String` (resolved from caches: an app's display name
  from its desktop id). A page that learns a better name later calls
  `use_page_title(name)` and the entry's title updates everywhere (header,
  history menu, window title); a row that already knows the name may pass it
  with `push_titled`. There is no side table of names.
* `ephemeral`: the entry is *replaced*, not stacked, when something else is
  navigated to from it, so Back skips it (an add-VPN form after saving, a
  search results page).
* `query(key, default)`: declares a query key. A value equal to the default is
  dropped when canonicalising. `.query_pushes()` makes a change of that key
  push a history entry; the default is to replace the top entry in place.
* `transition`: `Slide` (default for pages), `Fade` (roots), `None`.

Unknown or invalid URIs resolve to `Resolution::NotFound`, drawn by a built-in
not-found page that offers *Up* (nearest resolvable ancestor) and the app root.
Nothing panics: `navigate("garbage")` shows the not-found page for the
location it could parse, and is ignored when it cannot be parsed at all.

### 3.3 The stack

```
NavModel { back: Vec<Entry>, current: Entry, forward: Vec<Entry>, modals: Vec<Modal>, mode }
Entry    { id, location, title_override }
Modal    { id, kind, title }       // a Panel, a dialog, a menu, a Sheet/Dialog route
```

The stack is `back` followed by `current`. `forward` is kept only when the
table asks for it (`history(Linear)`, the file views); a hierarchical app
(Settings) discards it.

**Mode.** `Hierarchical` (Settings): `navigate(uri)` *synthesises* the stack
from the route table's ancestors, the way Android synthesises a back stack for
a deep link: `settings://network/wifi/network/Home` gives `[index, network,
wifi, network/Home]`. `Linear` (Files, the chooser): `navigate` just pushes;
there are no ancestors to synthesise, Back is the browser's Back, and
`forward` is on.

Operations (all pure, all take the table, all return a `Change`):

| Operation | Effect | Transition |
| --------- | ------ | ---------- |
| `push(loc)` | Append. A row that opens a subpage uses this. Ephemeral current is replaced. | Push (depth up or sibling), Fade (new root), none (query only) |
| `push_titled(loc, title)` | As `push`, with the title the row already knows. | as `push` |
| `pop()` | Drop `current`; the entry below becomes current. Dismisses the top modal instead when there is one. | Pop |
| `pop_to(entry_id)` | Drop everything above that entry **in one step**. | one Pop for any depth |
| `up()` | Go to `table.up(current)`: a `pop` when that entry is below, else a `replace` with it. | Pop |
| `replace(loc)` | Swap `current`. | none, or by depth |
| `switch_root(loc)` | Sidebar choice: the stack becomes `[index, root]`; choosing the root already shown pops to it. | Fade (Pop when already in that root) |
| `reset(loc)` | Drop everything, start at `loc`. | none |
| `navigate(uri)` | Deep link (`xdg-open`, search action, second launch, `--help` link): canonicalise, resolve, apply the mode's rule. | by depth |
| `back()` | *The* Back: top modal if any, else the entry's `pop guard`, else `pop`. Esc, Alt+Left, the mouse Back button, the Back button, the swipe all call it. | |
| `forward()` | Re-enter `forward.pop()`. | Push |

**Transition classification** (pure, `classify(old, new)`): same path, other
query or fragment: `None` and never a slide. New path deeper than old: Push.
Shallower: Pop. Same depth under the same first segment: Push (a sibling).
Different first segment (different root): Fade. This replaces `page_motion`.

**Modal-first Back.** Every floating surface (`Panel`: menus, choices, dialogs,
sheets; declared Sheet/Dialog routes) registers a `Modal` with the router
while it is open, with its `on_dismiss`. `back()` calls the top modal's
dismissal before touching a page. Escape (the panel's shield), Alt+Left, the
Back button, the mouse Back button and the swipe therefore do the same thing.
A modal is not a history entry (no URI, not in the menu), but it is in the
stack snapshot the inspector shows.

**Veto.** A page with unsaved edits calls `use_pop_guard(|| PopGuard::..)`.
`PopGuard::Allow` (default), `Block` (silently stay), `Confirm(Confirmation)`
(the router shows "Discard changes?"; Discard completes the pop). Every
removal of the entry (pop, pop_to over it, up, switch_root, navigate away)
asks, nearest entry first.

**Retained state.** The router keeps, per entry id (an entry keeps its id when
only its query changes) while the entry is on the stack, forward stack or
mid-slide:
the scroll controller, and `use_retained_state(key, init)` UI state (a
selected tab, an expanded group). Returning by Back finds the page as it was
left; a page reached any other way starts fresh. State is dropped when the
last entry for its location leaves.

**History menu.** `NavModel::back_menu()` lists `back` nearest first: title
(and the route icon), `steps`, the target entry. `forward_menu()` likewise.
Choosing one is `pop_to`/`forward_to` with one transition. On a wide window
the `Index` entry is hidden (it is the sidebar). Titles come from the entries,
so the menu, the header, the window title and the breadcrumb agree.

### 3.4 Derived surfaces

| Surface | Derived from |
| ------- | ------------ |
| Back button visible | `can_go_back()`: a visible page entry below the current one. Modals never count: opening a menu does not change the header. `back()` still dismisses the top modal first |
| Header title | `title(current)` |
| Window title | `app title` + `title(current)` |
| Breadcrumb (Files address bar, optional) | `table.ancestors(current)` titles |
| History menu on Back/Forward | `back_menu()`, `forward_menu()` |
| Previous screen under the swipe | `table.render(below)` with its retained scroll |
| Search results jump | `navigate(uri)`; the fragment is the row |
| Inspector "Navigation" | `NavModel::snapshot()` |
| Sidebar selection | the root of `current` |

## 4. Predictive back

`gesture.rs` is a pure state machine (injected clock): `Idle -> Tracking ->
Dragging -> Settling(Commit | Cancel)`.

* Starts on a pointer or touch down in the **edge zone** (x <= 20 px) when
  the window is compact or a finger drives it, and an entry below exists.
* `Tracking` becomes `Dragging` after 8 px horizontal travel; if vertical
  travel dominates first, the gesture is **aborted** and the touch belongs to
  the scroll view (hand-off). A horizontally scrollable region inside the
  page cannot take it: it starts at the edge, the region does not.
* `progress = dx / width` clamped to 0..1. Moving back past the start
  (dx < -4 px) cancels.
* Release: **commit** when progress >= 0.35 or the rightward velocity over the
  last 100 ms is >= 700 px/s (and progress > 3 %); a leftward fling >= 500 px/s
  cancels. Otherwise cancel.
* `Settling` runs the same pose function to 1 (commit, 260 ms expo-out) or to
  0 (cancel, 220 ms with a slight overshoot spring). Grabbing the edge again
  mid-settle continues from the pose on screen: no jump.

The pose (`PredictivePose`, a pure function of drag `d` and commit `k`):

| | at `d = 0` | at `d = 1` | on commit (`k` to 1) |
| - | - | - | - |
| Current scale | 1.00 | 0.90 | 0.90 |
| Current x | 0 | +8 % of width toward the finger | slides off to 105 % |
| Current corner radius | 0 | the window radius | window radius |
| Current shadow | none | soft, 32 px blur | fades with opacity |
| Current opacity | 1 | 1 | 1 to 0.4 over the last 40 % |
| Previous scale | 0.95 | 1.00 | 1.00 |
| Previous dim | 12 % black | 0 | 0 |

The previous screen is `table.render(below)` framed with its retained scroll.
It is built once when the gesture starts and kept as the same `Element`, so
Freya diffs it as unchanged: only the transform, opacity and corner values
change per frame. Page data does not refresh meanwhile (SettingsSmooth's
`task::set_sliding` defers background results while a slide or gesture runs).
A non-gesture pop (Back button, Alt+Left, Esc, mouse Back) on a compact window
plays the same pose as a synthetic fling (`d` 0 to 1 over the first 40 %, `k`
0 to 1 over 280 ms); on a desktop window it keeps the 320 ms slide, which is
the same pop with the pose of a wide window. Reduced motion
(`tokens.motion(ms) == 0`) replaces the pose with a cross-fade and the
settle with no animation.

The root of a compact window is the list of pages (`Index`), so leaving a page
to the list does the same gesture.

## 5. The back button's history menu

`BackButton` (in `shell/navigation.rs`, used by every split view and by
`HeaderBar` hosts that call `nav::back_button`) is an `IconButton` with a
context menu: **right click**, or a **long press** (touch, 450 ms; the button
shows its pressed state meanwhile) opens an anchored menu *under the button*
(`PanelTrigger::Control`, the shared placement) listing `back_menu()`. The
tooltip "Back" stays for hover and is closed when the menu opens. A stack with
one entry below shows one row. The forward button of a `Linear` app does the
same with `forward_menu()`. Rows are text-only unless the route declares an
icon. Choosing a row is one `pop_to`, one transition.

`IconButton::context_menu(items)` is the general primitive (right click or long
press anchors a menu under any icon button): the Files toolbar and any app with
a split button can use it.

## 6. Decisions and trade-offs

* **Locations, not ids.** A "route id + params" pair would need its own
  serialisation for deep links, handoff and logging. The URI is that
  serialisation and the pair is derived.
* **Fragment, not a route kind, for anchors.** A row is not a page: it has no
  title in the stack. A fragment keeps Back honest (it adds no entry).
* **Titles from the route, overridable by the page.** Static and param
  titles cover most pages; dynamic ones (display names) are set by the page
  that knows them. This removes the old `TITLES` side table.
* **Deep links synthesise ancestors; row pushes do not.** `push` records what
  happened (Back returns to what you saw); `navigate` has no history, so it
  builds the canonical one.
* **Files use `file:///...`.** It is the standard URI for a folder, so drag
  and drop (`text/uri-list`), `xdg-open` and every other file manager agree,
  and `bingux-files://` stays accepted as an alias that canonicalises to it.
  View state (sort, icon size) stays in `view_state`: it is per-folder
  memory, not part of the place; only a launch nonce is a query.
* **One table for every file view.** `files::folder_table()` (linear, `file:`
  with `bingux-files:` as an alias and `recent:` as a place of the same
  history, one `*path` route) is used by the chooser, each Files tab's
  `NavModel` (kept in its `TabMemory`) and the Files window's own router, which
  receives forwarded launches. `files::location_of` and `files::place_of`
  convert between a `Where` and its location. The toolbar's Back and Forward
  carry the history menu of that model. Up is a `go` to the parent folder, so
  it is a history entry like any navigation in a file view. Known limit: a
  location is text, so a folder whose name is not valid UTF-8 is recorded
  with a lossy name; Back to it would open a folder that is not there, which
  the view reports as not found.
* **Hidden `Index` on wide windows.** One stack for both size classes keeps a
  window that is resized across the breakpoint consistent.
* **Modal routes are URIs only when worth sharing.** Imperative dialogs
  (`Overlays::show_dialog`) register as modals but have no URI.

## 7. Declaring a new app's routes (15 lines)

```rust
fn main() {
    let table = RouteTable::new("notes")
        .index("Notes", |_| notes_list())
        .root("inbox").title("Inbox").icon(Icon::Mail).page(|_| inbox())
        .root("archive").title("Archive").icon(Icon::Folder).page(|_| archive())
        .route("inbox/:id:int").title_with(note_title).page(|p| note(p.int("id")))
        .route("inbox/:id:int/attachments").title("Attachments").page(|p| attachments(p.int("id")))
        .route("archive/:id:int").title_with(note_title).page(|p| note(p.int("id")));
    Application::new("notes").routes(table).run(|| NavigationSplitView::new("Notes"));
}
// anywhere below:  use_router().push(loc("inbox/7"));  use_router().back();
```

Nothing else: Back, the header title, the swipe, the long-press menu, Alt+Left,
Esc over a dialog, `xdg-open notes://inbox/7/attachments` and the inspector
all work.

## 8. What is not done

Listed at the end of the work, in the hand-off. Anything not in this table
exists.
