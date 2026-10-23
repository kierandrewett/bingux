# Vendored Freya crates

Copies of `freya-winit` and `freya-components` 0.5.0-rc.10, used through
`[patch.crates-io]` in `packages/bingux-ui` and `packages/bingux-settings`.
The only changes are scroll feel for a desktop:

- `freya-winit/src/bingux_scroll.rs` and `renderer.rs`: the wheel step is 64 px
  per notch (upstream 53) and touchpad movement is 1x (upstream 2x).
  `BINGUX_WHEEL_STEP` and `BINGUX_TOUCHPAD_FACTOR` override them.
- `freya-components/src/scrollviews/use_scroll_controller.rs`: wheel and
  keyboard scrolls use an adaptive spring after Firefox's smooth scrolling: a
  motion starts quickly, glides softly while wheel events keep coming and
  settles fast once they stop (upstream: one fixed 60 ms spring).
  `BINGUX_SCROLL_SPEED` scales it (1 default, 2 twice as quick).
- `freya-components/src/scrollviews/scrollview.rs`: a wheel scroll can be
  interrupted. Turning the wheel the other way drops the pending travel and
  starts from the content's current position. `BINGUX_SCROLL_DEBUG=1` prints
  every wheel event and animation frame to stderr.
- `freya-components/src/scrollviews/shared.rs`, `scrollbar.rs`,
  `scrollview.rs` and `virtual_scrollview.rs`: an auto-hidden scrollbar stays
  for 3 s after the last scroll or pointer movement, then fades out over
  400 ms, and fades back in over 120 ms (upstream: it vanishes after 0.8 to
  1.6 s with no fade). The idle check runs every 200 ms.

Both crates are MIT licensed by the Freya authors. Tests and examples are not
copied. Drop this directory and the patch entries when upstream exposes these
settings.
