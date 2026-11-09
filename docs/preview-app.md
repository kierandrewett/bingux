# Preview

Preview shows pictures and PDF documents in a window, with the other files of the same kind from the same folder one arrow key away.

```sh
bingux-preview photo.jpg          # the photo, among the pictures of its folder
bingux-preview ~/Pictures         # every picture and document in the folder
bingux-preview a.png b.pdf        # exactly these files
```

With no argument an empty window opens. Drop files on it, or choose Open file. A second launch shows its file in the window that is already running.

## What it opens

| Kind                                   | Formats                                                                                                                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pictures drawn by the application      | PNG, JPEG, WebP, GIF (animated), BMP, ICO, SVG                                                                                                            |
| Pictures decoded by the preview helper | HEIF and HEIC, AVIF, TIFF, Radiance HDR, OpenEXR, and the full-size JPEG inside camera RAW files (CR2, CR3, NEF, ARW, DNG, ORF, RW2, RAF, PEF and others) |
| Documents                              | PDF, shown with poppler in the preview helper                                                                                                             |
| Anything else                          | Text, archives, fonts, songs and films open in the same viewers Quick Look uses                                                                           |

High dynamic range pictures (floating point TIFF, EXR, HDR, and 10-bit PQ or HLG photos) are tone mapped to an ordinary display. A photograph stored on its side by its EXIF orientation is shown upright. Transparent pictures sit on a light chequerboard. Camera RAW files are not developed: the camera's own preview is shown, and a file without one says so.

`bingux-preview-host` must be installed beside the application for the helper formats and PDFs. It links GStreamer, poppler and libheif so that a damaged file takes the helper and not the window.

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

The page field in the toolbar goes to a page number. The sidebar is not offered in a phone-width window.

## Other commands

The main menu has Open with (the applications whose desktop entries name the file's type), Show in Files, the picture commands, full screen, Move to Trash, keyboard shortcuts and About.

## Not done yet

- Markup (annotating, signing, page edits) is not in this version.
- Dropping files from another application needs the compositor to deliver drag and drop; the Open button always works.
- Copy picture and Save a copy are not offered for SVG, HEIF and camera files.
