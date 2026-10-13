# Panel blur during fades

Gnoblin follows the alpha in each rendered surface automatically. Blur reaches
full strength at about 80% surface opacity and disappears as the surface fades
out. Shells need no blur-fade protocol for this behavior. See
[Gnoblin's blur guidance](https://gnoblin.org/blur-fades).

Bingux's `SurfaceFade` helper enables a temporary Qt layer during a visible
fade. This applies opacity once to overlapping children. At rest it restores
the original layer setting. Shadowed panels use a padded composition item so
the shadow and card share the fade without clipping the shadow.

`ShellPopup` owns its opening and closing animations. The card and outside-click
handling share one native window, created on first use and reused for quick reopening.
Closed cards draw nothing, disable blur and accept no pointer or keyboard
input.

After 30 seconds closed, unpinned windows unmap so Qt can release their
graphics resources. Windows held for editing or retained content stay available.
The Control Centre and More menu resume this timeout when desktop editing closes.
Inline menus keep using their host window.

The window menu follows the same cache lifetime. Its native window stays
unmapped until first use and expires after closing; quick reopening reuses it.

Notification history and toasts share a native window. An empty notification
scene draws nothing and releases input immediately. Its native window unmaps
after 30 seconds without history or visible toasts; new notifications recreate it.

Tooltips and the window switcher can still use compositor transitions when
policy matches. Search, notification cards and other items in a shared buffer
use their own item animations.

The optional `Bingux.Effects` plugin supplies standard background-effect
regions and still sends older blur-fade metadata for compatibility. Current
Gnoblin accepts that metadata but does not use it to calculate blur opacity.
Build the plugin with the same Qt version as Quickshell. Without standard
regions, blur depends on the compositor's rules and alpha-mask support.

For a disposable Gnoblin VM, `tests/popup-cache-live.py` checks real input,
window reuse, closed pixels, automatic blur fading and notification reopening.
Add `--expiry` to check idle release, cold reopening, and pinned-window retention.
`tests/panel-lifecycle-live.py` measures memory growth over repeated openings.
See the [desktop contributor guide](https://github.com/kierandrewett/bingux/blob/main/shell/bingux/README.md) for commands.
