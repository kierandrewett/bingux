import QtQuick
import Quickshell

ShellRoot {
    // Keep the fixture alive while the dock intentionally has no mapped window.
    FloatingWindow {
        implicitWidth: 1
        implicitHeight: 1
    }
    QtObject {
        id: manager
        property var activeToplevel: null
        property QtObject toplevels: QtObject {
            property var values: []
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    component Window: QtObject {
        property string appId
        property string title: appId
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        function setRectangle(window, rect) {
        }
    }
    Component {
        id: windowFactory
        Window {}
    }
    Dock {
        id: dock
        testManager: manager
        settings: ({
                pinnedApps: []
            })
    }
    property int step: 0
    property int ticks: 0
    property int completedAt: 0
    function check(value, message) {
        if (!value)
            throw new Error(message);
    }
    Timer {
        interval: 35
        running: true
        repeat: true
        onTriggered: {
            try {
                ticks++;
                if (ticks > 100)
                    throw new Error("Startup timed out");
                if (step < 4) {
                    check(!dock.visible, "Do not reveal a partial startup batch");
                    const windows = [];
                    for (let i = 0; i < 8; i++) {
                        const index = step * 8 + i;
                        windows.push(windowFactory.createObject(manager, {
                            appId: 'startup-' + (Quickshell.env('DOCK_REPLAY') ? 31 - index : index)
                        }));
                    }
                    manager.toplevels.values = manager.toplevels.values.concat(windows);
                    manager.toplevels.objectInsertedPost(windows[0], step * 8);
                    step++;
                } else if (step === 4 && dock.visible) {
                    check(dock.testItems.count === 32, "All startup apps are present");
                    for (let i = 0; i < 32; i++) {
                        const item = dock.testItems.itemAt(i);
                        check(!item.entering && item.transitionProgress === 1, "Existing apps must not replay insertion animations");
                        check(item.currentGroup.id === 'startup-' + i, "Cached order survives reversed enumeration");
                    }
                    const added = windowFactory.createObject(manager, {
                        appId: 'opened-after-startup'
                    });
                    manager.toplevels.values = manager.toplevels.values.concat([added]);
                    manager.toplevels.objectInsertedPost(added, 32);
                    step++;
                } else if (step === 5 && dock.testItems.count === 33) {
                    check(dock.testItems.itemAt(32).entering, "Real launches still animate");
                    completedAt = ticks;
                    step++;
                } else if (step === 6 && ticks - completedAt >= 20) {
                    console.warn("DOCK_TEST_PASSED");
                    Qt.quit();
                }
            } catch (error) {
                console.error("DOCK_BEHAVIOUR_FAILED", error.message);
                Qt.quit();
            }
        }
    }
}
