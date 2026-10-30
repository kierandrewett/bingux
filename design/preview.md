# Previewing files in bingux-ui

Contributor design note. The usage docs are the "Preview" sections of
`docs/files.md` and `docs/bingux-ui.md`; this file says what existed, what was
compared, what was decided and why, and what is deliberately not done. Update it
when the design changes.

The one-sentence version: **a preview is a `PreviewSource` (a path, a URI or
bytes plus a type) handed to one `PreviewHost` component that shows it in a
popup, a docked pane or fullscreen; the host picks a viewer from a registry by
type, the viewer asks a worker (or, for audio, video and PDF, a sandboxed helper
process) for what it shows, and nothing in the chain knows what a file list
is.**

## 1. What exists today (audit)

Read from `c4adcbf`, and looked at in the running chooser
(`file_chooser --features testing -- OUT preview-image,preview-text,preview-document,preview-folder`,
captures kept in `/var/tmp/PreviewKit/cap0`).

| Piece | Where | What it does |
| ----- | ----- | ------------ |
| Kind and text head | `bingux-files/src/preview.rs` | `kind_of(Kind, mime) -> PreviewKind` (image, text, poster, folder, sniff, icon), `read_text` (first 64 KiB, NUL means binary), `neighbour` (arrow-key arithmetic). Takes `Cancel` from `bingux_files::listing`. |
| Header facts | `bingux-files/src/media/*` | `image_info` (PNG/GIF/BMP/WebP/JPEG size, JPEG EXIF), `av_info` (WAV, FLAC, Ogg, MP4, Matroska length and codecs), `clock`. Header parsers, no decoding. Also used by Properties. |
| The card | `bingux-ui/src/files/preview.rs` | `show(shared)` opens a `framed_dialog` 1040x760 around `QuickLook`; keys (`route`), `step` moves the file view's selection; the header is `HeaderBar` with name, "n of N" and Close. |
| The stage | `bingux-ui/src/files/preview_stage.rs` | `Subject` per kind: `PictureStage` (`ImageViewer`, decode at most 2048 px), `TextStage` (64 KiB, 1500 lines, monospace), `PosterStage` (largest thumbnail in `~/.cache/thumbnails`, else the icon), `FolderStage` (live count), `IconStage`. A one-line footer. |
| Properties | `bingux-ui/src/files/properties*.rs` | A separate dialog (Alt+Enter, Ctrl+I, menu). Preview closes first, then it opens. |
| Entry points | `files/view.rs` (Space, `Env::preview`), the context menu | Only the file view. Nothing outside `files::` can preview anything. |

What is wrong with it, as a person using it:

1. **It is a modal card and nothing else.** The list behind is dimmed and
   dead; to compare ten photos you hold the arrow keys inside a dialog. There is
   no pane that lets the list stay usable, which is how Finder's preview column,
   Nautilus's sidebar and Dolphin's Information panel are used.
2. **Documents and films do not preview at all.** The capture of
   `Quarterly report.pdf` is a large grey "PDF" icon and "No thumbnail has been
   made for this file". The same for every film and song. There is no player
   and no page view; only a thumbnail the desktop may never have made.
3. **The picture is a fixed-fit image.** No zoom, no pan, no rotate, no actual
   size, no EXIF, no animation control. A 320x200 picture is stretched to the
   card width and blurred (the capture shows it); the white letterbox on a white
   card has no edge.
4. **The header is not a viewer header.** The title sits after an empty 60 px
   slot, the focus ring is permanently on Close (the dialog gives it the
   keyboard), and the only action is Close. No Open, Open with, Info, no file
   type, no mode switch. On a phone the card floats over a dimmed sheet with
   margins and the title centres, while on a desktop it left-aligns.
5. **The position counter is wrong for a viewer.** "4 of 46" counts every row,
   including folders and files that cannot be previewed, and the chooser's own
   "1 of 46 selected" toast is drawn over the card's corner.
