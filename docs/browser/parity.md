# Browser: parity with the Doop design

Audit of 2026-11-26 on `feat/browser` (HEAD `96178968`). One row per frame of the canvas "Bingux: Browser" (107
frames), the gap that remains, and what must be answered before it is built. Nothing in this file changes code.

**Paths.** The browser is in the `apps/` layout now (branch `feat/apps`): `packages/bingux-browser` is `apps/browser`,
`packages/bingux-ui` is `crates/ui` and `packages/bingux-taxonomy` is `apps/taxonomy`. In this file `B/` is
`apps/browser/src` and `UI/` is `crates/ui/src`; line numbers are from the audit on the old layout.

## The bar

Kieran's rule: parity means each feature is designed and built properly, production ready. A screen that looks like
the frame is not parity.

- **yes**: behaviour is real, kept, isolated, and has its error and empty states.
- **partial**: UI exists, but anything behind it is faked, held in memory, unsaved, or missing a state.
- **no**: there is no UI for it.

Result: **0 yes, 60 partial, 47 no**. No frame is yes. The nearest are Tools 01 (find) and Window 05 (loading): they work on the
real engine but miss states of the frame.

| Group       | Frames | yes | partial | no  |
| ----------- | ------ | --- | ------- | --- |
| Window      | 30     | 0   | 19      | 11  |
| Tools       | 13     | 0   | 4       | 9   |
| Page states | 10     | 0   | 8       | 2   |
| Prompts     | 14     | 0   | 9       | 5   |
| Library     | 3      | 0   | 0       | 3   |
| Settings    | 11     | 0   | 3       | 8   |
| Phone       | 20     | 0   | 17      | 3   |
| Embedded    | 6      | 0   | 0       | 6   |

## How this was checked

- **Frames.** Rendered from the `.mjs` sources in `bingux-doop/frames/apps-browser/` (not the old PNGs) with
  system Chrome, one PNG per frame. Script: `render-frames.mjs`. The sources carry the consolidated names: 30 Window,
  10 Page states, 14 Prompts, 13 Tools, 3 Library, 11 Settings, 20 Phone, 6 Embedded.
- **Browser.** `bingux-browser --features snapshot --snapshot DIR` over the test double, at 1100x700, 760x760,
  390x800 and 844x390. These are 36 scenarios. The pages in them are the double's wireframes, so a row about page
  content cannot be judged from them.
- **Behaviour.** Read from code. A claim about the real engine rests on the code path and on the earlier real-engine
  runs recorded in `design.md` (find, download control). I ran no real-engine session for this audit.
- **Engine.** `~/dev/vivarium` at `729d5ea`: `docs/research/api-parity.md`, `TODO.md`, the headers and the Rust
  crate. There is no Gecko checkout on this machine, so Gecko source paths are quoted from the rows of
  `api-parity.md`, not opened.

Evidence is in `~/.cache/a37-box-scripts/browser-parity-2026-11-26/`:

| Path                                       | Holds                                                                                      |
| ------------------------------------------ | ------------------------------------------------------------------------------------------ |
| `frames/<group>-<nn>-<title>.png`          | The 107 Doop frames, plus `frames/html/` and `index.json`                                  |
| `wide/`, `medium/`, `phone/`, `landscape/` | Browser snapshots, one PNG per scenario                                                    |
| `pairs/`                                   | Frame left, browser right, named `w01__page.png`, `ps04__cert.png`, `phone-sheet-1..5.png` |
| `sheets/`                                  | Contact sheets of frames that have no browser counterpart                                  |

## Reading the tables

| Column | Meaning                                                                                                                              |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| Impl   | yes, partial or no, against the bar above                                                                                            |
| Where  | File and function, or the snapshot scenario (`name`) that renders it. `none` means no code                                           |
| Gap    | S: small UI change. M: several days or a new model. L: a feature with its own store or engine work                                   |
| Engine | `none`; **U** needs a Vivarium primitive; **O** is a browser-side overlay; `wire` means the ABI has it and bingux-ui does not use it |

Every Engine entry is argued in "Engine-dependent gaps" below. The rule applied: a primitive goes upstream to
Vivarium only when many kinds of embedder use it. Browser policy and layout stay here.

## Window (30)

