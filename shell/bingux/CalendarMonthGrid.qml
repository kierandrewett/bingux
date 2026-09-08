import QtQuick
import QtQuick.Controls

// Both pages use the same cells; the outgoing page keeps its original selection.
MonthGrid {
    id: root
    required property date pageDate
    required property date selectedDate
    required property var highlightedDate
    required property var eventSource
    property var events: eventSource.events
    property bool animateSelection: true
    month: pageDate.getMonth()
    year: pageDate.getFullYear()
    locale: Qt.locale()
    padding: 0
    spacing: 0
    delegate: Item {
        id: dayCell
        required property var model
        readonly property bool selected: root.highlightedDate !== null && model.date.getFullYear() === root.highlightedDate.getFullYear()
            && model.date.getMonth() === root.highlightedDate.getMonth() && model.date.getDate() === root.highlightedDate.getDate()
        readonly property bool keyboardFocused: root.visualFocus && model.date.getFullYear() === root.selectedDate.getFullYear()
            && model.date.getMonth() === root.selectedDate.getMonth() && model.date.getDate() === root.selectedDate.getDate()
        implicitWidth: 40; implicitHeight: 40
        Accessible.role: Accessible.Button
        Accessible.name: model.date.toLocaleDateString(Qt.locale(), "dddd, d MMMM yyyy")
        Accessible.selected: selected
        Accessible.ignored: !root.enabled
        Accessible.onPressAction: if (root.enabled) root.clicked(model.date)
        Rectangle {
            objectName: "calendarDaySurface-" + dayCell.model.date.getFullYear() + "-" + dayCell.model.month + "-" + dayCell.model.day
            anchors.centerIn: parent
            width: Math.min(36, dayCell.width); height: Math.min(36, dayCell.height); radius: Math.min(Theme.radius, width / 2)
            // Fade alpha only: interpolating grey to transparent black creates
            // a dark dip below the card's resting colour on pointer/selection exit.
            property real highlightOpacity: dayCell.selected ? 1 : 0
            Behavior on highlightOpacity { enabled: root.animateSelection; NumberAnimation { duration: Theme.reducedMotion ? 0 : Theme.motion } }
            color: dayCell.model.today ? Theme.accent : Qt.rgba(Theme.hover.r, Theme.hover.g, Theme.hover.b, highlightOpacity)
            border.width: dayCell.selected || dayCell.keyboardFocused ? 1 : 0
            border.color: root.visualFocus ? Theme.text : Theme.outline
            Rectangle {
                objectName: "calendarDayHover"
                anchors.fill: parent
                radius: parent.radius
                visible: dayHover.hovered && root.enabled
                color: dayCell.model.today ? Qt.rgba(1, 1, 1, 0.16)
                    : dayCell.selected ? Theme.pressed : Theme.hover
            }
            Text {
                anchors.centerIn: parent
                text: dayCell.model.day
                color: dayCell.model.today ? Theme.background : dayCell.model.month === root.month ? Theme.text : Theme.muted
                Behavior on color { enabled: root.animateSelection; ColorAnimation { duration: Theme.reducedMotion ? 0 : Theme.motion } }
                opacity: dayCell.model.month === root.month ? 1 : 0.4
                font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                font.weight: dayCell.selected || dayCell.model.today ? Font.DemiBold : Font.Normal
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 3
                width: 4; height: 4; radius: 2
                color: dayCell.model.today ? Theme.background : Theme.muted
                visible: root.eventSource.forDay(dayCell.model.date, root.events).length > 0
            }
        }
        HoverHandler { id: dayHover; enabled: root.enabled; cursorShape: Qt.ArrowCursor }
    }
}
