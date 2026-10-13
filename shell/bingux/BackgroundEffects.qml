pragma Singleton
import QtQuick

QtObject {
    readonly property bool nativeEnabled: !Theme.softwareRendering
    readonly property var component: nativeEnabled ? Qt.createComponent("BackgroundEffectNative.qml") : null
}