| #   | Frame                            | Impl    | Where                                                                                       | What differs                                                                                                                                                  | Gap | Engine                                                                       |
| --- | -------------------------------- | ------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- | ---------------------------------------------------------------------------- |
| 01  | Page                             | partial | B/ui/app.rs:Content, ui/toolbar.rs:Toolbar, ui/strip.rs:strip; `page` (pairs/w01__page.png) | Address is a plain text field: no favicon, bold domain, path crumbs or reader chip. No extension buttons. Tab icons are one generic glyph.                    | M   | U: favicon bytes, registrable domain; extension host surface                 |
| 02  | Page, light                      | partial | as Window 01; scheme follows the system (ui/app.rs PageEnvironment); no light scenario      | Same gaps as Window 01. No bookmark star or saved chip. Light rendering not checked: the snapshot list has no light scenario.                                 | S   | none                                                                         |
| 03  | New tab                          | partial | B/ui/pages.rs:new_tab; `newtab` (pairs/w03__newtab.png)                                     | One status page plus five in-memory recent rows. No pinned-site grid, Continue list, This-week tracker stats or Privacy report link.                          | L   | U: tracker stats (api-parity C4); O: pinned sites, reading list              |
| 04  | First run                        | no      | none: no first-run state anywhere in B/                                                     | No ready card, no import from Firefox/Chromium, no pinned-sites hint, no Worth-knowing rows, no default-browser prompt.                                       | L   | O: reads another browser's profile files; no Gecko API in an embedded engine |
| 05  | Loading                          | partial | B/ui/page.rs:LoadProgress, ui/strip.rs:status_of, ui/toolbar.rs:reload_button; `loading`    | Progress line, tab spinner and Stop work. Address field keeps its search icon (no spinner in the trail). No load status line.                                 | S   | none                                                                         |
| 06  | Typing in the address field      | partial | B/ui/address.rs:AddressBar, model/suggest.rs; `suggestions` (pairs/w06__suggestions.png)    | No scope chips, section headers, Switch-to-tab pill, key-hint footer, inline completion, bookmarks, settings rows or page-text matches. History is in memory. | L   | U: URL fixup/parse, frecency if Places runs; O: bookmark/history store       |
| 07  | Main menu                        | partial | B/ui/menus.rs:main_menu; `menu` (pairs/w07__menu.png)                                       | No command search, zoom row, New window, Profile, Translate, Save page as, Share, Library submenu, Extensions, Developer tools or Help rows.                  | M   | U: save page, translate; devtools wired on engine main                       |
| 08  | Main menu, searching             | no      | none                                                                                        | No typed menu search: no commands, settings or open-tab results from the menu field.                                                                          | M   | none                                                                         |
| 09  | Downloads                        | partial | B/ui/popovers.rs:download_list, model/downloads.rs; `downloads` (pairs/w09__downloads.png)  | Pause, resume, cancel, retry, clear work. No sections, file-type icons, Show in folder, blocked row or footer link. List is memory only.                      | M   | U: MIME/file-type icons; blocked downloads need reputation (not planned)     |
| 10  | Site information                 | partial | B/ui/popovers.rs:page_info; `info`, `info-plain` (pairs/w10__info.png)                      | Hero row, per-kind Allow/Ask/Block pickers, tracker row, site-data row and Site settings footer are missing. Only remembered permissions list, with Reset.    | M   | U: permission store (PM2/PM3), tracker stats (C4), site-data size (C13/ST1)  |
| 11  | Zoom for this site               | partial | B/ui/toolbar.rs:zoom_chip, model/zoom.rs; `zoom` (pairs/w11__zoom.png)                      | Chip only resets to 100%. No popover with minus, plus and Reset. Zoom belongs to the tab, not the site; nothing remembers it.                                 | M   | U: per-site zoom via content prefs, or O: browser store                      |
| 12  | Extensions menu                  | no      | none                                                                                        | No Extensions menu, pin toggles, Manage link, per-site Allow for an extension or extension list at all.                                                       | L   | U: extension host surface for actions (vivarium TODO, Extension API row)     |
| 13  | uBlock Origin                    | no      | none                                                                                        | No uBlock Origin button or popup: no per-site switch, counters or zapper. No extension installs in the browser.                                               | L   | U: extension popups as sessions; O: bundle and update uBlock                 |
| 14  | Bitwarden                        | no      | none                                                                                        | No Bitwarden button or popup; no fill-in-field from an extension.                                                                                             | L   | U: as Window 13                                                              |
| 15  | Private window                   | partial | B/main.rs (--private), model/browser/mod.rs:Options, ui/pages.rs:new_tab; `private`         | Private is a separate process with a temp profile. No violet tones, no Private chip, tab icon is a shield. No page-state scenario.                            | M   | none: private identity exists                                                |
| 16  | Private window, new tab          | partial | B/ui/pages.rs:new_tab private branch; `private` (wide/private.png)                          | One-paragraph explainer. No What-is-different rows, no Private chip, no violet tone, no Ctrl+Shift+P hint.                                                    | S   | none                                                                         |
| 17  | Medium window                    | partial | same layout as wide; `page` at 760x760 (medium/page.png, pairs/w17__medium-page.png)        | No medium rules: chips and crumbs do not condense, tabs do not shrink to favicons first. The window simply gets narrower.                                     | S   | none                                                                         |
| 18  | Tabs at their natural width      | partial | UI/controls/tabs.rs:TabStrip via B/ui/strip.rs; `tabs`                                      | Floating tiles up to a cap work. Generic icons instead of site favicons. Three-tab natural width not rendered: no scenario.                                   | S   | U: favicon bytes                                                             |
| 19  | Tab states                       | partial | B/ui/strip.rs:status_of; `tabs` (pairs/w19__tabs.png)                                       | Loading, attention, failed, muted, audio and pinned marks exist. No asleep, in-group or camera-in-use states. Audio and mute never fire on the real engine.   | M   | U: audible/mute (ABI lacks); sleeping tabs: not checked in engine            |
| 20  | Many tabs, favicons only         | partial | UI/controls/tabs.rs (shrink to icon, then scroll)                                           | Component documents the rule; no browser scenario has 16 tabs, so the look was not checked.                                                                   | S   | none                                                                         |
| 21  | Forty tabs, the strip scrolls    | partial | UI/controls/tabs.rs overflow list button                                                    | Scroll and list button exist in bingux-ui. Not checked: no 40-tab scenario. The +N count chip is not confirmed.                                               | S   | none                                                                         |
| 22  | All tabs list                    | partial | UI/controls/tabs.rs list menu; `tab-list` is the phone sheet, not this                      | No All-tabs popover with count, off-strip section, search or Show-in-overview link. The scenario shows the phone Tabs sheet at desktop size.                  | M   | none                                                                         |
| 23  | Tab groups                       | no      | none: no group model in B/model/tabs.rs                                                     | No tab groups: no colour, name, collapse, group chip, Add to group, saved groups.                                                                             | L   | none: groupId is host-owned                                                  |
| 24  | Tab menu                         | partial | B/ui/menus.rs:tab_items; `tab-menu` (pairs/w24__tab-menu.png)                               | Reload, Duplicate, Pin, Mute, Close others and Close right exist. Missing: New tab right, Add to group, Sleep, Move to window, Copy link, hints.              | S   | none                                                                         |
| 25  | Tabs on the side                 | no      | none: no vertical tab component in UI/                                                      | No Tabs on the side: no sidebar, search, pinned grid, groups as sections, no Layout setting.                                                                  | L   | none                                                                         |
| 26  | Tabs on the side, collapsed rail | no      | none                                                                                        | No collapsed rail.                                                                                                                                            | M   | none                                                                         |
| 27  | One row                          | no      | none                                                                                        | No One row layout: nav, address, tabs and window buttons stay in two rows.                                                                                    | M   | none                                                                         |
| 28  | One row with bookmarks bar       | no      | none: no bookmarks anywhere                                                                 | No bookmarks bar, folders, overflow or All-bookmarks item.                                                                                                    | L   | O: bookmark store                                                            |
| 29  | Installed site                   | no      | none                                                                                        | No Installed site window: no header with site icon, no install command, no per-site app mode.                                                                 | L   | O: web app manifest read through the page                                    |
| 30  | Focus mode, header shown         | partial | B/ui/app.rs (fullscreen hides chrome), Command::Fullscreen; `fullscreen`                    | Window fullscreen hides chrome but is not Focus mode: no top-edge reveal, no Leave-focus-mode bar, no setting.                                                | S   | none                                                                         |

## Page states (10)

| #   | Frame                             | Impl    | Where                                                                                  | What differs                                                                                                                 | Gap | Engine                                                     |
| --- | --------------------------------- | ------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --- | ---------------------------------------------------------- |
| 01  | Offline                           | partial | B/model/failure.rs:for_load_error, ui/pages.rs:failed; `error`                         | Text and Try again exist. No Saved-for-offline list, no Network settings button, technical code is always shown, not folded. | S   | U: typed error category                                    |
| 02  | Can’t find that site              | partial | same; `error` (pairs/ps01__error.png)                                                  | No Did-you-mean from history, no Search-for action, no folded technical details with DNS note.                               | S   | none                                                       |
| 03  | No secure connection              | no      | none: pinned scenario shows the certificate page instead                               | No HTTPS-first fallback page: no Continue-to-site, no Do-not-ask-again, no Not-secure chip. Engine error not mapped.         | M   | not checked: what the engine sends for HTTPS-only failures |
| 04  | Certificate expired               | partial | B/ui/pages.rs:cert_error; `cert` (pairs/ps04__cert.png)                                | One paragraph, no red Not-secure chip in the field, no Proceed link on the page, no technical details, no clock check.       | S   | none                                                       |
| 05  | Certificate details               | partial | B/ui/pages.rs:cert_error details dialog; `cert-details` (pairs/ps05__cert-details.png) | Dialog lists site, problem, issuer, fingerprint. No chain, subject, validity, key fields, no Copy details.                   | L   | U: certificate chain and fields from the engine            |
| 06  | Not secure page, site information | partial | B/model/security.rs:summarize; `info-plain` (pairs/ps06__info-plain.png)               | Toolbar shows an icon, not a solid red Not-secure chip. Card lacks the warning pill, permission pickers and tracker rows.    | M   | none                                                       |
| 07  | Deceptive site                    | no      | none                                                                                   | No deceptive-site page, no list, no Go-back-to-safety.                                                                       | M   | U: navigation veto hook (N11); O: local list               |
| 08  | Tab stopped                       | partial | B/ui/pages.rs:crashed; `crash` (pairs/ps08__crash.png)                                 | Reload tab exists. No Close tab, no typed-text-restored note, no crash report link, no folded code.                          | S   | U: session restore (H1)                                    |
| 09  | Browser engine stopped            | partial | B/ui/pages.rs:engine_failed; `engine-failed` (pairs/ps09__engine-failed.png)           | Replaces the whole window: strip hidden, no tab count, no crash-dump path or Show report, no crashed tab icons.              | S   | none                                                       |
| 10  | Engine starting again             | partial | B/ui/page.rs:Waiting; `engine-restart` shows the end state                             | Transient Starting-again page with spinner and tab count is not drawn; each tab shows a bare Starting-the-engine page.       | S   | none                                                       |

