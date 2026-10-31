import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.SystemTray
import Quickshell.Widgets

Item {
    id: root

    required property var parentWindow
    property var presentation: null
    property bool panelLayout: false
    property bool serviceEnabled: true
    readonly property int maximumVisibleItems: 12
    property var trayItems: []

    function refreshItems() {
        if (serviceEnabled)
            root.trayItems = SystemTray.items.values;
    }

    Component.onCompleted: root.refreshItems()
    onServiceEnabledChanged: root.refreshItems()

    Connections {
        enabled: root.serviceEnabled
        target: SystemTray.items
        function onValuesChanged() {
            root.refreshItems();
        }
    }

    readonly property real naturalWidth: {
        let width = 0;
        for (let index = 0; index < trayRepeater.count; index++)
            width += trayRepeater.itemAt(index)?.implicitWidth || 0;
        return width;
    }
    implicitWidth: Math.min(naturalWidth, root.maximumVisibleItems * Theme.barIconTarget)
    implicitHeight: panelLayout ? Math.max(Theme.barHeight, trayFlow.implicitHeight) : Theme.barHeight
    width: panelLayout && parent ? parent.width : implicitWidth
    Layout.fillWidth: panelLayout
    Layout.maximumWidth: panelLayout && parent ? parent.width : Infinity
    height: implicitHeight
    clip: true

    Flickable {
        id: trayRow

        width: root.width
        height: root.height
        contentWidth: trayFlow.width
        contentHeight: trayFlow.implicitHeight
        clip: true
        interactive: !root.panelLayout && contentWidth > width
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        onWidthChanged: contentX = Math.max(0, Math.min(contentX, contentWidth - width))
        onContentWidthChanged: contentX = Math.max(0, Math.min(contentX, contentWidth - width))

        Flow {
            id: trayFlow
            width: root.panelLayout ? root.width : root.naturalWidth
            Repeater {
                id: trayRepeater
                model: ScriptModel {
                    values: root.trayItems
                    objectProp: "id"
                }
                delegate: Item {
                    id: trayButton

                    required property var modelData
                    required property int index
                    objectName: "trayItem-" + modelData.id
                    property real transitionProgress: 0
                    property int slideDirection: 1
                    property bool trayMenuRetained: false
                    property bool trayMenuOpenRequested: false
                    property var trayMenuRequestHandle: null
                    readonly property var trayMenuHandle: modelData.menu
                    readonly property real slideOffset: slideDirection * (width + Theme.spaceSmall) * (1 - transitionProgress)
                    function revealFocus() {
                        if (!activeFocus || root.panelLayout)
                            return;
                        trayRow.contentX = Math.max(0, Math.min(trayRow.contentWidth - trayRow.width, Math.max(x + width - trayRow.width, Math.min(trayRow.contentX, x))));
                    }
                    onActiveFocusChanged: {
                        revealFocus();
                        if (!activeFocus && trayMenuOpenRequested)
                            cancelTrayMenuOpen();
                    }
                    onXChanged: if (activeFocus)
                        Qt.callLater(revealFocus)
                    NumberAnimation {
                        id: presenceAnimation
                        target: trayButton
                        property: "transitionProgress"
                        from: 0
                        to: 1
                        duration: Theme.reducedMotion || !root.enabled ? 0 : Theme.motion * 3
                        easing.type: Easing.OutCubic
                    }
                    Component.onCompleted: {
                        // Enter from the nearest outside edge so an item does
                        // not slide across or underneath its neighbour.
                        slideDirection = trayRepeater.count < 2 ? 0 : index < (trayRepeater.count - 1) / 2 ? -1 : 1;
                        presenceAnimation.start();
                    }
                    opacity: transitionProgress
                    transform: Translate {
                        x: trayButton.slideOffset
                    }
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: typeof modelData.tooltipTitle === "string" && modelData.tooltipTitle.length > 0 ? modelData.tooltipTitle : modelData.title
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
                            if (modelData.onlyMenu && modelData.hasMenu)
                                trayButton.openTrayMenu();
                            else if (SessionServices.allows("trayMenus"))
                                modelData.activate();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape && trayButton.trayMenuOpenRequested) {
                            trayButton.cancelTrayMenuOpen();
                            event.accepted = true;
                        }
                    }
                    onTrayMenuHandleChanged: cancelTrayMenuOpen()
                    function cancelTrayMenuOpen() {
                        trayMenuOpenTimeout.stop();
                        trayMenuOpenRequested = false;
                        trayMenuRequestHandle = null;
                    }
                    function openTrayMenu() {
                        if (!SessionServices.allows("trayMenus"))
                            return;
                        trayMenuRequestHandle = trayMenuHandle;
                        trayMenuOpenRequested = true;
                        trayMenuOpenTimeout.restart();
                        forceActiveFocus();
                        trayMenuRetained = true;
                        tryOpenRequestedTrayMenu();
                    }
                    function tryOpenRequestedTrayMenu() {
                        if (!trayMenuOpenRequested || trayMenuRequestHandle !== trayMenuHandle || !trayMenuLoader.item?.hasMenuItems)
                            return;
                        cancelTrayMenuOpen();
                        trayMenuLoader.item.visible = true;
                    }
                    Timer {
                        id: trayMenuOpenTimeout
                        interval: 4000
                        onTriggered: trayButton.cancelTrayMenuOpen()
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
                        return trayButton.isSafeIconSource(icon) ? icon : BinguxIcons.source("application-x-executable", "application-x-executable");
                    }

                    implicitWidth: root.presentation?.custom ? customFace.implicitWidth + Theme.barPrimaryPadding * 2 : Theme.barIconTarget
                    width: root.panelLayout ? Math.min(implicitWidth, root.width) : implicitWidth
                    height: Theme.barHeight
                    BarTooltip {
                        anchorItem: trayButton
                        barWindow: root.parentWindow
                        requested: !DesktopEditing.active && (trayMouse.containsMouse || trayButton.activeFocus) && !(trayMenuLoader.item?.visible ?? false)
                        text: trayButton.Accessible.name || "Tray application"
                    }

                    BarControlSurface {
                        hovered: trayMouse.containsMouse
                        pressed: trayMouse.pressed
                        selected: trayMenuLoader.item?.visible ?? false
                        focused: trayButton.activeFocus
                    }

                    WidgetFace {
                        id: customFace
                        anchors.centerIn: parent
                        visible: !!root.presentation?.custom
                        width: root.panelLayout ? Math.min(implicitWidth, Math.max(0, trayButton.width - Theme.barPrimaryPadding * 2)) : implicitWidth
                        presentation: Object.assign({}, root.presentation || {}, {
                            label: root.presentation?.labelOverridden ? root.presentation.label : trayButton.Accessible.name
                        })
                        iconSource: root.presentation?.iconOverridden ? BinguxIcons.source(root.presentation.icon) : trayButton.iconSource(trayButton.modelData.icon)
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
                    Loader {
                        id: trayMenuLoader
                        active: trayButton.trayMenuRetained || trayMouse.containsMouse || trayButton.activeFocus
                        asynchronous: true
                        sourceComponent: Component {
                            TrayMenu {
                                objectName: "trayItemMenu"
                                menu: trayButton.trayMenuHandle
                                screen: root.parentWindow.screen
                                anchorWindow: root.parentWindow
                                anchorItem: trayButton
                            }
                        }
                        onLoaded: {
                            trayButton.trayMenuRetained = true;
                            trayButton.tryOpenRequestedTrayMenu();
                        }
                    }
                    Connections {
                        target: trayMenuLoader.item
                        function onHasMenuItemsChanged() {
                            trayButton.tryOpenRequestedTrayMenu();
                        }
                    }
                    MouseArea {
                        id: trayMouse
                        hoverEnabled: true
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.ArrowCursor

                        onClicked: function (mouse) {
                            if (mouse.button === Qt.LeftButton) {
                                if (trayButton.modelData.onlyMenu && trayButton.modelData.hasMenu) {
                                    trayButton.openTrayMenu();
                                } else if (SessionServices.allows("trayMenus")) {
                                    trayButton.modelData.activate();
                                }
                            } else if (mouse.button === Qt.MiddleButton && SessionServices.allows("trayMenus")) {
                                trayButton.modelData.secondaryActivate();
                            } else if (mouse.button === Qt.RightButton && trayButton.modelData.hasMenu) {
                                trayButton.openTrayMenu();
                            }
                        }

                        onWheel: function (wheel) {
                            if (!root.panelLayout && trayRow.contentWidth > trayRow.width) {
                                const delta = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.angleDelta.x;
                                trayRow.contentX = Math.max(0, Math.min(trayRow.contentWidth - trayRow.width, trayRow.contentX - delta));
                                wheel.accepted = true;
                            } else {
                                trayButton.modelData.scroll(wheel.angleDelta.y, false);
                            }
                        }
                        onExited: if (trayButton.trayMenuOpenRequested)
                            trayButton.cancelTrayMenuOpen()
                    }
                }
            }
        }
    }
}
