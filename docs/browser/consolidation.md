# Browser canvas: consolidation record (2026-11-25)

Copied from bingux-doop `out/apps-browser/CONSOLIDATION.md`; paths such as `frames/apps-browser/` are in that repository.

Kieran asked: "can you check through the entire canvas/pages and just consolidate everything. there seems to be a lot
of extra pages and a lot of inconsistency can you establish a defacto design with everything i asked for. read the
comments too."

## What changed

- Canvas "Bingux: Browser" (`KyPwogXD5M`). Before: 203 frames on 8 pages (Standard layout; Vertical, compact,
  tablet, app window; Tabs, split, picture-in-picture; Site info, permissions, sign-in; Errors and loading; Tools,
  sidebars, keyboard; Settings; Phone). After: 107 frames on 6 pages: **Window** (30), **Page states and prompts**
  (24), **Tools** (13), **Settings and library** (14), **Phone** (20), **Web view in other apps** (6).
- The browser frames that are still on the shared canvas `q3ckG9DfPo` are untouched (Main: keep the originals there
  until Kieran has looked at the per-app canvases). They carry no comments.
- One frame per screen and state. Light variants were dropped except one per set (Window 02, Phone 01); dark is the
  default everywhere else. Transient states (drag in progress, resize in progress, hover peeks) and states that only
  change words on the same template (error pages) were folded into the frame that shows the pattern.
- Every frame now uses one kit (`frames/apps-browser/_parts.mjs`, `_pages.mjs`): the same strip, toolbar, address
  trail, three-dot main menu, popovers, sheets and status pages.

## Nothing is lost

- Before-shots of all 203 frames: `out/apps-browser/before/` (local, not committed).
- Source of every removed frame: bingux-doop commit `2d5b127` (`frames/apps-browser/` as it was). Check it out in a
  scratch worktree and push one module to bring a frame back.

## Removed frames and why

"→ X" means the kept frame X now shows that screen or state. "Dropped" means no frame shows it any more; the reason
follows.

### Standard layout (22)

| Removed                                          | Now                                                                                                                 |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| Standard / 01 First run, dark                    | → Window / 04 First run                                                                                             |
| Standard / 02 First run, light                   | Dropped: light duplicate                                                                                            |
| Standard / 03 New tab, dark                      | → Window / 03 New tab                                                                                               |
| Standard / 04 New tab, light                     | Dropped: light duplicate                                                                                            |
| Standard / 05 Page, dark                         | → Window / 01 Page                                                                                                  |
| Standard / 06 Page, light                        | → Window / 02 Page, light                                                                                           |
| Standard / 07 Loading                            | → Window / 05 Loading                                                                                               |
| Standard / 08 Typing in the address field, dark  | → Window / 06 Typing in the address field                                                                           |
| Standard / 09 Typing in the address field, light | Dropped: light duplicate                                                                                            |
| Standard / 10 Main menu, Library                 | → Window / 07 Main menu                                                                                             |
| Standard / 11 Main menu, searching               | → Window / 08 Main menu, searching                                                                                  |
| Standard / 12 Downloads                          | → Window / 09 Downloads                                                                                             |
| Standard / 13 Site information                   | → Window / 10 Site information                                                                                      |
| Standard / 14 Site information, light            | Dropped: light duplicate                                                                                            |
| Standard / 15 Private window, new tab            | → Window / 16 Private window, new tab                                                                               |
| Standard / 16 Private window, page               | → Window / 15 Private window                                                                                        |
| Standard / 17 Work profile window                | Dropped: no profile marker in the header any more (avatar comment); profiles are in Settings / 10 and the main menu |
| Standard / 18 Narrow window, 760                 | → Window / 17 Medium window                                                                                         |
| Standard / 19 Zoom, 110% for this site           | → Window / 11 Zoom for this site (smaller popover, new icon, no avatar: three comments)                             |
| Standard / 20 Translating                        | → Tools / 04 Translated page                                                                                        |
| Standard / 21 One tab, dark                      | → Window / 18 Tabs at their natural width                                                                           |
| Standard / 22 Extensions and a hover tip         | → Window / 12 Extensions menu                                                                                       |

### Tabs, split, picture-in-picture (15)

