import QtQuick
import QtQuick.Layouts
import Quickshell

// A gallery of the shared shell states for a human to look at, run under a
// compositor: tests/qml/run.sh ShellStatesTest.qml. It draws StateMessage,
// ShellNotice, ShellDialogCard, SessionRow, ControlSwitch, ActionButton tones,
// SessionSlider, OsdCard and the period picker, and prints SHELL_STATES_DONE.
ShellRoot {
    FloatingWindow {
        implicitWidth: 1280
        implicitHeight: 800
        color: "#26262a"
        Flow {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 12
            Repeater {
                model: 1
                Item {
                    width: 0
                    height: 0
                }
            }
            Rectangle {
                width: 300; height: 560; radius: 16; color: Theme.popupSurface
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 8; spacing: 4
                    StateMessage { compact: true; iconName: "network-offline-symbolic"; title: "No active network connection" }
                    StateMessage { compact: true; loading: true; title: "Looking for networks…" }
                    StateMessage { compact: true; tone: "danger"; title: "Could not load connections"; detail: "Open Network settings to manage them."; actionText: "Try again" }
                    StateMessage { compact: true; tone: "warning"; title: "Some search sources are unavailable" }
                    StateMessage { compact: true; tone: "success"; title: "Restarted foo.service" }
                    StateMessage { fill: true; iconName: "checkbox-checked-symbolic"; title: "No tasks"; detail: "Add one above to start your list." }
                }
            }
            Rectangle {
                width: 340; height: 560; radius: 16; color: Theme.popupSurface
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 12; spacing: 8
                    ShellNotice { tone: "danger"; title: "This erases data"; body: "Installing erases /dev/vda completely." }
                    ShellNotice { tone: "warning"; title: "Check the cable"; body: "The display is not connected." }
                    ShellNotice { tone: "info"; title: "Existing data is kept"; body: "Install into the free space." }
                    ShellNotice { tone: "success"; title: "Saved"; body: "Log saved to /tmp/x.log" }
                    Row {
                        spacing: 12
                        ControlSwitch { checked: false }
                        ControlSwitch { checked: true }
                        ControlSwitch { checked: false; enabled: false }
                    }
                    Row {
                        spacing: 8
                        ActionButton { text: "Wait" }
                        ActionButton { text: "Allow"; tone: "primary" }
                        ActionButton { text: "Force Quit"; tone: "danger" }
                    }
                    SessionSlider { Layout.fillWidth: true; value: 0.4; fillColor: Theme.accent; caption: "New size"; valueText: "40 GiB" }
                    ControlRow { Layout.fillWidth: true; title: "Shut Down"; iconName: "system-shutdown-symbolic"; destructive: true }
                    ControlRow { Layout.fillWidth: true; title: "Cancel"; iconName: "window-close-symbolic" }
                }
            }
            Rectangle {
                width: 300; height: 560; radius: 16; color: "#3a4a6a"
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 12; spacing: 4
                    SessionRow { Layout.fillWidth: true; text: "Rest" }
                    SessionRow { Layout.fillWidth: true; text: "Selected"; selected: true; ticked: true }
                    SessionRow { Layout.fillWidth: true; text: "Restart"; iconName: "system-reboot-symbolic"; destructive: true }
                    SessionRow { id: focused; Layout.fillWidth: true; text: "Focused"; Component.onCompleted: forceActiveFocus(Qt.TabFocusReason) }
                }
            }
            ShellDialogCard {
                width: 420
                tone: "danger"
                title: "“Notes” is not responding"
                body: "You can wait for it to respond, or force it to quit. Unsaved work in this application is lost."
                secondaryText: "Wait"
                primaryText: "Force Quit"
            }
            Rectangle {
                width: 380; height: 330; radius: 16; color: Theme.popupSurface
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 10; spacing: 8
                    OsdCard { request: ({ icon: "audio-volume-high-symbolic", label: "Headphones", level: 0.65, maxLevel: 1 }); opacity: 1 }
                    OsdCard { request: ({ icon: "action-unavailable-symbolic", label: "Brightness", level: -1, maxLevel: -1 }); opacity: 1 }
                    CalendarPeriodPicker { Layout.fillWidth: true; Layout.preferredHeight: 110; level: 2; year: 2024; selectedDate: new Date(2024, 5, 1) }
                }
            }
        }
        Timer {
            interval: 6000
            running: true
            onTriggered: console.warn("SHELL_STATES_DONE")
        }
    }
}
