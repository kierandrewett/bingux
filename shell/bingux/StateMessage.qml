import QtQuick
import QtQuick.Layouts

// The one treatment for "nothing here", "still loading" and "that failed" in
// shell popups and sidebar panels: a glyph (or a spinner while loading), a
// title, an optional detail line and an optional action. Every popup uses it,
// so the three states look the same wherever they appear.
//
//   StateMessage { title: "No paired devices"; compact: true }
//   StateMessage { loading: true; title: "Loading events…" }
//   StateMessage { tone: "danger"; title: "Could not load connections"; detail: "Open Network settings to manage them."
//                  actionText: "Try again"; onActionTriggered: reload() }
//
// compact: a leading glyph with left-aligned text, for a message inside a list
// or a section. Not compact: centred and stacked, for a message that stands
// for a whole panel; set fill to centre it in the room the layout gives it.
Item {
    id: root

    // "neutral" (empty, loading), "success", "warning" or "danger".
    property string tone: "neutral"
    property bool loading: false
    // Overrides the tone's glyph. Empty draws the tone's own, or none for neutral.
    property string iconName: ""
    property string title: ""
    property string detail: ""
    property string actionText: ""
    property bool compact: false
    // Take all the height the layout offers and centre the message in it.
    property bool fill: false

    signal actionTriggered

    readonly property color toneColor: tone === "danger" ? Theme.danger : tone === "warning" ? Theme.warning : tone === "success" ? Theme.success : Theme.muted
    readonly property string displayedIcon: iconName !== "" ? iconName : tone === "danger" ? "dialog-error-symbolic" : tone === "warning" ? "dialog-warning-symbolic" : tone === "success" ? "object-select-symbolic" : ""
    readonly property bool hasGlyph: loading || displayedIcon !== ""
    readonly property color titleColor: tone === "neutral" && compact ? Theme.muted : Theme.text
    readonly property int glyphSize: compact ? Theme.iconSize : 28
    readonly property int inset: compact ? Theme.padding : Theme.paddingLarge

    Layout.fillWidth: true
    Layout.fillHeight: fill
    implicitHeight: layout.implicitHeight + inset * 2
    Accessible.role: tone === "danger" ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: root.title + (root.detail !== "" ? ". " + root.detail : "")

    GridLayout {
        id: layout
        x: root.inset
        y: root.fill ? Math.max(root.inset, (root.height - height) / 2) : root.inset
        width: root.width - root.inset * 2
        columns: root.compact ? 2 : 1
        columnSpacing: Theme.gap
        rowSpacing: Theme.gap

        Item {
            objectName: "stateGlyph"
            visible: root.hasGlyph
            Layout.alignment: root.compact ? Qt.AlignLeft | Qt.AlignTop : Qt.AlignHCenter
            implicitWidth: root.glyphSize
            implicitHeight: root.glyphSize
            PreviewSpinner {
                anchors.fill: parent
                loading: root.loading
                colour: Theme.muted
                accessibleName: root.title
            }
            SymbolicIcon {
                anchors.centerIn: parent
                visible: !root.loading
                implicitSize: root.glyphSize
                source: root.displayedIcon !== "" ? BinguxIcons.source(root.displayedIcon) : ""
                color: root.toneColor
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceSmall
            Text {
                objectName: "stateTitle"
                Layout.fillWidth: true
                visible: root.title !== ""
                text: root.title
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                horizontalAlignment: root.compact ? Text.AlignLeft : Text.AlignHCenter
                color: root.titleColor
                font.family: Theme.fontFamily
                font.pixelSize: root.compact ? Theme.fontSmall + 1 : Theme.fontSize
                font.weight: root.compact ? Font.Normal : Font.DemiBold
            }
            Text {
                objectName: "stateDetail"
                Layout.fillWidth: true
                visible: root.detail !== ""
                text: root.detail
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                horizontalAlignment: root.compact ? Text.AlignLeft : Text.AlignHCenter
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            ActionButton {
                objectName: "stateAction"
                Layout.topMargin: Theme.spaceSmall
                Layout.alignment: root.compact ? Qt.AlignLeft : Qt.AlignHCenter
                visible: root.actionText !== ""
                text: root.actionText
                onClicked: root.actionTriggered()
            }
        }
    }
}