| Removed                                     | Now                                                                               |
| ------------------------------------------- | --------------------------------------------------------------------------------- |
| Tabs / 01 Tab states, dark                  | → Window / 19 Tab states                                                          |
| Tabs / 02 Tab states, light                 | Dropped: light duplicate                                                          |
| Tabs / 03 Three tabs at their natural width | → Window / 18 Tabs at their natural width                                         |
| Tabs / 04 Sixteen tabs, favicons only       | → Window / 20 Many tabs, favicons only                                            |
| Tabs / 05 Forty tabs, the strip scrolls     | → Window / 21 Forty tabs, the strip scrolls                                       |
| Tabs / 06 All tabs list                     | → Window / 22 All tabs list                                                       |
| Tabs / 07 Tab groups                        | → Window / 23 Tab groups                                                          |
| Tabs / 08 Tab group menu                    | Dropped: same anchored menu as Window / 24 Tab menu                               |
| Tabs / 09 Tab menu                          | → Window / 24 Tab menu                                                            |
| Tabs / 10 Dragging a tab into a group       | Dropped: transient drag state                                                     |
| Tabs / 11 Hover preview                     | Dropped: a setting (Settings / 03 Tabs); the preview is the standard tooltip card |
| Split / 01 Two pages side by side           | → Tools / 09 Split view (each pane has its own toolbar: comment)                  |
| Split / 02 Resizing the split               | Dropped: transient                                                                |
| Split / 03 Picture-in-picture               | → Tools / 10 Picture-in-picture                                                   |
| Split / 04 Link preview                     | Dropped: not asked for; a Zen idea with no Bingux use yet                         |

### Vertical, compact, tablet, app window (31)

| Removed                                           | Now                                                                                 |
| ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Vertical / 01 Tabs on the side, dark              | → Window / 25 Tabs on the side                                                      |
| Vertical / 02 Tabs on the side, light             | Dropped: light duplicate                                                            |
| Vertical / 03 Collapsed rail, dark                | → Window / 26 Tabs on the side, collapsed rail                                      |
| Vertical / 04 Collapsed rail, light               | Dropped: light duplicate                                                            |
| Vertical / 05 Collapsed rail, peeking             | Dropped: transient hover state                                                      |
| Vertical / 06 Searching tabs                      | Dropped: tab search is the Tabs scope of the address card (Window / 06)             |
| Vertical / 07 Forty tabs                          | Dropped: the sidebar scrolls like any list                                          |
| Vertical / 08 Split view                          | → Tools / 09 Split view                                                             |
| Vertical / 09 Private window                      | → Window / 15 Private window                                                        |
| Vertical / 10 Medium window, rail                 | → Window / 26 Tabs on the side, collapsed rail                                      |
| Compact / 01 One row, dark                        | → Window / 27 One row                                                               |
| Compact / 02 One row, light                       | Dropped: light duplicate                                                            |
| Compact / 03 With bookmarks bar, dark             | → Window / 28 One row with bookmarks bar                                            |
| Compact / 04 With bookmarks bar, light            | Dropped: light duplicate                                                            |
| Compact / 05 Typing in the address field          | → Window / 06 (the same address card)                                               |
| Compact / 06 Bookmark folder open                 | Dropped: a standard anchored menu                                                   |
| Compact / 07 Tabs overflow                        | → Window / 20 and 21 (same overflow rules)                                          |
| Compact / 08 Narrow window, 900                   | → Window / 17 Medium window                                                         |
| Tablet / 01 Tablet, light                         | Dropped: no separate tablet layout; size decides (Window / 17 medium, Phone frames) |
| Tablet / 02 Tablet, dark                          | Dropped: as above                                                                   |
| Tablet / 03 Tab overview                          | → Phone / 07 Tabs, grid                                                             |
| Tablet / 04 Split view                            | → Tools / 09 Split view                                                             |
| Tablet / 05 Portrait, medium width                | → Window / 17 Medium window                                                         |
| App window / 01 Installed site, light             | → Window / 29 Installed site                                                        |
| App window / 02 Installed map, glass header       | Dropped: Window / 30 shows the glass header                                         |
| App window / 03 Glass header, reduce transparency | Dropped: the opaque fallback is the plain header of Window / 29                     |
| App window / 04 App menu                          | Dropped: the same anchored menu as the main menu                                    |
| App window / 05 Install as an app                 | Dropped: a main-menu command with a standard confirm dialog                         |
| App window / 06 Focus mode                        | → Window / 30 Focus mode, header shown                                              |
| App window / 07 Focus mode, header shown          | → Window / 30                                                                       |
| App window / 08 Full screen video                 | Dropped: no chrome to design                                                        |

