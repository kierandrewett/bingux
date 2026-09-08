import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: result; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    QtObject {
        id: calendarData
        property bool loading: false
        property bool available: true
        property string error: ""
        property var events: [{title: "Review plans", start: new Date(2026, 8, 7, 10).getTime()/1000, end: new Date(2026, 8, 7, 11).getTime()/1000}]
        function forDay(day) { return day.getDate() === 7 ? events : []; }
    }
    component FakePlayer: QtObject {
        property string identity: "Music"
        property string desktopEntry: ""
        property string trackTitle: "A Quiet Evening"
        property string trackArtist: "Local library"
        property string trackArtUrl: ""
        property bool positionSupported: true
        property bool lengthSupported: true
        property bool canSeek: true
        property real position: 42
        property real length: 180
        property int uniqueId: 1
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canGoPrevious: true
        property bool canGoNext: true
        property bool isPlaying: false
        function play() { isPlaying = true; }
        function pause() { isPlaying = false; }
        function previous() {}
        function next() {}
    }
    FakePlayer { id: player }
    FakePlayer { id: secondPlayer; identity: "Video"; trackTitle: "A second player"; uniqueId: 2 }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FloatingWindow {
        id: window
        implicitWidth: 384; implicitHeight: 720
        color: Theme.barBackground
        SidebarTasks { id: tasks; x: 8; y: 40; width: window.width - 16; height: window.height - 48; preferencesLocation: Qt.resolvedUrl("tasks.ini") }
        SidebarCalendar { id: calendar; x: 8; y: 40; width: tasks.width; height: tasks.height; visible: false; serviceEnabled: false; eventSource: calendarData }
        SidebarMedia { id: media; x: 8; y: 40; width: tasks.width; height: tasks.height; visible: false; players: [player, secondPlayer] }
        TestCase {
            property string checks: ""
            function equal(a, b) { checks += "Compare " + a + " / " + b + "\n"; compare(a, b); }
            function check(value) { checks += "Check " + value + "\n"; verify(value); }
            when: window.visible
            function test_panels() {
                tasks.addTask("Write release notes");
                tasks.addTask("Check a long task wraps within a narrow sidebar without moving the delete button off screen");
                equal(tasks.tasks.length, 2);
                tasks.toggleTask(0);
                equal(tasks.remaining, 1);
                const component = Qt.createComponent("SidebarTasks.qml");
                equal(component.status, Component.Ready);
                const restored = component.createObject(window.contentItem, {visible: false, preferencesLocation: tasks.preferencesLocation});
                equal(restored.tasks.length, 2);
                check(restored.tasks[0].done);
                restored.destroy();
                for (const width of [134, 304, 368]) {
                    tasks.width = width;
                    wait(120);
                    const row = findChild(tasks, "taskList").itemAtIndex(1);
                    check(row !== null);
                    check(row.width > 0 && row.width <= width);
                    calendar.visible = true;
                    calendar.selectDate(new Date(2026, 8, 7));
                    equal(calendar.dayEvents[0].title, "Review plans");
                    calendar.shiftMonth(4);
                    equal(calendar.month.getFullYear(), 2027);
                    equal(calendar.month.getMonth(), 0);
                    calendar.selectDate(new Date(2026, 8, 7));
                    wait(100);
                    check(calendar.contentWidth <= width);
                    media.visible = true;
                    wait(120);
                    check(media.contentWidth <= width);
                    calendar.visible = false;
                    media.visible = false;
                }
                tasks.width = 368;
                if (Quickshell.env("BINGUX_PANELS_SCREENSHOT")) grabImage(tasks).save(Quickshell.env("BINGUX_PANELS_SCREENSHOT") + ".tasks.png");
                tasks.visible = false; calendar.visible = true;
                wait(150);
                if (Quickshell.env("BINGUX_PANELS_SCREENSHOT")) grabImage(calendar).save(Quickshell.env("BINGUX_PANELS_SCREENSHOT") + ".calendar.png");
                calendar.visible = false; media.visible = true;
                wait(350);
                if (Quickshell.env("BINGUX_PANELS_SCREENSHOT")) grabImage(media).save(Quickshell.env("BINGUX_PANELS_SCREENSHOT") + ".media.png");
                const firstPanel = findChild(media, "sidebarPlayer0");
                const secondPanel = findChild(media, "sidebarPlayer1");
                check(firstPanel !== null && secondPanel !== null);
                check(secondPanel.y >= firstPanel.y + firstPanel.height);
                findChild(firstPanel, "mediaPlayPause").clicked();
                check(player.isPlaying);
                check(!secondPlayer.isPlaying);
                findChild(secondPanel, "mediaPlayPause").clicked();
                check(player.isPlaying && secondPlayer.isPlaying);
                findChild(firstPanel, "mediaPlayPause").clicked();
                check(!player.isPlaying && secondPlayer.isPlaying);
                media.players = [];
                equal(media.players.length, 0);
                tasks.removeTask(1);
                tasks.removeTask(0);
                equal(tasks.tasks.length, 0);
            }
            function cleanupTestCase() { result.setText("FAILURES " + qtest_results.failCount + "\n" + checks); finish.start(); }
        }
    }
}
