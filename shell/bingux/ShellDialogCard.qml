import QtQuick
import QtQuick.Layouts

// The card of a shell dialog: a title, a body, room for content between the
// body and the buttons, and up to two buttons on the right. ShellDialog puts
// it in a layer window; tests and previews draw it on its own.
//
// The primary button is the action the dialog is for; the secondary button
// is the way out. tone "danger" makes the primary button destructive (Force
// Quit, Delete) and leaves the secondary quiet. Escape emits dismissed().
FocusScope {
    id: root

    property string title: ""
    property string body: ""
    // "": the primary button is the accent button. "danger": it is destructive.
    property string tone: ""
    property string primaryText: ""
    property string secondaryText: ""
    property bool primaryEnabled: true
    // Which button takes the keyboard when the card opens. A destructive
    // dialog focuses the safe one.
    property bool focusPrimary: tone !== "danger"
    // Content between the body and the buttons.
    default property alias content: contentSlot.data
    readonly property alias primaryButton: primary
    readonly property alias secondaryButton: secondary

    signal primaryTriggered
    signal secondaryTriggered
    signal dismissed

    implicitWidth: 420
    implicitHeight: frame.implicitHeight
    Accessible.role: Accessible.Dialog
    Accessible.name: root.title
    Accessible.description: root.body

    function focusDefault() {
        const button = root.focusPrimary && root.primaryText !== "" ? primary : root.secondaryText !== "" ? secondary : primary;
        button.forceActiveFocus();
    }

    Keys.onEscapePressed: root.dismissed()

    Rectangle {
        id: frame
        anchors.fill: parent
        implicitHeight: layout.implicitHeight + Theme.padding * 2
        radius: Theme.shellRadius
        color: Theme.shellSurface
        border.width: 1
        border.color: Theme.outline

        ColumnLayout {
            id: layout
            anchors.fill: parent
            anchors.margins: Theme.padding
            spacing: Theme.gap

            Text {
                objectName: "dialogTitle"
                Layout.fillWidth: true
                visible: root.title !== ""
                text: root.title
                textFormat: Text.PlainText
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
            Text {
                objectName: "dialogBody"
                Layout.fillWidth: true
                visible: root.body !== ""
                text: root.body
                textFormat: Text.PlainText
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }
            ColumnLayout {
                id: contentSlot
                Layout.fillWidth: true
                spacing: Theme.gap
                visible: children.length > 0
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.gap
                spacing: Theme.gap
                visible: root.primaryText !== "" || root.secondaryText !== ""
                Item {
                    Layout.fillWidth: true
                }
                ActionButton {
                    id: secondary
                    objectName: "dialogSecondary"
                    visible: root.secondaryText !== ""
                    text: root.secondaryText
                    onClicked: root.secondaryTriggered()
                }
                ActionButton {
                    id: primary
                    objectName: "dialogPrimary"
                    visible: root.primaryText !== ""
                    text: root.primaryText
                    tone: root.tone === "danger" ? "danger" : "primary"
                    enabled: root.primaryEnabled
                    onClicked: root.primaryTriggered()
                }
            }
        }
    }
}
