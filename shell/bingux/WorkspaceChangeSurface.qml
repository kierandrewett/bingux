import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

// One short label naming the active workspace after a completed change. It is
// click-through and never takes keyboard focus, so it cannot alter selection.
QtObject {
    id: root

    required property QtObject state
    property real dockSafeInset: Theme.dockExclusiveHeight
    property var workspaceWindows

    workspaceWindows: Variants {
        model: Quickshell.screens

        PanelWindow {
            id: changeWindow

            property var modelData
            readonly property bool active: root.state.shown && root.state.outputName === modelData.name

            screen: modelData
            color: "transparent"
            focusable: false
            visible: active || changeCard.opacity > 0
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-workspace"
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
                id: changeCard
                objectName: "workspaceChangeCard"

                width: Math.min(Theme.osdWidth, changeWindow.width - Theme.osdPadding * 2)
                height: content.implicitHeight + Theme.osdPadding * 2
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: root.dockSafeInset + Theme.gap * 2
                radius: Theme.shellRadius
                color: Theme.popupSurface
                border.width: 1
                border.color: Theme.outline
                PanelOutline {
                    surface: changeCard
                }
                opacity: changeWindow.active ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: Theme.reducedMotion ? 0 : (changeWindow.active ? Theme.popupOpenMotion : Theme.osdCloseMotion)
                        easing.type: Easing.OutCubic
                    }
                }
                Accessible.role: Accessible.StaticText
                Accessible.name: root.state.workspace ? "Workspace " + root.state.workspace.number + ": " + root.state.workspace.name : ""

                ColumnLayout {
                    id: content
                    anchors.fill: parent
                    anchors.margins: Theme.osdPadding
                    spacing: Theme.spaceSmall

                    Text {
                        objectName: "workspaceChangeNumber"
                        Layout.fillWidth: true
                        color: Theme.muted
                        elide: Text.ElideRight
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        font.features: ({
                                "tnum": 1
                            })
                        text: root.state.workspace ? "Workspace " + root.state.workspace.number : ""
                        textFormat: Text.PlainText
                    }
                    Text {
                        objectName: "workspaceChangeName"
                        Layout.fillWidth: true
                        color: Theme.text
                        elide: Text.ElideRight
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontHeading
                        font.weight: Font.DemiBold
                        text: root.state.workspace ? root.state.workspace.name : ""
                        textFormat: Text.PlainText
                    }
                }
            }

            mask: Region {
                item: clickThroughTarget
            }
        }
    }
}
