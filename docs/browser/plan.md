# Browser: plan

Copied from bingux-doop `out/apps-browser/PLAN.md`; paths such as `frames/apps-browser/` are in that repository.

One app, one component tree. Size and one setting (Layout) decide the arrangement. Frames live in
`frames/apps-browser/`, named `Browser / <Group> / NN Title`, on the canvas "Bingux: Browser" (`KyPwogXD5M`). This
plan is the consolidated design of 2026-11-25 (`CONSOLIDATION.md` records what was merged and removed).

## Two parts: the web view control and the browser frame (Kieran, 2026-11-25)

The browser is built like Internet Explorer: a reusable control that shows the web (IE's WebBrowser control,
shdocvw/ieframe hosting MSHTML) and a thin frame around it (iexplore.exe).

### Web view control

`bingux_ui::web_view`, under the cargo feature `web-view` of bingux-ui.

- **Owns one page:** navigation and its history, load progress, title and icon, security state, find, zoom, audio,
  the page's drawing and input, and the content-tier status pages (cannot load, crashed, certificate).
- **The page's questions** (JavaScript dialogs, sign-in, permissions, certificate errors, context menus) arrive as
  events. An embedder that does not answer one gets the control's default: a bingux-ui dialog inside the view's
  rectangle, or the policy's refusal, as IE's `IDocHostShowUI` did.
- **Never owns** tabs, the address field, menus, the downloads list, history or settings.

### Browser frame

`bingux-browser`, its own package.

- **Owns** tabs, the toolbar, the address field and suggestions, the main menu, downloads, history, permissions
  remembered per site, private windows, the settings and library windows, and the window-wide "engine stopped" page.
- **Never owns** how a page is drawn or asked about, or starting the engine (the control's `WebEngine` does that for
  every app).

### Engine and policy

The engine (Vivarium, an embeddable Gecko) runs its own content processes, so the control is in-process glue only:
one engine per app, one engine session per web view. An app that does not embed the web does not link Vivarium (the
cargo feature is off).

An embedder decides the policy the control cannot know:

| Question                             | Default                                     |
| ------------------------------------ | ------------------------------------------- |
| What a new window does               | Hand it to Browser                          |
| Whether a download is allowed        | Hand the address to Browser                 |
| Which permissions a page may ask for | None: refused without a prompt              |
| Whether to show the origin strip     | On whenever the page can ask for a password |

A certificate error inside an embedded view has no "Proceed": only the browser frame offers that.

Second use: **Mail's account sign-in** (the provider's OAuth page inside the Add account sheet instead of a trip to
the browser; frames `Embedded / 01-06`). Later uses: Mail's HTML message view, a help viewer.

## Window structure (design/boundaries.md slots)

| Slot             | Holds                                                                                                                                                | Tier    |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| Strip            | Tabs as floating neutral tiles up to 200 px, + after the last tab, a `+N` list on overflow; window buttons                                           | Strip   |
| Toolbar (header) | Back, Forward, Reload; the address trail and its chips; downloads while there are any; uBlock Origin, Bitwarden, Extensions; the three-dot main menu | Chrome  |
| Sidebar          | Tabs on the side (Layout setting): search, pinned sites, groups as sections, tabs, New tab; folds to a rail                                          | Chrome  |
| Side panel       | Bookmarks, history, reading list, switched by an icon rail on the window's edge (after Vivaldi)                                                      | Chrome  |
| Content          | The web page or an internal page (new tab, first run, status pages, reader view), with find floating over it                                         | Content |
| Overlays         | Address card, main menu, site card, zoom, downloads, extensions, permission prompts, page dialogs, toasts                                            | Overlay |

No profile avatar in the header (Kieran: "avatar icon isn't really needed yet"); the profile is a row of the main
menu and a page of Settings. Settings and the Library (history, bookmarks, downloads) are split windows of their
own, never a page inside a tab.

## Layouts (Settings › Appearance › Layout; size can override)

| Layout                | Wide (≥ 1100)                                                                                                                    | Medium (740–1100)                                                                   | Compact (< 740: phone)                                               |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Tabs on top (default) | Strip + toolbar (Window 01)                                                                                                      | Tabs shrink to favicons, then the strip scrolls with a `+N` list (Window 17, 20-22) | Phone: toolbar at the bottom, tabs as an overview grid (Phone 01-08) |
| Tabs on the side      | Sidebar flush with the toolbar (Window 25)                                                                                       | Folded rail (Window 26)                                                             | Phone                                                                |
| One row               | Nav, address, tabs, extensions, menu, window buttons in one row (Window 27); optional bookmarks bar in the same tone (Window 28) | Same, narrower address                                                              | Phone                                                                |
| Installed site        | Header bar: Back, Reload, the site's icon, name and domain, menu (Window 29)                                                     | Same                                                                                | Phone: full screen, no address bar                                   |
| Focus mode            | Content only; the header slides in at the top edge (Window 30)                                                                   | Same                                                                                | Same                                                                 |

