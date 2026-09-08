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
            function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n"); finish.start(); }
        }
    }
}
