import QtQuick

// A boxed message with a title and a body: a warning, a danger note, a plain
// fact or a confirmation. The tone sets the edge, the tint, the glyph and the
// title colour, so the meaning never rests on colour alone. Installer pages,
// settings pages and popups use it for anything longer than a StateMessage.
//
//   ShellNotice { tone: "danger"; title: "This erases data"; body: "Installing erases /dev/vda completely." }
//
// tone: "info", "success", "warning" or "danger" (the Theme colours of the
// same name; info is the accent). unit scales the box for a surface that
// scales its type, such as the installer card; 1 is the desktop size.
Rectangle {
    id: root

    property string tone: "info"
    property string title: ""
    property string body: ""
    property real unit: 1

    readonly property color accent: tone === "danger" ? Theme.danger : tone === "warning" ? Theme.warning : tone === "success" ? Theme.success : Theme.accent
    readonly property string glyph: tone === "danger" ? "dialog-error-symbolic" : tone === "warning" ? "dialog-warning-symbolic" : tone === "success" ? "object-select-symbolic" : "dialog-information-symbolic"
    readonly property real inset: Math.round(18 * unit)

    width: parent ? parent.width : 300
    implicitHeight: row.implicitHeight + Math.round(24 * unit)
    height: implicitHeight
    radius: Theme.radius
    color: Qt.rgba(accent.r, accent.g, accent.b, 0.12)
    border.width: 1
    border.color: Qt.rgba(accent.r, accent.g, accent.b, 0.55)
    Accessible.role: tone === "danger" || tone === "warning" ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: root.title + (root.title !== "" && root.body !== "" ? ". " : "") + root.body

    Row {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: root.inset
        anchors.rightMargin: root.inset
        spacing: Math.round(12 * root.unit)

        SymbolicIcon {
            id: glyphIcon
            implicitSize: Math.round(20 * root.unit)
            source: BinguxIcons.source(root.glyph)
            color: root.accent
        }
        Column {
            width: parent.width - glyphIcon.width - parent.spacing
            spacing: Math.round(4 * root.unit)

            Text {
                visible: root.title !== ""
                width: parent.width
                text: root.title
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: root.accent
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(18 * root.unit)
                font.weight: Font.DemiBold
            }
            Text {
                visible: root.body !== ""
                width: parent.width
                text: root.body
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(16 * root.unit)
                lineHeight: 1.2
            }
        }
    }
}
