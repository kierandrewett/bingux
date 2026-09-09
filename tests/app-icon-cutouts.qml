import QtQuick
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: test
    property int phase: 0
    property bool capturing: false
    function findBadge(name) { return icon.children.find(child => child.objectName === name); }
    FloatingWindow {
        visible: true
        implicitWidth: 160
        implicitHeight: 160
        color: "transparent"
        Item {
            id: canvas
            anchors.fill: parent
            AppIcon {
                id: icon
                x: 32; y: 32
                implicitSize: 96
                source: Quickshell.env("BINGUX_TEST_ICON")
                group: ({id: "cutout", displayName: "Cutout"})
                notifications: test.phase < 2 ? Array.from({length: test.phase === 0 ? 1 : 123}, () => ({desktopEntry: "cutout"})) : []
                activeStreams: test.phase === 1 ? [{properties: {"application.id": "cutout"}}] : []
                additionalBadges: [extra]
            }
            DockBadge {
                id: extra
                x: icon.x - 4; y: icon.y - 4
                shown: test.phase === 2
                count: 1
                color: "cyan"
            }
        }
    }
    Timer {
        interval: test.phase === 3 ? 2400 : 600
        running: true
        repeat: true
        onTriggered: {
            if (test.capturing) return;
            test.capturing = true;
            const samples = [];
            for (const badge of [test.findBadge("dockNotificationBadge"), test.findBadge("dockAudioBadge"), extra]) {
                if (!badge.visible) continue;
                const position = canvas.mapFromItem(badge.parent, badge.x, badge.y);
                const bottomBadge = position.y > 80;
                samples.push({opacity: badge.opacity, x: position.x + badge.width / 2,
                    gapY: bottomBadge ? position.y - 1 : position.y + badge.height + 1,
                    iconY: bottomBadge ? position.y - 4 : position.y + badge.height + 4});
            }
            canvas.grabToImage(result => {
                const path = Quickshell.env("BINGUX_TEST_OUTPUT") + "/cutout-" + test.phase + ".png";
                if (!result.saveToFile(path)) { console.error("CUTOUT_SAVE_FAILED"); Qt.quit(); return; }
                console.info("CUTOUT_SAMPLE " + JSON.stringify({phase: test.phase, path, samples}));
                if (++test.phase === 4) Qt.quit();
                test.capturing = false;
            });
        }
    }
}
