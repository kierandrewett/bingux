import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

ShellPopup {
    id: root
    objectName: "launchErrorDialog"
    property var pending: []
    property string applicationId: ""
    property string applicationName: ""
    property string message: ""
    signal retryRequested(string id)
    function show(id, name, detail) {
        if (visible && applicationId !== id) {
            pending = pending.filter(item => item.id !== id).concat([
                {
                    id: id,
                    name: name,
                    detail: detail
                }
            ]);
            return;
        }
        applicationId = id;
        applicationName = name;
        message = detail;
        visible = true;
        dismiss.forceActiveFocus();
    }
    function dismissCurrent() {
        visible = false;
        if (pending.length > 0) {
            const next = pending[0];
            pending = pending.slice(1);
            show(next.id, next.name, next.detail);
        }
    }
    function resolve(id) {
        pending = pending.filter(item => item.id !== id);
        if (visible && applicationId === id)
            dismissCurrent();
    }
    visible: false
    popupWidth: 440
    popupHeight: contents.implicitHeight + contentPadding * 2
    preferredY: (height - popupHeight) / 2
    dismissOnOutsideClick: false
    body.Keys.onEscapePressed: root.dismissCurrent()
    Item {
        anchors.fill: parent
        ColumnLayout {
            id: contents
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            spacing: 12
            Keys.onEscapePressed: root.dismissCurrent()
            Text {
                Layout.fillWidth: true
                text: "Could not open " + root.applicationName
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.text
                font {
                    family: Theme.fontFamily
                    pixelSize: Theme.fontSize + 3
                    weight: Font.DemiBold
                }
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Text {
                Layout.fillWidth: true
                text: root.message.slice(0, 1000)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.text
                font {
                    family: Theme.fontFamily
                    pixelSize: Theme.fontSize
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                ActionButton {
                    text: "Retry"
                    onClicked: {
                        const id = root.applicationId;
                        root.dismissCurrent();
                        root.retryRequested(id);
                    }
                }
                ActionButton {
                    id: dismiss
                    text: "Dismiss"
                    onClicked: root.dismissCurrent()
                }
            }
        }
    }
}
