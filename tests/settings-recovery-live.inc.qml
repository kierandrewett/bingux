    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    FileView { id: savedSettings; path: BinguxPreferences.path; watchChanges: true; printErrors: false; onFileChanged: reload() }
    Process {
        id: permissions
        property int resultCode: -1
        onExited: code => resultCode = code
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function writable(value) {
            permissions.command = ["chmod", value ? "700" : "500", BinguxPreferences.path.slice(0, BinguxPreferences.path.lastIndexOf("/"))];
            permissions.resultCode = -1; permissions.running = true;
            tryCompare(permissions, "running", false, 2000); compare(permissions.resultCode, 0);
        }
        function test_recovery() {
            let phase = "import";
            let originalText = "";
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                savedSettings.reload();
                tryVerify(() => savedSettings.text().length > 0, 2000);
                originalText = savedSettings.text();
                const originalData = JSON.stringify(BinguxPreferences.data);
                const mutations = [
                    value => { delete value.desktop.layout.dock; },
                    value => { value.desktop.layout.dock.push("clock"); },
                    value => { value.desktop.layout.sidebar = ["unknown"]; },
                    value => { value.desktop.layoutVersion = true; },
                    value => { value.desktop.dockApps = {pinnedApps: [null], order: []}; },
                    value => { value.desktop.widgetOptions = {clock: {label: 42}}; },
                    value => { value.desktop.containers = {dock: {display: "invalid"}}; },
                    value => { value.desktop.dockSize = "large"; }
                ];
                for (let index = 0; index <= mutations.length; index++) {
                    phase = "invalid file " + index;
                    const value = JSON.parse(originalText);
                    if (index < mutations.length) mutations[index](value);
                    savedSettings.setText(index < mutations.length ? JSON.stringify(value) : "{");
                    tryVerify(() => BinguxPreferences.layoutError.length > 0, 3000);
                    compare(JSON.stringify(BinguxPreferences.data), originalData, "A rejected file keeps the last valid runtime state");
                    phase = "restore valid file " + index;
                    savedSettings.setText(originalText);
                    tryCompare(BinguxPreferences, "layoutError", "", 3000);
                    compare(JSON.stringify(BinguxPreferences.data), originalData);
                }
                phase = "failed save";
                desktopCustomiser.open(); desktopCustomiser.put("clock", "dock", 0);
                writable(false);
                desktopCustomiser.apply();
                tryCompare(binguxSettings, "busy", false, 4000);
                verify(desktopCustomiser.visible && binguxSettings.dirty);
                verify(binguxSettings.status !== "" && binguxSettings.status !== "Saved");
                compare(savedSettings.text(), originalText);
                desktopCustomiser.cancel();
                compare(JSON.stringify(BinguxPreferences.data), originalData);
                writable(true);
                phase = "retry save";
                desktopCustomiser.open(); desktopCustomiser.put("clock", "dock", 0); desktopCustomiser.apply();
                tryCompare(desktopCustomiser, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                tryVerify(() => JSON.parse(savedSettings.text()).desktop.layout.dock.includes("clock"), 3000);
                compare(clockPill.parent, dock.widgetHost);
                layoutReport.setText("PASS");
            } catch (error) {
                writable(true);
                if (originalText) savedSettings.setText(originalText);
                console.error("CUSTOMISE_TEST_FAILED", phase, error.message, error.stack);
                layoutReport.setText("FAIL " + phase + " " + error.stack);
            }
        }
    }
