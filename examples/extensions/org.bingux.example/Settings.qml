import QtQuick

Text {
    required property var context
    text: "Add Counter from Customise UI. Click it to test its action and popup. This example does not need any configuration."
    color: context.theme.text
    wrapMode: Text.Wrap
    font.family: context.theme.fontFamily
    font.pixelSize: context.theme.fontSize
}
