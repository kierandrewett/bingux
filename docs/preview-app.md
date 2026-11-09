# Preview

Preview shows a file in a window of its own: a picture, a PDF, text or code, an archive, a font, a song or a film, or a folder. The other files of the same kind from the same folder are one arrow key away.

Preview is the dedicated way to look at a file. Files has a quick look (Space, or a pane docked beside the list) that is the same component with less around it; Preview gives every viewer its tools. The component, its frames and how to add a viewer are in `docs/bingux-ui.md` ("File preview").

```sh
bingux-preview photo.jpg          # the photo, among the pictures of its folder
bingux-preview main.rs            # the source file, among the text and code of its folder
bingux-preview ~/Downloads        # every file in the folder that Preview can show
bingux-preview a.png b.pdf        # exactly these files
```

With no argument an empty window opens. Drop files on it, or choose Open file. A second launch shows its file in the window that is already running.

## What the arrows reach

| You open                | Left and Right go through                                                                                             |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------- |
| one file                | the files of its folder in the same family, in the order Files lists them (natural order, hidden files left out)      |
| one folder              | the files in it that Preview can show, of every family; a folder with none is shown as a folder, with Open in Files   |
| several files or folders | exactly those, in the order given; a folder among them is shown as a folder                                          |

The families are pictures (and camera RAW files), PDF documents, text and code, archives, songs and films, and fonts. A file that no viewer is named for (a word processor document, a disk image) is shown alone, as a card with its facts.

Why the family and not every file in the folder: the arrows should step between files that are read the same way. A photo, a film, a font and an archive in a row would change the whole window and its toolbar on every press; the slideshow needs only pictures; and the size you chose for text, or the fit you chose for a photograph, carries over to the next file of its kind. To see everything in order, open the folder.

The Open file chooser starts on All files. It also has Pictures and documents and the built-in kinds (Images, Audio, Video, Documents, Archives, Text).

## What it opens

| Kind                                   | Formats                                                                                                                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pictures drawn by the application      | PNG, JPEG, WebP, GIF (animated), BMP, ICO, SVG                                                                                                            |
| Pictures decoded by the preview helper | HEIF and HEIC, AVIF, TIFF, Radiance HDR, OpenEXR, and the full-size JPEG inside camera RAW files (CR2, CR3, NEF, ARW, DNG, ORF, RW2, RAF, PEF and others) |
| Documents                              | PDF, shown with poppler in the preview helper                                                                                                             |
| Text, code, markup and data            | Plain text and logs, Markdown, HTML, CSS, CSV, JSON, XML, YAML, TOML, configuration files, and source code (Rust, Python, JavaScript, TypeScript, C, C++, Java, Go, Lua, Nix, QML, shell and more), in UTF-8, UTF-16 or Windows-1252 |
| Archives                               | ZIP, tar (gzip, xz, bzip2, zstd) and 7-Zip, listed without being unpacked                                                                                 |
| Fonts                                  | TrueType, OpenType and TrueType collections (the first font of a collection)                                                                              |
| Songs and films                        | Whatever GStreamer plays, through the preview helper                                                                                                      |
| Folders                                | How many items and how much space, counted while you look                                                                                                 |
| Anything else                          | A card with the type's icon and the file's facts, and the reason a better viewer could not show it                                                        |

High dynamic range pictures (floating point TIFF, EXR, HDR, and 10-bit PQ or HLG photos) are tone mapped to an ordinary display. A photograph stored on its side by its EXIF orientation is shown upright. Transparent pictures sit on a light chequerboard. Camera RAW files are not developed: the camera's own preview is shown, and a file without one says so.

`bingux-preview-host` must be installed beside the application for the helper formats, PDFs, songs and films. It links GStreamer, poppler and libheif so that a damaged file takes the helper and not the window. Extracting an archive needs `unzip`, `tar` or `7z`, and installing a font runs `fc-cache` when it is there.

The desktop entry lists the MIME type of every kind above, so Open with in Files offers Preview for them. It never sets a default.

## What Preview adds to a quick look

Every viewer takes the same component in two depths. A quick look shows the content and the keys that read it. Preview's window adds the viewer's tools.

| Viewer   | Quick look in Files                                  | Preview                                                                                                                      |
| -------- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Picture  | zoom, turn and flip keys                             | the same, plus Crop, Copy picture, Save a copy, Move to Trash, the slideshow and the turn buttons                            |
| Document | compact toolbar in a pane                            | the full toolbar: page field, zoom, find, thumbnails and outline                                                              |
| Text     | plain monospace, `W` wraps                           | colour for code, markup and data; a View menu (wrap, colour, larger, smaller, default size); Copy; the line count            |
| Archive  | the listing and totals                               | Extract all, which unpacks into a new folder beside the archive and offers Show in Files                                       |
| Font     | the name, three sizes, the characters you can pick   | six sizes from 12 to 72 px, a sample field to type your own text, Copy character, and Install                                  |
| Song, film | compact controls in a pane                         | the full control bar                                                                                                         |
| Folder   | the count                                            | Open in Files                                                                                                                |

## Text

