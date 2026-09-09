import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    objectName: "launchErrorDialog"
    property var pending: []
    property string applicationId: ""
    property string applicationName: ""
    property string message: ""
    signal retryRequested(string id)
    function show(id, name, detail) {
        if (visible && applicationId !== id) {
            pending = pending.filter(item => item.id !== id).concat([{id: id, name: name, detail: detail}]);
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
        if (visible && applicationId === id) dismissCurrent();
    }
    visible: false
    implicitWidth: 440
    implicitHeight: contents.implicitHeight + 40
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-launch-error"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    Rectangle {
        anchors.fill: parent
        radius: Theme.cardRadius
        color: Theme.popupSurface
        ColumnLayout {
            id: contents
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
            spacing: 12
            Keys.onEscapePressed: root.dismissCurrent()
            Text {
                Layout.fillWidth: true
                text: "Could not open " + root.applicationName
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.text
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize + 3; weight: Font.DemiBold }
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Text {
                Layout.fillWidth: true
                text: root.message.slice(0, 1000)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.text
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                ActionButton {
                    text: "Retry"
                    onClicked: { const id = root.applicationId; root.dismissCurrent(); root.retryRequested(id); }
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
