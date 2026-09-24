import QtQuick
import QtQuick.Controls
import Quickshell

Item {
    id: root

    required property var authentication
    property date now: new Date()

    function submitResponse() {
        if (!authentication.active || !authentication.responseRequired)
            return;
        authentication.respond(response.text);
        response.text = "";
    }

    Image {
        anchors.fill: parent
        visible: LockTheme.backgroundImage !== ""
        source: LockTheme.backgroundImage
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    Rectangle {
        anchors.fill: parent
        color: LockTheme.background
        opacity: LockTheme.backgroundImage === "" ? 1 : 0.72
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: "#33000000"
            }
            GradientStop {
                position: 1
                color: "#99000000"
            }
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 400)
        height: content.implicitHeight + 56
        radius: 28
        color: LockTheme.surface
        border.color: "#26ffffff"
        border.width: 1

        Column {
            id: content
            anchors.centerIn: parent
            width: parent.width - 48
            spacing: 16

            Text {
                width: parent.width
                text: Qt.formatDateTime(root.now, "dddd, d MMMM")
                color: LockTheme.mutedText
                font.family: "Adwaita Sans"
                font.pixelSize: 18
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                width: parent.width
                text: Qt.formatTime(root.now, LockTheme.useTwelveHourClock ? "h:mm AP" : "HH:mm")
                color: LockTheme.text
                font.family: "Adwaita Sans"
                font.pixelSize: 72
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
            }
            Item {
                width: 1
                height: 24
            }
            Text {
                width: parent.width
                text: root.authentication.status
                color: root.authentication.statusIsError ? "#ff7b63" : LockTheme.mutedText
                font.family: "Adwaita Sans"
                font.pixelSize: 15
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
            }
            TextField {
                id: response
                width: parent.width
                enabled: authentication.active && authentication.responseRequired && !authentication.unlockRequested
                placeholderText: authentication.responseRequired ? (authentication.prompt || "Password") : "Password"
                echoMode: authentication.responseVisible ? TextInput.Normal : TextInput.Password
                font.family: "Adwaita Sans"
                font.pixelSize: 16
                selectByMouse: false
                background: Rectangle {
                    radius: 10
                    color: "#22000000"
                    border.width: 1
                    border.color: response.activeFocus ? LockTheme.accent : "#40ffffff"
                }
                onAccepted: root.submitResponse()
            }
            Button {
                width: parent.width
                text: authentication.active ? "Continue" : "Unlock"
                enabled: authentication.sessionLock.secure && !authentication.unlockRequested && (!authentication.active || authentication.responseRequired)
                onClicked: authentication.active ? root.submitResponse() : authentication.start()
                contentItem: Text {
                    text: parent.text
                    color: LockTheme.text
                    font: parent.font
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    radius: 10
                    color: parent.down ? Qt.darker(LockTheme.accent, 1.2) : LockTheme.accent
                    opacity: parent.enabled ? 1 : 0.45
                }
            }
        }
    }

    SystemClock {
        precision: SystemClock.Minutes
        onDateChanged: root.now = date
    }

    Component.onCompleted: response.forceActiveFocus()
    Connections {
        target: root.authentication
        function onResponseRequested() {
            response.text = "";
            response.forceActiveFocus();
        }
    }
}
