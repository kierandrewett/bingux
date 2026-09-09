    FileView { id: externalReloadReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_external_widgets() {
            try {
                for (const id of ["search", "clock", "label:1", "icon:1"]) {
                    const item = topBar.defaultControls.find(item => topBar.nameFor(item) === id);
                    tryVerify(() => item && item.parent === controlCentre.widgetHost, 2000);
                    verify(BinguxPreferences.data.desktop.controlLayout.groups["control-centre"].includes(id));
                    verify(!Object.values(BinguxPreferences.data.desktop.layout).some(values => values.includes(id)));
                }
                const label = topBar.decorationWidgets.find(item => item.widgetId === "label:1");
                compare(label.presentation.label, "My controls");
                externalReloadReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                externalReloadReport.setText("FAIL " + error.stack);
            }
        }
    }