### Site info, permissions, sign-in (19)

| Removed                                     | Now                                                                            |
| ------------------------------------------- | ------------------------------------------------------------------------------ |
| Security / 01 Trackers on this page         | → Window / 13 uBlock Origin                                                    |
| Security / 02 Not secure, typing a password | → Page states / 06 Not secure page, site information                           |
| Security / 03 Not secure, site info         | → Page states / 06 (red solid "Not secure" chip, proper icon: comment)         |
| Security / 04 Secure with insecure images   | Dropped: mixed content shows the same "Not secure" chip                        |
| Security / 05 Certificate expired           | → Page states / 04 Certificate expired                                         |
| Security / 06 Certificate expired, light    | → Page states / 04 (solid icon, rewritten page: two comments)                  |
| Security / 07 Certificate details           | → Page states / 05 Certificate details (macOS-style chain and fields: comment) |
| Prompts / 01 Location, asked                | → Prompts / 01 Location (redesigned: comment)                                  |
| Prompts / 02 Camera and microphone, asked   | → Prompts / 02 Camera and microphone                                           |
| Prompts / 05 Screen sharing, choose a tab   | → Prompts / 05 Screen sharing (redesigned: comment)                            |
| Prompts / 07 Sign in with a passkey         | → Prompts / 07 Passkey (rebuilt: comment "this looks broken")                  |
| Prompts / 08 Autofill, address              | → Prompts / 08 Autofill (key hint removed: comment)                            |
| Prompts / 09 Autofill, card                 | Dropped: the same list as Prompts / 08                                         |
| Prompts / 10 Page says, alert               | → Prompts / 09 Page says, alert                                                |
| Prompts / 11 Leave this page                | → Prompts / 12 Leave this page                                                 |
| Prompts / 12 Sign in to a site              | → Prompts / 13 Sign in to a site                                               |
| Prompts / 13 Pop-up blocked                 | → Prompts / 14 Pop-up blocked                                                  |
| Prompts / 14 Page asks, confirm             | → Prompts / 10 Page asks, confirm                                              |
| Prompts / 15 Page asks for text             | → Prompts / 11 Page asks for text                                              |

### Errors and loading (14)

| Removed                                     | Now                                                                            |
| ------------------------------------------- | ------------------------------------------------------------------------------ |
| Errors / 01 Offline, dark                   | → Page states / 01 Offline                                                     |
| Errors / 02 Offline, light                  | Dropped: light duplicate                                                       |
| Errors / 03 Can’t find that site            | → Page states / 02 Can’t find that site                                        |
| Errors / 04 The site took too long          | Dropped: the Page states / 02 template with other words (DESIGN.md lists them) |
| Errors / 05 The site refused the connection | Dropped: as above                                                              |
| Errors / 06 No secure connection            | → Page states / 03 No secure connection                                        |
| Errors / 07 Deceptive site                  | → Page states / 07 Deceptive site                                              |
| Errors / 08 Blocked by a filter list        | Dropped: the status-page template with other words                             |
| Errors / 09 Tab stopped                     | → Page states / 08 Tab stopped                                                 |
| Errors / 10 Tab ran out of memory           | Dropped: Page states / 08 with other words                                     |
| Errors / 11 Browser engine stopped          | → Page states / 09 Browser engine stopped                                      |
| Errors / 12 Engine starting again           | → Page states / 10 Engine starting again                                       |
| Errors / 13 File can’t be opened            | Dropped: the status-page template with other words                             |
| Errors / 14 Restoring tabs after a restart  | → Page states / 10 Engine starting again                                       |

### Tools, sidebars, keyboard (34)

