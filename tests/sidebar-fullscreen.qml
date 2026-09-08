import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    TerminalSidebar {
        id: sidebar
        screen: Quickshell.screens[0]
        settings: QtObject { property bool sidebarEnabled: true; property bool dockEnabled: false }
    }
    Window { id: app; visible: true; width: 400; height: 300; title: "Sidebar fullscreen regression" }
    TestCase {
        property string checks: ""
        when: app.visible
        function test_fullscreen() {
            sidebar.selectContent("notes");
            sidebar.open();
            wait(400);
            app.showFullScreen();
            app.requestActivate();
            tryVerify(() => ToplevelManager.toplevels.values.some(w => w.title === app.title && w.fullscreen));
            checks += "Fullscreen windows: " + ToplevelManager.toplevels.values.filter(w => w.fullscreen).length + "; sidebar open: " + sidebar.opened + "\n";

            tryCompare(sidebar, "opened", false);
            checks += "Collapsed: " + sidebar.opened + "\n";
            wait(400);
            checks += "Insets " + sidebar.leftInset + "," + sidebar.rightInset + "," + sidebar.topInset + "\n";
            tryCompare(sidebar, "rightInset", 0);
            // A deliberate edge drag must still reveal the same content.
            const sensor = sidebar.edgeSurface;
            wait(300);
            checks += "Sensor visible " + sensor.visible + " width " + sensor.width + "\n";
            verify(sensor.visible);
            compare(sensor.width, 2);
            mouseMove(sensor.contentItem, 1, 240);
            mousePress(sensor.contentItem, 1, 240);
            checks += "Gesture " + sidebar.gestureActive + " pointer " + sidebar.pointerPosition + "\n";
            verify(sidebar.gestureActive);
            mouseMove(sensor.contentItem, -179, 240);
            mouseRelease(sensor.contentItem, -179, 240);
            checks += "Dragged: " + sidebar.opened + "\n";
            tryCompare(sidebar, "opened", true);
            wait(300);
            verify(ToplevelManager.toplevels.values.some(w => w.title === app.title && w.fullscreen));
            checks += "Retained fullscreen: " + sidebar.fullscreenApp + "\n";
            compare(sidebar.fullscreenApp, true);
            sidebar.hide();
            app.showNormal();
            app.requestActivate();
            tryCompare(sidebar, "fullscreenApp", false);
            checks += "Exited fullscreen\n";
            sidebar.open();
            wait(300);
            app.showFullScreen();
            app.requestActivate();
            tryCompare(sidebar, "opened", false);
        }
        function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n" + checks); finish.start(); }
    }
}
