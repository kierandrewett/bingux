import QtQuick
import Quickshell
import Quickshell.Services.SystemTray
import Quickshell.Widgets

Item {
    id: root

    required property var parentWindow
    property var presentation: null
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

    implicitWidth: Math.min(trayRow.contentWidth, root.maximumVisibleItems * Theme.barIconTarget)
    implicitHeight: trayRow.implicitHeight
    width: implicitWidth
    height: implicitHeight
    clip: true

    ListView {
        id: trayRow

        width: root.width
        height: Theme.barHeight
        implicitWidth: Math.min(contentWidth, root.maximumVisibleItems * Theme.barIconTarget)
        implicitHeight: Theme.barHeight
        orientation: ListView.Horizontal
        spacing: 0
        clip: true
        interactive: contentWidth > width
        boundsBehavior: Flickable.StopAtBounds
        model: root.trayItems
        add: Transition {
            ParallelAnimation {
                NumberAnimation {
                    property: "opacity"
                    from: 0
                    to: 1
                    duration: Theme.reducedMotion ? 0 : Theme.motion * 2
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    property: "scale"
                    from: 0
                    to: 1
                    duration: Theme.reducedMotion ? 0 : Theme.motion * 2
                    easing.type: Easing.OutCubic
                }
            }
        }

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

                width: root.presentation?.custom ? customFace.implicitWidth + Theme.barPrimaryPadding * 2 : Theme.barIconTarget
                height: Theme.barHeight
                BarTooltip {
                    anchorItem: trayButton
                    barWindow: root.parentWindow
                    requested: (trayMouse.containsMouse || trayButton.activeFocus) && !trayMenu.visible
                    text: trayButton.Accessible.name || "Tray application"
                }

                BarControlSurface {
                    hovered: trayMouse.containsMouse
                    pressed: trayMouse.pressed
                    selected: trayMenu.visible
                    focused: trayButton.activeFocus
                }

                WidgetFace {
                    id: customFace; anchors.centerIn: parent; visible: !!root.presentation?.custom
                    presentation: Object.assign({}, root.presentation || {}, {label: root.presentation?.labelOverridden ? root.presentation.label : trayButton.Accessible.name})
                    iconSource: root.presentation?.iconOverridden ? Quickshell.iconPath(root.presentation.icon) : trayButton.iconSource(trayButton.modelData.icon)
                    colouredIcon: !root.presentation?.iconOverridden
                }

                OsIconImage {
                    visible: !root.presentation?.custom && !trayButton.usesFallbackIcon
                    anchors.centerIn: parent
                    implicitSize: Theme.iconSize
                    source: trayButton.iconSource(trayButton.modelData.icon)
                }

                Item {
                    visible: !root.presentation?.custom && trayButton.usesFallbackIcon
                    width: 16
                    height: 16
                    anchors.centerIn: parent

                    Rectangle {
                        x: 8
                        y: 4
                        width: 2
                        height: 10
                        color: Theme.accent
                    }

                    Rectangle {
                        x: 3
                        y: 12
                        width: 12
                        height: 2
                        color: Theme.accent
                    }

                    Rectangle {
                        x: 7
                        y: 1
                        width: 5
                        height: 5
                        radius: 2.5
                        color: Theme.accent
                    }

                    Rectangle {
                        x: 0
                        y: 11
                        width: 5
                        height: 5
                        radius: 2.5
                        color: Theme.accent
                    }

                    Rectangle {
                        x: 13
                        y: 11
                        width: 5
                        height: 5
                        radius: 2.5
                        color: Theme.accent
                    }
                }
                TrayMenu {
                    id: trayMenu
                    menu: trayButton.modelData.menu
                    screen: root.parentWindow.screen
                    anchorWindow: root.parentWindow
                    anchorItem: trayButton
                    function open() {
                        visible = true;
                    }
                }
                MouseArea {
                    id: trayMouse
                    hoverEnabled: true
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                    cursorShape: Qt.ArrowCursor

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
