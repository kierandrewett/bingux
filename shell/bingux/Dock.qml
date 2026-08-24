import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

PanelWindow {
    id: root

    required property var settings
    property var appGroups: []

    function normaliseAppId(appId) {
        if (!appId || appId.length === 0)
            return "";

        return appId.endsWith(".desktop") ? appId.slice(0, -8) : appId;
    }

    function desktopEntryFor(appId) {
        const exactEntry = DesktopEntries.byId(appId);
        return exactEntry ? exactEntry : DesktopEntries.heuristicLookup(appId);
    }

    function refreshAppGroups() {
        const groups = [];
        const groupIndexes = {
        };
        const addGroup = function addGroup(appId, fallbackId) {
            const normalisedAppId = root.normaliseAppId(appId);
            if (normalisedAppId.length === 0 && fallbackId.length === 0)
                return -1;

            const groupId = normalisedAppId.length > 0 ? normalisedAppId : fallbackId;
            if (groupIndexes[groupId] !== undefined)
                return groupIndexes[groupId];

            const group = {
                "id": groupId,
                "desktopEntry": normalisedAppId.length > 0 ? root.desktopEntryFor(normalisedAppId) : null,
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
            const fallbackId = "toplevel-" + index;
            const groupIndex = addGroup(toplevel.appId, fallbackId);
            groups[groupIndex].windows.push(toplevel);
        }
        root.appGroups = groups;
    }

    function launch(group) {
        if (group.desktopEntry)
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

    exclusiveZone: 0
    implicitHeight: 80
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "bingux-dock"
    Component.onCompleted: root.refreshAppGroups()

    anchors {
        bottom: true
        left: true
        right: true
    }

    Rectangle {
        id: dockSurface

        anchors.centerIn: parent
        width: Math.min(root.width - 24, dockRow.implicitWidth + 20)
        height: 64
        radius: height / 2
        color: "#202632"
        visible: root.appGroups.length > 0

        Row {
            id: dockRow

            anchors.centerIn: parent
            spacing: 4

            Repeater {
                model: root.appGroups

                delegate: Item {
                    id: dockButton

                    required property var modelData
                    property bool menuOpen: false
                    property bool active: {
                        for (let index = 0; index < modelData.windows.length; index++) {
                            if (modelData.windows[index].activated)
                                return true;

                        }
                        return false;
                    }

                    width: 52
                    height: 56

                    Rectangle {
                        radius: 10
                        color: dockButton.active ? "#3a4962" : dockMouse.containsMouse ? "#2b3545" : "transparent"

                        anchors {
                            fill: parent
                            margins: 2
                        }

                        Behavior on color {
                            ColorAnimation {
                                duration: 100
                            }

                        }

                    }

                    IconImage {
                        implicitSize: 36
                        source: dockButton.modelData.desktopEntry ? Quickshell.iconPath(dockButton.modelData.desktopEntry.icon, "application-x-executable") : Quickshell.iconPath("application-x-executable", "application-x-executable")

                        anchors {
                            horizontalCenter: parent.horizontalCenter
                            top: parent.top
                            topMargin: 5
                        }

                    }

                    Row {
                        spacing: 2
                        visible: dockButton.modelData.windows.length > 0

                        anchors {
                            bottom: parent.bottom
                            bottomMargin: 3
                            horizontalCenter: parent.horizontalCenter
                        }

                        Repeater {
                            model: dockButton.modelData.windows.length

                            delegate: Rectangle {
                                width: 4
                                height: 4
                                radius: width / 2
                                color: dockButton.active ? "#f5f7fa" : "#aeb8ca"
                            }

                        }

                    }

                    MouseArea {
                        id: dockMouse

                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.PointingHandCursor
                        hoverEnabled: true
                        onClicked: function(mouse) {
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

                    PopupWindow {
                        id: appMenu

                        visible: dockButton.menuOpen
                        implicitWidth: 240
                        implicitHeight: menuSurface.implicitHeight
                        color: "transparent"
                        grabFocus: true
                        onVisibleChanged: {
                            if (!visible)
                                dockButton.menuOpen = false;

                        }

                        anchor {
                            window: root
                            edges: Edges.Top | Edges.Left
                            gravity: Edges.Top | Edges.Left
                            adjustment: PopupAdjustment.Flip | PopupAdjustment.Slide
                            margins.top: 8
                            onAnchoring: {
                                const position = dockButton.mapToItem(root.contentItem, 0, 0);
                                rect = Qt.rect(position.x, position.y, dockButton.width, dockButton.height);
                            }
                        }

                        Rectangle {
                            id: menuSurface

                            width: appMenu.width
                            height: implicitHeight
                            implicitHeight: menuColumn.implicitHeight + 12
                            radius: 10
                            color: "#202632"

                            Column {
                                id: menuColumn

                                spacing: 2

                                anchors {
                                    fill: parent
                                    margins: 6
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

                                        label: modelData.name
                                        onTriggered: {
                                            modelData.execute();
                                            dockButton.menuOpen = false;
                                        }
                                    }

                                }

                                Rectangle {
                                    width: parent.width
                                    height: visible ? 1 : 0
                                    color: "#39465b"
                                    visible: desktopActions.count > 0 && dockButton.modelData.windows.length > 0
                                }

                                Repeater {
                                    model: dockButton.modelData.windows

                                    delegate: MenuAction {
                                        required property var modelData

                                        label: modelData.title || dockButton.modelData.id
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

    Connections {
        function onObjectInsertedPost() {
            root.refreshAppGroups();
        }

        function onObjectRemovedPost() {
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

        width: parent ? parent.width : 240
        height: visible ? 34 : 0

        Rectangle {
            anchors.fill: parent
            radius: 6
            color: actionMouse.containsMouse ? "#344158" : "transparent"
        }

        Text {
            color: "#edf1f7"
            elide: Text.ElideRight
            font.pixelSize: 13
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
            cursorShape: Qt.PointingHandCursor
            onClicked: action.triggered()
        }

    }

}
