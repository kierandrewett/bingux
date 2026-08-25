import QtQuick
import Quickshell
import Quickshell.Services.SystemTray
import Quickshell.Widgets

Item {
    id: root

    required property var parentWindow

    implicitWidth: trayRow.implicitWidth
    implicitHeight: trayRow.implicitHeight
    width: implicitWidth
    height: implicitHeight

    Row {
        id: trayRow

        width: implicitWidth
        height: implicitHeight
        spacing: 2

        Repeater {
            model: SystemTray.items

            delegate: Item {
                id: trayButton

                required property var modelData
                // Tailscale can expose a pixmap URL that the layer-shell image provider cannot render.
                readonly property bool usesFallbackIcon: {
                    const icon = modelData.icon;
                    return typeof icon !== "string" || icon.length === 0 || icon.startsWith("image://");
                }

                width: 24
                height: 24

                IconImage {
                    visible: !trayButton.usesFallbackIcon
                    anchors.centerIn: parent
                    implicitSize: 18
                    source: trayButton.modelData.icon
                }

                Item {
                    visible: trayButton.usesFallbackIcon
                    width: 18
                    height: 18
                    anchors.centerIn: parent

                    Rectangle {
                        x: 8
                        y: 4
                        width: 2
                        height: 10
                        color: "#8bd5ff"
                    }

                    Rectangle {
                        x: 3
                        y: 12
                        width: 12
                        height: 2
                        color: "#8bd5ff"
                    }

                    Rectangle {
                        x: 7
                        y: 1
                        width: 5
                        height: 5
                        radius: 2.5
                        color: "#8bd5ff"
                    }

                    Rectangle {
                        x: 0
                        y: 11
                        width: 5
                        height: 5
                        radius: 2.5
                        color: "#8bd5ff"
                    }

                    Rectangle {
                        x: 13
                        y: 11
                        width: 5
                        height: 5
                        radius: 2.5
                        color: "#8bd5ff"
                    }
                }
                QsMenuAnchor {
                    id: trayMenu

                    menu: trayButton.modelData.menu
                    anchor.window: root.parentWindow
                    anchor.item: trayButton
                }
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                    cursorShape: Qt.PointingHandCursor

                    onClicked: function(mouse) {
                        if (mouse.button === Qt.LeftButton) {
                            if (trayButton.modelData.onlyMenu && trayButton.modelData.hasMenu) {
                                trayMenu.open();
                            } else {
                                trayButton.modelData.activate();
                            }
                        } else if (mouse.button === Qt.MiddleButton) {
                            trayButton.modelData.secondaryActivate();
                        } else if (mouse.button === Qt.RightButton && trayButton.modelData.hasMenu) {
                            trayMenu.open();
                        }
                    }

                    onWheel: function(wheel) {
                        trayButton.modelData.scroll(wheel.angleDelta.y, false);
                    }
                }
            }
        }
    }
}
