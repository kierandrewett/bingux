//@ pragma UseQApplication

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets

ShellRoot {
    id: root

    property var currentTime: new Date()

    function openSearch() {
        searchOverlay.showSearch();
    }

    ProfileSettings {
        id: profileSettings
    }

    Metrics {
        id: metrics
    }

    SearchOverlay {
        id: searchOverlay
    }

    NotificationState {
        id: notificationState
    }

    NotificationSurface {
        state: notificationState
    }

    OsdState {
        id: osdState
    }

    OsdSurface {
        state: osdState
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.currentTime = new Date()
    }

    IpcHandler {
        target: "shell"
        function calendar(): void { calendarPopup.visible = !calendarPopup.visible }
        function search(): void { root.openSearch() }
        function controls(): void { controlCentre.visible = !controlCentre.visible }
    }

    ControlCentre { id: controlCentre; indicators: systemIndicators; screen: topBar.screen }

    CalendarPopup { id: calendarPopup; screen: topBar.screen }

    PanelWindow {
        id: topBar
        exclusiveZone: Theme.barHeight
        implicitHeight: Theme.barHeight
        color: Theme.background
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-top-bar"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
        anchors { top: true; left: true; right: true }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.padding
            anchors.rightMargin: Theme.padding
            spacing: Theme.gap
            Item {
                Layout.fillWidth: true
                Layout.preferredWidth: Math.max(searchPill.implicitWidth, rightControls.implicitWidth)
                Layout.minimumWidth: Math.max(searchPill.implicitWidth, rightControls.implicitWidth)
                Layout.fillHeight: true
                Pill {
                    id: searchPill
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    color: searchMouse.containsMouse || searchPill.activeFocus ? Theme.surface : "transparent"
                    activeFocusOnTab: true
                    Accessible.name: "Search applications"
                    Accessible.role: Accessible.Button
                    Keys.onReturnPressed: root.openSearch()
                    Keys.onSpacePressed: root.openSearch()
                    SymbolicIcon { implicitSize: Theme.iconSize; source: Quickshell.iconPath("system-search-symbolic") }
                    MouseArea { id: searchMouse; parent: searchPill; anchors.fill: parent; hoverEnabled: true; onClicked: root.openSearch() }
                }
            }
            Pill {
                id: clockPill
                color: clockMouse.containsMouse || clockPill.activeFocus ? Theme.surface : "transparent"
                activeFocusOnTab: true
                Accessible.name: "Calendar, " + clockLabel.text
                Accessible.role: Accessible.Button
                Keys.onReturnPressed: calendarPopup.visible = !calendarPopup.visible
                Keys.onSpacePressed: calendarPopup.visible = !calendarPopup.visible
                Text { id: clockLabel; text: root.currentTime.toLocaleDateString(Qt.locale(), "ddd d MMM") + "   " + root.currentTime.toLocaleTimeString(Qt.locale(), "hh:mm"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
                MouseArea { id: clockMouse; parent: clockPill; anchors.fill: parent; hoverEnabled: true; onClicked: calendarPopup.visible = !calendarPopup.visible }
            }
            Item {
                Layout.fillWidth: true
                Layout.preferredWidth: Math.max(searchPill.implicitWidth, rightControls.implicitWidth)
                Layout.minimumWidth: Math.max(searchPill.implicitWidth, rightControls.implicitWidth)
                Layout.fillHeight: true
                RowLayout {
                    id: rightControls
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Theme.gap
                    Pill { visible: tray.implicitWidth > 0; Tray { id: tray; parentWindow: topBar } }
                    Pill { visible: privacy.implicitWidth > 0; PrivacyIndicators { id: privacy; metrics: metrics } }
                    Pill {
                        visible: profileSettings.metricsEnabled && metrics.available && topBar.width > 1400
                        Text { text: metrics.cpuLabel; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        Text { text: metrics.memoryLabel; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                    }
                    Pill { visible: metrics.desktopStateAvailable; InputSourceSelector { parentWindow: topBar; metrics: metrics; gnoblinCtlPath: profileSettings.gnoblinCtlPath } }
                    Pill {
                        id: systemPill
                        activeFocusOnTab: true
                        Accessible.name: "Open control centre"
                        Accessible.role: Accessible.Button
                        Keys.onReturnPressed: controlCentre.visible = !controlCentre.visible
                        Keys.onSpacePressed: controlCentre.visible = !controlCentre.visible
                        SystemIndicators { id: systemIndicators; timeoutPath: profileSettings.timeoutPath }
                        MouseArea { parent: systemPill; anchors.fill: parent; onClicked: controlCentre.visible = !controlCentre.visible }
                    }
                }
            }
        }
    }

    Dock {
        settings: profileSettings
        visible: profileSettings.dockEnabled
    }

}
