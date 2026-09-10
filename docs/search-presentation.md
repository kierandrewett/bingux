# Search presentation

Full-colour app icons in search, launch effects, the dock and its menus,
notifications, and the tray share `OsIconImage` and one `OsIcons` background helper.
GTK resolves the OS theme; GdkPixbuf/librsvg renders SVGs directly into memory.
The helper has an 8 MiB LRU cache keyed by source path, modification time, file
size, and render size. PNG data URIs provide lossless in-memory transport to Qt's
image/texture cache; no converted icon files are written. Live tray/notification
image providers bypass this path because they already supply pixel data.
Raster images pass through unchanged, including legitimate opaque black pixels.
Theme changes refresh the resolver. This avoids QtSvg's black-background rendering
of icons such as GNOME Web without changing OS assets or recolouring application
artwork. Symbolic icons keep their existing tinting path. The helper needs Python
with PyGObject, GTK 3, and GdkPixbuf's librsvg loader; the native package supplies
these through `BINGUX_ICON_HELPER`. A failed helper falls back to native Qt icons.

File and folder results use GIO's full-colour file icons, including custom folder
icons and valid thumbnails. GNOME Desktop generates missing thumbnails in the
background, with two concurrent jobs and a five-second deadline per job. Image
and PDF thumbnails share Nautilus's standard cache; failed previews retain their
file-type icon. The renderer package includes GNOME Desktop for this path.

Search ignores result hover and clicks during its opening and resize animations.
After the animation, a result accepts mouse input only after fresh pointer
movement over it. Pointer coordinates belong to the fixed window, so animation,
model replacement and keyboard scrolling cannot select a stationary cursor's row.

Highlight a supported file and press Right Arrow at the end of the query to open
its preview. Hover alone does not open the pane. The search card keeps its size
and position; the preview uses a matching adjacent surface. On narrow screens,
the preview covers the search card instead of moving it off-screen. Left Arrow or the selected result's Hide control closes the pane. Escape closes search and its preview. Space remains normal
query input. While the pane is open, it follows the selected file.

PDFs, common images and text/code files have previews. Scroll through PDF pages,
drag to pan at larger zoom levels, use Ctrl+wheel or the zoom controls, and use
Fit to return to the viewport width. Pages render on demand in separate helper
processes. Changing files or closing the pane cancels pending work. Text input is capped at 256 KiB; Markdown and HTML use sanitised rich text. Files over 20 MB are refused before rendering, including cache hits. Encrypted,
damaged and unsupported files keep their normal open action. The native package
supplies Poppler, Pillow and the Python helper through `BINGUX_PREVIEW_HELPER`.

The preview is 480 pixels tall and centres images at their fitted size. The
Information table uses aligned labels and values with fine row separators.
It shows file dates, dimensions or page count, and location. Embedded image
metadata adds capture date, resolution, colour space and colour profile when
available. File type and size appear beneath the filename. Show Less collapses
the table. Files without a creation date do not substitute modification time.

The background uses Firefox's original light/dark noise textures; attribution
and MPL 2.0 are in `shell/bingux/preview-assets/`. The toolbar switches between
light and dark backgrounds. The pane slides and fades on opening and closing,
newly rendered content fades in, and zoom uses easing. The chevron rotates and
moves from `Preview >` to `< Hide`. Ctrl+scroll zooms from anywhere in the search
window, including outside the preview. Reduced motion disables these animations.

Checks: `bash tests/search-pointer.sh`, `bash tests/document-preview.sh`,
`python3 tests/os-icons.test.py` and `python3 tests/document-preview.test.py`.

Web activation uses the desktop portal to open the URL and then explicitly
foregrounds the configured HTTPS browser through Quickshell's toplevel API.
The focus handoff runs after the search layer unmaps, so closing the popout
cannot restore the previously focused window over the browser. Browser identity
comes from the OS default and desktop startup class, never a hard-coded browser.
The bounded handoff prefers the window whose title contains the search term.
`qs -p shell/bingux/SearchWebOpenTest.qml --no-color` is an opt-in live test: it
opens a real search and requires its browser window to become active, not merely
a successful spawn or portal response.

Search opens as an input above screen center and expands below it as results arrive.
The 660px-wide surface uses 19px input text; there is no footer. Loading, empty, and failure states appear
inside the input. Entrance, dismissal, resizing, and row colors respect
`BINGUX_REDUCED_MOTION=1`.

