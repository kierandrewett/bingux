import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }
    Timer {
        id: finish
        interval: 200
        onTriggered: Qt.quit()
    }
    TerminalSidebar {
        id: sidebar
        screen: Quickshell.screens[0]
        settings: QtObject {
            property bool sidebarEnabled: true
            property bool dockEnabled: false
        }
    }
    Window {
        id: app
        visible: true
        width: 400
        height: 300
        title: "Sidebar fullscreen regression"
    }
    TestCase {
        property string checks: ""
        when: app.visible
        function test_drag_width() {
            sidebar.selectContent("notes");
            sidebar.open();
            wait(400);
            const initial = sidebar.rightInset;
            sidebar.beginGesture(1000, 400, 400);
            sidebar.updateGesture(1000 + initial - 300, 400);
            wait(30);
            checks += "Resize width " + sidebar.contentItem.width + " inset " + sidebar.rightInset + "\n";
            compare(sidebar.contentItem.width, 300);
            compare(sidebar.rightInset, 300);
            sidebar.updateGesture(1000 + initial - 200, 400);
            wait(30);
            checks += "Slide width " + sidebar.contentItem.width + " inset " + sidebar.rightInset + "\n";
            compare(sidebar.contentItem.width, 250);
            compare(sidebar.rightInset, 200);
            sidebar.updateGesture(1000 + initial - 100, 400);
            wait(30);
            checks += "Slide width " + sidebar.contentItem.width + " inset " + sidebar.rightInset + "\n";
            compare(sidebar.contentItem.width, 250);
            compare(sidebar.rightInset, 100);
            sidebar.updateGesture(1000 + initial - 200, 400);
            wait(30);
            checks += "Slide width " + sidebar.contentItem.width + " inset " + sidebar.rightInset + "\n";
            compare(sidebar.contentItem.width, 250);
            sidebar.finishGesture(false);
            wait(400);
            wait(30);
            checks += "Slide width " + sidebar.contentItem.width + " inset " + sidebar.rightInset + "\n";
            compare(sidebar.contentItem.width, 250);
            compare(sidebar.rightInset, 250);
            sidebar.beginGesture(1000, 400, 400);
            sidebar.updateGesture(1160, 400);
            sidebar.finishGesture(false);
            wait(400);
            compare(sidebar.opened, false);
            compare(sidebar.rightInset, 0);
            checks += "250px minimum, sliding, reversal, release and close passed\n";
        }
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
            mouseMove(sensor.contentItem, 1, 400);
            mousePress(sensor.contentItem, 1, 400);
            checks += "Gesture " + sidebar.gestureActive + " pointer " + sidebar.pointerPosition + "\n";
            verify(sidebar.gestureActive);
            mouseMove(sensor.contentItem, -179, 400);
            mouseRelease(sensor.contentItem, -179, 400);
            checks += "Dragged: " + sidebar.opened + "\n";
            tryCompare(sidebar, "opened", true);
            wait(300);
            verify(ToplevelManager.toplevels.values.some(w => w.title === app.title && w.fullscreen));
            checks += "Retained fullscreen: " + sidebar.fullscreenApp + "\n";
            compare(sidebar.fullscreenApp, true);
            compare(sidebar.desktopCornerSize, 0, "Fullscreen sidebar has square corners");
            sidebar.hide();
            app.showNormal();
            app.requestActivate();
            tryCompare(sidebar, "fullscreenApp", false);
            checks += "Exited fullscreen\n";
            sidebar.open();
            wait(300);
            verify(sidebar.desktopCornerSize > 0, "Desktop corners return outside fullscreen");
            app.showFullScreen();
            app.requestActivate();
            tryCompare(sidebar, "opened", false);
        }
        function cleanupTestCase() {
            report.setText("FAILURES " + qtest_results.failCount + "\n" + checks);
            finish.start();
        }
    }
}
