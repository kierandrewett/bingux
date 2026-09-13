import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    required property var services
    property string page: "vpn"
    property bool tailscaleOpen: false
    property bool publicNetworkOpen: false
    readonly property var publicNetwork: root.services.publicNetwork || ({})
    onPageChanged: {
        tailscaleOpen = false;
        publicNetworkOpen = false;
    }
    onVisibleChanged: if (!visible) {
        tailscaleOpen = false;
        publicNetworkOpen = false;
    }
    function goBack() {
        if (tailscaleOpen)
            tailscaleOpen = false;
        else
            backRequested();
    }
    function togglePublicNetwork() {
        publicNetworkOpen = !publicNetworkOpen;
        if (publicNetworkOpen && root.services.refreshPublicNetwork)
            root.services.refreshPublicNetwork();
    }
    implicitHeight: layout.implicitHeight
    readonly property bool keyboardNavigation: back.visualFocus
    function focusBack(reason) {
        back.forceActiveFocus(reason);
    }
    signal backRequested
    signal settingsRequested(string panel)
    Keys.onEscapePressed: goBack()
    Keys.onLeftPressed: goBack()
    readonly property var choices: [
        {
            id: "vpn",
            title: "VPN",
            icon: "network-vpn-symbolic",
            available: true
        },
        {
            id: "dnd",
            title: "Do Not Disturb",
            icon: "notifications-disabled-symbolic",
            available: !!services.state.dndAvailable
        },
        {
            id: "nightLight",
            title: "Night Light",
            icon: "night-light-symbolic",
            available: !!services.state.nightLightAvailable
        },
        {
            id: "power",
            title: "Power mode",
            icon: "power-profile-balanced-symbolic",
            available: !!services.state.power && services.state.power.available
        },
        {
            id: "awake",
            title: "Keep Awake",
            icon: "display-brightness-symbolic",
            available: !!services.state.awakeAvailable
        }
    ]
    ColumnLayout {
        id: layout
        anchors.fill: parent
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            IconButton {
                id: back
                objectName: "controlExtrasBack"
                iconName: "go-previous-symbolic"
                label: root.tailscaleOpen ? "Back to VPN" : "Back to Control Centre"
                tooltipEnabled: false
                onClicked: root.goBack()
            }
            Text {
                Layout.fillWidth: true
                text: root.tailscaleOpen ? "Tailscale" : root.page === "vpn" ? "VPN" : root.page === "power" ? "Power mode" : root.page === "session" ? "Power options" : "Customise controls"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.Medium
            }
        }
        Text {
            visible: root.page === "customise"
            Layout.fillWidth: true
            text: "Choose the controls to show. Adding a control does not turn it on."
            wrapMode: Text.Wrap
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        Flickable {
            id: list
            visible: !root.tailscaleOpen
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: rows.implicitHeight
            contentHeight: rows.implicitHeight
            contentWidth: width
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {
                parent: root
                x: list.x + list.width + 4
                y: list.y
                height: list.height
                width: 8
                visible: list.contentHeight > list.height
            }
            ColumnLayout {
                id: rows
                width: list.width
                SessionPower {
                    Layout.fillWidth: true
                    visible: root.page === "session"
                }
                spacing: 4
                Repeater {
                    model: root.page === "vpn" ? root.services.vpns : []
                    ControlRow {
                        required property var modelData
                        objectName: "controlVpn_" + modelData.id
                        title: modelData.name
                        subtitle: modelData.subtitle
                        iconName: "network-vpn-symbolic"
                        rowInteractive: false
                        toggleVisible: true
                        selected: modelData.connected
                        navigation: modelData.id === "tailscale"
                        onNavigationRequested: root.tailscaleOpen = true
                        toggleEnabled: modelData.canToggle && root.services.ready && !root.services.busy
                        onToggleRequested: root.services.action({
                            kind: "vpn",
                            id: modelData.id,
                            enabled: !modelData.connected
                        })
                    }
                }
                ControlRow {
                    id: publicNetworkRow
                    objectName: "controlVpnPublicNetwork"
                    visible: root.page === "vpn"
                    title: "Connection details"
                    subtitle: root.services.publicNetworkBusy ? "Checking public connection…" : root.publicNetwork.status === "ready" ? root.publicNetwork.ip : "Show current IP, provider and region"
                    iconName: "network-wired-symbolic"
                    navigation: true
                    navigationRotation: root.publicNetworkOpen ? 90 : 0
                    onClicked: root.togglePublicNetwork()
                }
                Rectangle {
                    id: publicNetworkDetails
                    objectName: "controlVpnPublicNetworkDetails"
                    visible: root.page === "vpn" && root.publicNetworkOpen
                    Layout.fillWidth: true
                    Layout.leftMargin: 8
                    Layout.rightMargin: 8
                    implicitHeight: detailsColumn.implicitHeight + 20
                    radius: 10
                    color: Theme.elevated
                    border.width: 1
                    border.color: Theme.outline
                    ColumnLayout {
                        id: detailsColumn
                        anchors.fill: parent
                        anchors.margins: 10
                        spacing: 6
                        Text {
                            visible: !!root.services.publicNetworkBusy
                            Layout.fillWidth: true
                            text: "Looking up public connection…"
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                        GridLayout {
                            visible: root.publicNetwork.status === "ready"
                            Layout.fillWidth: true
                            columns: 2
                            columnSpacing: 16
                            rowSpacing: 5
                            Text {
                                text: "Current IP"
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                            Text {
                                objectName: "controlVpnPublicIp"
                                text: root.publicNetwork.ip || "Unavailable"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignRight
                                Layout.fillWidth: true
                            }
                            Text {
                                text: "ISP / Provider"
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                            Text {
                                objectName: "controlVpnPublicProvider"
                                text: root.publicNetwork.provider || "Unavailable"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignRight
                                Layout.fillWidth: true
                            }
                            Text {
                                text: "Country / Region"
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                            Text {
                                objectName: "controlVpnPublicRegion"
                                text: [root.publicNetwork.country, root.publicNetwork.region].filter(Boolean).join(" · ") || "Unavailable"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignRight
                                Layout.fillWidth: true
                            }
                        }
                        Text {
                            visible: root.publicNetwork.status === "error"
                            Layout.fillWidth: true
                            text: root.publicNetwork.error || "Public connection details unavailable."
                            wrapMode: Text.Wrap
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                    }
                }
                Text {
                    visible: root.page === "vpn" && root.services.vpns.length === 0
                    Layout.fillWidth: true
                    Layout.margins: 8
                    text: "No VPN service or saved VPN connections found."
                    color: Theme.muted
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSmall
                }
                Repeater {
                    model: root.page === "power" && root.services.state.power ? root.services.state.power.profiles : []
                    ControlRow {
                        required property string modelData
                        objectName: "controlPower_" + modelData
                        title: modelData === "power-saver" ? "Power Saver" : modelData === "performance" ? "Performance" : "Balanced"
                        iconName: "power-profile-" + modelData + "-symbolic"
                        selected: modelData === root.services.state.power.profile
                        enabled: !root.services.busy
                        onClicked: root.services.action({
                            kind: "power",
                            profile: modelData
                        })
                    }
                }
                Repeater {
                    model: root.page === "customise" ? root.choices : []
                    ControlRow {
                        required property var modelData
                        objectName: "controlOption_" + modelData.id
                        title: modelData.title
                        subtitle: modelData.available ? "" : "Unavailable on this device"
                        iconName: modelData.icon
                        rowInteractive: false
                        toggleVisible: true
                        toggleEnabled: modelData.available || root.services.showControl(modelData.id)
                        selected: root.services.showControl(modelData.id)
                        onToggleRequested: root.services.setControl(modelData.id, !root.services.showControl(modelData.id))
                    }
                }
            }
        }
        Loader {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: item ? item.implicitHeight : 560
            visible: root.tailscaleOpen
            active: root.tailscaleOpen
            sourceComponent: Component {
                TailscalePage {
                    services: root.services
                }
            }
        }
        Text {
            Layout.fillWidth: true
            visible: root.services.error !== ""
            text: root.services.error
            wrapMode: Text.Wrap
            color: Theme.muted
            font.pixelSize: Theme.fontSmall
        }
        Rectangle {
            visible: !root.tailscaleOpen && (root.page === "vpn" || root.page === "power")
            Layout.fillWidth: true
            implicitHeight: 1
            color: Theme.outline
            opacity: 0.4
        }
        ControlRow {
            objectName: "controlExtrasSettings"
            visible: !root.tailscaleOpen && (root.page === "vpn" || root.page === "power")
            title: root.page === "vpn" ? "Network settings…" : "Power settings…"
            iconName: ""
            navigation: true
            onClicked: root.settingsRequested(root.page === "vpn" ? "network" : "power")
        }
    }
}
