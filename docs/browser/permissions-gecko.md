# Browser: permissions in Gecko 160

**Approved 2026-11-26 (follow Firefox 160).** Kieran's rule is Gecko's behaviour as well as its names, and he did not
object to the proposal below. It is its own step in the phase 2 plan, after the permission naming step (`naming.md`,
step 8b). Nothing here is built yet.

This file records what Firefox does at `FIREFOX_160_0_RELEASE`, kind by kind, what the engine gives the browser to do
the same, and what Vivarium would have to add.

Paths are relative to the Gecko root, read on the vivarium box. `:N` is a line at the tag. Paths under Vivarium are
relative to `~/dev/vivarium`.

## What the browser does today

- The engine reports a `PermissionRequest` of a `PermissionKind` (`crates/ui/src/web_view/requests.rs`).
- The browser shows one banner for it, with Block, Allow this time and Allow (`permissions/banner.rs`).
- That holds for all ten kinds, including autoplay, storage, pop-ups and clipboard.
- A pop-up opened by a page is blocked with a toast, `browser-tabs-popup-blocked-notification`, in `tabs/`.

## What Gecko does, kind by kind

| Kind (Gecko id)                     | Default                                                                                              | What Firefox shows                                                                                                                                             | Source                                                                                                                                                                                |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `geo`                               | ask                                                                                                  | A prompt: "Allow { host } to access your location?" with Allow, Block and "Remember this decision"                                                             | `browser/modules/SitePermissions.sys.mjs:1018`; `browser.properties:148,158`                                                                                                          |
| `desktop-notification`              | ask                                                                                                  | A prompt: "Allow { host } to send notifications?" with Allow and Always Block                                                                                  | `SitePermissions.sys.mjs:934`; `browser.properties:180-186`                                                                                                                           |
| `camera`, `microphone`, `screen`    | ask                                                                                                  | A prompt per kind, and whole prompts for camera with microphone or screen with microphone                                                                      | `SitePermissions.sys.mjs:939,958,962`; `webrtcIndicator.ftl:130-137`                                                                                                                  |
| `midi`                              | ask, only when the Web MIDI pref is on                                                               | A prompt: "Allow { host } to access your MIDI devices?"                                                                                                        | `SitePermissions.sys.mjs:1091,1124`; `browser.properties:283`                                                                                                                         |
| `persistent-storage`                | ask                                                                                                  | **A prompt**: "Allow { host } to store data in persistent storage?", Allow and Block, and "Remember this decision"                                             | `browser/modules/PermissionUI.sys.mjs:1848-1925`; `browser.properties:170-174`; `SitePermissions.sys.mjs:1047`                                                                        |
| `popup`                             | **blocked**: `dom.disable_open_during_load` is `true` in the browser (`false` in the engine default) | **No prompt.** A notification bar: "{ brand } prevented this site from opening a pop-up window.". An address bar indicator                                     | `browser/app/profile/firefox.js:1305`; `StaticPrefList.yaml:2852`; `PopupAndRedirectBlockerObserver.sys.mjs:101-135`; `browser.ftl:177,1585,1615,1629`; `SitePermissions.sys.mjs:975` |
| `autoplay-media`                    | **audible media blocked**: `media.autoplay.default` is `1` (`0` allows, `5` blocks all)              | **No prompt.** An address bar indicator. The identity panel offers Block Audio, Block Audio and Video, Allow Audio and Video                                   | `StaticPrefList.yaml:12479`; `SitePermissions.sys.mjs:867-879`; `browser.ftl:179`; `sitePermissions.properties:30-32`                                                                 |
| clipboard read (no id)              | not stored; absent from `SitePermissions` (`:867-1091`)                                              | **No prompt.** `navigator.clipboard.readText()` or `read()` opens a one-item "Paste" menu at the pointer; the page gets the text only if the person chooses it | `toolkit/modules/ClipboardContextMenu.sys.mjs:11,161`; `textActions.ftl` `text-action-paste`                                                                                          |
| storage access (cross-site cookies) | ask                                                                                                  | A prompt: "Allow { site } to use its cookies on { site }?"; the identity panel lists the parties under "Cross-site cookies"                                    | `PermissionUI.sys.mjs:2308`; `browser.properties:308`; `browser.ftl:485-487`                                                                                                          |

`permissions.default.*` prefs exist for `camera`, `microphone`, `geo`, `desktop-notification`, `xr`, `shortcuts`,
`image` and a few more (`modules/libpref/init/`). They set a default for a whole kind.

## Where Bingux differs

| Kind                                                      | Bingux today                                            | Firefox                                                                                                                                                        | Difference                                                                                   |
| --------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| location, notifications, camera, microphone, screen, MIDI | A banner with three buttons                             | A prompt with Allow and Block, and a remember box                                                                                                              | Wording only (the naming plan)                                                               |
| persistent storage                                        | A banner                                                | A prompt                                                                                                                                                       | None of substance: Firefox asks too                                                          |
| pop-ups                                                   | A banner when the engine asks; a toast when blocked     | Blocked, notification bar, no prompt                                                                                                                           | Behaviour                                                                                    |
| autoplay                                                  | A banner when the engine asks                           | Blocked by default for audible media, indicator only                                                                                                           | Behaviour                                                                                    |
| clipboard read (no id)                                    | not stored; absent from `SitePermissions` (`:867-1091`) | **No prompt.** `navigator.clipboard.readText()` or `read()` opens a one-item "Paste" menu at the pointer; the page gets the text only if the person chooses it | `toolkit/modules/ClipboardContextMenu.sys.mjs:11,161`; `textActions.ftl` `text-action-paste` |
| storage access                                            | Not offered                                             | A prompt                                                                                                                                                       | Missing feature; the engine may not request it                                               |

