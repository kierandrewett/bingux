# Path elision

How Bingux shows a path or a file name in less room than it needs. Contributor
doc: read it before touching `crates/ui/src/elide.rs`, `PathLabel`
or anything that cuts text in the middle.

Code: `bingux_ui::elide` (pure functions, tests beside it in
`src/elide/tests.rs`). The control is `bingux_ui::controls::path_label`. User
facing summary: `docs/bingux-ui.md`, section "Path text".

## The problem

`/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf` in a
300 px field. A cut at the end (`/home/kieran/Documents/Pro…`) loses the file,
which is the part the person came for. A cut in the middle by characters
(`/home/kieran/Docu…-report-2026.pdf`) shows two half words and no structure.
The first version of `elide_path` collapsed whole directories and then
shortened the name, which was better but left room unused (on the phone
specimen it stopped at `/home/…/design/final-report-2026.pdf` with a visible
gap at the right) and had no step between "some directories" and "none".

The question Kieran asked: is there a known algorithm? There is no standard
one. There are several partial ones, each solving one slice. They are
listed here, then composed.

## What exists

### Operating systems and toolkits

- **Windows `PathCompactPathEx`** (shlwapi). Replaces path components with an
  ellipsis so the path fits a number of *characters*. A file name that is too
  long on its own is truncated at the end (`My Fil...`). It counts characters,
  not pixels; `PathCompactPath` is the pixel-width version. Neither is
  specified beyond that.
  <https://learn.microsoft.com/en-us/windows/win32/api/shlwapi/nf-shlwapi-pathcompactpathexw>
