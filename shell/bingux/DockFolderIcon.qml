pragma ComponentBehavior: Bound
import QtQuick
import "DockFolders.js" as DockFolders

// A closed dock folder: a tile in the folder's colour holding a grid of its
// apps. Up to four apps show 2 by 2, more show 3 by 3, and when the grid is
// full its last cell says how many more there are.
Item {
    id: root
    property var folder: null
    property var dock: null
    property real iconSize: 56
    property bool highlighted: false
    // True while the folder is open: its popup shows the apps instead.
    property bool contentsHidden: false
    property string activeAppId: ""
    readonly property var apps: folder?.apps || []
    readonly property var grid: DockFolders.preview(apps.length, iconSize)
    readonly property real inset: iconSize * 0.1
    readonly property real gap: Math.max(2, iconSize * 0.05)
    readonly property real cellSize: (iconSize - inset * 2 - gap * (grid.columns - 1)) / grid.columns
    readonly property int cellCount: grid.shown + (grid.more > 0 ? 1 : 0)
    readonly property bool activeOutsidePreview: activeAppId.length > 0 && apps.indexOf(activeAppId) >= grid.shown
    readonly property color folderColor: Theme.accentColorFor(folder?.color || Theme.gnomeAccentName)
    width: iconSize
    height: iconSize

    // Where the mini icon of app `index` sits, in this item's coordinates.
    // The open folder's popup flies its apps out of these places.
    function miniGeometry(index) {
        // A grid with spare rows (two apps in a 2 by 2) sits in the middle.
        const spare = grid.columns - Math.ceil(cellCount / grid.columns);
        return {
            x: inset + (index % grid.columns) * (cellSize + gap),
            y: inset + (Math.floor(index / grid.columns) + spare / 2) * (cellSize + gap),
            size: cellSize
        };
    }

    Rectangle {
        id: tile
        anchors.fill: parent
        radius: Math.max(12, root.iconSize * 0.25)
        color: Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, root.contentsHidden ? 0.38 : 0.28), Theme.popupSurface)
        property real pulse: 1
        opacity: root.highlighted ? pulse : 1
        border.width: 1
        border.color: root.highlighted ? Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.48), Theme.popupSurface) : Theme.surfaceColor(Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.32), Theme.popupSurface)
        Behavior on color {
            ColorAnimation {
                duration: Theme.reducedMotion ? 0 : 170
            }
        }
        SequentialAnimation on pulse {
            running: root.highlighted && !Theme.reducedMotion
            loops: Animation.Infinite
            NumberAnimation {
                to: 0.68
                duration: 170
            }
            NumberAnimation {
                to: 1
                duration: 170
            }
        }
    }

    Repeater {
        model: root.contentsHidden ? 0 : root.cellCount
        Item {
            id: cell
            required property int index
            readonly property var place: root.miniGeometry(index)
            readonly property bool overflow: root.grid.more > 0 && index === root.grid.shown
            readonly property string appId: overflow ? "" : root.apps[index] || ""
            readonly property bool active: appId.length > 0 && appId === root.activeAppId
            x: place.x
            y: place.y
            width: place.size
            height: place.size
            Rectangle {
                anchors.fill: parent
                anchors.margins: -1
                radius: Math.max(3, parent.width * 0.24)
                color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.26), Theme.popupSurface)
                visible: cell.active
            }
            OsIconImage {
                anchors.fill: parent
                visible: !cell.overflow
                source: root.dock?.folderIcon(cell.appId) || "application-x-executable"
                implicitSize: cell.width
            }
            Rectangle {
                anchors.fill: parent
                visible: cell.overflow
                radius: Math.max(3, width * 0.24)
                color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.18), Theme.popupSurface)
                Text {
                    anchors.centerIn: parent
                    text: "+" + Math.min(99, root.grid.more)
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.max(7, parent.width * 0.5)
                    font.weight: Font.DemiBold
                }
            }
        }
    }

    // The app in focus when the folder's preview does not show it.
    Rectangle {
        visible: root.activeOutsidePreview && !root.contentsHidden
        x: root.width - width + 4
        y: -4
        width: 23
        height: 23
        radius: 8
        color: Theme.popupSurface
        border.width: 1
        border.color: root.folderColor
        OsIconImage {
            anchors.centerIn: parent
            width: 17
            height: 17
            implicitSize: 17
            source: root.dock?.folderIcon(root.activeAppId) || "application-x-executable"
        }
    }
}
