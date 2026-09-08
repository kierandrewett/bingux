import QtQuick
import Quickshell

// A visible widget uses its rendered frame. Dormant widgets use their existing
// component type, with state copied from the live instance, inside the palette.
Item {
    id: root
    required property string widgetId
    readonly property var live: DesktopEditing.sources[widgetId]
    readonly property var frame: widgetId === "notifications" && !live?.visible ? null : DesktopEditing.previews[widgetId]
    property var metrics: null
    readonly property bool panelWidget: ["notes", "monitor", "terminal", "calendar", "media", "tasks"].includes(widgetId)
    clip: true
    Image {
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.panelWidget ? 0 : (parent.height - height) / 2
        width: Math.min(parent.width, root.frame?.width || 0)
        height: root.panelWidget && root.frame ? root.frame.height * width / Math.max(1, root.frame.width) : Math.min(parent.height, root.frame?.height || 0)
        source: root.frame?.url || ""; fillMode: Image.PreserveAspectFit; cache: false
    }
    Loader {
        id: fallback
        active: DesktopEditing.active && !!DesktopEditing.editor?.nativeWindow && !root.frame
        anchors.centerIn: parent
        width: root.widgetId.startsWith("control-") ? Math.min(parent.width, root.live?.width || 188) : parent.width
        height: root.widgetId.startsWith("control-") ? 104 : parent.height
        sourceComponent: root.widgetId.startsWith("control-") ? control : root.widgetId === "notifications" ? notifications
            : root.widgetId === "capture" ? capture : root.widgetId === "privacy" ? privacy
            : root.widgetId === "overflow" ? overflow : root.widgetId === "tray" ? tray
            : ["calendar", "media", "tasks"].includes(root.widgetId) ? panel : empty
    }
    Component {
        id: control
        ControlRow {
            title: root.live?.title || ""; subtitle: root.live?.subtitle || ""; iconName: root.live?.iconName || ""
            presentation: root.live?.presentation || null
            tileLayout: root.live?.tileLayout ?? true; compactTile: root.live?.compactTile ?? false
            selected: root.live?.selected ?? false; navigation: root.live?.navigation ?? false
            toggleVisible: root.live?.toggleVisible ?? false; toggleChecked: root.live?.toggleChecked ?? false
            valueText: root.live?.valueText || ""; rowInteractive: false
        }
    }
    Component { id: notifications; Item { NotificationIndicator { anchors.centerIn: parent; count: 1 } } }
    Component { id: capture; Item { ActivityIndicator { anchors.centerIn: parent; barWindow: DesktopEditing.editor?.nativeWindow; filled: true; label: "00:00"; activityColor: Theme.recordingIndicator; trailingIcon: "screencast-stop-symbolic" } } }
    Component { id: privacy; Item { ActivityIndicator { anchors.centerIn: parent; barWindow: DesktopEditing.editor?.nativeWindow; iconName: "camera-web-symbolic" } } }
    Component { id: overflow; Item { IconButton { anchors.centerIn: parent; iconName: "view-more-symbolic"; label: "More"; background: BarControlSurface {} } } }
    Component { id: tray; Item { Tray { anchors.centerIn: parent; parentWindow: DesktopEditing.editor?.nativeWindow } } }
    Component {
        id: panel
        Item {
            clip: true
            Loader {
                anchors.centerIn: parent; width: 400; height: 220
                scale: Math.min(parent.width / width, parent.height / height)
                source: root.widgetId === "calendar" ? "SidebarCalendar.qml" : root.widgetId === "media" ? "SidebarMedia.qml" : "SidebarTasks.qml"
                onLoaded: {
                    if (root.widgetId === "calendar") item.serviceEnabled = false;
                }
            }
        }
    }
    Component { id: empty; Item { Text { anchors.centerIn: parent; text: root.widgetId === "terminal" ? "Terminal is closed" : "Hidden when inactive"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall } } }
    // Preview controls never dispatch their normal actions.
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
}
