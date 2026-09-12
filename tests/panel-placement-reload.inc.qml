    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_panels() {
            try {
                const destinations = {
                    notes: "dock",
                    terminal: "top-right",
                    calendar: "control-centre",
                    tasks: "top-left",
                    media: "dock",
                    monitor: "control-centre"
                };
                for (const id of Object.keys(destinations)) {
                    const widget = terminalSidebar.panelWidget(id);
                    tryCompare(widget, "container", destinations[id], 4000);
                    compare(widget.parent, topBar.hostFor(widget));
                    verify(widget.width > 0 && widget.height > 0);
                }
                compare(terminalSidebar.contentType, "");
                compare(terminalSidebar.activePanel, null);
                controlCentre.visible = true;
                tryCompare(controlCentre, "revealScale", 1, 3000);
                for (const id of ["calendar", "monitor"]) {
                    tryVerify(() => terminalSidebar.panelFor(id) !== null, 3000);
                    verify(terminalSidebar.panelFor(id).visible);
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
