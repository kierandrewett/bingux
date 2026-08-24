import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

PanelWindow {
    id: root

    required property var state

    function hasDefaultAction(entry) {
        for (let index = 0; index < entry.actions.length; index += 1) {
            if (entry.actions[index].defaultAction)
                return true;

        }
        return false;
    }

    function invokeDefaultAction(entry) {
        for (let index = 0; index < entry.actions.length; index += 1) {
            const action = entry.actions[index];
            if (action.defaultAction) {
                action.action.invoke();
                return ;
            }
        }
    }

    function hasSecondaryAction(entry) {
        for (let index = 0; index < entry.actions.length; index += 1) {
            if (!entry.actions[index].defaultAction)
                return true;

        }
        return false;
    }

    color: "transparent"
    focusable: false
    visible: state.visibleEntries.length > 0
    exclusionMode: ExclusionMode.Ignore
    surfaceFormat.opaque: false
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-notifications"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }

    Column {
        id: notificationColumn

        width: Math.min(384, parent.width - 24)
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: 48
        anchors.rightMargin: 12
        spacing: 8

        Repeater {
            model: root.state.visibleEntries

            delegate: Rectangle {
                id: notificationCard

                required property var modelData
                readonly property var entry: modelData
                readonly property var notification: entry.notification
                readonly property bool defaultActionAvailable: root.hasDefaultAction(entry)

                width: notificationColumn.width
                height: cardContents.implicitHeight + 28
                radius: 12
                color: "#202632"
                border.width: 1
                border.color: "#3d485e"
                Accessible.name: entry.appName + ": " + entry.summary
                Accessible.role: defaultActionAvailable ? Accessible.Button : Accessible.StaticText
                Accessible.focusable: defaultActionAvailable
                Accessible.onPressAction: root.invokeDefaultAction(entry)

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: notificationCard.defaultActionAvailable
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: root.invokeDefaultAction(notificationCard.entry)
                }

                Column {
                    id: cardContents

                    width: parent.width - 32
                    anchors.top: parent.top
                    anchors.topMargin: 14
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 8

                    Item {
                        width: parent.width
                        height: 28

                        IconImage {
                            id: applicationIcon

                            width: 24
                            height: 24
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            source: Quickshell.iconPath(notificationCard.entry.appIcon, "dialog-information-symbolic")
                        }

                        Text {
                            anchors.left: applicationIcon.right
                            anchors.leftMargin: 10
                            anchors.right: closeButton.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            color: "#b9c5d6"
                            elide: Text.ElideRight
                            font.pixelSize: 12
                            text: notificationCard.entry.appName || notificationCard.entry.desktopEntry || "Notification"
                            textFormat: Text.PlainText
                        }

                        Item {
                            id: closeButton

                            width: 24
                            height: 24
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            Accessible.name: "Dismiss notification"
                            Accessible.role: Accessible.Button
                            Accessible.focusable: true
                            Accessible.onPressAction: root.state.dismiss(notificationCard.notification)

                            IconImage {
                                anchors.centerIn: parent
                                implicitSize: 16
                                source: Quickshell.iconPath("window-close-symbolic", "edit-clear-symbolic")
                            }

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.state.dismiss(notificationCard.notification)
                            }

                        }

                    }

                    Text {
                        width: parent.width
                        color: "#eef3fb"
                        elide: Text.ElideRight
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                        maximumLineCount: 2
                        text: notificationCard.entry.summary || "Notification"
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    Text {
                        width: parent.width
                        visible: notificationCard.entry.body.length > 0
                        color: "#c6d0df"
                        font.pixelSize: 13
                        lineHeight: 1.2
                        maximumLineCount: 3
                        text: notificationCard.entry.body
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    Flow {
                        width: parent.width
                        height: visible ? implicitHeight : 0
                        visible: root.hasSecondaryAction(notificationCard.entry)
                        spacing: 6

                        Repeater {
                            model: notificationCard.entry.actions

                            delegate: Rectangle {
                                id: actionButton

                                required property var modelData

                                width: visible ? Math.min(164, actionLabel.implicitWidth + 20) : 0
                                height: visible ? 30 : 0
                                radius: 6
                                color: actionMouse.containsMouse ? "#3b4b66" : "#303b50"
                                visible: !modelData.defaultAction
                                Accessible.name: modelData.text
                                Accessible.role: Accessible.Button
                                Accessible.focusable: true
                                Accessible.onPressAction: modelData.action.invoke()

                                Text {
                                    id: actionLabel

                                    anchors.centerIn: parent
                                    color: "#d9e7ff"
                                    elide: Text.ElideRight
                                    font.pixelSize: 12
                                    maximumLineCount: 1
                                    text: actionButton.modelData.text
                                    textFormat: Text.PlainText
                                }

                                MouseArea {
                                    id: actionMouse

                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: actionButton.modelData.action.invoke()
                                }

                            }

                        }

                    }

                }

            }

        }

    }

    mask: Region {
        item: notificationColumn
    }

}