The input has a clear button; Ctrl+L selects the query. Arrow keys navigate
results, Tab fills the highlighted result's name into the query without opening
it, Shift+Tab moves backward, and Enter opens the selected item. Moving the mouse updates
the same selection. The input shows a faint selected-name hint without altering
the query: a completion suffix for prefix matches, otherwise a separated name.
It elides to available space and hides during text selection, composition, or
mid-query editing. Keyboard navigation scrolls off-screen rows into view over
150ms; repeated keys retarget from the current position. Reduced motion scrolls
immediately. Moving the mouse updates
selection without a stationary pointer overriding keyboard navigation. Selected
rows are the only highlighted state: there is no separate hover appearance.
Activating a result copies its icon above the card and list, scales it from 1x to
4x and fades it from opacity 1 to 0 over 260ms. Launching is immediate; the copy
finishes independently of the card's dismissal. Reduced motion skips this effect.
Selected
rows show a themed chevron that slides 24px from the left and fades in over 230ms.
Entrances are claimed once per highlighted index in the overlay, so query updates,
delegate replacement, and loading-state changes cannot replay them. Reopening the
popout resets the entrance. A persistent list-level hover handler tracks actual
pointer movement/entry without disappearing when result delegates are replaced.
Overflowing titles and full paths begin a continuous marquee after 500ms of
selection, at 32px/second with a pause between loops. Deselection immediately
resets the text to its shortened, highlighted resting presentation. Result rows
have no tooltips; reduced-motion disables automatic scrolling, while manual path
scrolling remains available. There is still no footer,
divider, icon tile background, or left accent strip.

Tooltip visuals live in `TooltipBubble.qml`, shared by `DockTooltip` and the
anchored `ShellTooltip` wrapper. Callers set `text`, `maximumWidth`, and `wrapText`;
they do not duplicate fonts, padding, colors or borders. Search uses OS symbolic
icons (`edit-clear-symbolic` with a close fallback, and `go-next-symbolic`) rather
than text glyphs for clear and open actions. Search retains the shared tooltip
only for the clear button. `MarqueeText.qml` owns the selection dwell timer and
loop/reset behavior; run `qs -p shell/bingux/SearchMarqueeTest.qml` (also with
`BINGUX_REDUCED_MOTION=1`) for the native timing regression.

Grouping explicitly retains the incoming relevance order using each row's original
index as a tie-breaker: Qt's JavaScript sort cannot be assumed stable. Regression
tests exercise an intentionally unstable sort, not just Node's stable implementation.

Search animates its own card. Configure this Gnoblin window rule in
`~/.config/gnoblin/gnoblin.toml` so the compositor does not also slide the
full-screen layer surface:

```toml
[[window-rules]]
match.layer = "^bingux-search$"
animation = "none"
```

Providers already supply a themed `icon` with each result. Empty or missing theme
icons get a fallback. Icons fill their slot without a background. Results are
grouped under Applications, Files, Web, and provider-specific headings; calculation,
weather, and chat kinds have larger, distinct presentations. Title and subtitle
remain plain text on the wire. The shell escapes them before adding its own
match emphasis, including repeated words and ordered fuzzy matches.

Local sources contribute up to five matches each so one index cannot fill the
entire list. A Web action gets a reserved slot and opens a DuckDuckGo query in
the browser via the configured `commands.fileOpener`. Without the optional
suggestion provider, no network request is made until activation. This opens
browser search results; it does not fetch web snippets into the launcher.

## Optional Web suggestions

`packages/bingux-searchd/web-suggestions.py` implements the external-provider
protocol for DuckDuckGo autocomplete. Register a manifest with ID `web-suggestions`,
lazy startup, priority 0, timeout 2000ms, and a command consisting of Python 3,
the script path, and the desired URL-opener executable. Add its manifest path to
`providerManifestPaths` to opt in; removing it disables remote suggestions.

Enabling it sends eligible typed queries to DuckDuckGo after a 200ms debounce.
Explicit local paths, assistant prompts and local filter queries are skipped.
It returns up to four distinct suggestions below the original Web action, uses
a five-minute in-memory cache capped at 64 queries, bounds response size and
socket timeout, and discards superseded responses. In-flight HTTP requests may
finish after cancellation; they cannot replace the newer query's results. Local
results are streamed independently and do not wait for suggestions.

`open-default.py` opens web URLs through the desktop OpenURI portal and waits for
its response. This lets the default browser launch outside the search daemon's
NoNewPrivileges boundary, including AppImages needing FUSE, without weakening
the daemon. It requires system Python with PyGObject; local paths continue through
xdg-open. Configure it as `commands.fileOpener` and as the suggestion script's
opener so both activation paths use the same default-browser routing.

Applications and calculation are built in. File search needs `fileRoots` in the
runtime search configuration. Weather needs `weather` location settings, AI can be enabled through Bingux Settings with a CLI harness; legacy `ai` endpoint/model/credential-file settings remain compatible, database search needs
`sqliteSources`, and external providers need `providerManifestPaths`.
Disabled or empty sources do not create empty group headings.

