import QtQuick
import QtQuick.Layouts
import Quickshell

// A gallery of the panel-level shell states for a human to look at, run under a
// compositor like ShellStatesTest.qml: the session power confirm step, a services
// list that could not be read, and a services list with an action error. It
// prints SHELL_PANEL_STATES_DONE once drawn.
ShellRoot {
    FloatingWindow {
        implicitWidth: 1000
        implicitHeight: 620
        color: "#26262a"
        RowLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 12
            Rectangle {
                Layout.preferredWidth: 300
                Layout.fillHeight: true
                radius: 16
                color: Theme.popupSurface
                SessionPower {
                    anchors.fill: parent
                    anchors.margins: 8
                    pending: "reboot"
                }
            }
            Rectangle {
                Layout.preferredWidth: 340
                Layout.fillHeight: true
                radius: 16
                color: Theme.popupSurface
                ServicesPanel {
                    anchors.fill: parent
                    anchors.margins: 8
                    readFailed: true
                }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: 16
                color: Theme.popupSurface
                ServicesPanel {
                    anchors.fill: parent
                    anchors.margins: 8
                    actionError: "Could not restart ssh.service"
                    actionDetail: "The service manager refused the request."
                }
            }
        }
    }
    Component.onCompleted: console.warn("SHELL_PANEL_STATES_DONE")
}
