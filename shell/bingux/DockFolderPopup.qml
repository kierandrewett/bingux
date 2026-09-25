pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ShellPopup {
    id: root
    required property var dock
    property string folderId: ""
    readonly property var folder: dock.folderById(folderId)
    readonly property color folderColor: Theme.accentColorFor(folder?.color || Theme.gnomeAccentName)
    property int page: 0
    readonly property int pageCount: Math.max(1, Math.ceil((folder?.apps?.length || 0) / 9))
    readonly property int shownCount: Math.max(0, Math.min(9, (folder?.apps?.length || 0) - page * 9))
    readonly property int rowCount: pageCount > 1 ? 3 : Math.max(1, Math.ceil(shownCount / 3))
    property int displayPage: 0
    property int pageTravelDirection: 0
    property real pageShift: 0
    property bool pageTransitioning: false
    property real anchorX: 0
    property real anchorBottom: 0
    property real originX: 0
    property real originY: 0
    property int pageDirection: 0
    property bool optionsOpen: false
    property bool flightRunning: false
    property real flightProgress: 0
    property var flightApps: []
    readonly property bool previewHidden: flightRunning || (visible && flightProgress >= 1)
    property real optionsHeight: optionsOpen ? 58 : 0
    Behavior on optionsHeight { NumberAnimation { duration: Theme.reducedMotion ? 0 : 190; easing.type: Easing.OutCubic } }
    readonly property int cellSize: 86
    readonly property int expandedIconSize: 50
    readonly property int gridX: 31
    readonly property int gridY: 86
    readonly property int footerY: gridY + rowCount * cellSize + 8
    popupWidth: 320
    popupHeight: footerY + (pageCount > 1 ? 39 : 20) + optionsHeight
    contentPadding: 0
    cornerRadius: 28
    outlineColor: Qt.rgba(1, 1, 1, 0.11)
    preferredX: anchorX - popupWidth / 2
    preferredY: height - anchorBottom - popupHeight - Theme.gap
    revealOriginY: popupHeight
    initialRevealScale: 1
    openMotion: Theme.reducedMotion ? 0 : 120
    closeMotion: Theme.reducedMotion ? 0 : 170
    keepContentAlive: true
    keyboardInteractive: dock.draggedId.length === 0
    pointerInteractive: dock.draggedId.length === 0
    dismissOnOutsideClick: dock.draggedId.length === 0
    onFolderChanged: if (!folder && visible) visible = false
    onPageCountChanged: if (page >= pageCount) page = pageCount - 1
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
    onFolderIdChanged: if (visible) {
        page = 0;
        pageSlide.stop();
        displayPage = 0;
        pageShift = 0;
        pageTransitioning = false;
        flightAnimation.stop();
        flightRunning = false;
        flightProgress = 0;
        nameField.text = folder?.name || "Folder";
        Qt.callLater(() => { if (visible) openFlight(); });
    }
    function prepareFlight() {
        const apps = [];
        const size = dock.iconSize;
        const cell = size * 0.30;
        const gap = Math.max(3, size * 0.09);
        const inset = Math.max(1, size * 0.022);
        const mini = cell - inset * 2;
        const miniLeft = originX + (size - cell * 2 - gap) / 2 + inset;
        const miniTop = originY + (size - cell * 2 - gap) / 2 + inset;
        const count = Math.min(9, folder?.apps?.length || 0);
        for (let index = 0; index < count; index++) {
            const previewed = index < 4;
            apps.push({
                appId: folder.apps[index],
                fromX: previewed ? miniLeft + (index % 2) * (cell + gap) : originX + (size - mini) / 2,
                fromY: previewed ? miniTop + Math.floor(index / 2) * (cell + gap) : originY + (size - mini) / 2,
                fromSize: previewed ? mini : mini * 0.4,
                toX: panelX + gridX + (index % 3) * cellSize + (cellSize - expandedIconSize) / 2,
                toY: panelY + gridY + Math.floor(index / 3) * cellSize + 8
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
    Connections {
        target: root
        function onVisibleChanged() {
            if (root.visible) {
                root.page = 0;
                pageSlide.stop();
                root.displayPage = 0;
                root.pageShift = 0;
                root.pageTransitioning = false;
                root.optionsOpen = false;
                flightAnimation.stop();
                root.flightRunning = false;
                root.flightProgress = 0;
                nameField.text = root.folder?.name || "Folder";
                Qt.callLater(() => { if (root.visible) root.openFlight(); });
            } else {
                root.closeFlight();
                root.stopDragPaging();
            }
        }
    }
    NumberAnimation {
        id: flightAnimation
        target: root
        property: "flightProgress"
        duration: 260
        easing.type: root.visible ? Easing.OutCubic : Easing.InOutCubic
        onFinished: root.flightRunning = false
    }
    NumberAnimation {
        id: pageSlide
        target: root
        property: "pageShift"
        from: 0
        to: -root.pageTravelDirection * root.cellSize * 3
        duration: 260
        easing.type: Easing.OutCubic
        onFinished: {
            root.displayPage = root.page;
            root.pageTransitioning = false;
            root.pageShift = 0;
        }
    }
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
    function dropIndexAt(x, y) {
        const column = Math.max(0, Math.min(2, Math.floor((x - gridX) / cellSize)));
        const row = Math.max(0, Math.min(2, Math.floor((y - gridY) / cellSize)));
        return Math.min(folder?.apps?.length || 0, page * 9 + row * 3 + column);
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

    Item {
        id: content
        width: root.popupWidth
        height: root.popupHeight
        focus: true
        Keys.onLeftPressed: root.page = Math.max(0, root.page - 1)
        Keys.onRightPressed: root.page = Math.min(root.pageCount - 1, root.page + 1)

        TextField {
            id: nameField
            x: 24
            y: 17
            width: 205
            height: 36
            text: root.folder?.name || "Folder"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontHeading
            font.weight: Font.DemiBold
            color: Theme.text
            selectByMouse: true
            background: Rectangle {
                radius: 8
                color: nameField.activeFocus ? Theme.hover : "transparent"
            }
            onEditingFinished: {
                const name = text.trim();
                if (root.folder && name && name !== root.folder.name)
                    root.dock.updateFolder(root.folderId, {name});
            }
            Accessible.name: "Folder name"
        }
        Text {
            x: 26
            y: 56
            text: (root.folder?.apps?.length || 0) + " apps" + (root.dock.activeFolderApp(root.folderId) ? "  ·  " + root.dock.folderName(root.dock.activeFolderApp(root.folderId)) + " active" : "")
            width: 270
            elide: Text.ElideRight
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        ActionButton {
            x: 272
            y: 17
            width: 34
            height: 34
            flat: true
            text: "×"
            Accessible.name: "Close folder"
            onClicked: root.visible = false
        }
        Item {
            x: 233
            y: 17
            width: 34
            height: 34
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Folder colour and options"
            Keys.onReturnPressed: root.optionsOpen = !root.optionsOpen
            Keys.onSpacePressed: root.optionsOpen = !root.optionsOpen
            Rectangle {
                anchors.centerIn: parent
                width: 22
                height: 22
                radius: 11
                color: root.folderColor
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.25)
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.optionsOpen = !root.optionsOpen
            }
        }

        Item {
            id: grid
            x: root.gridX
            y: root.gridY
            width: root.cellSize * 3
            height: root.rowCount * root.cellSize
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
                    Text {
                        anchors.centerIn: parent
                        visible: (root.folder?.apps?.length || 0) === 0
                        text: "Drag apps onto this folder"
                        color: Theme.text
                        opacity: 0.78
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                    }
                    Repeater {
                        model: 9
                        Item {
                            id: cell
                            required property int index
                            readonly property int appIndex: pageLayer.pageNumber * 9 + index
                            readonly property string appId: root.folder?.apps?.[appIndex] || ""
                            readonly property bool occupied: appId.length > 0
                            readonly property var appState: root.dock.folderAppState(appId)
                            visible: index < root.rowCount * 3
                            x: (index % 3) * root.cellSize
                            y: Math.floor(index / 3) * root.cellSize
                            width: root.cellSize
                            height: root.cellSize
                        Rectangle {
                            x: (cell.width - width) / 2
                            y: 1
                            width: 64
                            height: 64
                            radius: 17
                                color: cell.appState.active ? Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.18) : root.dock.folderDropId === root.folderId && root.dock.folderDropIndex === cell.appIndex ? Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.26) : cellMouse.containsMouse ? Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.10) : "transparent"
                                border.width: cell.appState.active ? 1 : 0
                                border.color: Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.52)
                                Behavior on color { ColorAnimation { duration: Theme.reducedMotion ? 0 : 120 } }
                            }
                        OsIconImage {
                            x: (cell.width - width) / 2
                            y: 8
                            width: root.expandedIconSize
                            height: width
                            implicitSize: width
                                visible: cell.occupied && root.visible && !root.flightRunning && root.flightProgress >= 1
                                source: root.dock.folderIcon(cell.appId)
                            }
                            Text {
                                x: 1
                                y: 70
                                width: 84
                                text: cell.occupied ? root.dock.folderName(cell.appId) : ""
                                textFormat: Text.PlainText
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                color: Theme.text
                                opacity: Math.max(0, Math.min(0.9, (root.flightProgress - 0.7) * 3))
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                        DockWindowIndicators {
                            x: (cell.width - width) / 2
                            y: 62
                                windows: cell.occupied ? (root.dock.memberGroup(cell.appId)?.windows || []) : []
                                visible: cell.occupied && root.visible && width > 0
                                opacity: Math.max(0, Math.min(1, (root.flightProgress - 0.75) * 4))
                            }
                            MouseArea {
                                id: cellMouse
                                anchors.fill: parent
                                enabled: cell.occupied
                                hoverEnabled: true
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                onClicked: mouse => {
                                    if (mouse.button === Qt.RightButton) {
                                        root.dock.removeFromFolder(root.folderId, cell.appId);
                                    } else {
                                        const group = root.dock.memberGroup(cell.appId);
                                        if (group)
                                            root.dock.toggleGroup(group);
                                        root.visible = false;
                                    }
                                }
                            }
                            ActionButton {
                                x: 60
                                y: 0
                                width: 24
                                height: 24
                                visible: cell.occupied && cellMouse.containsMouse && !root.dock.draggedId
                                flat: true
                                text: "×"
                                Accessible.name: "Remove " + root.dock.folderName(cell.appId) + " from folder"
                                onClicked: root.dock.removeFromFolder(root.folderId, cell.appId)
                            }
                        }
                    }
                }
            }
            DragHandler {
                target: null
                onActiveChanged: if (!active && Math.abs(translation.x) > 35)
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
            y: root.footerY + 10
            spacing: Math.min(9, (root.popupWidth - 56 - root.pageCount * 5) / Math.max(1, root.pageCount - 1))
            Repeater {
                model: root.pageCount
                Rectangle {
                    id: dot
                    required property int index
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: "Folder page " + (index + 1)
                    width: index === root.page ? 7 : 5
                    height: width
                    radius: width / 2
                    color: index === root.page ? Theme.text : Theme.muted
                    opacity: index === root.page ? 1 : 0.65
                    MouseArea { anchors.fill: parent; onClicked: root.page = dot.index }
                    Keys.onReturnPressed: root.page = index
                    Keys.onSpacePressed: root.page = index
                }
            }
        }
        ActionButton {
            x: 214
            y: root.footerY + 34
            width: 96
            height: 34
            visible: root.optionsOpen
            flat: true
            text: "Ungroup"
            Accessible.name: "Ungroup apps"
            onClicked: root.dock.deleteFolder(root.folderId)
        }
        Row {
            visible: root.optionsOpen
            x: 20
            y: root.footerY + 46
            spacing: 6
            Repeater {
                model: root.dock.folderColors
                Rectangle {
                    id: colourChoice
                    required property string modelData
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    width: 16
                    height: 16
                    radius: 10
                    color: Theme.accentColorFor(modelData)
                    border.width: root.folder?.color === modelData ? 2 : 0
                    border.color: Theme.text
                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.dock.updateFolder(root.folderId, {color: colourChoice.modelData})
                    }
                    Keys.onReturnPressed: root.dock.updateFolder(root.folderId, {color: modelData})
                    Keys.onSpacePressed: root.dock.updateFolder(root.folderId, {color: modelData})
                    Accessible.name: modelData + " folder colour"
                }
            }
        }
    }
}