| Removed                                          | Now                                                                             |
| ------------------------------------------------ | ------------------------------------------------------------------------------- |
| Tools / 02 Find in page, no matches              | → Tools / 01 Find in page (X button instead of Done: comment)                   |
| Tools / 03 Reader view, light                    | → Tools / 03 Reader view                                                        |
| Tools / 04 Reader view, sepia, type              | Dropped: a type popover like Window / 11                                        |
| Tools / 05 Reader view, dark, listening          | Dropped: not asked for                                                          |
| Tools / 06 Translated page                       | → Tools / 04 Translated page (bar takes the toolbar's tone: comment)            |
| Tools / 07 Print                                 | → Tools / 05 Print (a popup dialog: comment)                                    |
| Tools / 08 Context menu, link                    | → Tools / 07 Context menu, link                                                 |
| Tools / 09 Context menu, image and selection     | Dropped: the same menu                                                          |
| Tools / 10 Developer tools                       | → Tools / 08 Developer tools (Firefox DevTools layout: comment)                 |
| Tools / 11 Developer tools, light                | Dropped: light duplicate                                                        |
| Tools / 12 Share                                 | → Tools / 06 Share (trimmed, solid phone icon: two comments)                    |
| Tools / 13 Find in page, options                 | → Tools / 02 Find options                                                       |
| Sidebars / 01 Bookmarks                          | → Tools / 12 Side panel, bookmarks                                              |
| Sidebars / 02 History                            | Dropped: the side panel's icon rail switches to it; Library / 01 is the manager |
| Sidebars / 03 Reading list                       | → Tools / 11 Side panel, reading list (icon rail, after Vivaldi: comment)       |
| Sidebars / 04 Notes                              | Dropped: not asked for                                                          |
| Sidebars / 05 Library, history                   | → Library / 01 History (a real manager: comment)                                |
| Sidebars / 06 Library, bookmarks                 | → Library / 02 Bookmarks                                                        |
| Sidebars / 07 Library, downloads                 | → Library / 03 Downloads                                                        |
| Sidebars / 08 Library, passwords                 | → Settings / 08 Passwords and passkeys                                          |
| Keyboard / 01 Shortcuts                          | → Tools / 13 Keyboard shortcuts                                                 |
| Keyboard / 02 Focus on the address trail         | Dropped: the design system's focus ring; PLAN.md describes the order            |
| Keyboard / 03 Focus on a tab                     | Dropped: as above                                                               |
| Keyboard / 04 Focus in the page, F6 to leave     | Dropped: as above                                                               |
| Extensions / 01 uBlock Origin                    | → Window / 13 uBlock Origin                                                     |
| Extensions / 02 uBlock Origin, off for this site | Dropped: a switch state of Window / 13                                          |
| Extensions / 03 uBlock Origin, element picker    | Dropped: the extension's own UI                                                 |
| Extensions / 04 Bitwarden, logins for this site  | → Window / 14 Bitwarden                                                         |
| Extensions / 05 Bitwarden, generator             | Dropped: the extension's own UI                                                 |
| Extensions / 06 Bitwarden, locked                | Dropped: the extension's own UI                                                 |
| Extensions / 07 Bitwarden, fill in the field     | Dropped: the extension's own UI                                                 |
| Extensions / 08 Extensions menu                  | → Window / 12 Extensions menu (redesigned: comment)                             |
| Extensions / 09 Extensions menu, unpinned        | Dropped: a pin state of Window / 12                                             |
| Extensions / 10 Profile                          | Dropped: comment "this menu sucks"; the profile is a row in the main menu       |

### Settings (14)

| Removed                                   | Now                                                            |
| ----------------------------------------- | -------------------------------------------------------------- |
| Settings / 03 Appearance, dark            | Dropped: Settings / 02 shows Appearance                        |
| Settings / 04 Tabs                        | → Settings / 03 Tabs                                           |
| Settings / 05 Search                      | → Settings / 04 Search                                         |
| Settings / 06 Privacy and security        | → Settings / 05 Privacy and security                           |
| Settings / 07 Privacy and security, light | Dropped: light duplicate                                       |
| Settings / 08 Sites                       | → Settings / 06 Sites                                          |
| Settings / 09 Site details                | → Settings / 07 Site details (rebuilt: comment "this sucks..") |
| Settings / 10 Passwords and passkeys      | → Settings / 08 Passwords and passkeys                         |
| Settings / 11 Extensions                  | → Settings / 09 Extensions                                     |
| Settings / 12 Sync                        | Dropped: the same row pattern; listed in the sidebar           |
| Settings / 13 Profiles                    | → Settings / 10 Profiles                                       |
| Settings / 14 Import from another browser | → Window / 04 First run (import card)                          |
| Settings / 15 Searching settings          | Dropped: the Settings app's search pattern, shared             |
| Settings / 16 About Browser               | → Settings / 11 About Browser                                  |

### Phone (37)

| Removed                                | Now                                                            |
| -------------------------------------- | -------------------------------------------------------------- |
| Phone / 06 New tab, dark               | Dropped: dark duplicate (Phone / 05)                           |
| Phone / 07 Editing the address         | → Phone / 06 Typing, suggestions                               |
| Phone / 08 Typing, suggestions         | → Phone / 06                                                   |
| Phone / 09 Typing, dark                | Dropped: dark duplicate                                        |
| Phone / 10 Tabs, grid                  | → Phone / 07 Tabs, grid                                        |
| Phone / 11 Tabs, list                  | Dropped: one tab overview                                      |
| Phone / 12 Tabs, groups                | → Phone / 07 (groups are sections of the grid)                 |
| Phone / 13 Tab group, sheet            | Dropped: a standard sheet                                      |
| Phone / 14 Private tabs                | → Phone / 08 Private tabs                                      |
| Phone / 15 Private tab, new            | Dropped: Window / 16 on a phone column                         |
| Phone / 16 Searching tabs              | Dropped: search field of Phone / 07                            |
| Phone / 17 Menu, sheet                 | → Phone / 09 Menu, sheet                                       |
| Phone / 18 Menu, sheet, dark           | Dropped: dark duplicate                                        |
| Phone / 19 Site information, sheet     | → Phone / 10 Site information, sheet                           |
| Phone / 20 Location asked, sheet       | → Phone / 11 Location asked, sheet                             |
| Phone / 21 Camera asked, sheet         | Dropped: the same sheet as Phone / 11                          |
| Phone / 22 Notifications asked, sheet  | Dropped: as above                                              |
| Phone / 23 Long press on a link        | → Phone / 12 Long press on a link                              |
| Phone / 24 Long press on an image      | Dropped: the same panel                                        |
| Phone / 25 Reader view                 | → Phone / 13 Reader view                                       |
| Phone / 26 Reader, type sheet          | Dropped: a standard sheet                                      |
| Phone / 27 Find in page                | → Phone / 14 Find in page                                      |
| Phone / 28 Download running            | → Phone / 15 Downloads                                         |
| Phone / 29 Downloads                   | → Phone / 15                                                   |
| Phone / 30 History                     | Dropped: Library pages push on a phone with the same list rows |
| Phone / 31 Bookmarks                   | Dropped: as above                                              |
| Phone / 32 Settings                    | → Phone / 16 Settings                                          |
| Phone / 33 Profiles, sheet             | Dropped: profile row in Phone / 09 and Phone / 16              |
| Phone / 34 uBlock Origin, sheet        | Dropped: extension panels become sheets by the general rule    |
| Phone / 35 Bitwarden over the keyboard | Dropped: as above                                              |
| Phone / 36 Bitwarden, sheet            | Dropped: as above                                              |
| Phone / 37 Page says, alert            | → Phone / 17 Page says, alert                                  |
| Phone / 38 Offline                     | → Phone / 18 Offline                                           |
| Phone / 39 Can’t find that site        | Dropped: the status-page template                              |
| Phone / 40 Certificate expired         | → Phone / 19 Certificate expired                               |
| Phone / 41 Not secure page             | Dropped: the red chip in Phone / 19's toolbar                  |
| Phone / 42 Landscape                   | → Phone / 20 Landscape                                         |

## Edits to kept frames during the clean-up

- Window / 29 Installed site: the avatar left its header (same comment as Standard / 19).
- Embedded / 04: dark like the other five Embedded frames; the designer's note in its footer is gone (the footer
  says what the user needs, as in the other four).
- Every page re-laid out in name order, four columns.
