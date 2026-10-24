import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

// The tablet-pad help card, drawn on the one output the request names. The
// card takes the keyboard while it is open so Escape can dismiss it. Pointer
// input outside the card passes through to the desktop.
QtObject {
    id: root

    required property QtObject state
    property var padHelpWindows
    readonly property int cardWidth: 520
    readonly property int maxBodyHeight: 480

    function labelText(label) {
        return label === undefined || label === "" ? "No label reported" : label;
    }

    function hasLabel(label) {
        return label !== undefined && label !== "";
    }

    function buttonList(buttons) {
        return buttons.length === 0 ? "none" : buttons.map(button => String(button)).join(", ");
    }

    function featureTitle(feature) {
        const names = ({
                "ring": "Ring",
                "dial": "Dial",
                "strip": "Strip"
            });
        return (names[feature.type] || "Feature") + " " + feature.index;
    }

    function deviceSummary(record) {
        const parts = [];
        if (record.device.deviceType.length > 0)
            parts.push(record.device.deviceType);
        if (record.device.id.length > 0)
            parts.push(record.device.id);
        return parts.join(" · ");
    }

    padHelpWindows: Variants {
        model: Quickshell.screens

        PanelWindow {
            id: padHelpWindow

            property var modelData
            // Only the output named by the request shows the card, so a
            // request never draws on two screens.
            readonly property bool isTarget: root.state.targetOutputName === modelData.name
            readonly property var request: isTarget ? root.state.request : null
            // Keep the content on screen until the close fade has finished.
            property var presentedRequest: null

            onRequestChanged: {
                if (request) {
                    presentedRequest = request;
                    focusCard.restart();
                }
            }

            screen: modelData
            color: "transparent"
            // The explicit layershell keyboard mode below owns focus; the
            // focusable setter would replace Exclusive with OnDemand.
            visible: request !== null || padCard.opacity > 0
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-pad-help"
            WlrLayershell.keyboardFocus: request !== null ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            Timer {
                id: focusCard
                interval: 0
                onTriggered: if (padHelpWindow.request !== null)
                    keyTarget.forceActiveFocus()
            }

            Rectangle {
                id: padCard
                objectName: "padHelpCard"

                width: Math.min(root.cardWidth, padHelpWindow.width - Theme.osdPadding * 2)
                height: body.implicitHeight + Theme.paddingLarge * 2
                anchors.centerIn: parent
                radius: Theme.shellRadius
                color: Theme.popupSurface
                border.width: 1
                border.color: Theme.outline
                PanelOutline {
                    surface: padCard
                }
                opacity: padHelpWindow.request !== null ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: Theme.reducedMotion ? 0 : (padHelpWindow.request !== null ? Theme.popupOpenMotion : Theme.osdCloseMotion)
                        easing.type: Easing.OutCubic
                    }
                }
                Accessible.role: Accessible.Dialog
                Accessible.name: padHelpWindow.presentedRequest ? "Pad help for " + padHelpWindow.presentedRequest.device.name : ""

                Item {
                    id: keyTarget
                    objectName: "padHelpKeys"
                    anchors.fill: parent
                    focus: padHelpWindow.request !== null
                    Keys.onEscapePressed: event => {
                        if (!event.isAutoRepeat)
                            root.state.dismiss();
                        event.accepted = true;
                    }
                }

                ColumnLayout {
                    id: body
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: Theme.paddingLarge
                    spacing: Theme.padding

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceSmall

                        Text {
                            objectName: "padHelpDevice"
                            Layout.fillWidth: true
                            color: Theme.text
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontHeading
                            font.weight: Font.DemiBold
                            text: padHelpWindow.presentedRequest ? padHelpWindow.presentedRequest.device.name : ""
                            textFormat: Text.PlainText
                        }
                        Text {
                            objectName: "padHelpSummary"
                            Layout.fillWidth: true
                            color: Theme.muted
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            text: padHelpWindow.presentedRequest ? root.deviceSummary(padHelpWindow.presentedRequest) + (padHelpWindow.presentedRequest.editionMode ? " · Edition mode" : "") : ""
                            textFormat: Text.PlainText
                        }
                    }

                    Flickable {
                        id: scroll
                        objectName: "padHelpBody"
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(sections.implicitHeight, root.maxBodyHeight)
                        clip: true
                        contentWidth: width
                        contentHeight: sections.implicitHeight
                        boundsBehavior: Flickable.StopAtBounds

                        ColumnLayout {
                            id: sections
                            width: scroll.width
                            spacing: Theme.paddingLarge

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSmall
                                visible: padHelpWindow.presentedRequest && padHelpWindow.presentedRequest.buttons.length > 0

                                Text {
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    font.weight: Font.DemiBold
                                    text: "Buttons"
                                    textFormat: Text.PlainText
                                }
                                Repeater {
                                    model: padHelpWindow.presentedRequest ? padHelpWindow.presentedRequest.buttons : []
                                    delegate: RowLayout {
                                        id: buttonRow
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: Theme.padding

                                        Text {
                                            Layout.preferredWidth: 88
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            font.features: ({
                                                    "tnum": 1
                                                })
                                            text: "Button " + buttonRow.modelData.button
                                            textFormat: Text.PlainText
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            color: root.hasLabel(buttonRow.modelData.label) ? Theme.text : Theme.muted
                                            elide: Text.ElideRight
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            font.italic: !root.hasLabel(buttonRow.modelData.label)
                                            text: root.labelText(buttonRow.modelData.label)
                                            textFormat: Text.PlainText
                                        }
                                        Text {
                                            visible: buttonRow.modelData.modeGroup !== undefined
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            text: "Mode group " + buttonRow.modelData.modeGroup
                                            textFormat: Text.PlainText
                                        }
                                    }
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSmall
                                visible: padHelpWindow.presentedRequest && padHelpWindow.presentedRequest.modeGroups.length > 0

                                Text {
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    font.weight: Font.DemiBold
                                    text: "Modes"
                                    textFormat: Text.PlainText
                                }
                                Repeater {
                                    model: padHelpWindow.presentedRequest ? padHelpWindow.presentedRequest.modeGroups : []
                                    delegate: ColumnLayout {
                                        id: modeRow
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: 2

                                        Text {
                                            Layout.fillWidth: true
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            text: "Mode group " + modeRow.modelData.group + ": " + modeRow.modelData.modeCount + " modes"
                                            textFormat: Text.PlainText
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            text: "Switch buttons: " + root.buttonList(modeRow.modelData.switchButtons)
                                            textFormat: Text.PlainText
                                        }
                                    }
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSmall
                                visible: padHelpWindow.presentedRequest && padHelpWindow.presentedRequest.features.length > 0

                                Text {
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    font.weight: Font.DemiBold
                                    text: "Touch controls"
                                    textFormat: Text.PlainText
                                }
                                Repeater {
                                    model: padHelpWindow.presentedRequest ? padHelpWindow.presentedRequest.features : []
                                    delegate: ColumnLayout {
                                        id: featureRow
                                        required property var modelData
                                        readonly property bool isStrip: modelData.type === "strip"
                                        Layout.fillWidth: true
                                        spacing: 2

                                        Text {
                                            Layout.fillWidth: true
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            font.weight: Font.DemiBold
                                            text: root.featureTitle(featureRow.modelData)
                                            textFormat: Text.PlainText
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            font.italic: !root.hasLabel(featureRow.modelData.positiveLabel)
                                            text: (featureRow.isStrip ? "Up: " : "Clockwise: ") + root.labelText(featureRow.modelData.positiveLabel)
                                            textFormat: Text.PlainText
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            font.italic: !root.hasLabel(featureRow.modelData.negativeLabel)
                                            text: (featureRow.isStrip ? "Down: " : "Counterclockwise: ") + root.labelText(featureRow.modelData.negativeLabel)
                                            textFormat: Text.PlainText
                                        }
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true

                        Item {
                            Layout.fillWidth: true
                        }
                        Rectangle {
                            id: closeButton
                            objectName: "padHelpClose"

                            implicitWidth: closeLabel.implicitWidth + Theme.padding * 2
                            implicitHeight: closeLabel.implicitHeight + Theme.spaceSmall * 2
                            radius: height / 2
                            color: closeArea.containsMouse ? Theme.elevated : "transparent"
                            border.width: 1
                            border.color: Theme.outline
                            Accessible.role: Accessible.Button
                            Accessible.name: "Close pad help"

                            Text {
                                id: closeLabel
                                anchors.centerIn: parent
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                text: "Close"
                                textFormat: Text.PlainText
                            }
                            MouseArea {
                                id: closeArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.state.dismiss()
                            }
                        }
                    }
                }
            }

            mask: Region {
                item: padCard
            }
        }
    }
}
