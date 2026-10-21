import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Wayland

Variants {
    model: Quickshell.screens

    PanelWindow {
        id: window
        required property var modelData
        screen: modelData

        // Let Gnoblin's compositor background show through while no usable
        // wallpaper image is configured.
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.namespace: "bingux-wallpaper"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }

        // Two layers: the wallpaper on show and the one fading in. The first
        // wallpaper eases in with a slight zoom-out, and later changes crossfade.
        Item {
            id: stage
            anchors.fill: parent

            property bool firstShow: true
            property bool frontIsA: true
            readonly property Image front: frontIsA ? layerA : layerB
            readonly property Image back: frontIsA ? layerB : layerA
            readonly property url wanted: BinguxPreferences.wallpaperUrl
            // Decode at the settled physical size. Enlarging the decode for the
            // login zoom resamples matching-size images twice and softens edges.
            // Use the window ratio: the screen ratio can round fractional scales up.
            // Keep this size fixed throughout the animation.
            readonly property size decodeSize: Qt.size(Math.ceil(width * window.devicePixelRatio), Math.ceil(height * window.devicePixelRatio))

            function show(image) {
                const first = firstShow;
                firstShow = false;
                front.z = 0;
                image.z = 1;
                frontIsA = image === layerA;
                image.scale = first ? 1.06 : 1;
                image.opacity = 0;
                reveal.target = image;
                reveal.zoom = first;
                reveal.restart();
            }

            function ready(image) {
                if (!reveal.running && image.status === Image.Ready && image.source === wanted && back === image)
                    show(image);
            }

            function load() {
                if (wanted.toString() === "") {
                    reveal.stop();
                    layerA.opacity = 0;
                    layerB.opacity = 0;
                    layerA.scale = 1;
                    layerB.scale = 1;
                    layerA.source = "";
                    layerB.source = "";
                    return;
                }
                if (width <= 0 || height <= 0)
                    return;
                // Keep both images intact until their current fade ends. The
                // wanted binding retains the latest choice without a third layer.
                if (reveal.running)
                    return;
                if (front.source === wanted && front.status !== Image.Null)
                    return;
                back.source = wanted;
                if (back.status === Image.Ready)
                    show(back);
            }

            onWantedChanged: load()
            onWidthChanged: load()
            onHeightChanged: load()
            Component.onCompleted: load()

            Image {
                id: layerA
                anchors.fill: parent
                asynchronous: true
                cache: true
                sourceSize: stage.decodeSize
                retainWhileLoading: true
                fillMode: Image.PreserveAspectCrop
                opacity: 0
                onStatusChanged: stage.ready(layerA)
            }

            Image {
                id: layerB
                anchors.fill: parent
                asynchronous: true
                cache: true
                sourceSize: stage.decodeSize
                retainWhileLoading: true
                fillMode: Image.PreserveAspectCrop
                opacity: 0
                onStatusChanged: stage.ready(layerB)
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.RightButton
                onPressed: mouse => {
                    const shellPath = Quickshell.shellPath("shell.qml");
                    // The installed launchers export BINGUX_QUICKSHELL. Packaged
                    // installs require /usr/bin/qs, which is the fallback.
                    const quickshell = Quickshell.env("BINGUX_QUICKSHELL") || "qs";
                    Quickshell.execDetached([quickshell, "-p", shellPath, "ipc", "call", "shell", "desktopMenu", window.screen.name, String(mouse.x), String(mouse.y)]);
                }
            }

            ParallelAnimation {
                id: reveal
                property Item target
                property bool zoom: false
                NumberAnimation {
                    target: reveal.target
                    property: "opacity"
                    to: 1
                    duration: Theme.reducedMotion ? 0 : (reveal.zoom ? 900 : 600)
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    target: reveal.target
                    property: "scale"
                    to: 1
                    duration: Theme.reducedMotion ? 0 : (1100)
                    easing.type: Easing.OutCubic
                }
                onFinished: {
                    const old = stage.back;
                    old.opacity = 0;
                    if (old.source !== stage.wanted)
                        old.source = "";
                    stage.load();
                }
            }
        }
    }
}
