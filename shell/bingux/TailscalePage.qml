import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

Item {
    id: root
    required property var services
    readonly property var service: services.vpns.find(row => row.id === "tailscale") || ({})
    // Polls must not reset the virtual list when its data has not changed.
    property var nodes: []
    onServiceChanged: {
        const next = service.nodes || [];
        if (JSON.stringify(next) !== JSON.stringify(nodes))
            nodes = next;
    }
    readonly property var devices: nodes.filter(node => !node.provider)
    readonly property var preferences: service.preferences || ({})
    readonly property string selectedExit: service.exitNode || ""
    readonly property var activeExit: nodes.find(node => node.id === selectedExit)
    readonly property bool editable: services.ready && !services.busy && !!service.canToggle
    property string tab: "devices"
    property string query: ""
    property string exitSource: "all"
    readonly property var peers: nodeTable.listView
    property string sortKey: ""
    property bool descending: false
    property string copiedId: ""
    property var pendingExit: null
    readonly property var filteredNodes: {
        const needle = query.trim().toLocaleLowerCase();
        let result = (tab === "exit" ? nodes.filter(node => !node.self && node.exitOption && (exitSource === "all" || (exitSource === "provider" ? !!node.provider : !node.provider))) : devices).filter(node => !needle || [node.name, node.dns, node.os, node.country, node.city, node.provider].concat(node.ips).join(" ").toLocaleLowerCase().includes(needle));
        if (tab === "exit")
            result.sort((a, b) => Number(b.id === selectedExit) - Number(a.id === selectedExit) || Number(b.online) - Number(a.online) || (a.country || "").localeCompare(b.country || "") || (a.city || "").localeCompare(b.city || "") || a.name.localeCompare(b.name));
        if (sortKey)
            result.sort((a, b) => {
                const value = node => sortKey === "Name" ? node.name : sortKey === "Address" ? node.ips[0] || "" : sortKey === "Location" ? [node.country, node.city].join(" ") : sortKey === "Provider" ? node.provider || "Tailnet" : sortKey === "OS" ? node.os : node.online ? "Online" : "Offline";
                return (descending ? -1 : 1) * String(value(a)).localeCompare(String(value(b))) || a.name.localeCompare(b.name);
            });
        return result;
    }
    readonly property bool canApply: editable && pendingExit !== null && pendingExit.id !== selectedExit && (pendingExit.id === "" || nodes.some(node => node.id === pendingExit.id && node.online && node.exitOption))
    implicitHeight: 560
    onTabChanged: {
        nodeMenu.visible = false;
        nodeTable.contentX = 0;
        sortKey = "";
        descending = false;
        query = "";
        pendingExit = null;
        peers.positionViewAtBeginning();
    }
    onQueryChanged: peers.positionViewAtBeginning()
    onExitSourceChanged: peers.positionViewAtBeginning()
    onSelectedExitChanged: pendingExit = null
    Keys.onEscapePressed: event => {
        if (pendingExit !== null)
            pendingExit = null;
        else if (query)
            query = "";
        else
            event.accepted = false;
    }
    function countryFlag(node) {
        const code = (node.countryCode || "").toUpperCase();
        return /^[A-Z]{2}$/.test(code) ? String.fromCodePoint(127397 + code.charCodeAt(0), 127397 + code.charCodeAt(1)) : "";
    }
    function exitLabel(node) {
        return node ? [node.city, node.country].filter(Boolean).join(", ") || node.name || node.dns : "";
    }
    function copyAddress(node) {
        if (!node.ips.length)
            return;
        Quickshell.clipboardText = node.ips[0];
        copiedId = node.id;
        copiedFeedback.restart();
    }
    property var contextNode: null
    function openNodeMenu(row, position) {
        contextNode = row.modelData;
        peers.currentIndex = row.index;
        const point = row.mapToItem(nodeMenu.hostItem, position.x, position.y);
        nodeMenu.preferredX = point.x;
        nodeMenu.preferredY = point.y;
        nodeMenu.visible = true;
    }
    onVisibleChanged: if (!visible)
        nodeMenu.visible = false
    TrayMenu {
        id: nodeMenu
        objectName: "tailscaleNodeMenu"
        hostItem: root.Window.window ? root.Window.window.contentItem : null
        actions: [
            {
                text: "Copy IP address",
                enabled: !!root.contextNode && root.contextNode.ips.length > 0,
                isSeparator: false,
                hasChildren: false,
                checkState: Qt.Unchecked,
                triggered: () => root.copyAddress(root.contextNode)
            }
        ]
    }
    Timer {
        id: copiedFeedback
        interval: 1800
        onTriggered: root.copiedId = ""
    }
    readonly property var settingsOptions: [
        {
            id: "accept-dns",
            name: "Tailscale DNS",
            description: "Use DNS settings from your tailnet",
            icon: "network-server-symbolic"
        },
        {
            id: "accept-routes",
            name: "Subnet routes",
            description: "Reach networks shared by other nodes",
            icon: "network-wired-symbolic"
        },
        {
            id: "exit-node-allow-lan-access",
            name: "Local network access",
            description: "Keep LAN access while using an exit node",
            icon: "network-workgroup-symbolic"
        },
        {
            id: "shields-up",
            name: "Block incoming connections",
            description: "Reject incoming connections over Tailscale",
            icon: "security-high-symbolic"
        }
    ]
    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.gap
        SegmentedControl {
            Layout.fillWidth: true
            options: ["devices", "exit", "settings"]
            currentValue: root.tab
            objectNamePrefix: "tailscaleTab_"
            onSelected: value => root.tab = value
            segmentContent: Component {
                Text {
                    text: parent.segmentValue === "devices" ? "Devices" : parent.segmentValue === "exit" ? "Exit nodes" : "Settings"
                    color: parent.segmentSelected ? Theme.text : Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
        ControlRow {
            objectName: "tailscaleConnection"
            visible: root.tab !== "exit"
            title: root.service.connected ? "Connected" : root.service.subtitle || "Unavailable"
            subtitle: root.tab === "devices" ? root.devices.filter(node => node.online).length + " of " + root.devices.length + " tailnet devices online" : ""
            iconName: "network-vpn-symbolic"
            rowInteractive: false
            selected: !!root.service.connected
            toggleVisible: true
            toggleLabel: "Tailscale connection"
            toggleChecked: !!root.service.connected
            toggleEnabled: root.editable
            onToggleRequested: root.services.action({
                kind: "vpn",
                id: "tailscale",
                enabled: !root.service.connected
            })
        }
        ControlRow {
            visible: root.tab === "exit"
            objectName: "tailscaleCurrentExit"
            implicitHeight: 48
            title: root.selectedExit ? (root.activeExit ? root.exitLabel(root.activeExit) : "Exit node unavailable") : "Direct connection"
            subtitle: !root.service.connected ? "Tailscale is disconnected" : root.selectedExit ? (root.activeExit && root.activeExit.online ? "Current exit node" : "Exit node is offline") : "Choose a location to use an exit node"
            iconName: "network-workgroup-symbolic"
            rowInteractive: false
            actionLabel: root.selectedExit ? "Stop using" : ""
            enabled: root.editable
            onActionTriggered: root.pendingExit = {
                id: "",
                name: "Normal connection"
            }
        }
        RowLayout {
            visible: root.tab !== "settings"
            Layout.fillWidth: true
            FilterField {
                id: search
                objectName: "tailscaleSearch"
                Layout.fillWidth: true
                text: root.query
                onTextEdited: root.query = text
                placeholderText: root.tab === "exit" ? "Search location or server" : "Search name or address"
                Keys.onDownPressed: {
                    peers.forceActiveFocus();
                    peers.currentIndex = 0;
                }
            }
        }
        SegmentedControl {
            visible: root.tab === "exit" && root.nodes.some(node => !!node.provider)
            Layout.fillWidth: true
            options: ["all", "tailnet", "provider"]
            currentValue: root.exitSource
            objectNamePrefix: "tailscaleSource_"
            onSelected: value => root.exitSource = value
            segmentContent: Component {
                Text {
                    text: parent.segmentValue === "all" ? "All" : parent.segmentValue === "tailnet" ? "My devices" : "VPN locations"
                    color: parent.segmentSelected ? Theme.text : Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
        DataTable {
            id: nodeTable
            objectName: "tailscaleTable"
            rowsObjectName: "tailscaleNodes"
            sortObjectPrefix: "tailscaleSort_"
            visible: root.tab !== "settings"
            Layout.fillWidth: true
            Layout.fillHeight: true
            headerVisible: root.tab !== "exit"
            rowHeight: root.tab === "exit" ? 56 : 34
            columns: root.tab === "exit" ? [] : ["Name", "Address", "State", "OS"]
            columnWidths: root.tab === "exit" ? [width] : [Math.max(136, width - 210), 126, 84, 80]
            sortKey: root.sortKey
            descending: root.descending
            model: root.filteredNodes
            onSortRequested: key => {
                root.descending = root.sortKey === key ? !root.descending : false;
                root.sortKey = key;
                root.peers.positionViewAtBeginning();
            }
            onKeyPressed: event => {
                const list = root.peers;
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (list.currentItem && list.currentItem.enabled)
                        list.currentItem.clicked();
                } else {
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
                    list.currentIndex = Math.max(0, Math.min(list.count - 1, next));
                    list.positionViewAtIndex(list.currentIndex, ListView.Contain);
                }
                event.accepted = true;
            }
            delegate: AbstractButton {
                id: nodeRow
                required property var modelData
                required property int index
                readonly property string title: modelData.name || modelData.dns
                readonly property string actionLabel: root.copiedId === modelData.id ? "Copied" : "Copy IP"
                readonly property bool activeExit: root.tab === "exit" && modelData.id === root.selectedExit
                width: nodeTable.tableWidth
                height: nodeTable.rowHeight
                hoverEnabled: true
                enabled: root.tab !== "exit" || (root.editable && modelData.online)
                Accessible.name: title + ", " + (modelData.online ? "Online" : "Offline") + (root.tab === "devices" ? ", right-click for options" : ", choose exit node")
                background: TableRowSurface {
                    selected: nodeRow.activeExit || (root.pendingExit !== null && root.pendingExit.id === nodeRow.modelData.id)
                    hovered: nodeRow.hovered
                    focused: nodeRow.visualFocus || (root.peers.activeFocus && root.peers.currentIndex === nodeRow.index)
                }
                contentItem: Item {
                    Row {
                        anchors.fill: parent
                        visible: root.tab !== "exit"
                        Item {
                            width: nodeTable.columnWidths[0]
                            height: parent.height
                            SymbolicIcon {
                                x: 6
                                anchors.verticalCenter: parent.verticalCenter
                                implicitSize: 18
                                source: Quickshell.iconPath(root.tab === "exit" ? "network-vpn-symbolic" : ["android", "iOS"].includes(nodeRow.modelData.os) ? "phone-symbolic" : "computer-symbolic")
                                color: nodeRow.activeExit ? Theme.accent : Theme.muted
                            }
                            NodeCell {
                                x: 30
                                width: parent.width - 30
                                text: nodeRow.title
                                color: nodeRow.enabled ? Theme.text : Theme.muted
                            }
                        }
                        NodeCell {
                            width: nodeTable.columnWidths[1]
                            text: root.tab === "exit" ? [nodeRow.modelData.city, nodeRow.modelData.country].filter(Boolean).join(", ") || "Tailnet" : root.copiedId === nodeRow.modelData.id ? "Copied" : nodeRow.modelData.ips[0] || "—"
                            color: root.copiedId === nodeRow.modelData.id && root.tab === "devices" ? Theme.accent : Theme.text
                        }
                        Item {
                            width: nodeTable.columnWidths[2]
                            height: parent.height
                            readonly property color stateColor: nodeRow.modelData.online ? Theme.success : Theme.muted
                            Rectangle {
                                x: 6
                                anchors.verticalCenter: parent.verticalCenter
                                width: 6
                                height: 6
                                radius: 3
                                color: parent.stateColor
                            }
                            NodeCell {
                                x: 12
                                width: parent.width - 12
                                color: parent.stateColor
                                text: nodeRow.activeExit ? "In use" : nodeRow.modelData.self ? "This PC" : nodeRow.modelData.online ? "Online" : "Offline"
                            }
                        }
                        NodeCell {
                            width: nodeTable.columnWidths[3]
                            text: root.tab === "exit" ? nodeRow.modelData.provider || "Tailnet" : nodeRow.modelData.os || "—"
                            color: Theme.muted
                        }
                    }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        anchors.rightMargin: 14
                        spacing: 10
                        visible: root.tab === "exit"
                        Item {
                            Layout.preferredWidth: 28
                            Layout.preferredHeight: 28
                            Text {
                                anchors.centerIn: parent
                                text: root.countryFlag(nodeRow.modelData)
                                font.family: "Noto Color Emoji"
                                font.pixelSize: 22
                            }
                            SymbolicIcon {
                                anchors.centerIn: parent
                                visible: !root.countryFlag(nodeRow.modelData)
                                implicitSize: 22
                                source: Quickshell.iconPath("computer-symbolic")
                                color: Theme.muted
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                Layout.fillWidth: true
                                text: root.exitLabel(nodeRow.modelData)
                                elide: Text.ElideRight
                                color: nodeRow.enabled ? Theme.text : Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSize
                                font.weight: Font.Medium
                            }
                            Text {
                                Layout.fillWidth: true
                                text: [nodeRow.modelData.provider, nodeRow.modelData.city || nodeRow.modelData.country ? nodeRow.title : "", !nodeRow.modelData.online ? "Offline" : ""].filter(Boolean).join(" · ") || "Your device"
                                elide: Text.ElideRight
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                        }
                        Text {
                            text: nodeRow.activeExit ? "Connected" : ""
                            visible: nodeRow.activeExit
                            color: Theme.accent
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                        Rectangle {
                            Layout.preferredWidth: 16
                            Layout.preferredHeight: 16
                            radius: 8
                            readonly property bool chosen: root.pendingExit !== null ? root.pendingExit.id === nodeRow.modelData.id : nodeRow.activeExit
                            color: "transparent"
                            border.width: 1
                            border.color: chosen ? Theme.accent : Theme.outline
                            Rectangle {
                                anchors.centerIn: parent
                                width: 8
                                height: 8
                                radius: 4
                                color: Theme.accent
                                visible: parent.chosen
                            }
                        }
                    }
                }
                onClicked: {
                    root.peers.currentIndex = index;
                    if (root.tab === "exit" && !activeExit && enabled)
                        root.pendingExit = modelData;
                }
                ContextMenu.menu: null
                ContextMenu.onRequested: position => root.openNodeMenu(nodeRow, position)
                Keys.onMenuPressed: root.openNodeMenu(nodeRow, Qt.point(16, height))
                ShellTooltip {
                    visible: nodeRow.hovered && !nodeMenu.visible
                    text: [nodeRow.title, [nodeRow.modelData.city, nodeRow.modelData.country].filter(Boolean).join(", "), nodeRow.modelData.ips.join("\n"), nodeRow.modelData.self ? "This device" : "", root.tab === "devices" ? "Right-click for options" : nodeRow.modelData.provider || "Tailnet exit node"].filter(Boolean).join("\n")
                }
            }
            Text {
                x: nodeTable.contentX
                y: 40
                width: nodeTable.width
                height: Math.max(0, nodeTable.height - 52)
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.Wrap
                visible: root.peers.count === 0
                text: root.query ? "No matches. Try a different search." : root.tab === "exit" ? "No exit nodes available" : "No tailnet devices available"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
        }
        RowLayout {
            visible: root.tab === "exit"
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            Layout.minimumHeight: 40
            Layout.maximumHeight: 40
            spacing: 8
            Text {
                Layout.fillWidth: true
                text: root.services.busy ? "Connecting…" : root.pendingExit ? (root.pendingExit.id ? root.exitLabel(root.pendingExit) : "Direct connection") : "Select a location"
                elide: Text.ElideRight
                color: root.pendingExit ? Theme.text : Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            ActionButton {
                visible: root.pendingExit !== null
                text: "Cancel"
                onClicked: root.pendingExit = null
            }
            ActionButton {
                objectName: "tailscaleApplyExit"
                text: root.pendingExit && !root.pendingExit.id ? "Disconnect" : "Connect"
                enabled: root.canApply
                onClicked: {
                    root.services.action({
                        kind: "tailscale",
                        setting: "exit-node",
                        value: root.pendingExit.id
                    });
                    root.pendingExit = null;
                }
            }
        }
        Flickable {
            visible: root.tab === "settings"
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: settingsRows.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {
                width: 6
            }
            ColumnLayout {
                id: settingsRows
                width: parent.width - 8
                spacing: Theme.gap
                Repeater {
                    model: root.settingsOptions
                    ControlRow {
                        required property var modelData
                        objectName: "tailscaleSetting_" + modelData.id
                        title: modelData.name
                        subtitle: modelData.description
                        iconName: modelData.icon
                        rowInteractive: false
                        toggleVisible: true
                        toggleChecked: root.preferences[modelData.id] === true
                        toggleEnabled: root.editable && typeof root.preferences[modelData.id] === "boolean"
                        onToggleRequested: root.services.action({
                            kind: "tailscale",
                            setting: modelData.id,
                            enabled: !root.preferences[modelData.id]
                        })
                    }
                }
                Text {
                    visible: Object.keys(root.preferences).length === 0
                    Layout.fillWidth: true
                    text: "Preferences could not be read. Check Tailscale permissions."
                    color: Theme.muted
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSmall
                }
            }
        }
    }
    component NodeCell: Text {
        height: parent.height
        leftPadding: 6
        rightPadding: 6
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.features: ({
                "tnum": 1
            })
    }
}
