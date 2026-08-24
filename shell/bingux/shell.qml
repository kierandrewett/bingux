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
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        Row {
            anchors {
                left: parent.left
                leftMargin: 12
                verticalCenter: parent.verticalCenter
            }

            spacing: 8

            Text {
                color: "#d9dee8"
                font.pixelSize: 13
                text: "Bingux"
            }

            Item {
                id: searchButton

                implicitWidth: searchButtonContents.implicitWidth + 12
                implicitHeight: 24
                width: implicitWidth
                height: implicitHeight
                activeFocusOnTab: true
                Accessible.name: "Open search"
                Accessible.role: Accessible.Button

                Rectangle {
                    anchors.fill: parent
                    radius: 4
                    color: (searchButtonMouse.containsMouse || searchButton.activeFocus) ? "#2b3545" : "transparent"
                }

                Row {
                    id: searchButtonContents

                    anchors.centerIn: parent
                    spacing: 4

                    IconImage {
                        implicitSize: 14
                        source: Quickshell.iconPath("system-search-symbolic", "edit-find-symbolic")
                    }

                    Text {
                        color: "#d9dee8"
                        font.pixelSize: 12
                        text: "Search"
                    }
                }

                MouseArea {
                    id: searchButtonMouse

                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true

                    onClicked: {
                        searchButton.forceActiveFocus();
                        root.openSearch();
                    }
                }

                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
                        root.openSearch();
                        event.accepted = true;
                    }
                }
            }
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

            PrivacyIndicators {
                metrics: metrics
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

            InputSourceSelector {
                metrics: metrics
                gnoblinCtlPath: profileSettings.gnoblinCtlPath
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
