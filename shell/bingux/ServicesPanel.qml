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
    // Two kinds of failure. The list could not be read: a message replaces the
    // table (Try again reloads it). An action on a service failed: a banner
    // above the list that stays until the next action, because the reload that
    // follows must not hide it.
    // `status` is the short confirmation after an action succeeded; an error
    // and a status never share a line.
    property bool readFailed: false
    property string actionError: ""
    property string actionDetail: ""
    property string status: ""
    spacing: 12
    function refresh() {
        if (!visible || worker.running)
            return;
        worker.requestedScope = scope;
        worker.command = ["systemctl", scope === "User" ? "--user" : "--system", "list-units", "--type=service", "--all", "--output=json", "--no-pager"];
        worker.running = true;
    }
    onVisibleChanged: {
        if (visible)
            refresh();
        else
            menu.visible = false;
    }
    onScopeChanged: {
        services = [];
        selection = [];
        menu.visible = false;
        readFailed = false;
        actionError = "";
        actionDetail = "";
        status = "";
        refresh();
    }
    Component.onCompleted: refresh()
    Timer {
        id: statusTimer
        interval: 4000
        onTriggered: root.status = ""
    }
    Timer {
        interval: 10000
        repeat: true
        running: root.visible
        onTriggered: root.refresh()
    }
    Process {
        id: worker
        property string requestedScope: ""
        stdout: StdioCollector {
            onStreamFinished: {
                if (worker.requestedScope !== root.scope)
                    return;
                try {
                    const data = JSON.parse(text);
                    if (!Array.isArray(data))
                        throw new Error("Invalid service list");
                    root.services = data.filter(service => typeof service.unit === "string" && typeof service.description === "string" && typeof service.sub === "string");
                    root.readFailed = false;
                } catch (_) {
                    root.readFailed = true;
                }
            }
        }
        stderr: StdioCollector {
            onStreamFinished: if (text.trim() && worker.requestedScope === root.scope) {
                console.warn("services: systemctl said: " + text.trim());
                root.readFailed = true;
            }
        }
        onExited: if (requestedScope !== root.scope)
            root.refresh()
    }
    RowLayout {
        Layout.fillWidth: true
        Text {
            Layout.fillWidth: true
            text: "Services"
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
        }
        SegmentedControl {
            options: ["User", "System"]
            currentValue: root.scope
            implicitWidth: 116
            implicitHeight: 30
            accessiblePrefix: "Services scope "
            onSelected: value => root.scope = value
        }
    }
    // A list that could not be read is not an empty list: the message takes
    // the table's place, so there is no empty table and no 0 / 0 count.
    StateMessage {
        objectName: "servicesReadError"
        fill: true
        visible: root.readFailed
        tone: "danger"
        title: "Could not read services"
        detail: "Check that the " + (root.scope === "User" ? "user" : "system") + " service manager is running."
        actionText: "Try again"
        onActionTriggered: root.refresh()
    }
    StateMessage {
        objectName: "servicesError"
        compact: true
        visible: root.actionError !== ""
        tone: "danger"
        title: root.actionError
        detail: root.actionDetail
    }
    StateMessage {
        objectName: "servicesStatus"
        compact: true
        visible: root.status !== "" && root.actionError === "" && !root.readFailed
        tone: "success"
        title: root.status
    }
    ProcessTable {
        objectName: "servicesTable"
        visible: !root.readFailed
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: 160
        mode: "services"
        sortKey: "Name"
        descending: false
        records: root.visible ? root.services.map(service => ({
                    key: service.unit,
                    name: service.unit,
                    description: service.description,
                    state: service.sub
                })) : []
        totalCount: root.services.length
        applicationFor: () => null
        formatBytes: () => ""
        onContextRequested: (row, selected) => {
            root.selection = selected.map(service => service.name);
            const point = row.mapToItem(menu.contentItem, 0, row.height);
            menu.preferredX = point.x;
            menu.preferredY = point.y;
            menu.visible = true;
            Qt.callLater(() => startButton.forceActiveFocus());
        }
    }
    function act(action) {
        if (actionWorker.running || !selection.length)
            return;
        menu.visible = false;
        actionError = "";
        actionDetail = "";
        status = "";
        actionWorker.action = action;
        actionWorker.targets = selection.slice();
        actionWorker.command = ["systemctl", scope === "User" ? "--user" : "--system", "--no-ask-password", action, "--"].concat(selection);
        actionWorker.running = true;
    }
    Process {
        id: actionWorker
        property string action: ""
        property var targets: []
        readonly property string subject: targets.length === 1 ? targets[0] : targets.length + " services"
        stderr: StdioCollector {
            onStreamFinished: if (text.trim())
                console.warn("services: systemctl said: " + text.trim())
        }
        onExited: (code, exitStatus) => {
            if (code === 0) {
                root.status = ({
                        start: "Started ",
                        stop: "Stopped ",
                        restart: "Restarted "
                    })[action] + subject;
                statusTimer.restart();
            } else {
                root.actionError = "Could not " + action + " " + subject + ".";
                root.actionDetail = root.scope === "System" ? "Changing system services needs administrator permission." : "Check the service and try again.";
            }
            root.refresh();
        }
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
            ActionButton {
                id: startButton
                Layout.fillWidth: true
                flat: true
                alignLeft: true
                text: "Start"
                iconName: "media-playback-start-symbolic"
                enabled: !actionWorker.running
                onClicked: root.act("start")
            }
            ActionButton {
                Layout.fillWidth: true
                flat: true
                alignLeft: true
                text: "Stop"
                iconName: "media-playback-stop-symbolic"
                enabled: !actionWorker.running
                onClicked: root.act("stop")
            }
            ActionButton {
                Layout.fillWidth: true
                flat: true
                alignLeft: true
                text: "Restart"
                iconName: "view-refresh-symbolic"
                enabled: !actionWorker.running
                onClicked: root.act("restart")
            }
            Keys.onPressed: event => {
                if (![Qt.Key_Up, Qt.Key_Down].includes(event.key))
                    return;
                const buttons = children.filter(child => typeof child.clicked === "function" && child.enabled);
                if (!buttons.length)
                    return;
                const index = buttons.findIndex(child => child.activeFocus);
                buttons[(index + (event.key === Qt.Key_Down ? 1 : buttons.length - 1)) % buttons.length].forceActiveFocus();
                event.accepted = true;
            }
        }
    }
}
