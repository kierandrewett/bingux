import QtQuick

Item {
    required property var context
    property int count: 0
    Component.onCompleted: context.registerAction("increment", () => ++count)
}
