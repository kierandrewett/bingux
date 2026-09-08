    component TestPlayer: QtObject {
        property string identity: "Music"
        property string trackTitle: "Sample track"
        property string trackArtist: "Sample artist"
        property string trackArtUrl: ""
        property real position: 42
        property real length: 180
        property int uniqueId: 1
        property bool positionSupported: true
        property bool lengthSupported: true
        property bool canSeek: true
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canGoPrevious: true
        property bool canGoNext: true
        property bool isPlaying: false
        property int nextCount: 0
        property int previousCount: 0
        function play() { isPlaying = true; }
        function pause() { isPlaying = false; }
        function next() { nextCount++; uniqueId++; }
        function previous() { previousCount++; uniqueId++; }
    }
    TestPlayer { id: firstPlayer }
    TestPlayer { id: secondPlayer; identity: "Second player"; trackTitle: "Another track" }
    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        property var gestureArguments: []
        property string capturePath: ""
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + nativeInput.capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    TestCase {
        id: controlMediaTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, x, y, options) {
            const point = DesktopEditing.point(item, window, x, y);
            nativeInput.gestureArguments = [point.x.toString(), point.y.toString()].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0, "The compositor accepts the native gesture");
        }
        function drag(item, x, y, destination) {
            gesture(item, controlCentre.nativeWindow, x, y, ["--drag-to", destination.x.toString(), destination.y.toString()]);
            tryCompare(desktopCustomiser, "draggedId", "", 4000);
            wait(150);
        }
        function test_native_media() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                controlCentre.mediaPlayers = [firstPlayer, secondPlayer];
                const editor = desktopCustomiser;
                editor.open();
                tryCompare(controlCentre, "visible", true, 3000);
                tryCompare(controlCentre, "revealScale", 1, 4000); wait(300);
                const media = findChild(controlCentre.body, "controlMediaCard");
                const originalParent = media.parent;
                const play = findChild(media, "mediaPlayPause");
                const next = findChild(media, "mediaNext");
                const previous = findChild(media, "mediaPrevious");
                const transport = play.parent;
                const art = findChild(media, "mediaArtButton");
                const seek = findChild(media, "mediaSeek");
                const elapsed = findChild(media, "mediaElapsed");
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                drag(media, media.width - 4, 4,
                    Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                compare(media.parent, dock.widgetHost);
                verify(!editor.desktop.controlLayout.groups["control-centre"].includes("control-media"));
                compare(findChild(media, "mediaPlayPause"), play);
                verify(play.parent !== transport && media.inlineControls);
                editor.undo(); wait(200); compare(media.parent, originalParent); compare(play.parent, transport);
                editor.redo(); wait(200); compare(media.parent, dock.widgetHost);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000); wait(300);
                compare(BinguxPreferences.data.desktop.layout.dock[0], "control-media");
                tryCompare(dock.margins, "bottom", 0, 2000, "The dock finishes moving out of the editor before native input");
                nativeInput.capturePath = Quickshell.env("BINGUX_NATIVE_SCREENSHOT");
                gesture(play, dock, play.width / 2, play.height / 2, ["--hover-only"]);
                nativeInput.capturePath = "";
                tryCompare(play, "hovered", true, 1000, "The pointer reaches the compact player in the dock");
                gesture(play, dock, play.width / 2, play.height / 2, ["--click-only"]);
                verify(firstPlayer.isPlaying, "The moved play button controls the original player");
                gesture(next, dock, next.width / 2, next.height / 2, ["--click-only"]);
                compare(firstPlayer.nextCount, 1);
                gesture(previous, dock, previous.width / 2, previous.height / 2, ["--click-only"]);
                compare(firstPlayer.previousCount, 1);
                const openDetails = findChild(media, "mediaOpenDetails");
                gesture(openDetails, dock, openDetails.width / 2, openDetails.height / 2, ["--click-only"]);
                const popup = media.detailPopup;
                tryCompare(popup, "visible", true, 3000);
                tryCompare(popup, "revealScale", 1, 3000); wait(250);
                nativeInput.capturePath = Quickshell.env("BINGUX_NATIVE_SCREENSHOT");
                gesture(seek, popup.nativeWindow, seek.width / 2, seek.height / 2, ["--hover-only"]);
                nativeInput.capturePath = "";
                tryCompare(seek, "hovered", true, 1000, "The pointer reaches the full player in its popup");
                compare(play.parent, transport, "The same playback buttons move into the full player");
                compare(popup.anchorItem, media); verify(popup.anchorAbove);
                mouseClick(seek, seek.width / 2, seek.height / 2, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "control-media");
                compare(widgetMenu.anchorWindow, popup.nativeWindow, "Editing from the full player anchors its menu to that popup");
                widgetMenu.visible = false; wait(300);
                gesture(seek, popup.nativeWindow, seek.width * 0.75, seek.height / 2, ["--click-only"]);
                verify(firstPlayer.position > firstPlayer.length * 0.6, "Seeking works through the actual dock popup");
                gesture(elapsed, popup.nativeWindow, elapsed.width / 2, elapsed.height / 2, ["--click-only"]);
                verify(media.showRemaining);
                const cycle = findChild(media, "mediaNextPlayer");
                gesture(cycle, popup.nativeWindow, cycle.width / 2, cycle.height / 2, ["--click-only"]);
                compare(media.player, secondPlayer, "The existing player selector keeps its service binding");
                gesture(art, popup.nativeWindow, art.width / 2, art.height / 2, ["--click-only"]);
                tryCompare(media, "artExpansion", 1, 1500);
                compare(art.width, popup.popupWidth - Theme.padding * 2);
                popup.visible = false; tryCompare(media, "inlineControls", true, 3000);
                editor.open(); editor.put("control-media", "top-left", 0);
                editor.selectedContainer = "top-left"; editor.containerDisplay("icons");
                verify(media.presentation.showIcon && !media.presentation.showText);
                verify(media.implicitWidth < 320, "Icons-only mode removes the reserved title space");
                editor.selectedWidget = "control-media"; editor.widgetOption("label", "Now playing");
                verify(findChild(media, "mediaArtwork").visible, "A label override retains the album artwork");
                editor.widgetOption("icon", "starred-symbolic"); editor.widgetOption("display", "both");
                compare(findChild(media, "mediaTitle").text, "Now playing");
                verify(findChild(media, "mediaOverrideIcon").visible);
                editor.widgetOption("label", ""); editor.widgetOption("icon", ""); editor.widgetOption("display", "inherit");
                editor.containerDisplay("native");
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000); wait(300);
                compare(media.parent, leftControls);
                mouseClick(media, media.width / 2, media.height / 2, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "control-media");
                widgetMenu.visible = false; wait(300);
                gesture(play, topBar, play.width / 2, play.height / 2, ["--click-only"]);
                verify(secondPlayer.isPlaying);
                gesture(openDetails, topBar, openDetails.width / 2, openDetails.height / 2, ["--click-only"]);
                tryCompare(popup, "visible", true, 3000); verify(!popup.anchorAbove);
                editor.open();
                tryCompare(popup, "visible", false, 3000, "Opening the editor dismisses the widget popup");
                editor.put("control-media", "control-centre", 4);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                compare(media.parent, originalParent); compare(play.parent, transport);
                compare(findChild(media, "mediaArtButton"), art);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
