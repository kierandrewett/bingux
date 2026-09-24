import QtQuick
import QtQuick.Controls

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

    Rectangle {
        anchors.fill: parent
        color: "#111318"
    }

    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 400)
        spacing: 16

        Text {
            width: parent.width
            text: Qt.formatDateTime(root.now, "dddd, d MMMM")
            color: "#c7cbd5"
            font.family: "Adwaita Sans"
            font.pixelSize: 18
            horizontalAlignment: Text.AlignHCenter
        }
        Text {
            width: parent.width
            text: Qt.formatTime(root.now, "HH:mm")
            color: "#ffffff"
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
            color: root.authentication.statusIsError ? "#ff7b63" : "#c7cbd5"
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
            onAccepted: root.submitResponse()
        }
        Button {
            width: parent.width
            text: authentication.active ? "Continue" : "Unlock"
            enabled: !authentication.unlockRequested && (!authentication.active || authentication.responseRequired)
            onClicked: authentication.active ? root.submitResponse() : authentication.start()
        }
    }

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: root.now = new Date()
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
