import QtQuick

Item {
    id: root
    required property var context
    property int count: 0
    property var button: null
    property var popup: null
    implicitWidth: button ? button.implicitWidth : 100
    implicitHeight: context.theme.barHeight
    Component.onCompleted: {
        button = context.createButton(root, {
            label: "Counter",
            iconName: "list-add-symbolic",
            width: Qt.binding(() => root.width),
            height: Qt.binding(() => root.height)
        });
        button.clicked.connect(() => {
            count = context.invokeAction("org.bingux.example/increment");
            if (!popup)
                popup = context.createPopup(details, {
                    popupWidth: 240
                });
            if (popup)
                popup.visible = !popup.visible;
        });
    }
    Component {
        id: details
        Text {
            required property var context
            width: 216
            height: 60
            text: "Clicked " + root.count + " times"
            color: context.theme.text
            font.family: context.theme.fontFamily
            font.pixelSize: context.theme.fontSize
            verticalAlignment: Text.AlignVCenter
        }
    }
}
