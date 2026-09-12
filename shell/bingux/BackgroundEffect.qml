import QtQuick

// Optional native protocol support. Older compositors retain rule-based blur.
Loader {
    id: root
    required property Item target
    property real radius: 0
    property bool requested: true
    readonly property bool available: item?.available ?? false
    sourceComponent: BackgroundEffects.component.status === Component.Ready ? BackgroundEffects.component : null
    onLoaded: {
        item.target = Qt.binding(() => root.target);
        item.radius = Qt.binding(() => root.radius);
        item.enabled = Qt.binding(() => root.requested);
    }
}
