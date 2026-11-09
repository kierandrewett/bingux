import QtQuick
import QtQuick.Controls

// A clipped, scrollable column for installer page bodies. Content that fits
// stays still; content that does not scrolls instead of running past the card.
// Overflow is always visible: a scroll bar sits at the right edge and the
// bottom edge fades into the card while more content waits below, so a row is
// never cut off with nothing to say there is more.
Flickable {
    id: root

    default property alias content: column.data
    property alias spacing: column.spacing
    // Content runs past the viewport and can be scrolled.
    readonly property bool overflowing: contentHeight > height + 0.5
    // More content is hidden below the viewport edge.
    readonly property bool moreBelow: overflowing && contentY + height < contentHeight - 1
    // Width reserved at the right for the scroll bar. Fixed, so a page does
    // not reflow when it starts or stops overflowing.
    readonly property real gutter: Math.round(14 * InstallerState.uiScale)

    width: parent ? parent.width : 300
    height: parent ? parent.height : 300
    clip: true
    contentWidth: width
    contentHeight: column.implicitHeight
    boundsBehavior: Flickable.StopAtBounds
    interactive: overflowing

    ScrollBar.vertical: ScrollBar {
        objectName: "installerScrollBar"
        parent: root
        x: root.width - width
        y: 0
        height: root.height
        width: Math.round(8 * InstallerState.uiScale)
        policy: root.overflowing ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        visible: root.overflowing
    }

    Column {
        id: column
        width: root.width - root.gutter
        spacing: Math.round(12 * InstallerState.uiScale)
    }

    Rectangle {
        objectName: "installerScrollFade"
        // Over the viewport, not inside the scrolled content.
        parent: root
        z: 2
        x: 0
        y: root.height - height
        width: root.width
        height: Math.round(36 * InstallerState.uiScale)
        opacity: root.moreBelow ? 1 : 0
        visible: opacity > 0
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.rgba(Theme.popupSurface.r, Theme.popupSurface.g, Theme.popupSurface.b, 0)
            }
            GradientStop {
                position: 1
                color: Theme.popupSurface
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.motion
            }
        }
    }
}
