import QtQuick
import QtQuick.Layouts
import ".."
import "../auth"

// Suspend, restart and shut down, as a frosted panel above the bottom-right
// corner. Restart and shut down ask for confirmation first. The panel only
// reports `requested(action)`; bsm decides whether it is allowed and answers
// with operation events that the screen shows.
Item {
    id: root

    property AuthTheme theme: AuthTheme {}
    property Item backdrop: null
    property bool open: false
    property string pending: ""
    readonly property var actions: [
        {
            id: "suspend",
            title: "Suspend",
            icon: "weather-clear-night-symbolic",
            confirm: false
        },
        {
            id: "reboot",
            title: "Restart",
            icon: "system-reboot-symbolic",
            confirm: true
        },
        {
            id: "poweroff",
            title: "Shut Down",
            icon: "system-shutdown-symbolic",
            confirm: true
        }
    ]
    readonly property var selected: actions.find(action => action.id === pending)

    signal requested(string action)
    signal dismissed

    visible: open
    onOpenChanged: {
        if (!open)
            pending = "";
        else
            Qt.callLater(root.focusFirst);
    }
    onPendingChanged: Qt.callLater(root.focusFirst)

    // Keyboard users land on the first choice, and on the confirm button
    // after choosing restart or shut down.
    function focusFirst() {
        if (!open)
            return;
        const item = pending === "" ? rows.itemAt(0) : confirmRow;
        if (item)
            item.forceActiveFocus();
    }

    MouseArea {
        anchors.fill: parent
        enabled: root.open
        onClicked: root.dismissed()
    }

    Item {
        id: panel
        objectName: "powerMenu"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: Theme.paddingLarge
        anchors.bottomMargin: Theme.paddingLarge + 32 + Theme.gap
        width: 232
        height: content.implicitHeight + Theme.padding * 2
        opacity: root.open ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.theme.motion
            }
        }
        Keys.onEscapePressed: root.dismissed()

        SessionGlass {
            anchors.fill: parent
            backdrop: root.backdrop
            radius: Theme.radius + 4
            tint: Theme.softwareRendering ? Qt.rgba(0.12, 0.12, 0.14, 0.96) : Qt.rgba(0.1, 0.1, 0.12, 0.45)
            outline: root.theme.glassOutline
        }
        ColumnLayout {
            id: content
            anchors.fill: parent
            anchors.margins: Theme.padding
            spacing: 4
            Repeater {
                id: rows
                model: root.pending === "" ? root.actions : []
                SessionRow {
                    required property var modelData
                    objectName: "power-" + modelData.id
                    Layout.fillWidth: true
                    text: modelData.title
                    iconName: modelData.icon
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                    Keys.onDownPressed: nextItemInFocusChain(true).forceActiveFocus()
                    Keys.onUpPressed: nextItemInFocusChain(false).forceActiveFocus()
                    onClicked: {
                        if (modelData.confirm)
                            root.pending = modelData.id;
                        else
                            root.requested(modelData.id);
                    }
                }
            }
            Text {
                textFormat: Text.PlainText
                Layout.fillWidth: true
                visible: root.pending !== ""
                text: root.selected ? root.selected.title + "?" : ""
                color: "#ffffff"
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.Medium
            }
            Text {
                textFormat: Text.PlainText
                Layout.fillWidth: true
                visible: root.pending !== ""
                text: "Everyone who is signed in will be signed out."
                wrapMode: Text.Wrap
                color: Qt.rgba(1, 1, 1, 0.72)
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            SessionRow {
                id: confirmRow
                objectName: "power-confirm"
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                Keys.onDownPressed: nextItemInFocusChain(true).forceActiveFocus()
                Layout.fillWidth: true
                visible: root.pending !== ""
                text: root.selected ? root.selected.title : ""
                iconName: root.selected ? root.selected.icon : ""
                onClicked: root.requested(root.pending)
            }
            SessionRow {
                objectName: "power-cancel"
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                Keys.onUpPressed: nextItemInFocusChain(false).forceActiveFocus()
                Layout.fillWidth: true
                visible: root.pending !== ""
                text: "Cancel"
                iconName: "go-previous-symbolic"
                onClicked: root.pending = ""
            }
        }
    }
}
