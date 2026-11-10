# Design boundaries

Contributor guideline. Every Bingux window has one content boundary: the
region that shows what the person came for. Controls that choose what that
region shows live outside it. Controls that act on the shown thing live inside
it.

bingux-ui enforces this with its types, a source scan and a debug check, so
the rule is not a matter of taste per screen.

The audit of the apps against this guideline is in
[the boundaries audit](boundaries-audit.md).

## The rule in one line

**If a control changes what the content shows, or which content shows, it is
chrome. If it changes the thing that is shown, it is content.**

A segmented control that switches Calculator between Basic and Scientific
decides which calculator is on screen, so it sits in the header bar. The keypad
types into the calculation on screen, so it sits in the content.

## Terms

| Term             | Meaning                                                                                                                             |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Content          | The region that shows the document, list, calculation, page or picture. It has the content tone.                                    |
| Content boundary | The edge of the content region. A tone step, one hairline, or a glass edge shows it.                                                |
| Chrome           | Everything around the content that orients, navigates, chooses or frames it: header, toolbar, sidebar, status bar, persistent bars. |
| Strip            | The band of document tabs above the chrome. It sits outside the browsing frame, so it has its own tone.                             |
| Overlay          | A surface above everything: menus, popovers, dialogs, sheets, the command palette, toasts.                                          |
| Slot             | A named place in the window shell that takes one kind of item: `header`, `sidebar`, `content` and so on.                            |

## Sources

The rule is the one every major desktop and phone guideline states, in its own
words.

