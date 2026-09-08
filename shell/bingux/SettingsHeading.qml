import QtQuick
import QtQuick.Layouts
ColumnLayout {
    property string title
    property string description: ""
    Layout.fillWidth: true
    spacing: Theme.gap
    Text { Layout.fillWidth: true; text: parent.title; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold; wrapMode: Text.Wrap }
    Text { Layout.fillWidth: true; visible: text !== ""; text: parent.description; textFormat: Text.PlainText; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.Wrap }
}
