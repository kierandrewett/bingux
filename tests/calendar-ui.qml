import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CALENDAR_TEST_RESULTS") }
    Timer { id: finish; interval: 1000; onTriggered: Qt.quit() }
    FloatingWindow {
        id: window
        property var launchedCalendar: []
        implicitWidth: 420
        implicitHeight: 680
        color: Theme.barBackground
        Item { id: canvas; anchors.fill: parent }
        CalendarPopup { id: calendar; hostItem: canvas; preferredY: 8; calendarServiceEnabled: false; launchCalendar: command => window.launchedCalendar = command }
        TestCase {
            id: testCase
            property bool imageSaved: false
            when: window.visible
            name: "Calendar"
            function test_calendar() {
                report.setText("FAIL: calendar checks did not finish\n");
                calendar.visible = true;
                wait(250);
                const originalHeight = calendar.popupHeight;
                calendar.selectDate(new Date(calendar.today.getFullYear(), calendar.today.getMonth(), calendar.today.getDate() === 1 ? 2 : calendar.today.getDate() - 1));
                mouseClick(findChild(window.contentItem, "backToToday"));
                verify(calendar.sameDay(calendar.selectedDate, calendar.today));
                compare(calendar.periodProgress, 1, "Today in the same month does not restart view motion");
                compare(calendar.monthProgress, 1, "Today in the same month does not restart month motion");
                const initialDate = calendar.selectedDate.getTime();
                const heading = findChild(window.contentItem, "calendarPeriodHeading");
                mouseClick(heading);
                compare(calendar.viewLevel, 1);
                if (!Theme.reducedMotion) {
                    wait(70);
                    verify(calendar.periodProgress > 0 && calendar.periodProgress < 1, "Period transition animates over time");
                    verify(findChild(window.contentItem, "calendarPeriodPicker").scale !== 1, "Picker moves and scales during transition");
                }
                wait(260);
                compare(calendar.periodProgress, 1);
                const periodGrid = findChild(window.contentItem, "calendarPeriodPicker");
                compare(periodGrid.y, 0, "Month picker has no reserved weekday gap");
                compare(periodGrid.height, periodGrid.parent.height, "Picker uses the complete viewport");
                compare(heading.leftPadding, 0, "Heading has no duplicate left padding");
                calendar.body.parent.grabToImage(result => result.saveToFile("/tmp/bingux-calendar-month-picker.png"));
                wait(80);
                mouseClick(heading);
                compare(calendar.viewLevel, 2);
                mouseClick(heading);
                compare(calendar.viewLevel, 3);
                wait(260);
                calendar.body.parent.grabToImage(result => result.saveToFile("/tmp/bingux-calendar-decade-picker.png"));
                wait(80);
                compare(calendar.popupHeight, originalHeight, "Picker levels preserve popup height");
                compare(calendar.selectedDate.getTime(), initialDate, "Browsing does not change selected date");
                calendar.browseYear = 2026;
                const picker = findChild(window.contentItem, "calendarPeriodPicker");
                mouseClick(findChild(picker, "calendarPeriod-3"));
                compare(calendar.viewLevel, 2);
                compare(calendar.browseYear, 2020);
                mouseClick(findChild(picker, "calendarPeriod-7"));
                compare(calendar.viewLevel, 1);
                compare(calendar.browseYear, 2026);
                mouseClick(findChild(picker, "calendarPeriod-8"));
                compare(calendar.viewLevel, 0);
                compare(calendar.displayedMonth.getFullYear(), 2026);
                compare(calendar.displayedMonth.getMonth(), 8);
                mouseClick(heading);
                mouseClick(findChild(window.contentItem, "nextMonth"));
                compare(calendar.browseYear, 2027);
                keyClick(Qt.Key_Escape);
                compare(calendar.viewLevel, 0, "Escape returns to days from the header");
                verify(calendar.visible);
                calendar.selectDate(new Date(2024, 0, 31));
                calendar.viewLevel = 1;
                calendar.choosePeriod(2024, 1);
                compare(calendar.selectedDate.getDate(), 29, "Month choice clamps to leap February");
                calendar.selectDate(calendar.today);
                wait(300);
                const agendaCard = findChild(window.contentItem, "agendaCard");
                const originalAgendaY = agendaCard.y;
                const todayButton = findChild(window.contentItem, "backToToday");
                verify(todayButton.visible && !todayButton.enabled);
                const activeGrid = findChild(window.contentItem, "calendarGrid");
                function surface(date) {
                    return findChild(activeGrid, "calendarDaySurface-" + date.getFullYear() + "-" + date.getMonth() + "-" + date.getDate());
                }
                calendar.shiftMonth(1);
                wait(300);
                verify(surface(calendar.selectedDate) !== null);
                verify(surface(calendar.selectedDate).color.toString() !== Theme.accent.toString(), "Selected date in another month is not blue");
                compare(surface(calendar.selectedDate).color.a, 0, "Month navigation does not highlight a day");
                calendar.selectDate(calendar.today);
                wait(300);
                compare(surface(calendar.today).color, Theme.accent, "Today is blue");
                calendar.selectDate(new Date(calendar.today.getFullYear(), calendar.today.getMonth(), calendar.today.getDate() + 1));
                wait(220);
                verify(todayButton.visible && todayButton.enabled);
                compare(surface(calendar.today).color, Theme.accent, "Today stays blue when another date is selected");
                verify(surface(calendar.selectedDate).color.toString() !== Theme.accent.toString(), "Other selected dates are neutral");
                compare(calendar.popupHeight, originalHeight, "Today button reserves its space");
                compare(agendaCard.y, originalAgendaY, "Agenda never shifts when Today becomes available");
                calendar.shiftMonth(1);
                if (!Theme.reducedMotion) verify(calendar.monthProgress < 1, "Month transition starts immediately");
                calendar.shiftMonth(-1);
                tryCompare(calendar, "monthProgress", 1, 500);
                if (!Theme.reducedMotion) {
                    calendar.shiftMonth(1);
                    wait(50);
                    const incoming = findChild(window.contentItem, "calendarGrid");
                    const outgoing = findChild(window.contentItem, "outgoingCalendarGrid");
                    verify(outgoing.visible, "Outgoing month stays rendered during transition");
                    const incomingX = incoming.x;
                    const outgoingX = outgoing.x;
                    calendar.shiftMonth(-1);
                    fuzzyCompare(incoming.x, outgoingX, 1, "Reversal preserves incoming page position");
                    fuzzyCompare(outgoing.x, incomingX, 1, "Reversal preserves outgoing page position");
                    wait(300);
                    calendar.shiftMonth(1); calendar.shiftMonth(1); calendar.shiftMonth(1);
                    wait(550);
                    verify(calendar.sameMonth(calendar.renderedMonth, calendar.displayedMonth), "Rapid requests settle at latest month");
                    compare(calendar.monthProgress, 1);
                    calendar.selectDate(calendar.today);
                    wait(300);
                }
                calendar.selectDate(new Date(2024, 0, 31));
                calendar.shiftMonth(1);
                compare(calendar.selectedDate.getDate(), 29);
                calendar.shiftMonth(10);
                compare(calendar.selectedDate.getFullYear(), 2024);
                calendar.shiftMonth(1);
                compare(calendar.selectedDate.getFullYear(), 2025);
                const grid = findChild(window.contentItem, "calendarGrid");
                verify(grid !== null);
                grid.forceActiveFocus();
                keyClick(Qt.Key_Right);
                compare(calendar.selectedDate.getDate(), 30);
                keyClick(Qt.Key_Down);
                compare(calendar.selectedDate.getMonth(), 1);
                compare(calendar.selectedDate.getDate(), 6);
                calendar.selectDate(new Date(2026, 8, 7));
                wait(100);
                mouseClick(grid, grid.width / 7 * 1.5, grid.height / 6 * 1.5);
                compare(calendar.selectedDate.getDate(), 8, "Pointer selects date");
                wait(150);
                compare(surface(calendar.selectedDate).color, calendar.sameDay(calendar.selectedDate, calendar.today) ? Theme.accent : Theme.hover, "Click highlights the date while preserving today styling");
                const clickedSurface = surface(calendar.selectedDate);
                calendar.highlightedDate = null;
                for (let frame = 0; frame < 12; frame++) {
                    wait(10);
                    const c = clickedSurface.color;
                    const red = c.r * c.a + Theme.elevated.r * (1 - c.a);
                    verify(red >= Theme.elevated.r - 0.002, "Clearing a highlight never darkens the resting surface");
                }
                const nextButton = findChild(window.contentItem, "nextMonth");
                for (let cycle = 0; cycle < 4; cycle++) {
                    mouseMove(nextButton, nextButton.width / 2, nextButton.height / 2);
                    verify(nextButton.hovered, "Month button enters hover every time");
                    mouseMove(canvas, 2, 2);
                    verify(!nextButton.hovered && !nextButton.down, "Month button clears hover/press on leave");
                }
                calendar.selectDate(new Date(2026, 8, 7));
                const start = new Date(2026, 8, 7).getTime() / 1000;
                const next = new Date(2026, 8, 8).getTime() / 1000;
                calendar.eventSource.events = [
                    {id: "all-day", title: "Design week", start: start, end: next},
                    {id: "meeting", title: "A deliberately long event title to check that the agenda stays inside the panel", start: start + 36000, end: start + 39600},
                    {id: "old", title: "Yesterday", start: start - 3600, end: start}
                ];
                compare(calendar.dayEvents.length, 2);
                compare(calendar.eventTime(calendar.dayEvents[0]), "All day");
                wait(250);
                compare(calendar.popupHeight, originalHeight, "Event loading cannot resize the popup");
                verify(calendar.popupHeight < window.height - 16);
                compare(calendar.eventCommand({id: "source\nevent\n", start: start}).slice(-2).join("|"), "--uuid|source:event");
                compare(calendar.eventCommand({id: "source\nevent\n20260908T090000Z", start: start}).slice(-2).join("|"), "--uuid|source:event:20260908T090000Z");
                const eventRow = findChild(window.contentItem, "calendarEvent-0");
                mouseMove(eventRow, eventRow.width / 2, eventRow.height / 2);
                verify(eventRow.hovered);
                mouseClick(eventRow);
                verify(!calendar.visible, "Opening an event closes the popup");
                compare(window.launchedCalendar[0], "python3");
                calendar.visible = true;
                calendar.selectDate(new Date(2026, 8, 7));
                wait(300);
                const keyboardRow = findChild(window.contentItem, "calendarEvent-0");
                window.launchedCalendar = [];
                keyboardRow.forceActiveFocus();
                keyClick(Qt.Key_Return);
                compare(window.launchedCalendar[0], "python3", "Enter opens the focused event");
                calendar.visible = true;
                calendar.selectDate(new Date(2026, 8, 7));
                wait(300);
                const sampleEvents = calendar.eventSource.events;
                calendar.eventSource.events = Array.from({length: 12}, (_, i) => ({id: String(i), title: "Event " + i, start: start + 36000, end: start + 39600}));
                wait(100);
                const agenda = findChild(window.contentItem, "calendarAgenda");
                agenda.forceActiveFocus();
                keyClick(Qt.Key_Down);
                wait(250);
                verify(agenda.contentY > 0, "Keyboard scrolls the agenda");
                calendar.selectDate(new Date(2026, 8, 8));
                wait(100);
                compare(agenda.contentY, 0, "Changing date resets agenda scroll");
                calendar.selectDate(new Date(2026, 8, 7));
                calendar.eventSource.events = sampleEvents;
                wait(250);
                canvas.grabToImage(result => { testCase.imageSaved = result.saveToFile(Quickshell.env("BINGUX_CALENDAR_TEST_IMAGE")); });
                tryCompare(testCase, "imageSaved", true, 2000);
                if (!Theme.reducedMotion) {
                    testCase.imageSaved = false;
                    calendar.shiftMonth(1);
                    wait(35);
                    canvas.grabToImage(result => { testCase.imageSaved = result.saveToFile("/tmp/bingux-calendar-motion.png"); });
                    tryCompare(testCase, "imageSaved", true, 2000);
                }
                report.setText("PASS: date navigation, event boundaries, fixed Today/agenda layout, interruptible month motion, keyboard scrolling and reset\n");
                finish.start();
            }
        }
    }
}
