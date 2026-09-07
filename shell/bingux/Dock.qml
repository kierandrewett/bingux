import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtCore
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

PanelWindow {
    id: root

    required property var settings
    property var appGroups: []
    property string draggedId: ""
    property real dragOffset: 0
    property int dropIndex: -1
    Settings {
        id: dockState
        location: "file://" + (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/gnoblin/dock.ini"
        category: "dock"
        property var order: []
    }
    function moveGroup(id, destination) {
        const ids = appGroups.map(group => group.id);
        const source = ids.indexOf(id);
        if (source < 0 || destination < 0 || destination >= ids.length) return;
        ids.splice(source, 1); ids.splice(destination, 0, id);
        dockState.order = ids.concat(dockState.order.filter(old => ids.indexOf(old) < 0));
        dockState.sync();
        refreshAppGroups();
    }
    property var emptyAppIdGroupAssociations: []
    property string pendingLaunchGroupId: ""
    property var pendingLaunchToplevel: null

    function normaliseAppId(appId) {
        if (!appId || appId.length === 0)
            return "";

        return appId.endsWith(".desktop") ? appId.slice(0, -8) : appId;
    }

    function desktopEntryFor(appId) {
        const exactEntry = DesktopEntries.byId(appId);
        if (exactEntry)
            return exactEntry;

        const suffixedEntry = DesktopEntries.byId(appId + ".desktop");
        if (suffixedEntry)
            return suffixedEntry;

        return DesktopEntries.heuristicLookup(appId);
    }
    function menuLabel(value, fallback) {
        const text = typeof value === "string" && value.length > 0 ? value : fallback;
        return text.slice(0, 256);
    }

    function associatedGroupIdFor(toplevel) {
        const associations = root.emptyAppIdGroupAssociations;
        for (let index = 0; index < associations.length; index++) {
            const association = associations[index];
            if (association.toplevel === toplevel)
                return association.groupId;

        }
        return "";
    }

    function toplevelGroupId(toplevel) {
        const appId = toplevel && typeof toplevel.appId === "string" ? toplevel.appId : "";
        const normalisedAppId = root.normaliseAppId(appId);

        const desktopEntry = root.desktopEntryFor(normalisedAppId);
        const desktopEntryIdentity = desktopEntry ? root.normaliseAppId(desktopEntry.startupClass || desktopEntry.id) : "";
        return desktopEntryIdentity.length > 0 ? desktopEntryIdentity : normalisedAppId;
    }

    function associatePendingLaunchToplevel(toplevel) {
        const toplevelGroupId = root.toplevelGroupId(toplevel);
        if (!toplevel || root.pendingLaunchGroupId.length === 0)
            return;

        if (toplevelGroupId.length > 0) {
            if (toplevelGroupId !== root.pendingLaunchGroupId)
                return;

            if (root.pendingLaunchToplevel) {
                root.removeToplevelAssociation(root.pendingLaunchToplevel);
                root.pendingLaunchToplevel = null;
            }
            root.pendingLaunchGroupId = "";
            pendingLaunchTimer.stop();
            return;
        }

        if (root.pendingLaunchToplevel || root.associatedGroupIdFor(toplevel).length > 0)
            return;

        const groupId = root.pendingLaunchGroupId;
        const associations = root.emptyAppIdGroupAssociations.slice();
        associations.push({
            "toplevel": toplevel,
            "groupId": groupId
        });
        root.emptyAppIdGroupAssociations = associations;
        root.pendingLaunchToplevel = toplevel;
    }

    function removeToplevelAssociation(toplevel) {
        const associations = [];
        const currentAssociations = root.emptyAppIdGroupAssociations;
        for (let index = 0; index < currentAssociations.length; index++) {
            if (currentAssociations[index].toplevel !== toplevel)
                associations.push(currentAssociations[index]);

        }
        root.emptyAppIdGroupAssociations = associations;
    }

    function refreshAppGroups() {
        const groups = [];
        const groupIndexes = {
        };
        const addGroup = function addGroup(appId, fallbackId) {
            const normalisedAppId = root.normaliseAppId(appId);
            if (normalisedAppId.length === 0 && fallbackId.length === 0)
                return -1;

            const desktopEntry = normalisedAppId.length > 0 ? root.desktopEntryFor(normalisedAppId) : null;
            const desktopEntryIdentity = desktopEntry ? root.normaliseAppId(desktopEntry.startupClass || desktopEntry.id) : "";
            const groupId = desktopEntryIdentity.length > 0 ? desktopEntryIdentity : normalisedAppId.length > 0 ? normalisedAppId : fallbackId;
            if (groupIndexes[groupId] !== undefined) {
                const existingGroup = groups[groupIndexes[groupId]];
                if (!existingGroup.desktopEntry && desktopEntry)
                    existingGroup.desktopEntry = desktopEntry;
                return groupIndexes[groupId];
            }

            const group = {
                "id": groupId,
                "desktopEntry": desktopEntry,
                "windows": []
            };
            groupIndexes[groupId] = groups.length;
            groups.push(group);
            return groupIndexes[groupId];
        };
        const toplevels = ToplevelManager.toplevels.values;
        for (let index = 0; index < root.settings.pinnedApps.length; index++) {
            addGroup(root.settings.pinnedApps[index], "");
        }
        for (let index = 0; index < toplevels.length; index++) {
            const toplevel = toplevels[index];
            const hasAppId = typeof toplevel.appId === "string" && toplevel.appId.length > 0;
            const associatedGroupId = root.associatedGroupIdFor(toplevel);
            const fallbackId = !hasAppId && associatedGroupId.length > 0 ? associatedGroupId : "toplevel-" + index;
            const groupIndex = addGroup(toplevel.appId, fallbackId);
            groups[groupIndex].windows.push(toplevel);
        }
        const order = dockState.order;
        groups.sort((a, b) => {
            const ai = order.indexOf(a.id), bi = order.indexOf(b.id);
            return (ai < 0 ? order.length : ai) - (bi < 0 ? order.length : bi);
        });
        root.appGroups = groups;
    }

    function launch(group) {
        if (!group.desktopEntry)
            return ;

        if (root.pendingLaunchToplevel) {
            root.removeToplevelAssociation(root.pendingLaunchToplevel);
            root.pendingLaunchToplevel = null;
        }
        root.pendingLaunchGroupId = group.id;
        pendingLaunchTimer.restart();
        group.desktopEntry.execute();
    }

    function activeWindow(group) {
        for (let index = 0; index < group.windows.length; index++) {
            if (group.windows[index].activated)
                return group.windows[index];

        }
        return null;
    }

    function toggleGroup(group) {
        if (group.windows.length === 0) {
            root.launch(group);
            return ;
        }
        const active = root.activeWindow(group);
        if (active) {
            active.minimized = true;
            return ;
        }
        group.windows[0].minimized = false;
        group.windows[0].activate();
    }

    function cycleGroup(group, delta) {
        if (group.windows.length === 0) {
            root.launch(group);
            return ;
        }
        const activeIndex = group.windows.indexOf(ToplevelManager.activeToplevel);
        const startIndex = activeIndex >= 0 ? activeIndex : 0;
        const direction = delta > 0 ? -1 : 1;
        const nextIndex = (startIndex + direction + group.windows.length) % group.windows.length;
        group.windows[nextIndex].activate();
    }

    exclusiveZone: implicitHeight
    implicitHeight: Theme.dockHeight + Theme.padding * 2
    mask: Region { item: dockSurface }
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "bingux-dock"
    Component.onCompleted: root.refreshAppGroups()
    Timer {
        id: pendingLaunchTimer

        interval: 3000
        repeat: false
        onTriggered: {
            if (root.pendingLaunchToplevel) {
                root.removeToplevelAssociation(root.pendingLaunchToplevel);
                root.pendingLaunchToplevel = null;
            }
            root.pendingLaunchGroupId = "";
            root.refreshAppGroups();
        }
    }

    anchors {
        bottom: true
        left: true
        right: true
    }

    Rectangle {
        id: dockSurface

        anchors.centerIn: parent
        width: Math.min(root.width - Theme.padding * 2, dockRow.implicitWidth + Theme.gap * 2)
        height: Theme.dockHeight
        radius: Theme.cardRadius + 4
        color: Theme.surface
        border.width: 1
        border.color: Theme.outline
        visible: root.appGroups.length > 0

        Flickable {
            anchors.fill: parent
            anchors.leftMargin: Theme.gap
            anchors.rightMargin: Theme.gap
            contentWidth: dockRow.implicitWidth
            contentHeight: height
            clip: true
            interactive: root.draggedId.length === 0 && contentWidth > width
            boundsBehavior: Flickable.StopAtBounds
        RowLayout {
            id: dockRow
            height: parent.height
            spacing: Theme.spaceSmall

            Repeater {
                model: root.appGroups

                delegate: Item {
                    id: dockButton

                    required property var modelData
                    required property int index
                    property bool menuOpen: false
                    property bool active: {
                        for (let index = 0; index < modelData.windows.length; index++) {
                            if (modelData.windows[index].activated)
                                return true;

                        }
                        return false;
                    }

                    Layout.preferredWidth: Theme.dockItemSize
                    Layout.preferredHeight: Theme.dockItemSize
                    z: root.draggedId === modelData.id ? 2 : 0
                    transform: Translate { x: root.draggedId === dockButton.modelData.id ? root.dragOffset : 0 }
                    opacity: root.draggedId.length > 0 && root.draggedId !== modelData.id ? 0.65 : 1
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.desktopEntry ? modelData.desktopEntry.name : modelData.id
                    Keys.onReturnPressed: root.toggleGroup(modelData)
                    Keys.onSpacePressed: root.toggleGroup(modelData)
                    Keys.onLeftPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index - 1) }
                    Keys.onRightPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index + 1) }
                    Timer {
                        id: tooltipDelay
                        interval: 500
                        running: dockMouse.containsMouse && root.draggedId.length === 0 && !dockButton.menuOpen
                        onTriggered: {
                            tooltip.centreX = dockButton.mapToItem(root.contentItem, dockButton.width / 2, 0).x;
                            tooltip.visible = true;
                        }
                    }
                    DockTooltip {
                        id: tooltip
                        screen: root.screen
                        text: dockButton.Accessible.name
                    }

                    Rectangle {
                        radius: Theme.cardRadius + 4 - Theme.gap
                        color: dockButton.active ? Theme.elevated : dockMouse.containsMouse || dockButton.activeFocus ? Theme.hover : "transparent"

                        anchors {
                            fill: parent
                            margins: 0
                        }

                        Behavior on color {
                            ColorAnimation {
                                duration: Theme.motion
                            }

                        }

                    }

                    IconImage {
                        implicitSize: Theme.dockIconSize
                        source: dockButton.modelData.desktopEntry ? Quickshell.iconPath(dockButton.modelData.desktopEntry.icon, "application-x-executable") : Quickshell.iconPath("application-x-executable", "application-x-executable")

                        anchors {
                            centerIn: parent
                        }

                    }

                    Row {
                        spacing: 2
                        visible: dockButton.modelData.windows.length > 0

                        anchors {
                            bottom: parent.bottom
                            bottomMargin: 0
                            horizontalCenter: parent.horizontalCenter
                        }

                        Repeater {
                            model: Math.min(4, dockButton.modelData.windows.length)

                            delegate: Rectangle {
                                width: dockButton.active ? 12 : 6
                                height: 6
                                radius: height / 2
                                color: dockButton.active ? Theme.accent : Theme.muted
                            }

                        }

                    }

                    Rectangle {
                        visible: root.dropIndex === dockButton.index && root.draggedId !== dockButton.modelData.id
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: 3
                        height: Theme.dockIconSize
                        radius: 2
                        color: Theme.accent
                    }
                    MouseArea {
                        id: dockMouse

                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.ArrowCursor
                        hoverEnabled: true
                        onExited: tooltip.visible = false
                        property real pressX: 0
                        property bool moved: false
                        onPressed: function(mouse) { pressX = mouse.x; moved = false; tooltip.visible = false }
                        onPositionChanged: function(mouse) {
                            if (!(pressedButtons & Qt.LeftButton)) return;
                            const offset = mouse.x - pressX + (root.draggedId === dockButton.modelData.id ? root.dragOffset : 0);
                            if (!moved && Math.abs(offset) < 8) return;
                            moved = true;
                            root.draggedId = dockButton.modelData.id;
                            root.dragOffset = offset;
                            root.dropIndex = Math.max(0, Math.min(root.appGroups.length - 1, dockButton.index + Math.round(offset / (Theme.dockItemSize + Theme.spaceSmall))));
                        }
                        onReleased: {
                            const id = root.draggedId, target = root.dropIndex;
                            root.draggedId = ""; root.dragOffset = 0; root.dropIndex = -1;
                            if (moved) root.moveGroup(id, target);
                        }
                        onCanceled: { root.draggedId = ""; root.dragOffset = 0; root.dropIndex = -1 }
                        onClicked: function(mouse) {
                            if (moved) return;
                            if (mouse.button === Qt.LeftButton)
                                root.toggleGroup(dockButton.modelData);
                            else if (mouse.button === Qt.MiddleButton)
                                root.launch(dockButton.modelData);
                            else if (mouse.button === Qt.RightButton)
                                dockButton.menuOpen = !dockButton.menuOpen;
                        }
                        onWheel: function(wheel) {
                            root.cycleGroup(dockButton.modelData, wheel.angleDelta.y);
                        }
                    }

                    ShellPopup {
                        id: appMenu
                        visible: dockButton.menuOpen
                        popupWidth: 280
                        popupHeight: menuColumn.implicitHeight + Theme.padding * 2
                        screen: root.screen
                        preferredY: height - root.height - popupHeight - Theme.gap
                        onVisibleChanged: {
                            if (!visible) dockButton.menuOpen = false;
                            else preferredX = dockButton.mapToItem(root.contentItem, 0, 0).x - popupWidth / 2 + dockButton.width / 2;
                        }

                        Rectangle {
                            id: menuSurface

                            width: parent.width
                            height: implicitHeight
                            implicitHeight: menuColumn.implicitHeight + 12
                            radius: 10
                            color: "transparent"

                            ColumnLayout {
                                id: menuColumn

                                spacing: 2

                                anchors {
                                    left: parent.left
                                    right: parent.right
                                }

                                MenuAction {
                                    label: "Open new window"
                                    visible: dockButton.modelData.desktopEntry !== null
                                    onTriggered: {
                                        root.launch(dockButton.modelData);
                                        dockButton.menuOpen = false;
                                    }
                                }

                                Repeater {
                                    id: desktopActions

                                    model: dockButton.modelData.desktopEntry ? dockButton.modelData.desktopEntry.actions : []

                                    delegate: MenuAction {
                                        required property var modelData

                                        label: root.menuLabel(modelData.name, "Application action")
                                        onTriggered: {
                                            modelData.execute();
                                            dockButton.menuOpen = false;
                                        }
                                    }

                                }

                                Rectangle {
                                    width: parent.width
                                    height: visible ? 1 : 0
                                    color: Theme.outline
                                    visible: desktopActions.count > 0 && dockButton.modelData.windows.length > 0
                                }

                                Repeater {
                                    model: dockButton.modelData.windows

                                    delegate: MenuAction {
                                        required property var modelData

                                        label: root.menuLabel(modelData.title, dockButton.modelData.id)
                                        onTriggered: {
                                            modelData.activate();
                                            dockButton.menuOpen = false;
                                        }
                                    }

                                }

                            }

                        }

                    }

                }

            }

        }
        }

    }

    Connections {
        function onObjectInsertedPost(object, index) {
            root.associatePendingLaunchToplevel(object);
            root.refreshAppGroups();
        }

        function onObjectRemovedPost(object, index) {
            root.removeToplevelAssociation(object);
            if (root.pendingLaunchToplevel === object) {
                root.pendingLaunchToplevel = null;
                root.pendingLaunchGroupId = "";
                pendingLaunchTimer.stop();
            }
            root.refreshAppGroups();
        }

        target: ToplevelManager.toplevels
    }

    Connections {
        function onApplicationsChanged() {
            root.refreshAppGroups();
        }

        target: DesktopEntries
    }

    component MenuAction: Item {
        id: action

        required property string label

        signal triggered()

        Layout.fillWidth: true
        implicitHeight: visible ? 38 : 0

        Rectangle {
            anchors.fill: parent
            radius: 6
            color: actionMouse.containsMouse ? Theme.hover : "transparent"
        }

        Text {
            color: Theme.text
            elide: Text.ElideRight
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
            textFormat: Text.PlainText
            text: action.label

            anchors {
                left: parent.left
                right: parent.right
                leftMargin: 10
                rightMargin: 10
                verticalCenter: parent.verticalCenter
            }

        }

        MouseArea {
            id: actionMouse

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.ArrowCursor
            onClicked: action.triggered()
        }

    }

}
