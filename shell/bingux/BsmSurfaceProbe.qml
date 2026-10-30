import QtQuick
import QtQuick.Window
import Quickshell

// Evidence that one layer-shell window has really been drawn, for bsm's
// readiness gate (BsmReadiness.qml). Created only in integrated mode.
//
// `rendered` turns true when Qt finishes swapping a frame for the window's
// backing surface while the window is mapped, has a size and its content is
// ready. Setting `visible` is not enough. It falls back to false when the
// window is unmapped, and a window recreated by a reload starts at false. It
// does not fall back on later content changes: a wallpaper crossfade after the
// first frame must not withdraw readiness. This proves a frame was committed
// to the compositor; that bsm received and presented it is bsm's own check.
QtObject {
    id: probe

    // The PanelWindow to watch.
    property var window: null
    // wallpaper | bar | dock
    property string role: ""
    // False while the window has nothing final to show (a configured wallpaper
    // that is still decoding). Frames swapped meanwhile do not count.
    property bool contentReady: true
    // The component whose process owns this window: wallpaper | desktop
    property string component: ""

    readonly property string output: window && window.screen && window.screen.name ? window.screen.name : ""
    property bool rendered: false

    readonly property var surface: window && window.backingWindowVisible && window.contentItem ? window.contentItem.Window.window : null

    readonly property Connections frames: Connections {
        target: probe.surface
        ignoreUnknownSignals: true
        function onFrameSwapped() {
            if (!probe.rendered && probe.contentReady && probe.window.width > 0 && probe.window.height > 0)
                probe.rendered = true;
        }
    }

    readonly property Connections visibility: Connections {
        target: probe.window
        ignoreUnknownSignals: true
        function onBackingWindowVisibleChanged() {
            if (!probe.window.backingWindowVisible)
                probe.rendered = false;
        }
    }

    onRenderedChanged: BsmReadiness.changed()
    onOutputChanged: BsmReadiness.changed()

    // Properties arrive after construction when a Loader creates the probe,
    // so registration is explicit.
    function attach() {
        BsmReadiness.register(probe);
    }

    Component.onDestruction: BsmReadiness.unregister(probe)
}
