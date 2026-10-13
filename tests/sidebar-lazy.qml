import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "DesktopLayout.js" as DesktopLayout

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }
    QtObject {
        id: preferences
        property bool sidebarEnabled: true
        property bool dockEnabled: false
    }
    FloatingWindow {
        id: widgetWindow
        visible: Quickshell.env("BINGUX_SIDEBAR_PLACEMENT") !== ""
        implicitWidth: 500
        implicitHeight: 500
    }
    QtObject {
        id: layoutHost
        property var defaultControls: []
        function windowFor(widget) {
            return widgetWindow;
        }
        function hostFor(widget) {
            return widgetWindow.contentItem;
        }
        function controlColumn(widget) {
            return 0;
        }
        function controlRow(widget) {
            return 0;
        }
    }
    TerminalSidebar {
        id: sidebar
        settings: preferences
        screen: Quickshell.screens[0]
        inputSuspended: true
        widgetLayout: Quickshell.env("BINGUX_SIDEBAR_PLACEMENT") ? layoutHost : null
    }
    TestCase {
        parent: sidebar.detachedSurface.contentItem
        when: true
        property string stage: "cold"
        property bool passed: false
        readonly property var panels: ["notes", "monitor", "calendar", "media", "tasks"]
        function check(value, message) {
            if (!value)
                console.error("SIDEBAR_LAZY_FAILED", stage, message);
            verify(value, message);
        }
        function test_lifetime() {
            wait(150);
            const restored = Quickshell.env("BINGUX_SIDEBAR_RESTORE") === "1";
            const selected = Quickshell.env("BINGUX_SIDEBAR_SELECTION") || "terminal";
            compare(sidebar.contentType, selected);
            compare(sidebar.opened, restored);
            for (const id of panels) {
                if (restored && id === selected) {
                    check(sidebar.panelFor(id) !== null, "Saved open selection loads");
                    compare(DesktopEditing.sources[id], sidebar.panelFor(id));
                } else {
                    check(sidebar.panelFor(id) === null, "Unused " + id + " content is absent");
                    check(!DesktopEditing.sources[id], "Unused source is unregistered");
                }
            }
            const placement = Quickshell.env("BINGUX_SIDEBAR_PLACEMENT");
            if (placement) {
                stage = "first-placement";
                tryCompare(BinguxPreferences, "loaded", true);
                for (const id of ["notes", "monitor"]) {
                    const layout = DesktopLayout.defaults();
                    layout.sidebar = layout.sidebar.filter(value => value !== id);
                    layout[placement] = (layout[placement] || []).concat([id]);
                    BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {
                        desktop: Object.assign({}, BinguxPreferences.data.desktop, {
                            layout
                        })
                    });
                    const widget = sidebar.panelWidget(id);
                    compare(widget.sidebarMode, false);
                    if (placement !== "control-centre") {
                        compare(sidebar.panelFor(id), null);
                        mouseClick(widget, widget.width / 2, widget.height / 2);
                        tryCompare(widget.popup, "visible", true);
                    }
                    tryVerify(() => sidebar.panelFor(id) !== null);
                    const item = sidebar.panelFor(id);
                    compare(DesktopEditing.sources[id], item);
                    verify(waitForRendering(item, 2000), "First placement renders");
                    if (id === "notes") {
                        compare(item.menuHost, placement === "control-centre" ? widgetWindow.contentItem : widget.popup.contentItem);
                    }
                    widget.popup.visible = false;
                }
                passed = true;
                return;
            }
            if (!restored) {
                stage = "closed-selection";
                for (const id of panels) {
                    sidebar.selectContent(id);
                    wait(20);
                    compare(sidebar.panelFor(id), null);
                }
                stage = "detach-before-notes";
                sidebar.selectContent("monitor");
                sidebar.popOut();
                tryVerify(() => sidebar.detachedSurface.visible);
                check(sidebar.panelFor("notes") === null, "Detaching does not load unused notes");
            } else {
                sidebar.popOut();
            }
            stage = "first-open";
            const retained = {};
            for (const id of panels) {
                const started = Date.now();
                sidebar.selectContent(id);
                tryVerify(() => sidebar.panelFor(id) !== null);
                retained[id] = sidebar.panelFor(id);
                compare(DesktopEditing.sources[id], retained[id]);
                check(retained[id].width > 0 && retained[id].height > 0, "First open has geometry");
                verify(waitForRendering(retained[id], 2000), "First open renders content");
                console.warn("SIDEBAR_FIRST_RENDER", id, Date.now() - started);
            }
            const metrics = {
                available: false,
                latest: null,
                history: [],
                formatBytes: () => "—",
                formatRate: () => "—"
            };
            sidebar.systemMetrics = metrics;
            compare(retained.monitor.metrics, metrics);
            sidebar.systemMetrics = null;
            compare(retained.monitor.metrics, null);
            stage = "notes-input";
            sidebar.selectContent("notes");
            const notes = findChild(sidebar.contentItem, "notesEditor");
            check(notes !== null, "First open creates real editor");
            notes.text = "Retained note";
            sidebar.focusContent();
            tryVerify(() => notes.activeFocus);
            notes.cursorPosition = notes.length;
            keyClick(Qt.Key_Exclam);
            check(notes.getText(0, notes.length).endsWith("!"), "Real keyboard input reaches notes");
            const text = notes.getText(0, notes.length);
            stage = "hide-reopen-dock";
            sidebar.hide();
            wait(250);
            sidebar.open();
            sidebar.dockBack();
            wait(250);
            sidebar.popOut();
            wait(100);
            compare(findChild(sidebar.contentItem, "notesEditor"), notes);
            compare(notes.getText(0, notes.length), text);
            for (const id of panels)
                compare(sidebar.panelFor(id), retained[id]);
            compare(retained.notes.menuWindow, sidebar.widgetWindow);
            compare(retained.notes.menuHost, sidebar.detachedSurface.contentItem);
            compare(retained.notes.screen, sidebar.detachedSurface.screen);
            sidebar.hide();
            passed = true;
            console.warn("SIDEBAR_LAZY_PASSED");
        }
        function cleanupTestCase() {
            report.setText("FAILURES " + qtest_results.failCount + "\nSTAGE " + stage + "\nPASSED " + passed);
            Qt.quit();
        }
    }
}