| Input         | Effect                                                                     |
| ------------- | -------------------------------------------------------------------------- |
| `W`           | wrap long lines, or not                                                    |
| `+`, `-`, `0` | larger, smaller, the default text size                                     |
| View menu     | wrap, colour code, text size                                               |
| Copy          | copy the text that is loaded                                               |

The first 64 KiB are read with the viewer; Show more reads the next piece, up to 4 MiB. What you chose (wrap, colour, text size) is kept for the next file.

## Archives

Extract all unpacks the archive into a new folder named after it, beside the archive. Nothing is overwritten (a name that is taken gets "(copy)"), and a failure leaves nothing half unpacked. Locked archives are opened from Files, which asks for the password.

## Fonts

The font itself draws the specimen, not a thumbnail. Type in the sample field to see your own text at every size. Click a character in the grid to see its code point (`U+00E9`); in Preview, Copy character puts it on the clipboard.

Install copies the file into `$XDG_DATA_HOME/fonts` (`~/.local/share/fonts`) and runs `fc-cache`. A file already there with the same name and contents shows Installed; a different file with the same name is kept and the new one is saved as "name (copy)". Programs that are already running may need a restart to see a new font.

## Pictures

| Input                           | Effect                                                                                             |
| ------------------------------- | -------------------------------------------------------------------------------------------------- |
| Left, Right                     | previous and next picture of the folder                                                            |
| `+`, `-`, Ctrl and wheel        | zoom (the wheel zooms about the pointer)                                                           |
| `0`, `1`                        | fit the window, actual size (one picture pixel on one device pixel)                                |
| double click                    | actual size at the pointer, then fit                                                               |
| drag, wheel, Shift and an arrow | pan a zoomed picture; letting go while moving glides on                                            |
| `R`, `Shift+R`                  | turn right, left                                                                                   |
| `H`, `V`                        | flip horizontally, vertically                                                                      |
| `Space`                         | play or pause an animated GIF                                                                      |
| `S`                             | slideshow of the folder, full screen, every 4 seconds; Escape stops it                             |
| `I`                             | information: size, camera, lens, exposure, software, place                                         |
| `F`, `F11`                      | full screen; Escape leaves it                                                                      |
| `Ctrl+C`                        | copy the picture as it looks now                                                                   |
| `Ctrl+Shift+S`                  | save a copy, turned and flipped as it looks now, beside the original                               |
| `Delete`                        | move to the Trash; Undo in the toast or `Ctrl+Z` puts it back                                      |
| `C`                             | crop: drag the box or use the arrows (Shift moves further, Alt resizes), pick a shape, Save a copy |

Preview remembers, for each kind of picture, whether you last wanted it fitted or at actual size, so pixel art stays at actual size and photographs stay fitted. The window size is remembered too. Both are kept in the state file Files uses (`$XDG_STATE_HOME/bingux/files/view.json`).

Pictures open at once: a quick small version shows while the sharp one is decoded, and a picture you zoom into is decoded at the size the screen needs, up to 8192 pixels on a side.

Copy picture needs `wl-copy` (Wayland) or `xclip`. Save a copy and Copy picture work for PNG, JPEG, GIF, WebP, BMP, ICO and TIFF; SVG, HEIF and camera files are only viewed.

## Crop

`C`, the crop button or Crop in the menu shows the picture with a crop box over it (`bingux_ui::crop`). Drag the box or a handle, or use the arrows; the shape list holds Free, Original, 1:1, 4:3, 3:2, 16:9, 5:4 and 21:9, and a button turns a shape on its side. Save a copy writes the cropped picture (turned as it looks) beside the original as "name edited.jpg" and shows it next in the folder; the original is never changed. Undo in the toast or `Ctrl+Z` moves the copy to the Trash and shows the original again. Escape cancels. Crop works for the formats Save a copy writes.

## Documents

| Input                                        | Effect                                                             |
| -------------------------------------------- | ------------------------------------------------------------------ |
| scroll, Page Up, Page Down, Space, Home, End | move through the pages, one continuous column                      |
| `F9` or the sidebar button                   | page thumbnails; Outline when the document has a table of contents |
| `Ctrl+F`                                     | find; Enter and Shift+Enter go to the next and previous match      |
| drag over text, `Ctrl+C`                     | select words and copy them                                         |
| `+`, `-`, `0`, `1`, `9`, Ctrl and wheel      | zoom, fit width, 100 %, fit page                                   |

The page field in the toolbar goes to a page number. In a phone-width window the pages button opens the thumbnails and outline as a sheet over the page.

## Other commands

The main menu has Open with (the applications whose desktop entries name the file's type), Show in Files, the picture commands, full screen, Move to Trash (not for folders), keyboard shortcuts and About. Delete moves a picture to the Trash; the other viewers have keys of their own, so Delete is left to the menu.

## Not done yet

- Markup (annotating, signing, page edits) is not in this version.
- Dropping files from another application needs the compositor to deliver drag and drop; the Open button always works.
- Copy picture and Save a copy are not offered for SVG, HEIF and camera files.
- Word processor, spreadsheet and presentation files, e-books, comics, 3D models, calendars and contacts have no viewer yet; they show their card. Fonts in WOFF and WOFF2 are not read either.
- A font stays registered with the text engine until Preview quits; stepping through very many fonts in one session keeps them in memory.
