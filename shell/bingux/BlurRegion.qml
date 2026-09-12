import QtQuick
import Quickshell

QtObject {
    id: root
    required property var window
    required property string surfaceNamespace
    required property rect region
    property var published: null
    onRegionChanged: Qt.callLater(BlurRegions.flush)
    property var windowSignals: Connections {
        target: root.window
        function onVisibleChanged() {
            Qt.callLater(BlurRegions.flush);
        }
        function onScreenChanged() {
            Qt.callLater(BlurRegions.flush);
        }
    }
    Component.onCompleted: BlurRegions.add(root)
    Component.onDestruction: BlurRegions.remove(root)
}
