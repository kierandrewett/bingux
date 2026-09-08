import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 100; onTriggered: Qt.quit() }
    QtObject {
        id: player
        property string identity: "Test"
        property string trackTitle: "Track"
        property string trackArtist: "Artist"
        property string trackArtUrl: ""
        property bool positionSupported: true
        property bool lengthSupported: true
        property bool canControl: false
        property bool canSeek: false
        property bool canGoNext: false
        property bool canGoPrevious: false
        property bool canPlay: false
        property bool canPause: false
        property bool isPlaying: true
        property real length: 300
        property real position: 42
    }
    FloatingWindow {
        implicitWidth: 400; implicitHeight: 500
        MediaControls { id: controls; width: 320; player: player; menuActive: false }
        AlbumArtwork { id: art; width: 100; height: 100; active: false }
        TestCase {
            when: true
            SignalSpy { id: positionSpy; target: player; signalName: "positionChanged" }
            function test_preload() {
                art.source = Qt.resolvedUrl("icons/format-code-symbolic.svg");
                tryCompare(art, "status", Image.Ready);
                compare(art.front.opacity, 1);
                art.source = Qt.resolvedUrl("icons/format-quote-symbolic.svg");
                tryVerify(() => art.front.source.toString() === art.source && art.status === Image.Ready);
                compare(art.front.opacity, 1);
                positionSpy.clear();
                controls.menuActive = true;
                compare(positionSpy.count, 1);
                art.active = true;
                compare(art.front.opacity, 1);
            }
            function test_sliding_times() {
                controls.menuActive = true;
                const elapsed = findChild(controls, "mediaElapsedDigits");
                const duration = findChild(controls, "mediaDurationDigits");
                verify(elapsed !== null && duration !== null);
                player.position = 43;
                tryVerify(() => elapsed.animating, 100);
                wait(100);
                verify(elapsed.animating, "Media digits remain in motion long enough to see");
                verify(elapsed.width >= elapsed.implicitWidth, "Elapsed digits have enough visible width");
                const glyph = findChild(elapsed, "rollingGlyph3");
                const oldDigit = findChild(glyph, "oldRollingDigit");
                verify(oldDigit.y < (glyph.height - oldDigit.height) / 2, "Outgoing digit slides upwards");
                if (Quickshell.env("MEDIA_ROLL_FRAME")) grabImage(controls).save(Quickshell.env("MEDIA_ROLL_FRAME"));
                tryCompare(elapsed, "displayedText", "0:43", 500);
                controls.showRemaining = true;
                tryCompare(elapsed, "displayedText", "-4:17", 500);
                player.position = 44;
                tryVerify(() => elapsed.animating, 100);
                compare(elapsed.direction, -1);
                tryCompare(elapsed, "displayedText", "-4:16", 500);
                player.length = 301;
                tryCompare(duration, "displayedText", "5:01", 500);
                controls.menuActive = false;
                player.position = 60;
                tryCompare(elapsed, "displayedText", "-4:01", 100);
                verify(!elapsed.animating);
            }
            function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n"); finish.start(); }
        }
    }
}
