    FileView {
        id: report
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_tray() {
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
                            hasMenu: false,
                            onlyMenu: false
                        }));
                tryCompare(trayContainer, "parent", controlCentre.widgetHost, 3000);
                verify(trayContainer.panelLayout);
                verify(tray.panelLayout);
                controlCentre.visible = true;
                tryVerify(() => trayContainer.width <= controlCentre.widgetHost.width, 3000);
                verify(trayContainer.height > Theme.barHeight);
                report.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                report.setText("FAIL " + error.stack);
            }
        }
    }
