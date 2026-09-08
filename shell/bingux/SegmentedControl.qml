import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    property var options: []
    property string currentValue: ""
    property string accessiblePrefix: ""
    property string objectNamePrefix: ""
    property var disabledOptions: []
    // This is a navigation control, not a collection of small buttons. Keep
    // the rail quiet and let the selected segment be the only raised plane.
    property real buttonRadius: 7
    property Component segmentContent: defaultContent
    signal selected(string value)
    implicitWidth: options.length * 42 + 8
    implicitHeight: 32
    radius: buttonRadius + 2
    color: Theme.surface
    Component {
        id: defaultContent
        Text {
            text: parent.segmentValue.charAt(0).toUpperCase() + parent.segmentValue.slice(1)
            color: parent.segmentSelected ? Theme.text : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
    Row {
        anchors.fill: parent
        anchors.margins: 4
        spacing: 4
        Repeater {
            model: root.options
            AbstractButton {
                id: segment
                hoverEnabled: true
                required property string modelData
                required property int index
                enabled: !root.disabledOptions.includes(modelData)
                objectName: root.objectNamePrefix + modelData
                readonly property real slot: (parent.width - parent.spacing * (root.options.length - 1)) / root.options.length
                width: Math.round((index + 1) * slot) - Math.round(index * slot)
                height: parent.height
                checkable: true
                autoExclusive: true
                checked: root.currentValue === modelData
                Accessible.name: root.accessiblePrefix + modelData.charAt(0).toUpperCase() + modelData.slice(1)
                onClicked: root.selected(modelData)
                background: ControlCentreButtonSurface {
                    control: segment
                    radius: root.buttonRadius
                    baseColor: segment.checked ? Theme.hover : "transparent"
                }
                ShellTooltip {
                    parent: segment
                    visible: segment.hovered && root.accessiblePrefix.length > 0
                    text: segment.Accessible.name
                }
                contentItem: Loader {
                    property string segmentValue: segment.modelData
                    property bool segmentSelected: segment.checked
                    sourceComponent: root.segmentContent
                }
            }
        }
    }
}
