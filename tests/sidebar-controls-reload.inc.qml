    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_sidebar_controls() {
            try {
                for (const id of ["controls-header", "controls-audio", "controls-tiles", "control-media", "control-divider", "control-customise"]) {
                    const item = controlCentre.movableWidgets.find(item => item.widgetId === id);
                    tryCompare(item, "parent", terminalSidebar.widgetHost, 2000);
                    verify(!item.barLayout);
                    verify(BinguxPreferences.data.desktop.layout.sidebar.includes(id));
                    verify(!BinguxPreferences.data.desktop.controlLayout.groups["control-centre"].includes(id));
                }
                const settings = controlCentre.movableWidgets.find(item => item.widgetId === "control-settings");
                compare(settings.presentation.label, "Prefs");
                verify(settings.presentation.showText && settings.presentation.showIcon);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
