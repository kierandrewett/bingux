# Capture

`binguxctl capture` opens Bingux Capture from a terminal or script.
Use `binguxctl capture take`, `stop`, `cancel` and `status` to control it.
See [binguxctl](binguxctl.md) for the other shell commands.

Alt+S opens Bingux Capture. Press it again to close the selector, cancel a pending
capture, or stop and save a recording. The top-bar Stop button also ends a recording.

Enter captures. Escape returns from settings to the capture controls. Press Escape
again to close the selector, even when a button has focus. Holding Escape does not
dismiss both pages at once.

Drag to draw a region. Drag inside it to move it, or drag an edge or corner to resize
it. Arrow keys move the region; Shift moves it ten pixels at a time. Reopening the
selector remembers the last region.

Drag the compact toolbar by its background to move it. It fades in over 120 ms and
dims to 25% while you draw, move, or resize a region. It returns to full opacity on
release. Reduced-motion mode skips these fades.

## Select and capture

Settings open inside the same card. The page transition takes 160 ms and the card
resizes over 180 ms. Controls fade between pages. Reduced-motion mode skips the
transition.

Recording settings provide separate System audio and Microphone switches. Both
default to off. Microphone capture uses the default input device. Encoder and capture
backend choices are under Advanced.

Screenshot settings hide video options. Image quality appears only for JPEG. Save to
opens a folder chooser; Reset restores the default output folder.

## Preview and capture targets

On desktops that support screencopy, the selector freezes full-resolution frames
with and without the OS cursor. The cursor toggle switches between those previews.

The selector dismisses open menus only after the frozen frame arrives. A zero-delay
region or screen screenshot uses that same frame. Recordings and delayed screenshots
capture the live desktop after the selector closes. Recording mode shows the live
desktop while you frame the shot.

If the desktop cannot freeze a frame, the selector labels its live-preview fallback.

Region, window and screen modes are available for screenshots and recordings.
Window mode uses the desktop's permission picker and captures the selected window,
not a fixed rectangle that happens to contain it. Availability is detected from
the portal. Screen selection is per monitor.

While recording a region, its white outline and rounded markers remain visible. The
area outside the region is covered with 20% black; the top-bar controls stay clear.

Hovering the top-bar Stop button marks the intended ending. Clicking it trims the
hover and click tail. Moving the pointer away cancels the mark. If trimming fails,
Bingux preserves the original recording and reports its path.

## Capture settings

Capture options persist in `$XDG_CONFIG_HOME/bingux/capture.ini`, normally
`~/.config/bingux/capture.ini`. Defaults and choices are:

| Option          | Accepted values                        | Default                       |
| --------------- | -------------------------------------- | ----------------------------- |
| Mode            | Screenshot, recording                  | Screenshot                    |
| Target          | Region, screen, window                 | Region                        |
| Cursor          | On, off                                | Off                           |
| Copy screenshot | On, off                                | On                            |
| Delay           | 0, 1, 2, 3, 5, or 10 seconds           | 0 seconds                     |
| Image format    | PNG, JPEG                              | PNG                           |
| JPEG quality    | Compact, balanced, high                | Balanced                      |
| Frame rate      | 15, 30, 60 fps                         | 30 fps                        |
| Maximum height  | Original (0), 720, 1080, 1440, 2160 px | 1080 px                       |
| Audio           | None, system, microphone, both         | None                          |
| Encoder         | Automatic, CPU                         | Automatic                     |
| Backend         | Automatic, portal                      | Automatic                     |
| Output folder   | Absolute path                          | XDG Pictures or Videos folder |

The cursor option includes the OS pointer in the screenshot. Copy screenshot controls
whether the saved PNG or JPEG is placed on the clipboard. Turn it off in Screenshot
settings or run `binguxctl capture configure --no-copy`.

System audio records the desktop output, Microphone records the default input, and
Both records both sources. Automatic encoding tries a supported hardware encoder
before falling back to CPU encoding. The Portal backend forces the portable capture
path; Automatic uses Gnoblin's native path when available. See [binguxctl](binguxctl.md)
for the command options and their accepted values.

Return and keypad Enter activate Capture after a toolbar control takes focus.
While settings are open, Enter remains available to settings controls.

## Saved captures

Saved recordings use desktop notifications with Open, Save As, and Discard actions.
Errors use the same notification path and include the location of any preserved
partial recording.

Saved screenshots send a notification with an image preview and Copy, Save As, and
Discard actions. Copy publishes the final PNG or JPEG through the Wayland clipboard.
The clipboard owner stays alive for later paste requests.

Click the notification body to open the image. Save As opens the desktop file chooser
and keeps the original. Discard moves the original to Trash. Actions target the
capture's saved path and refuse to modify a file changed after capture.

## Recording quality

Recordings use H.264 in an MP4 container. The defaults are balanced quality, 30 fps,
and a 1080-pixel height limit. Bingux does not upscale video. Choose 720p or 15 fps
to reduce the workload on a slower machine.

