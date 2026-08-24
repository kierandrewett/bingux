import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

QtObject {
    id: root

    required property QtObject state
    property var osdWindows

    function labelFor(request) {
        if (request.label.length > 0)
            return request.label;

        if (request.icon.indexOf("audio-") === 0)
            return "Volume";

        if (request.icon.indexOf("display-brightness") === 0)
            return "Brightness";

        if (request.icon.indexOf("keyboard-brightness") === 0)
            return "Keyboard brightness";

        if (request.icon.indexOf("microphone-") === 0)
            return "Microphone";

        return "System control";
    }

    osdWindows: Variants {
        model: Quickshell.screens

        PanelWindow {
            id: osdWindow

            property var modelData
            readonly property var request: root.state.requestForOutputName(modelData.name)
            readonly property bool hasLevel: request !== null && request.maxLevel > 0 && request.level >= 0
            readonly property real levelFraction: hasLevel ? Math.min(1, request.level / request.maxLevel) : 0
            readonly property string percentLabel: hasLevel ? Math.round(levelFraction * 100) + "%" : ""

            screen: modelData
            color: "transparent"
            focusable: false
            visible: request !== null
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-osd"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            Item {
                id: clickThroughTarget

                width: 0
                height: 0
            }

            Rectangle {
                id: osdCard

                width: Math.min(336, parent.width - 32)
                height: osdWindow.hasLevel ? 112 : 76
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Math.max(96, parent.height / 6)
                radius: 12
                color: "#202632"
                border.width: 1
                border.color: "#3d485e"

                Row {
                    id: heading

                    anchors.top: parent.top
                    anchors.topMargin: 18
                    anchors.left: parent.left
                    anchors.leftMargin: 20
                    anchors.right: parent.right
                    anchors.rightMargin: 20
                    spacing: 14

                    IconImage {
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: 28
                        source: Quickshell.iconPath(osdWindow.request ? osdWindow.request.icon : "dialog-information-symbolic", "dialog-information-symbolic")
                    }

                    Text {
                        width: parent.width - 28 - parent.spacing - (percentLabel.visible ? percentLabel.width + parent.spacing : 0)
                        anchors.verticalCenter: parent.verticalCenter
                        color: "#e7edf7"
                        elide: Text.ElideRight
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                        text: osdWindow.request ? root.labelFor(osdWindow.request) : ""
                        textFormat: Text.PlainText
                    }

                    Text {
                        id: percentLabel

                        anchors.verticalCenter: parent.verticalCenter
                        color: "#c6d0df"
                        font.pixelSize: 14
                        text: osdWindow.percentLabel
                        visible: osdWindow.hasLevel
                    }

                }

                Rectangle {
                    visible: osdWindow.hasLevel
                    height: 6
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 20
                    anchors.rightMargin: 20
                    anchors.bottomMargin: 20
                    radius: height / 2
                    color: "#3a4559"

                    Rectangle {
                        width: parent.width * osdWindow.levelFraction
                        height: parent.height
                        radius: parent.radius
                        color: "#8ab4f8"
                    }

                }

            }

            mask: Region {
                item: clickThroughTarget
            }

        }

    }

}
