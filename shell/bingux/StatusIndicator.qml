import QtQuick
import Quickshell

// Keep a stable slot while visibility changes, including a reversal mid-exit.
Item {
    id: root
    property bool shown: false
    property string iconName: ""
    property string label: ""
    property int slotSize: 24
    property string retainedIcon: ""
    property real reveal: shown ? 1 : 0
    property bool ready: false
    Component.onCompleted: { retainedIcon = iconName; ready = true; }
    onIconNameChanged: if (iconName) retainedIcon = iconName
    Behavior on reveal { enabled: root.ready; NumberAnimation { duration: Theme.statusIndicatorMotion; easing.type: Easing.OutCubic } }
    implicitWidth: (slotSize + Theme.spaceSmall) * reveal
    implicitHeight: slotSize
    visible: reveal > 0
    clip: true
    Accessible.role: Accessible.StaticText
    Accessible.name: label
    Accessible.ignored: !shown
    Item {
        width: root.slotSize
        height: root.slotSize
        opacity: root.reveal
        transform: Translate { x: Theme.spaceSmall * (1 - root.reveal) }
        SymbolicIcon {
            objectName: "statusIcon"
            anchors.centerIn: parent
            implicitSize: Theme.iconSize
            source: root.retainedIcon ? Quickshell.iconPath(root.retainedIcon) : ""
            color: Theme.text
        }
    }
}