Automatic encoding tests hardware support before use and falls back to x264 or
OpenH264 on the CPU. CPU mode skips the hardware check. A GPU is not required. If a
device fails during recording, Bingux reports the error and preserves the original.

## Capture backend

Capture uses GStreamer and PipeWire with the standard ScreenCast portal. On
Gnoblin/Mutter, Automatic can use native screen and region streams. `grim` is an
optional screenshot and preview fast path. Portal forces the portable capture path.

Window capture and cursor modes depend on the installed desktop portal. Portal
regions require monitor position and size metadata. Unsupported geometry returns an
error instead of capturing the wrong area. Bingux targets Linux desktops.

Frozen previews use private runtime temporary files. Bingux removes them on cancel,
capture completion, or worker exit.

Video is encoded by a worker and streamed to a temporary file in the destination
folder. Bingux renames it after the muxer finishes, without buffering the recording
in memory. Non-empty failed recordings are preserved for recovery. Screen sharing
uses desktop portal permissions.

## Portal requests

`binguxctl capture request` lets a script or the Bingux portal backend capture a region
or the whole desktop. A request has a caller-chosen ID. `--interactive` opens the normal
region selector with the screenshot mode. `--desktop` captures every output with no picker.
The request state is kept for ten minutes, with at most fifteen finished requests. An ID
cannot be reused while its entry is kept.

Requests save a private PNG in the temporary directory, as Gnoblin's portal screenshots do.
The result path names that file. Requests never copy to the clipboard, send a
notification, or play the shutter. Only one capture runs at a time, so
a second request fails at once. Window capture is refused for requests. See
[binguxctl](binguxctl.md) for the command and reply formats.

## Shortcuts

Alt+S is delivered through the running capture process's `ShortcutSession`.
It does not start a command or a UI process. Modifier holds ignore repeat
activations, so holding the keys cannot immediately toggle the selector closed.
Selector visibility and region survive shell configuration reloads.

Set `BINGUX_CAPTURE_SHORTCUT` on `bingux-capture-ui.service` to change the default
`<Alt>s` binding. Remove any duplicate capture command from Gnoblin's
`[[shortcuts]]` configuration. On other compositors, bind `binguxctl capture toggle`.

For compositor layer rules, disable motion, opacity reduction and blur on the
`bingux-capture` surface so selection remains aligned with the captured screen.

## Verify capture

```sh
python3 tests/capture-backend.test.py
python3 tests/capture-notify.test.py
python3 tests/capture-launch.test.py
python3 tests/capture-reload.py
PYTHONDONTWRITEBYTECODE=1 python3 tests/capture-pipeline-lifecycle.test.py
# Isolated notification server; only its generated test image is trashed:
dbus-run-session -- python3 tests/capture-notify-live.py
# Opt-in real keyboard/pointer checks on an idle desktop:
python3 tests/capture-shortcut-live.py
python3 tests/capture-drag-live.py
# Opt-in, with the compact toolbar at its default centered position:
python3 tests/capture-toolbar-live.py
# Opt-in: captures a real 640x360 desktop region; no audio. Outputs in /tmp.
python3 tests/capture-live.py
# Real recording guide, exact scrim opacity, top-bar stop and hover-tail trim:
python3 tests/capture-recording-controls-live.py
# Also records the default system audio and microphone devices for three seconds
# each, then both together. Checks AAC tracks, duration and full decode:
python3 tests/capture-live.py --audio
tests/qml/run.sh CapturePreviewTest.qml
# The interaction fixture checks frozen previews, drawing and resizing, inline
# capture options, keyboard control and Escape without recording:
tests/qml/run.sh CaptureInteractionTest.qml
# The request fixture checks request IDs, duplicate reservation and cancel states:
tests/qml/run.sh CaptureRequestTest.qml
```

Live checks cover CPU and automatic H.264 recording, including duration and full
decode, plus PNG and JPEG screenshots on Gnoblin. The opt-in audio check verifies
system, microphone, and mixed AAC tracks against video duration; it does not measure
speech quality.

Portal window selection, mixed-DPI, and other compositors need interactive platform
coverage. Pure tests cover HiDPI crop arithmetic and unavailable-hardware fallback.

The pipeline lifecycle check checks capture bus-watch cleanup with a bounded
CPU pipeline. It can test either the checkout or an installed backend without
recording the desktop. See `tests/capture-pipeline-lifecycle.md` in the checkout
for the installed-backend command and coverage limits.

## Compositor support

Successful screenshots play the GNOME `screen-capture` shutter event through
libcanberra. The current sound theme and event-sound preference apply. Preview,
cancellation, failure and recording completion do not play the shutter.

The selector runs in `bingux-capture-ui.service`. Its frozen preview uses the
`bingux-capture` layer. The transparent toolbar and settings use a separate
`bingux-capture-controls` layer above the preview, so compositor blur samples the
image behind the controls.

Gnoblin disables blur on the full-screen preview and applies 24-pixel blur to the
controls layer. Other compositors can still show the controls without this blur.
