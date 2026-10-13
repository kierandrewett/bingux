//@ pragma UseQApplication
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Services.SystemTray
import Quickshell.Wayland
import "../shell/bingux"

ShellRoot {
    id: root
    property var nativeTray: null
    property var trayWindow: null
    property var peerTray: null
    property var peerWindow: null
    property var popup: null
    function findChild(item, name) {
        return item ? objectFinder.findChild(item, name) : null;
    }
    function loadedMenu(target) {
        const direct = root.findChild(target, "trayItemMenu");
        if (direct)
            return direct;
        for (const child of target?.children || []) {
            if ("item" in child && child.item?.objectName === "trayItemMenu")
                return child.item;
        }
        return null;
    }
    // QObject lookup includes the Loader's nonvisual Scope, unlike Item.children.
    TestCase {
        id: objectFinder
        when: false
    }

    PanelWindow {
        id: panel
        visible: true
        anchors {
            top: true
            left: true
            right: true
        }
        implicitHeight: 52
        color: "#20242b"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-native-tray-fixture"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
        Row {
            anchors.centerIn: parent
            spacing: 24
            Tray {
                id: tray
                objectName: "nativeTrayTarget"
                parentWindow: panel
                serviceEnabled: false
                width: implicitWidth
                height: implicitHeight
                trayItems: SystemTray.items.values.filter(item => item.id === "tray-menu-native-target")
            }
            Tray {
                id: peer
                objectName: "nativeTrayPeer"
                parentWindow: panel
                width: implicitWidth
                height: implicitHeight
                serviceEnabled: false
                trayItems: SystemTray.items.values.filter(item => item.id !== "tray-menu-native-target")
            }
        }
        Component.onCompleted: {
            root.nativeTray = tray;
            root.peerTray = peer;
            root.trayWindow = panel;
        }
    }

    IpcHandler {
        target: "tray-menu-native"
        function snapshot(): string {
            const target = root.findChild(root.nativeTray, "trayItem-tray-menu-native-target");
            const menu = root.loadedMenu(target);
            const nativeWindow = menu ? menu.nativeWindow : null;
            const peerIds = root.peerTray ? root.peerTray.trayItems.map(item => item.id) : [];
            return JSON.stringify({
                ready: !!target,
                targetId: target ? target.modelData.id : "",
                targetGeometry: target ? {
                    x: target.mapToItem(root.trayWindow.contentItem, 0, 0).x + root.trayWindow.margins.left,
                    y: target.mapToItem(root.trayWindow.contentItem, 0, 0).y + root.trayWindow.margins.top,
                    width: target.width,
                    height: target.height
                } : null,
                menuConstructed: !!menu,
                menuVisible: menu ? menu.visible : false,
                menuHasRows: menu ? menu.hasMenuItems : false,
                menuRows: menu ? menu.menuEntries.map(entry => ({
                            text: entry.text,
                            enabled: entry.enabled,
                            isSeparator: entry.isSeparator
                        })) : [],
                menuAnchor: menu ? {
                    x: menu.anchorPosition.x,
                    y: menu.anchorPosition.y
                } : null,
                popupGeometry: nativeWindow ? {
                    width: nativeWindow.width,
                    height: nativeWindow.height
                } : null,
                popupBounds: menu ? {
                    x: menu.panelX,
                    y: menu.panelY,
                    width: menu.popupWidth,
                    height: menu.popupHeight
                } : null,
                targetFocused: target ? target.activeFocus : false,
                queuedOpen: target ? target.trayMenuOpenRequested : false,
                popupFocused: menu ? menu.contentItem.activeFocus : false,
                peerPresent: peerIds.length > 0,
                peerIds,
                peerMenusConstructed: peerIds.filter(id => root.loadedMenu(root.findChild(root.peerTray, "trayItem-" + id)) !== null).length,
                peerMenusWithRows: peerIds.filter(id => root.loadedMenu(root.findChild(root.peerTray, "trayItem-" + id))?.hasMenuItems ?? false).length,
                windowVisible: root.trayWindow ? root.trayWindow.visible : false
            });
        }
        function openTarget(): void {
            const target = root.findChild(root.nativeTray, "trayItem-tray-menu-native-target");
            if (target)
                target.openTrayMenu();
        }
        function cancelTarget(): void {
            const target = root.findChild(root.nativeTray, "trayItem-tray-menu-native-target");
            if (target)
                target.cancelTrayMenuOpen();
        }
    }
}
