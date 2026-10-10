import QtQuick
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
                if (image.status === Image.Ready && image.source === wanted && back === image)
                    show(image);
            }

            function load() {
                if (wanted.toString() === "") {
                    layerA.source = "";
                    layerB.source = "";
                    return;
                }
                if (front.source === wanted && front.status !== Image.Null)
                    return;
                back.source = wanted;
                if (back.status === Image.Ready)
                    show(back);
            }

            onWantedChanged: load()
            Component.onCompleted: load()

            Image {
                id: layerA
                anchors.fill: parent
                asynchronous: true
                cache: true
                fillMode: Image.PreserveAspectCrop
                opacity: 0
                onStatusChanged: stage.ready(layerA)
            }

            Image {
                id: layerB
                anchors.fill: parent
                asynchronous: true
                cache: true
                fillMode: Image.PreserveAspectCrop
                opacity: 0
                onStatusChanged: stage.ready(layerB)
            }

            ParallelAnimation {
                id: reveal
                property Item target
                property bool zoom: false
                NumberAnimation {
                    target: reveal.target
                    property: "opacity"
                    to: 1
                    duration: reveal.zoom ? 900 : 600
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    target: reveal.target
                    property: "scale"
                    to: 1
                    duration: 1100
                    easing.type: Easing.OutCubic
                }
                onFinished: {
                    const old = stage.back;
                    old.opacity = 0;
                    old.source = "";
                }
            }
        }
    }
}
