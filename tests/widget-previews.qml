import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    QtObject {
        id: editor
        property var nativeWindow: window
        property bool visible: true
        property var desktop: ({})
        property var layout: ({})
    }
    PanelWindow {
        id: window
        implicitWidth: 700; implicitHeight: 400
        color: Theme.background
        ActionButton { id: before; x: 30; y: 100; text: "Before preview" }
        Item {
            id: previewHost
            x: 200; y: 100; width: 320; height: 240
            Rectangle { anchors.fill: parent; color: Theme.background }
        }
        ActionButton { x: 550; y: 100; text: "After preview" }
        Component { id: previewComponent; WidgetPreview { anchors.fill: parent } }
        TestCase {
            when: window.visible
            function initTestCase() { report.setText("RUNNING\n"); DesktopEditing.editor = editor; verify(waitForRendering(before)); }
            function cleanupTestCase() { DesktopEditing.editor = null; report.setText("FAILURES " + qtest_results.failCount); }
            function capture(name) {
                const prefix = Quickshell.env("BINGUX_PREVIEW_CAPTURE");
                if (!prefix) return;
                let saved = false;
                verify(previewHost.grabToImage(result => { saved = result.saveToFile(prefix + "-" + name + ".png"); }));
                tryVerify(() => saved, 2000);
            }
            function test_individual_presentation_data() {
                const rows = [];
                const ids = ControlLayout.widgets.filter(widget => widget.action).map(widget => widget.id)
                    .concat(["control-volume", "control-microphone", "control-battery", "control-media"])
                    .concat(DesktopLayout.controlOrder().map(name => "control-" + name));
                for (const id of ids) {
                    const group = ControlLayout.groupFor(id) || "controls-tiles";
                    for (const container of ["dock", "sidebar", "control-centre"]) {
                        rows.push({tag: id + "-" + container, id, group, container, grouped: false});
                        if (group !== "control-centre") rows.push({tag: id + "-group-" + container, id, group, container, grouped: true});
                    }
                }
                return rows;
            }
            function test_individual_presentation(data) {
                const target = data.grouped ? data.group : data.id;
                const containers = {[data.container]: {display: "text"}};
                editor.desktop = {controlLayout: ControlLayout.defaults(), containers, widgetOptions: {}};
                editor.layout = DesktopLayout.defaults();
                if (data.container !== "control-centre") editor.layout = Object.assign({}, editor.layout, {[data.container]: [target]});
                const preview = previewComponent.createObject(previewHost, {widgetId: data.id});
                try {
                    tryVerify(() => !!preview.previewControl, 1500);
                    const item = preview.previewControl;
                    compare(preview.container, data.container, "A child follows its placed group");
                    compare(ControlLayout.isAction(data.id) ? item.barStyle : item.barLayout, data.container === "dock",
                        "Sidebar controls keep their full panel layout");
                    verify(!!item.presentation, "The preview receives the native presentation settings");
                    compare(item.presentation.mode, "text");
                    verify(item.presentation.showText && !item.presentation.showIcon);
                    if (data.grouped) {
                        editor.desktop = Object.assign({}, editor.desktop, {containers: Object.assign({}, containers, {[data.group]: {display: "icons"}})});
                        compare(item.presentation.mode, "icons", "Group display overrides its host container");
                    }
                    editor.desktop = Object.assign({}, editor.desktop, {widgetOptions: {[data.id]: {display: "both", label: "My widget", icon: "starred-symbolic"}}});
                    verify(item.presentation.showText && item.presentation.showIcon);
                    compare(item.presentation.label, "My widget");
                    compare(item.presentation.icon, "starred-symbolic");
                    verify(waitForRendering(preview, 2000));
                    if (data.container === "dock") {
                        compare(item.width, item.implicitWidth, "A compact preview uses its native width");
                        compare(item.height, item.implicitHeight);
                    }
                    verify(preview.visualItem.width <= preview.width && preview.visualItem.height <= preview.height);
                    if (["control-media", "control-network", "control-volume", "control-settings"].includes(data.id) && !data.grouped)
                        capture(data.tag);
                } finally {
                    preview.destroy(); wait(50);
                    editor.desktop = {}; editor.layout = {};
                }
            }
            function test_shared_group_previews() {
                const originalSources = Object.assign({}, DesktopEditing.sources);
                editor.desktop = {controlLayout: ControlLayout.defaults(),
                    controlCentre: {vpn: true, dnd: true, nightLight: true, power: true, awake: true},
                    containers: {}, widgetOptions: {}};
                editor.layout = DesktopLayout.defaults();
                let preview = null;
                try {
                    for (const id of ["controls-header", "controls-audio", "controls-tiles"]) {
                        before.forceActiveFocus();
                        preview = previewComponent.createObject(previewHost, {widgetId: id});
                        tryVerify(() => !!preview.previewControl, 1500);
                        verify(before.activeFocus, "Creating a group preview does not steal focus");
                        for (let index = 0; index < 12; index++) {
                            keyClick(Qt.Key_Tab);
                            for (let item = window.contentItem.Window.window.activeFocusItem; item; item = item.parent)
                                verify(item !== preview, "Tab does not enter sample controls");
                        }
                        const view = preview.previewControl;
                        verify(!!view.movableWidgets, "The palette uses the complete native control-centre view");
                        const group = view.movableWidgets.find(item => item.widgetId === id);
                        const members = () => group.memberEntries.filter(entry => entry.item.visible).map(entry => entry.id);
                        const hasSession = view.movableWidgets.some(item => item.widgetId === "control-session");
                        compare(members().length, id === "controls-header" ? (hasSession ? 6 : 5) : id === "controls-audio" ? 2 : 7);
                        verify(waitForRendering(preview, 2000)); capture(id);
                        verify(preview.visualItem.width <= preview.width && preview.visualItem.height <= preview.height,
                            "The complete group fits inside the preview");
                        if (id === "controls-header") {
                            const controls = JSON.parse(JSON.stringify(editor.desktop.controlLayout));
                            controls.groups[id] = ["control-lock", "control-account", "control-settings"];
                            editor.desktop = Object.assign({}, editor.desktop, {controlLayout: controls});
                            compare(members().length, 3);
                            const lock = view.movableWidgets.find(item => item.widgetId === "control-lock");
                            const account = view.movableWidgets.find(item => item.widgetId === "control-account");
                            compare(lock.Layout.column, 0); compare(account.Layout.column, 1);
                            editor.layout = Object.assign({}, editor.layout, {dock: [id]});
                            editor.desktop = Object.assign({}, editor.desktop, {containers: {dock: {display: "text"}}});
                            verify(group.barLayout);
                            verify(lock.presentation.showText && !lock.presentation.showIcon);
                            editor.desktop = Object.assign({}, editor.desktop, {widgetOptions: {"control-lock": {display: "both", label: "Lock screen"}}});
                            verify(lock.presentation.showText && lock.presentation.showIcon);
                            compare(lock.presentation.label, "Lock screen");
                            verify(waitForRendering(preview, 2000)); capture("header-dock-labels");
                            editor.layout = DesktopLayout.defaults();
                        }
                        preview.destroy(); preview = null; wait(50);
                        for (const key of Object.keys(originalSources)) compare(DesktopEditing.sources[key], originalSources[key]);
                        compare(Object.keys(DesktopEditing.sources).length, Object.keys(originalSources).length,
                            "Sample widgets do not replace live drag sources");
                    }
                } finally {
                    if (preview) preview.destroy();
                    editor.desktop = {}; editor.layout = {};
                }
            }
        }
    }
}
