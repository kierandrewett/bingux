pragma Singleton
import QtQuick

QtObject {
    readonly property bool nativeEnabled: true
    readonly property var component: nativeEnabled ? Qt.createComponent("BackgroundEffectNative.qml") : null
}