## Prompts (14)

| #   | Frame                          | Impl    | Where                                                                      | What differs                                                                                                                          | Gap | Engine                                         |
| --- | ------------------------------ | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | --- | ---------------------------------------------- |
| 01  | Location                       | partial | B/ui/prompts.rs:PermissionBanner; `permission` (pairs/p01__permission.png) | A banner under the toolbar, not a card from the field chip. No Precise-location switch. Decisions not kept across restarts.           | M   | U: permission store                            |
| 02  | Camera and microphone          | partial | same banner                                                                | Text only: no camera preview, device pickers or level meter.                                                                          | L   | U: device list and preview (PM4)               |
| 03  | Camera in use                  | no      | none                                                                       | No in-use indicator on the tab, in the field, or a Stop/Mute popover.                                                                 | M   | U: recording-status event (PM4)                |
| 04  | Notifications, quietly blocked | no      | none                                                                       | No quiet blocking: nothing blocks a notification request that fires on load or shows a Keep-blocking card.                            | M   | none: browser policy over PM1                  |
| 05  | Screen sharing                 | partial | B/model/permissions.rs label only                                          | Screen kind has a label. No tab/window/screen picker with thumbnails, no share-sound switch.                                          | L   | U: capture request and portal (PM5)            |
| 06  | Save password                  | no      | none                                                                       | No save-password card: no store, no reveal, no Not-now.                                                                               | L   | U: login-save request (A1); store decision A2  |
| 07  | Passkey                        | no      | none                                                                       | No passkey dialog.                                                                                                                    | L   | U: WebAuthn prompt request (A6)                |
| 08  | Autofill                       | no      | none                                                                       | No autofill list for addresses, cards or logins.                                                                                      | L   | U: autocomplete request (D15); O: store        |
| 09  | Page says, alert               | partial | B/ui/prompts.rs:show_dialog; `dialog` (pairs/p09__dialog.png)              | Centred bingux-ui dialog, modal to the window. Design hangs it from the page top, modal to the tab, with a Do-not-show-more checkbox. | M   | none                                           |
| 10  | Page asks, confirm             | partial | same                                                                       | As Prompts 09 (Cancel and OK exist).                                                                                                  | S   | none                                           |
| 11  | Page asks for text             | partial | same                                                                       | As Prompts 09 (text field exists).                                                                                                    | S   | none                                           |
| 12  | Leave this page                | partial | B/ui/prompts.rs:show_dialog BeforeUnload                                   | Stay and Leave exist. Generic text; page-specific detail from the design is not available. Same placement gap as Prompts 09.          | S   | none                                           |
| 13  | Sign in to a site              | partial | B/ui/prompts.rs:show_auth; `auth` (pairs/p13__auth.png)                    | Name, password and reveal exist. No hang-from-top placement, no secure-connection note, no save-to-manager.                           | S   | none                                           |
| 14  | Pop-up blocked                 | partial | B/model/browser/lifecycle.rs:accept_new_tab (toast); `popup`               | Toast says Blocked a pop-up, with no Show-it action and no chip. The user-gesture test never fires: the engine sends none.            | S   | U: gesture and disposition on new window (N13) |

## Tools (13)

| #   | Frame                    | Impl    | Where                                                                           | What differs                                                                                                                   | Gap | Engine                                        |
| --- | ------------------------ | ------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --- | --------------------------------------------- |
| 01  | Find in page             | partial | B/ui/find.rs:FindBar, UI/controls/find_card.rs; `find` (pairs/t01__find.png)    | Card, count, steps and X work on the real engine. No match ticks on the scrollbar. Options sit inline, not behind a menu.      | S   | not checked: match positions                  |
| 02  | Find options             | partial | B/ui/find.rs options                                                            | Match case and Highlight all exist. Whole words is in the model but not shown. No Match diacritics.                            | S   | none                                          |
| 03  | Reader view              | no      | none                                                                            | No reader button or reader view: no type popover, themes, read-aloud.                                                          | L   | U: readerable and article transform (L15)     |
| 04  | Translated page          | no      | none                                                                            | No translate chip, translation bar or language choices.                                                                        | L   | decision open: Gecko translations need models |
| 05  | Print                    | no      | B/model/browser/commands.rs:Command::Print (a notice)                           | Print says it is not available. No print dialog, preview, printer or page options, no Save as PDF.                             | L   | wire existing print_pdf; O: printers, preview |
| 06  | Share                    | no      | none                                                                            | No Share menu: copy link, nearby devices, QR, email.                                                                           | M   | O: portal share                               |
| 07  | Context menu, link       | partial | B/ui/menus.rs:context_items; `context` (pairs/t07__context.png)                 | Open in new tab and Copy address exist. Missing beside-tab, private window, preview, reading list, clean link, Inspect.        | S   | none                                          |
| 08  | Developer tools          | no      | none                                                                            | No developer tools panel: docking, tabs, resize.                                                                               | M   | wire open_devtools (engine main); O: docking  |
| 09  | Split view               | no      | none: no split model or pane header                                             | No split view: no panes, per-pane toolbar, ratio gutter, focus mark.                                                           | L   | none: splitViewId is host-owned               |
| 10  | Picture-in-picture       | no      | none; the engine request is not subscribed (UI/web_view/engine/caps.rs:27 list) | No picture-in-picture window or toolbar chip.                                                                                  | L   | not checked: how frames reach a PiP window    |
| 11  | Side panel, reading list | no      | none                                                                            | No side panel, icon rail or reading list.                                                                                      | L   | O: reading-list store                         |
| 12  | Side panel, bookmarks    | no      | none                                                                            | No side panel bookmarks tree, search or folders.                                                                               | L   | O: bookmark store                             |
| 13  | Keyboard shortcuts       | partial | B/ui/menus.rs:shortcut_text (dialog)                                            | Plain text dialog. No search, three-column groups, or many commands (Bookmark, Reader, Focus mode). Not rendered: no scenario. | S   | none                                          |

