# Bingux Files

Files is the file manager of the Bingux desktop. It is a native Wayland window
(Rust, Freya) with the shell's look. Its window is the body of the file chooser
dialog every Bingux app uses (see
[Files and the file chooser](bingux-ui.md#files-and-the-file-chooser)), hosted
in tabs. The list, address bar, search, previews, properties and file
operations are the same code, so a fix to one is a fix to the other.

## Open it

- Search for "Files", or open it from the launcher or the dock.
- From a terminal: `bingux-files [FOLDER | FILE | URI]`.
- From a link or `xdg-open`: `xdg-open /home/me/Documents`. The desktop entry
  lists `inode/directory`; make Files the default with
  `xdg-mime default bingux-files.desktop inode/directory`, or choose it under
  **Settings > Applications > Default applications**.
- Right-click the dock icon for New Tab, Downloads and Recent.

Only one Files window runs. A second launch hands its folder to the running
window, which opens it in a new tab and comes to the front. Opening a file's
address opens the folder that holds it.

| Argument | Opens |
| -------- | ----- |
| none | the home folder on first launch; raises the window if it runs |
| `/srv/data`, `~/Music`, `Documents` | that folder (a relative path is relative to where you ran the command) |
| `file:///home/me/My%20Files` | that folder |
| `recent:///` | the recent files |
| `trash:///`, `trash:///Old%20folder` | the Trash, or a folder in it |
| `bingux-files://home/me` | that folder |

Only the first argument is used; Files opens one folder per launch.

A folder's name is bytes, not text. A name that is not valid UTF-8 (`café` saved
as Latin-1, `caf\xe9`) is a valid argument and opens that exact folder: the
launch address carries its bytes (`file:///home/me/caf%E9`), and so do the
history, the tab and the per-folder state below. Two folders that display alike
(`caf\xe9-\xff` and `caf\xe9-\xfe` both show as `caf�-�`) stay two folders, with
their own sort and icon size, and the cached listing of their parent keeps both.

## The window

```text
+-------------------------------------------------------------+
| +   [ Documents ][ Downloads ]  +                  ≡  _ □ x |  tabs, menu, window buttons
| <  >  ^  ⟳  [ Home > Documents >            ]  +folder  ▦ v |  toolbar
| Places   |  Name            Size    Modified                 |
| Recent   |  Notes                   27 Oct                   |  file view
| Home     |  Budget.xlsx     88 kB   15 Oct                   |
| Drives   |  ...                                              |
+-------------------------------------------------------------+
```

- The **tabs** are the title bar: separate rounded tabs, the selected one with
  the fill of a selected sidebar row, the others quiet until the pointer is
  over them, a New tab button after the last. The bar is the page's colour and
  the file view under it runs square to the window's edges; nothing joins a tab
  to it (`design/tabs.md`).
- The **sidebar** lists Recent, Home, your folders, the Trash (once something
  has been trashed; the icon is faint while it is empty), your bookmarks and
  your drives with free space. Every row has a context menu (right click,
  long press, the Menu key or Shift+F10), in the order of the list's menu:
  open, what changes the place, the path and bookmark, then Properties.

  | Row | Menu |
  | --- | --- |
  | Recent | Open |
  | Home, a folder of yours | Open, Open in new tab, Copy path, Properties |
  | Trash | Open, Open in new tab, Empty Trash (grey while it is empty), Properties |
  | Bookmark | Open, Open in new tab, Rename…, Copy path, Remove from bookmarks, Properties |
  | Computer | Open, Open in new tab, Copy path, Properties |
  | Drive | Open, Open in new tab, Eject and Unmount (or Unmount), Copy path, Add to or Remove from bookmarks, Properties |
  | Drive not mounted | Mount |

  **Properties** is the dialog a folder in the list has, opened for the
  place's folder: a drive shows its free space bar, the Trash shows its item
  count and size, a bookmark is called by its folder's name. Recent and a
  drive that is not mounted have no folder, so no Properties. The keys on a
  focused row are the hints in the menu: Alt+Enter Properties, F2 Rename,
  Delete Remove from bookmarks, Ctrl+Shift+C Copy path, Enter Open.
- The **Trash** (a bin's `files` folder, address `trash:///`) is a special
  view. Its list shows
  **Original Location** and **Deleted** in place of Type and Modified; the
  status line says `Trash · N items`; when it is empty the page says
  **Trash is empty**. The context menu offers **Restore**, **Restore to…**,
  **Delete permanently** (always asks, Cancel is the default), Copy, Open,
  Preview and Properties (original location and deletion date; its Location
  is `trash:///`) and none of Move to Trash, Rename, Compress or Cut. Nothing
  is made there: **New folder** and New file are off (the toolbar button is
  grey and says so) and **Paste into Trash** moves what was copied or cut to
  the Trash (a copy cannot go there). The toolbar has an
  **Empty Trash** button while the Trash is on show; **Empty Trash** (also the
  empty-space menu and the sidebar menu) counts the items and the space first.
  Dragging files onto the Trash row trashes them; dragging items out of the
  Trash restores them into the folder you drop on. Moving to the Trash says
  `Moved 3 items to Trash` and Restore says `Restored 3 items`, each with an
  Undo. The folders the desktop stands on (home, Documents, the Trash itself)
  are refused with a reason, and what a drive cannot take (no Trash there, a
  read-only drive) is explained and offered to delete permanently, never
  deleted quietly. A bin on another drive (`.Trash-$uid`) is listed when you
  open its `files` folder, by its path; the Trash place itself, `trash:///`,
  is the home bin only.
- The **address bar** shows the path as crumbs. It holds a place and nothing
  else: search has its own bar (below).
  Every crumb, chevron and the `…` crumb lights under the pointer and deepens
  when pressed; on a phone the folder name and the pencil do the same. Typing a
  path (Ctrl+L, the empty end of the bar, the pencil) swaps the text into the
  same box: the icon, the start of the text, the height, the radius and, on a
  phone, the button at the end (pencil, then close) stay where they were.
  Home, the Trash and Recent are **named places** (see below): their crumb is
  the icon and the name, never `/home/me` or `.local/share/Trash/files`.
- **Named places.** A few places are known by a name, not by a path. Each has
  one address, one name, one icon and one way to be typed, and every part of
  the app that says where you are (the crumbs, the field, a tab, the window
  title, the Back and Forward menus, Properties) uses the same words.

  | Place | Address (what Copy location gives, and what the field holds) | Name | Typed forms that open it |
  | ----- | ------------------------------------------------------------- | ---- | ------------------------ |
  | Computer | `file:///` | the computer's name (tab and window: Computer) | `/` |
  | Home | `file:///home/me` | Home | `~`, `~/`, the path, `file://` |
  | Recent | `recent:///` | Recent | `recent:`, `recent://`, `recent:///`, `recent` |
  | Trash | `trash:///` (`trash:///Old%20folder` inside it) | Trash | `trash:`, `trash://`, `trash:///`, `Trash`, `~/.local/share/Trash/files` and its `file://` form |

  `Trash` or `Recent` alone open that place unless a folder of that name is
  where you are. Whichever way the Trash is reached (the sidebar, a typed
  `~/.local/share/Trash/files`, `bingux-files trash:///`, a link) it is the one
  place, so the history never holds it twice. In the Trash the trail reads
  `Trash › Old folder › inside` with the names the items had before they were
  deleted (a second `Photos` that the bin kept as `Photos.2` still reads
  `Photos`); the field holds the exact name in the bin
  (`trash:///Photos.2/`) and completes from the bin's folder, hidden names
  included. Up from the Trash goes nowhere (it is a place of its own, not a
  folder of `~/.local/share/Trash`). The tab and window title read `Trash`,
  and the Back and Forward menus say `Trash`. The crumb menu offers Open and
  Copy location (`trash:///…`) and no bookmark. The rules are in
  `bingux_ui::files::named` (`Roots`), tested without a window; another named
  place (a network place, a drive by its label) is one more `Root` there.
- The **view button** switches between a list and a grid; the arrow beside it
  opens sort and hidden-file choices. Text never changes size with zoom; zoom
  changes the icons, and large sizes show thumbnails.
- The **status pill** floats at the bottom right of the list, on an opaque
  surface, and says how many items there are (`95 items`), what is selected
  (`1 of 95 selected (19 kB)`), how a search is going and what an operation is
  doing. It slides to the other corner when the pointer comes near it, and
  never counts hidden files.

Each folder remembers its icon size and sort order, and the window remembers
its size. State lives in `$XDG_STATE_HOME/bingux/files/view.json`; a folder is
remembered under its exact path bytes, so folders that display alike do not share a view.

## Icons

Every icon in the app, in the sidebar, the list, the grid, the address bar and
its menus, Properties, Quick Look, the drag chip and the tabs, comes from one
place (`bingux_ui::files::FileIcon`, see "File icons" in `docs/bingux-ui.md`),
so a folder looks the same wherever it shows.

- A folder's icon is decided by its **path as written**, never by where a link
  leads. `~/Downloads` keeps the Downloads icon when it is a link to
  `/mnt/nas/Downloads`; the share's own path is a plain folder. Home, Desktop,
  Documents, Downloads, Music, Pictures, Videos, Templates, Public, the Trash
  and the root have their own, by the paths in `~/.config/user-dirs.dirs` and
  by their usual names under home (a line that says `$HOME/` only turns the
  configured folder off). Names are case sensitive. Any other folder, such as a
  bookmark called Projects or Notes, has the plain folder in every place.
- A file's icon follows its type, and is a colour icon in the style of the Bingux
  Files app icon: a rounded page with a rounded folded corner in the colour of
  its family (green spreadsheet, orange presentation, red PDF, blue document,
  yellow-brown archive, pink audio, violet video and so on), lit from above (a
  pale sheen on the top, a soft shade at the foot), with a glyph, and from 24 px
  up a short label (`XLS`, `PDF`, `ZIP`). The families are text, source code,
  data (JSON, YAML, TOML), markup (HTML, Markdown, XML), spreadsheet,
  presentation, PDF, document, e-book, image, audio, video, archive, disk image,
  program or script, font, 3D model, database, certificate or key, torrent, and
  a page with a question mark when the type is unknown. The colours follow the
  theme: lighter pages with a deep tone of the family's hue for the ink in the
  dark scheme, deeper pages with white ink in the light one (the yellows stay
  light and saturated and keep a deep ink; every page has a darker rim of its
  own colour that holds its edge on any background).
- Every **folder** icon, the plain folder, the user folders and the Trash,
  is in the **accent colour** of the system, resting, selected or hidden. A
  folder is the app icon's folder: a tab and a back in the accent darkened, a
  sheet of paper standing in it (from 32 px), and a pale front panel (the accent
  lifted) with a thin highlight along its top edge and a lip under it. Its
  glyph (Documents, Downloads, Music, Pictures, Videos, Desktop, Home,
  Templates, Public) is a deep tone of the accent, set into the front panel. The
  Trash is a bin, with its lid closed and faint while it is empty and with
  paper over its rim when it is not. Changing the accent in Settings changes
  every folder.
- A symbolic link has the icon of what it is. Only on a tile of 64 px or more
  (the grid) it gains a small arrow emblem, a quarter of the glyph, ringed by the
  page colour; the list, sidebar, address bar, menus and Properties head leave
  the glyph whole, so a linked `~/Documents` reads as plain as an unlinked one.
  Where a link leads is in the hover card ("Link to ..."), Properties ("Link
  target") and the status pill (one linked item selected).
- A hidden file or folder is faint. The icons are the same colour selected or
  not: the row's own fill marks the selection. A drive or the Recent clock is a
  symbolic glyph, muted, and the accent only when selected.
- A list row has the same padding, icon column and gap at every width. On a
  phone the row is 56 px high with two lines and a **32 px icon** (the last
  list level of the zoom ladder; the desktop opens at 16 px), and the name
  starts the same distance after the icon as on a desktop.

## Preview

Space on a file shows it in a popup over the window (Quick Look); Space or
Escape closes it, and the keyboard goes back to the list. Alt+P or F3 docks
the same preview as a pane beside the list: it follows the selection, the list
keeps the keyboard, and its edge drags it wider. Space always means Quick Look,
with the pane docked or not. A phone-width window has one frame, a sheet.

**The arrows choose the file.** In a popup, in a docked pane and in every
viewer, Left, Right, Up and Down move the selection in the file view (Up and
Down a line in a grid) and the preview follows. They never scroll a document,
seek a film or pan a picture. Enter opens the file, Alt+Enter shows Properties.

A document scrolls with the wheel or the touchpad, Page Up, Page Down, Home
and End. In a pane, click into the preview to give it its own keys (Space and
Shift+Space page down and up, `+` and `-` zoom, K plays a film); Escape, or a
click in the list, gives the keyboard back. The arrows still choose the file
while the pane has the keys.

Every preview is one slim title line (the file's icon, name, "5 of 14", then
Open, Info, dock or pop out, Close) over the content. There is no footer; the
facts of the file are behind Info (I). Only a film, a song and a document add
a bar of their own.

| File | The preview shows |
| ---- | ----------------- |
| Picture | The picture, fitted. Zoom buttons appear while the pointer is on it; `+`, `-`, `0`, `1` zoom, R rotates |
| Text, code, JSON | The first 64 KiB in plain monospace (W wraps lines) |
| Film, song | The first frame or the cover, with a bar: play, time, seek, volume, speed |
| PDF | The whole first page, fitted and centred, with a bar: page, zoom, find |
| Folder, archive, font | Its item count, its listing, a specimen |
| Anything else | Its icon, type and size, and Open |

**Films and songs.** The popup starts playing, with sound, because you asked
for that file. The pane never makes a sound by itself: it shows the first frame
with a play button, and plays when you click it. K plays and pauses (Space in
a pane that has been clicked), Shift with Left or Right seeks 5 seconds, J and
L 10; 0 to 9 jump to a tenth of the film; Shift with Up or Down changes the
volume, M mutes, `[` and `]` change the speed, C cycles the subtitles (a `.srt`
or `.vtt` named like the film is found), F goes full screen. The bar fades out
2.5 seconds after the pointer stops while it plays. The volume you set is kept
for the next file.

**PDF.** A document opens fitted to the page, so the first page is whole and in
the middle. The zoom menu has Fit width, Fit page and 50 % to 400 %; `+`, `-`,
`0`, `1` and `9` zoom (in, out, fit width, 100 %, fit page), Ctrl and the wheel
zoom, Ctrl+F finds (Enter and Shift+Enter step through the matches), F9 shows
the page thumbnails, and dragging over text selects it and Ctrl+C copies it.

Films, songs and PDFs are decoded by a separate program,
`bingux-preview-host` (the `bingux-preview-host` package; GStreamer and
poppler). If it is not installed, if it crashes, or if a file cannot be decoded, the
preview shows the file's card with the reason and Open, and nothing else
changes. A film whose codec is missing says which kind it is; install the
matching GStreamer plugins (`gstreamer1-plugin-libav`, `gstreamer1-plugins-bad-free`).

## Search

Ctrl+F, the search button of the toolbar, or just typing a letter on the list
opens the **search bar** under the toolbar. Escape clears the text; Escape
again closes the bar and gives back the selection you had.

```text
[ Search Documents ............. x ]  [ Subfolders v ]  [ Filters (2) ]
[ Type: Images x ] [ Size over 10 MB x ]  Clear all        14 found in Documents
```

- **Where to look** is the dropdown: *This folder* (the names already on show,
  filtered as you type, nothing read), *Subfolders* (this folder and below),
  *Home*, *Everywhere* (home and the drives you mounted; system folders are
  not searched) and the places of the sidebar (Documents, Pictures, bookmarks).
  The choice stays for the next search of the tab.
- **Filters** opens a panel under its button (a sheet on a phone) with Type
  (folders, documents, images, video, audio, archives, code), Modified (today,
  last 7 or 30 days, this year, a range of dates), Size (small, medium, large,
  a range in MB), Name (contains, starts with, is exactly, a regular
  expression), Sort by (relevance, name, date, size) and switches for hidden
  files, match case and build and dependency folders. The button says how many
  filters are set. Each set filter is also a **token** under the field with its
  own remove button, and *Clear all* when there are two or more. A line under
  the field says how the search is going (`Searching Pictures… 1,204 found`
  with **Stop**, then `1,204 found in Pictures`).
- **Typing filters.** Everything the panel sets can be typed, and a typed
  `key:value` becomes a token when a space ends it: `report "tax return" -draft
  type:document size:>1MB modified:2024 in:Taxes`. The words match the file
  name only (never the content): exact name, starts with, starts a word,
  anywhere, every word in any order, letters in order (`rpt` finds
  `my_report.txt`), and, under the home folder, a near miss (`reprot`). Quotes
  need the words side by side; `-word` leaves a name out; `name:*.tar.gz`,
  `ext:png,jpg`, `type:image`, `size:>10MB` (also `<`, `<=`, `>=`, `=`,
  `1MB..1GB`), `modified:today|yesterday|week|month|year|3d|2024|2024-05-17|>2024-01-01|2024-01-01..2024-02-28`,
  `in:Pictures/2024`, `hidden:yes|no|only`, `case:yes`, `match:starts|exact|regex`
  and `sort:name|date|size|relevance`. A token that cannot be read stays a
  word and the line under the field says what is wrong with it.
- **Results** are in the folder's list with a **Location** column (the folder
  below where the search began, cut in the middle when long, in full in the
  tooltip) and the matched letters marked. They stream in; **rows already on
  show never move** while the search runs, and the list is put in the order
  chosen (best match first by default, then shallower folders) once, when it
  ends. Typing one more letter keeps the old rows until the new search has its
  first results. The search stops at 10,000 results and says so.
- **Recent searches** drop down under the empty field (eight at most, newest
  first, *Clear recent searches* at the end). Up and Down choose, Enter runs.
- **Nothing found** says where it looked and offers the way out: *Remove
  "Type: Images"*, *Search subfolders*, *Search everywhere*.

| Key | Does |
| --- | ---- |
| Ctrl+F | Open the bar, or focus the field |
| Ctrl+Shift+F | Open or close the Filters panel |
| Enter | Open the selected result, or the first |
| Down | Select the first result and move the keyboard to the list |
| Up | The previous result, without leaving the field |
| Escape | In the field: clear the text; with none, close the search. From the Subfolders or Filters button, or the list, close the search. A scope list that is open closes first |

What it looks in: hidden files only when Show hidden files is on or `hidden:` /
the Hidden files switch say so; build and dependency folders (`node_modules`,
`target`, `.git` and seven more) unless that switch is off; never into a
symbolic link, never onto another filesystem from the folder it started in. A
file dialog's file type filter still applies. File **contents** are not
searched and saved searches are not offered. The design, the numbers and what
is not on `bingux-searchd` are in `design/files-search.md`.

## Tabs

A tab keeps its own folder and its Back and Forward history, and for each
folder it has been in, how far the list was scrolled and what was selected; it
also keeps its sort, icon size, list or grid, hidden-file switch, preview pane
and the path or search being typed. A tab that is not on show holds no watch,
so many tabs cost little; its last listing stays in a shared memory of the 16
folders (60,000 entries) shown most recently, so when you come back it is whole
in its first frame and the folder is checked against the disk behind it. Two
tabs on one folder each keep their own place, and Back, Forward and the parent
folder return to the place the list was left at. Scrolling is never reset by a
thumbnail arriving, the folder changing on disk, a refresh, a selection or a
zoom step (a zoom keeps the content under the pointer where it was); only a new
search that changes the rows goes back to the top.

### Switching tabs does not move the window

There is one body for all the tabs, mounted once (`bingux_ui::files::browse`
with each tab's `TabMemory`). Showing another tab changes which states the body
reads, in the pass that renders it, and the file view (a view per tab and
folder). Nothing is written to a state while the new tab renders and nothing
around the list is mounted again, so:

- the toolbar, the crumb bar (its elision is laid out for the width it had),
  the places sidebar and the tab strip are the same in every frame;
- the first frame of a tab that was shown before has its rows, its scroll
  position, its selection, its zoom and view mode, its column header (widths
  from the saved view state) and its docked pane, because the view starts from
  the size the tab's last view measured and the scroll view builds the rows of
  that size at once (a scroll view otherwise knows its size only after its first
  layout, and draws one row and a scrollbar in the frame before);
- a new tab on the folder that is on show (Ctrl+T) is whole in its first frame;
  one on a folder not seen yet shows the column header and the status line of
  the loaded view with grey rows in the real rows' height (after 120 ms, so a
  folder that opens at once never shows them), and the rows come into that;
- the loader never publishes less than what is shown: with a listing on show it
  waits for the cache's entries or the whole folder, so rows do not vanish and
  return.

`scripts/files-shift.py` records the real window while Ctrl+T, Ctrl+Tab, a click
on a tab, Ctrl+W and a middle click run, and reports every persistent element
(toolbar, crumb bar, sidebar rows, column header, status pill, rules) that moves
in any frame; `cargo test --lib --features testing files::tab_tests` does the
same pass by pass in the headless runner.

| Do | Result |
| -- | ------ |
| Ctrl+T, or the **+** button | New tab at the folder on show |
| Ctrl+W, middle click on a tab, or its **x** (every tab has one, a lone tab too) | Close the tab; the tab on its right is shown, else the one on its left. The last tab closes the window, and asks first only if a copy, move or compression is still running |
| Ctrl+Shift+T, or **Reopen closed tab** | Bring back the tab closed last, at its folder and its place, with its Back and Forward history (the last 10 are kept) |
| Right click on a tab | Close tab, Close other tabs, Close tabs to the left (greyed out on the first tab), Close tabs to the right, Duplicate tab, Reopen closed tab, Move left, Move right. Reopen brings a closed group back left to right at its places |
| Wheel over the tab bar (vertical or sideways), two fingers on a touchpad, or a finger dragged along the tabs | Scroll the tabs when they do not all fit; the **+** and the tab list button stay put. The tab list button (a menu of every tab with its icon, the open one ticked) is there when they overflow. A title too long for its tab fades out at its end instead of ending in an ellipsis |
| Close several tabs one after another with the **x** | The tabs keep their width while the pointer stays in the tab bar, so the next **x** is under the pointer; they grow back when it leaves |
| Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Page Down, Ctrl+Page Up | Next and previous tab |
| Alt+1 to Alt+8, Alt+9 | Go to that tab, or the last |
| Drag a tab down or up past the strip by a strip's height (a lone tab: drag it) | Tear it off: its window follows the pointer, and the other tabs go to a window of their own. Letting go over another Files window's tab strip adds the tab there and closes the carried window; elsewhere the window stays. Let go beyond the strip with the button already up: the tab opens a window. What each compositor allows is in the table below |
| Middle click a folder, or **Open in new tab** in a folder's or a place's menu | Open it in a new tab |

### Tearing a tab off, by compositor

| Compositor | Tear off, window follows the pointer | Drop on another window's strip |
| ---------- | ------------------------------------ | ------------------------------ |
| sway | Yes, for a floating window (a tiled window ignores the move and stays in a window of its own) | Yes, with a marker between the tabs |
| Gnoblin | Yes | Yes. The drop lands where the pointer is. The marker is set; whether it shows is not yet confirmed (see below) |
| GNOME Shell, KDE Plasma, any other | Yes (the same window request; not tried) | **No.** Files has no supported way to learn where the other windows are (GNOME Shell refuses its window list to ordinary programs), so there is no marker and no drop onto a strip. Letting go leaves the window where it is |

On Gnoblin the marker used to be set but not seen, because the faded carried window was not
translucent. Freya cleared a transparent window's canvas to opaque white before drawing, so the
window's 0.4 opacity mixed with white and not with what lies under it. `crates/ui/src/app.rs`
now clears a transparent window to transparent too. This has not been run on Gnoblin yet.

Gnoblin's control socket is asked for the window list (`windows.list`, API
1.37), so Files must reach it: `GNOBLIN_COMPOSITOR_SOCKET`, or the default
socket under `$XDG_RUNTIME_DIR/gnoblin/`. When Gnoblin's Lua supervisor is not
up, or the socket cannot be reached, a tab can still be torn off but not
dropped on a strip. How Files finds out which compositor it is under, and why
nothing is asked of the others, is in `design/tab-tearoff.md`.


## History

Every tab has its own history, like a browser's: the folders it has shown, in
order, and where in that list it is. The window has none: Back and Forward
always mean the tab on show. The rules are in `bingux_ui::files::history`, the
only code that changes a history, and they are tested without a window.

| What happens | History |
| ------------ | ------- |
| Open a folder (double click, Enter, a crumb, a sidebar place, a bookmark, a typed path, Alt+Home, Up, Alt+Up or Backspace) | A new entry after the current one; Forward is forgotten |
| Go to the folder already shown (also written with a trailing slash, `.`, `..` or `//`) | Nothing: no entry, Forward stays |
| Back, Forward, a choice from the Back or Forward menu | Only moves along the list; choosing a row several entries away is one move |
| Refresh (F5, Ctrl+R), a search (Ctrl+F, typing), Escape | Nothing |
| A typed path that is not there, is a file or cannot be entered | The address bar says why and stays: no entry. A file is opened with its program |
| The tab's first folder | The only entry. Starting at a folder never adds it twice |
| The recent files, the Trash and the root folder `/` | Places like any other, each with its entry. The Trash is `trash:///` however it was reached, and its rows say `Trash` (and the name an item had before it was deleted), not `files`; Up from it goes nowhere |

- A folder is its **path as it was named**: `..` and repeated or trailing
  slashes are cleaned by name and a symbolic link is *not* followed, so a link
  is an entry under the link's name, and going Back to it opens the link.
  Files has no redirects. The address of a folder in another scheme
  (`bingux-files://`, `file://`) is the same folder and the same entry.
- Only a place can be an entry: an absolute folder path or the recent files.
  Text that is not one (a relative path, an empty path, a command line option)
  is dropped before it reaches a history.
- A history keeps the last 100 folders behind the one shown (`HISTORY_LIMIT`);
  the oldest is forgotten first.
- **What an entry remembers: nothing of its own.** State is kept per folder,
  not per entry, so a folder shown twice in a history is the same place both
  times. For each tab, the last 32 folders it left keep how far they were
  scrolled and what was selected, and coming back by any route (Back, Forward,
  a menu row, Up, or opening it again) puts both back; two tabs on one folder
  keep their own. A folder the tab has not left before opens at the top with
  nothing selected (except the file a command line named). The sort and the
  icon size are the folder's own and are the same in every tab and the next
  session. The hidden-file switch, the preview pane and the path or search
  being typed belong to the tab; going to another folder ends the search.
- **A folder that is gone.** Its entry stays, because the drive may come back.
  Choosing it opens the "Folder not found" page, which says why (also when a
  cached listing of the folder had been on show); the toolbar's Back, Forward
  and Up still work (Up goes by the name alone). Known gap: the error page has
  no focused row, so Alt+Left, Alt+Up and Backspace do not reach the view from
  it; click the toolbar. Nothing is skipped silently.
- **The menus.** A right click or a 450 ms hold on Back or Forward lists the
  folders that button reaches, nearest first, as plain text: the folder's name,
  *Home*, *Computer* for `/`, *Recent*. Folders that share a name carry their
  parent (`Documents — kieran`, `Documents — work`). A click goes to that
  folder, crossing the entries between; a middle click opens it in a new tab
  and leaves this tab's history alone.
- **Tabs.** A new tab (Ctrl+T, **+**) starts a history of its own at its
  folder. **Duplicate tab** copies the history, and the copy then goes its own
  way. **Reopen closed tab** brings back the closed tab with its Back and
  Forward lists as they were. Tearing a tab off into another window, or onto
  another window's strip, moves the history with it: the same lists, the same
  entries. Two tabs never share one. Switching tabs changes nothing in any
  history.

## Launching

```
bingux-files [--] [FOLDER | FILE | URI]...
```

Each argument opens in **a new tab** of the running Files window (like a
browser handing a link to the window that is open), and starts the window when
none is running; with several arguments the first starts the window and the
rest open as tabs. The tab shown is the last one opened, and the window is
raised. (Nautilus opens a window for each launch; Files follows a browser: one
window, a tab each, and a tab can be torn off.) A launch is not a move: it
never adds an entry to the history of a tab that is already there, and each
new tab starts with the one folder it was asked for.

| Argument | Opens |
| -------- | ----- |
| none | A new window on the home folder, or the running window is raised |
| `PATH`, `./rel`, `rel`, `PATH/` | That folder; a relative path is relative to where the command was run, not to the running Files |
| `~`, `~/Music` | A folder under the home folder |
| `FILE` | The folder that holds it, with the file selected |
| a symbolic link | The link, under its own name |
| `file:///PATH`, `file://localhost/PATH` | The folder (`%20` and the like decoded) |
| `bingux-files://PATH` | The folder; `bingux-files://` is the home folder |
| `recent:///` | The recent files |
| `trash:///`, `trash:`, `trash:///Old%20folder` | The Trash, or a folder in it (the same place as its real path, `~/.local/share/Trash/files`) |
| `--help`, `-h` | Prints the usage and exits |
| `--` | Ends the options: `bingux-files -- -odd-folder` |
| any other `-option` (such as `--snapshot` in a build without it) | `bingux-files: unknown option '--snapshot'` and the usage on stderr, status 2. Nothing is opened |
| a path that is not there or cannot be read, a scheme other than the above (`smb://`, `https://`), a `file://` host that is another computer | The reason on stderr (`bingux-files: /x: No such file or directory`); that argument opens nothing, and the others still do. A first launch whose only argument failed opens the home folder |

An option is never taken for a folder, so a mistyped flag can never become an
entry in a history. The same rules hold for Settings and every other
`Application`, through `bingux_ui::app`.

## Keyboard

| Keys | Action |
| ---- | ------ |
| Alt+Left, Alt+Right, Alt+Up, Alt+Home | Back, Forward, parent folder, home |
| Backspace | Parent folder |
| Ctrl+L, or type `/` or `~` | Edit the path |
| Ctrl+F, or just start typing a name | Open the search bar (see Search) |
| Ctrl+Shift+F | Open or close the search Filters panel |
| F5, Ctrl+R | Read the folder again |
| Escape | Stop reading the folder while the toolbar shows Stop (see Reload and Stop); in a search, clear the text, then end the search |
| Ctrl+H | Show or hide hidden files |
| Ctrl+Shift+N | New folder |
| F2 | Rename |
| Ctrl+C, Ctrl+X, Ctrl+V | Copy, cut, paste |
| Delete | Move to the Trash (inside the Trash: delete permanently, after a question) |
| Shift+Delete | Delete permanently, after a question that names the files |
| Ctrl+Z | Undo the last move, rename, trash, restore or create |
| Ctrl+A | Select all |
| Space | Preview the file (Quick Look) |
| Enter | Open the folder, or open the file in its default application |
| Alt+Enter, Ctrl+I | Properties |
| Ctrl+mouse wheel, Ctrl+plus, Ctrl+minus | Zoom the icons |
| Shift+F10, Menu | Context menu |

## Reload and Stop

The toolbar's fourth button is Reload (`Refresh (F5)`) and becomes Stop
(`Stop (Esc)`) while the folder is being read: the first load of a folder, the
check of a listing shown from the cache, a read that F5 or the button asked
for, and a search of folders. A change that the watcher noticed is read
quietly and does not turn the button into Stop.

- **One button.** It is the same `IconButton` all the time, 16 px icon in the
  same box (nothing moves), with the same hover, pressed and focus state. The
  icon morphs: the reload arrows turn a quarter turn clockwise while they shrink
  and fade, as the stop square turns in from the other side, grows and fades up;
  coming back is the same motion in reverse. It takes `MOVE_MS` (160 ms, from the
  motion tokens), and none at all under reduced motion. A flip in the middle of
  the motion goes back from where the icon is, never snapping. The tooltip and
  the accessible name are `Refresh (F5)` or `Stop (Esc)` (`Stop searching (Esc)`
  while only a search runs).
- **No flicker.** Stop shows only after the read has gone on for 150 ms, and
  once shown it stays for at least 300 ms (`Dwell::LOADING`), so a folder that
  opens at once never shows it and one that ends just after it shows does not
  take it away mid-motion. In that last moment nothing is reading, so pressing it
  does nothing.
- **What Stop does.** It stops the read in flight (`Reload::stop`, which asks
  the loader to give up the read and ignore its late answers) and keeps the
  listing as it is:

  | Read stopped | Listing |
  | ---- | ---- |
  | A re-read of a folder on show | Untouched; nothing is taken away |
  | A first load that had read some entries | Those entries stay, marked stopped (`Listing::stopped`), and the status pill says "Stopped · 12,000 items read so far"; F5 reads the rest |
  | A first load that had read nothing | The page says "Loading stopped" and offers F5 |
  | A search of folders | The search stops and what it found stays, with the line "stopped" |

  The folder is still followed, and F5 or Ctrl+R reads it again whatever the
  button shows. Escape does the same as pressing Stop, but only while Stop shows,
  so an Escape pressed in the first moments of a quick load still closes a
  dialog; in a search Escape clears the text, then ends the search (which also
  stops it).

## Properties

Alt+Enter or Ctrl+I opens Properties for the selection, or for the folder on
show when nothing is selected. It has tabs under the header:

| Tab | What it shows |
| --- | ------------- |
| Basic | Picture or icon, name, type, size with the byte count, location (press it to open the folder), the drive with a free space bar, the times, and for a file the applications that open it |
| Permissions | Who may read, write and run it, as a grid you can change if you own the file, the numeric mode, "Allow executing as program", owner and group; for a folder, **Apply to enclosed items**, which asks before it changes anything |
| Details | A picture's size and camera, a recording's length and codecs, the SHA-256 (press **Calculate**), inode, device, links, disk usage and the extended attributes |
| Contents | For a folder: how many items and how big, counted live with a **Stop** button, and the five largest items |

Several selected items show Basic with their combined count and size, and
Permissions, which change all of them together; a box where they differ shows
a dash. Ctrl+1 to Ctrl+4 pick a tab, Ctrl+Tab and Ctrl+Shift+Tab step through
them, Left and Right move along the tab bar, and Escape closes the dialog. The
tab you used last opens next time. The dialog stays one size on every tab and
scrolls inside; on a phone the tab bar scrolls sideways.

Everything slow happens off the window: counting a folder, the checksum and
Apply to enclosed items each run on their own thread, show how far they have
got, can be stopped, and stop when the dialog closes. What a permission change
did is read back from the disk, so a box that could not be changed goes back
and says why.

## Opening files

A file opens in the application the system names for its type (`xdg-open`).
**Open with** in the context menu lists the applications whose desktop entries
accept the file's type. A file that cannot be opened says why under the list.

## Moving and copying by dragging

Drag files onto a folder in the list, a place in the sidebar or a tab: they
move there. Hold Ctrl to copy. The target under the pointer is highlighted.
Dragging uses the same operations as cut and paste, so a move between drives
copies, checks and then removes the source, and nothing is half-written.

Dragging files **to or from other applications** is not supported: Freya and
its winit version have no drag-and-drop source or target on Wayland.

## Drives

The sidebar lists every mounted drive with a usage bar, and each USB stick,
card and disc that is plugged in and not mounted yet, marked "Not mounted".
Open one to mount it (through `udisksctl`, with your own rights) and enter it.
A drive's menu has **Unmount** or **Eject**, which write everything out before
they detach the drive and say when it is safe to remove.

Copies, moves and deletes on removable media are durable: the data is on the
device before the operation reports that it is done (see
[Moving, copying and removing safely](bingux-ui.md#files-and-the-file-chooser)).
Partitions of the machine's own disks are not listed, and encrypted volumes
are not unlocked.

## Install

`make` builds `bingux-files` with the other programs, and
`scripts/install-shell.py` installs the program, its desktop entry and its
icon next to Settings. The RPM package lists them. To run a build from the
repository:

```sh
cargo run --manifest-path apps/files/Cargo.toml -- ~/Documents
```

## Not done

- Dragging files to and from other applications (see above).
- `org.freedesktop.FileManager1` (`ShowFolders`, `ShowItems`,
  `ShowItemProperties`) is not served, because Bingux has no D-Bus client
  library in its offline dependency set. A program that calls it, such as a
  browser's "Show in folder", falls back to `xdg-open` on the folder.
- Connect to server and network browsing. (A delete still goes to the Trash,
  and Ctrl+Z puts it back.)
- Opening a second window from a menu. A second launch reuses a window.
  Windows come from tearing a tab off (below).
- Named places other than Computer, Home, Recent and the Trash (a network
  place, a drive by its label). A bin on another drive keeps its path
  (`bingux-files /mnt/x/.Trash-1000/files`) and is not `trash:///`. The
  chevron after the Trash crumb lists the items by their names in the bin.
- Restoring the tabs of the last session.


## Compress and Extract

**Compress…** (Ctrl+E, the item's menu, or the empty-space menu opened with a Ctrl or Shift right click while items stay selected) opens a form dialog titled `Compress "name"` or `Compress 3 items`. The card is one size (520 x 680 px; a full-window sheet at 390 x 800) and its body scrolls. Cancel is Escape, Create is Enter and is disabled with the reason in the footer while the form is invalid.

**Archive** (the only mode for a mixed selection):

| Option | Notes |
| --- | --- |
| Name | Prefilled from the selection. Empty, a slash or a name too long is an inline error. A taken name shows "Keep both" (`Archive 2.zip`) or "Replace" (the old file goes to the Trash) |
| Format | `.zip`, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.tar.bz2`, `.7z`, `.tar`. A format whose tool is missing is listed dimmed with the reason |
| Save in | The same folder, or Choose… (the folder chooser) |
| Level | Fastest, Balanced, Smallest; hidden for `.tar` |
| Password | `.zip` and `.7z`, AES-256 through 7-Zip (zip's own `-P` would put it on the command line). `.7z` can also encrypt the file names. The password reaches 7-Zip on standard input only and is never logged |
| Volumes | `.zip` and `.7z`: 100 MB, 650 MB, 700 MB, 2 GB, 4095 MB |
| Options | Include hidden files, keep links as links, keep owners (tar), leave out comma-separated patterns (`*.tmp, node_modules`; a pattern with `/` matches from the archive root), move the originals to the Trash after the archive is checked (asks first) |

**Reduce size** is the default when everything selected is a picture, a film or a song. Pictures: JPEG, WebP, AVIF, PNG or the same format, quality, longest side, remove EXIF/GPS. Films: H.264, H.265, AV1 or VP9 (only encoders `ffmpeg -encoders` lists), Fastest/Balanced/Smallest or a custom CRF, resolution and frame-rate caps, sound bitrate or none. Songs: Opus, AAC, MP3 or FLAC and a bitrate. The Result group shows `Original 842 MB, estimated 120 MB`: the estimate encodes the first picture, or four seconds of the first film, at the chosen settings off the UI thread. Results are `name (compressed).ext` beside the original; a file that would not shrink is kept ("Already small: kept original"). "Replace the originals" moves each original to the Trash once the new file opened again and checked out (asks first). A kind with no tool shows a disabled row with the reason.

Both modes remember the format, level and options (not the name, a password, "move originals" or "replace original") in `$XDG_CONFIG_HOME/bingux/files/compress.conf`.

**Compress as** has Quick .zip and Quick .tar.gz: default options, no dialog. **Extract Here** unpacks into a new folder beside the archive and replaces nothing; **Extract to…** (Ctrl+Shift+E) asks for the folder, whether to make a folder, what to do when a name is taken (keep both, replace through the Trash, skip) and the password of a locked archive, which is tried before the dialog closes.

A job runs on a worker with a busy line (files or percent; for media, "Reducing 2 of 5: name, 45%") and a cancel button that kills the programs and removes the hidden work folder. When it ends a toast offers **Show** and **Undo** (the made files go to the Trash and what was replaced comes back). Everything is written to a hidden folder beside the result, read back, written out (`fsync`, `syncfs` on removable media) and renamed into place.

Tools: `zip`, `unzip`, `tar` with `gzip`/`xz`/`zstd`/`bzip2`, `7z` (`7zz`, `7za`), `ffmpeg`/`ffprobe`, ImageMagick (`magick`). Names reach them from NUL lists, standard input, list files or after `--`, never in a shell string.
