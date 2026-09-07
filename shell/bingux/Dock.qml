import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtCore
import QtQml.Models
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

PanelWindow {
    id: root

    required property var settings
    property var appGroups: []
    onAppGroupsChanged: rectangleUpdate.restart()
    onWidthChanged: rectangleUpdate.restart()
    onHeightChanged: rectangleUpdate.restart()
    onVisibleChanged: rectangleUpdate.restart()
    onDragOffsetChanged: rectangleUpdate.restart()
    Timer {
        id: rectangleUpdate
        interval: 16
        onTriggered: {
            for (let i = 0; i < dockItems.count; i++) {
                const item = dockItems.itemAt(i);
                if (item) item.publishRectangle();
            }
        }
    }
    Instantiator {
        model: ToplevelManager.toplevels
        delegate: Connections {
            required property var modelData
            target: modelData
            function onAppIdChanged() { root.refreshAppGroups() }
        }
    }
    property string draggedId: ""
    property real dragOffset: 0
    property int dropIndex: -1
    property var tooltipOwner: null
    readonly property bool tooltipVisible: dockTooltip.visible

    Timer {
        id: tooltipHideDelay
        interval: 120
        onTriggered: {
            if (root.tooltipOwner === null)
                dockTooltip.visible = false;
        }
    }

    DockTooltip {
        id: dockTooltip
        screen: root.screen
    }

    function tooltipEntered(owner) {
        tooltipHideDelay.stop();
    }

    function showTooltip(owner) {
        if (!owner || owner.menuOpen || root.draggedId.length > 0)
            return;

        root.tooltipOwner = owner;
        dockTooltip.text = owner.Accessible.name;
        dockTooltip.centreX = owner.mapToItem(root.contentItem, owner.width / 2, 0).x;
        dockTooltip.visible = true;
    }

    function leaveTooltip(owner) {
        if (root.tooltipOwner !== owner)
            return;

        root.tooltipOwner = null;
        tooltipHideDelay.restart();
    }

    function dismissTooltip() {
        tooltipHideDelay.stop();
        root.tooltipOwner = null;
        dockTooltip.visible = false;
    }

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

    function groupIndex(id) {
        for (let index = 0; index < root.appGroups.length; index++) {
            if (root.appGroups[index].id === id)
                return index;
        }
        return -1;
    }

    function liveReorderShift(index, id) {
        const movingIndex = root.groupIndex(root.draggedId);
        if (movingIndex < 0 || root.dropIndex < 0 || id === root.draggedId)
            return 0;

        const slot = Theme.dockItemSize + Theme.spaceSmall;
        if (movingIndex < root.dropIndex && index > movingIndex && index <= root.dropIndex)
            return -slot;
        if (movingIndex > root.dropIndex && index >= root.dropIndex && index < movingIndex)
            return slot;
        return 0;
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
    mask: Region { item: root.draggedId.length > 0 ? dragCapture : dockSurface }
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "bingux-dock"
    Component.onCompleted: root.refreshAppGroups()

    Item {
        id: dragCapture
        anchors.fill: parent
        visible: root.draggedId.length > 0
        z: 100

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            onReleased: root.finishDrag()
            onCanceled: root.cancelDrag()
        }
    }

    function finishDrag() {
        const id = root.draggedId;
        const target = root.dropIndex;
        if (id.length > 0 && target >= 0 && target < root.appGroups.length)
            root.moveGroup(id, target);
        root.draggedId = "";
        root.dragOffset = 0;
        root.dropIndex = -1;
    }

    function cancelDrag() {
        root.draggedId = "";
        root.dragOffset = 0;
        root.dropIndex = -1;
    }

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
        readonly property real itemPadding: Math.max(0, (height - Theme.dockItemSize) / 2)
        onXChanged: rectangleUpdate.restart()
        onYChanged: rectangleUpdate.restart()

        anchors.centerIn: parent
        width: Math.min(root.width - Theme.padding * 2, dockRow.implicitWidth + dockSurface.itemPadding * 2)
        height: Theme.dockHeight
        radius: Theme.cardRadius + 4
        color: Theme.surface
        border.width: 1
        border.color: Theme.outline
        visible: root.appGroups.length > 0

        Flickable {
            anchors.fill: parent
            anchors.leftMargin: dockSurface.itemPadding
            anchors.rightMargin: dockSurface.itemPadding
            onContentXChanged: rectangleUpdate.restart()
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
                id: dockItems
                model: root.appGroups

                delegate: Item {
                    id: dockButton

                    required property var modelData
                    required property int index
                    property alias menuOpen: appMenu.visible
                    function publishRectangle() {
                        const position = dockIcon.mapToItem(root.contentItem, 0, 0);
                        const rect = root.visible
                            ? Qt.rect(Math.round(position.x), Math.round(position.y), dockIcon.width, dockIcon.height)
                            : Qt.rect(0, 0, 0, 0);
                        for (const window of modelData.windows) window.setRectangle(root, rect);
                    }
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
                    transform: [
                        Translate {
                            x: root.draggedId === dockButton.modelData.id ? root.dragOffset : 0
                        },
                        Translate {
                            id: reorderTransform
                            x: root.liveReorderShift(dockButton.index, dockButton.modelData.id)
                            Behavior on x {
                                NumberAnimation {
                                    duration: 180
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }
                    ]
                    scale: root.draggedId === dockButton.modelData.id ? 1.06 : 1
                    Behavior on scale {
                        NumberAnimation {
                            duration: 140
                            easing.type: Easing.OutCubic
                        }
                    }
                    opacity: root.draggedId.length > 0 && root.draggedId !== modelData.id ? 0.65 : 1
                    Behavior on x {
                        NumberAnimation {
                            duration: Theme.motion
                            easing.type: Easing.OutCubic
                        }
                    }
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.desktopEntry ? modelData.desktopEntry.name : modelData.id
                    Keys.onReturnPressed: root.toggleGroup(modelData)
                    Keys.onSpacePressed: root.toggleGroup(modelData)
                    Keys.onLeftPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index - 1) }
                    Keys.onRightPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index + 1) }
                    Timer {
                        id: tooltipDelay
                        interval: root.tooltipVisible ? 0 : 500
                        running: dockMouse.containsMouse && root.draggedId.length === 0 && !dockButton.menuOpen
                        onTriggered: {
                            root.showTooltip(dockButton);
                        }
                    }

                    Rectangle {
                        radius: Theme.insetRadius(dockSurface.radius, dockSurface.itemPadding)
                        color: dockMouse.pressed ? Theme.pressed : dockButton.active ? Theme.elevated : dockMouse.containsMouse || dockButton.activeFocus ? Theme.hover : "transparent"

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
                        id: dockIcon
                        implicitSize: Theme.dockIconSize
                        source: dockButton.modelData.desktopEntry ? Quickshell.iconPath(dockButton.modelData.desktopEntry.icon, "application-x-executable") : Quickshell.iconPath("application-x-executable", "application-x-executable")

                        anchors {
                            centerIn: parent
                        }

                    }

                    ListView {
                        id: windowIndicators
                        implicitWidth: contentWidth
                        width: contentWidth
                        height: 8
                        implicitHeight: 8
                        visible: dockButton.modelData.windows.length > 0
                        orientation: ListView.Horizontal
                        spacing: 2
                        interactive: false
                        clip: false
                        boundsBehavior: Flickable.StopAtBounds

                        anchors {
                            bottom: parent.bottom
                            bottomMargin: 0
                            horizontalCenter: parent.horizontalCenter
                        }

                        model: Math.min(4, dockButton.modelData.windows.length)
                        add: Transition {
                            ParallelAnimation {
                                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 180; easing.type: Easing.OutCubic }
                                NumberAnimation { property: "scale"; from: 0.55; to: 1; duration: 180; easing.type: Easing.OutBack }
                            }
                        }
                        remove: Transition {
                            ParallelAnimation {
                                NumberAnimation { property: "opacity"; to: 0; duration: 140; easing.type: Easing.InCubic }
                                NumberAnimation { property: "scale"; to: 0.55; duration: 140; easing.type: Easing.InCubic }
                            }
                        }
                        displaced: Transition {
                            NumberAnimation { properties: "x"; duration: 180; easing.type: Easing.OutCubic }
                        }

                        delegate: Rectangle {
                            required property int index
                            readonly property var representedWindow: dockButton.modelData.windows[index]
                            readonly property bool windowActive: representedWindow !== null && representedWindow.activated
                            width: windowActive ? 12 : 6
                                height: 6
                                radius: height / 2
                                scale: windowActive ? 1.08 : 1
                                opacity: 1
                                color: windowActive ? Theme.accent : Theme.muted
                                Behavior on width {
                                    NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                                }
                                Behavior on scale {
                                    NumberAnimation { duration: 180; easing.type: Easing.OutBack }
                                }
                                Behavior on color {
                                    ColorAnimation { duration: Theme.motion }
                                }
                            }

                    }

                    MouseArea {
                        id: dockMouse

                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.ArrowCursor
                        hoverEnabled: true
                        onEntered: root.tooltipEntered(dockButton)
                        onExited: {
                            tooltipDelay.stop();
                            root.leaveTooltip(dockButton);
                        }
                        property real pressX: 0
                        property bool moved: false
                        onPressed: function(mouse) { pressX = mouse.x; moved = false; root.dismissTooltip() }
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
                            if (moved)
                                root.finishDrag();
                            else
                                root.cancelDrag();
                        }
                        onCanceled: root.cancelDrag()
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
                        popupWidth: 280
                        contentPadding: Theme.gap
                        popupHeight: menuColumn.implicitHeight + contentPadding * 2
                        screen: root.screen
                        preferredY: height - root.height - popupHeight - Theme.gap
                        onVisibleChanged: {
                            if (visible) {
                                preferredX = dockButton.mapToItem(root.contentItem, 0, 0).x - popupWidth / 2 + dockButton.width / 2;
                                menuColumn.forceActiveFocus();
                            }
                        }

                        Rectangle {
                            id: menuSurface

                            width: parent.width
                            height: implicitHeight
                            implicitHeight: menuColumn.implicitHeight + 12
                            color: "transparent"

                            ColumnLayout {
                                id: menuColumn
                                function moveSelection(delta) {
                                    const actions = Array.from(children).filter(item => item.menuEntry === true && item.visible && item.enabled);
                                    if (!actions.length) return;
                                    let index = actions.findIndex(item => item.activeFocus);
                                    if (index < 0) index = delta > 0 ? -1 : 0;
                                    actions[(index + delta + actions.length) % actions.length].forceActiveFocus();
                                }
                                Keys.onDownPressed: moveSelection(1)
                                Keys.onUpPressed: moveSelection(-1)

                                spacing: 2

                                anchors {
                                    left: parent.left
                                    right: parent.right
                                }

                                MenuAction {
                                    cornerRadius: appMenu.contentRadius
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
                                    cornerRadius: appMenu.contentRadius
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
                                    cornerRadius: appMenu.contentRadius
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

    component MenuAction: ActionButton {
        readonly property bool menuEntry: true
        flat: true
        required property string label
        signal triggered()
        text: label
        Layout.fillWidth: true
        implicitHeight: 38
        onClicked: triggered()
        contentItem: Text {
            text: parent.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            verticalAlignment: Text.AlignVCenter
            leftPadding: Theme.padding
            rightPadding: Theme.padding
        }
    }
}
