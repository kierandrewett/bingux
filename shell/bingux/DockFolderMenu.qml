import QtQuick

// The menu of a folder tile in the dock.
DockPointMenu {
    id: root

    required property var dock
    property string folderId: ""

    function openFor(id, x, y) {
        const folder = dock.folderById(id);
        if (!folder)
            return;
        folderId = id;
        requestedPosition = Qt.point(x, y);
        visible = false;
        actions = [row("Open", () => root.dock.openFolder(root.folderId)), row("Rename", () => root.dock.openFolder(root.folderId, "rename")), row("Change colour", () => root.dock.openFolder(root.folderId, "colour")), separator(), row("Ungroup", () => root.dock.ungroupFolder(root.folderId))];
        visible = true;
    }
}
