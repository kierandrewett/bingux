import QtQuick
import QtQuick.Layouts
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
                radius: Theme.cardRadius
                color: Theme.surface
                border.width: 1
                border.color: Theme.outline

                RowLayout {
                    id: heading

                    anchors.top: parent.top
                    anchors.topMargin: Theme.paddingLarge
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.paddingLarge
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.paddingLarge
                    spacing: Theme.padding

                    SymbolicIcon {
                        Layout.alignment: Qt.AlignVCenter
                        implicitSize: 24
                        source: Quickshell.iconPath(osdWindow.request ? osdWindow.request.icon : "dialog-information-symbolic", "dialog-information-symbolic")
                    }

                    Text {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        color: Theme.text
                        elide: Text.ElideRight
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                        font.weight: Font.DemiBold
                        text: osdWindow.request ? root.labelFor(osdWindow.request) : ""
                        textFormat: Text.PlainText
                    }

                    Text {
                        id: percentLabel

                        Layout.alignment: Qt.AlignVCenter
                        color: Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
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
                    anchors.leftMargin: Theme.paddingLarge
                    anchors.rightMargin: Theme.paddingLarge
                    anchors.bottomMargin: 20
                    radius: height / 2
                    color: Theme.elevated

                    Rectangle {
                        width: parent.width * osdWindow.levelFraction
                        height: parent.height
                        radius: parent.radius
                        color: Theme.accent
                    }

                }

            }

            mask: Region {
                item: clickThroughTarget
            }

        }

    }

}
