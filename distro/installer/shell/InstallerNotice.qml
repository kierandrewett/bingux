import QtQuick

// A boxed message for installer pages: a warning, a danger note or a plain
// fact. The tone sets the accent bar and the title colour.
Rectangle {
    id: root

    // "danger", "warning" or "info".
    property string tone: "info"
    property string title: ""
    property string body: ""

    readonly property color accent: tone === "danger" ? Theme.danger : tone === "warning" ? Theme.warning : Theme.accent

    width: parent ? parent.width : 300
    implicitHeight: column.implicitHeight + Math.round(24 * InstallerState.uiScale)
    height: implicitHeight
    radius: Theme.radius
    color: Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.12)
    border.width: 1
    border.color: Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.55)

    Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Math.round(18 * InstallerState.uiScale)
        anchors.rightMargin: Math.round(18 * InstallerState.uiScale)
        spacing: Math.round(4 * InstallerState.uiScale)

        Text {
            visible: root.title !== ""
            width: parent.width
            text: root.title
            wrapMode: Text.WordWrap
            color: root.accent
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(18 * InstallerState.uiScale)
            font.weight: Font.DemiBold
        }
        Text {
            visible: root.body !== ""
            width: parent.width
            text: root.body
            wrapMode: Text.WordWrap
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(16 * InstallerState.uiScale)
            lineHeight: 1.2
        }
    }
}
