# Emoji picker

Press Win+Period to open the visual picker. Search by name, select an emoji with
the arrow keys, then press Enter to insert it into the original input. Tab moves
to the skin-tone button, then the category strip; Left and Right change category. Escape or a click outside
the picker closes it. The picker has no title or close button.

Each emoji appears once. The hand button to the right of search selects the
preferred skin tone (yellow or one of five skin tones). The choice is saved and
applies to search, categories and recent emoji. Existing recent variants are
folded together. The 3,944 Unicode sequences form 1,914 distinct picker entries.

The picker first uses Gnoblin's native text-input rectangle. It opens above the
caret when space permits, below it near the top of a screen, or beside it when
neither vertical position fits. The rectangle follows window movement. Without
native geometry, a bounded AT-SPI query checks the focused application's caret.
The pointer is the fallback when neither source provides usable geometry.

Electron 39.2.7 exposes its focused input through AT-SPI but can return empty
character bounds. Its Chromium version does not request inline text boxes from
the character-extents call. The native Wayland rectangle avoids this dependency
and also works for empty inputs. No renderer-accessibility launch flag is needed.
See [Chromium 142's accessibility implementation](https://chromium.googlesource.com/chromium/src/+/142.0.7444.175/ui/accessibility/platform/ax_platform_node_auralinux.cc).

After the picker closes, Gnoblin commits the selected Unicode sequence to the
focused input. It does not copy the emoji to the clipboard. A missing input or
changed window focus produces an error instead of an unverified insertion.

## Verification

Run the isolated integration test with an installed Gnoblin prefix and an Electron
executable. The default prefix is the sibling Gnoblin checkout's `install/`.

```sh
GNOBLIN_PREFIX=/usr BINGUX_TEST_ELECTRON=/path/to/electron tests/emoji-electron.sh
node --test tests/emoji-search.test.mjs tests/popup-placement.test.mjs
```

The integration test starts a private compositor, copies the picker into a
temporary configuration, and uses a local Electron test page. It prevents IBus
from connecting to the host daemon. Optional `BINGUX_EMOJI_SCREENSHOT` names a PNG
to capture while the picker is open.

Verified on Electron 39.2.7 with native Wayland:

- Win+Period, search and Enter in a normal input, an empty input, a multiline
  textarea and a contenteditable element.
- Exact received text for a plain emoji, a joined emoji and a skin-tone sequence.
- Insertion in the middle of a line and replacement of selected text.
- Caret placement that leaves the target line visible.
- Input blur clears the available caret and rejects insertion.
- Picker category navigation, search, recents, empty results and Escape.

Native GTK insertion also passed on the live session. Discord Canary 0.0.820
(Electron 37.6.0) was verified on native Wayland: its search input received
`😀👩🏽‍💻👍🏽` through the compositor, with no chat message sent.

Older Electron apps must start with `--ozone-platform=wayland --enable-wayland-ime`.
Discord's local desktop launcher was updated with these flags. Launch flags take
effect after restarting the app. XWayland does not expose the required native
text-input interface and is not supported by this insertion path.
