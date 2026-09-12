    FileView {
        id: extensionReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    Component {
        id: extensionPreview
        WidgetPreview {
            width: 240
            height: 100
            widgetId: "extension:org.bingux.example/counter"
        }
    }
    TestCase {
        id: extensionTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_extension_containers() {
            try {
                const id = "extension:org.bingux.example/counter";
                tryVerify(() => !!ExtensionRegistry.widget(id), 5000);
                tryVerify(() => !!ExtensionRegistry.actions["org.bingux.example/increment"], 3000);
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => BinguxPreferences.data.desktop.layoutVersion === 1, 4000);
                console.log("EXTENSION_STAGE: containers");
                for (const zone of ["top-right", "dock", "sidebar", "top-left", "top-center"]) {
                    console.log("EXTENSION_ZONE: " + zone);
                    const next = DesktopLayout.move(topBar.snapshotLayout(), id, zone, 0);
                    BinguxPreferences.saveLayout(next);
                    tryVerify(() => topBar.extensionWidgets.length === 1, 3000);
                    const widget = topBar.extensionWidgets[0];
                    tryCompare(widget, "parent", topBar.hostFor(widget), 3000);
                    compare(DesktopLayout.placement(DesktopEditing.desktop, id), zone);
                    tryVerify(() => !!widget.children[0].item?.button, 3000);
                    const counter = widget.children[0].item;
                    counter.button.clicked();
                    tryVerify(() => counter.popup && counter.popup.visible, 3000);
                    compare(counter.popup.anchorWindow, topBar.windowFor(widget));
                    compare(counter.popup.anchorItem, widget);
                    counter.popup.visible = false;
                }
                console.log("EXTENSION_STAGE: preview");
                const widget = topBar.extensionWidgets[0];
                binguxSettings.openCustomise("", "");
                tryVerify(() => DesktopEditing.active, 3000);
                compare(widget.enabled, false);
                const preview = extensionPreview.createObject(topBar.contentItem);
                tryVerify(() => !!preview.previewControl?.children[0].item, 3000);
                compare(preview.previewControl.context.preview, true);
                preview.destroy();
                binguxSettings.customiser.cancel();
                console.log("EXTENSION_STAGE: disable");
                ExtensionRegistry.setEnabled("org.bingux.example", false);
                tryVerify(() => !ExtensionRegistry.widget(id), 4000);
                tryVerify(() => !ExtensionRegistry.actions["org.bingux.example/increment"], 3000);
                compare(topBar.extensionWidgets.length, 1);
                compare(DesktopLayout.placement(DesktopEditing.desktop, id), "top-center");
                ExtensionRegistry.setEnabled("org.bingux.example", true);
                tryVerify(() => !!ExtensionRegistry.widget(id), 4000);
                tryVerify(() => !!ExtensionRegistry.actions["org.bingux.example/increment"], 3000);
                console.log("EXTENSION_STAGE: reload");
                const generation = ExtensionRegistry.generation;
                ExtensionRegistry.reload();
                tryVerify(() => ExtensionRegistry.generation > generation, 4000);
                wait(100);
                const value = ExtensionRegistry.invoke("org.bingux.example/increment");
                console.log("EXTENSION_ACTION_VALUE: " + value);
                compare(value, 1);
                console.log("EXTENSION_STAGE: control-centre");
                const next = DesktopLayout.move(topBar.snapshotLayout(), id, "palette", 0);
                const groups = ControlLayout.move(BinguxPreferences.data.desktop.controlLayout, "control-centre", id, 0);
                BinguxPreferences.saveDesktop({
                    layout: next,
                    controlLayout: groups
                });
                tryCompare(topBar.extensionWidgets[0], "parent", controlCentre.widgetHost, 3000);
                controlCentre.visible = true;
                const counter = topBar.extensionWidgets[0].children[0].item;
                counter.button.clicked();
                tryVerify(() => !!counter.popup?.visible, 3000);
                compare(counter.popup.anchorWindow, topBar.windowFor(topBar.extensionWidgets[0]));
                counter.popup.visible = false;
                console.log("EXTENSION_STAGE: settings");
                binguxSettings.page = "Extensions";
                binguxSettings.visible = true;
                wait(200);
                compare(binguxSettings.pageTitle, "Extensions");
                const extensionSettings = findChild(binguxSettings.contentItem, "extensionSettings");
                verify(!!extensionSettings);
                extensionSettings.selectedId = "org.bingux.example";
                const settingsContent = findChild(extensionSettings, "extensionSettingsContent");
                tryVerify(() => !!settingsContent.item, 3000);
                compare(settingsContent.item.context.extensionId, "org.bingux.example");
                extensionReport.setText("PASS");
            } catch (error) {
                extensionReport.setText("FAIL: " + error);
                throw error;
            }
        }
    }
