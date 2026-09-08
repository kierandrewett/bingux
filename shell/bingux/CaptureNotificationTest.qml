import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    NotificationState { id: notifications }
    NotificationSurface { id: surface; state: notifications }
    IpcHandler {
        target: "test"
        function snapshot(): string {
            return JSON.stringify(notifications.allEntries.map(entry => ({
                id: entry.notification.id, summary: entry.summary, body: entry.body, image: entry.image,
                actions: entry.actions.map(action => action.action.identifier)
            })));
        }
        function invoke(identifier: string): void {
            const entry = notifications.allEntries[0];
            entry.actions.find(action => action.action.identifier === identifier).action.invoke();
        }
        function dismiss(): void { notifications.dismissAll(); }
        function buttonGeometry(): string {
            const buttons = [];
            function visit(item) {
                if (item.iconName && item.text && item.contentItem) {
                    const row = item.contentItem.children[0];
                    const icon = row.children[0], label = row.children[1];
                    const origin = icon.mapToItem(item, 0, 0), end = label.mapToItem(item, label.width, 0);
                    buttons.push({left: origin.x, right: item.width - end.x,
                        iconCenter: origin.y + icon.height / 2, center: item.height / 2,
                        gap: label.x - icon.x - icon.width});
                }
                for (const child of item.children || []) visit(child);
            }
            visit(surface.viewport);
            return JSON.stringify(buttons);
        }
        function preview(): string {
            function find(item) {
                if (item.objectName === "notificationImagePreview") return item;
                for (const child of item.children || []) { const found = find(child); if (found) return found; }
                return null;
            }
            const image = find(surface.viewport);
            return JSON.stringify(image ? {height: image.height, paintedHeight: image.paintedHeight, ready: image.status === Image.Ready} : null);
        }
    }
}
