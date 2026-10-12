import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    id: root
    readonly property string generation: Date.now() + ':' + Math.random()
    QtObject {
        id: manager
        property var activeToplevel: null
        property QtObject toplevels: QtObject {
            property var values: []
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    Dock {
        id: dock
        testManager: manager
        settings: ({
                pinnedApps: []
            })
    }
    IpcHandler {
        target: "folder-test"
        function status(): string {
            const p = dock.testFolderPopup;
            const buttons = [];
            for (let i = 0; i < dock.testItems.count; i++) {
                const b = dock.testItems.itemAt(i);
                const point = DesktopEditing.point(b, dock, b.width / 2, b.height / 2);
                buttons.push({
                    id: b.currentGroup.id,
                    launchable: !!b.currentGroup.desktopEntry,
                    x: point.x,
                    y: point.y
                });
            }
            return JSON.stringify({
                generation: root.generation,
                buttons,
                folders: dock.folders,
                dragged: dock.draggedId,
                hover: dock.folderHoverId,
                ready: dock.folderHoverReady,
                drop: dock.folderDropId,
                popup: p ? {
                    identity: String(p),
                    visible: p.visible,
                    retained: p.retained,
                    closing: p.closing,
                    opacity: p.presentationOpacity,
                    page: p.page,
                    pageCount: p.pageCount,
                    flight: p.flightRunning,
                    hidden: p.previewHidden,
                    folderId: p.folderId,
                    sameScreen: p.screen === dock.screen,
                    anchor: p.anchorBottom === dock.dockTopFromBottom,
                    x: p.panelX,
                    y: p.panelY,
                    width: p.popupWidth,
                    height: p.popupHeight
                } : null
            });
        }
        function seed(kind: string): void {
            const ids = [...new Set(DesktopEntries.applications.values.filter(e => !e.noDisplay && e.id && !e.id.includes('steam')).map(e => dock.pinIdentity(e.id)))].slice(0, 14);
            if (ids.length < 12)
                throw new Error('Need 12 desktop entries');
            const folders = kind === 'saved' ? [
                {
                    id: 'folder:fixture',
                    name: 'Fixture',
                    color: 'blue',
                    apps: ids.slice(0, 10)
                }
            ] : [];
            BinguxPreferences.saveDesktop({
                dockApps: {
                    pinnedApps: ids,
                    order: folders.length ? ['folder:fixture', ...ids.slice(10)] : ids
                },
                dockFolders: folders
            });
            dock.refreshAppGroupsNow();
        }
        function invalid(): void {
            dock.openFolder('absent');
            dock.cancelDrag();
        }
        function coldDrop(): void {
            const app = dock.appGroups.find(g => g.desktopEntry && !g.folder && !dock.folders[0].apps.includes(dock.pinIdentity(g.desktopEntry.id)));
            dock.draggedId = app.id;
            dock.folderHoverId = 'folder:fixture';
            dock.finishDrag();
        }
        function dragPoint(x: real, y: real): void {
            dock.updateFolderDrag(dock.contentItem, {
                x: x - dock.margins.left,
                y: y - (dock.screen.height - dock.height - dock.margins.bottom)
            });
        }
        function close(): void {
            if (dock.testFolderPopup)
                dock.testFolderPopup.visible = false;
        }
        function remove(): void {
            dock.deleteFolder(dock.openFolderId);
        }
        function reload(): void {
            Quickshell.reload(false);
        }
    }
}
