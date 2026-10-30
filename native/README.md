# Native components

C and C++ code, built by the top-level `Makefile` rather than Cargo.

| Directory | What it is |
| --------- | ---------- |
| `effects/` | QML plugin for the background effect (`ext-background-effect-v1`). |
| `text-layout/` | QML plugin for document text layout and spacing. |
| `wayland-sync/` | QML plugin for launch activation over Wayland. |
| `audio-meter/` | Reads per-application peak levels from PulseAudio monitor streams. |
| `image-clipboard/` | Clipboard helper that uses `ext-data-control-v1`. |
| `frame/` | The GTK window frame helper that Gnoblin starts to draw window frames. See its README. |
