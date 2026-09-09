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
    TestPlayer { id: retainedPlayer }
    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        property var gestureArguments: []
        property int captureNumber: 0
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + (Quickshell.env("BINGUX_PANEL_CAPTURE") ? Quickshell.env("BINGUX_PANEL_CAPTURE") + "-" + captureNumber + ".png" : ""), "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, typing) {
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(typing ? [] : ["--click-only"]);
            nativeInput.captureNumber++;
            nativeInput.resultCode = -1; nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000); compare(nativeInput.resultCode, 0);
        }
        function selectPanel(id) {
            console.debug("PANEL_SELECT", id, Date.now());
            const picker = findChild(terminalSidebar.contentItem, "sidebarContentPicker");
            gesture(picker, terminalSidebar.widgetWindow);
            tryCompare(terminalSidebar.contentSelector, "visible", true, 3000);
            tryCompare(terminalSidebar.contentSelector, "revealScale", 1, 3000);
            const choice = findChild(terminalSidebar.contentSelector.body, "sidebar-select-" + id);
            verify(choice !== null);
            gesture(choice, terminalSidebar.contentSelector.nativeWindow);
            tryCompare(terminalSidebar, "contentType", id, 2000);
            tryVerify(() => terminalSidebar.activePanel !== null, 2000);
            wait(100);
            return terminalSidebar.activePanel;
        }
        function test_native_panel_lifetime() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => BinguxPreferences.data.desktop.layoutVersion === 1, 4000);
                terminalSidebar.open(); wait(350);
                const calendar = selectPanel("calendar");
                calendar.serviceEnabled = false;
                calendar.shiftMonth(-2);
                calendar.selectDate(new Date(2026, 6, 12));
                const month = calendar.month;
                const selected = calendar.selectedDate.getTime();
                const tasks = selectPanel("tasks");
                verify(!calendar.visible, "Retained hidden panels stay invisible");
                const input = findChild(tasks, "taskInput");
                verify(input !== null); gesture(input, terminalSidebar.widgetWindow, true);
                compare(input.text, "Find", "Native input reaches the task draft");
                compare(selectPanel("calendar"), calendar, "Switching panels retains the original calendar instance");
                compare(calendar.month, month); compare(calendar.selectedDate.getTime(), selected);
                compare(selectPanel("tasks"), tasks);
                compare(findChild(tasks, "taskInput"), input); compare(input.text, "Find", "Switching panels retains the unsent task");
                const media = selectPanel("media");
                media.players = [retainedPlayer]; wait(200);
                const play = findChild(media, "mediaPlayPause"); verify(play !== null);
                gesture(play, terminalSidebar.widgetWindow); tryCompare(retainedPlayer, "isPlaying", true, 2000);
                const notes = selectPanel("notes");
                verify(!media.visible);
                const monitor = selectPanel("monitor");
                const terminal = selectPanel("terminal");
                verify(!terminal.useFBORendering, "Retained terminals use the image paint path");
                const terminalPid = terminal.shellPid;
                verify(terminalPid > 0);
                for (const entry of [{id:"calendar", panel:calendar}, {id:"tasks", panel:tasks}, {id:"media", panel:media}, {id:"notes", panel:notes}, {id:"monitor", panel:monitor}, {id:"terminal", panel:terminal}]) {
                    compare(selectPanel(entry.id), entry.panel);
                    compare(DesktopEditing.sources[entry.id], entry.panel, "Drag previews refer to the retained native panel");
                    const point = entry.panel.mapToItem(terminalSidebar.contentItem, 0, 0);
                    compare(point.x, Theme.gap * 2); compare(point.y, Theme.barHeight + Theme.gap);
                    compare(entry.panel.width, terminalSidebar.contentItem.width - Theme.gap * 4);
                    compare(entry.panel.height, terminalSidebar.contentItem.height - Theme.barHeight - Theme.gap * 2);
                }
                compare(selectPanel("media"), media);
                compare(terminal.shellPid, terminalPid, "Panel switching keeps the same terminal session");
                compare(findChild(media, "mediaPlayPause"), play);
                gesture(play, terminalSidebar.widgetWindow); tryCompare(retainedPlayer, "isPlaying", false, 2000);
                terminalSidebar.popOut(); tryCompare(terminalSidebar.detachedSurface, "visible", true, 2000); wait(250);
                compare(terminalSidebar.activePanel, media);
                compare(media.Window.window, terminalSidebar.detachedSurface.contentItem.Window.window);
                mouseClick(play, play.width / 2, play.height / 2); tryCompare(retainedPlayer, "isPlaying", true, 2000);
                terminalSidebar.dockBack(); wait(350);
                binguxSettings.read(); tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                editor.open(); wait(350);
                editor.put("media", "sidebar", 0); editor.undo(); editor.redo(); wait(150);
                compare(terminalSidebar.activePanel, media);
                editor.cancel(); wait(350);
                compare(terminalSidebar.activePanel, media);
                compare(selectPanel("tasks"), tasks); compare(input.text, "Find");
                compare(selectPanel("calendar"), calendar); compare(calendar.selectedDate.getTime(), selected);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
