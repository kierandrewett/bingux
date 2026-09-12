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
        function previous() {
        }
        function next() {
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
            property bool saved: false
            when: window.visible
            function cleanupTestCase() {
                if (qtest_results.failCount)
                    report.setText(report.text() + "FAILURES " + qtest_results.failCount + "\n");
            }
            function test_switcher() {
                report.setText("RUNNING\n");
                waitForRendering(media);
                const selector = findChild(media, "mediaPlayerSelector");
                const surface = findChild(media, "mediaCardSurface");
                const art = findChild(media, "mediaArtButton");
                const selectorY = selector.y;
                const playerName = findChild(media, "mediaPlayerIdentity");
                fuzzyCompare(playerName.x + playerName.width / 2, selector.width / 2, 0.5, "Player identity is centred independently of the counter");
                verify(findChild(media, "mediaPlayerAppIcon") !== null, "Player identity includes its OS app icon");
                mouseClick(art);
                for (let frame = 0; frame < 20; frame++) {
                    wait(16);
                    compare(selector.y, selectorY, "Switcher remains in a stable header");
                    verify(art.y >= selector.y + selector.height + Theme.gap - 1, "Switcher never overlaps the artwork");
                    compare(surface.y, 0, "Card background stays behind the switcher");
                }
                compare(media.artExpansion, 1);
                compare(art.width, media.width - Theme.padding * 2);
                mouseClick(findChild(media, "mediaNextPlayer"));
                compare(media.player, second, "Switcher is clickable with expanded artwork");
                const installedBrowser = DesktopEntries.heuristicLookup("Helium");
                if (installedBrowser) {
                    second.identity = "Helium";
                    tryCompare(findChild(media, "mediaPlayerAppIcon"), "source", installedBrowser.icon);
                    tryVerify(() => findChild(media, "mediaPlayerAppIcon").resolvedSource.length > 0, 4000);
                    wait(150); // Allow the asynchronously loaded OS icon to paint.
                    console.log("Verified OS provider icon:", installedBrowser.id, installedBrowser.icon);
                }
                fuzzyCompare(playerName.x + playerName.width / 2, selector.width / 2, 0.5, "Expanded switcher identity remains centred");
                compare(media.artworkExpanded, true);
                canvas.grabToImage(result => {
                    test.saved = result.saveToFile("/tmp/bingux-media-switcher.png");
                });
                tryCompare(test, "saved", true, 2000);
                mouseClick(art);
                tryCompare(media, "artExpansion", 0, 500);
                media.playerOptions = [second];
                verify(!selector.visible);
                compare(media.playerSelectorHeight, 0, "Single-player mode leaves no empty switcher space");
                report.setText("PASS stable switcher header, artwork separation, expanded player switching, collapse and single-player layout\nFAILURES 0\n");
            }
        }
    }
}
