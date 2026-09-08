# Recording and privacy indicators

Bingux follows GNOME Shell's panel activity design:

- Recording: a filled Adwaita red button, elapsed `m:ss` time, and the rounded square stop glyph.
- Screen sharing: a filled Adwaita orange button with screen-sharing and stop symbols.
- Camera: an orange `camera-web-symbolic` icon while GNOME reports a camera in use.
- Microphone and location: orange symbolic icons with descriptive tooltips.

All icons use 16px slots and the bar's existing font. Buttons retain a full-height
hit target, with inset backgrounds. Recording and privacy indicators never move
into the overflow menu. The sharing indicator remains visible for at least five
seconds, as in GNOME; its stop action is disabled after sharing ends.

`ActivityIndicator.qml` owns the common presentation and keyboard/accessibility
activation. `RecordingIndicator.qml` uses the capture worker's actual start time
and stops it through its normal finalisation path, so the recording is saved.
Other recordings use the compositor's start time and stop action. `PrivacyState.qml`
subscribes to the persistent compositor connection; it does not poll processes.

Gnoblin's bridge uses `Shell.CameraMonitor`, the same PipeWire monitor as GNOME,
and Mutter's remote-access handles. Active handle start times survive script
reloads. A temporary connection loss retains the last known indicators; stop
controls are disabled until the connection returns. Microphone and location
state continue to come from the existing metrics service.

The design references are GNOME Shell's `js/ui/status/remoteAccess.js`,
`js/ui/status/camera.js`, and `data/theme/gnome-shell-sass/widgets/_panel.scss`.
The recording colour is Adwaita red 4 (`#c01c28`); privacy is orange 3 (`#ff7800`).
The stop glyph is a 10px rounded square centred inside its 16px slot.

## Validation

`tests/privacy-indicators.py` runs the QML state, timer, control and layout checks
through Gnoblin's private headless launcher, with `QS_TEST_BIN` set to the desired
Quickshell runtime. Gnoblin's `tests/privacy-bridge.py` creates real Mutter capture
sessions and checks separate stop controls and timer continuity across reloads.
Use a private `GNOBLIN_COMPOSITOR_SOCKET` when running compositor tests.

Activity indicators use 12px horizontal padding per side, with semibold text, 16px icons and a full-height pointer target. Saved recordings and screenshots go through the normal desktop notification service, including Open, Save As and Discard actions; only screenshots offer Copy and an image preview. Successful saves never open the custom capture feedback panel.
