import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "../shell/bingux"

ShellRoot {
    id: root
    property string mode: ""
    function find(item, name) {
        if (item.objectName === name)
            return item;
        for (const child of item.children || []) {
            const match = find(child, name);
            if (match)
                return match;
        }
        return null;
    }
    PanelWindow {
        id: background
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        WlrLayershell.layer: WlrLayer.Background
        color: "black"
        Grid {
            columns: Math.ceil(background.width / 8)
            Repeater {
                model: parent.columns * Math.ceil(background.height / 8)
                Rectangle {
                    required property int index
                    width: 8
                    height: 8
                    color: (index % parent.columns + Math.floor(index / parent.columns)) % 2 ? "#eeeeee" : "#222222"
                }
            }
        }
    }
    PanelWindow {
        id: shared
        visible: ["inline", "shadowed", "tooltip", "corner"].includes(root.mode)
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "effect-shared"
        color: "transparent"
        TooltipBubble {
            id: tooltip
            visible: root.mode === "tooltip"
            x: 180
            y: 180
            width: implicitWidth
            height: implicitHeight
            text: "Shared tooltip"
        }
        ShellCorner {
            id: corner
            visible: root.mode === "corner"
            x: 180
            y: 180
            width: 64
            height: 64
        }
    }
    ShellPopup {
        id: inlinePopup
        visible: ["inline", "shadowed", "floating"].includes(root.mode)
        windowShadow: root.mode === "shadowed"
        hostItem: root.mode === "floating" ? floating.contentItem : shared.contentItem
        initialRevealScale: 1
        popupWidth: 280
        popupHeight: 180
        Text {
            text: "Inline panel with opaque text"
            color: Theme.text
        }
    }
    FloatingWindow {
        id: floating
        title: "Floating panel fade regression"
        visible: root.mode === "floating"
        color: "transparent"
        implicitWidth: 600
        implicitHeight: 400
    }
    QtObject {
        id: notification
        property int id: 100
        property real expireTimeout: 0
        property string appName: "Files"
        property string desktopEntry: ""
        property string appIcon: "system-file-manager"
        property string summary: "Fade test"
        property string body: "The blur must follow this card."
        property var actions: []
        property bool tracked: false
        signal closed(int reason)
        function dismiss() {
        }
        function expire() {
        }
    }
    NotificationState {
        id: notifications
    }
    NotificationSurface {
        id: notificationSurface
        state: notifications
        inputSuspended: root.mode !== "notification"
    }
    SearchOverlay {
        id: search
    }
    CaptureTool {
        id: capture
        opened: root.mode === "capture"
    }
    IpcHandler {
        target: "fades"
        function diagnostic(): string {
            if (root.mode === "preview") {
                const panel = root.find(search.contentItem, "searchSurface");
                const preview = root.find(search.contentItem, "searchPreviewSurface");
                return JSON.stringify({
                    width: search.width,
                    height: search.height,
                    mainY: panel.y,
                    previewY: preview.y,
                    previewHeight: preview.height,
                    opacity: preview.opacity
                });
            }
            const helper = root.find(inlinePopup.body.parent, "surfaceFade");
            return JSON.stringify({
                status: SurfaceFades.component.status,
                error: SurfaceFades.component.errorString(),
                helper: !!helper,
                loaded: !!helper?.item,
                available: helper?.item?.available,
                target: helper?.item?.target === inlinePopup.body.parent
            });
        }
        function present(kind: string): void {
            search.closeSearch();
            root.mode = kind;
            if (kind === "notification" && !notifications.allEntries.length)
                notifications.accept(notification);
            if (["search", "preview"].includes(kind))
                search.showSearch();
        }
        function prepare(): void {
            if (root.mode === "preview") {
                const preview = root.find(search.contentItem, "searchPreviewSurface");
                preview.presentedResult = {
                    title: "Preview fade",
                    subtitle: "/home/kieran/dev/bingux/README.md"
                };
                preview.reveal = 1;
                preview.visible = true;
                preview.opacity = 1;
                root.find(search.contentItem, "searchSurface").y = 200;
            }
        }
        function alpha(value: real): void {
            if (["inline", "shadowed", "floating"].includes(root.mode))
                inlinePopup.presentationOpacity = value;
            if (root.mode === "tooltip")
                tooltip.opacity = value;
            if (root.mode === "corner")
                corner.opacity = value;
            if (root.mode === "notification")
                notificationSurface.viewport.presentationOpacity = value;
            if (root.mode === "preview")
                root.find(search.contentItem, "searchPreviewSurface").opacity = value;
            if (root.mode === "search")
                root.find(search.contentItem, "searchSurface").opacity = value;
            if (root.mode === "capture")
                root.find(capture.previewItem, "captureToolbar").opacity = value;
        }
    }
}
