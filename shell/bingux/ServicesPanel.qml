import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell.Io
import QtQuick.Window

ColumnLayout {
    id: root
    property string scope: "User"
    property var selection: []
    property var services: []
    property string error: ""
    spacing: 12
    function refresh() {
        if (!visible || worker.running) return;
        worker.requestedScope = scope;
        worker.command = ["systemctl", scope === "User" ? "--user" : "--system", "list-units", "--type=service", "--all", "--output=json", "--no-pager"];
        worker.running = true;
    }
    onVisibleChanged: { if (visible) refresh(); else menu.visible = false; }
    onScopeChanged: { services = []; selection = []; menu.visible = false; error = ""; refresh(); }
    Component.onCompleted: refresh()
    Timer { interval: 10000; repeat: true; running: root.visible; onTriggered: root.refresh() }
    Process {
        id: worker
        property string requestedScope: ""
        stdout: StdioCollector {
            onStreamFinished: {
                if (worker.requestedScope !== root.scope) return;
                try {
                    const data = JSON.parse(text);
                    if (!Array.isArray(data)) throw new Error("Invalid service list");
                    root.services = data.filter(service => typeof service.unit === "string" && typeof service.description === "string" && typeof service.sub === "string");
                    root.error = "";
                } catch (_) { root.error = "Could not read services"; }
            }
        }
        stderr: StdioCollector { onStreamFinished: if (text.trim() && worker.requestedScope === root.scope) root.error = text.trim() }
        onExited: if (requestedScope !== root.scope) root.refresh()
    }
    RowLayout {
        Layout.fillWidth: true
        Text { Layout.fillWidth: true; text: "Services"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold }
        SegmentedControl { options: ["User", "System"]; currentValue: root.scope; implicitWidth: 116; implicitHeight: 30; accessiblePrefix: "Services scope "; onSelected: value => root.scope = value }
    }
    ProcessTable {
        objectName: "servicesTable"
        Layout.fillWidth: true; Layout.fillHeight: true
        Layout.minimumHeight: 160
        mode: "services"
        sortKey: "Name"
        descending: false
        records: root.visible ? root.services.map(service => ({key: service.unit, name: service.unit, description: service.description, state: service.sub})) : []
        totalCount: root.services.length
        applicationFor: () => null
        formatBytes: () => ""
        onContextRequested: (row, selected) => {
            root.selection = selected.map(service => service.name);
            const point = row.mapToItem(menu.contentItem, 0, row.height);
            menu.preferredX = point.x; menu.preferredY = point.y;
            menu.visible = true;
            Qt.callLater(() => startButton.forceActiveFocus());
        }
    }
    Text { visible: root.error.length > 0; Layout.fillWidth: true; wrapMode: Text.Wrap; text: root.error; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
    function act(action) {
        if (actionWorker.running || !selection.length) return;
        menu.visible = false;
        error = "";
        actionWorker.command = ["systemctl", scope === "User" ? "--user" : "--system", "--no-ask-password", action, "--"].concat(selection);
        actionWorker.running = true;
    }
    Process {
        id: actionWorker
        stderr: StdioCollector { onStreamFinished: if (text.trim()) root.error = text.trim() }
        onExited: (code, status) => { if (code === 0) root.error = "Service action completed"; root.refresh(); }
    }
    ShellPopup {
        id: menu
        hostItem: root.Window.window ? root.Window.window.contentItem : null
        popupWidth: 210
        contentPadding: 6
        ColumnLayout {
            id: actions
            width: parent.width
            spacing: 2
            ActionButton { id: startButton; Layout.fillWidth: true; flat: true; alignLeft: true; text: "Start"; iconName: "media-playback-start-symbolic"; enabled: !actionWorker.running; onClicked: root.act("start") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: "Stop"; iconName: "media-playback-stop-symbolic"; enabled: !actionWorker.running; onClicked: root.act("stop") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: "Restart"; iconName: "view-refresh-symbolic"; enabled: !actionWorker.running; onClicked: root.act("restart") }
            Keys.onPressed: event => {
                if (![Qt.Key_Up, Qt.Key_Down].includes(event.key)) return;
                const buttons = children.filter(child => typeof child.clicked === "function" && child.enabled);
                if (!buttons.length) return;
                const index = buttons.findIndex(child => child.activeFocus);
                buttons[(index + (event.key === Qt.Key_Down ? 1 : buttons.length - 1)) % buttons.length].forceActiveFocus();
                event.accepted = true;
            }
        }
    }
}
