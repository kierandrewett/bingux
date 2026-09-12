import QtQuick
import QtQuick.Layouts
import "DesktopLayout.js" as DesktopLayout

ColumnLayout {
    id: root
    objectName: "desktopSettings"
    required property var settings
    readonly property var desktop: settings.draft.desktop
    readonly property var layout: desktop.layout || settings.currentLayout || DesktopLayout.defaults()
    readonly property var controls: desktop.controlCentre || {
        vpn: true,
        dnd: true,
        nightLight: false,
        power: false,
        awake: false
    }
    readonly property var panelCatalog: DesktopLayout.widgets.filter(item => item.panel)
    readonly property var optionalControls: [
        {
            id: "vpn",
            title: "VPN",
            description: "VPN connections and Tailscale exit nodes",
            icon: "network-vpn-symbolic"
        },
        {
            id: "dnd",
            title: "Do Not Disturb",
            description: "Pause notification banners",
            icon: "notifications-disabled-symbolic"
        },
        {
            id: "nightLight",
            title: "Night Light",
            description: "Reduce blue light from your display",
            icon: "night-light-symbolic"
        },
        {
            id: "power",
            title: "Power mode",
            description: "Choose between performance and battery life",
            icon: "power-profile-balanced-symbolic"
        },
        {
            id: "awake",
            title: "Keep awake",
            description: "Temporarily prevent the desktop from sleeping",
            icon: "display-brightness-symbolic"
        }
    ]
    Layout.fillWidth: true
    spacing: 24
    function setPanel(id, shown) {
        const next = JSON.parse(JSON.stringify(layout));
        if (!shown && next.sidebar.length === 1)
            return;
        next.sidebar = next.sidebar.filter(item => item !== id);
        if (shown)
            next.sidebar.push(id);
        settings.update("desktop", "layout", next);
    }
    function setPresentation(container, value) {
        settings.update("desktop", "containers", Object.assign({}, desktop.containers, {
            [container]: {
                display: value
            }
        }));
    }
    component ToggleRow: SettingsRow {
        toggleVisible: true
        focusPolicy: Qt.NoFocus
        Accessible.role: Accessible.Grouping
        onClicked: toggleRequested()
    }
    component Route: SettingsRow {
        property string destination
        navigation: true
        onClicked: root.settings.page = destination
    }
    ColumnLayout {
        visible: root.settings.page === "Desktop"
        Layout.fillWidth: true
        spacing: 24
        SettingsHeading {
            title: "Desktop"
            description: "Choose how your workspace is arranged."
            SettingsGroup {
                Route {
                    title: "Top Bar"
                    subtitle: "Status icons, labels and system monitors"
                    iconName: "view-dual-symbolic"
                    destination: "TopBar"
                }
                Route {
                    title: "Dock"
                    subtitle: "Application size, position and mouse actions"
                    iconName: "view-app-grid-symbolic"
                    valueText: root.desktop.dock ? "On" : "Off"
                    destination: "Dock"
                }
                Route {
                    title: "Sidebar"
                    subtitle: "Panels and screen position"
                    iconName: "sidebar-show-symbolic"
                    valueText: root.desktop.sidebar ? "On" : "Off"
                    destination: "Sidebar"
                }
            }
        }
        SettingsHeading {
            title: "Layout"
            SettingsGroup {
                SettingsRow {
                    objectName: "customiseDesktop"
                    title: "Arrange widgets"
                    subtitle: "Move widgets between the top bar, dock and sidebar"
                    navigation: true
                    onClicked: root.settings.customiser.open()
                }
            }
        }
    }
    ColumnLayout {
        visible: root.settings.page === "TopBar"
        Layout.fillWidth: true
        spacing: 24
        SettingsHeading {
            title: "System Monitors"
            SettingsGroup {
                ToggleRow {
                    objectName: "settingsMetrics"
                    title: "Show system monitors"
                    subtitle: "CPU and memory usage in the top bar"
                    toggleChecked: root.desktop.metrics
                    onToggleRequested: root.settings.update("desktop", "metrics", !toggleChecked)
                }
            }
        }
        SettingsHeading {
            title: "Widget appearance"
            description: "Individual widget choices take priority over these defaults."
            SettingsGroup {
                Repeater {
                    model: [
                        {
                            id: "top-left",
                            title: "Left side"
                        },
                        {
                            id: "top-center",
                            title: "Centre"
                        },
                        {
                            id: "top-right",
                            title: "Right side"
                        }
                    ]
                    SettingsOptionRow {
                        required property var modelData
                        objectName: "settingsPresentation" + modelData.id
                        title: modelData.title
                        value: root.desktop.containers?.[modelData.id]?.display || "native"
                        options: [
                            {
                                value: "native",
                                label: "Default"
                            },
                            {
                                value: "icons",
                                label: "Icons"
                            },
                            {
                                value: "text",
                                label: "Labels"
                            },
                            {
                                value: "both",
                                label: "Icons and labels"
                            }
                        ]
                        onChosen: value => root.setPresentation(modelData.id, value)
                    }
                }
            }
        }
        SettingsGroup {
            SettingsRow {
                title: "Arrange top bar widgets"
                navigation: true
                onClicked: {
                    root.settings.customiser.open();
                    root.settings.customiser.selectedContainer = "top-right";
                }
            }
        }
    }
    ColumnLayout {
        visible: root.settings.page === "Sidebar"
        Layout.fillWidth: true
        spacing: 24
        SettingsGroup {
            ToggleRow {
                title: "Show sidebar"
                toggleChecked: root.desktop.sidebar
                onToggleRequested: root.settings.update("desktop", "sidebar", !toggleChecked)
            }
        }
        SettingsHeading {
            title: "Position"
            enabled: root.desktop.sidebar
            SettingsGroup {
                SettingsOptionRow {
                    objectName: "settingsSidebarEdge"
                    title: "Screen edge"
                    value: root.desktop.sidebarEdge || root.settings.currentSidebarEdge
                    options: [
                        {
                            value: "left",
                            label: "Left"
                        },
                        {
                            value: "right",
                            label: "Right"
                        },
                        {
                            value: "top",
                            label: "Top"
                        }
                    ]
                    onChosen: value => root.settings.update("desktop", "sidebarEdge", value)
                }
            }
        }
        SettingsHeading {
            title: "Panels"
            description: "Choose which panels appear in the sidebar menu. Keep at least one enabled."
            enabled: root.desktop.sidebar
            SettingsGroup {
                Repeater {
                    model: root.panelCatalog
                    ToggleRow {
                        required property var modelData
                        objectName: "settingsPanel" + modelData.id
                        title: modelData.label
                        iconName: modelData.icon
                        toggleChecked: root.layout.sidebar.includes(modelData.id)
                        enabled: !toggleChecked || root.layout.sidebar.length > 1
                        onToggleRequested: root.setPanel(modelData.id, !toggleChecked)
                    }
                }
            }
        }
    }
    ColumnLayout {
        visible: root.settings.page === "Controls"
        Layout.fillWidth: true
        spacing: 24
        SettingsHeading {
            title: "Optional controls"
            description: "Add the controls you use. Wi-Fi, Bluetooth and sound are always available."
            SettingsGroup {
                Repeater {
                    model: root.optionalControls
                    ToggleRow {
                        required property var modelData
                        objectName: "settingsControl" + modelData.id
                        title: modelData.title
                        subtitle: modelData.description
                        iconName: modelData.icon
                        toggleChecked: root.controls[modelData.id]
                        onToggleRequested: root.settings.update("desktop", "controlCentre", Object.assign({}, root.controls, {
                            [modelData.id]: !toggleChecked
                        }))
                    }
                }
            }
        }
        SettingsHeading {
            title: "Layout"
            SettingsGroup {
                SettingsOptionRow {
                    title: "Control labels"
                    value: root.desktop.containers?.["control-centre"]?.display || "native"
                    options: [
                        {
                            value: "native",
                            label: "Default"
                        },
                        {
                            value: "icons",
                            label: "Icons"
                        },
                        {
                            value: "text",
                            label: "Labels"
                        },
                        {
                            value: "both",
                            label: "Icons and labels"
                        }
                    ]
                    onChosen: value => root.setPresentation("control-centre", value)
                }
                SettingsRow {
                    title: "Arrange controls"
                    navigation: true
                    onClicked: {
                        root.settings.customiser.open();
                        root.settings.customiser.selectedContainer = "control-centre";
                    }
                }
            }
        }
    }
}
