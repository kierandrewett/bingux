import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    QtObject {
        id: source
        property bool loading: false
        property bool available: true
        property string error: ""
        property int count: 3
        function forDay(day) {
            const start = new Date(2026, 7, 31).getTime() / 1000;
            return Array.from({length: count}, (_, i) => ({id: String(i), title: "Summer Bank Holiday (regional holiday)", start, end: start + 86400}));
        }
    }
    Window {
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        id: window
        width: 520
        height: 900
        color: Theme.barBackground
        SidebarCalendar {
            id: calendar
            anchors.fill: parent
            anchors.margins: 16
            serviceEnabled: false
            eventSource: source
            selectedDate: new Date(2026, 7, 31)
        }
        TestCase {
            when: window.visible
            property string checks: ""
            function check(value, message) {
                checks += "CHECK " + value + " " + message + "\n";
                report.setText(checks + "FAILURES 1\n");
                verify(value, message);
            }
            function test_agenda_height() {
                wait(300);
                const agenda = findChild(calendar, "calendarAgenda");
                const card = findChild(calendar, "agendaCard");
                check(agenda.count === 3, "Three events loaded");
                check(agenda.height >= agenda.contentHeight, "All events fit inside the agenda");
                check(card.height > Theme.calendarAgendaHeight, "Agenda grows beyond the popout limit");
                check(!agenda.interactive, "Sidebar has no nested event scrolling");
                check(calendar.contentHeight <= calendar.availableHeight, "Available sidebar space is used without scrolling");
                check(agenda.itemAtIndex(2).mapToItem(calendar, 0, agenda.itemAtIndex(2).height).y <= calendar.height, "Last event is fully visible");
                grabImage(calendar).save("/tmp/bingux-sidebar-calendar-fit.png");
                source.count = 20;
                wait(300);
                check(agenda.height >= agenda.contentHeight, "Busy day still expands the agenda");
                check(calendar.contentHeight > calendar.availableHeight, "Whole sidebar scrolls for a busy day");
                window.width = 260;
                wait(300);
                check(agenda.height >= agenda.contentHeight, "Wrapped titles fit after narrowing");
                source.count = 0;
                wait(300);
                check(card.height === Theme.calendarAgendaHeight, "Empty day returns to its compact height");
                report.setText(checks + "FAILURES 0\n");
            }
            function cleanupTestCase() { window.visible = false; finish.start(); }
        }
    }
}
