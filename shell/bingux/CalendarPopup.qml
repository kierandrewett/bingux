import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

ShellPopup {
    id: root
    property date today: new Date()
    property date displayedMonth: new Date(today.getFullYear(), today.getMonth(), 1)
    popupWidth: 364
    popupHeight: calendar.implicitHeight + Theme.padding * 2
    onVisibleChanged: if (visible) { today = new Date(); displayedMonth = new Date(today.getFullYear(), today.getMonth(), 1) }
    function shiftMonth(amount) { displayedMonth = new Date(displayedMonth.getFullYear(), displayedMonth.getMonth() + amount, 1) }
    ColumnLayout {
        id: calendar
        width: parent.width
        spacing: Theme.padding
        Text { text: root.today.toLocaleDateString(Qt.locale(), "dddd, d MMMM yyyy"); color: Theme.muted; font.pixelSize: Theme.fontSize; Layout.alignment: Qt.AlignHCenter }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            Button { text: "‹"; Accessible.name: "Previous month"; onClicked: root.shiftMonth(-1) }
            Text { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: root.displayedMonth.toLocaleDateString(Qt.locale(), "MMMM yyyy"); color: Theme.text; font.pixelSize: 16; font.weight: Font.DemiBold }
            Button { text: "›"; Accessible.name: "Next month"; onClicked: root.shiftMonth(1) }
        }
        DayOfWeekRow { Layout.fillWidth: true; locale: Qt.locale(); delegate: Text { required property string shortName; text: shortName; color: Theme.muted; font.pixelSize: 12; horizontalAlignment: Text.AlignHCenter } }
        MonthGrid {
            id: grid
            Layout.fillWidth: true
            Layout.preferredHeight: 252
            month: root.displayedMonth.getMonth()
            year: root.displayedMonth.getFullYear()
            locale: Qt.locale()
            delegate: Rectangle {
                required property var model
                implicitWidth: 40
                implicitHeight: 40
                radius: 20
                color: model.today ? Theme.accent : "transparent"
                Text {
                    anchors.centerIn: parent
                    text: parent.model.day
                    color: parent.model.today ? Theme.background : parent.model.month === grid.month ? Theme.text : Theme.muted
                    opacity: parent.model.month === grid.month ? 1 : 0.5
                    font.pixelSize: Theme.fontSize
                    font.weight: parent.model.today ? Font.Bold : Font.Normal
                }
            }
        }
        Button { text: "Today"; Layout.alignment: Qt.AlignHCenter; onClicked: root.displayedMonth = new Date(root.today.getFullYear(), root.today.getMonth(), 1) }
    }
}
