import QtQuick

IconButton {
    id: root
    property bool available: false
    property string summary: ""
    property bool barLayout: false
    label: available ? summary.replace(/^Battery /, "").replace(" percent", "%").replace(/,.*$/, "") : "No battery"
    text: label
    iconName: "battery-good-symbolic"
    barStyle: barLayout
    tooltipEnabled: barLayout
    tooltipText: available ? summary : label
    activeFocusOnTab: true
    implicitHeight: barLayout ? Theme.barHeight : 32
    presentation: ({
            custom: true,
            label: root.label,
            icon: root.iconName,
            showIcon: true,
            showText: true
        })
    Accessible.name: available ? summary : label
}
