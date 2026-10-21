# Vendored Freya crates

Copies of `freya-winit` and `freya-components` 0.5.0-rc.10, used through
`[patch.crates-io]` in `packages/bingux-ui` and `packages/bingux-settings`.
The only changes are scroll feel for a desktop:

- `freya-winit/src/bingux_scroll.rs` and `renderer.rs`: the wheel step is 64 px
  per notch (upstream 53) and touchpad movement is 1x (upstream 2x).
  `BINGUX_WHEEL_STEP` and `BINGUX_TOUCHPAD_FACTOR` override them.
- `freya-components/src/scrollviews/use_scroll_controller.rs`: a wheel scroll
  eases out over 130 ms (upstream 60 ms on desktop).
  `BINGUX_SCROLL_SMOOTHING` overrides it, in seconds.

Both crates are MIT licensed by the Freya authors. Tests and examples are not
copied. Drop this directory and the patch entries when upstream exposes these
settings.
