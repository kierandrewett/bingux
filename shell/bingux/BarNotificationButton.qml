import QtQuick
import QtQuick.Effects
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

AbstractButton {
    id: root
    property int count: 0
    property var presentation: null
    property var barWindow: null
    property bool selected: false
    implicitWidth: face.implicitWidth + Theme.barEdgeHitWidth - Theme.iconSize
    implicitHeight: Theme.barHeight
    Accessible.name: "Notifications"
    Accessible.description: count + " notifications"
    function playArchive() {
        badge.playArchive();
    }
    contentItem: Item {
        RowLayout {
            id: face
            anchors.centerIn: parent
            spacing: Theme.gap
            layer.enabled: true
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowColor: Theme.contentShadow
                shadowBlur: 0.28
                shadowVerticalOffset: 1.2
            }
            WidgetFace {
                visible: !!root.presentation?.custom
                presentation: root.presentation
            }
            SymbolicIcon {
                // The notification indicator intentionally disappears at zero.
                // Keep the control discoverable with the bell in that state.
                visible: root.count === 0
                implicitSize: Theme.iconSize
                source: root.presentation?.icon ? Quickshell.iconPath(root.presentation.icon) : Quickshell.iconPath("preferences-system-notifications-symbolic")
                color: Theme.text
            }
            NotificationIndicator {
                id: badge
                count: root.count
            }
        }
    }
    BarTooltip {
        reorderable: true
        anchorItem: root
        barWindow: root.barWindow
        requested: notificationHover.hovered
        text: "Notifications"
        supportingText: root.count === 1 ? "1 unread notification" : root.count + " unread notifications"
    }
    background: BarControlSurface {
        hovered: notificationHover.hovered
        pressed: root.down
        focused: root.visualFocus
        selected: root.selected
    }
    HoverHandler {
        id: notificationHover
    }
}
