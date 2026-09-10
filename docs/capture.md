# Capture

`binguxctl capture` opens Bingux Capture from a terminal or script.
Use `binguxctl capture take`, `stop`, `cancel` and `status` to control it.
See [binguxctl](binguxctl.md) for the other shell commands.

Alt+S opens Bingux Capture. Press it again to close the selector, cancel a pending
capture or stop and save a recording. The top-bar stop button also ends recording.
Enter captures; Escape closes the selector even after a button or dropdown gains
focus. Drag a region, move its interior or resize its edges/corners. Arrow keys
move the region; Shift moves ten pixels at a time. Reopening remembers the region.

The compact toolbar has a grip on its left; drag it anywhere on the screen.
It fades in over 120 ms and dims to 25% while drawing, moving or resizing a region,
then returns to full opacity on release. Reduced-motion mode skips these fades.
Settings live in a separate popover; dropdowns reuse Bingux's ShellPopup,
ActionButton and MenuNavigator rather than native ComboBox popups.
Mode/target switches use the same SegmentedControl as the sidebar and Control
Centre. Menus measure their field anchor when opened, match its width, and flip
above it when there is insufficient room below.

On screencopy-capable desktops, the selector freezes full-resolution frames both
with and without the OS cursor. The cursor toggle switches the preview variant.
Existing menus, including Search, are dismissed only after the frozen frame has
arrived. Capture does not steal focus and start their exit animation first.
Zero-delay region/screen screenshots are cropped from that exact frozen image;
recordings and delayed screenshots capture live after the selector closes.
PPM frames avoid PNG compression during opening. The measured open-state round
trip on the development desktop fell from about 380 ms to 150 ms, including IPC.
If freezing is unavailable, the selector explicitly labels its live-preview fallback.

Region, window and screen modes are available for screenshots and recordings.
Window mode uses the desktop's permission picker and captures the selected window,
not a fixed rectangle that happens to contain it. Availability is detected from
the portal. Screen selection is per monitor.

Options persist in `$XDG_CONFIG_HOME/bingux/capture.ini` (normally
`~/.config/bingux/capture.ini`): cursor visibility, delay, PNG/JPEG, quality,
clipboard copying, output directory, 15/30/60 fps, resolution limit, system/mic
audio, encoder and backend. Audio defaults off. Screenshots default to
Pictures/Screenshots; recordings to Videos/Recordings, respecting XDG user dirs.

Return and keypad Enter activate Capture after a toolbar control takes focus.
While settings are open, Enter remains available to fields and menus.

Saved recordings use normal desktop notifications with Open, Save As and Discard
actions. Successful saves never display a separate capture feedback panel.
Saved screenshots send a normal desktop notification with an aspect-correct
image preview and standard icon-labelled Copy, Save As and Discard buttons.
Clicking the body opens the image. Save As uses the desktop file chooser and
keeps the original; Discard moves the original to Trash (never permanently
deletes it if trash is unsupported). Actions retain the specific capture path
and refuse to modify a file that changed after capture. Notification actions
remain available across capture-worker/shell reloads.

Recordings are compressed H.264 MP4, defaulting to balanced quality, 30 fps and
a 1080-pixel height limit without upscaling. Automatic encoding tries an actual
synthetic encode before trusting hardware: an installed GPU plugin is not proof
that a compatible device or driver exists. It falls back to x264/OpenH264 on the
CPU. Software only never probes hardware. Native packaging includes CPU encoders;
a GPU is not required. Slower machines can choose 720p/15 fps. A device failure
during an ongoing recording is reported rather than silently dropping frames or
overwriting the original recording.

The worker uses GStreamer/PipeWire with the standard ScreenCast portal. On
Gnoblin/Mutter it can use native screen/region streams, and grim is an optional
screenshot/preview fast path. The Portal option forces the portable path. Window
support and cursor modes depend on the installed desktop portal. Portal regions
need monitor position/size metadata; unsupported geometry produces an explicit
error instead of capturing the wrong area. This is a Linux desktop tool, not a
claim of tested Windows/macOS support.

Frozen previews are held in private runtime temporary files, removed on cancel,
capture completion or worker exit. Video is encoded on a worker process and streamed to a temporary file in
the destination folder, then renamed after muxer finalisation. No full recording
is buffered in memory. Non-empty failed recordings are preserved and their path
is shown for recovery. Screen sharing follows desktop portal permissions.

Alt+S is registered without auto-repeat, so holding the keys cannot immediately
toggle the selector closed. The launcher retries explicit "not ready" replies
during a shell reload, but never retries an uncertain result that might already
have executed. Selector visibility and region survive shell configuration reloads.

Configuration shortcut: `bingux.desktopShell.capture.shortcut` (default `<Alt>s`).
Set the equivalent binding in Gnoblin's TOML configuration. Other
compositors can bind their shortcut to:

```sh
qs ipc --any-display -c bingux call capture open
```

For compositor layer rules, disable motion, opacity reduction and blur on the
`bingux-capture` surface so selection remains aligned with the captured screen.

Checks:

```sh
python3 tests/capture-backend.test.py
python3 tests/capture-notify.test.py
python3 tests/capture-launch.test.py
python3 tests/capture-reload.py
# Isolated notification server; only its generated test image is trashed:
dbus-run-session -- python3 tests/capture-notify-live.py
# Opt-in real keyboard/pointer checks on an idle desktop:
python3 tests/capture-shortcut-live.py
python3 tests/capture-drag-live.py
# Opt-in: captures a real 640x360 desktop region; no audio. Outputs in /tmp.
python3 tests/capture-live.py
qs -p shell/bingux/CapturePreviewTest.qml
# Set BINGUX_CAPTURE_SETTINGS_PATH to a temporary file:// path and
# BINGUX_CAPTURE_TEST_RESULTS to a temporary output path for interaction tests:
qs -p shell/bingux/CaptureInteractionTest.qml
```

Live checks cover CPU and automatic H.264 recording duration/full decode, PNG and
JPEG on Gnoblin. Portal window selection, microphone/system audio, mixed-DPI and
other compositors still require interactive platform coverage; pure tests cover
HiDPI crop arithmetic and unavailable hardware fallback.

Successful screenshots play the GNOME `screen-capture` shutter event through
libcanberra. The current sound theme and event-sound preference apply. Preview,
cancellation, failure and recording completion do not play the shutter.
