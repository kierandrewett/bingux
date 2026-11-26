# Browser: permissions in Gecko 160

**Proposal. Needs Kieran's approval.** Nothing here is built, and nothing here is part of the naming plan
(`naming.md`). The naming work changes words and identifiers only: the browser keeps asking and denying exactly as it
does today. This file records what Firefox does at `FIREFOX_160_0_RELEASE`, kind by kind, so that Kieran can choose
whether any behaviour should follow.

Paths are relative to the Gecko root, read on the vivarium box. `:N` is a line at the tag.

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

## Proposal

Each line needs Kieran's approval before anyone builds it. None changes what a page can do by default except where
it says so.

1. **Pop-ups.** Block without a question, as the toast already does. Replace the toast with Firefox's notification
   bar, "prevented this site from opening a pop-up window", with a button that offers to show the blocked pop-up.
   Keep the per-site Allow and Block choice in the site information panel.
2. **Autoplay.** Block audible media by default, as Firefox's `media.autoplay.default = 1`. Show an indicator in the
   address bar and offer Block Audio, Block Audio and Video, and Allow Audio and Video in the site information panel.
   No question.
3. **Clipboard read.** Do not store a permission. Show a one-item "Paste" menu at the pointer, as
   `ClipboardContextMenu` does, and let the page read only what the person chooses to paste.
4. **Persistent storage.** No change: Firefox asks, so the banner stays and takes Firefox's question.

The words for the four kinds stay in the naming plan as they are written there: the question for persistent storage
is Firefox's; the questions for pop-ups, autoplay and clipboard exist only for as long as the browser still asks them.
If Kieran approves 1 to 3, those three questions and their `-prompt` messages go away with the behaviour change.
