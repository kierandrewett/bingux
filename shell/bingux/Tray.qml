import Quickshell.Services.SystemTray
import Quickshell.Widgets
import QtQuick

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

                width: 24
                height: 24

                function showMenu(mouse) {
                    const position = trayButton.mapToItem(root.parentWindow.contentItem, mouse.x, mouse.y);
                    modelData.display(root.parentWindow, position.x, position.y);
                }

                IconImage {
                    anchors.centerIn: parent
                    implicitSize: 18
                    source: trayButton.modelData.icon
                }

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                    cursorShape: Qt.PointingHandCursor

                    onClicked: function(mouse) {
                        if (mouse.button === Qt.LeftButton) {
                            if (trayButton.modelData.onlyMenu && trayButton.modelData.hasMenu) {
                                trayButton.showMenu(mouse);
                            } else {
                                trayButton.modelData.activate();
                            }
                        } else if (mouse.button === Qt.MiddleButton) {
                            trayButton.modelData.secondaryActivate();
                        } else if (mouse.button === Qt.RightButton && trayButton.modelData.hasMenu) {
                            trayButton.showMenu(mouse);
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