## Library (3)

| #   | Frame     | Impl | Where                                           | What differs                                                                                                | Gap | Engine            |
| --- | --------- | ---- | ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | --- | ----------------- |
| 01  | History   | no   | none (model/history.rs is a 5-row memory list)  | No history manager: no filters, sort, multi-select, by-site view, forget actions. History is not persisted. | L   | O: history store  |
| 02  | Bookmarks | no   | none                                            | No bookmark manager: no folders, tags, import/export, check.                                                | L   | O: bookmark store |
| 03  | Downloads | no   | B/ui/popovers.rs:download_list is the only list | No downloads manager: no filters, search, blocked state, Open folder.                                       | M   | none              |

## Settings (11)

Settings today is one page in a tab (`TabKind::Settings`, `B/model/tab.rs:184`). `plan.md` says Settings and the
Library are windows of their own, never a page in a tab. That difference alone makes every row below partial or no.

| #   | Frame                  | Impl    | Where                                                                          | What differs                                                                                                                            | Gap | Engine                               |
| --- | ---------------------- | ------- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- | --- | ------------------------------------ |
| 01  | General                | no      | none; B/ui/settings.rs is one page in a tab                                    | No General: start behaviour, default browser, smooth scroll, save-to folder. Settings is a tab, not the split window PLAN.md calls for. | M   | O: default-browser registration      |
| 02  | Appearance             | no      | none                                                                           | No Appearance: layout, style, pages-in-dark choice, bookmarks bar, default zoom.                                                        | M   | none                                 |
| 03  | Tabs                   | no      | none                                                                           | No Tabs settings or Groups list.                                                                                                        | S   | none                                 |
| 04  | Search                 | partial | B/ui/settings.rs:SettingsPage, model/address.rs:ENGINES                        | Four fixed engines in a dropdown. No private-window engine, no suggest toggle, no custom engines. Choice is not saved.                  | M   | none: host owns search (SE2)         |
| 05  | Privacy and security   | partial | B/ui/settings.rs privacy and engine groups; `settings`, `security`             | Clear history and Reset permissions act on memory. No blocking level, fingerprint, HTTPS-only, Secure DNS, deceptive warning.           | L   | U: prefs allowlist, clear-data (ST1) |
| 06  | Sites                  | no      | none                                                                           | No Sites: no per-kind defaults or sites list.                                                                                           | M   | U: permission store                  |
| 07  | Site details           | no      | none                                                                           | No Site details: visits, storage, per-kind pickers, zoom, clear.                                                                        | M   | U: permission store, clear-data      |
| 08  | Passwords and passkeys | no      | none                                                                           | No password manager, checks, or autofill settings.                                                                                      | L   | U+O: see Prompts 06                  |
| 09  | Extensions             | no      | none                                                                           | No extension list, toolbar pins or private-window opt-in.                                                                               | L   | U: extension management              |
| 10  | Profiles               | no      | none; `BINGUX_BROWSER_PROFILE` env var only (UI/web_view/engine/config.rs:134) | No profile list, add, ask-at-start, guest. See the Profiles section.                                                                    | L   | none: profile_dir per engine exists  |
| 11  | About Browser          | partial | B/ui/settings.rs engine group                                                  | Engine rows and sandbox report exist. No product hero, version, updates or licences. Real-engine user agent reads Not reported yet.     | M   | U: user agent not produced           |

## Phone (20)

The phone layout is chosen by width (`is_compact()`), and shows the address bar on top. The frames put the address
pill at the bottom. "Address bar at the bottom" is itself a setting in Phone 16.

| #   | Frame                    | Impl    | Where                                                                         | What differs                                                                                                             | Gap | Engine                             |
| --- | ------------------------ | ------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | --- | ---------------------------------- |
| 01  | Page, light              | partial | B/ui/toolbar.rs compact branch; `page` phone (phone-sheet-1.png)              | Address bar on top with tab-count button; design has a bottom address pill and share/reader buttons. Light not rendered. | M   | none                               |
| 02  | Page, dark               | partial | same; (pairs/phone-sheet-1.png)                                               | Same as Phone 01 in dark.                                                                                                | M   | none                               |
| 03  | Scrolled, toolbar folded | no      | none                                                                          | Toolbar does not fold on scroll.                                                                                         | S   | not checked: scroll events         |
| 04  | Loading                  | partial | B/ui/page.rs:LoadProgress; `loading` phone                                    | Progress line and Stop exist. No spinner in the pill, no bottom load card.                                               | S   | none                               |
| 05  | New tab                  | partial | B/ui/pages.rs:new_tab; `newtab` phone                                         | Same single status page. No top sites, reading list, This week.                                                          | L   | as Window 03                       |
| 06  | Typing, suggestions      | partial | B/ui/address.rs; `suggestions` phone                                          | Dropdown under the field, not a full-width card above the keyboard. No sections.                                         | M   | as Window 06                       |
| 07  | Tabs, grid               | partial | B/ui/phone.rs:TabsSheet; `tab-list` phone                                     | Row list in a sheet. No thumbnail grid, groups, search, Private/Tabs/Groups/Devices bar.                                 | L   | O: thumbnails from the page frames |
| 08  | Private tabs             | no      | none                                                                          | No private tab view; private is a separate window.                                                                       | M   | none                               |
| 09  | Menu, sheet              | partial | B/ui/menus.rs:main_menu; `menu` phone                                         | Anchored list, not a sheet with profile card, tiles, Add to home screen, Desktop site, extension switches.               | L   | O: desktop-site UA override        |
| 10  | Site information, sheet  | partial | B/ui/popovers.rs:page_info; `info` phone                                      | Anchored card, not a sheet. Same content gaps as Window 10.                                                              | M   | as Window 10                       |
| 11  | Location asked, sheet    | partial | B/ui/prompts.rs:PermissionBanner; `permission` phone                          | Banner, not a sheet with Precise-location switch.                                                                        | M   | as Prompts 01                      |
| 12  | Long press on a link     | partial | B/ui/menus.rs:context_items; UI/web_view/touch.rs long press; `context` phone | Same short menu as desktop. No link preview panel, Share, reading list. Long press is a synthesised right click.         | M   | U: touch input                     |
| 13  | Reader view              | no      | none                                                                          | No reader view.                                                                                                          | L   | as Tools 03                        |
| 14  | Find in page             | partial | B/ui/find.rs; `find` phone                                                    | Card at the top, not above the keyboard.                                                                                 | S   | none                               |
| 15  | Downloads                | partial | B/ui/popovers.rs:download_list; `downloads` phone                             | A bottom popover, not a pushed page with Now/Earlier sections.                                                           | M   | none                               |
| 16  | Settings                 | partial | B/ui/settings.rs; `settings` phone                                            | Desktop page in a tab, not a list page with pushed pages and Address-bar-at-bottom.                                      | L   | none                               |
| 17  | Page says, alert         | partial | B/ui/prompts.rs:show_dialog; `dialog` phone                                   | Centred dialog exists; no Do-not-show-more checkbox.                                                                     | S   | none                               |
| 18  | Offline                  | partial | B/ui/pages.rs:failed; `error` phone                                           | Page exists with Try again; no Saved-for-offline, no network settings.                                                   | S   | none                               |
| 19  | Certificate expired      | partial | B/ui/pages.rs:cert_error; `cert` phone                                        | As Page states 04; no red chip in the bottom pill.                                                                       | S   | none                               |
| 20  | Landscape                | partial | none: 844x390 uses the desktop layout (landscape/page.png)                    | Desktop strip and toolbar leave a sliver of page. Compact is chosen by width, not height.                                | M   | none                               |

