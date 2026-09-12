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
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, options) {
            tryVerify(() => item.width > 0 && item.height > 0, 3000);
            let frame = null;
            verify(item.grabToImage(result => frame = result));
            tryVerify(() => frame !== null, 4000);
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0);
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
        }
        property var calls: []
        function test_moved_tray() {
            try {
                tray.serviceEnabled = false;
                tray.trayItems = Array.from({
                    length: 18
                }, (_, index) => ({
                            id: "sample-" + index,
                            title: "Application " + index,
                            tooltipTitle: "Application " + index,
                            icon: Quickshell.iconPath("applications-other"),
                            menu: null,
                            hasMenu: true,
                            onlyMenu: index === 17,
                            activate: () => calls = calls.concat(["activate:" + index]),
                            secondaryActivate: () => calls = calls.concat(["secondary:" + index]),
                            scroll: (delta, horizontal) => calls = calls.concat(["scroll:" + index + ":" + delta])
                        }));
                tryVerify(() => findChild(tray, "trayItem-sample-17") !== null);
                const first = findChild(tray, "trayItem-sample-0");
                const last = findChild(tray, "trayItem-sample-17");
                const menu = findChild(last, "trayItemMenu");
                menu.actions = [
                    {
                        text: "Open application",
                        enabled: true,
                        isSeparator: false,
                        hasChildren: false,
                        checkState: Qt.Unchecked,
                        icon: "",
                        triggered: () => calls = calls.concat(["menu:17"])
                    }
                ];
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                for (const container of ["sidebar", "control-centre"]) {
                    desktopCustomiser.open();
                    tryCompare(binguxSettings, "busy", false, 4000);
                    tryCompare(controlCentre, "revealScale", 1, 3000);
                    if (container === "sidebar") {
                        desktopCustomiser.put("tray", "control-centre", 0);
                        tryCompare(trayContainer, "parent", controlCentre.widgetHost);
                        tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                        const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar");
                        gesture(first, topBar.windowFor(trayContainer), ["--drag-to", String(area.screenRect.x + area.screenRect.width / 2), String(area.screenRect.y + Theme.barHeight + 12)]);
                        tryCompare(desktopCustomiser, "draggedId", "", 4000);
                        compare(desktopCustomiser.containerFor("tray"), "sidebar", "Native drag places the original tray");
                        desktopCustomiser.undo();
                        compare(desktopCustomiser.containerFor("tray"), "control-centre");
                        desktopCustomiser.redo();
                        compare(desktopCustomiser.containerFor("tray"), "sidebar");
                    } else
                        desktopCustomiser.put("tray", container, 0);
                    desktopCustomiser.selectedContainer = container;
                    desktopCustomiser.containerDisplay("native");
                    save();
                    if (container === "sidebar") {
                        terminalSidebar.open();
                        tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                    } else {
                        terminalSidebar.hide();
                        tryCompare(terminalSidebar.editWindow, "reveal", 0, 3000);
                        controlCentre.visible = true;
                        tryCompare(controlCentre, "revealScale", 1, 3000);
                    }
                    const host = container === "sidebar" ? terminalSidebar.widgetHost : controlCentre.widgetHost;
                    const window = topBar.windowFor(trayContainer);
                    verify(trayContainer.parent === host);
                    verify(tray.panelLayout);
                    tryVerify(() => trayContainer.width <= host.width && tray.height > Theme.barHeight, 1000);
                    compare(findChild(tray, "trayItem-sample-17"), last, "Moving retains app buttons and menus");
                    for (let index = 0; index < 18; index++) {
                        const item = findChild(tray, "trayItem-sample-" + index);
                        tryVerify(() => {
                            const p = item.mapToItem(trayContainer, item.width, item.height);
                            return p.x <= trayContainer.width + .01 && p.y <= trayContainer.height + .01;
                        }, 1000);
                    }
                    nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-" + container + ".png" : "";
                    gesture(first, window, ["--click-only"]);
                    nativeInput.capturePath = "";
                    compare(calls[calls.length - 1], "activate:0");
                    for (const options of [["--right-click"], ["--click-only"]]) {
                        gesture(last, window, options);
                        tryCompare(menu, "visible", true, 3000);
                        tryCompare(menu, "revealScale", 1, 3000);
                        verify(menu.anchorWindow === window);
                        const action = findChild(menu.body, "trayMenuEntry0");
                        verify(action !== null);
                        gesture(action, menu.nativeWindow, ["--click-only"]);
                        tryCompare(menu, "visible", false, 3000);
                        compare(calls[calls.length - 1], "menu:17");
                    }
                    gesture(first, window, ["--shift-right-click"]);
                    tryCompare(widgetMenu, "visible", true, 3000);
                    compare(widgetMenu.widgetId, "tray");
                    widgetMenu.visible = false;
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
