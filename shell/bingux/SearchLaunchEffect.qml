import QtQuick
import Quickshell.Widgets

// An independent icon copy: neither list clipping nor the closing card's
// opacity/scale affects the launch animation.
Item {
    id: root
    objectName: "searchLaunchEffect"
    property string source: ""
    property bool symbolic: false
    readonly property real maximumScale: 4
    readonly property bool running: launch.running
    signal finished
    visible: running
    z: 100

    function play(icon, iconSource, isSymbolic, clickPosition) {
        launch.stop();
        if (Theme.reducedMotion || !icon || !iconSource) return;
        const position = icon.mapToItem(parent, 0, 0);
        x = clickPosition ? clickPosition.x - icon.width / 2 : position.x;
        y = clickPosition ? clickPosition.y - icon.height / 2 : position.y;
        width = icon.width;
        height = icon.height;
        source = iconSource;
        symbolic = isSymbolic;
        scale = 1;
        opacity = 1;
        launch.restart();
    }

    function cancel() { launch.stop(); }

    OsIconImage {
        // Rasterise at the largest displayed size, then shrink for the start.
        anchors.centerIn: parent
        width: root.width * root.maximumScale
        height: root.height * root.maximumScale
        scale: 1 / root.maximumScale
        visible: !root.symbolic
        source: root.symbolic ? "" : root.source
    }
    SymbolicIcon {
        // Rasterise at the largest displayed size, then shrink for the start.
        anchors.centerIn: parent
        width: root.width * root.maximumScale
        height: root.height * root.maximumScale
        scale: 1 / root.maximumScale
        visible: root.symbolic
        source: root.source
        color: Theme.muted
    }
    ParallelAnimation {
        id: launch
        NumberAnimation { target: root; property: "scale"; from: 1; to: root.maximumScale; duration: 260; easing.type: Easing.OutCubic }
        NumberAnimation { target: root; property: "opacity"; from: 1; to: 0; duration: 260; easing.type: Easing.OutCubic }
        onFinished: root.finished()
    }
}
