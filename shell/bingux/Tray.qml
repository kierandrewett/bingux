import QtQuick
import Quickshell
import Quickshell.Services.SystemTray
import Quickshell.Widgets

Item {
    id: root

    required property var parentWindow
    readonly property int maximumVisibleItems: 12
    property var trayItems: []

    function refreshItems() {
        root.trayItems = SystemTray.items.values;
    }

    Component.onCompleted: root.refreshItems()

    Connections {
        target: SystemTray.items
        function onValuesChanged() {
            root.refreshItems();
        }
    }

    implicitWidth: Math.min(trayRow.contentWidth, root.maximumVisibleItems * 26)
    implicitHeight: trayRow.implicitHeight
    width: implicitWidth
    height: implicitHeight
    clip: true

    ListView {
        id: trayRow

        width: root.width
        height: 24
        implicitWidth: Math.min(contentWidth, root.maximumVisibleItems * 26)
        implicitHeight: 24
        orientation: ListView.Horizontal
        spacing: 2
        clip: true
        interactive: contentWidth > width
        boundsBehavior: Flickable.StopAtBounds
        model: root.trayItems

            delegate: Item {
                id: trayButton

                required property var modelData
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: typeof modelData.tooltipTitle === "string" && modelData.tooltipTitle.length > 0 ? modelData.tooltipTitle : modelData.title
                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
                        if (modelData.onlyMenu && modelData.hasMenu)
                            trayMenu.open();
                        else
                            modelData.activate();
                        event.accepted = true;
                    }
                }
                readonly property bool isTailscaleItem: {
                    const id = typeof modelData.id === "string" ? modelData.id : "";
                    const title = typeof modelData.title === "string" ? modelData.title.toLowerCase() : "";
                    const tooltipTitle = typeof modelData.tooltipTitle === "string" ? modelData.tooltipTitle.toLowerCase() : "";
                    return id.startsWith("systray_") && (title === "disconnected" || title.indexOf("tailscale") >= 0 || tooltipTitle.indexOf("tailscale") >= 0);
                }
                readonly property bool usesFallbackIcon: {
                    const icon = modelData.icon;
                    return isTailscaleItem && (typeof icon !== "string" || icon.length === 0 || icon.startsWith("image://"));
                }
                function isSafeIconSource(icon) {
                    if (typeof icon !== "string" || icon.length === 0 || icon.length > 256 || /[\u0000-\u001f\u007f-\u009f]/.test(icon))
                        return false;

                    if (icon.startsWith("image://"))
                        return /^image:\/\/[A-Za-z0-9._-]+\/[A-Za-z0-9._?=&:/-]+$/.test(icon);

                    return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(icon);
                }

                function iconSource(icon) {
                    return trayButton.isSafeIconSource(icon) ? icon : Quickshell.iconPath("application-x-executable", "application-x-executable");
                }

                width: 24
                height: 24

                IconImage {
                    visible: !trayButton.usesFallbackIcon
                    anchors.centerIn: parent
                    implicitSize: 18
                    source: trayButton.iconSource(trayButton.modelData.icon)
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
                TrayMenu {
                    id: trayMenu
                    menu: trayButton.modelData.menu
                    screen: root.parentWindow.screen
                    function open() {
                        preferredX = trayButton.mapToItem(root.parentWindow.contentItem, 0, 0).x - popupWidth + trayButton.width;
                        visible = true;
                    }
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
                        if (trayRow.contentWidth > trayRow.width) {
                            const delta = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.angleDelta.x;
                            trayRow.contentX = Math.max(0, Math.min(trayRow.contentWidth - trayRow.width, trayRow.contentX - delta));
                            wheel.accepted = true;
                        } else {
                            trayButton.modelData.scroll(wheel.angleDelta.y, false);
                        }
                    }
                }
            }
        }
}
