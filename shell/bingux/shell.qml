import Quickshell
import Quickshell.Wayland
import QtQuick

ShellRoot {
    id: root

    readonly property int topBarHeight: 36
    property var currentTime: new Date()
    readonly property string formattedTime: formatClock(currentTime)

    function padTime(value) {
        return value < 10 ? "0" + value : String(value);
    }

    function formatClock(timestamp) {
        const weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        return weekdays[timestamp.getDay()]
            + " "
            + padTime(timestamp.getDate())
            + " "
            + months[timestamp.getMonth()]
            + " "
            + padTime(timestamp.getHours())
            + ":"
            + padTime(timestamp.getMinutes())
            + ":"
            + padTime(timestamp.getSeconds());
    }

    ProfileSettings {
        id: profileSettings
    }

    Metrics {
        id: metrics
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.currentTime = new Date()
    }

    PanelWindow {
        id: topBar

        anchors {
            top: true
            left: true
            right: true
        }

        exclusiveZone: root.topBarHeight
        implicitHeight: root.topBarHeight
        color: "#171a21"

        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-top-bar"

        Text {
            anchors {
                left: parent.left
                leftMargin: 12
                verticalCenter: parent.verticalCenter
            }

            color: "#d9dee8"
            font.pixelSize: 13
            text: "Bingux"
        }

        Text {
            anchors.centerIn: parent
            color: "#f5f7fa"
            font.pixelSize: 14
            text: root.formattedTime
        }

        Row {
            anchors {
                right: parent.right
                rightMargin: 12
                verticalCenter: parent.verticalCenter
            }

            spacing: 12

            Tray {
                parentWindow: topBar
            }


            Rectangle {
                width: 5
                height: 5
                radius: width / 2
                color: metrics.available ? "#8cc265" : "#8b94a3"
            }

            Text {
                color: metrics.available ? "#d9dee8" : "#8b94a3"
                font.pixelSize: 12
                text: metrics.cpuLabel
            }

            Text {
                color: metrics.available ? "#d9dee8" : "#8b94a3"
                font.pixelSize: 12
                text: metrics.memoryLabel
            }

            Text {
                color: metrics.available ? "#d9dee8" : "#8b94a3"
                font.pixelSize: 12
                text: metrics.networkLabel
            }

            SystemIndicators {
            }
        }
    }

    Dock {
        settings: profileSettings
        visible: profileSettings.dockEnabled
    }
}