## Embedded (6)

Only the raw control exists: `web_view(engine, session, surface)` in `UI/web_view/view.rs`. The design describes more:

- The types `WebView`, `ViewConfig`, `ViewState`, `WebViewEvent` and `WebViewHost`.
- The origin strip.
- Status pages and dialogs owned by the control.

None of that exists. The browser frame draws the status pages and dialogs.

| #   | Frame                                     | Impl | Where                                                              | What differs                                                                                                  | Gap | Engine                                   |
| --- | ----------------------------------------- | ---- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- | --- | ---------------------------------------- |
| 01  | Mail signs in to Postbox inside the sheet | no   | none: no embedder exists; `UI/web_view/view.rs` is the raw control | No origin strip, sheet or sign-in flow. No `WebView`, `ViewConfig`, `ViewState` or `WebViewHost` type.        | L   | U: registrable domain (P2) for the strip |
| 02  | Sign-in page loading                      | no   | none                                                               | No embedded loading state: spinner in the origin strip and a progress line at its foot.                       | M   | none                                     |
| 03  | Sign-in page cannot load                  | no   | none: status pages are drawn by `B/ui/pages.rs`                    | The control draws no status page, so an embedder gets none.                                                   | M   | none                                     |
| 04  | Certificate problem, no way past it       | no   | none: `B/ui/pages.rs:cert_error` offers Proceed in Details         | No embedded rule that a certificate error has Go back only.                                                   | M   | none                                     |
| 05  | The page asks, alert inside the view      | no   | none: dialogs are `B/ui/prompts.rs`                                | Dialogs belong to the browser frame, so an embedder cannot show them inside its view, modal to the view only. | M   | none                                     |
| 06  | Phone, sign in inside a sheet             | no   | none                                                               | No sheet embedding of the control on a phone.                                                                 | M   | none                                     |

## Engine-dependent gaps: Vivarium or browser side

The test, from Kieran: upstream to Vivarium only when it is useful to many kinds of embedder. Otherwise a clean
browser-side overlay. Never a workaround in the UI. "Open" means the answer needs Kieran or the engine owner.

| Gap                                      | In the engine today                                                  | Where it goes                            | Reasoning                                                                                         |
| ---------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------- |
| IME composition                          | Not on main. Plan on `feat/ime` (`~/dev/vivarium-ime`)               | Upstream                                 | Only the engine knows caret rects and surrounding text; every embedder with a text input needs it |
| Drag and drop, both ways                 | `vv_session_send_drag` removed from ABI; announced unsupported       | Upstream                                 | The DataTransfer is engine state. The browser only maps pointer to OS drag (`UI/drag.rs`)         |
| Mute and audible                         | No call, no event (`caps.rs:94`)                                     | Upstream                                 | Audio is per-session engine state; any embedder with media needs it                               |
| Session restore                          | `vv_session_state_acquire`/`restore` removed (H1 missing)            | Upstream                                 | Session store is engine internal; tab apps, mail and viewers all lose state on a crash            |
| Touch input                              | Announced unsupported                                                | Upstream                                 | Same: gestures and pinch are engine input                                                         |
| Permission store                         | Prompts work; remember, list, edit missing (PM2, PM3)                | Upstream                                 | Gecko owns the store; every prompting embedder must remember an answer                            |
| Camera and mic devices, in-use indicator | Missing (PM4)                                                        | Upstream                                 | Enumeration and capture state live in the engine                                                  |
| Screen sharing picker                    | Missing (PM5)                                                        | Upstream request, O picker               | Capture needs the engine and portal; the picker UI is browser layout                              |
| Credentials, autofill, save prompt       | Missing (A1, A2, D15)                                                | Upstream detection and requests; O store | Form detection is in page actors. Where the store lives (profile or keyring) is open (A2)         |
| Passkeys                                 | Prompts leave as observer topics (A6)                                | Upstream                                 | Only the engine can turn them into a request                                                      |
| Window-open gesture and disposition      | Missing (N13)                                                        | Upstream                                 | Pop-up policy is browser, but the facts are the engine's                                          |
| Navigation veto before load              | Missing (N11, `NAV_STARTED`)                                         | Upstream                                 | Needed by deceptive-site lists, parental filters, mailto handlers                                 |
| Focus leaving the page                   | Missing (N15)                                                        | Upstream                                 | Tab out of the last element must reach any host                                                   |
| Certificate chain and fields             | Only host, code, issuer, fingerprint                                 | Upstream                                 | Any embedder with a lock icon can show the chain                                                  |
| Reader and document facts                | Missing (L15)                                                        | Upstream                                 | Readability is Gecko code; help viewers and mail use it too                                       |
| Tracker statistics                       | Missing (C4)                                                         | Upstream                                 | Gecko's tracking DB; the embedder only displays it                                                |
| Clear site data, site size               | Missing (C13, ST1)                                                   | Upstream                                 | `nsIClearDataService` is already used inside the engine (`VivariumContexts.cpp`)                  |
| Favicon bytes                            | URL only (L4)                                                        | Upstream                                 | Fetch must use the engine's network stack and partition, or private tabs leak                     |
| Print to PDF                             | Exists: `Session::print_pdf`                                         | Wire                                     | Not used by `UI/web_view` yet. Print dialog and printers are O                                    |
| DevTools                                 | On main: `Engine::open_devtools`, `VV_ENGINE_ALLOW_DEVTOOLS`         | Wire                                     | Docking and layout are browser                                                                    |
| Page fullscreen                          | `VV_REQUEST_FULLSCREEN` exists; not subscribed                       | Wire                                     | `caps.rs:27` omits it, so pages cannot go fullscreen                                              |
| Picture-in-picture                       | `VV_REQUEST_PICTURE_IN_PICTURE` exists                               | Wire, then check                         | Not checked how video frames reach a separate window                                              |
| Tab thumbnails                           | `vv_session_screenshot` removed                                      | Overlay                                  | The browser already holds each tab's frames; few embedders want thumbnails                        |
| Bookmarks, reading list                  | None; Vivarium removed history query                                 | Overlay                                  | A browser data model. Whether Places runs in the engine is unanswered                             |
| History and frecency                     | Same                                                                 | Open                                     | If Places runs, ask for a query API; else a browser store. Needs the engine owner                 |
| Content blocking                         | Engine call removed; uBlock Origin as extension                      | Overlay                                  | Shipping and updating the extension and its levels is browser policy                              |
| Deceptive-site list                      | Safe Browsing not planned (C12)                                      | Overlay plus the veto above              | Lists and updates are browser policy                                                              |
| Translation                              | Gecko translations exist but are unreachable                         | Open                                     | Needs models and DOM access; decide after reader                                                  |
| User agent string                        | `EngineInfo::user_agent` not produced                                | Upstream                                 | Engine gap 4 in `docs/rust.md`                                                                    |
| Extension host surface                   | Install, enable, prompts exist (E1 to E7); actions and popups do not | Upstream                                 | Any embedder using extensions needs the same surface                                              |
| Import from Firefox or Chromium          | None                                                                 | Overlay                                  | Reads another browser's files; no engine involvement                                              |