- **Win32 `DT_PATH_ELLIPSIS`** (`DrawText`). Replaces characters in the middle
  of the string with an ellipsis; when the string has backslashes, "preserves
  as much as possible of the text after the last backslash". The rule is the
  one that matters most: *the end of a path is the most valuable part*.
  `DT_END_ELLIPSIS` and `DT_WORD_ELLIPSIS` are the contrast.
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-drawtextw>
- **macOS.** Finder list view and Get Info cut a long name in the middle so
  the extension and the end stay visible (`NSLineBreakByTruncatingMiddle`,
  "the beginning and end fit in the container and an ellipsis glyph indicates
  the missing text in the middle"). The Finder path bar and `NSPathControl`
  (standard style) hide *whole names between the first and the last item* when
  the list is too long. The HIG describes only that.
  <https://developer.apple.com/documentation/uikit/nslinebreakmode/bytruncatingmiddle>,
  <https://developer.apple.com/design/human-interface-guidelines/path-controls>
- **GNOME.** `PANGO_ELLIPSIZE_MIDDLE` omits characters in the middle of the
  text; it knows nothing about paths or extensions. GNOME Files (Nautilus)
  does not elide inside its path bar: the bar is a row of buttons that
  scrolls when it overflows (`pathbar: Add overflow scrolling`), so nothing
  is cut but the visible window slides to the end.
  <https://docs.gtk.org/Pango/enum.EllipsizeMode.html>,
  <https://mail.gnome.org/archives/commits-list/2021-December/msg08201.html>
- **Qt `QFontMetrics::elidedText`.** `ElideLeft`, `ElideMiddle`, `ElideRight`
  by pixel width; "the elide mark follows the layout direction". Paths and
  extensions are the caller's problem.
  <https://doc.qt.io/qt-6/qfontmetrics.html#elidedText>
- **Chromium `gfx::ElideFilename` / `ElideText`.** `ElideFilename` is the
  closest to our step 4: "with an emphasis on not hiding the extension unless
  we have to". It keeps the whole name when root and extension fit, else cuts
  the end of the root (not the middle) and keeps the extension, and when the
  extension alone is wider than the room, cuts the whole name in the middle.
  It wraps the result in an LTR embedding so the elided name keeps its
  direction in an RTL UI. `StringSlicer` cuts "while respecting UTF-16
  surrogate pairs and combining character sequences", and `ElideString` has
  a comment that it does not handle bidi.
  <https://chromium.googlesource.com/chromium/src/+/main/ui/gfx/text_elider.h>,
  <https://chromium.googlesource.com/chromium/src/+/main/ui/gfx/text_elider.cc>
- **Chromium `ElideUrl` / `ElideHost`.** `ElideUrl` returns "a string
  composed of parts from subdomain, domain, path, filename and query" with a
  trailing `...` when it does not fit, and says that in RTL locales the result
  must be marked LTR (<http://crbug.com/6487>). `ElideHost` "will never elide
  past the TLD+1 point": what identifies the site is never elided away. We
  keep the same rule for `scheme://host`: it is one segment that is never
  abbreviated or cut while any other rung is left.
  <https://chromium.googlesource.com/chromium/src/+/main/components/url_formatter/elide_url.h>

### Shells and editors

- **fish `prompt_pwd`.** "Replace the home directory with `~` and shorten
  every path component but the last to a default of one character."
  `fish_prompt_pwd_dir_length` sets the length (0 disables), 
  `fish_prompt_pwd_full_dirs` keeps components from the right in full.
  `/tmp/banana/sausage/with/mustard` is `/t/b/s/w/mustard`. This is the
  abbreviation step. It keeps *every* component recognisable by its first
  letter and the last one whole.
  <https://fishshell.com/docs/current/cmds/prompt_pwd.html>
- **Starship `directory`.** `truncation_length` drops directories from the
  left (`rock/and/roll`), `truncation_symbol = '…/'` marks it. With
  `fish_style_pwd_dir_length` the directories that would be dropped are
  abbreviated instead: `/b/t/c/o/rock/and/roll`. So the shells compose the
  two operations too: *keep the last N whole, abbreviate the rest*.
  <https://starship.rs/config/#directory>
- **zsh** prompts keep the last or the first N components with `%Nd`/`%N~`
  and `%-N~` (no abbreviation).
  <https://zsh.sourceforge.io/Doc/Release/Prompt-Expansion.html>
- **VS Code.** `labels.shorten` is not about width but about *telling apart*:
  for a list of paths it finds, for each, the smallest sub-path that no other
  path contains, and shows it with `…` where the rest was ("every shorten
  path matches only one original path and vice versa"). Used for editor tab
  descriptions where two tabs have the same file name.
  <https://github.com/microsoft/vscode/blob/main/src/vs/base/common/labels.ts>
- **Blog posts, Stack Overflow and design systems.** The recurring advice,
  collected: keep the end (file name and extension), keep the root, cut the
  middle; progressive cuts; never cut the extension unless nothing else
  fits; give the full text in a tooltip and keep it copyable. Vercel's
  Geist `MiddleTruncate` says the same ("strings whose head and tail both
  carry information: file paths", "copying truncated text yields the full
  original string", pair it with a tooltip, "the truncation point jitters if
  the container width changes during an interaction").
  <https://examples.vercel.com/geist/middle-truncate>; Jeff Atwood's
  "Shortening Long File Paths" (<https://blog.codinghorror.com/shortening-long-file-paths>):
  "keep the first two folders at the beginning, replace the middle with an
  ellipsis, and leave the final folder and filename on the end".
- **CSS** has no path elision. A W3C wiki proposal for
  `text-overflow: ellipsis` in the middle is only a proposal.
  <https://www.w3.org/wiki/Text-overflow_middle_cropping>

### Unicode

- **UAX #29** (text segmentation). A cut is allowed only at *extended grapheme
  cluster* boundaries: "boundaries never occur within a combining character
  sequence". An accent, a joined emoji, a flag or a Hangul syllable is one
  user-perceived character. A byte or a `char` is neither.
  <https://unicode.org/reports/tr29/#Grapheme_Cluster_Boundaries>
- **UAX #9** (bidi). In a right-to-left paragraph a path still reads from its
  root: `/home/…` is not `…/home`. The fix that does not alter the text is an
  isolate (U+2066 LRI … U+2069 PDI) around the path.
  <https://unicode.org/reports/tr9/>

None of them does the whole job. Windows and macOS keep the end and hide the
middle. Shells abbreviate and keep the end. Chromium protects the extension.
VS Code tells duplicates apart. The Bingux algorithm is those composed, in
the order of what costs the reader least.

## The choice

A **ladder**: a fixed, ordered list of candidate strings from the most to the
least informative. The function returns the **first candidate that fits**.

Why a ladder and not a greedy loop that shortens until it fits:

1. **Monotone by construction.** The ladder depends on the path only, never on
   the width. The set of candidates that fit grows with the width, so the
   first one never comes later: a wider width never shows less. There is no
   threshold, no state and no hysteresis; dragging a window edge back and
   forth goes through the same texts in the same order.
2. **The result is never wider than the room** (except the lone `…`, the
   least there is), since only a candidate that was measured to fit is
   returned.
3. **Idempotent.** An elided result fits the width it was made for, so
   eliding it again returns it unchanged.
4. **Pixel exact.** The decision is made on `Measure::width` of the whole
   candidate with the real font, size, tracking and tabular features, so
   kerning and proportional letters are right. A fake monospace measurer in
   the tests gives the same ladder.
5. **Cheap.** The segment rungs are tried one by one (a path has a handful of
   segments); the rungs that cut letters are a monotone run and are found by
   halving. Candidate strings repeat across widths, so `measure_text`'s
   per-style cache turns a window resize into hash lookups.

The last property of a ladder is its sorting. The steps are ordered by what
the reader loses least:

### The steps

Take `/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf`
(`n = 7` segments: `home kieran Documents Projects bingux design
final-report-2026.pdf`). "Anchor" is the first segment: `~`, a drive such as
`C:`, `scheme://host`, or the first directory after `/`.

1. **Whole.** The path, if it fits. Verbatim: nothing is normalised.
2. **Collapse the middle** (`ElideStep::Collapsed`). Replace directories by one
   `…` segment, from the middle outwards, one at a time: keep the anchor and
   the last directories, the end gets the extra segment (`/home/kieran/…/design/file`
   before `/home/…/design/file`). The rungs are the kept counts
   `n-1, n-2, …, 4` (for a path with at least five segments; the floor is
   `n-1` for shorter paths, never below 2). This is Win32 `DT_PATH_ELLIPSIS`,
   macOS path bar and Atwood's advice, with the end preferred.
3. **Abbreviate** (`ElideStep::Abbreviated`). On the last skeleton of step 2
   shorten directories to their first character, `.config` to `.c`, longest
   first and from the left among equals, one at a time. Never the anchor if it
   is `~`, a drive or a host (they are the identity of the place), never
   `.` or `..`, never the file name and never the directory that holds it
   (what `prompt_pwd` calls `full_dirs`). Once every directory that may be is
   abbreviated the collapse goes on, down to `anchor/…/file`:
   `/h/k/…/design/f` → `/h/…/design/f` → `/h/…/f`.
   The floor of step 2 is why this step has anything to work on: collapsing
   all the way before abbreviating would leave one directory, the anchor, and
   abbreviating it alone gains nothing. Starship's `fish_style_pwd_dir_length`
   composes the same pair the same way.
4. **Cut the name** (`ElideStep::NameCut`). In `/h/…/` plus the name, cut the
   *middle of the stem*: `head…tail.ext`. The extension stays whole
   (`.tar.gz` and any `.tar.*` as one; a leading dot, a long or spaced tail
   is not an extension) and so does the end of the stem, `min(4, k/2)`
   graphemes, because the end of a stem is often a number or a version
   (`…2026.pdf`, `…-v2.tar.gz`). When fewer than eight stem graphemes would
   survive, the directories give their room to the name and it is cut on its
   own (`ElideStep::NameOnly`): `final-rep…2026.pdf`. This is Chromium's
   `ElideFilename` with the cut in the middle, as the Finder does it.
5. **Last resort** (`ElideStep::Middle`, `ElideStep::Least`). The extension goes
   too: the name is cut in the middle with no special care, down to one
   grapheme and `…`, and finally `…` alone. The text is never empty.
   The brief said "elide the middle of the whole string" here; at this point
   every directory is already gone, so the whole string *is* the name.

A trailing separator stays in every candidate. A directory name has no
extension (`releases/v1.2/` is a folder), a trailing slash says so. `/`, `///`
and other separators-only strings fall to the plain middle cut.

### What was weighed and not chosen

- **Abbreviate first, collapse later** (fish alone, `/h/k/D/P/b/d/file`). Keeps
  every component visible, which is attractive. But it destroys the readable
  part of the path early (`Documents` is known by `Documents`, not by `D`)
  and it is what the shells do because their directory is a prompt, not a
  place to read. Kept as step 3, where the choice is between losing a whole
  directory and losing letters.
- **Always keep the first two directories** (Atwood). Wastes room on a
  `/home/kieran` that is the same on every row of a list. The ladder keeps the
  anchor and the end, then the extra segment goes to the *end*.
- **Cut the end of the stem** (Chromium `ElideFilename`). Fine for a Save As
  field; in a list of `IMG_20260101_1.jpg` … `IMG_20260101_2.jpg` the end is
  the only part that differs, so the middle is cut.
- **A greedy loop** (shorten the widest piece until it fits). Not monotone:
  a rounding of one pixel can change which piece is shortened first, so the
  text flickers while a window is resized.
- **Binary search over the whole ladder.** The widths along the ladder are not
  monotone (a collapsed path with a long name can be wider than an abbreviated
  one with a short name), so the segment rungs are scanned in order. Only the
  letter-cutting rungs, whose width grows with the letters kept, are halved.

## Unicode and direction

- **Graphemes, not chars or bytes.** Names are split with `unicode-segmentation`
  (UAX #29, extended grapheme clusters). An accent, an emoji with a joiner
  or skin tone, a flag, a Hangul syllable and a CJK character each count as
  one and are never separated from their own marks. Rust strings are UTF-8, so
  a cut never lands inside a multi-byte sequence either; there are no
  surrogates to split. Tests assert this for every width of samples that
  include combining marks, a ZWJ family and flags.
- **Abbreviation takes the first grapheme** after an optional dot:
  `é` + U+0301 stays whole.
- **Bidi.** The ladder works in logical order (head and tail of the stored
  string), which is what the toolkits do. Right-to-left *names* inside a path
  therefore keep their own order; the path as a whole reads left to right.
  `isolate_ltr` wraps text that contains right-to-left letters in
  U+2066 … U+2069 so a neighbouring RTL run cannot reorder the `/`
  separators; text without such letters is returned unchanged so ASCII paths
  carry no invisible characters. `PathLabel` applies it to what it draws, not
  to the text it measures, the tooltip, the accessible label or Copy.
- **The ellipsis** is the single `…` (U+2026), as in Pango, Qt and Chromium;
  not three dots. It is part of the measured candidate.
- **Separators.** `/` by default; a path with a backslash and no slash is a
  Windows path and keeps `\` (`C:\Users\…\file`, `\\server\share\…`).
  `scheme://host` is one segment that is never abbreviated.
- **Not done:** reordering of the ellipsis for RTL UI directions (Qt moves the
  mark with the layout direction). A path stays LTR, and so does its
  ellipsis.

## Telling duplicates apart

`distinguish(&[&str]) -> Vec<Distinct>` (`name`, `context`). For lists of paths
or tabs: a name that is unique in the list is shown alone; names shared by
different paths add the nearest parent directories, one more per round and
**all of the clashing paths together**, until they differ
(`index.ts — app/src`, `index.ts — lib/src`). This is the VS Code idea with
the common-suffix restriction (the context is always the nearest parents, so a
label reads like the end of its path), and growing together so that `app/src`
is never set beside a bare `src`. The same path twice has the same label. Use
`Distinct::label()` for one string or draw `name` and `context` with different
inks.

## Where it is used

| Place | Cut |
| --- | --- |
| `PathLabel` (`path_label(..)`, default) | `elide_path` |
| `PathLabel::name()`: file list name column, grid tile name, preview header, properties title | `elide_name` |
| `PathLabel::middle()`: place and bookmark names, other labels | `elide_middle` |
| Address bar | crumbs, not text: the middle crumbs collapse into a `…` menu button that lists them, the tail stays; one crumb name wider than a limit is cut by `elide_middle` |
| Properties dialog "Location" | `PathLabel` (path) |
| Tab titles | `distinguish`, then the title is cut by the tab |

The full text is always available: the hover tooltip, the accessible label,
`copyable(true)` (right click, long press) and `selectable(true)`.

## Properties (tested)

For both a monospace and a proportional fake measurer, every sample path, every
width in half-pixel steps:

- the result fits, unless it is `…`;
- the width of the result and the step of the ladder only grow with the width;
- eliding a result with the same width returns it unchanged;
- the same input gives the same output;
- every cut is on a grapheme boundary.

Plus table tests of each step for the paths of the taxonomy "Path label"
page, `~`, relative, `..`, a trailing slash, Windows, UNC, `scheme://host`,
empty, `/`, a 300-letter component, no extension, `.tar.gz`, a dot file,
CJK, combining marks, a joined emoji and flags.
