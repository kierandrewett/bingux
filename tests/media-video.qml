import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    component FakePlayer: QtObject {
        property string identity: "Music"
        property string desktopEntry: ""
        property string trackTitle: "A Quiet Evening"
        property string trackArtist: "Local library"
        property string trackArtUrl: Quickshell.shellPath("avatar.svg")
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
        function play() {
            isPlaying = true;
        }
        function pause() {
            isPlaying = false;
        }
        property var calls: []
        property var metadata: ({})
        function previous() {
            calls = calls.concat("previous");
        }
        function next() {
            calls = calls.concat("next");
        }
        function seek(offset) {
            calls = calls.concat(offset);
            position += offset;
        }
    }
    FakePlayer {
        id: first
    }
    FakePlayer {
        id: second
        identity: "Browser"
        uniqueId: 2
    }
    FloatingWindow {
        id: window
        implicitWidth: 416
        implicitHeight: 760
        color: Theme.shellSurface
        Item {
            id: canvas
            anchors.fill: parent
            MediaControls {
                id: media
                x: 16
                y: 16
                width: 384
                height: implicitHeight
                compact: true
                menuActive: true
                player: first
                playerOptions: [first, second]
                onPlayerSelected: selectedPlayer => player = selectedPlayer
            }
        }
        TestCase {
            id: test
            when: window.visible
            function cleanupTestCase() {
                if (qtest_results.failCount)
                    report.setText(report.text() + "FAILURES " + qtest_results.failCount + "\n");
            }
            function test_video_controls() {
                report.setText("RUNNING\n");
                waitForRendering(media);
                const next = findChild(media, "mediaNext");
                const previous = findChild(media, "mediaPrevious");
                compare(next.text, "Next track");
                mouseClick(next);
                compare(first.calls[0], "next");
                wait(250);
                first.length = 2003;
                tryCompare(next, "text", "Forward 10 seconds");
                compare(next.displayedIcon, "media-seek-forward-symbolic");
                mouseClick(next);
                mouseClick(previous);
                compare(first.calls[1], 10);
                compare(first.calls[2], -10);
                first.canSeek = false;
                tryCompare(next, "enabled", false);
                verify(!previous.enabled);
                first.canSeek = true;
                first.length = 180;
                first.metadata = {
                    "xesam:url": "https://www.youtube.com/watch?v=example"
                };
                tryCompare(next, "text", "Forward 10 seconds");
                first.metadata = {};
                tryCompare(next, "text", "Next track");
                compare(next.displayedIcon, "media-skip-forward-symbolic");
                compare(previous.text, "Previous track");
                report.setText("PASS music track controls, long-form seek clicks, disabled seeking, video provider and icon changes\nFAILURES 0\n");
            }
        }
    }
}
