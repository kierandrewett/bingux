import Quickshell
import Quickshell.Wayland
import QtQuick

PanelWindow {
    id: root

    required property var state

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

    mask: Region {
        item: notificationColumn
    }

    function hasDefaultAction(notification) {
        for (let index = 0; index < notification.actions.length; index += 1) {
            if (notification.actions[index].identifier === "default") {
                return true;
            }
        }

        return false;
    }

    function invokeDefaultAction(notification) {
        for (let index = 0; index < notification.actions.length; index += 1) {
            const action = notification.actions[index];
            if (action.identifier === "default") {
                action.invoke();
                return;
            }
        }
    }

    function hasSecondaryAction(notification) {
        for (let index = 0; index < notification.actions.length; index += 1) {
            if (notification.actions[index].identifier !== "default") {
                return true;
            }
        }

        return false;
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
                readonly property var notification: modelData.notification
                readonly property bool defaultActionAvailable: root.hasDefaultAction(notification)

                width: notificationColumn.width
                height: cardContents.implicitHeight + 28
                radius: 12
                color: "#202632"
                border.width: 1
                border.color: "#3d485e"
                Accessible.name: notification.appName + ": " + notification.summary
                Accessible.role: defaultActionAvailable ? Accessible.Button : Accessible.StaticText

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: notificationCard.defaultActionAvailable
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: root.invokeDefaultAction(notificationCard.notification)
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
                            source: Quickshell.iconPath(
                                notificationCard.notification.appIcon,
                                "dialog-information-symbolic",
                            )
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
                            text: notificationCard.notification.appName || notificationCard.notification.desktopEntry || "Notification"
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
                        text: notificationCard.notification.summary || "Notification"
                        textFormat: Text.PlainText
                    }

                    Text {
                        width: parent.width
                        visible: notificationCard.notification.body.length > 0
                        color: "#c6d0df"
                        font.pixelSize: 13
                        lineHeight: 1.2
                        maximumLineCount: 3
                        text: notificationCard.notification.body
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    Flow {
                        width: parent.width
                        height: visible ? implicitHeight : 0
                        visible: root.hasSecondaryAction(notificationCard.notification)
                        spacing: 6

                        Repeater {
                            model: notificationCard.notification.actions

                            delegate: Rectangle {
                                id: actionButton

                                required property var modelData

                                width: visible ? Math.min(164, actionLabel.implicitWidth + 20) : 0
                                height: visible ? 30 : 0
                                radius: 6
                                color: actionMouse.containsMouse ? "#3b4b66" : "#303b50"
                                visible: modelData.identifier !== "default"
                                Accessible.name: modelData.text
                                Accessible.role: Accessible.Button

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
                                    onClicked: actionButton.modelData.invoke()
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
