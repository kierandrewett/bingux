import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    QtObject {
        id: sampleEvents
        property bool loading: false
        property bool available: true
        property string error: ""
        property var events: []
        function forDay(day) { return sampleEvents.events; }
    }
    FloatingWindow {
        id: window
        implicitWidth: 440
        implicitHeight: 740
        Item { id: canvas; width: 420; height: 720 }
        CalendarPopup {
            id: calendar
            hostItem: canvas
            preferredY: 140
            calendarServiceEnabled: false
            eventSource: sampleEvents
        }
        TestCase {
            when: window.visible
            function initTestCase() { report.setText("RUNNING"); }
            function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount); }
            function test_short_popup() {
                try {
                    calendar.visible = true;
                    tryCompare(calendar, "revealScale", 1, 3000);
                    const agenda = findChild(calendar.body, "agendaCard");
                    const viewport = findChild(calendar.body, "calendarViewport");
                    for (const height of [720, 600]) {
                        canvas.height = height;
                        wait(100);
                        const bottom = agenda.mapToItem(calendar.body, 0, agenda.height).y;
                        verify(bottom <= calendar.body.height + 0.01, "The complete agenda fits the popup at " + height);
                        verify(calendar.panelY >= Theme.gap);
                        verify(calendar.panelY + calendar.body.parent.height <= height - Theme.gap);
                        verify(!viewport.interactive, "The whole calendar only scrolls when necessary");
                    }
                    canvas.height = 480;
                    tryCompare(viewport, "interactive", true);
                    mouseWheel(viewport, 3, 3, 0, -120);
                    tryVerify(() => viewport.contentY > 0);
                    const heading = findChild(calendar.body, "calendarPeriodHeading");
                    heading.forceActiveFocus();
                    tryVerify(() => heading.mapToItem(viewport, 0, 0).y >= 0, 1000, "Keyboard focus reveals the heading");
                    const start = calendar.today.getTime() / 1000;
                    sampleEvents.events = Array.from({length: 10}, (_, index) => ({id: String(index), title: "Event " + index, start, end: start + 3600}));
                    const list = findChild(calendar.body, "calendarAgenda");
                    tryCompare(list, "count", 10);
                    list.forceActiveFocus();
                    tryVerify(() => list.mapToItem(viewport, 0, list.height).y <= viewport.height + 0.01, 1000,
                        "Keyboard focus reveals the agenda on a very short screen");
                    keyClick(Qt.Key_Down);
                    tryVerify(() => list.contentY > 0, 1000, "The compact agenda retains keyboard scrolling");
                    canvas.height = 720;
                    tryCompare(viewport, "interactive", false);
                    tryCompare(viewport, "contentY", 0);
                    tryCompare(agenda, "height", Theme.calendarAgendaHeight);
                } catch (error) { console.error("CALENDAR_FIT_FAILED", error.message, error.stack); throw error; }
            }
        }
    }
}
