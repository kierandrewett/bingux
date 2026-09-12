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
        function play() {
            isPlaying = true;
        }
        function pause() {
            isPlaying = false;
        }
        function next() {
            nextCount++;
            uniqueId++;
        }
        function previous() {
            previousCount++;
            uniqueId++;
        }
    }
    TestPlayer {
        id: firstPlayer
    }
    TestPlayer {
        id: secondPlayer
        identity: "Second player"
        trackTitle: "Another track"
    }
    QtObject {
        id: outputAudio
        property bool muted: false
        property real volume: 0.65
    }
    QtObject {
        id: inputAudio
        property bool muted: false
        property real volume: 0.4
    }
    QtObject {
        id: testOutput
        property bool ready: true
        property var audio: outputAudio
    }
    QtObject {
        id: testInput
        property bool ready: true
        property var audio: inputAudio
    }
    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector {
            onStreamFinished: if (text)
                console.warn("NATIVE_INPUT", text)
        }
        property var gestureArguments: []
        property string capturePath: ""
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    FileView {
        id: actionReport
        path: Quickshell.env("BINGUX_ACTION_REPORT")
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, x, y, options) {
            const point = DesktopEditing.point(item, window, x, y);
            nativeInput.gestureArguments = [point.x.toString(), point.y.toString()].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0);
        }
        function drag(item, window, x, y, destination) {
            gesture(item, window, x, y, ["--drag-to", destination.x.toString(), destination.y.toString()]);
            tryCompare(desktopCustomiser, "draggedId", "", 4000);
            wait(150);
        }
        function capture(name, item, window) {
            const prefix = Quickshell.env("BINGUX_GROUP_CAPTURE");
            if (!prefix)
                return;
            nativeInput.capturePath = prefix + "-" + name + ".png";
            gesture(item, window, item.width / 2, item.height / 2, ["--hover-only"]);
            nativeInput.capturePath = "";
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
            tryCompare(dock.margins, "bottom", 0, 2000);
            wait(100);
        }
        function revealWidget(item) {
            const viewport = terminalSidebar.widgetViewport;
            const point = item.mapToItem(terminalSidebar.widgetHost, 0, 0);
            viewport.contentY = Math.max(0, Math.min(point.y - 12, viewport.contentHeight - viewport.height));
            wait(120);
        }
        function test_sidebar_controls() {
            console.debug("SIDEBAR_CONTROLS_START");
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                terminalSidebar.selectContent("notes");
                terminalSidebar.open();
                wait(300);
                const editor = desktopCustomiser;
                editor.open();
                tryCompare(controlCentre, "revealScale", 1, 4000);
                wait(300);
                const byId = id => controlCentre.movableWidgets.find(item => item.widgetId === id);
                const header = byId("controls-header"), audio = byId("controls-audio"), quick = byId("controls-tiles");
                const output = byId("control-volume"), input = byId("control-microphone"), settings = byId("control-settings");
                const media = byId("control-media"), network = byId("control-network");
                output.node = testOutput;
                input.node = testInput;
                media.player = firstPlayer;
                const play = findChild(media, "mediaPlayPause");
                const nativeAudioHeight = audio.height;
                const centre = DesktopEditing.surfaces.find(surface => surface.zoneName === "control-centre");
                const sidebar = DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar");
                const grip = findChild(centre, "customise-group-handle-controls-audio");
                drag(grip, controlCentre.nativeWindow, grip.width / 2, grip.height / 2, Qt.point(sidebar.screenRect.x + 40, sidebar.screenRect.y + Theme.barHeight + 12));
                compare(audio.parent, terminalSidebar.widgetHost);
                verify(!audio.barLayout && !output.barLayout);
                compare(audio.height, nativeAudioHeight);
                compare(output.parent, audio);
                compare(input.parent, audio);
                editor.undo();
                wait(100);
                verify(audio.parent !== terminalSidebar.widgetHost);
                editor.redo();
                wait(100);
                compare(audio.parent, terminalSidebar.widgetHost);
                for (const item of controlCentre.movableWidgets) {
                    editor.put(item.widgetId, "sidebar", 0);
                    wait(30);
                    compare(item.parent, terminalSidebar.widgetHost);
                    verify(!item.barLayout);
                }
                verify(terminalSidebar.widgetViewport.contentHeight > terminalSidebar.widgetViewport.height);
                verify(terminalSidebar.activePanel.height >= 200);
                editor.cancel();
                wait(300);
                compare(output.parent, audio);
                compare(settings.parent, header);
                compare(network.parent, quick);
                editor.open();
                wait(300);
                for (const id of ["controls-header", "controls-audio", "controls-tiles", "control-media", "control-divider", "control-customise"])
                    editor.put(id, "sidebar", editor.layout.sidebar.indexOf("notes"));
                wait(250);
                verify(controlCentre.controlsHeight >= 96, "An empty control centre keeps a usable drop target");
                const viewport = terminalSidebar.widgetViewport;
                console.debug("SIDEBAR_BEFORE_WHEEL", viewport.width, viewport.height, viewport.contentHeight);
                mouseWheel(viewport, viewport.width - 4, viewport.height / 2, 0, -120);
                console.debug("SIDEBAR_AFTER_WHEEL", viewport.contentY);
                tryVerify(() => viewport.contentY > 0, 2000, "The sidebar scrolls while editing");
                viewport.cancelFlick();
                viewport.contentY = 0;
                wait(100);
                const quickGrip = findChild(sidebar, "customise-group-handle-controls-tiles");
                const headerGrip = findChild(sidebar, "customise-group-handle-controls-header");
                const gripY = quickGrip.y;
                terminalSidebar.widgetViewport.contentY = 64;
                wait(100);
                compare(Math.round(quickGrip.y), Math.round(gripY - 64), "Group handles follow scrolling");
                verify(!headerGrip.visible, "Offscreen groups do not leave a handle over the visible controls");
                terminalSidebar.widgetViewport.contentY = 0;
                wait(100);
                editor.selectedContainer = "sidebar";
                editor.containerDisplay("both");
                wait(100);
                capture("sidebar-header-labels", header, terminalSidebar.editWindow);
                verify(header.width <= terminalSidebar.widgetViewport.width, "The header fits the sidebar with text labels");
                for (const entry of header.memberEntries.filter(entry => entry.item.visible))
                    verify(entry.item.width >= entry.item.implicitWidth && entry.item.x + entry.item.width <= header.width, "Header labels fit without overlap: " + entry.id);
                editor.selectedContainer = "controls-header";
                editor.containerDisplay("icons");
                editor.selectedWidget = "control-settings";
                editor.widgetOption("display", "both");
                editor.widgetOption("label", "Prefs");
                save();
                gesture(systemPill, topBar, systemPill.width / 2, systemPill.height / 2, ["--click-only"]);
                tryCompare(controlCentre, "visible", true, 3000);
                verify(controlCentre.controlsHeight >= 96, "An empty menu has a readable state outside the editor");
                controlCentre.visible = false;
                wait(200);
                compare(settings.presentation.label, "Prefs");
                verify(settings.presentation.showText && settings.presentation.showIcon);
                verify(!settings.barLayout && !audio.barLayout && !network.barLayout && !media.barLayout);
                compare(findChild(media, "mediaPlayPause"), play);
                revealWidget(settings);
                gesture(settings, terminalSidebar.editWindow, settings.width / 2, settings.height / 2, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.anchorWindow, terminalSidebar.editWindow);
                widgetMenu.visible = false;
                gesture(settings, terminalSidebar.editWindow, settings.width / 2, settings.height / 2, ["--click-only"]);
                tryVerify(() => actionReport.text().includes("gnome-control-center"), 3000);
                revealWidget(output);
                const mute = findChild(output, "controlMute"), slider = findChild(output, "controlVolume");
                gesture(mute, terminalSidebar.editWindow, mute.width / 2, mute.height / 2, ["--click-only"]);
                verify(outputAudio.muted);
                gesture(slider, terminalSidebar.editWindow, slider.width * .7, slider.height / 2, ["--click-only"]);
                verify(!outputAudio.muted && outputAudio.volume > .9);
                const nav = findChild(output, "controlSoundDetails");
                gesture(nav, terminalSidebar.editWindow, nav.width / 2, nav.height / 2, ["--click-only"]);
                tryCompare(controlCentre, "visible", true, 3000);
                compare(controlCentre.detailPage, "audio");
                compare(controlCentre.movedAnchor, output);
                compare(controlCentre.anchorWindow, terminalSidebar.editWindow);
                controlCentre.visible = false;
                wait(200);
                revealWidget(network);
                const navigation = findChild(network, "controlNetworkNavigation");
                verify(navigation);
                gesture(navigation, terminalSidebar.editWindow, navigation.width / 2, navigation.height / 2, ["--click-only"]);
                tryCompare(controlCentre, "visible", true, 3000);
                compare(controlCentre.detailPage, "network");
                compare(controlCentre.movedAnchor, network);
                controlCentre.visible = false;
                wait(200);
                revealWidget(play);
                gesture(play, terminalSidebar.editWindow, play.width / 2, play.height / 2, ["--click-only"]);
                verify(firstPlayer.isPlaying);
                capture("sidebar-native-controls", media, terminalSidebar.editWindow);
                terminalSidebar.popOut();
                tryCompare(terminalSidebar.detachedSurface, "visible", true, 3000);
                wait(200);
                revealWidget(output);
                mouseClick(nav, nav.width / 2, nav.height / 2);
                tryCompare(controlCentre, "visible", true, 3000);
                compare(controlCentre.hostItem, terminalSidebar.detachedSurface.contentItem);
                compare(controlCentre.movedAnchor, output);
                controlCentre.visible = false;
                terminalSidebar.dockBack();
                wait(200);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
