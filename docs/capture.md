# Capture

`binguxctl capture` opens Bingux Capture from a terminal or script.
Use `binguxctl capture take`, `stop`, `cancel` and `status` to control it.
See [binguxctl](binguxctl.md) for the other shell commands.

Alt+S opens Bingux Capture. Press it again to close the selector, cancel a pending
capture or stop and save a recording. The top-bar stop button also ends recording.
Enter captures; Escape returns from settings to capture controls, then a second
Escape closes the selector, including when a button has focus. Holding Escape
does not dismiss both pages. Drag a region, move its interior or resize its edges/corners. Arrow keys
move the region; Shift moves ten pixels at a time. Reopening remembers the region.

Drag the compact toolbar by its background to move it around the screen.
It fades in over 120 ms and dims to 25% while drawing, moving or resizing a region,
then returns to full opacity on release. Reduced-motion mode skips these fades.
Settings replace the controls inside the same card, using Control Centre's
160 ms page transition and 180 ms resize timing. Each page stays at its final
screen position while the card reveals it; controls do not reflow or fly with
the moving edge. Outgoing controls fade before incoming controls appear.
Changing direction mid-transition continues from the current size. Reduced
motion skips the transition. Back is a fixed 36-pixel square.
Recording settings start with independent System audio and Microphone switches.
Both default to off. Enable either source or both in settings.
Microphone uses the default input device. Encoder and capture
backend choices are under Advanced. Screenshot settings hide video options and
show image quality only for JPEG.
Settings use Control Centre's SettingsRow switches and inline SegmentedControl
choices, without nested popovers. Save to opens a folder chooser; Reset restores
the default capture folder.

On screencopy-capable desktops, the selector freezes full-resolution frames both
with and without the OS cursor. The cursor toggle switches the preview variant.
Existing menus, including Search, are dismissed only after the frozen frame has
arrived. Capture does not steal focus and start their exit animation first.
Zero-delay region/screen screenshots are cropped from that exact frozen image;
recordings and delayed screenshots capture live after the selector closes.
Recording mode also shows the live desktop while framing the shot.
PPM frames avoid PNG compression during opening. The measured open-state round
trip on the development desktop fell from about 380 ms to 150 ms, including IPC.
If freezing is unavailable, the selector explicitly labels its live-preview fallback.

Region, window and screen modes are available for screenshots and recordings.
Window mode uses the desktop's permission picker and captures the selected window,
not a fixed rectangle that happens to contain it. Availability is detected from
the portal. Screen selection is per monitor.

While recording a region, its white outline and rounded markers remain visible.
The area outside is covered with 20% black, with the top-bar controls left clear.
The guide is input-transparent and its border/markers sit outside the recorded
crop. It uses the same screen-aligned, unblurred compositor policy as selection.
Hovering the top-bar Stop button marks the intended ending; clicking trims the
hover/click tail with ffmpeg. Leaving the button cancels that mark. A failed trim
preserves the original recording and reports its path.

Options persist in `$XDG_CONFIG_HOME/bingux/capture.ini` (normally
`~/.config/bingux/capture.ini`): cursor visibility, delay, PNG/JPEG, quality,
clipboard copying, output directory, 15/30/60 fps, resolution limit, system/mic
audio, encoder and backend. Audio defaults off. Screenshots default to
Pictures/Screenshots; recordings to Videos/Recordings, respecting XDG user dirs.

Return and keypad Enter activate Capture after a toolbar control takes focus.
While settings are open, Enter remains available to settings controls.

Saved recordings use normal desktop notifications with Open, Save As and Discard
actions. Errors also use the normal notification path, including worker failures
and any preserved partial-recording path. There is no separate capture feedback
panel, and replaying an error after a UI reload does not notify it again.
Saved screenshots send a normal desktop notification with an aspect-correct
image preview and standard icon-labelled Copy, Save As and Discard buttons.
Screenshot copying publishes the final PNG/JPEG through the native Wayland
clipboard source; it does not spawn `wl-copy`. A small native owner process
keeps that source alive for later paste requests.
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

Alt+S is delivered through the running capture process's `ShortcutSession`.
It does not start a command or a UI process. Modifier holds ignore repeat
activations, so holding the keys cannot immediately toggle the selector closed.
Selector visibility and region survive shell configuration reloads.

Set `BINGUX_CAPTURE_SHORTCUT` on `bingux-capture-ui.service` to change the default
`<Alt>s` binding. Remove any duplicate capture command from Gnoblin's
`[[shortcuts]]` configuration. On other compositors, bind `binguxctl capture toggle`.

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
# Real recording guide, exact scrim opacity, top-bar stop and hover-tail trim:
python3 tests/capture-recording-controls-live.py
# Also records the default system audio and microphone devices for three seconds
# each, then both together. Checks AAC tracks, duration and full decode:
python3 tests/capture-live.py --audio
qs -p shell/bingux/CapturePreviewTest.qml
# Set BINGUX_CAPTURE_SETTINGS_PATH to a temporary file:// path and
# BINGUX_CAPTURE_TEST_RESULTS to a temporary output path for interaction tests:
qs -p shell/bingux/CaptureInteractionTest.qml
# With the same temporary settings/report environment, test audio switches,
# Advanced settings, keyboard control, resize and Escape without recording:
qs -p shell/bingux/CaptureSettingsTest.qml
```

Live checks cover CPU and automatic H.264 recording duration/full decode, PNG and
JPEG on Gnoblin. The opt-in audio check covers system, microphone and mixed AAC
tracks with matching video duration. It does not establish microphone speech
quality. Portal window selection, mixed-DPI and other compositors still require
interactive platform coverage; pure tests cover
HiDPI crop arithmetic and unavailable hardware fallback.

Successful screenshots play the GNOME `screen-capture` shutter event through
libcanberra. The current sound theme and event-sound preference apply. Preview,
cancellation, failure and recording completion do not play the shutter.

The selector runs in `bingux-capture-ui.service`. Its frozen preview uses the
`bingux-capture` layer. The toolbar and settings use a separate transparent
`bingux-capture-controls` layer above it, so compositor blur samples the preview.
The packaged Gnoblin integration keeps blur disabled for `bingux-capture` and
enables 24px blur for `bingux-capture-controls`; the latter also publishes the
toolbar/settings geometry through the native background-effect protocol and the
legacy `BlurRegion` fallback.
`CaptureShell.qml` publishes their stacking relationship through `UiSession`.
