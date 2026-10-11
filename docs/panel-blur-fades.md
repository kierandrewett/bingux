# Panel blur during fades

Gnoblin follows the alpha in each rendered surface automatically. Blur reaches
full strength at about 80% surface opacity and disappears as the surface fades
out. Shells need no blur-fade protocol for this behavior. See
[Gnoblin's blur guidance](https://gnoblin.org/blur-fades).

Bingux's `SurfaceFade` helper enables a temporary Qt layer during a visible
fade. This applies opacity once to overlapping children. At rest it restores
the original layer setting. Shadowed panels use a padded composition item so
the shadow and card share the fade without clipping the shadow.

`ShellPopup` owns its opening and closing animations. Its native card and
dismissal windows are created on first use and reused on subsequent openings.
Closed cards draw nothing, disable blur and accept no pointer or keyboard
input. Inline menus keep using their host window. Notification history and
toasts also reuse their native window after first use.

An empty notification scene draws
nothing and releases input.

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
`tests/panel-lifecycle-live.py` measures memory growth over repeated openings.
See the [desktop contributor guide](https://github.com/kierandrewett/bingux/blob/main/shell/bingux/README.md) for commands.