File indexing includes configured root folders and file/folder symlink aliases.
Aliases are not recursively followed unless explicitly configured as a root.
Traversal is breadth-first across roots, with at least a quarter of the 20,000-entry
index reserved for folders (up to 15,000 files). The scan remains bounded at
131,072 inspected entries and respects ignore rules. It is not a whole-disk index:
folders outside `fileRoots` need their parent added for fresh-file scanning.
Partial results are published during startup rather than waiting for the full scan.

When the host provides `plocate` and a readable OS locate database, search also
queries that index for paths in the user's home directory and configured roots.
This finds already-indexed folders outside the background scan's entry limit.
Case-insensitive basename retrieval supplies candidates; the complete local query
then filters and ranks them. Helpers share an 80ms timeout budget and return at
most 256 paths per seed. Hidden/build-cache paths and missing entries are excluded.
No shell is invoked. Missing tools/databases fall back to the in-memory index.
The OS database's freshness depends on the host's `updatedb` schedule; Bingux
does not launch a privileged full-system scan. Short/fuzzy-only queries still
rely on the in-memory cache.

Under a hardened service with `NoNewPrivileges`, the OS database may be
unreadable because `plocate` normally gains a dedicated database group. Keep
that protection: run `bash packages/bingux-searchd/refresh-os-index.sh` in the
ordinary desktop session to build a private snapshot from user-visible OS-index
entries. Bingux automatically prefers `$XDG_CACHE_HOME/bingux/locate.db` (default
`~/.cache/bingux/locate.db`). Refresh it after the host index updates to pick up
new entries outside the fresh-file scan roots. This script does not run `updatedb`
or change privileges, and its replacement is atomic.

Path ranking applies a small depth/length penalty (at most 3.5%), with an extra
penalty for hash-like components. Simpler paths win among similar matches, but
an exact match still outranks a weaker match in a short path. This is a heuristic,
not a claim that a path was generated or written by a person.

`packages/bingux-searchd/search-excluded-directories.txt` is the shared exclusion
list for both indexing paths and the snapshot refresh script. It skips dependency,
build-output and cache directories such as `node_modules`, `target`, `build`,
`dist`, `out`, `CMakeFiles`, virtual environments and framework caches, plus object
files/bytecode. Names match whole path components, case-insensitively; `build.rs`,
`src`, `lib`, and “building plans” remain searchable. Use the refresh script's
`--filter-current` option to compact an existing snapshot after exclusions change
without re-enumerating the host database. Runtime filtering takes effect immediately.

## Local search syntax

- `visual code`: require both terms (word order does not matter).
- `"visual studio"`: require a contiguous, case-insensitive phrase.
- `firefox OR chromium`: match either branch. Use uppercase `OR` and `AND`;
  implicit/explicit AND binds more tightly than OR.
- `report -draft` or `report -"old copy"`: exclude literal text, never fuzzy text.
  Exclusions apply to their OR branch: `report -draft OR notes -draft` excludes
  drafts in both branches.
- `report filetype:pdf` or `report ext:pdf`: exact filename extension; `-ext:pdf`
  excludes that extension. A leading dot is optional.

Unfinished quotes match the phrase typed so far; trailing operators are ignored
while typing. Parentheses, wildcards, dates, and `site:` are not local operators.
Web queries are passed unchanged to the engine, which interprets its own syntax.
External providers and configured SQL retrieval queries receive the original query;
their retrieval support is provider-specific. SQL-returned rows also undergo local
matching before display.

The local query is parsed once per request. Indexed names and paths use Unicode
lowercasing. Exact names and word-boundary matches outrank substrings, path-only
matches, and bounded fuzzy matches. Fuzzy terms cannot span arbitrarily distant
letters in a path. Positive match highlights share the lexical rules; syntax and
exclusions are not highlighted or protected during middle-ellipsis shortening.

Files use themed icons chosen from their filename extension, case-insensitively:
PDFs, office documents, spreadsheets, presentations, images, audio, video,
archives, source/configuration files, fonts and disk images have distinct icons.
Files and utility results prefer the OS theme's transparent symbolic icons;
applications and folders retain their OS icons. Unknown types keep the OS generic
file icon. There is no custom PDF artwork.
The background index skips dependency and cache directories such as node_modules.

Web actions use compact 38px single-line rows: engine icon, query, and engine
label. DuckDuckGo uses the OS icon if installed, otherwise its unmodified official
SVG, bundled locally so opening search never fetches an icon over the network.
The asset source is https://duckduckgo.com/assets/logo_header.v109.svg.

## Custom provider layouts

A trusted shell profile may assign `SearchOverlay.providerDelegates`, a map from
provider ID to a QML Component. Unregistered providers use SearchResult.
This is a presentation extension, so the closed v1 socket protocol is unchanged.
Provider processes continue to send data; the profile installs their QML component.

