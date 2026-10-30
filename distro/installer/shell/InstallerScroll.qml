import QtQuick

// A clipped, scrollable column for installer page bodies. Content that fits
// stays still; content that does not scrolls instead of running past the card.
Flickable {
    id: root

    default property alias content: column.data
    property alias spacing: column.spacing

    width: parent ? parent.width : 300
    height: parent ? parent.height : 300
    clip: true
    contentWidth: width
    contentHeight: column.implicitHeight
    boundsBehavior: Flickable.StopAtBounds

    Column {
        id: column
        width: root.width
        spacing: Math.round(12 * InstallerState.uiScale)
    }
}