## Engine primitives wanted

Places where the browser or bingux-ui reimplements something Gecko already does. These are proposals: a primitive goes
upstream to Vivarium only when its row states which other kinds of embedder would use it (Kieran's rule); otherwise it
stays a browser-side layer. P1 to P4 (URL fixup, parse, display and query) are accepted on Kieran's own example and are
being built on Vivarium's `feat/url-primitives`. Gecko paths come from `api-parity.md` rows, or are marked as not opened.

| #   | Browser code today                                                                                                 | Gecko component                                                                                               | Sketch                       |
| --- | ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| P1  | `B/model/address.rs:126` `classify`, `:74` `looks_like_host`, `:113` `default_scheme`, `B/target.rs:20` file paths | `nsIURIFixup` (`FixupAndLoadURIString`, N1 and SE1 rows)                                                      | `vv_uri_fixup`               |
| P2  | `address.rs:190` `host`, `:201` `origin`, `:218` `initial`                                                         | `nsIEffectiveTLDService`, `nsIURI`, principal site origin (paths not opened)                                  | `vv_url_parse`               |
| P3  | `address.rs:173` `display` (no scheme, no slash)                                                                   | Firefox front end `trimURL`, `losslessDecodeURI` (in `browser/`, not toolkit; not opened)                     | `vv_url_parse` display field |
| P4  | `address.rs:40` `encode` query values                                                                              | `nsIURIMutator`, `nsEscape` (not opened)                                                                      | `vv_url_with_query`          |
| P5  | `B/model/failure.rs:22` error classes by nsresult name                                                             | `NetErrorChild` and `about:neterror` (N10 row); the nsresult to page mapping lives in docshell (not opened)   | `vv_load_error.category`     |
| P6  | `failure.rs:162` strips `profile_dir: ` from a message                                                             | none: `vv_error` lacks a key field (`docs/rust.md`)                                                           | `vv_error.key`               |
| P7  | `B/model/zoom.rs:5` zoom levels                                                                                    | Firefox's zoom steps, which the engine's own Ctrl+wheel uses (`docs/input.md`); the Gecko file was not opened | `vv_session_zoom_step`       |
| P8  | `B/model/permissions.rs` per-origin map                                                                            | `nsIPermissionManager` (PM2, PM3 rows)                                                                        | permission store calls       |
| P9  | `B/model/history.rs:89` `matching` score                                                                           | Places frecency (H2 row; running unknown)                                                                     | open, see above              |
| P10 | `B/model/security.rs:29` file override                                                                             | Security state flags of the web progress listener, and protocol handler flags (not opened)                    | typed state in `vv_security` |
| P11 | `B/ui/strip.rs:33` generic tab icon                                                                                | `nsIFaviconService`, page actor `DOMLinkAdded` (L4 row)                                                       | `vv_session_favicon`         |
| P12 | none yet: clear history, site data                                                                                 | `nsIClearDataService.deleteDataFromBaseDomain` (C13, ST1 rows)                                                | `vv_engine_clear_data`       |
| P13 | none yet: file-type icons, open-with                                                                               | `nsIMIMEService`, `nsIHandlerService` (DL6 row)                                                               | `vv_mime_for_path`           |
| P14 | none yet: per-site zoom                                                                                            | content prefs service (`toolkit/components/contentprefs`, stays in toolkit per `api-parity.md`)               | zoom scope argument          |

### Sketches

All sketches follow the ABI rules in `vivarium_base.h`: a `struct_size` first field, UTF-8 `vv_str`, an engine
handle, and a Rust wrapper in the matching module of `crates/vivarium`.

```c
/* P1: what typed text means. The engine already does this inside vv_session_load. */
typedef enum { VV_FIXUP_URL = 1, VV_FIXUP_SEARCH = 2, VV_FIXUP_FILE = 3, VV_FIXUP_REFUSED = 4 } vv_fixup_kind;
typedef struct vv_fixup_result {
    uint32_t struct_size; vv_fixup_kind kind;
    vv_str url;          /* what to load (empty for SEARCH: the host owns search) */
    vv_str search_terms; /* for SEARCH */
    uint32_t flags;      /* VV_FIXUP_USED_HTTP_DEFAULT, _WAS_IP, _HAD_USERPASS */
} vv_fixup_result;
VV_API vv_status vv_uri_fixup(vv_engine*, vv_str text, uint32_t flags, vv_fixup_result* out);

/* P2, P3, P4: parse once, get everything a UI reads. */
typedef struct vv_url_info {
    uint32_t struct_size;
    vv_str scheme, host_ascii, host_display /* IDN rules applied */, base_domain /* eTLD+1 */, public_suffix;
    vv_str origin, site_origin, path, display /* trimmed for an address field */;
    uint32_t port; uint32_t flags; /* IS_IP, IS_LOCAL_FILE, IS_INTERNAL, IDN_SHOWN_AS_PUNYCODE */
} vv_url_info;
VV_API vv_status vv_url_parse(vv_engine*, vv_str url, vv_url_info* out);
VV_API vv_status vv_url_with_query(vv_engine*, vv_str template_url /* %s */, vv_str query, vv_str* out);
```

```rust
// crates/vivarium/src/navigation/fixup.rs and url.rs
impl Engine {
    pub fn fixup(&self, text: &str, flags: FixupFlags) -> Result<Fixup>;      // Fixup { kind, url, terms, flags }
    pub fn parse_url(&self, url: &str) -> Result<UrlInfo>;                    // base_domain, host_display, display ...
}
```

The browser then keeps only policy: which search engine, which scheme to show bold, where a trail breaks.

## Frontend workarounds found

Every place in `B/` and `UI/web_view` that papers over a missing engine feature or special-cases a page, URL or
engine quirk. Kieran's rule: none of these may stay. "Fix" says where the real answer belongs.

| #   | Where                                                                                                                                   | What it hides                                                                                                                                            | Fix                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| W1  | `UI/web_view/touch.rs:1-15`, `view.rs:201-222`                                                                                          | No touch input: fingers become click, scroll, 500 ms right-click                                                                                         | Vivarium touch. Delete `touch.rs`                                                          |
| W2  | `view.rs:189`, `engine/convert.rs:233`                                                                                                  | No IME: pre-edit text is dropped, only commits pass                                                                                                      | Vivarium `feat/ime`                                                                        |
| W3  | `engine/caps.rs:94`, `backend.rs:321`, `B/ui/strip.rs:14-28`, `ui/menus.rs` Mute item                                                   | Audio UI built for events the engine never sends: dead on the real engine                                                                                | Vivarium mute and audible                                                                  |
| W4  | `backend.rs:188`, `engine/mod.rs:359`, `B/model/browser/lifecycle.rs:64`                                                                | `restore_from` is ignored; a restart reopens the address only, losing scroll and forms                                                                   | Vivarium session state                                                                     |
| W5  | `engine/mod.rs:536-543`, `caps.rs:27`                                                                                                   | Page fullscreen never answered; `set_fullscreen` is a no-op                                                                                              | Wire `VV_REQUEST_FULLSCREEN`                                                               |
| W6  | `B/model/browser/commands.rs:630`                                                                                                       | Print is a notice saying it is unavailable, though the engine has `print_pdf`                                                                            | Wire; build the print UI                                                                   |
| W7  | `B/ui/strip.rs:30-39`, `B/model/address.rs:218`                                                                                         | Generic tab icon; a letter-placeholder helper nobody calls                                                                                               | P11 favicons                                                                               |
| W8  | `commands.rs:26-29`, `:335`, `B/model/tab.rs:153`, `:184`, `address.rs:174`, `history.rs:47`, `security.rs:29`, `B/ui/toolbar.rs:40-47` | New tab and Settings are fake tabs: `about:config` mapped to Settings, engine kept on `about:blank` behind a widget, a synthetic Internal security state | Host scheme through the engine, or Settings as its own window (design question in slice 7) |
| W9  | `B/model/browser/prompts.rs:130-150`, `B/ui/web.rs:64`                                                                                  | A browser-invented context menu when the engine says nothing about what is under the pointer                                                             | Disable the menu when the capability is off; no invention                                  |
| W10 | `B/model/failure.rs:22-72`                                                                                                              | Error classes found by matching `NS_ERROR_*` names                                                                                                       | P5                                                                                         |
| W11 | `failure.rs:162`                                                                                                                        | Strips `profile_dir: ` off an error message                                                                                                              | P6                                                                                         |
| W12 | `B/model/browser/lifecycle.rs:107-125`                                                                                                  | Pop-up block depends on a gesture flag the engine does not send, so it never blocks on the real engine                                                   | N13 in Vivarium                                                                            |
| W13 | `B/ui/popovers.rs:170-181`                                                                                                              | Retry is the engine's Resume on a failed download                                                                                                        | Check Resume semantics; ask for a distinct Retry if it restarts wrongly                    |
| W14 | `B/model/permissions.rs:1-4`, `B/ui/settings.rs:184`                                                                                    | Comment says the engine's profile keeps decisions; no such store exists. Decisions live in a map for the window's life                                   | P8                                                                                         |
| W15 | `B/model/history.rs:1-5`, `settings.rs:155`                                                                                             | History, search engine and every setting are in memory. The browser writes no file at all                                                                | Slice 2                                                                                    |
| W16 | `UI/web_view/engine/requests.rs:250-262`, `B/ui/prompts.rs:232-233`                                                                     | Folder and Save picker branches for requests the ABI never sends                                                                                         | Delete the branches, or ask Vivarium                                                       |
| W17 | `B/model/security.rs:29-31`                                                                                                             | A `file://` URL is declared Local whatever the engine says                                                                                               | P10                                                                                        |
| W18 | `B/ui/settings.rs:43`                                                                                                                   | Real engine user agent shows "Not reported yet"                                                                                                          | Vivarium engine gap 4                                                                      |
| W19 | `UI/web_view/engine/config.rs:134-170`                                                                                                  | The only configuration path is environment variables                                                                                                     | Slice 2 settings layer                                                                     |
| W20 | `B/model/address.rs:12-37`                                                                                                              | Four search engines compiled in                                                                                                                          | Browser settings data, not code                                                            |

## Profiles

Design: Settings 10 shows Personal, Work and Study. "Each profile keeps its own history, cookies, passwords and
extensions." Also an Add profile row, Ask which profile when Browser starts, a Guest window, and Profile in the main
menu (Window 07, Phone 09). A profile is therefore a whole browser identity, not a cookie jar.

**Profile or contextual identity?** Vivarium has both.

- An engine has one `profile_dir` and holds a lock on it.
- A contextual identity is a storage partition inside that profile: cookies and local storage.
- Identities share the profile's extensions, certificate exceptions, prefs and history.

So a **profile is a profile directory with its own engine process**. Containers are a separate feature: identities
inside one profile. Only a separate directory isolates extensions, prefs, locks and a crash from another profile.

| What a real profile system needs                            | Today                                                                                                                      |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Registry: id, name, colour, directory, created, default     | None. One directory, `$XDG_DATA_HOME/bingux-browser/profile`                                                               |
| Create, rename, delete, with a confirm and a disk-size note | None                                                                                                                       |
| Default profile, and Ask at start                           | None                                                                                                                       |
| One engine and one app id per profile                       | App id is fixed `bingux-browser` (`B/main.rs:54`); second launch hands over to the one window                              |
| Lock against two instances                                  | Engine reports `ProfileLocked`; the failure page words it (`failure.rs:179`)                                               |
| Switching                                                   | None. It is opening a window of the other profile, in its own process                                                      |
| Per-profile settings, history, bookmarks, passwords         | None. Nothing is stored at all (W15)                                                                                       |
| Per-profile extensions and downloads folder                 | Engine side per profile directory. Browser side none                                                                       |
| Private window                                              | Separate process, temp profile removed on drop (`config.rs:316`). Not per profile                                          |
| Guest window                                                | Not offered. A private window is the nearest                                                                               |
| Import from Firefox or Chromium                             | None                                                                                                                       |
| Cache, crash, socket folders per profile                    | Done for a named profile: `config.rs:146` moves them next to it                                                            |
| Directory safety                                            | Engine refuses a group-writable folder (`ProfileUnsafe`)                                                                   |
| Env override                                                | `BINGUX_BROWSER_PROFILE` names a directory (`config.rs:134`). No UI                                                        |
| Containers                                                  | `TabConfig::container` works in the backend (`engine/mod.rs:181`); the browser always sends 0 (`model/browser/mod.rs:193`) |
| Sync (frames say "Synced 2 min ago")                        | No service exists. Decide: out of scope, or its own project                                                                |

## Slices

Ordered smallest risk first. Engine-blocked slices last. Each lists the frames it closes and the questions to answer
before code. "Engine / browser / bingux-ui" is the owner split.

### 1. Honest states and polish (no new data)

Frames: Window 24, Tools 02, 07, 13, Page states 01, 02, 04, 08, 09, 10, Prompts 09 to 14 (placement, suppress box,
Show it), Phone 14, 17 to 19, Window 16 (rows), Window 30 (focus mode).

- Which dialogs hang from the content edge, and how is tab-scoped modality built in `UI/shell`?
- Does the status page template get a folded Technical details component in bingux-ui?
- The Not-secure chip: where does it live in the field, and what does it do on the trail-less field of today?
- Split: browser and bingux-ui only.

### 2. The profile store and settings layer (foundation)

Frames it unblocks: Settings 04, 05, 01 to 03, Window 11, Library, bookmarks, history, per-site zoom.

- Data model: one SQLite per profile (history, bookmarks, permissions, zoom, search engines, settings), or files?
- Where: `<profile_dir>/browser/`, so deleting a profile deletes everything; a sibling of the engine's data, not inside it.
- Migration: versioned schema; first run reads the existing directory as profile "Personal".
- Isolation: a private window must open no store and write nothing; a test must prove it.
- Failure modes: corrupt or locked database, full disk, a newer schema than the binary. Each needs a page state.
- Concurrency: one process owns a profile (the engine lock already says so).
- Security: mode 0700, no secrets here (passwords are slice 11).
- Split: browser owns the store; bingux-ui gets no storage code.

### 3. Wire what the ABI already has

Frames: Tools 05 (print backend), 08 (developer tools), 10 (picture-in-picture), Window 30 and page fullscreen, W5, W6.

- Print: who renders the preview (a Vivarium session opening the PDF, or bingux-ui), who lists printers (CUPS portal)?
- DevTools: which edge does the toolbox dock to, and how is it closed when its target closes (`VV_CLOSE_TARGET_CLOSED`)?
- Fullscreen request: what does the user see, and how is it refused for a hidden tab?
- Is a DevTools toolbox allowed in a private window (the engine says denied)?

### 4. Address field, trail and card on engine URL primitives

Frames: Window 01, 06, 08, 10, Phone 06, Settings 04, Embedded origin strip.

- Needs P1 to P4 first. What is the trail's break rule, and how are IDN and punycode displayed?
- Suggestion sources and ranking: tabs, history, bookmarks, settings, commands: who ranks across stores?
- Which of them may be sent to the search engine (privacy)? Private windows: none.
- A bingux-ui `path_field` and `Omnibox` component with scope chips: owner is bingux-ui.
- Failure modes: a huge URL (engine caps 8192 bytes), a bad scheme, a paste with newlines.

### 5. Bookmarks, reading list, history manager, side panel

Frames: Library 01 to 03, Tools 11, 12, Window 28, Phone 09 partial.

- Needs slice 2. Folder model, tags, ordering, import and export format (HTML? JSON?), duplicate rule.
- History retention (the frame says 90 days), forget-this-site, and what a private window records (nothing).
- Reading list offline copies: stored where, sized how, rendered by the engine or a reader view?
- bingux-ui components: `item_bar`, `side_panel` with icon rail, a manager table with multi-select.
- Failure modes: an import with 10,000 items, a deleted folder with children, a synced conflict.

### 6. Layouts: tabs on the side, one row, groups, installed site

Frames: Window 17 to 23, 25 to 29, Settings 02, 03, Phone 07 (groups).

- Needs slice 2 for the Layout setting. Tab group data model: id, name, colour, collapsed, saved or not.
- Groups are browser-owned; the extension tab field `groupId` follows that.
- Needs bingux-ui `vertical_tabs`, a bookmarks bar, a glass header, and a `split_view`.
- Installed site: manifest source, a separate profile or storage partition, a window id for the OS.
- Session restore of groups: the engine cannot restore pages (W4), so the list is addresses only until slice 12.

### 7. Settings and Library as windows; Sites and privacy pages

Frames: Settings 01 to 07, 11, Window 10, Window 07 rows.

- Needs slices 2 and 5 and Vivarium P8 and P12. Which prefs are on the engine allowlist today? Not checked.
- A prefs change at run time or at next start? The engine says some are read once.
- Clear data: scope (all, site, time range), identity, and what the engine cannot clear (HTTP cache, DNS).
- Settings search across pages; deep links from the main menu.
- New tab and Settings today are fake tabs (W8). Are they pages the engine serves through a host scheme, or native windows? Settings and Library are windows in the design, so the tab kind goes.
- Split: engine holds prefs and the permission store; browser holds layout and the pages; bingux-ui the settings window.

### 8. Profiles

Frames: Settings 10, Window 07 (Profile row), Phone 09, Window 29 (per-site app profile question).

- See "Profiles". Needs slice 2 and slice 7.
- Decisions: process model (one browser process per profile), the app id, the picker window at start, delete semantics.
- Does a private window belong to a profile? Does a guest window exist?
- Migration of the single existing profile directory.
- Import: formats, passwords (NSS key4.db) and what is refused.
- Failure modes: two launches racing, a deleted profile that is running, a profile on a read-only mount.

### 9. Phone layout

Frames: Phone 01 to 20 (except those listed elsewhere).

- Does bottom-address-bar change the toolbar model or is it one component with a flag? Compact by width and height?
- Tab thumbnails: captured when, and kept where (memory, a cache dir)? Private tab thumbnails: never.
- Sheets with inset parent (bingux-ui gap in `design.md`), folded toolbar needs scroll events (not checked).
- Needs Vivarium touch (slice 12) for real gestures; today it is the W1 workaround.

### 10. Embedded web view control

Frames: Embedded 01 to 06.

- API: `WebView`, `ViewConfig`, `ViewState`, `WebViewHost`. Who owns status pages and dialogs: the control.
- Origin strip rule needs P2 (registrable domain). What does an embedder with no engine policy get by default?
- A consumer is needed to prove it: Mail's sign-in sheet.
- Security: embedded view has no Proceed on certificate errors; permission requests refused by default.

### 11. Extensions, content blocking, passwords

Frames: Window 12 to 14, Settings 08, 09, Prompts 06 to 08, Window 03 (This week stats), Settings 05 blocking.

- Engine first: extension action and popup host surface, a login-save request, an autocomplete request.
- Password store decision (A2): Gecko profile store, or the system keyring through a host store? Reauth before reveal.
- Which uBlock build ships, who updates it, how are the levels mapped to its settings?
- Trust: extension install source, permission prompts, private-window opt-in.

### 12. Engine-blocked input and media

Frames: Window 19 (audio states), Page states 08 (typed text restored), 10 (state restored), Prompts 02, 03, 05, Phone 12.

- Needs Vivarium IME, drag and drop, mute and audible, session restore, touch, device lists, capture state, N13.
- For each: the capability flag goes true, the workaround (W1 to W4, W12) is deleted in the same change.
- Design question for the session restore: what survives a browser restart, not only an engine crash?

### 13. Content features that need engine work

Frames: Tools 03, 04, 06, Phone 13 (reader, translate, share), Page states 03, 07 (HTTPS-first, deceptive).

- Reader needs document facts and a transform (L15). Translate: models, size, privacy, offline? Open.
- Share: xdg portal, targets, and what a private window shares.
- Deceptive list: source, update cadence, privacy of lookups, an override path.

## Not checked

- Light-scheme renders of the browser: the snapshot list has none.
- The 16-tab and 40-tab strips (Window 20, 21): no scenario; the component's own doc is the only source.
- Scroll and fold behaviour on a phone (Phone 03).
- How the real engine reports HTTPS-only failures, match positions for find, picture-in-picture frames, and which
  prefs are on the allowlist. Each needs a real-engine run.
- Hidden-tab frame availability for thumbnails.
- Whether Places runs inside the engine (open question 2 of `api-parity.md`).
- `feat/ime` state: read from `TODO.md` only.
