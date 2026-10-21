import QtQuick

// One page of a KioskPageStack. It is visible while the stack names its
// pageId and fades out when another page takes over. Under Theme.reducedMotion
// the change is immediate.
Item {
    id: root

    required property var stack
    required property string pageId
    readonly property bool active: stack.current === pageId

    anchors.fill: parent
    opacity: active ? 1 : 0
    visible: opacity > 0.001
    enabled: active
    transform: Translate {
        x: root.active ? 0 : (root.stack.forward ? 16 : -16)
        Behavior on x {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: Theme.motion
                easing.type: Easing.OutCubic
            }
        }
    }
    Behavior on opacity {
        enabled: !Theme.reducedMotion
        NumberAnimation {
            duration: Theme.motion
        }
    }
}
