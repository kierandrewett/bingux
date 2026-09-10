pragma Singleton
import QtQuick

QtObject {
    readonly property var component: Qt.createComponent("SurfaceFadeNative.qml")
}
