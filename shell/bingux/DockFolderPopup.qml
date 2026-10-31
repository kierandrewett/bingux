pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "DockFolders.js" as DockFolders

// An open dock folder: its name, its apps in pages of nine, and the colour and
// ungroup options. It opens from the folder's tile, and the apps fly out of the
// tile's preview into the grid.
//
// Pointer: click an app to open or focus it, drag it to reorder it, or drag it
// out of the card to take it out of the folder (onto a dock tile to put it
// there, onto another folder to move it). Click the name to rename it.
// Keyboard: arrows, Home and End move over the apps, Enter opens one, Ctrl and
// an arrow move it, Delete takes it out, F2 renames, Escape closes.
ShellPopup {
    id: root
    required property var dock
    property string folderId: ""
    readonly property var folder: dock.folderById(folderId)
    readonly property var apps: folder?.apps || []
    readonly property color folderColor: Theme.accentColorFor(folder?.color || Theme.gnomeAccentName)

    // Geometry follows the dock's icon size, so a large dock gets large icons
    // and a small one stays compact.
    readonly property int expandedIconSize: Math.max(44, Math.min(64, dock.iconSize))
    readonly property int columns: DockFolders.PAGE_COLUMNS
    readonly property int cellWidth: expandedIconSize + 30
    readonly property int cellHeight: expandedIconSize + 34
    readonly property int sidePadding: 18
    readonly property int gridX: sidePadding
    readonly property int gridY: 72
    readonly property int controlSize: 36
    readonly property int pageCount: DockFolders.pageCount(apps.length)
    property int page: 0
    readonly property int shownCount: Math.max(0, Math.min(DockFolders.PAGE_SIZE, apps.length - page * DockFolders.PAGE_SIZE))
    readonly property int rowCount: pageCount > 1 ? 3 : Math.max(1, Math.ceil(shownCount / columns))
    readonly property int footerY: gridY + rowCount * cellHeight + 8
    readonly property int optionsFullHeight: 92
    property real optionsHeight: optionsOpen ? optionsFullHeight : 0
    Behavior on optionsHeight {
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 190
            easing.type: Easing.OutCubic
        }
    }
    property bool optionsOpen: false
    property bool renaming: false

    // The tile this folder opens from, in screen coordinates.
    property real anchorX: 0
    property real anchorBottom: 0
    property real originX: 0
    property real originY: 0
    property var previewIcon: null

    // Paging while the pointer drags over a card edge.
    property int displayPage: 0
    property int pageTravelDirection: 0
    property real pageShift: 0
    property bool pageTransitioning: false
    property int pageDirection: 0

    // The opening and closing flight of the apps between the tile and the grid.
    property bool flightRunning: false
    property real flightProgress: 0
    property var flightApps: []
    readonly property bool previewHidden: flightRunning || (visible && flightProgress >= 1)

    // The app with keyboard focus, as an index into `apps`, and whether the
    // keyboard (not the pointer) is driving.
    property int focusIndex: 0
    property bool keyboardMode: false

    // A drag that started on one of this folder's apps.
    property bool memberDragging: false
    property int memberDragIndex: -1
    property real memberDragX: 0
    property real memberDragY: 0
    readonly property bool memberDragOutside: memberDragging && !cardContains(memberDragX, memberDragY, 6)
    property int memberDropIndex: -1

    popupWidth: cellWidth * columns + sidePadding * 2
    popupHeight: footerY + (pageCount > 1 ? 36 : 16) + optionsHeight
    contentPadding: 0
    cornerRadius: 28
    outlineColor: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.11), Theme.popupSurface)
    preferredX: anchorX - popupWidth / 2
    preferredY: height - anchorBottom - popupHeight - Theme.gap
    revealOriginY: popupHeight
    initialRevealScale: 1
    openMotion: Theme.reducedMotion ? 0 : 120
    closeMotion: Theme.reducedMotion ? 0 : 170
    keepContentAlive: true
    keyboardInteractive: dock.draggedId.length === 0
    pointerInteractive: dock.draggedId.length === 0
    dismissOnOutsideClick: dock.draggedId.length === 0 && !memberDragging

    onFolderChanged: if (!folder && visible)
        visible = false
    onPageCountChanged: if (page >= pageCount)
        page = pageCount - 1
    onAppsChanged: focusIndex = Math.max(0, Math.min(focusIndex, apps.length - 1))
    onFocusIndexChanged: if (keyboardMode && focusIndex >= 0)
        page = DockFolders.pageOf(focusIndex)
    onPageChanged: {
        if (!visible || Theme.reducedMotion || flightRunning || page === displayPage) {
            pageSlide.stop();
            displayPage = page;
            pageShift = 0;
            pageTransitioning = false;
            return;
        }
        pageSlide.stop();
        pageTravelDirection = page > displayPage ? 1 : -1;
        pageShift = 0;
        pageTransitioning = true;
        pageSlide.start();
    }
    // Another folder opens in the open popup: start over for that folder.
    onFolderIdChanged: if (visible)
        resetForOpening()
    onVisibleChanged: {
        if (visible) {
            resetForOpening();
        } else {
            cancelMemberDrag();
            endRename(false);
            closeFlight();
            stopDragPaging();
        }
    }

    function cardContains(x, y, margin) {
        return x >= panelX - margin && x <= panelX + popupWidth + margin && y >= panelY - margin && y <= panelY + popupHeight + margin;
    }
    function resetForOpening() {
        page = 0;
        pageSlide.stop();
        displayPage = 0;
        pageShift = 0;
        pageTransitioning = false;
        optionsOpen = false;
        keyboardMode = false;
        focusIndex = 0;
        cancelMemberDrag();
        endRename(false);
        flightAnimation.stop();
        flightRunning = false;
        flightProgress = 0;
        Qt.callLater(() => {
            if (visible) {
                openFlight();
                applyIntent();
            }
        });
    }
    // What the dock asked for when it opened the folder: "rename" selects the
    // name, "colour" shows the colour choices, "keyboard" starts keyboard focus.
    function applyIntent() {
        const intent = dock.folderIntent;
        dock.folderIntent = "";
        if (intent === "rename") {
            startRename();
            return;
        }
        content.forceActiveFocus();
        if (intent === "colour")
            optionsOpen = true;
        else if (intent === "keyboard")
            keyboardMode = true;
    }

    function prepareFlight() {
        const apps = [];
        const count = Math.min(DockFolders.PAGE_SIZE, root.apps.length);
        const grid = previewIcon ? previewIcon.grid : DockFolders.preview(root.apps.length, dock.iconSize);
        for (let index = 0; index < count; index++) {
            const previewed = previewIcon && index < grid.shown;
            const mini = previewed ? previewIcon.miniGeometry(index) : null;
            const fromSize = mini ? mini.size : dock.iconSize * 0.2;
            apps.push({
                appId: root.apps[index],
                fromX: mini ? originX + mini.x : originX + (dock.iconSize - fromSize) / 2,
                fromY: mini ? originY + mini.y : originY + (dock.iconSize - fromSize) / 2,
                fromSize,
                toX: panelX + cellX(index % columns) + (cellWidth - expandedIconSize) / 2,
                toY: panelY + cellY(Math.floor(index / columns)) + 6
            });
        }
        flightApps = apps;
    }
    function openFlight() {
        prepareFlight();
        flightAnimation.stop();
        flightProgress = Theme.reducedMotion ? 1 : 0;
        flightRunning = !Theme.reducedMotion && flightApps.length > 0;
        if (flightRunning) {
            flightAnimation.from = 0;
            flightAnimation.to = 1;
            flightAnimation.restart();
        }
    }
    function closeFlight() {
        if (Theme.reducedMotion || flightApps.length === 0) {
            flightRunning = false;
            return;
        }
        page = 0;
        prepareFlight();
        flightAnimation.stop();
        flightProgress = 1;
        flightRunning = true;
        flightAnimation.from = 1;
        flightAnimation.to = 0;
        flightAnimation.restart();
    }
    function cellX(column) {
        return gridX + column * cellWidth;
    }
    function cellY(row) {
        return gridY + row * cellHeight;
    }

    NumberAnimation {
        id: flightAnimation
        target: root
        property: "flightProgress"
        duration: Theme.reducedMotion ? 0 : 260
        easing.type: root.visible ? Easing.OutCubic : Easing.InOutCubic
        onFinished: root.flightRunning = false
    }
    NumberAnimation {
        id: pageSlide
        target: root
        property: "pageShift"
        from: 0
        to: -root.pageTravelDirection * root.cellWidth * root.columns
        duration: Theme.reducedMotion ? 0 : 260
        easing.type: Easing.OutCubic
        onFinished: {
            root.displayPage = root.page;
            root.pageTransitioning = false;
            root.pageShift = 0;
        }
    }

    // The apps in flight between the tile and the grid.
    Item {
        parent: root.contentItem
        anchors.fill: parent
        z: 20
        visible: root.flightRunning
        Repeater {
            model: root.flightApps
            OsIconImage {
                required property var modelData
                x: modelData.fromX + (modelData.toX - modelData.fromX) * root.flightProgress
                y: modelData.fromY + (modelData.toY - modelData.fromY) * root.flightProgress
                width: modelData.fromSize + (root.expandedIconSize - modelData.fromSize) * root.flightProgress
                height: width
                implicitSize: width
                source: root.dock.folderIcon(modelData.appId)
            }
        }
    }
    // The dock's own drag: an app of the dock carried over this card.
    OsIconImage {
        parent: root.contentItem
        z: 30
        visible: root.visible && root.dock.draggedId.length > 0
        x: root.dock.dragPointerX - width / 2
        y: root.dock.dragPointerY - height / 2
        width: root.dock.iconSize
        height: width
        implicitSize: width
        scale: 1.08
        opacity: root.dock.dragLift
        source: root.dock.draggedId ? root.dock.folderIcon(root.dock.draggedId) : "application-x-executable"
    }
    // This card's own drag: an app of this folder carried over or out of it.
    OsIconImage {
        parent: root.contentItem
        z: 30
        visible: root.memberDragging
        x: root.memberDragX - width / 2
        y: root.memberDragY - height / 2
        width: root.expandedIconSize
        height: width
        implicitSize: width
        scale: 1.08
        opacity: root.memberDragOutside ? 0.8 : 1
        source: root.memberDragIndex >= 0 ? root.dock.folderIcon(root.apps[root.memberDragIndex]) : "application-x-executable"
    }

    // The grid cell under a point of the card, as an index into the folder's
    // apps; used for a drop from the dock and for a drag inside the card.
    function dropIndexAt(x, y) {
        const column = Math.max(0, Math.min(columns - 1, Math.floor((x - gridX) / cellWidth)));
        const row = Math.max(0, Math.min(2, Math.floor((y - gridY) / cellHeight)));
        return Math.min(apps.length, page * DockFolders.PAGE_SIZE + row * columns + column);
    }
    function dragPageAt(x) {
        const direction = x < 34 ? -1 : x > popupWidth - 34 ? 1 : 0;
        if (direction !== pageDirection) {
            pageDirection = direction;
            pageDelay.stop();
            if (direction)
                pageDelay.restart();
        }
    }
    function stopDragPaging() {
        pageDirection = 0;
        pageDelay.stop();
    }
    Timer {
        id: pageDelay
        interval: 550
        repeat: true
        onTriggered: root.page = Math.max(0, Math.min(root.pageCount - 1, root.page + root.pageDirection))
    }

    // Rename in place.
    function startRename() {
        if (!folder)
            return;
        renaming = true;
        nameField.text = folder.name;
        nameField.forceActiveFocus();
        nameField.selectAll();
    }
    function endRename(commit) {
        if (!renaming)
            return;
        renaming = false;
        if (commit)
            dock.renameFolder(folderId, nameField.text);
        content.forceActiveFocus();
    }

    // Opening an app closes the folder, as for any dock tile.
    function activate(index, source) {
        const group = dock.memberGroup(apps[index]);
        if (group)
            dock.toggleGroup(group, source);
        visible = false;
    }
    function moveFocus(key) {
        if (apps.length === 0)
            return;
        keyboardMode = true;
        focusIndex = DockFolders.step(focusIndex, key, apps.length);
    }

    // A drag of one of this folder's apps. The pointer is in screen
    // coordinates, which this card's full-screen window shares.
    function beginMemberDrag(index) {
        memberDragIndex = index;
        memberDragging = true;
        memberDropIndex = index;
        keyboardMode = false;
    }
    function moveMemberDrag(x, y) {
        memberDragX = x;
        memberDragY = y;
        if (cardContains(x, y, 6)) {
            memberDropIndex = dropIndexAt(x - panelX, y - panelY);
            dock.updateMemberDrag(-1, -1);
            dragPageAt(x - panelX);
        } else {
            memberDropIndex = -1;
            dock.updateMemberDrag(x, y);
            stopDragPaging();
        }
    }
    function endMemberDrag(x, y) {
        if (!memberDragging)
            return;
        const appId = apps[memberDragIndex];
        const inside = cardContains(x, y, 6);
        const target = Math.min(apps.length - 1, memberDropIndex);
        memberDragging = false;
        stopDragPaging();
        if (inside) {
            dock.updateMemberDrag(-1, -1);
            if (appId && target >= 0)
                dock.moveInFolder(folderId, appId, target);
        } else if (appId) {
            dock.finishMemberDrag(folderId, appId, x, y);
        }
        memberDragIndex = -1;
        memberDropIndex = -1;
    }
    function cancelMemberDrag() {
        if (!memberDragging)
            return;
        memberDragging = false;
        memberDragIndex = -1;
        memberDropIndex = -1;
        stopDragPaging();
        dock.updateMemberDrag(-1, -1);
    }

    Item {
        id: content
        width: root.popupWidth
        height: root.popupHeight
        focus: true
        Accessible.role: Accessible.Pane
        Accessible.name: (root.folder?.name || "Folder") + ", folder with " + root.apps.length + " apps"

        Keys.onPressed: event => {
            if (root.renaming)
                return;
            const control = (event.modifiers & Qt.ControlModifier) !== 0;
            const keys = {};
            keys[Qt.Key_Left] = "left";
            keys[Qt.Key_Right] = "right";
            keys[Qt.Key_Up] = "up";
            keys[Qt.Key_Down] = "down";
            keys[Qt.Key_Home] = "home";
            keys[Qt.Key_End] = "end";
            const key = keys[event.key];
            if (key) {
                if (control && root.keyboardMode && root.apps.length > 0 && key !== "home" && key !== "end") {
                    // Move the focused app to where the arrow leads.
                    const target = DockFolders.step(root.focusIndex, key, root.apps.length);
                    if (target !== root.focusIndex && root.dock.moveInFolder(root.folderId, root.apps[root.focusIndex], target))
                        root.focusIndex = target;
                } else if (!root.keyboardMode) {
                    root.keyboardMode = true;
                } else {
                    root.moveFocus(key);
                }
                event.accepted = true;
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                if (!root.keyboardMode)
                    root.keyboardMode = true;
                else if (root.apps.length > 0)
                    root.activate(root.focusIndex, content);
                event.accepted = true;
            } else if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
                if (root.keyboardMode && root.apps.length > 0)
                    root.dock.removeFromFolder(root.folderId, root.apps[root.focusIndex]);
                event.accepted = true;
            } else if (event.key === Qt.Key_F2) {
                root.startRename();
                event.accepted = true;
            } else if (event.key === Qt.Key_PageDown || event.key === Qt.Key_PageUp) {
                root.page = Math.max(0, Math.min(root.pageCount - 1, root.page + (event.key === Qt.Key_PageDown ? 1 : -1)));
                event.accepted = true;
            }
        }

        // The name: a label that becomes a field when clicked.
        Item {
            id: nameArea
            x: root.sidePadding
            y: 14
            width: root.popupWidth - root.sidePadding * 2 - root.controlSize - 6
            height: root.controlSize
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Folder name " + (root.folder?.name || "") + ". Rename"
            Accessible.onPressAction: root.startRename()
            Keys.onReturnPressed: root.startRename()
            Keys.onEnterPressed: root.startRename()
            Keys.onSpacePressed: root.startRename()
            Rectangle {
                anchors.fill: parent
                radius: 8
                color: nameField.activeFocus ? Theme.hover : nameMouse.containsMouse ? Theme.hover : "transparent"
                border.width: nameArea.activeFocus && !root.renaming ? 2 : 0
                border.color: Theme.accent
            }
            Text {
                visible: !root.renaming
                anchors.verticalCenter: parent.verticalCenter
                x: 8
                width: parent.width - 16
                text: root.folder?.name || ""
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
            }
            TextField {
                id: nameField
                visible: root.renaming
                anchors.fill: parent
                maximumLength: DockFolders.MAXIMUM_NAME
                leftPadding: 8
                rightPadding: 8
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
                color: Theme.text
                selectByMouse: true
                background: Item {}
                Accessible.name: "Folder name"
                // The parent also starts renaming on Return: keep the key here.
                Keys.onReturnPressed: event => {
                    root.endRename(true);
                    event.accepted = true;
                }
                Keys.onEnterPressed: event => {
                    root.endRename(true);
                    event.accepted = true;
                }
                onActiveFocusChanged: if (!activeFocus)
                    root.endRename(true)
                Keys.onEscapePressed: event => {
                    root.endRename(false);
                    event.accepted = true;
                }
            }
            MouseArea {
                id: nameMouse
                anchors.fill: parent
                enabled: !root.renaming
                hoverEnabled: true
                cursorShape: Qt.IBeamCursor
                onClicked: root.startRename()
            }
        }
        Text {
            x: root.sidePadding + 8
            y: 52
            width: root.popupWidth - root.sidePadding * 2 - 16
            text: root.memberDragOutside ? "Release to take out of the folder" : root.apps.length + " apps"
            elide: Text.ElideRight
            color: root.memberDragOutside ? Theme.text : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        // The colour of the folder, which also opens the colour and ungroup options.
        Item {
            id: optionsButton
            x: root.popupWidth - root.sidePadding - root.controlSize
            y: 14
            width: root.controlSize
            height: root.controlSize
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Folder colour and options"
            Accessible.onPressAction: root.optionsOpen = !root.optionsOpen
            Keys.onReturnPressed: root.optionsOpen = !root.optionsOpen
            Keys.onSpacePressed: root.optionsOpen = !root.optionsOpen
            Rectangle {
                anchors.fill: parent
                radius: 8
                color: optionsMouse.containsMouse ? Theme.hover : "transparent"
                border.width: optionsButton.activeFocus ? 2 : 0
                border.color: Theme.accent
            }
            Rectangle {
                anchors.centerIn: parent
                width: 22
                height: 22
                radius: 11
                color: root.folderColor
                border.width: 1
                border.color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.25), root.folderColor)
            }
            MouseArea {
                id: optionsMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.optionsOpen = !root.optionsOpen
            }
        }

        Item {
            id: grid
            x: root.gridX
            y: root.gridY
            width: root.cellWidth * root.columns
            height: root.rowCount * root.cellHeight
            clip: true
            Repeater {
                model: root.pageTransitioning ? 2 : 1
                Item {
                    id: pageLayer
                    required property int index
                    readonly property int pageNumber: index === 0 ? root.displayPage : root.page
                    x: root.pageShift + (index === 0 ? 0 : root.pageTravelDirection * grid.width)
                    width: grid.width
                    height: grid.height
                    Repeater {
                        model: DockFolders.PAGE_SIZE
                        Item {
                            id: cell
                            required property int index
                            readonly property int appIndex: pageLayer.pageNumber * DockFolders.PAGE_SIZE + index
                            readonly property string appId: root.apps[appIndex] || ""
                            readonly property bool occupied: appId.length > 0
                            readonly property var appState: root.dock.folderAppState(appId)
                            readonly property bool focused: root.keyboardMode && root.focusIndex === appIndex && root.visible
                            readonly property bool dropTarget: (root.dock.folderDropId === root.folderId && root.dock.folderDropIndex === appIndex) || (root.memberDragging && root.memberDropIndex === appIndex && root.memberDragIndex !== appIndex)
                            readonly property bool lifted: root.memberDragging && root.memberDragIndex === appIndex
                            visible: index < root.rowCount * root.columns && occupied
                            x: (index % root.columns) * root.cellWidth
                            y: Math.floor(index / root.columns) * root.cellHeight
                            width: root.cellWidth
                            height: root.cellHeight
                            Accessible.role: Accessible.Button
                            Accessible.name: root.dock.folderName(appId)
                            Accessible.description: cell.appState.running ? (cell.appState.windows === 1 ? "Running, 1 window" : "Running, " + cell.appState.windows + " windows") : "Not running"
                            Accessible.focusable: true
                            Accessible.focused: cell.focused
                            Accessible.onPressAction: root.activate(cell.appIndex, cellMouse)
                            Rectangle {
                                x: (cell.width - width) / 2
                                y: 0
                                width: root.expandedIconSize + 14
                                height: width
                                radius: 17
                                color: cell.dropTarget ? Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.30), Theme.popupSurface) : cell.appState.active ? Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.18), Theme.popupSurface) : cellMouse.containsMouse && !root.memberDragging ? Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.10), Theme.popupSurface) : "transparent"
                                border.width: cell.focused ? 2 : cell.appState.active ? 1 : 0
                                border.color: cell.focused ? Theme.accent : Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.52), Theme.popupSurface)
                                Behavior on color {
                                    ColorAnimation {
                                        duration: Theme.reducedMotion ? 0 : 120
                                    }
                                }
                            }
                            OsIconImage {
                                x: (cell.width - width) / 2
                                y: 7
                                width: root.expandedIconSize
                                height: width
                                implicitSize: width
                                opacity: cell.lifted ? 0.3 : 1
                                visible: cell.occupied && root.visible && !root.flightRunning && root.flightProgress >= 1
                                source: root.dock.folderIcon(cell.appId)
                            }
                            Text {
                                x: 1
                                y: root.expandedIconSize + 18
                                width: cell.width - 2
                                text: cell.occupied ? root.dock.folderName(cell.appId) : ""
                                textFormat: Text.PlainText
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                color: Theme.text
                                opacity: Math.max(0, Math.min(0.9, (root.flightProgress - 0.7) * 3)) * (cell.lifted ? 0.3 : 1)
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                            DockWindowIndicators {
                                x: (cell.width - width) / 2
                                y: root.expandedIconSize + 12
                                windows: cell.occupied ? (root.dock.memberGroup(cell.appId)?.windows || []) : []
                                visible: cell.occupied && root.visible && width > 0
                                opacity: Math.max(0, Math.min(1, (root.flightProgress - 0.75) * 4))
                            }
                            MouseArea {
                                id: cellMouse
                                anchors.fill: parent
                                enabled: cell.occupied
                                hoverEnabled: true
                                preventStealing: true
                                property point pressPoint: Qt.point(0, 0)
                                property bool dragged: false
                                onPressed: mouse => {
                                    pressPoint = Qt.point(mouse.x, mouse.y);
                                    dragged = false;
                                    root.keyboardMode = false;
                                }
                                onPositionChanged: mouse => {
                                    if (!pressed)
                                        return;
                                    if (!dragged && Math.hypot(mouse.x - pressPoint.x, mouse.y - pressPoint.y) < 8)
                                        return;
                                    if (!dragged) {
                                        dragged = true;
                                        root.beginMemberDrag(cell.appIndex);
                                    }
                                    const point = cellMouse.mapToItem(root.contentItem, mouse.x, mouse.y);
                                    root.moveMemberDrag(point.x, point.y);
                                }
                                onReleased: mouse => {
                                    if (dragged) {
                                        const point = cellMouse.mapToItem(root.contentItem, mouse.x, mouse.y);
                                        root.endMemberDrag(point.x, point.y);
                                    }
                                }
                                onCanceled: {
                                    root.cancelMemberDrag();
                                    dragged = false;
                                }
                                onClicked: {
                                    if (!dragged)
                                        root.activate(cell.appIndex, cellMouse);
                                }
                            }
                            // Taking an app out without dragging: shown on the pointer's
                            // app and on the app with keyboard focus.
                            ActionButton {
                                x: cell.width - width - 2
                                y: 0
                                width: 26
                                height: 26
                                visible: cell.occupied && (cellMouse.containsMouse || cell.focused) && !root.memberDragging && !root.dock.draggedId
                                flat: true
                                text: "×"
                                Accessible.name: "Take " + root.dock.folderName(cell.appId) + " out of the folder"
                                onClicked: root.dock.removeFromFolder(root.folderId, cell.appId)
                            }
                        }
                    }
                }
            }
            DragHandler {
                target: null
                enabled: !root.memberDragging
                onActiveChanged: if (!active && Math.abs(translation.x) > 35 && !root.memberDragging)
                    root.page = Math.max(0, Math.min(root.pageCount - 1, root.page + (translation.x < 0 ? 1 : -1)))
            }
            WheelHandler {
                target: null
                onWheel: event => {
                    const delta = event.angleDelta.x || event.angleDelta.y || event.pixelDelta.x || event.pixelDelta.y;
                    if (delta)
                        root.page = Math.max(0, Math.min(root.pageCount - 1, root.page + (delta < 0 ? 1 : -1)));
                    event.accepted = true;
                }
            }
        }
        Row {
            id: dots
            visible: root.pageCount > 1
            anchors.horizontalCenter: parent.horizontalCenter
            y: root.footerY + 6
            spacing: Math.min(9, (root.popupWidth - 56 - root.pageCount * 5) / Math.max(1, root.pageCount - 1))
            Repeater {
                model: root.pageCount
                Item {
                    id: dot
                    required property int index
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: "Folder page " + (index + 1) + " of " + root.pageCount
                    width: 16
                    height: 20
                    Rectangle {
                        anchors.centerIn: parent
                        width: dot.index === root.page ? 7 : 5
                        height: width
                        radius: width / 2
                        color: dot.index === root.page ? Theme.text : Theme.muted
                        opacity: dot.index === root.page ? 1 : 0.65
                        border.width: dot.activeFocus ? 2 : 0
                        border.color: Theme.accent
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.page = dot.index
                    }
                    Keys.onReturnPressed: root.page = index
                    Keys.onSpacePressed: root.page = index
                }
            }
        }

        // The colour and ungroup options, below the grid.
        Item {
            id: options
            x: root.sidePadding
            y: root.footerY + (root.pageCount > 1 ? 30 : 8)
            width: root.popupWidth - root.sidePadding * 2
            height: root.optionsHeight
            clip: true
            opacity: root.optionsHeight / root.optionsFullHeight
            visible: root.optionsHeight > 0
            Row {
                id: swatches
                spacing: 6
                Repeater {
                    model: DockFolders.COLOURS
                    Item {
                        id: swatch
                        required property string modelData
                        readonly property bool current: root.folder?.color === modelData
                        activeFocusOnTab: root.optionsOpen
                        Accessible.role: Accessible.RadioButton
                        Accessible.name: modelData
                        Accessible.checked: current
                        Accessible.onPressAction: root.dock.recolourFolder(root.folderId, modelData)
                        width: 22
                        height: 22
                        Rectangle {
                            anchors.fill: parent
                            radius: 11
                            color: Theme.accentColorFor(swatch.modelData)
                            border.width: swatch.current || swatch.activeFocus ? 2 : 0
                            border.color: swatch.activeFocus ? Theme.accent : Theme.text
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.dock.recolourFolder(root.folderId, swatch.modelData)
                        }
                        Keys.onReturnPressed: root.dock.recolourFolder(root.folderId, modelData)
                        Keys.onSpacePressed: root.dock.recolourFolder(root.folderId, modelData)
                    }
                }
            }
            ActionButton {
                y: 36
                width: 120
                height: 34
                flat: false
                text: "Ungroup"
                Accessible.name: "Ungroup apps"
                onClicked: root.dock.ungroupFolder(root.folderId)
            }
        }
    }
}
