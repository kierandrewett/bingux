import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

// One large numbered label centred on each output named in an identify
// request. Labels are click-through and never take keyboard focus, so they
// cannot alter the desktop while they are shown.
QtObject {
    id: root

    required property QtObject state
    property var identifyWindows

    identifyWindows: Variants {
        model: Quickshell.screens

        PanelWindow {
            id: identifyWindow

            property var modelData
            readonly property int number: root.state.numberFor(modelData.name)
            readonly property bool active: number > 0

            screen: modelData
            color: "transparent"
            focusable: false
            visible: active || identifyCard.opacity > 0
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-identify"
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
                id: identifyCard
                objectName: "identifyCard"

                width: Math.min(content.implicitWidth + Theme.paddingLarge * 2, identifyWindow.width - Theme.osdPadding * 2)
                height: content.implicitHeight + Theme.paddingLarge * 2
                anchors.centerIn: parent
                radius: Theme.shellRadius
                color: Theme.popupSurface
                border.width: 1
                border.color: Theme.outline
                PanelOutline {
                    surface: identifyCard
                }
                opacity: identifyWindow.active ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: Theme.reducedMotion ? 0 : (identifyWindow.active ? Theme.popupOpenMotion : Theme.osdCloseMotion)
                        easing.type: Easing.OutCubic
                    }
                }
                Accessible.role: Accessible.StaticText
                Accessible.name: identifyWindow.active ? "Display " + identifyWindow.number + ": " + identifyWindow.modelData.name : ""

                ColumnLayout {
                    id: content
                    anchors.centerIn: parent
                    spacing: Theme.spaceSmall

                    Text {
                        objectName: "identifyNumber"
                        Layout.alignment: Qt.AlignHCenter
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontDisplay
                        font.weight: Font.DemiBold
                        font.features: ({
                                "tnum": 1
                            })
                        text: identifyWindow.active ? String(identifyWindow.number) : ""
                        textFormat: Text.PlainText
                    }
                    Text {
                        objectName: "identifyConnector"
                        Layout.alignment: Qt.AlignHCenter
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontHeading
                        text: identifyWindow.active ? identifyWindow.modelData.name : ""
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
