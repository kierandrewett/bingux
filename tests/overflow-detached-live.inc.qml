    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeCapture
        property string capturePath: ""
        property var gestureArguments: ["1", "1", "--hover-only"]
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3",
            Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeCapture.gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function click(item, window, button, modifiers) {
            tryVerify(() => item.width > 0 && item.height > 0, 3000);
            let frame = null;
            verify(item.grabToImage(result => frame = result));
            tryVerify(() => frame !== null, 3000);
            const floating = !("anchors" in window);
            const point = floating ? item.mapToItem(window.contentItem, item.width / 2, item.height / 2)
                : DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            const action = modifiers === Qt.ShiftModifier ? "--shift-right-click"
                : button === Qt.RightButton ? "--right-click" : "--click-only";
            nativeCapture.gestureArguments = [String(point.x), String(point.y), action]
                .concat(floating ? ["--window-title", window.title, "--window-size", String(window.width), String(window.height)] : []);
            nativeCapture.running = true;
            tryCompare(nativeCapture, "running", false, 4000);
            compare(nativeCapture.resultCode, 0);
        }
        function openMore() {
            click(overflowButton, topBar.windowFor(overflowButton));
            tryCompare(barOverflow, "visible", true, 3000);
            tryCompare(barOverflow, "revealScale", 1, 3000);
        }
        function resize(window, width, height) {
            nativeCapture.gestureArguments = ["0", "0", "--resize-to", String(width), String(height),
                "--window-title", window.title, "--window-size", String(window.width), String(window.height)];
            nativeCapture.running = true;
            tryCompare(nativeCapture, "running", false, 4000);
            compare(nativeCapture.resultCode, 0);
            tryCompare(window, "width", width, 3000);
            tryCompare(window, "height", height, 3000);
        }
        function checkContentFits(viewport) {
            const items = metricsPill.monitorNames.map(name => findChild(metricsPill, name + "Readout"))
                .concat(tray.trayItems.map(item => findChild(tray, "trayItem-" + item.id)));
            for (const item of items) {
                tryVerify(() => {
                    const point = item.mapToItem(viewport.contentItem, 0, 0);
                    return point.x >= 0 && point.x + item.width <= viewport.width + 1
                        && point.y >= 0 && point.y + item.height <= viewport.contentHeight + 1;
                }, 2000, item.objectName + " fits the scrollable content");
            }
        }
        function scroll(viewport, window, direction) {
            const point = viewport.mapToItem(window.contentItem, viewport.width / 2, viewport.height / 2);
            nativeCapture.gestureArguments = [String(point.x), String(point.y), "--scroll-" + direction,
                "--window-title", window.title, "--window-size", String(window.width), String(window.height)];
            nativeCapture.running = true;
            tryCompare(nativeCapture, "running", false, 4000);
            compare(nativeCapture.resultCode, 0);
        }
        function capture(name) {
            const prefix = Quickshell.env("BINGUX_GROUP_CAPTURE");
            if (!prefix) return;
            nativeCapture.gestureArguments = ["1", "1", "--capture-only"];
            nativeCapture.capturePath = prefix + "-" + name + ".png";
            nativeCapture.running = true;
            tryCompare(nativeCapture, "running", false, 4000);
            compare(nativeCapture.resultCode, 0);
            nativeCapture.capturePath = "";
        }
        function test_detached_overflow() {
            try {
                terminalSidebar.selectContent("notes");
                tray.serviceEnabled = false;
                tray.trayItems = Array.from({length: 18}, (_, index) => ({
                    id: "detached-" + index, title: "Application " + index,
                    icon: Quickshell.iconPath("applications-other"), menu: null, hasMenu: true, onlyMenu: true,
                }));
                metricsPill.preferencesLocation = Qt.resolvedUrl("detached-monitors.ini");
                metricsPill.setShown("cpu", true);
                metricsPill.previewMonitors = metricsPill.monitorNames;
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                editor.open();
                tryCompare(controlCentre, "revealScale", 1, 3000);
                editor.put("clock", "sidebar", 0);
                editor.put("overflow", "sidebar", 1);
                editor.apply();
                tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                terminalSidebar.popOut();
                const window = terminalSidebar.detachedSurface;
                tryCompare(window, "visible", true, 3000);
                let initialFrame = null;
                verify(overflowButton.grabToImage(result => initialFrame = result));
                tryVerify(() => initialFrame !== null, 3000);
                resize(window, window.width, 360);
                tryCompare(overflowButton, "parent", terminalSidebar.widgetHost);
                tryVerify(() => topBar.overflows(trayContainer), 3000);
                tryVerify(() => topBar.overflows(metricsPill), 3000);
                wait(300);
                openMore();
                compare(barOverflow.hostItem, window.contentItem);
                compare(topBar.windowFor(trayContainer), window);
                const first = findChild(tray, "trayItem-detached-0");
                const viewport = findChild(barOverflow.body, "overflowViewport");
                verify(viewport !== null);
                const barWidth = metricsPill.implicitWidth;
                for (const width of [384, 224, 180]) {
                    resize(window, width, 360);
                    checkContentFits(viewport);
                    compare(metricsPill.implicitWidth, barWidth, "Wrapping keeps overflow membership stable");
                    verify(topBar.overflows(metricsPill) && topBar.overflows(trayContainer));
                    compare(findChild(tray, "trayItem-detached-0"), first, "Resizing keeps the native item");
                }
                verify(viewport.interactive, "The narrow menu scrolls vertically");
                first.forceActiveFocus(Qt.TabFocusReason);
                tryVerify(() => first.mapToItem(viewport, 0, 0).y >= 0, 2000);
                metricsPill.forceActiveFocus(Qt.TabFocusReason);
                tryVerify(() => viewport.contentY > 0, 2000, "Keyboard focus reveals the monitors");
                capture("narrow-monitors");
                first.forceActiveFocus(Qt.BacktabFocusReason);
                tryVerify(() => {
                    const y = first.mapToItem(viewport, 0, 0).y;
                    return y >= 0 && y + first.height <= viewport.height;
                }, 2000, "Reverse focus reveals the first tray item");
                capture("narrow-tray");
                const beforeScroll = viewport.contentY;
                scroll(viewport, window, "down");
                tryVerify(() => viewport.contentY > beforeScroll, 2000, "The native wheel scrolls More");
                scroll(viewport, window, "down");
                tryCompare(viewport, "moving", false, 2000);
                verify(viewport.contentY <= viewport.contentHeight - viewport.height, "Wheel scrolling stops at the lower bound");
                scroll(viewport, window, "up");
                scroll(viewport, window, "up");
                tryCompare(viewport, "moving", false, 2000);
                verify(viewport.contentY >= 0, "Wheel scrolling stops at the upper bound");
                resize(window, 384, 360);
                checkContentFits(viewport);
                click(first, window, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.hostItem, window.contentItem);
                compare(widgetMenu.widgetId, "tray");
                verify(barOverflow.visible, "The detached More parent stays open for widget actions");
                widgetMenu.visible = false;
                tryCompare(widgetMenu, "retained", false, 3000);
                metricsPill.forceActiveFocus(Qt.TabFocusReason);
                click(metricsPill, window);
                tryCompare(metricsPopup, "visible", true, 3000);
                compare(metricsPopup.hostItem, window.contentItem);
                verify(barOverflow.visible, "The shell retains More for a child in its floating host");
                metricsPopup.visible = false;
                tryCompare(metricsPopup, "retained", false, 3000);
                click(metricsPill, window, Qt.RightButton);
                tryCompare(metricsPopup, "visible", true, 3000);
                verify(metricsPopup.customising);
                verify(!findChild(terminalSidebar.contentItem, "notesContextMenu")?.visible, "Configuring monitors does not open the Notes context menu");
                tryCompare(metricsPopup, "revealScale", 1, 3000);
                verify(metricsPopup.body.parent.z > barOverflow.body.parent.z, "The nested card is above its parent");
                let frame = null;
                verify(metricsPopup.body.parent.grabToImage(result => frame = result));
                tryVerify(() => frame !== null, 3000);
                verify(!findChild(metricsPill, "barTooltip").shown, "The monitor tooltip does not cover its popup");
                const toggle = findChild(metricsPopup.body, "monitorOption_cpuSwitch");
                verify(toggle.enabled, "The monitor switch is available");
                click(toggle, window);
                tryVerify(() => !metricsPill.isShown("cpu"), 2000);
                click(toggle, window);
                tryVerify(() => metricsPill.isShown("cpu"), 2000);
                capture("nested-monitors");
                metricsPopup.visible = false;
                tryCompare(metricsPopup, "retained", false, 3000);
                verify(!root.anchoredInPopup(calendarPopup, barOverflow), "A sibling in the same window is not a popup child");
                // Outside clicks dismiss More before reaching the sidebar.
                click(clockPill, window);
                tryCompare(barOverflow, "visible", false, 3000);
                tryCompare(barOverflow, "retained", false, 3000);
                click(clockPill, window);
                tryCompare(calendarPopup, "visible", true, 3000);
                verify(!barOverflow.visible, "An unrelated widget in the same window closes More");
                compare(calendarPopup.hostItem, window.contentItem);
                calendarPopup.visible = false;
                tryCompare(calendarPopup, "retained", false, 3000);
                const notesEditor = findChild(terminalSidebar.contentItem, "notesEditor");
                click(notesEditor, window, Qt.RightButton);
                const notesMenu = findChild(terminalSidebar.contentItem, "notesContextMenu");
                verify(notesMenu !== null);
                tryCompare(notesMenu, "visible", true, 3000, "Notes still opens its own context menu");
                notesMenu.visible = false;
                tryCompare(notesMenu, "retained", false, 3000);
                editor.open();
                tryCompare(window, "visible", false, 3000);
                tryCompare(controlCentre, "revealScale", 1, 3000);
                tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                openMore();
                compare(barOverflow.hostItem, null, "Editing returns the real container to its layer window");
                const surface = DesktopEditing.surfaces.find(item => item.zoneName === "overflow");
                verify(surface.screenRect.width > 0 && surface.screenRect.height > 0);
                editor.cancel();
                tryCompare(editor, "visible", false, 3000);
                tryCompare(window, "visible", true, 3000);
                tryCompare(barOverflow, "retained", false, 3000);
                openMore();
                compare(barOverflow.hostItem, window.contentItem);
                capture("restored");
                barOverflow.visible = false;
                terminalSidebar.dockBack();
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
