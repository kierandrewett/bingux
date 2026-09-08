import QtQuick

// Shared tooltip surface for layer windows and anchored controls.
Rectangle {
    id: root
    property string text: ""
    property string supportingText: ""
    property int maximumWidth: 320
    property bool wrapText: true
    readonly property int horizontalPadding: 12
    readonly property int verticalPadding: 9
    implicitWidth: Math.min(maximumWidth, Math.ceil(Math.max(
        ...text.split("\n").map(line => textMeasure.advanceWidth(line)),
        ...supportingText.split("\n").map(line => hintMeasure.advanceWidth(line)))) + horizontalPadding * 2)
    implicitHeight: Math.ceil(content.implicitHeight) + verticalPadding * 2
    color: Theme.surface
    border.color: Theme.outline
    radius: Theme.insetRadius(Theme.cardRadius, Theme.gap)
    FontMetrics { id: textMeasure; font: label.font }
    FontMetrics { id: hintMeasure; font: hint.font }
    Column {
        id: content
        x: root.horizontalPadding
        y: root.verticalPadding
        width: Math.max(0, root.width - root.horizontalPadding * 2)
        spacing: 6
        Text {
            id: label
            width: parent.width
            text: root.text
            textFormat: Text.PlainText
            wrapMode: root.wrapText ? Text.WrapAtWordBoundaryOrAnywhere : Text.NoWrap
            elide: root.wrapText ? Text.ElideNone : Text.ElideRight
            horizontalAlignment: Text.AlignLeft
            lineHeight: 1.1
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
        Text {
            id: hint
            visible: text.length > 0
            width: parent.width
            text: root.supportingText
            textFormat: Text.PlainText
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            horizontalAlignment: Text.AlignLeft
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
}
