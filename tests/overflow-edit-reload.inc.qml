    FileView {
        id: report
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_overflow() {
            try {
                tray.serviceEnabled = false;
                metricsPill.previewMonitors = metricsPill.monitorNames;
                tray.trayItems = Array.from({
                    length: 18
                }, (_, index) => ({
                            id: "overflow-" + index,
                            title: "Application " + index,
                            tooltipTitle: "Application " + index,
                            icon: Quickshell.iconPath("applications-other"),
                            menu: null,
                            hasMenu: false,
                            onlyMenu: false
                        }));
                tryCompare(overflowButton, "parent", dock.widgetHost, 3000);
                tryVerify(() => topBar.overflows(trayContainer), 3000);
                overflowButton.clicked();
                tryCompare(barOverflow, "visible", true, 3000);
                tryCompare(barOverflow, "revealScale", 1, 3000);
                compare(topBar.windowFor(trayContainer), barOverflow.nativeWindow);
                compare(barOverflow.anchorWindow, dock);
                tryVerify(() => Math.abs(barOverflow.panelY + barOverflow.body.parent.height - (dock.popupAnchorTop - Theme.gap)) < 1, 3000);
                report.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                report.setText("FAIL " + error.stack);
            }
        }
    }
