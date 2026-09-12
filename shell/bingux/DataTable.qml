import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Flickable {
    id: horizontal
    property var columns: []
    property var columnWidths: []
    readonly property real tableWidth: columnWidths.reduce((sum, value) => sum + value, 0)
    property int rowHeight: 34
    property bool headerVisible: true
    property string sortKey: ""
    property bool descending: false
    property string sortObjectPrefix: "tableSort_"
    property string rowsObjectName: "tableRows"
    property string scrollObjectName: "tableScrollBar"
    property var headerLabel: key => key + (sortKey === key ? (descending ? " ↓" : " ↑") : "")
    property var headerHint: key => ""
    property var headerAccessibleName: key => "Sort by " + key
    property var rightAligned: index => false
    property alias model: list.model
    property alias delegate: list.delegate
    readonly property alias listView: list
    readonly property bool hovered: tableHover.hovered
    signal sortRequested(string key)
    signal keyPressed(var event)
    Layout.fillWidth: true
    Layout.fillHeight: true
    Layout.minimumHeight: 80
    contentWidth: horizontal.tableWidth
    contentHeight: height
    interactive: false
    flickableDirection: Flickable.HorizontalFlick
    boundsBehavior: Flickable.StopAtBounds
    clip: true
    HorizontalWheelScroll {
        viewport: horizontal
    }
    ScrollBar.horizontal: ScrollBar {
        policy: horizontal.contentWidth > horizontal.width ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
    }
    WheelHandler {
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        acceptedModifiers: Qt.KeyboardModifierMask
        property real remainderX: 0
        property real remainderY: 0
        onWheel: event => {
            // Four rows per notch; fractional high-resolution input accumulates
            // into whole rows. Assign directly, without flicking or animation.
            const sideways = (event.modifiers & Qt.ShiftModifier) !== 0;
            const dx = sideways ? event.angleDelta.y : 0;
            const dy = sideways ? 0 : event.angleDelta.y;
            const px = sideways ? event.pixelDelta.y : 0;
            const py = sideways ? 0 : event.pixelDelta.y;
            if (!dx && !dy && !px && !py) {
                event.accepted = false;
                return;
            }
            remainderX += px ? px / 34 : dx / 30;
            remainderY += py ? py / horizontal.rowHeight : dy / 30;
            const xSteps = Math.trunc(remainderX);
            const ySteps = Math.trunc(remainderY);
            remainderX -= xSteps;
            remainderY -= ySteps;
            if (xSteps)
                horizontal.contentX = Math.max(0, Math.min(horizontal.contentWidth - horizontal.width, horizontal.contentX - xSteps * 34));
            if (ySteps)
                list.contentY = list.originY + Math.max(0, Math.min(list.contentHeight - list.height, list.contentY - list.originY - ySteps * horizontal.rowHeight));
            event.accepted = true;
        }
    }
    Rectangle {
        width: horizontal.tableWidth
        height: headings.height
        radius: 6
        color: Theme.surface
    }
    Row {
        id: headings
        visible: horizontal.headerVisible
        height: visible ? 34 : 0
        Repeater {
            model: horizontal.columns
            AbstractButton {
                id: heading
                required property string modelData
                required property int index
                objectName: horizontal.sortObjectPrefix + modelData
                width: horizontal.columnWidths[index]
                height: headings.height
                hoverEnabled: true
                Accessible.name: horizontal.headerAccessibleName(modelData)
                onClicked: horizontal.sortRequested(modelData)
                ShellTooltip {
                    visible: heading.hovered && text.length > 0
                    text: horizontal.headerHint(heading.modelData)
                }
                background: Rectangle {
                    radius: heading.index === 0 || heading.index === horizontal.columns.length - 1 ? 6 : 0
                    color: heading.hovered || heading.visualFocus ? Theme.hover : "transparent"
                }
                contentItem: Text {
                    leftPadding: 6
                    rightPadding: 6
                    text: horizontal.headerLabel(heading.modelData)
                    elide: Text.ElideRight
                    horizontalAlignment: horizontal.rightAligned(heading.index) ? Text.AlignRight : Text.AlignLeft
                    verticalAlignment: Text.AlignVCenter
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    font.weight: horizontal.sortKey === heading.modelData ? Font.DemiBold : Font.Normal
                    color: horizontal.sortKey === heading.modelData ? Theme.text : Theme.muted
                }
            }
        }
    }
    ListView {
        id: list
        objectName: horizontal.rowsObjectName
        y: headings.height
        width: horizontal.tableWidth
        height: Math.max(0, horizontal.height - headings.height - (horizontal.contentWidth > horizontal.width ? 12 : 0))
        model: null
        clip: true
        interactive: false
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        activeFocusOnTab: true
        HoverHandler {
            id: tableHover
        }
        Keys.onPressed: event => horizontal.keyPressed(event)
        ScrollBar.vertical: ScrollBar {
            objectName: horizontal.scrollObjectName
            x: horizontal.contentX + horizontal.width - width
            z: 2
            policy: list.contentHeight > list.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        }
    }
}
