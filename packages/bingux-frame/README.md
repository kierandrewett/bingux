# GTK frame helper

Build only the helper with `bash packages/bingux-frame/build.sh OUTPUT_DIR`.
Its transport uses the vendored Gnoblin frame SDK and protocol. See
[window frames](../../docs/window-frames.md) for renderer registration.

## Check readback coverage

Run this from the Bingux checkout, with a neighbouring Gnoblin checkout:

```sh
GSK_RENDERER=gl bash packages/bingux-frame/test-strip-readback.sh /tmp/frame-readback --require-strips
```

For a different checkout layout, set `GNOBLIN_FRAME_MASK_SOURCE` to Gnoblin's
`src/protocols/window-frame/meta-gnoblin-frame-renderer.c`. The check needs
GTK4 4.16 or newer, libadwaita, epoxy, Weston, a C compiler and Python 3.
It starts a private headless Wayland display and bus; it does not use your
current compositor or a VM.

The fixture extracts and executes the native GLSL mask, then compares every
pixel with nonzero coverage against the former full-viewport GTK readback.
Alpha must match exactly; cropped gradients may differ by up to two RGB
levels. Adjacent texels must match too, to cover fractional-scale filtering.

It covers light, dark, gradient and transparent themes, button states,
unequal extents, zero sides, zero extents and short windows. Radius samples
include fractional values, the native maximum and values clamped to it;
exponents cover the supported endpoints and intermediate values.

The painter keeps the four decoration strips and two bottom-corner guards.
Nonzero strips include one adjacent body pixel for texture filtering.

Because the protocol omits radius, each guard uses half the smaller canvas
dimension, rounded up, plus one pixel. Source comments prove coverage for all
native radii and exponents. Extreme unequal sides, clamped extents and
snapshots without a proven opaque body retain full readback.

## Measure readback cost

The test also reports readback bytes, peak texture size and median timings
for 720p, 1080p and 4K canvases. Maximum-radius guards substantially reduce
the earlier strips-only savings; use these measured bytes rather than a
fixed speedup claim. Timings depend on the renderer and theme. This check
does not establish compositor frame pacing or session memory lifetime.

## Check title-button layouts

```sh
bash packages/bingux-frame/test-button-layout.sh /tmp/frame-button-layout
```

This starts a private headless Weston display and an isolated settings bus.
It exercises the production painter with GTK layout changes on the same frame:
three buttons, close only, no buttons, and buttons on both sides. It checks
pixels, semantic click regions, redraw requests, and the explicit
`--button-layout=:close` override. It needs GTK4, libadwaita, Weston, a C
compiler, and D-Bus. It does not change desktop settings or use the active
session. Native compositor input and configuration reload still need a VM check.
