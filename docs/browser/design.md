# Browser: design

Copied from bingux-doop `out/apps-browser/DESIGN.md`; paths such as `frames/apps-browser/` are in that repository.

107 frames on the canvas "Bingux: Browser" (`KyPwogXD5M`), on six pages: Window; Page states and prompts; Tools;
Settings and library; Phone; Web view in other apps.

- **Source:** `frames/apps-browser/`, one module per page group, with the shared kit in the `_` modules.
- **Plan:** `plan.md`.
- **What the 2026-11-25 consolidation merged and removed:** `consolidation.md`.

## Requirements and the frames that answer them

Every request Kieran made for the browser's design, in the order he made it, with the frame that answers it.
"Comment" rows are his comments on the canvas (2026-11-25); the frame he commented on was replaced by the one named.

| #   | Requirement                                                                                             | Source                        | Answered by                                                                  |
| --- | ------------------------------------------------------------------------------------------------------- | ----------------------------- | ---------------------------------------------------------------------------- |
| 1   | The whole chrome is bingux-ui; a component other apps could use goes into bingux-ui                     | Prompt, 2026-10-29            | Every frame; the gaps table below                                            |
| 2   | Gecko engine; the normal Firefox user agent                                                             | Prompts, 2026-10-29 and 10-30 | Settings 11 About Browser (engine, user agent)                               |
| 3   | The engine's sandbox and security must be visible and trustworthy                                       | Prompt, 2026-10-30            | Settings 11 (engine security report), Settings 05                            |
| 4   | Link status on hover; Bingux scrollbars                                                                 | Prompt, 2026-10-30            | Window 01 (status card, scrollbar)                                           |
| 5   | Pages follow the system theme                                                                           | Prompt, 2026-10-30            | Settings 02 Appearance ("Pages in dark style"), Window 02                    |
| 6   | Consumers get a CEF/Electron-like API with full control of the view                                     | Prompt, 2026-10-30            | Embedded 01-06; the API sketch below                                         |
| 7   | More layouts                                                                                            | Prompt, 2026-11-10            | Window 17, 25-30; Settings 02                                                |
| 8   | Tabs either floating or attached, not both: floating                                                    | Prompt, 2026-11-10            | Window 18-24                                                                 |
| 9   | Compact mode is one row, like his reference                                                             | Prompt, 2026-11-10            | Window 27-28                                                                 |
| 10  | Fewer Chrome-isms; it must feel like Bingux                                                             | Prompt, 2026-11-10            | Decisions below; every frame                                                 |
| 11  | Find is a floating card (every app)                                                                     | Prompt, 2026-11-10            | Tools 01-02, Phone 14                                                        |
| 12  | A menu button                                                                                           | Prompt, 2026-11-10            | Window 07-08 (three-dot main menu)                                           |
| 13  | The bookmarks bar is the toolbar's colour                                                               | Prompt, 2026-11-10            | Window 28                                                                    |
| 14  | uBlock Origin and Bitwarden in the toolbar                                                              | Prompt, 2026-11-10            | Window 12-14                                                                 |
| 15  | A profile icon in every Doop app                                                                        | Prompt, 2026-11-10            | Superseded by row 37: no avatar; profiles in Settings 10 and the main menu   |
| 16  | The phone UI needs serious work                                                                         | Prompt, 2026-11-10            | Phone 01-20                                                                  |
| 17  | Real pages and real use cases (permissions.site, about:config, CEF-style apps)                          | Prompt, 2026-11-13            | Embedded 01-06 (Mail's sign-in); the rest are Vivarium demos (default below) |
| 18  | Drop bingux-geckoview: IE-style architecture, the web view a reusable control, the browser a thin frame | Prompt, 2026-11-25            | PLAN.md "Two parts"; Embedded 01-06                                          |
| 19  | Design in Doop with bingux-ui components before building                                                | Prompt, 2026-11-25            | This canvas                                                                  |
| 20  | Rust consumers control the extension-facing tab state (isArticle, audible, width)                       | Prompt, 2026-11-25            | API sketch below (`ViewState`)                                               |
| 21  | Consolidate: one design, few pages, read the comments                                                   | Prompt, 2026-11-25            | This file, PLAN.md, CONSOLIDATION.md                                         |
| 22  | Site details page "sucks"                                                                               | Comment                       | Settings 07 Site details                                                     |
| 23  | Profile menu "sucks"                                                                                    | Comment                       | Removed; profile row in Window 07                                            |
| 24  | Extensions menu could be improved                                                                       | Comment                       | Window 12 Extensions menu                                                    |
| 25  | The Library must be a real manager with tools                                                           | Comment                       | Library 01-03 (filters, sort, multi-select actions, folders)                 |
| 26  | Side panel: a switcher like Firefox's, or an icon rail like Vivaldi                                     | Comment                       | Tools 11-12 (icon rail)                                                      |
| 27  | Share: the phone icon is too skinny; cut the slop                                                       | Comment (2)                   | Tools 06 Share                                                               |
| 28  | Developer tools use the Firefox DevTools UI                                                             | Comment                       | Tools 08                                                                     |
| 29  | Printing is a popup                                                                                     | Comment                       | Tools 05 Print                                                               |
| 30  | The translation bar keeps the toolbar's colour                                                          | Comment                       | Tools 04                                                                     |
| 31  | Find: an X button instead of Done                                                                       | Comment                       | Tools 01, Phone 14                                                           |
| 32  | Certificate details: do better, after macOS                                                             | Comment                       | Page states 05                                                               |
| 33  | "Not secure": red, solid, a proper icon                                                                 | Comment                       | Page states 03, 04, 06; Phone 19                                             |
| 34  | Error pages: solid icons, seriously improved                                                            | Comment (2)                   | Page states 01-10                                                            |
| 35  | Autofill: no key hint in the corner                                                                     | Comment                       | Prompts 08                                                                   |
| 36  | Passkey dialog looked broken                                                                            | Comment                       | Prompts 07                                                                   |
| 37  | Avatar not needed yet; zoom icon ugly; zoom popup ugly and too big                                      | Comment (3)                   | Window 11; no avatar anywhere in the browser                                 |
| 38  | Screen sharing and location prompts could be designed better                                            | Comment (2)                   | Prompts 01, 05                                                               |
| 39  | Split view: each web view gets its own navigation bar                                                   | Comment                       | Tools 09                                                                     |

The binding Bingux rules (one header row; tabs as floating tiles up to 200 px with + after the last; asides closed by
their toggle and Escape; sheets on a phone; dense desktop rows; the `ds-sp` spinner) hold in every frame.

## Defaults chosen for Kieran (2026-11-25)

Kieran did not answer these in time, so these defaults stand. He can still change any of them; each is one edit.

| Question                                            | Default                                                | Where it lives                               |
| --------------------------------------------------- | ------------------------------------------------------ | -------------------------------------------- |
| Do the other apps drop the avatar too?              | No: the browser only is the exception                  | `docs/CONSISTENCY.md` §1.7                   |
| Find card: X or Done?                               | X in every app, in bingux-ui's find control (one rule) | bingux-ui find bar; `docs/CONSISTENCY.md` §2 |
| A separate tablet layout?                           | No: size decides (medium window, then phone)           | PLAN.md, Layouts                             |
| Example pages and CEF-style demo apps (2026-11-13)? | Vivarium demos, not frames on this canvas              | The Vivarium repository                      |

## Decisions

- **One tab model**: tabs are neutral floating tiles (30 px in a 40 px strip, radius 10, no border, a 16 px site
  icon), left-aligned at their natural width up to 200 px. New tab follows the last tab; the chosen tab is the raised
  tile; nothing joins a tab to the toolbar.
- **Tab overflow**: when tabs no longer fit they shrink together: the title fades, then a tab is its favicon, then
  the strip scrolls and a `+N` list appears. Colour is a small signal only (a group's dot, the spinner, audio). Tabs
  on the side are the same tiles in a chrome-tone sidebar.
- **Address field = a path bar for the web**, after the Files location field: at rest a trail (favicon, the
  registrable domain in full ink, path crumbs); no padlock (HTTPS is normal; "Not secure" is a solid red chip with a
  warning glyph); chips at the end for zoom, reader, translate and permissions in use. Typing grows the Omnibox card
  out of the field: scope chips (Everywhere, Tabs, History, Bookmarks, This site), sections, full-text history.
- **Toolbar end**: uBlock Origin and Bitwarden, the Extensions menu, then the three-dot main menu (the shared
  CONSISTENCY.md anatomy). No avatar. The main menu is a text menu with a command search on top and a zoom row.
- **Chrome-isms replaced**: new-tab tiles and search box → a page column of Bingux groups; downloads bubble → a
  transfer popover with Files' rows; chrome://settings → a Settings split window; history and bookmarks pages → a
  Library manager window; page info → a site card.
- **Boundaries** (design/boundaries.md): strip (Paint::Strip) / toolbar, sidebar and side panel (Paint::Sidebar,
  one tone, no line between) / page. The translation bar takes the toolbar's tone. Find is a floating card. Page
  dialogs hang from the content's top edge and are modal to the tab. The side panel has no close button of its own.
  In split view each pane has its own toolbar; the focused pane's field carries the accent ring.
- **Status pages**: one template (a solid glyph in a tinted tile, title, two lines, a primary action, Technical
  details folded). Words for the states without a frame: "The site took too long" (Try again), "The site refused the
  connection" (Try again), "Blocked by a filter list" (Go back, Allow once), "Tab ran out of memory" (Reload tab),
  "This file can’t be opened" (Open folder).
- **Spinner**: the shared `.ds-sp` everywhere something waits.

## The web view control: Rust API sketch

IE's WebBrowser control gave every Windows app the web through one object, and the host shaped it through
`IDocHostUIHandler`. The Bingux control is the same idea over Vivarium.

| IE                                                                  | Bingux                                  |
| ------------------------------------------------------------------- | --------------------------------------- |
| `Navigate`, `GoBack`, `Stop`, `Refresh`                             | `navigate`, `go_back`, `stop`, `reload` |
| `BeforeNavigate2`, `NavigateComplete2`, `TitleChange`, `NewWindow3` | `WebViewEvent`                          |
| `IDocHostUIHandler`, `IDocHostShowUI`                               | `WebViewHost`                           |

| Cargo feature of bingux-ui | Adds                                               |
| -------------------------- | -------------------------------------------------- |
| `web-view`                 | The control and its types; no engine               |
| `web-view-vivarium`        | The real engine (links Vivarium)                   |
| `testing` with `web-view`  | The in-memory test double, for tests and snapshots |

```rust
use bingux_ui::web_view::{WebEngine, WebView, ViewConfig, WebViewEvent, WebViewHost, web_view};

// One engine per app, started once (its own processes and sandbox).
let engine = WebEngine::start(EngineSettings::for_app("mail"))?;

// One view per page. A cheap handle: clone it into closures.
let view = WebView::new(&engine, ViewConfig { private: false, url: Some(url), ..Default::default() })?;

view.navigate("https://accounts.postbox.email/");
view.go_back(); view.go_forward(); view.go_to_history(2);
view.reload(false); view.stop();
view.set_zoom(1.1); view.find(&FindQuery::new("tram")); view.set_muted(true);

// What the page is now: one struct the UI reads; the frame and the extension host read the same fields.
let s: ViewState = view.state();
s.url; s.title; s.favicon; s.load /* Idle | Loading(progress) */; s.security; s.history;
s.audible; s.muted; s.zoom; s.hover_link; s.size /* width, height: tabs.Tab's width/height */;
s.is_article /* reader view possible: tabs.Tab.isArticle */; s.find /* current, total */;

// What happened, in order (IE's DWebBrowserEvents2).
view.on_event(|e: &WebViewEvent| match e {
    WebViewEvent::NavigationStarted { url } => {}
    WebViewEvent::NavigationFinished { url, error } => {}
    WebViewEvent::TitleChanged(_) | WebViewEvent::IconChanged(_) | WebViewEvent::SecurityChanged(_) => {}
    WebViewEvent::NewWindow(request) => {}   // the host decides: here, in a new tab, in Browser, or refused
    WebViewEvent::Download(info) => {}
    WebViewEvent::ProcessGone(reason) => {}  // the control shows its status page with Reload
    WebViewEvent::CloseRequested => {}
    _ => {}
});

// The host's policy (IE's IDocHostUIHandler). Every method has the safe default.
impl WebViewHost for MailSignIn {
    fn new_window(&self, r: &NewWindow) -> Disposition { Disposition::OpenInBrowser }
    fn request(&self, r: &Request) -> Answer { Answer::Default /* the control's own dialog, or refusal */ }
    fn allow_download(&self, d: &DownloadInfo) -> bool { false }
    fn origin_strip(&self) -> OriginStrip { OriginStrip::WhenPasswordsPossible }
}

// The control: draws the page, takes pointer, keys, IME and touch, shows the page's dialogs and its status pages.
rect().child(web_view(&view).host(MailSignIn))
```

What the engine cannot do yet is reported per feature in `Capabilities` (find, permissions, dialogs, certificate
errors, context menus, downloads, hover links, audio, touch, composition, form pickers, favicons, article
detection). A consumer greys such a control out with a reason instead of pretending.

Find in page and download control reach the engine (`Session::find`, `Engine::control_download`) when the engine
takes the calls. The find card steps with Enter and Shift+Enter and counts matches; the downloads list pauses, resumes
and cancels a running download, and "Clear finished" has the engine forget what is over. An engine that lacks a
call leaves `Capabilities::find` or `download_control` off: the command says so, and the buttons are not drawn.

A download that failed or was cancelled shows Retry in the list (the engine's Resume: from where it stopped when the
server takes a range, else from the start). Escape closes the downloads list and the page-info popover even while
the page holds the keyboard, and the downloads button's tooltip goes down when the list opens.

A download says whose it is: `DownloadInfo::private` (the engine's private flag, or the private identity) and
`DownloadInfo::container` (0 is the default). The browser lists a private download in a private window only, and
never in a normal window's list or toasts; it writes no downloads to disk.

A WebExtensions host (a real browser on top of the control) reads the same `ViewState` for `tabs.Tab`: address,
title, icon, status, audible, muted, width, height, article and private. Nothing extension-facing lives only in
Browser.

## References (what was borrowed, why)

No assets were copied; every site, place and person in the frames is invented.

| Browser                                       | Borrowed                                                                                        |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Firefox Proton, 2025 vertical tabs and groups | Strip, toolbar and chosen-tab tones; groups as coloured labels; sidebar panels; DevTools layout |
| Helium                                        | Blocking on by default with no setup; compact layouts                                           |
| Zen                                           | Compact mode; split view with per-pane focus                                                    |
| Arc                                           | Pinned site grid above vertical tabs; the folded rail                                           |
| Vivaldi                                       | Side panel icon rail; bookmarks bar groups                                                      |
| Safari                                        | Tab overview grid; privacy report; reader view; the domain-only address field                   |
| macOS Keychain                                | The certificate viewer                                                                          |
| Orion and Brave                               | Per-site blocking switch; tracker counts                                                        |
| GNOME Web                                     | Adwaita restraint; settings as a window of its own                                              |

## Engine and data notes

| Option                                      | Licence                | Embedding                                                                                                                    |
| ------------------------------------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Gecko via Vivarium (chosen in the worktree) | MPL-2.0                | Process-isolated engine behind a C ABI; capabilities announced at runtime; best privacy features (ETP, RFP, containers, DoH) |
| Servo                                       | MPL-2.0                | Rust-native and embeddable, but web compatibility is not there yet for daily browsing                                        |
| Chromium / CEF                              | BSD-3 plus third-party | Best compatibility; large, Google-service removal is ongoing work (Helium, ungoogled-chromium); Manifest V3 limits uBlock    |
| WebKitGTK                                   | LGPL-2.1 / BSD         | GTK-bound widget; GNOME Web's engine; weaker sandboxing on Linux                                                             |
| Ladybird                                    | BSD-2                  | Independent, promising, not ready                                                                                            |

Blocking: uBlock-class filter lists (EasyList, EasyPrivacy, uBlock filters) in the engine's content blocker,
"Standard / Strict" plus a per-site switch; fingerprinting protection on; HTTPS-first with a warning page; DoH
(Quad9 by default, choosable); containers map onto profiles plus per-site storage partitioning; deceptive-site list
checked locally.

## bingux-ui gaps and proposals

| Proposal                           | What and why                                                                                                                                  | Rust API idea                                                                                      | Reused by                   |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | --------------------------- |
| Web view control                   | The page itself as a bingux-ui control: drawing, input, the page's dialogs and status pages, an embedder policy                               | `web_view(&view).host(policy)`; `WebEngine`, `WebView`, `ViewState`, `WebViewEvent` (sketch above) | Browser, Mail, help viewer  |
| Floating tab strip                 | The new tab model: neutral tiles, natural width to a cap, shrink-to-favicon, scroll, `+N` list                                                | `tab_strip(tabs).max(Dim::TabMax).collapse(TabCollapse::FaviconThenScroll).on_overflow_list()`     | Files, Text Editor, Preview |
| Vertical tab sidebar               | Tabs as stacked tiles in a sidebar: pinned grid, groups as sections, search, collapse to a rail, peek on hover                                | `vertical_tabs(tabs).pinned(grid).groups(..).search(q).collapsed(bool).peek_on_hover()`            | Text Editor, Terminal       |
| Path trail field                   | An address field whose resting state is a clickable trail with an identity chip and trailing chips                                            | `path_field(trail).identity(Identity::Site{..}).chips(vec![Chip::zoom(..)])` + existing `Omnibox`  | Files, Settings search      |
| Omnibox scope chips with key hints | Scopes as a chip strip with Alt+N, footer of key hints                                                                                        | `Omnibox::scopes(..).hints(..)`                                                                    | Files, Calendar search      |
| Split view container               | Two content panes, each with a one-row header and focus mark, a resizable gutter, ratio tip                                                   | `split_view(a, b).ratio(r).pane_header(..)`                                                        | Files, Text Editor, Preview |
| Floating find card                 | Find over the content's top right, X to close, options menu                                                                                   | `find_card(state).options(..)`                                                                     | Text Editor, Preview, Mail  |
| Command-palette menu               | Three-dot main menu with a search field over text rows; typing turns it into commands, settings and tabs results                              | `main_menu(items).search(provider)`                                                                | Every app                   |
| Transfer popover rows              | Progress rows (Starting/running/paused/blocked) with actions and a footer row                                                                 | `transfer_row(op)`                                                                                 | Files operations            |
| Site card / hero popover row       | Popover with a hero row, grouped rows and a footer "… ›" row                                                                                  | `popover_hero(icon, title, status)` + `popover_footer(label)`                                      | Control centre, Settings    |
| Bookmarks bar                      | Chrome-tone bar of chips, folders with menus, overflow, trailing item                                                                         | `item_bar(items).overflow().trailing(..)`                                                          | Files bookmarks             |
| Side panel with icon rail          | A chrome-tone panel whose content is chosen from an icon rail on the window's edge                                                            | `side_panel(panels).rail()`                                                                        | Files, Mail                 |
| Glass header                       | Header bar as glass over immersive content with opaque fallback                                                                               | `HeaderBar::glass()` honouring reduce transparency                                                 | Weather, Maps, Media        |
| Sheet with inset parent            | Phone sheets with INSET_VIEWPORT shown over the dimmed page                                                                                   | exists in docs; frames need a Doop helper                                                          | All apps                    |
| Icons                              | About twenty glyphs the Bingux set lacks (lock, shield, reader, translate, split, passkey, crashed tab, devtools …) are drawn in `_parts.mjs` | add to crates/ui/icons                                                                             | Settings, Files             |

## Contrast (WCAG, computed from the tokens; `.run`-free check in this file's history)

Text on each tier, blue accent. Ratios are ink against the tier's fill (translucent inks composited first).

| Tier                    | Dark: text / muted / accent / warning / danger | Light: text / muted / accent / warning / danger |
| ----------------------- | ---------------------------------------------- | ----------------------------------------------- |
| Content                 | 14.67 / 8.18 / 5.78 / 13.36 / 6.24             | 12.96 / 5.08 / 3.61 / 3.43 / 5.86               |
| Strip                   | 16.26 / 8.84 / 6.46 / 14.94 / 6.97             | 9.89 / 3.88 / 2.76 / 2.61 / 4.47                |
| Toolbar, sidebar        | 12.51 / 7.28 / 4.93 / 11.40 / 5.32             | 11.36 / 4.45 / 3.17 / 3.00 / 5.13               |
| Popover                 | 10.48 / 6.36 / 4.13 / 9.55 / 4.46              | 13.52 / 5.30 / 3.77 / 3.57 / 6.11               |
| Raised (chosen tab)     | 6.40 / 4.27 / 2.50 / 5.78 / 2.70               | 13.52 / 5.30 / 3.77 / 3.57 / 6.11               |
| Control (address field) | 9.26 / 5.74 / 3.65 / 8.43 / 3.94               | 11.99 / 4.70 / 3.34 / 3.17 / 5.41               |

Failures found and what the frames do about them:

- Light unselected tab titles in muted ink on the strip were 3.88: they now use 82% text ink, 6.15.
- Accent as text (links such as "History", "Privacy report") was 3.17-3.77 in light and 4.13 on the dark popover:
  link ink is the accent mixed 74% with black in light (5.91 on content, 6.17 on cards) and 82% with white in dark
  (5.02 on the popover, 7.03 on content). The accent stays a fill (switches, the chosen scope, buttons with fitted ink).
- Warning as text in light ("Not secure", the site card) was 3.00-3.57: it is the warning mixed 60% with black, 7.00
  on the field and 7.90 on popovers. Dark warning text passes as is.
- Muted ink on the raised tile in dark is 4.27; the raised tile only ever carries full text ink (6.40).
- Danger text on the dark popover is 4.46 (proceed/close links are 13.5 px semibold); left as the token value and
  listed as a token issue for `ds-derive`.

## Left

- The contrast check reads the tokens, not the rendered pixels of each frame; the invented sites are not checked.

- Danger text on the dark popover (4.46) is a token matter for `ds-derive`.