6. **Information is one grey sentence** ("PDF document, 2.4 MB, Modified 25
   Oct"). Properties is a different dialog, so seeing a photo's camera means
   closing the preview and opening another window.
7. **Architecture:** the decision (`kind_of`), the loading (`preview_stage.rs`)
   and the chrome (`preview.rs`) live in the file view's module, take an `Entry`
   (a list row), and call private helpers of the view (`Shared`, `Ctx`).
   A wallpaper picker or a mail attachment cannot reuse any of it.

## 2. Conventions compared

| | Header | Content | Controls | Close | Next / previous |
| - | ------ | ------- | -------- | ----- | --------------- |
| macOS Quick Look | name, centred, Open with, Share | content first, no chrome until pointer moves; video has a floating bar | floating media bar, page thumbnails for PDF, rotate and markup for images | Space or Esc | arrows follow the Finder selection; Space toggles; fullscreen button |
| Finder preview pane | none (pane header is the file) | thumbnail large, then "Information" list | a few quick actions below (Rotate, Markup) | View menu | follows the selection live |
| GNOME Sushi | name and type, window title bar | content only | floating bar for video; page buttons for PDF | Space or Esc | follows Nautilus selection |
| Nautilus preview | n/a (Properties sidebar elsewhere) | | | | |
| KDE Dolphin Information panel / Gwenview | panel title is the file | thumbnail, metadata table, comment/rating | Gwenview: bottom thumbnail strip, zoom slider, fit/100% buttons | panel is a toggle (F11) | arrows; panel follows selection |
| Windows Explorer preview pane | none | content in the pane, resizable by its edge | none (content's own controls) | Alt+P toggles | follows selection |
| Google Photos / Drive viewer | scrim-free; top bar with name, actions, close at the left | content first | bottom bar for video; side arrows | Esc, X | arrows, side chevrons |

What the table settles, as rules for Bingux:

- **One header, one order.** Close at the trailing end (every Bingux header),
  the name takes the room, then position, then actions: Open, Open with, Share
  (copy path), Info, mode switch. Actions are icon buttons with tooltips; Open
  with is a split of Open. The header is the standard `HeaderBar`, not a
  private bar. No window drag (it is a card, not a window).
- **Content first.** The viewer owns every pixel below the header. Chrome that
  a viewer needs (media controls, zoom, page jump) floats over the content and
  hides after 2.5 s without pointer movement while playing, never while
  paused, never while a button has the keyboard.
- **Selection is the source of truth in the pane.** The pane follows the file
  view's selection live (Finder, Dolphin, Explorer, Nautilus). The popup also
  follows it (Quick Look, Sushi), so one mechanism serves both.
- **Space.** In the file list Space is Quick Look: the popup, whether or not a
  pane is docked (it used to open "the last mode used" and, with the pane open,
  hid the pane, so after one Alt+P Space never showed a popup again). Inside the
  popup Space and Esc close it, for every viewer including the player, as in
  Quick Look and Sushi; the player is paused with K, a click or its button. In a
  pane the list keeps the keyboard; after the person clicks into the pane it
  holds the pane's own keys, and then Space plays and pauses a film and pages a
  document (Shift+Space back). Esc, or a click in the list, gives the keyboard
  back (it does not close the pane; Alt+P or the header does).
- **Esc.** Leaves fullscreen, else moves the keyboard from the pane to the
  list, else closes the popup. With the pane closed Esc is the file view's.
- **Arrows choose the file. Always.** Left, Right, Up and Down (Up and Down a
  line in a grid), with or without Alt, step the file view's selection in the
  popup, in the pane and in every viewer; the preview follows. No viewer owns an
  arrow: they never scroll a document, seek a film or pan a picture, and
  `keys::route` has no viewer parameter, so one cannot be given. (The first
  design let a document keep Up and Down and a player Left and Right, with Alt
  as the escape; in use it meant a PDF or film swallowed the arrows the moment
  it had focus, and there was no way to reach the next file. Quick Look, Sushi
  and every file manager's preview agree: the arrows are the file's.) A document
  scrolls with the wheel or touchpad, Page Up, Page Down, Home, End; a film seeks
  with Shift+Left and Shift+Right (5 s), J and L (10 s), the volume is
  Shift+Up and Shift+Down. A pane steps the selection itself while it holds the
  keyboard (the list's own key handler is not focused then), through the same
  `on_step` the popup uses.
- **Other keys.** `F` fullscreen, `I` info panel, `R` and `Shift+R` rotate,
  `W` text wrap, `M` mute, `[` and `]` playback speed, `Alt+Enter` the
  Properties dialog (kept), `Enter` Open, `Alt+P` or `F3` the docked pane from
  the file view.
- **Modes.** Popup (modal card, over a scrim, Quick Look), pane (docked, not
  modal, slides out of the right edge, resizable by its edge, remembers its
  width), fullscreen (the whole window, the popup's content with the chrome
  hidden). A header button switches between popup and pane (dock and undock
  glyph); fullscreen is `F` or the player's button. In a compact window
  (phone, below 600 px) there is only one mode: a full-window sheet; the pane
  needs a width and a list beside it.
- **Information.** The Info panel is part of the viewer chrome: it slides in from
  the right inside the card or pane (a tabbed Details list, Properties as its
  last row's "More…" which opens the existing dialog), so camera, codec and
  dimensions do not need a second window. `Alt+Enter` keeps opening the full
  Properties dialog.

## 3. Presentation

One content component, three frames. `PreviewHost` takes a `PreviewSource`, a
`Mode` and a `Follow` (what to do with arrows) and renders `Viewer` inside:

| Mode | Frame | Keyboard | List |
| ---- | ----- | -------- | ---- |
| `Popup` | `framed_dialog` card over the scrim, 1040x760 cut to the window | the viewer owns it | dead (modal) |
| `Pane` | a docked column at the file view's right edge, between the list and the window edge, inside the view's scroll container's sibling slot | stays in the list | live, selection changes the pane |
| `Fullscreen` | `framed_dialog` at the whole window, header and footer hidden until the pointer moves | the viewer owns it | dead |
| `Sheet` (compact) | the popup at the whole window, standard header, Back closes | the viewer owns it | dead |

The three frames render the same `Viewer`; a headless test renders each at
1100x820 and 390x800 and compares the content tree.

**Pane state machine** (`pane_state`, pure, tested): `Closed -> Opening(t) ->
Open -> Closing(t) -> Closed`, with `width` in logical px. Opening and closing
tween the pane's width from 0 with `use_tween` (the standard quadratic ease
out, `MOVE_MS`), so the list resizes with it and no content jumps; reduced
motion gives `ms == 0` and the change is instant. The viewer inside is laid out
at its *final* width during the slide, clipped, so nothing re-flows per frame
(the list beside it does; that is one resize). Dragging the left edge sets the
width, clamped to `[280, 60 % of the view]`, saved through `view_state` key
`preview.pane` when the drag ends (not per frame), restored at open, and
shrunk (not forgotten) when the window is too narrow. Reversing mid-slide
continues from the shown width. A window below 600 px closes the pane and
future opens use the sheet.

**Selection following** is a `Subject` derived from the view's selection
(`Follow::Selection`), debounced by one frame, and the viewer is keyed by the
source's identity: stepping to another file is a new `Viewer`, so every load
starts fresh and the old one is cancelled (§ 6), but the *frame* around it
(header, pane) is not remounted, so nothing flickers. Multi-selection shows a
summary card ("3 files, 4.2 MB") rather than the cursor row only.

## 4. Architecture

### 4.1 Options for the split

| | A. Everything in bingux-ui | B. `bingux-preview-core` (no Freya) + UI in bingux-ui | C. core + a `bingux-preview` UI crate on top of bingux-ui |
| - | ------------------------- | ---------------------------------------------------- | -------------------------------------------------------- |
| Callers outside the file manager | yes | yes | yes, but bingux-ui cannot call it |
| bingux-ui's chooser can open it | yes | yes | **no**: the chooser is in bingux-ui, bingux-preview would depend on bingux-ui (cycle). It needs inversion (a registry of boxed components in bingux-ui) |
| Decode, sniff, protocol testable without Freya | no | yes | yes |
| Compile time of UI-less tools (a thumbnailer, `binguxctl`) | pulls Freya | light | light |
| Cost | one more module of a big crate | one more small crate | an inversion layer for a use nobody has yet |

**Decision: B.** The viewers are bingux-ui components (they use `HeaderBar`,
`icon_button`, tokens, `Overlays`, motion, `use_resource`), and the chooser
inside bingux-ui must open them, so they live in `bingux_ui::preview`. Everything
that can be separated without Freya is `bingux-preview-core`:

```
bingux-files (model: mime, Cancel, thumbs; libc only)
   ^
bingux-preview-core  (PreviewSource, kind sniffing, Previewer trait + Registry,
   ^                  text/archive/font/folder/image facts, media::* header readers,
   |                  host protocol (wire + client), player state machine, cache)
   |            \
bingux-ui        bingux-preview-host (binary: GStreamer + poppler-glib, GPL libraries
 (preview::*)       stay out of the UI process; depends on core for the protocol)
   ^
bingux-files-app, bingux-settings, any app
```

`bingux-preview-core` depends on `bingux-files` only for `mime` and `Cancel`
(both pure) and `libc` for `memfd`, `mmap`, `recvmsg`. `bingux-files` stops
owning `preview.rs` and `media/*`: they move to the core (clean cutover; the
file view and Properties import them from there, `bingux-files::preview`
disappears, `neighbour`/`Step` move to `bingux_ui::files`, where list rows are
the subject).

### 4.2 The input

```rust
pub enum PreviewSource {
    Path(PathBuf),                       // a file or folder on disk
    Uri(String),                         // file:// today; other schemes resolve through gio later
    Bytes { name: String, mime: String, data: Arc<[u8]> },   // a mail attachment
}
```

`PreviewSource::identity()` is the key (path + mtime + size, or a hash of the
bytes). Nothing in it is a file-list row. The file view builds `Path` sources;
the Settings wallpaper picker will build `Path` too; a mail view builds
`Bytes`.

### 4.3 Previewers and the registry

```rust
pub trait Previewer: Send + Sync {
    fn id(&self) -> ViewerId;
    /// How well this viewer handles `sniffed`, or `None`.
    fn claim(&self, sniffed: &Sniffed) -> Option<Priority>;
    /// What the viewer needs from disk, read off the UI thread, cancellable.
    fn load(&self, source: &PreviewSource, cancel: &Cancel) -> Result<Loaded, LoadError>;
}
```

`Sniffed` is `kind` (from `sniff`: the first 512 bytes, then the name's
extension: magic wins over a wrong extension, extension decides inside a
container family such as `.docx` being a zip) plus the `mime::Type`. `Registry`
holds previewers ordered by `Priority`; `chain(&sniffed)` returns the **fallback
chain**, best first: e.g. a PDF is `[Document, Card]`, an MP4 is `[Media, Card]`,
a `.svg` is `[Image, Text, Card]`, an unknown file is `[Text if it sniffs as
text, Card]`. A viewer that fails (`LoadError`, decoder missing, helper died) makes
the host move to the next one and say why in the card's note ("The player could
not open this file. Opening it in another app may work."). Previewers return
data models (`Loaded::Text`, `Image`, `Folder`, `Archive`, `Font`, `Facts`);
Freya components in `bingux_ui::preview::viewers` render them. Heavy pixels
(video frames, PDF pages) never travel inside `Loaded`: they arrive through the
host protocol (§ 5.4).

| Viewer | Handles | Loaded by | Notes |
| ------ | ------- | --------- | ----- |
| `Image` | PNG, JPEG, GIF, WebP, BMP, ICO, AVIF/HEIF where Skia has the codec, SVG | Freya `ImageViewer`/`GifViewer`/`SvgViewer` decode on their own pool; core reads EXIF and size | zoom, pan, rotate, fit and 100 %, EXIF panel, animated GIF. Animated WebP shows its first frame (§ 8) |
| `Media` | audio, video | the host (`OpenMedia`) | player, § 5 |
| `Document` | PDF; others via the host's poppler for the formats poppler opens (PDF, PS-less) | the host (`OpenDocument`) | page view, thumbnails, search, copy. Office documents: LibreOffice is installed here but not a dependency, so they fall to `Card` plus the thumbnail if one exists |
| `Text` | text, code, markdown, JSON, logs, unknown text | core, windowed | line numbers, wrap, small highlighter, windowed loading |
| `Archive` | zip, tar, tar.gz | core | listing without extracting (hand-written central directory reader; `tar` and `flate2` from the lock) |
| `Font` | ttf, otf | core reads names; the UI draws the specimen with the font bytes registered under a private name | |
| `Folder` | directories, mounts | core (the existing live counter) | |
| `Card` | everything else, and every failure | core facts | icon from the single file icon API, facts, Open with |

### 4.4 Viewer chrome

`bingux_ui::preview` exports: `PreviewSource`, `Mode`, `PreviewHost` (the
frame and the router between viewers), `open(overlays, source, options)`
(popup) and `pane(source, options)` (an element to dock), `ViewerChrome`
(header and Info panel, shared), `ImageViewer2` (named `ImageView` in code),
`MediaControls`, `DocumentView`, `TextView`, `ArchiveView`, `FolderView`,
`FactsCard`. The file view, the chooser and the Files app only ever touch
`PreviewHost`, `open` and `pane`.

## 5. Decoders

### 5.1 Evidence from this machine (Fedora 43)

| Need | Installed | Dev files | Crate offline? | Crate online? |
| ---- | --------- | --------- | -------------- | ------------- |
| Audio and video playback | GStreamer 1.26.11 with base, good, bad, ugly, libav, openh264, dav1d, vaapi, pipewire sink; ffmpeg 7.1.5, mpv 0.40 (`libmpv`) | `gstreamer1-devel`, `gstreamer1-plugins-base-devel` (pkg-config `gstreamer-app`, `gstreamer-video` present). **No** `ffmpeg-devel`, **no** `mpv-devel` | no `gstreamer*` in `~/.cargo/registry`; `symphonia` 0.5.5 (audio only) is | crates.io reachable; `gstreamer`, `gstreamer-app`, `gstreamer-video` 0.24.5 fetch and **build here** (probe in `/var/tmp/PreviewKit/scratch`, 28 s) |
| PDF | poppler 25.07, `poppler-glib`, `poppler-utils`; mupdf 1.27 libs and `mutool`; evince-djvu | `poppler-devel`, `poppler-glib-devel`, `cairo-devel` present; **no** `mupdf-devel` | `lopdf` (parser only), `pdf-inspector`; nothing that renders | `poppler-rs` 0.25 renders, extracts text and finds text: probe rendered page 1 of a 5-page PDF and returned its text and 1 search hit |
| SVG | librsvg | `librsvg2-devel` | `resvg`, `usvg` | Skia already has an SVG module through Freya |
| Images | Skia in Freya (PNG, JPEG, WebP, GIF, AVIF through its codecs), libheif, LibRaw, libwebp | `libwebp-devel` only | `image`, `png`, `gif`, `zune-jpeg` | — |
| Highlighting | — | — | `syntect` 5.3, tree-sitter grammars | both available; see 5.3 |
| Archives | `bsdtar`, `unzip` | — | `tar`, `flate2`, `miniz_oxide` already in the lock | — |

Licences. Every Bingux crate is `GPL-3.0-or-later` (checked in the manifests).
GStreamer core and plugin-base: LGPL-2.1+; `gstreamer-libav`: LGPL; the
Fedora ffmpeg here: GPLv3+ (RPM Fusion build); poppler-glib: GPL-2.0-only OR
GPL-3.0-only (so usable by a GPL-3.0 program); mupdf: **AGPL-3.0**; mpv-libs:
GPL-2.0+ AND LGPL-2.1+. The Rust bindings (`gstreamer`, `poppler-rs`,
`cairo-rs`) are MIT or MIT/Apache. None of them is incompatible with
GPL-3.0-or-later. mupdf's AGPL would extend a network-use clause to anything
linking it and is the one library avoided on licence alone.

### 5.2 Options compared

Media playback:

| | In-process GStreamer | ffmpeg-next | libmpv render API | Helper process decodes into shared memory | Wayland subsurface from a player |
| - | -------------------- | ----------- | ----------------- | ------------------------------------------ | ---------------------------------- |
| Builds here | yes | **no** (no `ffmpeg-devel`) | **no** (no `mpv-devel`) | yes (GStreamer in the helper) | yes (mpv exists) |
| Audio, A/V sync, seeking, subtitles, buffering | `playbin3` does all | write all of it | mpv does all | `playbin3` in the helper | mpv |
| A bad file can crash the file manager | **yes** (a codec bug is a segfault in the UI process) | yes | yes | no | no |
| Frames reach Freya | `appsink` copy to Skia | copy | GL render context shared with Skia (Freya owns the GL context; not exposed) | memfd frames, mapped and uploaded as `SkImage` | not at all: the compositor composes it. No clip to a rounded card, no overlay controls, no scrim, no screenshot |
| Licence of the UI binary | LGPL libraries linked in | GPL (this ffmpeg) | GPL | UI binary stays free of them | n/a |
| Cost | small | medium and blocked | medium and blocked | a protocol and a second binary | a window per preview |

**Decision: helper process.** The helper `bingux-preview-host` links GStreamer
and poppler; the UI process never does. A malformed file kills the helper, the
viewer shows "The player stopped" with Open with, and the next file starts a new
helper. It costs a copy per frame (viewport-sized, scaled in the helper, so a
4K film is 1280x720 on a pane) and a protocol. It earns: crash isolation,
licence isolation (the GPL libraries are a separate program, which is also what
the RPM's `bingux-preview-host` sub-package makes optional), no `Requires`
in the main UI package, and the same transport for PDF pages.

PDF:

| | poppler (glib) in the helper | mupdf | pdfium | pure Rust (lopdf, hayro, pdf-rs) |
| - | --------------------------- | ----- | ------ | ------------------------------- |
| Available here | yes, dev files too | libs, no dev files | not packaged | parsers only offline; hayro/pdf-rs would be new dependencies and unproven here |
| Licence | GPL-2/3, fine in a GPL-3 helper | AGPL | BSD, but a binary blob to ship | MIT/Apache |
| Text, search, links, page labels | all | all | all | text only, or nothing |
| Render quality and speed | reference quality, Cairo | fastest | good | not measured, large gaps in fonts and colour |
| Cost | `poppler-rs` crate, ~25 lines per call | needs headers | needs a packaging story | a renderer to write |

**Decision: poppler-glib through `poppler-rs`, in the helper.** The cairo image
surface becomes a memfd frame exactly like a video frame. Out-of-process also
covers poppler's CVE history: a malicious PDF takes the helper, not the file
manager.

### 5.3 Smaller decisions

- **Text highlighting:** a small in-tree line lexer (comments, strings, numbers,
  keywords per language family for Rust, C-likes, Python, JS/TS, shell, JSON,
  TOML, YAML, Markdown, QML, CSS, HTML). `syntect` brings a 5 MB default
  syntax dump (measured size is in § 9) and a regex engine to run on a 64 KiB
  window the lexer handles in microseconds; tree-sitter needs a grammar crate
  per language. The lexer is a fallback-safe design: unknown language means no
  colour, never an error. A true highlighter can replace the lexer behind
  `Highlighter` later.
- **Images in the UI process:** Skia's codecs already decode and cache; EXIF
  and header facts are parsed by the core without decoding. HEIF/RAW formats Skia
  lacks fall to the `Card` with the desktop thumbnail.
- **Animated WebP / APNG:** not decoded; first frame. GIF animates (`GifViewer`).
- **Office documents:** `Card` with the desktop thumbnail. LibreOffice exists on
  this machine, but a headless conversion takes seconds and gigabytes and is not
  a dependency. The `Previewer` for them is a registry entry that can call a
  converter later.

### 5.4 The host protocol (`bingux-preview-host` v1)

A strict, small, versioned binary protocol on a `socketpair(AF_UNIX,
SOCK_SEQPACKET)`, created by the client before `exec`; the child has the other end
as fd 3. `SEQPACKET` keeps message boundaries, so there is no framing code, and
carries `SCM_RIGHTS` file descriptors. No JSON, no serde.

Every message: `u8 tag`, `u8 version (1)`, `u16 reserved (0)`, `u32 session`,
`u32 request`, then the tag's fields, little-endian, strings as `u16 length` +
UTF-8. Maximum message 4 KiB; a larger or unknown message is a protocol error and
ends the session. Decoding is a total function (`decode(&[u8]) -> Result<Message,
ProtocolError>`) with a round-trip and a fuzz-style test (random bytes never
panic).

| Tag | Dir | Fields | fds |
| --- | --- | ------ | --- |
| `Hello` | C to H | `version` | |
| `Ready` | H to C | `version`, `features` (bit 0 media, bit 1 document) | |
| `OpenMedia` | C to H | `want_video`, `max_w`, `max_h`, `autoplay`, `has_subtitle_file` | media (O_RDONLY), subtitle |
| `OpenDocument` | C to H | | document (O_RDONLY) |
| `Opened` | H to C | `kind`, `duration_ns`, `video_w`, `video_h`, `has_audio`, `seekable`, `pages`, `title` | |
| `Play`, `Pause`, `Seek{ns}`, `SetVolume{f32}`, `SetMute{bool}`, `SetRate{f32}`, `SetSize{max_w,max_h}`, `SetSubtitle{index or -1}` | C to H | | |
| `FrameReady` | H to C | `slot`, `width`, `height`, `stride`, `pts_ns`, `format` (BGRA premul = 0) | memfd on first use of a slot |
| `FrameDone{slot}` | C to H | | |
| `State` | H to C | `playing`, `buffering_percent`, `ended` | |
| `Position` | H to C | `position_ns`, `buffered_to_ns` (every 250 ms while playing, on seek) | |
| `Tracks` | H to C | counts and names of audio and subtitle tracks | |
| `Cue` | H to C | `text`, `until_ns` (empty text clears) | |
| `RenderPage` | C to H | `page`, `scale_milli`, `max_w`, `max_h` | |
| `PageReady` | H to C | `page`, `width`, `height`, `stride` | memfd |
| `PageText`, `PageTextIs` | C to H, H to C | `page`; words as `rect + str`, as many as fit 4 KiB per message, continued | |
| `Search`, `SearchHit` | C to H, H to C | `query`; `page`, `rect` | |
| `Cancel{request}` | C to H | | |
| `Error` | H to C | `code` (not found, unsupported, corrupt, no decoder, killed), `message` for a person | |
| `Close` | C to H | | |

**Frames.** The host allocates up to three `memfd`s per session, each
`max_w * max_h * 4` bytes, sealed against shrinking, and sends each over
`SCM_RIGHTS` the first time it is used. `FrameReady{slot}` tells the client a
frame is in that slot; the host never writes a slot until `FrameDone{slot}`.
With no free slot the host drops the frame (video keeps its clock; the
picture skips). `SetSize` makes the host scale with `videoscale` and allocate
a new set. The client maps a slot read-only, copies into the `Bytes` of an
`ImageHandle` (one memcpy per frame), and replies `FrameDone`. Pages use the
same mechanism with one memfd per `PageReady`, closed after the copy.

**Host lifecycle.** Spawned by the UI (`bingux_ui::preview`, with
`runtime::die_with_parent`), one per previewed media or document, started
lazily when the first audio, video or PDF viewer mounts and stopped when the
viewer unmounts (its `Close`, then EOF). The host sets `PR_SET_PDEATHSIG`,
`PR_SET_NO_NEW_PRIVS`, `RLIMIT_AS` 2 GiB, `RLIMIT_CPU` 120 s for a document
(none for playback, which ends by `Close`), `RLIMIT_NOFILE` 64, clears
the environment of everything but `XDG_RUNTIME_DIR` (PipeWire), `HOME`-free
`GST_*`, `LANG`; opens only the descriptors it was given (`/proc/self/fd/N`),
never a path; ends on EOF of its socket. A watchdog in the client treats no
reply for 10 s to `Open*` as `Error(killed)`. The file is opened by the
**client**, so the helper never needs to be told a path and cannot be pointed
at another. It does not use seccomp (GStreamer plugins and PipeWire need too
wide a set to filter meaningfully without a maintained policy); it does not
get a network namespace unless the kernel allows an unprivileged one, which is
tried and the result is not relied on.

## 6. Security and performance

- **Untrusted files.** No decoder runs in the UI process except Skia's image
  codecs (already the case) and the core's header parsers, which read at most
  `INDEX_LIMIT` (16 MiB) and fail with `None`. Audio, video and documents run
  in the helper. Archives list at most 5000 entries and read at most 8 MiB of
  headers (a tar.gz inflates at most 64 MiB).
- **Size caps.** Text reads a window (64 KiB, 4 MiB when scrolled, a file of
  any size opens at once); images over 100 MP are refused to a card; PDFs over
  500 pages render lazily; a frame never exceeds 8192x8192.
- **Timeouts and cancellation.** Every `load` takes a `Cancel` that stops at
  its next read; stepping to another file cancels the previous viewer's
  loads (the viewer is a new component, its resources drop) and `Close`s its
  helper session (the helper process is reused for the next file within 2 s
  so arrowing through a folder of films does not spawn per file).
- **Off the UI thread.** Every disk read and every decoder step is on
  `task::spawn_blocking` or in the helper; the only work on the UI thread is
  mapping a finished frame, copying it, and building an `ImageHandle`.
- **Caches.** Under `$XDG_CACHE_HOME/bingux/preview/`: rendered PDF first pages
  and first video frames as PNG (named like thumbnails, by source identity),
  capped at 128 MiB total with least-recently-used eviction at startup and
  every 64 writes. A cache is read through `thumbs::lookup` first. Nothing is
  cached for `Bytes` sources and nothing is cached when `XDG_CACHE_HOME` is
  unwritable.
- **No network.** No viewer opens a URL; `Uri` sources other than `file://` are
  refused until a mounted-gvfs path exists. The helper is given no network
  access by construction (only descriptors).

## 7. Phases

1. **Foundation and first viewers:** the core crate (source, sniff, registry,
   text, archive, font, folder, image facts, moved `media/*`), the host and
   chrome in bingux-ui (popup, pane, fullscreen/sheet, mode switch, Info panel),
   image, text, folder, archive, font and card viewers, file view/chooser/Files
   wiring.
2. **Media and documents:** `bingux-preview-host`, the protocol, the player and
   the PDF viewer.
3. **Packaging, docs, taxonomy, tests and real-input proof.**

## 7a. Revision after use: less, whole, and the arrows

Kieran's review of the first build: "too much stuff going on", the PDF zoom
"broken", the menu "behind the preview", and the arrow keys scrolling the PDF
instead of choosing the next file. Each was reproduced with the installed Files
app in a headless session, and each had a cause:

| Seen | Cause | Fix |
| ---- | ----- | --- |
| The zoom menu (and any menu opened from the popup) opened behind the card; its right edge showed beside the card and the rest was invisible, so a zoom could not be chosen | The window's menu layer (`OverlayLevel(8)`) was below the dialog layer (`OverlayLevel(12)`), and the preview card is a dialog | `OverlayStack::menu_layer(over_dialog)`: above an open dialog the menu is on level 14. Pixel test: a menu over a loud-red dialog has no red in its panel |
| Down and Up scrolled the PDF in Quick Look; after a click in the docked PDF every arrow scrolled it | `keys::route` gave Up and Down to a document and Left and Right to a player; the PDF's own handler scrolled on the arrows, and a clicked pane took the keyboard from the list | No viewer takes an arrow; a pane that holds the keyboard steps the selection itself (`Keyboard`, `PreviewNav::pane_keys`) |
| Space never showed the popup after one use of Alt+P | Space opened "the last mode used" and, with the pane open, hid it | Space is always Quick Look |
| The PDF opened as fit width: a tall page showed only its top | `Zoom::FitWidth` was the default | The default is fit page: the whole first page, centred (a stack shorter than the viewport is centred vertically too) |
| The zoom buttons moved when the label changed from "Fit width" to "150%" | The label sat in a menu button that sized to its text | A fixed slot as wide as "Fit width" |
| A pane had no zoom label at all | The compact toolbar dropped the menu | One toolbar everywhere: page field, zoom out, menu, zoom in, find |
| The big play button sat at the top of a paused popup or pane | An absolute box with `Size::fill()` sized to its content's top, not to its parent | Overlays are sized from the measured box |
| The phone's seek bar shrank to nothing | The bar picked its form from a width the caller had to pass | The bar measures its own room once laid out |

The chrome that was removed: the footer line (it repeated the type and size
that Info shows), the Info-less fullscreen button in the header (F), the text
viewer's language bar and "Wrap lines" checkbox (a line appears only when the
preview stops early), the picture's two rotate buttons and its always-on zoom
pill (the pill shows under the pointer or when zoomed), the PDF toolbar's
thumbnails and its separate compact form, and the player's full screen
button. Syntax colour in the text preview went too: a preview is for reading
what a file is. A title line has at most four buttons (Open, Info, dock or pop
out, Close); a viewer adds its bar only if it is a player or a document.

Chrome budget (controls on screen at rest, popup, `preview::behaviour_tests`):
picture 4 (the header's), text 4, folder 4, card 5, document 9 (4 + page field,
zoom out, menu, zoom in, find), song 10.

## 8. What is not done, and open questions

- Animated WebP and APNG (first frame only); HEIF/RAW without a Skia codec.
- Office documents beyond the desktop's thumbnail.
- Wayland-native hardware-decoded video (zero copy dmabuf into Skia): the
  helper's frames are CPU frames. A VA-API decode in the helper already
  happens; the copy is after it.
- Text selection and copy in the PDF viewer needs word rectangles per page
  (`PageText` exists); drag selection inside a rendered page is the part with
  the most input handling and is tracked here, not hidden.
- Subtitles come from a sidecar `.srt`/`.vtt` named like the film and from
  tracks `playbin3` reports; no styling.
- `Uri` sources other than `file://`.
- Should `Pane` follow the *cursor* row or the *selected* set in a multi-select?
  Decided: the cursor row, with a summary card above two selected.
