pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    property var folder: null
    property var dock: null
    property real iconSize: 56
    property bool highlighted: false
    property bool contentsHidden: false
    property string activeAppId: ""
    readonly property real miniCellSize: iconSize * 0.30
    readonly property real miniGap: Math.max(3, iconSize * 0.09)
    readonly property real miniIconInset: Math.max(1, iconSize * 0.022)
    readonly property bool activeOutsidePreview: {
        if (!activeAppId) return false;
        for (let index = 0; index < Math.min(4, folder?.apps?.length || 0); index++)
            if (folder.apps[index] === activeAppId) return false;
        return true;
    }
    readonly property color folderColor: Theme.accentColorFor(folder?.color || Theme.gnomeAccentName)
    width: iconSize
    height: iconSize

    Rectangle {
        anchors.fill: parent
        radius: Math.max(12, root.iconSize * 0.25)
        color: Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, root.contentsHidden ? 0.38 : 0.28)
        property real pulse: 1
        opacity: root.highlighted ? pulse : 1
        border.width: 1
        border.color: root.highlighted ? Qt.rgba(1, 1, 1, 0.48) : Qt.rgba(root.folderColor.r, root.folderColor.g, root.folderColor.b, 0.32)
        Behavior on color { ColorAnimation { duration: Theme.reducedMotion ? 0 : 170 } }
        SequentialAnimation on pulse {
            running: root.highlighted && !Theme.reducedMotion
            loops: Animation.Infinite
            NumberAnimation { to: 0.68; duration: 170 }
            NumberAnimation { to: 1; duration: 170 }
        }
    }

    Grid {
        visible: !root.contentsHidden
        anchors.centerIn: parent
        columns: 2
        rows: 2
        spacing: root.miniGap
        Repeater {
            model: 4
            Item {
                required property int index
                width: root.miniCellSize
                height: width
                readonly property string appId: root.folder?.apps?.[index] || ""
                readonly property bool active: appId.length > 0 && appId === root.activeAppId
                Rectangle {
                    anchors.fill: parent
                    radius: Math.max(4, parent.width * 0.22)
                    color: Qt.rgba(1, 1, 1, 0.24)
                    border.width: parent.active ? 1 : 0
                    border.color: Qt.rgba(1, 1, 1, 0.7)
                    visible: parent.active
                }
                OsIconImage {
                    anchors.fill: parent
                    anchors.margins: root.miniIconInset
                    visible: parent.appId.length > 0
                    source: root.dock?.folderIcon(parent.appId) || "application-x-executable"
                    implicitSize: parent.width - root.miniIconInset * 2
                }
            }
        }
    }
    Grid {
        visible: root.contentsHidden
        anchors.centerIn: parent
        columns: 2
        spacing: Math.max(3, root.iconSize * 0.1)
        Repeater {
            model: 4
            Rectangle {
                width: root.iconSize * 0.15
                height: width
                radius: Math.max(3, width * 0.24)
                color: Qt.rgba(0.03, 0.03, 0.05, 0.13)
            }
        }
    }
    Rectangle {
        visible: root.activeAppId.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: -5
        width: 17
        height: 4
        radius: 2
        color: root.folderColor
    }
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
    Rectangle {
        visible: !root.contentsHidden && (root.folder?.apps?.length || 0) > 4
        width: 5
        height: 5
        radius: 3
        color: "white"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 5
    }
}
