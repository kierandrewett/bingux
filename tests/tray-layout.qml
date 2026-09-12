import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import "ControlLayout.js" as ControlLayout

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    QtObject {
        id: editor
        property bool visible: true
        property var nativeWindow: window
        property var desktop: ({
                controlLayout: ControlLayout.defaults()
            })
        property var layout: ({
                sidebar: ["tray"]
            })
    }
    FloatingWindow {
        id: window
        implicitWidth: 900
        implicitHeight: 760
        color: Theme.background
        Item {
            id: panel
            width: 208
            height: 720
        }
        Pill {
            id: wrapper
            parent: panel
            horizontalPadding: 0
            Tray {
                id: tray
                parentWindow: window
                serviceEnabled: false
            }
        }
        WidgetPreview {
            id: preview
            x: 420
            width: 320
            height: 240
            widgetId: "tray"
        }
        TestCase {
            when: window.visible
            property var calls: []
            function buttons(item) {
                if (item.modelData?.id?.startsWith("sample-"))
                    return [item];
                return (item.children || []).reduce((result, child) => result.concat(buttons(child)), []);
            }
            function initTestCase() {
                report.setText("RUNNING");
                DesktopEditing.editor = editor;
                DesktopEditing.registerSource("tray", wrapper);
            }
            function cleanupTestCase() {
                DesktopEditing.unregisterSource("tray", wrapper);
                DesktopEditing.editor = null;
                report.setText("FAILURES " + qtest_results.failCount);
            }
            function init() {
                wrapper.panelLayout = false;
            }
            function test_preview_uses_panel_layout() {
                wrapper.panelLayout = true;
                tryVerify(() => preview.previewControl !== null, 3000);
                verify(preview.previewControl.panelLayout);
                tryCompare(preview.previewControl, "width", wrapper.width);
                compare(preview.previewControl.trayItems.length, 3);
                verify(!preview.previewControl.enabled);
                wrapper.panelLayout = false;
            }
            function test_wrapping_and_actions() {
                try {
                    tray.trayItems = Array.from({
                        length: 18
                    }, (_, index) => ({
                                id: "sample-" + index,
                                title: "Application " + index,
                                tooltipTitle: "Application " + index,
                                icon: Quickshell.iconPath("applications-other"),
                                menu: null,
                                hasMenu: false,
                                onlyMenu: false,
                                activate: () => calls = calls.concat(["activate:" + index]),
                                secondaryActivate: () => calls = calls.concat(["secondary:" + index]),
                                scroll: (delta, horizontal) => calls = calls.concat(["scroll:" + index + ":" + delta])
                            }));
                    tryCompare(tray, "width", tray.maximumVisibleItems * Theme.barIconTarget);
                    tryCompare(wrapper, "width", tray.width);
                    const barWidth = wrapper.width;
                    const barHeight = wrapper.height;
                    wrapper.panelLayout = true;
                    if ("panelLayout" in tray)
                        tray.panelLayout = true;
                    tryVerify(() => tray.width <= panel.width, 1000, "Tray fits the panel width");
                    tryCompare(wrapper, "width", panel.width);
                    tryVerify(() => buttons(tray).length === 18);
                    const originals = buttons(tray);
                    for (const width of [208, 376, 208]) {
                        panel.width = width;
                        tryVerify(() => tray.width <= panel.width);
                        for (const button of originals) {
                            tryVerify(() => {
                                const p = button.mapToItem(tray, 0, 0);
                                return p.x >= 0 && p.y >= 0 && p.x + button.width <= tray.width + .1 && p.y + button.height <= tray.height + .1;
                            }, 1000, button.modelData.id + " stays inside the tray");
                        }
                    }
                    const last = originals[17];
                    mouseClick(last, last.width / 2, last.height / 2);
                    compare(calls[calls.length - 1], "activate:17");
                    mouseClick(last, last.width / 2, last.height / 2, Qt.MiddleButton);
                    compare(calls[calls.length - 1], "secondary:17");
                    mouseWheel(last, last.width / 2, last.height / 2, 0, 120);
                    compare(calls[calls.length - 1], "scroll:17:120");
                    last.forceActiveFocus();
                    keyClick(Qt.Key_Return);
                    compare(calls[calls.length - 1], "activate:17");
                    tray.presentation = {
                        custom: true,
                        showText: true,
                        showIcon: true,
                        labelOverridden: true,
                        label: "A very long application label that must fit this panel",
                        iconOverridden: false
                    };
                    tryVerify(() => findChild(last, "widgetFaceLabel").truncated);
                    verify(last.width <= tray.width);
                    tray.presentation = null;
                    tray.panelLayout = false;
                    wrapper.panelLayout = false;
                    tryCompare(wrapper, "width", barWidth);
                    tryCompare(wrapper, "height", barHeight);
                    compare(buttons(tray)[17], last, "Moving retains tray delegates");
                    last.forceActiveFocus();
                    tryVerify(() => {
                        const p = last.mapToItem(tray, 0, 0);
                        return p.x >= 0 && p.x + last.width <= tray.width + .1;
                    }, 1000, "Focused items scroll into view in the bar");
                    tray.trayItems = [];
                    tryCompare(wrapper, "implicitWidth", 0);
                } catch (error) {
                    console.error("TRAY_FIT_FAILED", error.message, error.stack, "tray", tray.width, tray.height, "host", panel.width);
                    throw error;
                }
            }
        }
    }
}
