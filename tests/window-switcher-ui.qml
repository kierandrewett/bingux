import QtQuick
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: test
    property int phase: 0
    property var failures: []
    property var openingFrames: []
    property var closingFrames: []
    property var selectionFrames: []
    property var selection: null
    property real selectionStart: 0
    property var viewport: null
    function findChild(parent, name) {
        if (parent.objectName === name) return parent;
        for (const child of parent.children || []) {
            const found = findChild(child, name);
            if (found) return found;
        }
        return null;
    }
    function check(condition, message) {
        if (!condition) { failures.push(message); console.error("FAIL: " + message); }
    }
    WindowSwitcher {
        id: chooser
        enabled: false
        showDelay: 0
        function appFor(window) {
            return window ? {id: window.appId.replace(/\.desktop$/, ""), name: "Test app",
                icon: "/usr/share/icons/hicolor/scalable/apps/org.gnome.Calculator.svg"} : null;
        }
        notifications: [{desktopEntry: "test-second"}, {desktopEntry: "test-second"}, {desktopEntry: "unrelated"}]
        activeStreams: [{properties: {"application.id": "test-second"}}]
    }
    FrameAnimation {
        running: chooser.active || chooser.revealProgress > 0
        onTriggered: {
            if (test.phase === 1) test.openingFrames.push(chooser.revealProgress);
            if (test.phase === 2 && test.selection) test.selectionFrames.push(test.selection.x);
            if (test.phase === 3 && test.selection && test.viewport) {
                const x = test.selection.x - test.viewport.contentX;
                test.check(x >= -1 && x + chooser.tileWidth <= test.viewport.width + 1,
                    "Highlight stays inside the viewport during scrolling and wraparound");
            }
            if (test.phase === 4) test.closingFrames.push(chooser.revealProgress);
        }
    }
    Timer {
        id: keyRepeat
        property int presses: 0
        interval: 20
        repeat: true
        onTriggered: {
            const before = chooser.selectedIcon;
            chooser.refresh(chooser.liveWindows.map(window => Object.assign({}, window)));
            test.check(chooser.selectedIcon === before, "Window snapshots preserve icon delegates during key repeat");
            chooser.step(presses >= 20);
            test.check(chooser.selectedIcon !== null, "Key repeat always has a rendered selected card");
            test.check(chooser.visibleWindows.length === chooser.displayCount && chooser.visibleWindows.every(window => !!window),
                "Every viewport slot remains populated across boundaries");
            if (++presses === 40) stop();
        }
    }
    Timer {
        interval: 300
        repeat: true
        running: true
        onTriggered: {
            if (test.phase === 0) {
                chooser.refresh([{id: "1", appId: "test-first.desktop", title: "First window", lastUserTime: 5, focused: true},
                    {id: "2", appId: "test-second.desktop", title: "Second window", lastUserTime: 4},
                    {id: "3", appId: "test-third.desktop", title: "Third window", lastUserTime: 3},
                    {id: "4", appId: "test-first.desktop", title: "Another window from the same app", parent: "1", lastUserTime: 2},
                    {id: "5", appId: "test-fifth.desktop", title: "", lastUserTime: 1}]);
                test.check(chooser.history.length === 5, "Each window is retained, including transients and untitled windows");
                chooser.step(false);
            } else if (test.phase === 1) {
                test.check(chooser.revealProgress === 1, "Opening reaches full opacity");
                test.check(Theme.reducedMotion || test.openingFrames.some(value => value > 0 && value < 1), "Opening renders intermediate frames");
                const before = chooser.selectedIcon;
                chooser.refresh(chooser.liveWindows.map(window => Object.assign({}, window, {title: window.title + " updated"})));
                test.check(chooser.selectedIcon === before, "Metadata updates preserve the selected icon and badge instances");
                test.check(chooser.selectedIcon.notificationCount === 2, "Shared icon excludes unrelated notifications");
                test.check(chooser.selectedIcon.playingAudio, "Shared icon matches audio activity");
                const audio = test.findChild(chooser.selectedIcon, "dockAudioBadge");
                const notifications = test.findChild(chooser.selectedIcon, "dockNotificationBadge");
                test.check(audio && audio.visible && audio.opacity === 1, "Audio badge is rendered");
                test.check(notifications && notifications.visible && notifications.count === 2, "Notification count is rendered");
                let presentation = chooser.selectedIcon;
                while (presentation && !test.findChild(presentation, "switcherTooltip")) presentation = presentation.parent;
                const tooltip = test.findChild(presentation, "switcherTooltip");
                test.check(tooltip && tooltip.text.includes("Playing audio") && tooltip.text.includes("Second window"), "Shared tooltip includes activity and window title");
                test.selection = test.findChild(presentation, "switcherSelection");
                test.viewport = test.findChild(presentation, "switcherViewport");
                test.selectionStart = test.selection.x;
                chooser.step(false);
            } else if (test.phase === 2) {
                const target = chooser.trackIndex * (chooser.tileWidth + chooser.tileGap);
                test.check(test.selection && test.selection.x === target, "Highlight reaches selected icon");
                test.check(Theme.reducedMotion || test.selectionFrames.some(value => value > test.selectionStart && value < target), "Selection slides between icons");
                test.check(!chooser.selectedIcon.playingAudio && chooser.selectedIcon.notificationCount === 0, "Badges do not transfer to another app");
                keyRepeat.start();
            } else if (test.phase === 3) {
                if (keyRepeat.running) return;
                test.check(chooser.selected === 2, "Forward and backward wrapping returns to the same window");
                chooser.close();
                test.check(!chooser.active, "Closing releases selection immediately");
                test.check(Theme.reducedMotion || chooser.revealProgress > 0, "Closing retains the rendered surface during its animation");
            } else if (test.phase === 4) {
                test.check(chooser.revealProgress === 0 && chooser.windows.length === 0, "Closing releases retained content");
                test.check(Theme.reducedMotion || test.closingFrames.some(value => value > 0 && value < 1), "Closing renders intermediate frames");
                console.info(test.failures.length ? "SWITCHER_UI_FAILED" : "SWITCHER_UI_PASSED");
                Qt.quit();
            }
            test.phase++;
        }
    }
}