## What to build

Four changes. Together they follow Firefox 160 for these four kinds.

1. **Pop-ups.** Blocked by default, with no question. Firefox's notification bar, "prevented this site from opening a
   pop-up window", and its address bar indicator replace the toast. Keep the per-site Allow and Block choice in the
   site information panel.
2. **Autoplay.** Audible media blocked by default, with no question. The address bar indicator, and the site
   information panel's Block Audio, Block Audio and Video, and Allow Audio and Video.
3. **Clipboard read.** No stored permission and no banner. A one-item "Paste" menu at the pointer, as
   `ClipboardContextMenu` does. The page reads only what the person chooses to paste.
4. **Persistent storage.** No change to behaviour: Firefox asks, so the banner stays and takes Firefox's question.

Once 1 to 3 land, the banner no longer asks about pop-ups, autoplay or clipboard, so their `-prompt` messages and
`PermissionBanner` branches go with them.

## What the engine gives the browser

Checked in `~/dev/vivarium` at `cb216ae`. "Vivarium change" means Vivarium cannot do it today.

| Need                               | Vivarium today                                                                                                                                                                          | The browser must                                                                                                               | Vivarium change                                                                                                             |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Pop-up blocker on                  | Pref `dom.disable_open_during_load` is on the host's allowlist (bool, `src/vivarium/protocol/vivarium_pref_allow.h:44`). The engine keeps Gecko's default, `false` (`docs/abi.md:1412`) | Set it `true` at start, as Firefox's profile does (`firefox.js:1305`)                                                          | None                                                                                                                        |
| How the pref gets there            | `vv_engine_config.prefs`, and `vv_engine_pref_set` (`docs/abi.md:487`); the Rust crate has `Config::prefs` (`crates/vivarium/src/config/mod.rs:281`)                                    | Add a `prefs` list to `web_view::engine::Config` and pass it on. It has none today (`crates/ui/src/web_view/engine/config.rs`) | None. The change is in `crates/ui`                                                                                          |
| Bar text and count                 | `VV_EVENT_POPUP_BLOCKED`: the count and the first address (`docs/abi.md:1412`)                                                                                                          | Show the bar and the indicator                                                                                                 | None                                                                                                                        |
| "Allow pop-ups for this site"      | A `popup` permission in the host's store lets the opens through (`docs/abi.md:847`)                                                                                                     | Write ALLOW with `vv_contextual_identity_permission_set`                                                                       | None                                                                                                                        |
| "Show { address }" for each pop-up | No list of blocked addresses, and no call to open one                                                                                                                                   | Offer the one item Firefox's menu starts with, or leave it out                                                                 | **Yes, optional**: the list, and a call that opens one                                                                      |
| Autoplay default                   | `media.autoplay.default` (0 to 5) and `media.autoplay.blocking_policy` are on the allowlist (`:49-50`); Gecko's default is `1`, block audible                                           | Nothing at start. Set the pref only for a person's global choice                                                               | None                                                                                                                        |
| Autoplay indicator                 | No event for "autoplay was blocked" (`docs/research/api-parity.md:186`, PM7: "the blocked indicator missing")                                                                           | Show the indicator when the page was blocked                                                                                   | **Yes**: an event per session, or a readable state                                                                          |
| Autoplay per site                  | Type `autoplay-media` is settable, with ALLOW, DENY or DEFAULT (`docs/abi.md:845-847`)                                                                                                  | Allow Audio and Video is ALLOW; Block Audio is DENY                                                                            | **Yes** for Block Audio and Video, a third state (`SitePermissions.sys.mjs:877`) `[INFERENCE: the store code was not read]` |
| Clipboard read                     | A `PERMISSION` request with one item `CLIPBOARD_READ`, never stored; then a `CLIPBOARD_READ` request for the data (`docs/abi.md:867-868`)                                               | Show the Paste menu at the last pointer position; `ACCEPT` with the system clipboard, else `REFUSE`                            | None                                                                                                                        |
| Persistent storage                 | `PERSISTENT_STORAGE` is raised as a request (`docs/abi.md:850`)                                                                                                                         | Keep the banner, with Firefox's question                                                                                       | None. The quota-flag question (`docs/abi.md:857-862`) is separate and stays Kieran's                                        |

Vivarium changes to list for Main:

1. A "blocked autoplay" event for a session (or a readable temporary state). Needed for the address bar indicator.
2. A third `autoplay-media` state, Block Audio and Video, in the permission store. Needed for the site information panel.
3. Optional: the addresses of blocked pop-ups and a call that opens one. Needed only for Firefox's per-pop-up menu items.

The pref change in `crates/ui` is the browser's side: a `prefs` list on the engine config, set to
`dom.disable_open_during_load = true` for every window, private or not.
