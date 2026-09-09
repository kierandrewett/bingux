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
        function gesture(item, window, options) {
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options || ["--click-only"]);
            nativeInput.captureNumber++;
            nativeInput.resultCode = -1; nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000); compare(nativeInput.resultCode, 0);
        }
        function launch(id) {
            const widget = terminalSidebar.panelWidget(id);
            gesture(findChild(widget, "sidebar-panel-launcher-" + id), widget.barWindow);
            tryCompare(widget.popup, "visible", true, 3000);
            tryCompare(widget.popup, "revealScale", 1, 3000);
            if (Quickshell.env("BINGUX_PANEL_CAPTURE")) gesture(findChild(widget, "sidebar-panel-launcher-" + id), widget.barWindow, ["--hover-only"]);
            return widget;
        }
        function test_panel_placement() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                terminalSidebar.open(); wait(350);
                const originals = {};
                for (const id of ["terminal", "calendar", "tasks", "media", "monitor", "notes"]) {
                    terminalSidebar.selectContent(id);
                    tryVerify(() => terminalSidebar.panelFor(id) !== null, 3000);
                    originals[id] = terminalSidebar.panelFor(id);
                    verify(waitForRendering(originals[id], 2000), "The selected panel renders before the next selection");
                }
                const terminalPid = originals.terminal.shellPid;
                originals.calendar.serviceEnabled = false;
                originals.calendar.selectDate(new Date(2026, 6, 12));
                const selected = originals.calendar.selectedDate.getTime();
                originals.media.players = [retainedPlayer];
                const input = findChild(originals.tasks, "taskInput"); input.text = "Keep my task draft";
                const oldSize = Qt.size(originals.notes.width, originals.notes.height);
                const editor = desktopCustomiser;
                editor.open(); tryCompare(controlCentre, "visible", true, 4000); tryCompare(controlCentre, "revealScale", 1, 4000); wait(350);
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                gesture(originals.notes, terminalSidebar.widgetWindow, ["--drag-to", String(dockArea.screenRect.x + dockArea.screenRect.width / 2), String(dockArea.screenRect.y + dockArea.screenRect.height / 2)]);
                compare(editor.containerFor("notes"), "dock", "Native drag moves Notes into the dock");
                editor.undo(); compare(editor.containerFor("notes"), "sidebar");
                editor.redo(); compare(editor.containerFor("notes"), "dock");
                for (const id of Object.keys(originals)) {
                    for (const target of ["top-center", "dock", "control-centre", "sidebar"]) {
                        editor.put(id, target, 0);
                        const widget = terminalSidebar.panelWidget(id);
                        compare(widget.container, target);
                        compare(terminalSidebar.panelFor(id), originals[id]);
                        if (target === "sidebar") compare(terminalSidebar.contentType, id);
                        else verify(waitForRendering(widget, 2000));
                        if (id === "media" && target === "control-centre")
                            verify(widget.contentHost.height >= originals.media.contentHeight - 0.5, "The inline media frame fits its controls below the heading");
                    }
                }
                const destinations = {notes: "dock", terminal: "top-right", calendar: "control-centre", tasks: "top-left", media: "dock", monitor: "control-centre"};
                for (const id of Object.keys(destinations)) editor.put(id, destinations[id], 0);
                wait(250);
                compare(terminalSidebar.contentType, "", "The sidebar has no phantom fallback panel");
                compare(terminalSidebar.activePanel, null);
                for (const id of Object.keys(originals)) {
                    compare(terminalSidebar.panelFor(id), originals[id]);
                    compare(DesktopEditing.sources[id], originals[id]);
                    const widget = terminalSidebar.panelWidget(id);
                    compare(widget.container, destinations[id]);
                    verify(widget.width > 0 && widget.height > 0);
                    if (widget.inlinePanel) verify(originals[id].parent === widget.contentHost || originals[id].parent.parent === widget.contentHost);
                }
                editor.selectedContainer = "control-centre"; editor.containerDisplay("text");
                compare(terminalSidebar.panelWidget("calendar").presentation.showIcon, false);
                editor.selectedWidget = "calendar"; editor.widgetOption("display", "both");
                compare(terminalSidebar.panelWidget("calendar").presentation.showIcon, true);
                editor.cancel(); wait(400);
                compare(terminalSidebar.activePanel, originals.notes);
                compare(Qt.size(originals.notes.width, originals.notes.height), oldSize);
                compare(input.text, "Keep my task draft");
                compare(originals.calendar.selectedDate.getTime(), selected);
                editor.open(); wait(350);
                for (const id of Object.keys(destinations)) editor.put(id, destinations[id], 0);
                editor.apply(); tryCompare(editor, "visible", false, 4000); wait(350);
                terminalSidebar.hide(); wait(350);
                const notesWidget = terminalSidebar.panelWidget("notes");
                gesture(notesWidget, dock, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.widgetId, "notes"); compare(widgetMenu.anchorWindow, dock);
                widgetMenu.visible = false; wait(200);
                for (const id of ["notes", "tasks", "media", "terminal"]) {
                    const widget = launch(id);
                    compare(terminalSidebar.panelFor(id), originals[id]);
                    compare(originals[id].Window.window, widget.popup.nativeWindow.contentItem.Window.window);
                    verify(originals[id].visible && originals[id].width > 200 && originals[id].height > 100);
                    if (id === "notes") {
                        originals.notes.showContextMenu(Qt.point(20, 20)); wait(100);
                        const menu = findChild(originals.notes, "notesContextMenu");
                        verify(menu !== null); compare(menu.hostItem, widget.popup.contentItem);
                        compare(menu.visible, true); originals.notes.closeContextMenu();
                    }
                    if (id === "media") {
                        gesture(findChild(originals.media, "mediaPlayPause"), widget.popup.nativeWindow);
                        tryCompare(retainedPlayer, "isPlaying", true, 2000);
                    }
                    if (id === "tasks") {
                        gesture(input, widget.popup.nativeWindow);
                        verify(input.activeFocus, "The moved task input still accepts keyboard focus");
                        originals.tasks.addTask("Check the moved task control");
                        tryVerify(() => !!findChild(originals.tasks, "taskCheck0"), 1500);
                        const check = findChild(originals.tasks, "taskCheck0");
                        verify(waitForRendering(check, 2000));
                        compare(check.focusPolicy, Qt.StrongFocus, "Real task controls retain their focus policy");
                        gesture(check, widget.popup.nativeWindow);
                        verify(originals.tasks.tasks[0].done, "The native checkbox still updates its task");
                        originals.tasks.removeTask(0);
                    }
                    widget.popup.visible = false; wait(180);
                }
                compare(originals.terminal.shellPid, terminalPid);
                compare(input.text, "Keep my task draft");
                compare(originals.calendar.selectedDate.getTime(), selected);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