## Surfaces and how they adapt

| Surface                                                    | Desktop                                                                                                | Phone                                                |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------- |
| Address card (typing)                                      | Card grows out of the field, scope chips (Window 06)                                                   | Full-width card above the keyboard (Phone 06)        |
| Main menu                                                  | Anchored panel from the three-dot button: command search, zoom row, text rows with keys (Window 07-08) | Sheet (Phone 09)                                     |
| Site card, zoom, downloads, extensions, permission prompts | Popovers hanging from the chip or button that opened them (Window 09-14, Prompts 01-06)                | Sheets over the dimmed, inset page (Phone 10-11, 15) |
| Tab list                                                   | `+N` popover from the strip (Window 22)                                                                | Tab overview grid, a page (Phone 07-08)              |
| Page dialogs (alert, confirm, prompt, leave page, sign in) | Hang from the content's top edge, modal to the tab (Prompts 09-13)                                     | Centred card (Phone 17)                              |
| Passkey, certificate details, print, keyboard shortcuts    | Centred dialogs (Prompts 07, Page states 05, Tools 05, Tools 13)                                       | Centred dialog                                       |
| Find in page                                               | Floating card at the content's top right, X to close (Tools 01-02)                                     | Card above the keyboard (Phone 14)                   |
| Context menus                                              | Anchored text menus (Tools 07, Window 24)                                                              | Anchored panel at the long-press point (Phone 12)    |
| Settings, Library                                          | Split windows (Settings 01-11, Library 01-03)                                                          | List page, pages pushed over it (Phone 16)           |
| Split view                                                 | Two panes, each with its own toolbar (Tools 09)                                                        | Not offered                                          |

## Navigation, Back and Escape

- Back/Forward move through the tab's history; Alt+Left/Right; the trail's crumbs go up the path.
- Escape closes the topmost overlay; in the address field it first reverts the text, then leaves the field.
- On a phone Back closes a sheet, then goes back in the tab, then leaves the tab overview.

## States

- **Loading:** the `ds-sp` spinner replaces the favicon in the tab and in the trail; a progress line runs on the
  field's foot.
- **Security:** secure (nothing shown), "Not secure" (a solid red chip), broken certificate (a status page).
- **Status pages:** offline, cannot find, no secure connection, certificate, deceptive, tab stopped, engine stopped
  and starting again.
- **Private window** and **permission prompts**.
- **Light and dark:** Window 02 and Phone 01 show light; every other frame is dark.

## Screens and their frames

| Screen                                                                                                                             | Frames                                 |
| ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| Main window, wide and medium                                                                                                       | Window 01-05, 17                       |
| Address field and suggestions                                                                                                      | Window 06; Phone 06                    |
| Main menu                                                                                                                          | Window 07-08; Phone 09                 |
| Downloads                                                                                                                          | Window 09; Library 03; Phone 15        |
| Site information and zoom                                                                                                          | Window 10-11; Page states 06; Phone 10 |
| Extensions (uBlock Origin, Bitwarden)                                                                                              | Window 12-14; Settings 09              |
| Private window                                                                                                                     | Window 15-16; Phone 08                 |
| Tab strip and overflow                                                                                                             | Window 18-24                           |
| Layouts                                                                                                                            | Window 25-30                           |
| Page states                                                                                                                        | Page states 01-10; Phone 18-19         |
| Permission prompts, passwords, passkeys, autofill                                                                                  | Prompts 01-08; Phone 11                |
| Page dialogs and sign-in                                                                                                           | Prompts 09-14; Phone 17                |
| Tools: find, reader, translate, print, share, context menu, developer tools, split view, picture-in-picture, side panel, shortcuts | Tools 01-13; Phone 12-14               |
| Library (a manager window)                                                                                                         | Library 01-03                          |
| Settings                                                                                                                           | Settings 01-11; Phone 16               |
| Phone                                                                                                                              | Phone 01-20                            |
| Another app embedding the web view                                                                                                 | Embedded 01-06                         |

## Embedding the web view (other apps)

- The embedder places `web_view(&view)` where its content goes; the view is content tier (`Paint::Bg`); the
  embedder's header and sheet are chrome.
- **Origin strip:** one 32 px row on top of the view: lock or warning glyph, the registrable domain in full ink,
  "Open in Browser" at the end. Shown whenever the page can ask for a password, hidden for local content.
- **Loading:** the `ds-sp` spinner in the origin strip and a progress line at its foot; the sheet's title stays.
- **Cannot load, certificate error, crashed:** the control's status page inside the view; a certificate error has Go
  back only.
- **The page's questions:** JavaScript dialogs and HTTP sign-in hang from the top edge of the view, modal to the view
  only; permission requests are refused by the default policy, with no prompt.
- **New windows and downloads:** handed to Browser by default.
- **Back and Escape:** Escape closes a page dialog first, then cancels the sheet; the sheet's Back goes back in the
  view's history.