| Source                                                                                                                   | What it says                                                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Apple HIG: Materials](https://developer.apple.com/design/human-interface-guidelines/materials)                          | Controls and navigation form "a distinct functional layer" above the content layer. Glass is not used in the content layer.                        |
| [Apple HIG: Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars)                            | A toolbar holds the view title, navigation, search and actions. Leading, center and trailing groups. Scroll edge effect separates it from content. |
| [Apple HIG: Segmented controls](https://developer.apple.com/design/human-interface-guidelines/segmented-controls)        | On macOS, use a segmented control to switch views in a toolbar or inspector. Limit to about five to seven segments.                                |
| [Apple HIG: Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars)                            | Tab bars navigate, toolbars act. Keep the tab bar visible.                                                                                         |
| [Apple HIG: Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars)                            | The sidebar navigates top-level areas and floats in the controls layer.                                                                            |
| [Apple HIG: Windows](https://developer.apple.com/design/human-interface-guidelines/windows)                              | A window is a frame (title bar, toolbar, rare bottom bar) around a body area.                                                                      |
| [GNOME HIG: Header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html)                           | Start, center, end. The window heading sits in the center, "sometimes replaced with a view switcher".                                              |
| [GNOME HIG: View switchers](https://developer.gnome.org/hig/patterns/nav/view-switchers.html)                            | Three to five views. When the window is too narrow, the switcher moves to the bottom edge.                                                         |
| [GNOME HIG: Sidebars](https://developer.gnome.org/hig/patterns/nav/sidebars.html)                                        | Controls that affect the sidebar list go above the list.                                                                                           |
| [WinUI: SelectorBar](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/selector-bar)                    | Switches between sets or views of data ("All, Unread, Flagged"). Use NavigationView for app navigation and TabView for documents.                  |
| [WinUI: Command bar](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar)                     | Top on large screens, bottom on handhelds for reach.                                                                                               |
| [Material 3: Segmented buttons](https://m3.material.io/components/segmented-buttons/guidelines)                          | Segmented buttons "select options, switch views, or sort elements".                                                                                |
| [KDE HIG: Layout and navigation](https://develop.kde.org/hig/layout_and_nav/)                                            | Toolbar and tab bar above the content on desktop, below it on mobile. A view's own controls go in a second toolbar.                                |
| [MDN: Firefox theme colors](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/theme)  | Separate colors for the `frame` (tab strip), the `toolbar` and the selected tab, which takes the toolbar color.                                    |
| [Mozilla: Nova and themes](https://blog.mozilla.org/addons/2026/09/29/nova-is-here-what-changes-for-your-firefox-theme/) | Toolbar and sidebar sit flush and share one background. A separator line marks where they meet the page.                                           |

## Surface tiers

A window has three tiers, plus overlays. The shell paints them. An app never
paints a structural tier by hand.

| Tier    | Paint token                                       | What it covers                               | Meaning                                                  |
| ------- | ------------------------------------------------- | -------------------------------------------- | -------------------------------------------------------- |
| Content | `Paint::Bg`                                       | The content slot                             | What the person came for.                                |
| Chrome  | `Paint::Sidebar`                                  | Header, sidebar, aside, footer               | The frame that orients and chooses. One continuous tone. |
| Strip   | `Paint::Strip`                                    | The document tab strip                       | The band outside the browsing frame.                     |
| Overlay | `Paint::Popover`, `Paint::Card`, `Paint::Surface` | Menus, dialogs, sheets, cards inside content | Floats above a tier; never a tier of its own.            |

Inside the content, a card (`Paint::Card`) or a viewer recess
(`Paint::Recess`) groups content. Those are content surfaces, not boundaries.

## Where a control goes

Ask two questions in order.

1. Does the control change **which** content shows (a mode, a view, a
   location, a page of a set)? It is chrome. Put it in the header.
2. Does it change **how** the content shows without changing the content
   (sort, filter, zoom, layout, wrap)? It is chrome. Put it in the header or
   the toolbar.
3. Otherwise it changes the content itself (typing, editing, a setting's
   value, playback of the item). It is content.

| Control                                                     | Kind                              | Slot                                     |
| ----------------------------------------------------------- | --------------------------------- | ---------------------------------------- |
| Mode or view switcher (Basic, Scientific; Day, Week, Month) | Chooses content                   | `HeaderBar::switcher`                    |
| Sidebar destinations (Songs, Albums; Settings pages)        | Chooses content                   | `sidebar`                                |
| Location (a place, a folder, a path bar)                    | Chooses content                   | Header title slot, or `pager` dots       |
| Back, Forward, Up, Today, Previous, Next                    | Moves through content             | Header leading                           |
| Search that filters the shown list                          | Filters content                   | Header trailing, or the sidebar top      |
| Sort, filter, group by, show hidden                         | Changes how content shows         | Header trailing menu                     |
| Zoom, scale, fit, rotate a view, wrap, font size            | Changes how content shows         | Header trailing                          |
| New, Open, Save, Share, Delete selected                     | Acts on the document or selection | Header leading or trailing               |
| Column header that sorts a table                            | Part of the table                 | Content (the table owns its header)      |
| Keypad, editor, form row value, slider of a setting         | Edits content                     | `content`                                |
| Find and replace bar                                        | Acts on content                   | `content_bar`, on the content's top edge |
| Transport (play, seek, volume) of the playing item          | Persistent app state              | `footer`                                 |
| Status (line and column, counts, encoding)                  | Reports on content                | `footer`                                 |
| Page dots of a carousel                                     | Chooses content                   | `pager`                                  |
| Command palette, menus, dialogs                             | Overlay                           | Overlays                                 |

### A switcher sits on the edge of what it switches

A switcher that changes the whole window sits in the window header. One that
changes one pane sits at the top edge of that pane: the PDF sidebar's Pages
and Outline switch sits at the top of the sidebar, as the GNOME sidebar
guideline asks. Never put a switcher below content it switches.

### What may cross a boundary

- **A find bar attaches to the content's top edge.** It acts on the content,
  so it is inside the boundary and shares the content tone.
- **Transient feedback.** A drop target highlight or a toast may span both.

Nothing else crosses. A header never grows a second row that holds content.

## How a boundary is drawn

| Situation                                        | Boundary                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------ |
| Chrome meets content on a desktop window         | Tone step plus one 1 px hairline (`Paint::Separator`) on the content's edge    |
| Sidebar meets header                             | No line. They are one chrome tone.                                             |
| Footer meets content and sidebar                 | One hairline across the footer's top edge                                      |
| Strip meets header                               | Tone step only. Tabs float as tiles; nothing joins a tab to the toolbar.       |
| A page header over a scrolling page (split view) | Content tone with a scroll-edge hairline that shows while the page is scrolled |
| Immersive content                                | A glass edge: blur and tint, one hairline on the inner edge                    |

Use one line, not two. No shadow on a structural boundary: shadows mean
"floats above", which is the overlay's job.

## Persistent bars

A bar that belongs to the app rather than to one view is chrome. Media's
now-playing bar and the Text Editor's status bar are the two in Bingux.

- It runs the full window width, under the sidebar and the content. The
  sidebar stops above it, as Spotify draws its player.
- It has the chrome tone and one hairline on its top edge.
- On a phone it becomes the mini player (or the status line) directly above
  any bottom navigation.

## Immersive content

Weather's sky, a film, a map: the content is the whole window. The chrome
floats over it as glass (`Paint::Frosted`: a blur plus a tint, one hairline on
the inner edge). The boundary is the glass edge, not a tone step.

- Use it only when the content is a picture of something, not a document.
- The chrome keeps the same slots and order.
- With reduce transparency on, the glass falls back to the opaque chrome tone.
- A carousel of content pages shows its page dots in the `pager` slot, over
  the content's bottom edge, as chrome.

## Phones and compact windows

A phone is held in one hand, so the views of a window go to the thumb. The
switcher is chrome on a phone too: it moves to the bottom edge, outside the
content, as GNOME's view switcher bar, Apple's tab bar and KDE's navigation
tab bar do.

| Slot              | Desktop                           | Compact                                                  |
| ----------------- | --------------------------------- | -------------------------------------------------------- |
| `header` switcher | Center of the header              | See the switcher rule below                              |
| `sidebar`         | Leading pane                      | Its own screen; `compact_pane(Pane::Sidebar)` shows it   |
| `aside`           | Trailing pane                     | A page pushed over the content, Back at the leading edge |
| `footer`          | Full-width bar                    | Mini player or status line, above the bottom bar         |
| `pager`           | Dots on the content's bottom edge | The same                                                 |
| `strip`           | Above the header                  | The same; tabs scroll                                    |

### The switcher rule

| Views                         | Phone form                                                                                              |
| ----------------------------- | ------------------------------------------------------------------------------------------------------- |
| 2 to 5, each with an icon     | The bottom navigation bar: icon and short label, the selected one on an accent pill, one hairline above |
| More than 5, or no icons      | A view menu in the header's title slot, a choice sheet when opened                                      |
| Some with icons, some without | Reported by the debug check: give every view an icon, or none                                           |

From the window's bottom edge up a phone stacks the bottom bar, then the
footer (the mini player, the status line), then the content. The bottom bar
is chrome tone and outside the content boundary.

- The header is one row at every size: nothing stacks under it.
- An aside or a sidebar on a phone is a page pushed over the content, with
  Back at the header's leading edge. A sheet is for a popup, never for a pane.
- An aside's header is the same row, its title at the start. Its only
  dismiss is the pressed toggle that opened it.

Two parts of the rule are not built yet, because bingux-ui has no platform
source for them: keeping the bar above the home indicator's inset, and
hiding it while the on-screen keyboard is up. Both belong to the shell when
the compositor reports them.

### Each app on a phone

| App         | Phone placement                                                                                                     |
| ----------- | ------------------------------------------------------------------------------------------------------------------- |
| Calculator  | Basic, Scientific, Programmer and Convert as the bottom bar, with icons; history is a page pushed over the keys     |
| Calendar    | Day, Week, Month, Year and Agenda as a view menu in the header's title slot, the arrows beside it                   |
| Media       | Songs, Albums, Artists and Videos are routed roots: the first screen lists them; the mini player sits at the bottom |
| Weather     | Places stay a carousel with page dots; the places list is its own screen from the header                            |
| Settings    | A page switcher (Sound effects, Data usage) is a view menu in the page header: its views have no icons              |
| Text Editor | No switcher; the project panel is a page, the status line sits at the bottom                                        |
| Files       | Tabs scroll in the strip; places are a menu from the header                                                         |
| Preview     | No switcher; the pages sidebar is its own screen                                                                    |

## The API enforces this

The shell owns the window structure. An app fills slots; it never lays out
or paints the window frame.

```rust
Screen::new(
    HeaderBar::new(tr!("calculator-title"))
        .switcher(view_switcher(modes, current, on_mode))
        .trailing(history_button)
        .trailing(app_menu(menu))
        .trailing(WindowControls),
)
.content(calculator)
.aside(history)
```

### Slots

| Builder call                                                    | Takes                       | Tier    | Notes                                                                 |
| --------------------------------------------------------------- | --------------------------- | ------- | --------------------------------------------------------------------- |
| `Screen::new(header)`                                           | `HeaderBar`                 | Chrome  | Painted `Paint::Sidebar`                                              |
| `.strip(bar)`                                                   | `HeaderBar` with `tab_slot` | Strip   | Holds the window buttons                                              |
| `.sidebar(item)`                                                | `impl ContentItem`          | Chrome  | Leading pane                                                          |
| `.content(item)`                                                | `impl ContentItem`          | Content | Hairline on each edge that meets chrome                               |
| `.content_bar(item)`                                            | `impl ContentItem`          | Content | Top edge of the content: find bar                                     |
| `.aside(item)`                                                  | `impl ContentItem`          | Chrome  | Trailing pane                                                         |
| `.footer(item)`                                                 | `impl ContentItem`          | Chrome  | Full width, under sidebar and content                                 |
| `.pager(dots)`                                                  | `PageDots`                  | Chrome  | Over the content's bottom edge                                        |
| `.immersive(on)`                                                | `bool`                      |         | Content fills the window; chrome is glass                             |
| `.compact_pane(pane)`                                           | `Pane`                      |         | Which pane a phone shows                                              |
| `.sidebar_width(dim)`                                           | `Dim`                       |         | `Dim::Sidebar` by default; Files uses `Dim::PlacesColumn`             |
| `HeaderBar::switcher(sw)`                                       | `ViewSwitcher`              | Chrome  | Center; on a phone the bottom bar or a view menu in the title slot    |
| `view(key, label).icon(icon)`                                   | `View`                      |         | One view; the icon is what a phone's bottom bar shows                 |
| `HeaderBar::switcher_leading(item)`, `.switcher_trailing(item)` | `impl ChromeItem`           | Chrome  | Previous and Next, Today: they join the leading and trailing clusters |
| `HeaderBar::leading(item)`, `.trailing(item)`                   | `impl ChromeItem`           | Chrome  |                                                                       |
| `NavigationSplitView::footer(item)`                             | `impl ContentItem`          | Chrome  | As `Screen::footer`                                                   |
| `use_page_switcher(sw)`                                         | `ViewSwitcher`              | Chrome  | A routed page's switcher, in its page header                          |
| `use_chrome_inset()`                                            |                             |         | How far immersive content starts its words below the glass            |

### What is checked where

| Check                                                 | When          | What it catches                                                                                                               |
| ----------------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `ViewSwitcher` and `PageDots` have no `Into<Element>` | Compile time  | A switcher or dots passed to `.child(..)` or any content slot                                                                 |
| `ContentItem` bound on content slots                  | Compile time  | The same, with a message that names `HeaderBar::switcher`                                                                     |
| `ChromeItem` bound on `leading` and `trailing`        | Compile time  | Anything that is not an element or a chrome item                                                                              |
| `Segmented` has no public tab mode                    | Compile time  | A value picker used as a view switcher                                                                                        |
| `boundaries_tests.rs` source scan                     | `cargo test`  | Hand-painted structural tiers, hand-built window frames, a segmented control outside a row in an app                          |
| `boundary::violations()`                              | Debug runtime | A chrome-only control under the content slot (a header bar nested in content); a phone switcher with icons on only some views |

The scan has a `PENDING` list. Each entry names a file that still breaks a
rule. The test fails when a listed file is clean, so the list only shrinks.

## Per-app placement

### Calculator

| Slot            | Holds                                                          |
| --------------- | -------------------------------------------------------------- |
| Header switcher | Basic, Scientific, Programmer, Convert                         |
| Header trailing | History toggle, main menu, window buttons                      |
| Content         | Display, keypads, Programmer base and word size, Convert units |
| Aside           | History; a page pushed over the keys on a phone                |
| Bottom bar      | The modes on a phone, with icons                               |

Programmer's base (HEX, DEC, OCT, BIN) and word size stay in the content. They
set how the keypad types and how the value wraps, so they edit the
calculation rather than choose a view of it.

### Calendar

| Slot            | Holds                                          |
| --------------- | ---------------------------------------------- |
| Header leading  | Today, Previous, Next                          |
| Header title    | The date range on show                         |
| Header switcher | Day, Week, Month, Year, Agenda                 |
| Header trailing | Search, New event, main menu, window buttons   |
| Sidebar         | Month picker, calendars (which calendars show) |
| Content         | The day, week, month, year or agenda grid      |

### Weather

| Slot    | Holds                                                   |
| ------- | ------------------------------------------------------- |
| Mode    | Immersive: the sky fills the window                     |
| Header  | Add place, refresh, main menu (units), window buttons   |
| Content | A carousel of places: hero, hourly, daily, detail cards |
| Pager   | Page dots, one per place                                |
| Sidebar | Saved places, wide windows only, as glass               |

Places are chosen by swiping the carousel, the dots or the sidebar. They are
never chips inside the content.

### Media

| Slot        | Holds                                                                               |
| ----------- | ----------------------------------------------------------------------------------- |
| Sidebar     | Search, Songs, Albums, Artists, Videos, playlists                                   |
| Page header | Page title, sort and view menu                                                      |
| Content     | The list or grid                                                                    |
| Footer      | Now playing: art and title; transport over the scrubber; queue, volume, mini player |
| Full player | Immersive: art or video fills the window                                            |

### Preview

| Slot            | Holds                                                          |
| --------------- | -------------------------------------------------------------- |
| Header leading  | Open, pages sidebar toggle                                     |
| Header title    | File name and position in the folder                           |
| Header trailing | Crop, rotate, zoom, info, main menu, window buttons            |
| Sidebar         | Pages and Outline, with their switch at the sidebar's top edge |
| Content         | The picture or page on the recess                              |

### Text Editor

The worked example for a document app, after VS Code and Zed.

| Slot            | Holds                                                                                    |
| --------------- | ---------------------------------------------------------------------------------------- |
| Strip           | Document tabs, New tab, window buttons                                                   |
| Header leading  | Open, New, Save                                                                          |
| Header trailing | Find, View menu (wrap, font size, spacing, line numbers), language, main menu            |
| Sidebar         | Outline, optional                                                                        |
| Content bar     | Find and replace, Go to line                                                             |
| Content         | The text, with its gutter and minimap                                                    |
| Footer          | Status: line and column, selection, indent, encoding, line endings, language, word count |

Prose is not a mode. Markdown and plain text get soft wrap, an optional
readable column width and the word count in the status bar; see
[the note on prose](text-editor-writing-mode.md).

The layout follows Zed's: a project panel, the tab strip, a breadcrumb row,
the editor with its gutter and minimap, the status bar, and optional bottom
and outline panels.

### Settings

| Slot        | Holds                                                                                           |
| ----------- | ----------------------------------------------------------------------------------------------- |
| Sidebar     | Search, pages                                                                                   |
| Page header | Back, page title, the page's switcher (Sound effects: Output and Input; Data usage: the period) |
| Content     | Groups and rows. A row's value is content: it edits the setting.                                |

### Files

| Slot    | Holds                                                               |
| ------- | ------------------------------------------------------------------- |
| Strip   | Tabs, New tab, window buttons                                       |
| Header  | Back, Forward, Up, path bar, search, view and sort menus, main menu |
| Sidebar | Places, devices, network                                            |
| Content | The file view; column headers sort the list                         |

Tabs float: each is a rounded tile on the strip, the selected one raised.
A tab is as wide as its icon, title and close circle need, `Dim::TabMax` (200
px) at most, left-aligned, never stretched. When the tabs no longer fit they
share the strip and shrink together (the title fades, then each is its favicon
only, down to `Dim::TabMin`), then the strip scrolls.

New tab follows the last tab. The Text Editor and the Browser use the same
model.

## Review checklist

Run this on every new screen and every Doop frame.

- [ ] Every control that switches, filters, sorts, scales or locates the
      content is in the header, the sidebar, the strip or the pager.
- [ ] Nothing in the content chooses which content shows.
- [ ] The content is `Paint::Bg`; header, sidebar, aside and footer share
      `Paint::Sidebar`; tabs sit on `Paint::Strip`.
- [ ] One hairline where chrome meets content. No line between sidebar and
      header. No shadow on a structural edge.
- [ ] A persistent bar runs the full window width under the sidebar.
- [ ] On a phone a switcher of 2 to 5 views with icons is the bottom bar, any
      other switcher a view menu in the title slot; the header is one row;
      panes become pushed pages with Back;
      the footer sits above the bottom bar.
- [ ] Immersive screens use glass chrome and fall back to opaque chrome with
      reduce transparency.
- [ ] The app builds its window with `Screen` or `NavigationSplitView` and
      paints no structural tier itself.

## Anti-patterns

- A segmented mode switch as the first row of the content (the old Doop
  Calculator frames).
- Chips that pick a place, a filter or a category inside the content.
- A second header bar drawn inside the content.
- A sort or view toggle floating over a list.
- A sidebar that runs to the window bottom beside a persistent bar.
- Two lines, or a line and a shadow, on one boundary.
- A tab joined to the toolbar, or tabs stretched to fill the strip.