Each component exposes these properties with initial defaults (not required
properties, since the Loader binds them after creation):

- `property var result: ({ title: "", subtitle: "", icon: "" })`
- `property string query: ""`
- `property bool selected: false`
- `property bool activationEnabled: false`
- `signal activated()`
- A bounded `implicitHeight` suitable for a result row.

The shell supplies the available width, accessible name, keyboard navigation and
activation routing. A custom component renders its selection state and emits
`activated()` when clicked, respecting `activationEnabled`. Use Text.PlainText for
provider strings, or the escaping/highlighting helper from SearchResult.
To opt into the launch effect, also expose `activationIcon` (the rendered icon
Item), `activationIconSource` (its image URL), and `symbolic` (whether to tint it
with Theme.muted). The standard result supplies these automatically.

For example, a profile can register a notes component:

```qml
SearchOverlay {
    providerDelegates: ({ "notes": notesResult })
}
Component {
    id: notesResult
    NotesSearchResult {}
}
```

Keep rendering local and bounded. Custom components should not open connections,
start processes, or execute result text.

## Manual verification

The search IPC target supports `open`, `close`, `query <text>`, and `move <delta>`.
For example: `qs ipc --pid <shell-pid> call search query firefox`.
Check an empty query, several app/file matches, a fuzzy query, a calculation,
no matches, selection movement, closing and immediate reopening.


## Settings and explicit AI search

Open **Bingux Settings** from application search, or run `binguxctl settings open`.
Its Search, AI, Previews and Desktop pages save user overrides in
`$XDG_CONFIG_HOME/bingux/settings.json` (normally `~/.config/bingux/settings.json`).
Managed search configuration is left in place. Applying search changes restarts
only `bingux-searchd`; desktop and preview preferences update in the running shell.

AI is disabled by default. Choose **Pi** or **Claude Code**, optionally a model
and executable path. Existing CLI authentication is reused; the settings app does
not read or store credentials. Enter `! your question` in search, then press Enter.
Typing the prompt does not invoke a harness or send it to external search providers.
The older `?` prefix remains compatible. Follow-ups in the open conversation do
not need another prefix. Closing search cancels the request and resets its connection, clearing both the UI
and daemon conversation. Up to six exchanges are retained while the conversation is open.

Pi uses JSON events with text deltas and thinking disabled; Claude Code uses
`stream-json` with partial messages. Tools, project context, extensions/MCP and
session persistence are disabled through the adapters' supported flags. The
harness runs in a separate process group with a neutral working directory.
Only answer text is forwarded, coalesced at 40 ms; cancellation checks also run
while there is no output. Requests have a 60-second deadline and a 12 KiB answer
limit. Cancellation, timeout or disconnection stops the process group. CLI startup
and model latency still determine time to the first real token.

The socket protocol adds `chat-progress`, with the same request ID and cumulative
`message` fields as `chat-response`. The final response replaces the pending
answer rather than adding another transcript entry. Newlines and tabs are
accepted in answer text; other control characters remain rejected.

## More search providers and preview formats

- Unit conversions: `10 km to mi`, `0 c to f`, `20 MB to MiB`, `90 min to h`.
  Length, mass, temperature, data sizes and time use local calculations; Enter
  copies the result. Incompatible dimensions produce no conversion.
- Website shortcuts: `wiki:`, `gh:`, `maps:` and `yt:` open Wikipedia, GitHub,
  OpenStreetMap and YouTube searches. Query text is URL-encoded as one argument.
- Search settings can independently hide applications, files, calculator,
  conversions, web search, website shortcuts and configured external providers.
- Markdown YAML (`---`) and TOML (`+++`) frontmatter appears in the Information
  table rather than the document body. Invalid frontmatter stays visible, with
  an explanation. Alias expansion is refused. Long metadata tables scroll within
  a bounded area so the document retains space.
- CSV/TSV files render escaped tables (up to 100 data rows and 30 columns).
  JSON is pretty-printed. EML previews show the message body and header facts.
  ZIP/EPUB and tar-based archives list up to 200 entries without extracting them.
- Preview settings enable/disable previews and background preparation and can
  lower the maximum file size from 20 MB to 1 MB. The hard ceiling stays 20 MB.

Checks: `python3 tests/search-cli-ai.py` uses the real daemon and a deterministic
fake CLI to check explicit activation, streaming, history, errors and cancellation
of descendants. It makes no AI account requests. `bash tests/search-ai.sh` checks
streamed UI updates and stale reply handling. `bash tests/bingux-settings.sh`
checks settings persistence, validation and layout. `python3 tests/preview-extra.test.py`
covers frontmatter, tables, email, archive escaping and limits.
