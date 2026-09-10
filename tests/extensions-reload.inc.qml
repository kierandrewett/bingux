    FileView { id: extensionReloadReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_extension() {
            try {
                tryVerify(() => topBar.extensionWidgets.length === 1, 4000);
                const widget = topBar.extensionWidgets[0];
                compare(widget.widgetId, "extension:org.bingux.example/counter");
                tryCompare(widget, "parent", controlCentre.widgetHost, 3000);
                tryVerify(() => !!widget.children[0].item?.button, 3000);
                compare(ExtensionRegistry.invoke("org.bingux.example/increment"), 1);
                extensionReloadReport.setText("PASS");
            } catch (error) { extensionReloadReport.setText("FAIL: " + error); throw error; }
        }
    }
