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

- The **sidebar** lists Recent, Home, your folders, the Trash (once something
  has been trashed; the icon is faint while it is empty), your bookmarks and
  your drives with free space. Every row has a context menu; the Trash row's
  has Open, Empty Trash and Properties.
- The **Trash** (a bin's `files` folder) is a special view. Its list shows
  **Original Location** and **Deleted** in place of Type and Modified; the
  status line says `Trash · N items`; when it is empty the page says
  **Trash is empty**. The context menu offers **Restore**, **Restore to…**,
  **Delete permanently** (always asks, Cancel is the default), Copy, Open,
  Preview and Properties (original location and deletion date) and none of
  Move to Trash, Rename, Compress, Cut, Paste or New folder. **Empty Trash**
  (empty-space menu, sidebar menu) counts the items and the space first.
  Dragging files onto the Trash row trashes them; dragging items out of the
  Trash restores them into the folder you drop on. Moving to the Trash says
  `Moved 3 items to Trash` and Restore says `Restored 3 items`, each with an
  Undo. The folders the desktop stands on (home, Documents, the Trash itself)
  are refused with a reason, and what a drive cannot take (no Trash there, a
  read-only drive) is explained and offered to delete permanently, never
  deleted quietly. A bin on another drive (`.Trash-$uid`) is listed when you
  open its `files` folder; the Trash place itself lists the home bin only.
- The **address bar** shows the path as crumbs. It is also the search box.
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
- A file's icon follows its type: image, audio, video, text, code, PDF,
  archive, spreadsheet, presentation, document, font, program, or a blank page
  when the type is unknown.
- A symbolic link has the icon of what it is and a small arrow badge in the
  list, grid, Properties and Quick Look. The sidebar and the address bar do not
  read the disk, so they show no badge.
- A hidden file or folder is faint. An icon is in the accent colour only on the
  selected row or tile, and muted otherwise.

## Tabs

A tab keeps its own folder and its Back and Forward history, and for each
folder it has been in, how far the list was scrolled and what was selected. A
tab that is not on show holds no listing and no watch, so many tabs cost
little; when you come back to it, it lists its folder again (from the cache,
so at once) and the list is where you left it, with the same selection. Two
tabs on one folder each keep their own place, and Back, Forward and the parent
folder return to the place the list was left at. Scrolling is never reset by a
thumbnail arriving, the folder changing on disk, a refresh, a selection or a
zoom step (a zoom keeps the content under the pointer where it was); only a new
search that changes the rows goes back to the top.

| Do | Result |
| -- | ------ |
| Ctrl+T, or the **+** button | New tab at the folder on show |
| Ctrl+W, middle click on a tab, or its **x** | Close the tab (the last tab closes the window) |
| Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Page Down, Ctrl+Page Up | Next and previous tab |
| Alt+1 to Alt+8, Alt+9 | Go to that tab, or the last |
| Middle click a folder, or **Open in new tab** in a folder's or a place's menu | Open it in a new tab |

## Keyboard

| Keys | Action |
| ---- | ------ |
| Alt+Left, Alt+Right, Alt+Up, Alt+Home | Back, Forward, parent folder, home |
| Backspace | Parent folder |
| Ctrl+L, or type `/` or `~` | Edit the path |
| Ctrl+F, or just start typing a name | Search the folder; Ctrl+Enter searches the subfolders |
| F5, Ctrl+R | Read the folder again |
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
cargo run --manifest-path packages/bingux-files-app/Cargo.toml -- ~/Documents
```

## Not done

- Dragging files to and from other applications (see above).
- `org.freedesktop.FileManager1` (`ShowFolders`, `ShowItems`,
  `ShowItemProperties`) is not served, because Bingux has no D-Bus client
  library in its offline dependency set. A program that calls it, such as a
  browser's "Show in folder", falls back to `xdg-open` on the folder.
- Connect to server and network browsing. (A delete still goes to the Trash,
  and Ctrl+Z puts it back.)
- Several windows. Open another folder in a tab; a second launch always
  reuses the one window.
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
