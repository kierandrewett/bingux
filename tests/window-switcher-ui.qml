import QtQuick
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: test
    property int phase: 0
    property var failures: []
    property var openingFrames: []
    property var closingFrames: []
    property var selection: null
    property var grid: null
    property var presentation: null
    function findChild(parent, name) {
        if (parent.objectName === name)
            return parent;
        for (const child of parent.children || []) {
            const found = findChild(child, name);
            if (found)
                return found;
        }
        return null;
    }
    function check(condition, message) {
        if (!condition) {
            failures.push(message);
            console.error("FAIL: " + message);
        }
    }
    WindowSwitcher {
        id: chooser
        enabled: false
        showDelay: 0
        function appFor(window) {
            return window ? {
                id: window.appId.replace(/\.desktop$/, ""),
                name: "Test app",
                icon: "/usr/share/icons/hicolor/scalable/apps/org.gnome.Calculator.svg"
            } : null;
        }
        notifications: [
            {
                desktopEntry: "test-second"
            },
            {
                desktopEntry: "test-second"
            },
            {
                desktopEntry: "unrelated"
            }
        ]
        activeStreams: [
            {
                properties: {
                    "application.id": "test-second"
                }
            }
        ]
    }
    FrameAnimation {
        running: chooser.active || chooser.revealProgress > 0
        onTriggered: {
            if (test.phase === 1)
                test.openingFrames.push(chooser.revealProgress);
            if (test.phase === 4)
                test.closingFrames.push(chooser.revealProgress);
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
            test.check(chooser.cardCount === chooser.windows.length && chooser.visibleWindows.length === chooser.windows.length, "The grid keeps exactly one card for every window across boundaries");
            if (++presses === 40)
                stop();
        }
    }
    Timer {
        interval: 300
        repeat: true
        running: true
        onTriggered: {
            if (test.phase === 0) {
                chooser.refresh([
                    {
                        id: "1",
                        appId: "test-first.desktop",
                        title: "First window",
                        lastUserTime: 5,
                        focused: true,
                        geometry: {
                            width: 1920,
                            height: 1080
                        }
                    },
                    {
                        id: "2",
                        appId: "test-second.desktop",
                        title: "Second window",
                        lastUserTime: 4,
                        geometry: {
                            width: 1280,
                            height: 800
                        }
                    },
                    {
                        id: "3",
                        appId: "test-third.desktop",
                        title: "Third window",
                        lastUserTime: 3,
                        geometry: {
                            width: 900,
                            height: 1400
                        }
                    },
                    {
                        id: "4",
                        appId: "test-first.desktop",
                        title: "Another window from the same app",
                        parent: "1",
                        lastUserTime: 2,
                        geometry: {
                            width: 1600,
                            height: 900
                        }
                    },
                    {
                        id: "5",
                        appId: "test-fifth.desktop",
                        title: "",
                        lastUserTime: 1,
                        geometry: {
                            width: 800,
                            height: 600
                        }
                    }
                ]);
                test.check(chooser.history.length === 5, "Each window is retained, including transients and untitled windows");
                const now = Date.now();
                chooser.previews = {
                    "1": "cached",
                    "2": "cached"
                };
                chooser.previewTimes = {
                    "1": now - 3000,
                    "2": now - 3000
                };
                test.check(chooser.needsPreview(chooser.liveWindows[0], now), "Refresh content from the focused window");
                test.check(!chooser.needsPreview(chooser.liveWindows[1], now), "Reuse background previews beyond two seconds");
                test.check(chooser.needsPreview(chooser.liveWindows[1], now + 30000), "Refresh old background snapshots");
                test.check(chooser.needsPreview(chooser.liveWindows[2], now), "Capture windows without a preview");
                chooser.previews = ({});
                chooser.previewTimes = ({});
                test.check(chooser.cardCount === chooser.history.length, "Idle cards are prepared before the shortcut");
                chooser.step(false);
                test.check(chooser.revealProgress === 1, "The shortcut reveals the chooser immediately");
            } else if (test.phase === 1) {
                test.check(chooser.revealProgress === 1, "Opening reaches full opacity");
                test.check(test.openingFrames.every(value => value === 1), "Opening does not delay the first fully visible frame");
                const before = chooser.selectedIcon;
                chooser.refresh(chooser.liveWindows.map(window => Object.assign({}, window, {
                        title: window.title + " updated"
                    })));
                test.check(chooser.selectedIcon === before, "Metadata updates preserve the selected icon and badge instances");
                test.check(chooser.selectedIcon.notificationCount === 2, "Shared icon excludes unrelated notifications");
                test.check(chooser.selectedIcon.playingAudio, "Shared icon matches audio activity");
                const audio = test.findChild(chooser.selectedIcon, "dockAudioBadge");
                const notifications = test.findChild(chooser.selectedIcon, "dockNotificationBadge");
                test.check(audio && audio.visible && audio.opacity === 1, "Audio badge is rendered");
                test.check(notifications && notifications.visible && notifications.count === 2, "Notification count is rendered");
                test.check(audio.height === 12 && notifications.height === 12, "Switcher badges are half the icon height");
                test.check(audio.cutoutMargin === 1, "Compact badges use a compact icon cutout");
                test.presentation = chooser.selectedIcon;
                while (test.presentation && !test.findChild(test.presentation, "switcherGrid"))
                    test.presentation = test.presentation.parent;
                test.grid = test.findChild(test.presentation, "switcherGrid");
                test.selection = test.findChild(test.presentation, "switcherSelection");
                test.check(chooser.gridWidth > 0 && chooser.gridWidth <= chooser.gridMaxWidth, "Window cards stay within the constrained central thumbnail width");
                test.check(chooser.previewSize(chooser.liveWindows[0]).width !== chooser.previewSize(chooser.liveWindows[2]).width, "Preview width follows the source window aspect ratio");
                test.check(test.grid && test.grid.width === chooser.gridWidth, "The switcher lays cards out in the computed thumbnail row width");
                test.check(test.grid && test.grid.height > chooser.cardHeightFor(chooser.liveWindows[0]), "Additional windows wrap onto a new thumbnail row");
                test.check(test.selection && test.findChild(test.selection, "switcherAppIcon"), "Selected cards put the app icon above the preview");
                test.check(test.selection && test.findChild(test.selection, "switcherAppTitle"), "Selected cards put the app title above the preview");
                test.check(test.selection && test.findChild(test.selection, "switcherPreview"), "Selected cards contain a preview below the header");
                test.check(chooser.cardCount === chooser.windows.length, "The grid has one card per window, without carousel copies");
                const previousSelection = test.selection;
                chooser.step(false);
                test.check(test.findChild(test.presentation, "switcherSelection") !== previousSelection, "Selection changes cards directly without sliding the grid");
            } else if (test.phase === 2) {
                test.selection = test.findChild(test.presentation, "switcherSelection");
                test.check(test.selection && test.selection.selectedCard, "Selection is represented by the active grid card");
                test.check(chooser.cardCount === chooser.windows.length, "Grid selection does not create duplicate cards");
                test.check(!chooser.selectedIcon.playingAudio && chooser.selectedIcon.notificationCount === 0, "Badges do not transfer to another app");
                keyRepeat.start();
            } else if (test.phase === 3) {
                if (keyRepeat.running)
                    return;
                test.check(chooser.selected === 2, "Forward and backward wrapping returns to the same window");
                chooser.close();
                test.check(!chooser.active, "Closing releases selection immediately");
                test.check(Theme.reducedMotion || chooser.revealProgress > 0, "Closing retains the rendered surface during its animation");
            } else if (test.phase === 4) {
                test.check(chooser.revealProgress === 0 && chooser.windows.length === 0, "Closing releases the gesture snapshot");
                chooser.selected = 1;
                const preparedIcon = chooser.selectedIcon;
                test.check(chooser.cardCount === chooser.history.length, "Closed chooser keeps its live cards ready");
                chooser.step(false);
                test.check(chooser.selectedIcon === preparedIcon, "Reopening reuses the prepared selected icon");
                chooser.cancel();
                test.check(Theme.reducedMotion || test.closingFrames.some(value => value > 0 && value < 1), "Closing renders intermediate frames");
                console.info(test.failures.length ? "SWITCHER_UI_FAILED" : "SWITCHER_UI_PASSED");
                Qt.quit();
            }
            test.phase++;
        }
    }
}
