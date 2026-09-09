import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    BinguxSettings {
        id: settings
        visible: true
        property string requestedContainer: ""
        function requestShellCustomise(container = "") { requestedContainer = container; }
    }
    TestCase {
        parent: settings.contentItem
        when: settings.visible && settings.ready && !settings.busy
        function initTestCase() { report.setText("RUNNING\n"); }
        function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount); }
        function test_container_links() {
            const original = JSON.stringify(settings.draft);
            for (const entry of [{page: "TopBar", id: "top-right"}, {page: "Dock", id: "dock"},
                {page: "Sidebar", id: "sidebar"}, {page: "Controls", id: "control-centre"}]) {
                settings.page = entry.page;
                settings.requestedContainer = "";
                tryVerify(() => !!findChild(settings.contentItem, "customise-container-" + entry.id), 1500);
                const button = findChild(settings.contentItem, "customise-container-" + entry.id);
                verify(waitForRendering(button, 2000));
                mouseClick(button, 90, button.height / 2);
                compare(settings.requestedContainer, entry.id);
                compare(JSON.stringify(settings.draft), original);
                verify(!settings.dirty, "A layout link does not change Settings");
            }
            settings.requestedContainer = "";
            settings.openContainerCustomise("unknown-container");
            compare(settings.requestedContainer, "", "Invalid container routes are ignored");
        }
    }
}
