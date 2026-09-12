import QtQuick
import QtQuick.Controls

ScrollView {
    id: root
    property alias selectedDate: calendar.selectedDate
    property alias month: calendar.displayedMonth
    property bool serviceEnabled: true
    property var eventSource: events
    readonly property var dayEvents: eventSource.forDay(selectedDate)
    contentWidth: availableWidth
    contentHeight: calendar.popupHeight + Theme.gap * 2
    clip: true
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
    function focusContent() {
        calendar.focusCalendar();
    }
    function shiftMonth(amount) {
        calendar.shiftMonth(amount);
    }
    function selectDate(date) {
        calendar.selectDate(date);
    }
    CalendarEvents {
        id: events
        active: root.visible && root.serviceEnabled
        month: root.month
    }
    Item {
        id: host
        width: root.availableWidth
        height: root.contentHeight
        CalendarPopup {
            id: calendar
            hostItem: host
            inlineMode: true
            calendarServiceEnabled: false
            eventSource: root.eventSource
            visible: root.visible
            surfaceVisible: false
            contentPadding: 0
            popupWidth: Math.max(0, host.width - Theme.gap * 2)
            preferredX: Theme.gap
            preferredY: Theme.gap
        }
    }
}
