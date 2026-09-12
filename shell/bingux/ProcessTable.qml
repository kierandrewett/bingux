import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

ColumnLayout {
    id: root
    required property var records
    required property var applicationFor
    required property var formatBytes
    property string mode: "processes"
    readonly property int rowHeight: mode === "services" ? 50 : 34
    property int totalCount: 0
    property string sortKey: "CPU"
    property bool descending: true
    property string search: ""
    property string firstState: ""
    property var selectedKeys: ({})
    property string anchorKey: ""
    readonly property var selectedRecords: records.filter(process => selectedKeys[entryKey(process)])
    readonly property var list: table.listView
    property bool dragSelecting: false
    readonly property bool holdOrder: table.hovered || selectedRecords.length > 0 || list.moving || dragSelecting
    onHoldOrderChanged: if (!holdOrder)
        reorderDelay.restart()
    Timer {
        id: reorderDelay
        interval: 900
        onTriggered: if (!root.holdOrder)
            root.refresh()
    }
    signal contextRequested(Item row, var processes)
    readonly property var columns: mode === "services" ? ["Name", "State"] : ["Name", "CPU", "Memory", "PID", "State"]
    readonly property var columnWidths: mode === "services" ? [Math.max(160, width - 100), 100] : [Math.max(136, width - 338), 76, 90, 84, 88]
    readonly property real tableWidth: columnWidths.reduce((sum, value) => sum + value, 0)
    readonly property var filteredRecords: records.filter(process => !search || [process.name, process.executable, process.description, String(process.pid ?? "")].some(value => String(value || "").toLowerCase().includes(search.toLowerCase())))
    readonly property var stateGroups: Array.from(new Set(filteredRecords.map(process => stateName(process.state)))).sort((a, b) => a.localeCompare(b))
    readonly property var sortedRecords: filteredRecords.slice().sort((a, b) => {
        if (sortKey === "State") {
            const start = Math.max(0, stateGroups.indexOf(firstState));
            const rank = state => (stateGroups.indexOf(stateName(state)) - start + stateGroups.length) % stateGroups.length;
            return rank(a.state) - rank(b.state) || String(a.name).localeCompare(String(b.name)) || entryKey(a).localeCompare(entryKey(b));
        }
        let delta = 0;
        if (sortKey === "CPU")
            delta = (a.cpuPercent ?? -1) - (b.cpuPercent ?? -1);
        else if (sortKey === "Memory")
            delta = a.memoryBytes - b.memoryBytes;
        else if (sortKey === "PID")
            delta = a.pid - b.pid;
        else
            delta = String(a.name).localeCompare(String(b.name));
        return (descending ? -delta : delta) || (a.pid ?? 0) - (b.pid ?? 0);
    })
    spacing: 10
    implicitHeight: 360
    function entryKey(record) {
        return record.key || record.pid + ":" + (record.startTime || 0);
    }
    function stateName(state) {
        return ({
                R: "Running",
                S: "Sleeping",
                D: "Waiting",
                T: "Stopped",
                t: "Stopped",
                Z: "Zombie",
                I: "Idle"
            })[state] || state || "Unknown";
    }
    function stateColor(state) {
        if (["R", "running", "active", "listening"].includes(state))
            return Theme.success;
        if (["Z", "failed"].includes(state))
            return Theme.danger;
        if (["D", "T", "t", "activating", "deactivating", "start", "start-pre", "start-post", "stop", "stop-sigterm", "stop-sigkill", "auto-restart", "reload"].includes(state))
            return Theme.warning;
        return Theme.muted;
    }
    function selectSort(key) {
        if (key === "State") {
            const current = sortKey === "State" ? stateGroups.indexOf(firstState) : -1;
            firstState = stateGroups.length ? stateGroups[(current + 1) % stateGroups.length] : "";
            descending = false;
        } else {
            descending = sortKey === key ? !descending : ["CPU", "Memory"].includes(key);
        }
        sortKey = key;
        refresh(true);
        list.positionViewAtBeginning();
    }
    function selectIndex(index, modifiers = 0) {
        if (index < 0 || index >= rows.count)
            return;
        const key = rows.get(index).key;
        let selection = modifiers & Qt.ControlModifier ? Object.assign({}, selectedKeys) : {};
        if (modifiers & Qt.ShiftModifier) {
            let anchor = Array.from({
                length: rows.count
            }, (_, i) => rows.get(i).key).indexOf(anchorKey);
            if (anchor < 0)
                anchor = index;
            for (let i = Math.min(anchor, index); i <= Math.max(anchor, index); i++)
                selection[rows.get(i).key] = true;
        } else {
            if ((modifiers & Qt.ControlModifier) && selection[key])
                delete selection[key];
            else
                selection[key] = true;
            anchorKey = key;
        }
        selectedKeys = selection;
        list.currentIndex = index;
    }
    function selectAll() {
        const selection = {};
        for (const process of sortedRecords)
            selection[entryKey(process)] = true;
        selectedKeys = selection;
    }
    onSearchChanged: {
        selectedKeys = {};
        list.positionViewAtBeginning();
    }
    function recordAt(index) {
        const row = rows.get(index);
        return {
            pid: row.pid,
            startTime: row.startTime,
            name: row.name,
            executable: row.executable,
            cpuPercent: row.cpuPercent < 0 ? null : row.cpuPercent,
            memoryBytes: row.memoryBytes,
            threads: row.threads,
            state: row.state
        };
    }
    function refresh(forceOrder = false) {
        if (!rows || !list)
            return;
        const oldOffset = list.contentY - list.originY;
        const keys = Array.from({
            length: rows.count
        }, (_, index) => rows.get(index).key);
        let ordered = sortedRecords;
        if (holdOrder && !forceOrder) {
            const byKey = new Map(sortedRecords.map(process => [entryKey(process), process]));
            ordered = keys.filter(key => byKey.has(key)).map(key => {
                const process = byKey.get(key);
                byKey.delete(key);
                return process;
            }).concat(Array.from(byKey.values()));
        }
        for (let i = 0; i < ordered.length; i++) {
            const process = ordered[i];
            const key = entryKey(process);
            const record = {
                key,
                pid: process.pid ?? -1,
                commandLine: (process.argv || []).map(arg => /^[a-zA-Z0-9_@%+=:,./-]+$/.test(arg) ? arg : "'" + arg.replace(/'/g, "'\"'\"'") + "'").join(" "),
                description: process.description || "",
                startTime: process.startTime || 0,
                name: process.name,
                executable: process.executable || "",
                cpuPercent: process.cpuPercent ?? -1,
                memoryBytes: process.memoryBytes ?? 0,
                threads: process.threads ?? 0,
                state: process.state || ""
            };
            if (i >= keys.length || keys[i] !== key) {
                const found = keys.indexOf(key, i + 1);
                if (found >= 0) {
                    rows.move(found, i, 1);
                    keys.splice(found, 1);
                } else
                    rows.insert(i, record);
                keys.splice(i, 0, key);
            }
            rows.set(i, record);
        }
        if (rows.count > ordered.length)
            rows.remove(ordered.length, rows.count - ordered.length);
        // Moving model rows can change ListView's origin. Preserve the visible
        // offset from that origin, rather than treating contentY as zero-based.
        list.forceLayout();
        list.contentY = list.originY + Math.max(0, Math.min(oldOffset, list.contentHeight - list.height));
    }
    onSortedRecordsChanged: refresh()
    Component.onCompleted: refresh()
    ListModel {
        id: rows
    }
    RowLayout {
        Layout.fillWidth: true
        FilterField {
            id: filter
            objectName: "processSearch"
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            placeholderText: root.mode === "services" ? "Search services" : "Search processes"
            onTextEdited: root.search = text
        }
        Text {
            visible: root.width >= 260
            text: root.selectedRecords.length ? root.selectedRecords.length + " selected" : root.sortedRecords.length + " / " + root.totalCount
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
    DataTable {
        id: table
        objectName: "processTableViewport"
        Layout.fillWidth: true
        Layout.fillHeight: true
        columns: root.columns
        columnWidths: root.columnWidths
        rowHeight: root.rowHeight
        sortKey: root.sortKey
        descending: root.descending
        model: rows
        sortObjectPrefix: "processSort_"
        rowsObjectName: "processRows"
        scrollObjectName: "processTableScrollBar"
        rightAligned: index => root.mode === "processes" && index > 0 && index < 4
        headerLabel: key => key + (root.sortKey === key ? (key === "State" ? " · " + (root.stateGroups.includes(root.firstState) ? root.firstState : root.stateGroups[0] || "") : root.descending ? " ↓" : " ↑") : "")
        headerHint: key => key === "State" ? "Click to show the next state first" + (root.sortKey === "State" && root.stateGroups.length ? " · " + (root.stateGroups.includes(root.firstState) ? root.firstState : root.stateGroups[0]) + " first" : "") : ""
        headerAccessibleName: key => key === "State" ? "Cycle state groups" + (root.sortKey === "State" ? ", " + root.firstState + " first" : "") : "Sort by " + key
        onSortRequested: key => root.selectSort(key)
        onKeyPressed: event => {
            if (event.key === Qt.Key_A && (event.modifiers & Qt.ControlModifier)) {
                root.selectAll();
                event.accepted = true;
                return;
            }
            if (event.key === Qt.Key_Escape) {
                root.selectedKeys = {};
                event.accepted = true;
                return;
            }
            let next = list.currentIndex;
            if (event.key === Qt.Key_Down)
                next++;
            else if (event.key === Qt.Key_Up)
                next--;
            else if (event.key === Qt.Key_Home)
                next = 0;
            else if (event.key === Qt.Key_End)
                next = list.count - 1;
            else
                return;
            next = Math.max(0, Math.min(list.count - 1, next));
            root.selectIndex(next, event.modifiers);
            list.positionViewAtIndex(next, ListView.Contain);
            if (list.currentItem)
                list.currentItem.forceActiveFocus();
            event.accepted = true;
        }
        delegate: Item {
            id: row
            required property int index
            required property string key
            required property int pid
            required property string name
            required property string description
            required property string commandLine
            required property string executable
            required property real cpuPercent
            required property real memoryBytes
            required property string state
            objectName: root.mode === "services" ? "serviceRow_" + name : "processRow_" + pid
            readonly property var desktopEntry: root.mode === "processes" ? root.applicationFor({
                name,
                executable
            }) : null
            width: root.tableWidth
            height: root.rowHeight
            activeFocusOnTab: false
            function openMenu() {
                if (!root.selectedKeys[key])
                    root.selectIndex(index);
                root.contextRequested(row, root.selectedRecords);
            }
            Keys.onMenuPressed: openMenu()
            Keys.onPressed: event => {
                if (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier)) {
                    openMenu();
                    event.accepted = true;
                }
            }
            TableRowSurface {
                anchors.fill: parent
                selected: !!root.selectedKeys[row.key]
                hovered: hover.hovered
                focused: row.activeFocus
            }
            Row {
                height: parent.height
                Item {
                    width: root.columnWidths[0]
                    height: parent.height
                    OsIconImage {
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: 18
                        source: Quickshell.iconPath(row.desktopEntry?.icon || (root.mode === "services" ? "system-run-symbolic" : "application-x-executable"), "application-x-executable")
                    }
                    Column {
                        x: 30
                        width: parent.width - 36
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3
                        Text {
                            width: parent.width
                            text: row.name
                            elide: Text.ElideRight
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                        Text {
                            visible: root.mode === "services" && row.description.length > 0
                            width: parent.width
                            text: row.description
                            elide: Text.ElideRight
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall - 1
                        }
                    }
                }
                Cell {
                    visible: root.mode === "processes"
                    width: root.columnWidths[1] ?? 0
                    text: row.cpuPercent >= 0 ? row.cpuPercent.toFixed(1) + "%" : "—"
                }
                Cell {
                    visible: root.mode === "processes"
                    width: root.columnWidths[2] ?? 0
                    text: root.formatBytes(row.memoryBytes)
                }
                Cell {
                    visible: root.mode === "processes"
                    width: root.columnWidths[3] ?? 0
                    text: String(row.pid)
                }
                Item {
                    width: root.columnWidths[root.mode === "services" ? 1 : 4]
                    height: parent.height
                    Rectangle {
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        width: 6
                        height: 6
                        radius: 3
                        color: root.stateColor(row.state)
                    }
                    Text {
                        x: 18
                        width: parent.width - 24
                        height: parent.height
                        text: root.stateName(row.state)
                        color: root.stateColor(row.state)
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                }
            }
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                preventStealing: root.dragSelecting
                property int lastDragIndex: -1
                onPressed: mouse => {
                    row.forceActiveFocus();
                    if (mouse.button === Qt.LeftButton) {
                        root.dragSelecting = true;
                        lastDragIndex = row.index;
                        root.selectIndex(row.index, mouse.modifiers);
                    }
                }
                onPositionChanged: mouse => {
                    if (!pressed || !root.dragSelecting)
                        return;
                    const point = mapToItem(list.contentItem, mouse.x, mouse.y);
                    const index = Math.max(0, Math.min(rows.count - 1, Math.floor((point.y - list.originY) / root.rowHeight)));
                    if (index !== lastDragIndex) {
                        root.selectIndex(index, Qt.ShiftModifier | (mouse.modifiers & Qt.ControlModifier));
                        lastDragIndex = index;
                    }
                }
                onReleased: mouse => {
                    root.dragSelecting = false;
                    if (mouse.button === Qt.RightButton)
                        row.openMenu();
                }
                onCanceled: root.dragSelecting = false
            }
            HoverHandler {
                id: hover
            }
            ShellTooltip {
                objectName: "processTooltip"
                maximumWidth: root.mode === "processes" ? 640 : 320
                timeout: root.mode === "processes" ? -1 : 8000
                visible: hover.hovered
                text: root.mode === "services" ? row.name + "\n" + row.description : row.name + (row.desktopEntry ? " · " + row.desktopEntry.name : "") + "\n" + (row.commandLine || "Command line unavailable") + "\nPID " + row.pid
            }
        }
    }
    component Cell: Text {
        height: parent.height
        leftPadding: 6
        rightPadding: 6
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.features: ({
                "tnum": 1
            })
    }
}
